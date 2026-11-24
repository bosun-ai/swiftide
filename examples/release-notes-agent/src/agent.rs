//! The release notes agent: its instructions, tools and stop condition.
use std::{path::PathBuf, sync::Arc};

use anyhow::{Context as _, Result, ensure};
use swiftide::{
    agents::{Agent, DefaultContext, tools::control::ApprovalRequired},
    chat_completion::ChatCompletion,
    prompt::Prompt,
    traits::{ToolBox, ToolExecutor},
};

use crate::{
    git::{Git, GitHubRepo, Revision},
    notes::ReleaseNotes,
    progress,
    tools::{OneEntryPerChange, WriteChangelog, git_diff, git_log_since, read_file},
};

/// What to write release notes for.
pub struct ReleaseTask {
    /// The last released tag.
    pub since: Revision,
    pub commit_count: usize,
    /// The version being released; the agent proposes one when absent.
    pub next_version: Option<String>,
    pub notes_file: PathBuf,
    /// Where the repository lives on GitHub, used to link pull requests.
    pub github: Option<GitHubRepo>,
}

const SYSTEM_PROMPT: &str = include_str!("system_prompt.md");

impl ReleaseTask {
    /// Resolves the release range: from `since` (default: the latest tag) up to `HEAD`.
    pub async fn resolve(
        git: Git<'_>,
        since: Option<Revision>,
        next_version: Option<String>,
        notes_file: PathBuf,
    ) -> Result<Self> {
        let since = match since {
            Some(since) => since,
            None => git.latest_tag().await?,
        };
        let commit_count = git
            .commit_count(&since.to_head())
            .await
            .with_context(|| format!("`{since}` is not a tag or commit in this repository"))?;
        ensure!(
            commit_count > 0,
            "no commits since {since}, nothing to release"
        );
        let github = git
            .origin_url()
            .await
            .ok()
            .and_then(|url| GitHubRepo::from_remote_url(&url));

        Ok(Self {
            since,
            commit_count,
            next_version,
            notes_file,
            github,
        })
    }

    // ANCHOR: prompt
    /// The system prompt is a Tera template; optional values are left undefined when absent.
    ///
    /// GitHub tools are only mentioned when the repository is on GitHub, since the prompt tells
    /// the agent which repository to query.
    fn system_prompt(&self, github_tools: bool) -> Prompt {
        let mut prompt = Prompt::from(SYSTEM_PROMPT)
            .with_context_value("since", self.since.as_str())
            .with_context_value("github_tools", github_tools && self.github.is_some());
        if let Some(version) = &self.next_version {
            prompt.insert_context_value("next_version", version.as_str());
        }
        if let Some(github) = &self.github {
            prompt.insert_context_value("github", github.to_string());
        }
        prompt
    }
    // ANCHOR_END: prompt

    /// The first user message.
    pub fn query(&self) -> String {
        let version = self
            .next_version
            .as_deref()
            .map_or_else(|| "the next release".to_owned(), |v| format!("version {v}"));
        format!(
            "Write the release notes for {version}, covering the {} commits since {}.",
            self.commit_count, self.since
        )
    }
}

/// How many completions one run of the agent may take before swiftide stops it.
pub const STEP_LIMIT: usize = 40;

// ANCHOR: agent
/// Builds the agent. All tools run through `executor`, whose working directory is the repo.
pub fn build(
    llm: &(impl ChatCompletion + Clone + 'static),
    executor: Arc<dyn ToolExecutor>,
    task: &ReleaseTask,
    github_tools: Option<impl ToolBox + 'static>,
) -> Result<Agent> {
    let mut builder = Agent::builder();
    builder
        .llm(llm)
        .context(DefaultContext::from_executor(executor))
        .system_prompt(task.system_prompt(github_tools.is_some()))
        // Our own `stop` tool replaces the default one; call this before `tools`.
        .without_default_stop_tool()
        .tools([
            git_log_since(),
            git_diff(),
            read_file(),
            // Drafts that describe a change twice go back to the model before anyone reviews them.
            OneEntryPerChange::new(ApprovalRequired::new(WriteChangelog::new(&task.notes_file)))
                .into(),
            ReleaseNotes::stop_tool().into(),
        ])
        .limit(STEP_LIMIT);

    if let Some(github_tools) = github_tools {
        builder.add_toolbox(github_tools);
    }
    progress::report_tool_calls(&mut builder);
    progress::stream_draft(&mut builder);

    Ok(builder.build()?)
}
// ANCHOR_END: agent

#[cfg(test)]
mod tests {
    use super::*;

    fn task(next_version: Option<&str>, github: Option<GitHubRepo>) -> ReleaseTask {
        ReleaseTask {
            since: "v0.33.0".parse().unwrap(),
            commit_count: 3,
            next_version: next_version.map(str::to_owned),
            notes_file: "RELEASE_NOTES.md".into(),
            github,
        }
    }

    #[test]
    fn system_prompt_with_everything_known() {
        let github = GitHubRepo::from_remote_url("git@github.com:bosun-ai/swiftide.git");
        let prompt = task(Some("0.34.0"), github)
            .system_prompt(false)
            .render()
            .unwrap();
        assert!(prompt.contains("## 0.34.0"));
        assert!(prompt.contains("https://github.com/bosun-ai/swiftide/pull/1207"));
        assert!(!prompt.contains("get_pull_request"));
    }

    #[test]
    fn system_prompt_without_version_or_github() {
        let prompt = task(None, None).system_prompt(false).render().unwrap();
        assert!(prompt.contains("## <version>"));
        assert!(prompt.contains("Choose the version yourself"));
        assert!(!prompt.contains("https://github.com"));
    }

    #[test]
    fn system_prompt_with_github_tools() {
        let github = GitHubRepo::from_remote_url("git@github.com:bosun-ai/swiftide.git");
        let prompt = task(None, github).system_prompt(true).render().unwrap();
        assert!(
            prompt.contains("`get_pull_request` tool (owner and repo from `bosun-ai/swiftide`)")
        );
    }

    #[test]
    fn system_prompt_ignores_github_tools_without_a_github_origin() {
        let prompt = task(None, None).system_prompt(true).render().unwrap();
        assert!(!prompt.contains("get_pull_request"));
    }
}
