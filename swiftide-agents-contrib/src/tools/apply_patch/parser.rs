use anyhow::bail;
use std::path::PathBuf;

pub(super) enum Hunk {
    Add {
        path: PathBuf,
        contents: String,
    },
    Delete {
        path: PathBuf,
    },
    Update {
        path: PathBuf,
        move_path: Option<PathBuf>,
        chunks: Vec<UpdateFileChunk>,
    },
}

pub(super) struct ParsedPatch {
    pub(super) hunks: Vec<Hunk>,
}

#[derive(Clone)]
pub(super) struct UpdateFileChunk {
    pub(super) change_context: Option<String>,
    pub(super) old_lines: Vec<String>,
    pub(super) new_lines: Vec<String>,
    pub(super) is_end_of_file: bool,
}

impl UpdateFileChunk {
    pub(super) fn is_navigation(&self) -> bool {
        self.old_lines == self.new_lines
            && (!self.old_lines.is_empty() || self.change_context.is_some())
    }
}

/// Parse all directives before any file is changed.
pub(super) fn parse_patch(input: &str) -> anyhow::Result<ParsedPatch> {
    let lines: Vec<_> = input.trim().lines().collect();
    anyhow::ensure!(
        lines
            .first()
            .is_some_and(|line| line.trim() == "*** Begin Patch"),
        "patch must start with '*** Begin Patch'"
    );
    anyhow::ensure!(
        lines
            .last()
            .is_some_and(|line| line.trim() == "*** End Patch"),
        "patch must end with '*** End Patch'"
    );
    let end = lines.len() - 1;
    let mut index = 1;
    let mut hunks = Vec::new();
    while index < end {
        let header = lines[index].trim();
        index += 1;
        if header.is_empty() {
            continue;
        }
        let (hunk, next) = parse_operation(header, &lines, index, end)?;
        hunks.push(hunk);
        index = next;
    }
    Ok(ParsedPatch { hunks })
}

fn parse_operation(
    header: &str,
    lines: &[&str],
    index: usize,
    end: usize,
) -> anyhow::Result<(Hunk, usize)> {
    if let Some(path) = header.strip_prefix("*** Add File: ") {
        return parse_add(path, lines, index, end);
    }
    if let Some(path) = header.strip_prefix("*** Delete File: ") {
        return Ok((
            Hunk::Delete {
                path: PathBuf::from(path.trim()),
            },
            index,
        ));
    }
    if let Some(path) = header.strip_prefix("*** Update File: ") {
        return parse_update(path, lines, index, end);
    }
    bail!("line {index}: unknown patch directive '{header}'")
}

fn parse_add(
    path: &str,
    lines: &[&str],
    mut index: usize,
    end: usize,
) -> anyhow::Result<(Hunk, usize)> {
    let mut contents = String::new();
    while index < end && !lines[index].trim().starts_with("***") {
        let Some(content) = lines[index].strip_prefix('+') else {
            bail!(
                "line {}: lines inside an Add File section must start with '+'",
                index + 1
            );
        };
        contents.push_str(content);
        contents.push('\n');
        index += 1;
    }
    if contents.is_empty() {
        contents.push('\n');
    }
    Ok((
        Hunk::Add {
            path: path.trim().into(),
            contents,
        },
        index,
    ))
}

fn parse_update(
    path: &str,
    lines: &[&str],
    mut index: usize,
    end: usize,
) -> anyhow::Result<(Hunk, usize)> {
    let move_path = lines
        .get(index)
        .and_then(|line| line.trim().strip_prefix("*** Move to: "))
        .map(|path| PathBuf::from(path.trim()));
    if move_path.is_some() {
        index += 1;
    }
    let mut chunks = Vec::new();
    while index < end {
        let raw = lines[index];
        let line = raw.trim();
        if line.starts_with("***") && line != "*** End of File" {
            break;
        }
        if is_blank_hunk_separator(lines, index) {
            index += 1;
            continue;
        }
        let implicit = chunks.is_empty() && (raw.is_empty() || raw.starts_with([' ', '-', '+']));
        anyhow::ensure!(
            line.starts_with("@@") || implicit,
            "line {}: expected '@@' hunk header",
            index + 1
        );
        let (chunk, next) = parse_update_chunk(lines, index, end)?;
        chunks.push(chunk);
        index = next;
    }
    anyhow::ensure!(
        !chunks.is_empty(),
        "update for '{path}' must contain at least one hunk"
    );
    Ok((
        Hunk::Update {
            path: path.trim().into(),
            move_path,
            chunks,
        },
        index,
    ))
}

fn parse_update_chunk(
    lines: &[&str],
    start: usize,
    end: usize,
) -> anyhow::Result<(UpdateFileChunk, usize)> {
    let header = lines[start].trim();
    let (change_context, mut idx) = match header {
        "@@" => (None, start + 1),
        _ if header.starts_with("@@ ") => (Some(header[3..].trim().to_string()), start + 1),
        _ if lines[start].is_empty() || lines[start].starts_with([' ', '-', '+']) => (None, start),
        _ => bail!("line {}: invalid hunk header '{header}'", start + 1),
    };

    let mut chunk = UpdateFileChunk {
        change_context,
        old_lines: Vec::new(),
        new_lines: Vec::new(),
        is_end_of_file: false,
    };
    while idx < end {
        let raw = lines[idx];
        if is_blank_hunk_separator(lines, idx) {
            idx += 1;
            break;
        }
        if raw.starts_with("@@") {
            break;
        }
        if raw.starts_with("***") {
            chunk.is_end_of_file = raw == "*** End of File";
            idx += usize::from(chunk.is_end_of_file);
            break;
        }
        append_content(&mut chunk, raw, idx + 1)?;
        idx += 1;
    }
    anyhow::ensure!(
        !chunk.old_lines.is_empty()
            || !chunk.new_lines.is_empty()
            || chunk.change_context.is_some(),
        "line {}: hunk must contain a change or a navigation anchor",
        start + 1
    );
    Ok((chunk, idx))
}

fn append_content(chunk: &mut UpdateFileChunk, line: &str, number: usize) -> anyhow::Result<()> {
    match line.chars().next() {
        Some(' ') => {
            chunk.old_lines.push(line[1..].into());
            chunk.new_lines.push(line[1..].into());
        }
        Some('-') => chunk.old_lines.push(line[1..].into()),
        Some('+') => chunk.new_lines.push(line[1..].into()),
        None => {
            chunk.old_lines.push(String::new());
            chunk.new_lines.push(String::new());
        }
        Some(prefix) => bail!("line {number}: unexpected hunk line prefix '{prefix}'"),
    }
    Ok(())
}

fn is_blank_hunk_separator(lines: &[&str], index: usize) -> bool {
    lines[index].is_empty()
        && lines.get(index + 1).is_some_and(|next| {
            let next = next.trim();
            next.starts_with("@@") || (next.starts_with("***") && next != "*** End of File")
        })
}
