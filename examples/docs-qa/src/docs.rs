//! The documentation domain: a [`Page`] is one titled unit of markdown that we index.
//!
//! Everything here is plain data and string handling; no I/O and no Swiftide types, so the rules
//! for "what is a page" are easy to test and to change.

use std::fmt;

/// Metadata key under which every chunk carries the title of the page it was cut from.
pub const PAGE_TITLE: &str = "Page";

/// The human readable title of a documentation page, e.g. `Swiftide` or `Contributing`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageTitle(String);

impl PageTitle {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PageTitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One titled page of markdown, together with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub title: PageTitle,
    /// A file path or URL, used to point readers back to the original.
    pub origin: String,
    pub markdown: String,
}

impl Page {
    /// A markdown file is one page, titled by its first top-level heading.
    ///
    /// GitHub-style READMEs often use an HTML `<h1>` instead; after that the file name is used.
    pub fn from_markdown_file(origin: String, markdown: String) -> Self {
        let title = top_level_headings(&markdown)
            .next()
            .map(|(_, title)| title)
            .or_else(|| html_heading(&markdown))
            .unwrap_or_else(|| title_from_origin(&origin));

        Self {
            title: PageTitle(title),
            origin,
            markdown,
        }
    }

    /// Splits an `llms-full.txt` file into pages, one per top-level (`# `) heading.
    ///
    /// Headings inside fenced code blocks are ignored, and any preamble before the first heading
    /// becomes its own page titled after the origin.
    pub fn split_llms_full(origin: &str, text: &str) -> Vec<Self> {
        let mut starts: Vec<(usize, String)> = top_level_headings(text).collect();
        if starts.first().is_none_or(|(offset, _)| *offset > 0) {
            starts.insert(0, (0, title_from_origin(origin)));
        }

        let ends = starts
            .iter()
            .skip(1)
            .map(|(offset, _)| *offset)
            .chain([text.len()]);

        starts
            .iter()
            .zip(ends)
            .filter_map(|((start, title), end)| {
                let markdown = text[*start..end].trim();
                (!markdown.is_empty()).then(|| Self {
                    title: PageTitle(title.clone()),
                    origin: origin.to_string(),
                    markdown: markdown.to_string(),
                })
            })
            .collect()
    }
}

/// Yields `(byte offset, line)` for every line outside fenced code blocks.
fn lines_outside_fences(markdown: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut in_fence = false;
    let mut offset = 0;

    markdown.split_inclusive('\n').filter_map(move |line| {
        let line_start = offset;
        offset += line.len();

        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            return None;
        }
        (!in_fence).then_some((line_start, line))
    })
}

/// Yields `(byte offset, title)` for every `# Title` line outside fenced code blocks.
fn top_level_headings(markdown: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    lines_outside_fences(markdown).filter_map(|(offset, line)| {
        line.trim_end()
            .strip_prefix("# ")
            .map(|title| (offset, title.trim().to_string()))
    })
}

/// The text of the first single-line `<h1 ...>Title</h1>` outside code.
fn html_heading(markdown: &str) -> Option<String> {
    lines_outside_fences(markdown).find_map(|(_, line)| html_heading_in(line))
}

fn html_heading_in(line: &str) -> Option<String> {
    let start = line.find("<h1")?;
    // An odd number of backticks before the tag means it sits inside an inline code span.
    if line[..start].matches('`').count() % 2 == 1 {
        return None;
    }

    let tag = &line[start..];
    let open = tag.find('>')? + 1;
    let close = tag.find("</h1>")?;
    let text = tag.get(open..close)?.trim();
    (!text.is_empty() && !text.contains('<')).then(|| text.to_string())
}

fn title_from_origin(origin: &str) -> String {
    origin
        .trim_end_matches('/')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(origin)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_markdown_file_is_titled_by_its_first_heading() {
        let page = Page::from_markdown_file(
            "docs/guide.md".into(),
            "Intro line\n\n# Getting started\n\n## Install\n".into(),
        );

        assert_eq!(page.title.as_str(), "Getting started");
    }

    #[test]
    fn a_readme_with_an_html_heading_is_titled_by_it() {
        let page = Page::from_markdown_file(
            "README.md".into(),
            "<div>\n  <h1 align=\"center\">Swiftide</h1>\n</div>\n\n## Why\n".into(),
        );

        assert_eq!(page.title.as_str(), "Swiftide");
    }

    #[test]
    fn html_headings_in_code_or_malformed_tags_are_ignored() {
        for markdown in [
            "Use `<h1>Title</h1>` for headings.",
            "```html\n<h1>Title</h1>\n```",
            "<h1</h1>",
            "<h1 class=x</h1>",
            "<h1><img src=\"logo.svg\"></h1>",
        ] {
            let page = Page::from_markdown_file("docs/page.md".into(), markdown.into());

            assert_eq!(page.title.as_str(), "page.md", "{markdown}");
        }
    }

    #[test]
    fn a_markdown_file_without_heading_is_titled_by_its_file_name() {
        let page = Page::from_markdown_file("docs/CHANGELOG.md".into(), "no headings".into());

        assert_eq!(page.title.as_str(), "CHANGELOG.md");
    }

    #[test]
    fn llms_full_is_split_on_top_level_headings_only() {
        let text = "# Indexing\nLoad things.\n## Loaders\nMore.\n```md\n# not a page\n```\n# Querying\nAsk things.\n";

        let pages = Page::split_llms_full("https://swiftide.rs/llms-full.txt", text);

        let titles: Vec<_> = pages.iter().map(|page| page.title.as_str()).collect();
        assert_eq!(titles, ["Indexing", "Querying"]);
        assert!(pages[0].markdown.contains("# not a page"));
        assert!(pages[1].markdown.starts_with("# Querying"));
    }

    #[test]
    fn llms_full_preamble_becomes_its_own_page() {
        let pages = Page::split_llms_full("site/llms-full.txt", "> Summary\n\n# First\nBody\n");

        let titles: Vec<_> = pages.iter().map(|page| page.title.as_str()).collect();
        assert_eq!(titles, ["llms-full.txt", "First"]);
    }
}
