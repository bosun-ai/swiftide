# Source parity

This crate extracts the following implementations from Fluyt commit
`3f7a5d15` (`quak/quak-core/`). Fluyt is unchanged.

| Contrib API | Source under `src/` | Preserved behavior |
| --- | --- | --- |
| `ReadFile` (tool name `Read`) | `tools/claude_code_file_tools.rs`: `ClaudeCodeRead` and its reachable helpers | Absolute paths, arguments, 2,000-line default, character accounting, numbered output, metadata checks, locks, and page coverage |
| `ExactReplace` | `tools/exact_replace.rs` | Arguments, lossy decoding, exact matching, validation, errors, and writes |
| `ApplyPatch` | `tools/apply_patch.rs` | Parser, matching, newline handling, path validation, shell commands, operation order, retry detection, and reports |
| `UpdatePlan` | `tools/update_plan.rs` | Shared state, required arguments, validation, trimming, rendering, and errors |
| `ToolSummarizer` | `hooks/tool_summarizer.rs` | Defaults, exclusions, truncation, summary selection, Git diffs, prompt, logging, and model failures |
| `ContextCompression` | `hooks/context_compression.rs` and `hooks/context_compression/original_instructions.rs` | Original v1 thresholds, counter order, history filtering, plan extraction, Git diffs, prompt, and plain-text instruction recovery |
| `LazyExecutor` | `lazy_executor.rs` | Shared initialization, retries, streaming, startup status, cleanup, and download delegation |
| `ToolExecutorExt` and download helpers | `traits.rs`, `executor_download.rs` | Lifecycle, scoped delegation, chunking, size checks, binary transport, checksum, and error cleanup |

The two prompt templates are copied verbatim from `templates/tool_summarizer.md` and
`templates/conversation_summary.md`. The source's uncapped `read_file`, other provider file tools,
and context compression v2 are not exposed.

## Extraction adapters

- Imports target `swiftide-core` and `swiftide-agents` directly. Internal application registration
  and Quacker wiring are removed; callers register tools and hooks explicitly.
- `ClaudeCodeRead` is named `ReadFile` in Rust, and its state has a generic internal name.
  Its model-visible name remains `Read`. State helpers used only by unextracted write/edit tools
  are omitted. This does not attach read-state checks to the separate `ExactReplace` tool.
- `UpdatePlan` and original-instructions recovery implement `Tool` directly instead of using the
  facade derive macro, which would create a crate dependency cycle. Their generated contracts
  are retained, including schema descriptions and required fields. Public plan documentation
  is kept out of model-facing schemas.
- A private template adapter embeds the two templates and renders them with Tera. Neither
  template depends on application templates, custom filters, or includes.
- `ToolExecutorExt` retains the generic source contract; application executor implementations
  are excluded. The local executor has no cleanup work. A local boxed-future alias replaces
  the source's equivalent futures alias.
- Feature gates, public documentation, `no_run` examples, and builder documentation are added.
  The public builder `build` methods delegate to the original generated builder logic.
  `ContextCompression::builder()` is a convenience for its default builder.

## Verification and behavior changes

Runtime function bodies were compared against this source revision, ignoring comments,
whitespace, import paths, and the listed type names. Templates were checked byte for byte.
Tests exercise source read limits and race checks, patch operations and failures, exact
replacement, plan contracts, hook defaults and failures, initialization, and binary downloads.
Public examples compile as doctests, and each feature is checked independently.

This extraction deliberately retains source behavior, including its existing edge cases.
Earlier contrib-only changes to UTF-8 validation, patch trailing blank lines, file identity
checks, multiline plan validation, summary retries, final-answer handling, and multipart
instruction recovery have been removed. Behavior changes should first be made in Fluyt,
then carried over here with matching tests.
