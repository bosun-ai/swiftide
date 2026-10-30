pub(crate) use crate::command::accept_non_zero_exit;

pub(crate) const FILE_READ_TOOL_NAMES: &[&str] =
    &["read_file", "read_file_with_line_numbers", "Read"];

pub(crate) fn is_file_read_tool(name: &str) -> bool {
    FILE_READ_TOOL_NAMES.contains(&name)
}

pub(crate) struct Templates;
impl Templates {
    pub(crate) fn from_file(name: &str) -> anyhow::Result<String> {
        Ok(match name {
            "tool_summarizer.md" => include_str!("../templates/tool_summarizer.md"),
            "conversation_summary.md" => include_str!("../templates/conversation_summary.md"),
            _ => anyhow::bail!("unknown template: {name}"),
        }
        .to_owned())
    }
    pub(crate) fn render(name: &str, context: &tera::Context) -> anyhow::Result<String> {
        Ok(tera::Tera::one_off(
            &Self::from_file(name)?,
            context,
            false,
        )?)
    }
}
