use metrics::histogram;

pub fn emit_usage(model: &str, prompt_tokens: u32, completion_tokens: u32, total_tokens: u32) {
    histogram!
}
