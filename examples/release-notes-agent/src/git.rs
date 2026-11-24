//! Read-only git queries, executed through a Swiftide [`ToolExecutor`].
//!
//! Both the CLI (to resolve the release range) and the agent's tools go through [`Git`], so every
//! command runs in the executor's working directory: the repository root. The value types
//! [`Revision`] and [`RepoPath`] validate whatever the LLM passes in before it reaches a shell.
use std::{
    borrow::Cow,
    fmt,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use swiftide::traits::{Command, ToolExecutor};

/// A git revision or revision range, e.g. `v0.33.0`, `HEAD~3` or `v0.33.0..HEAD`.
///
/// Only characters that are meaningful in revisions are accepted, so a revision can never smuggle
/// in a flag or a shell metacharacter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision(String);

impl Revision {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The range from this revision up to `HEAD`.
    pub fn to_head(&self) -> Revision {
        Revision(format!("{}..HEAD", self.0))
    }
}

impl FromStr for Revision {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let allowed = |c: char| c.is_ascii_alphanumeric() || "._/~^@-".contains(c);
        ensure!(!raw.is_empty(), "revision is empty");
        ensure!(
            !raw.starts_with('-'),
            "revision `{raw}` must not start with `-`"
        );
        ensure!(
            raw.chars().all(allowed),
            "revision `{raw}` contains characters that are not valid in a git revision"
        );
        Ok(Self(raw.to_owned()))
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A path relative to the repository root that cannot point outside of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoPath(PathBuf);

impl RepoPath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl FromStr for RepoPath {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self> {
        let path = Path::new(raw.trim());
        ensure!(!path.as_os_str().is_empty(), "path is empty");
        for component in path.components() {
            match component {
                Component::Normal(_) | Component::CurDir => {}
                Component::ParentDir => bail!("path `{raw}` must not contain `..`"),
                Component::RootDir | Component::Prefix(_) => {
                    bail!("path `{raw}` must be relative to the repository root")
                }
            }
        }
        Ok(Self(path.to_path_buf()))
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

/// A GitHub repository, as `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
    pub owner: String,
    pub name: String,
}

impl GitHubRepo {
    /// Parses `git@github.com:owner/name.git` and `https://github.com/owner/name(.git)`.
    pub fn from_remote_url(url: &str) -> Option<Self> {
        let path = url
            .trim()
            .strip_prefix("git@github.com:")
            .or_else(|| url.trim().strip_prefix("https://github.com/"))?;
        let (owner, name) = path.trim_end_matches('/').split_once('/')?;
        let name = name.strip_suffix(".git").unwrap_or(name);
        (!owner.is_empty() && !name.is_empty()).then(|| Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

impl fmt::Display for GitHubRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// Git, as seen through an executor whose working directory is the repository.
#[derive(Clone, Copy)]
pub struct Git<'a> {
    executor: &'a dyn ToolExecutor,
}

impl<'a> Git<'a> {
    pub fn new(executor: &'a dyn ToolExecutor) -> Self {
        Self { executor }
    }

    /// Absolute path of the repository root.
    pub async fn top_level(&self) -> Result<PathBuf> {
        let root = self.run("git rev-parse --show-toplevel").await?;
        Ok(PathBuf::from(root.trim()))
    }

    /// The most recent tag reachable from `HEAD`.
    pub async fn latest_tag(&self) -> Result<Revision> {
        let tag = self
            .run("git describe --tags --abbrev=0")
            .await
            .context("no tag found to start the release from; pass --since")?;
        tag.parse()
    }

    /// Number of non-merge commits in `range`.
    pub async fn commit_count(&self, range: &Revision) -> Result<usize> {
        let count = self
            .run(&format!("git rev-list --count --no-merges {range}"))
            .await?;
        count
            .trim()
            .parse()
            .context("unexpected `git rev-list` output")
    }

    /// Subjects, bodies and changed files of every non-merge commit since `since`, oldest first.
    pub async fn log_since(&self, since: &Revision) -> Result<String> {
        self.run(&format!(
            "git log --no-merges --reverse --stat=120 --stat-count=25 \
             --format='%n## %h %s%n%n%b' {}",
            since.to_head()
        ))
        .await
    }

    /// The diff of a single path over `range`.
    pub async fn diff(&self, range: &Revision, path: &RepoPath) -> Result<String> {
        self.run(&format!(
            "git diff --stat --patch {range} -- {}",
            shell_quote(&path.to_string())
        ))
        .await
    }

    /// The url of the `origin` remote.
    pub async fn origin_url(&self) -> Result<String> {
        self.run("git remote get-url origin").await
    }

    async fn run(&self, command: &str) -> Result<String> {
        let output = self
            .executor
            .exec_cmd(&Command::shell(command))
            .await
            // The error carries git's stderr, which tells the model what to fix.
            .map_err(|err| anyhow!("`{command}` failed: {err}"))?;
        Ok(output.into_string_lossy())
    }
}

/// Wraps `raw` in single quotes for `sh`.
pub fn shell_quote(raw: &str) -> Cow<'_, str> {
    if raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
    {
        Cow::Borrowed(raw)
    } else {
        Cow::Owned(format!("'{}'", raw.replace('\'', r"'\''")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_accept_tags_and_ranges() {
        for ok in [
            "v0.33.0",
            "v0.33.0..HEAD",
            "HEAD~3",
            "origin/master",
            "abc123^",
        ] {
            assert!(ok.parse::<Revision>().is_ok(), "{ok}");
        }
    }

    #[test]
    fn revisions_reject_flags_and_shell() {
        for bad in ["", "--output=/tmp/x", "v1; rm -rf /", "$(id)", "a b"] {
            assert!(bad.parse::<Revision>().is_err(), "{bad}");
        }
    }

    #[test]
    fn repo_paths_stay_inside_the_repo() {
        assert!("swiftide-agents/src/lib.rs".parse::<RepoPath>().is_ok());
        assert!("./README.md".parse::<RepoPath>().is_ok());
        assert!("../secrets".parse::<RepoPath>().is_err());
        assert!("src/../../etc/passwd".parse::<RepoPath>().is_err());
        assert!("/etc/passwd".parse::<RepoPath>().is_err());
    }

    #[test]
    fn shell_quote_escapes_quotes() {
        assert_eq!(shell_quote("src/lib.rs"), "src/lib.rs");
        assert_eq!(shell_quote("it's here.md"), r"'it'\''s here.md'");
    }

    #[test]
    fn github_repo_from_remotes() {
        let expected = GitHubRepo {
            owner: "bosun-ai".into(),
            name: "swiftide".into(),
        };
        assert_eq!(
            GitHubRepo::from_remote_url("git@github.com:bosun-ai/swiftide.git"),
            Some(expected.clone())
        );
        assert_eq!(
            GitHubRepo::from_remote_url("https://github.com/bosun-ai/swiftide"),
            Some(expected)
        );
        assert_eq!(GitHubRepo::from_remote_url("https://gitlab.com/a/b"), None);
    }
}
