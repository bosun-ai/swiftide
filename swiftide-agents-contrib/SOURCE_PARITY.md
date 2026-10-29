# Source parity

This crate extracts the following implementations from Fluyt commit
`aa62169de72ff16ac92d4e5aec54bf3cd8773c00` (`quak/quak-core/`) as its original
extraction baseline. The merged fixes listed below are synchronized from Fluyt `master`
`0446f4610cb620964b7bb4d0f2058dbca1cb728b`, verified against the GitHub commits API
and the local `origin/master` ref on 2026-10-29. Fluyt received these fixes through separate
upstream pull requests before they were carried into this crate.

| Contrib API | Source under `src/` | Preserved behavior |
| --- | --- | --- |
| `ReadFile` (`read_file` and `read_file_with_line_numbers`) | `tools/read_file.rs` | Shared window selection, arguments, 2,000-line default, 2,000-character line truncation, 100,000-character window limit, numbering, continuation, and executor errors |
| `ExactReplace` | `tools/exact_replace.rs` | Arguments, strict UTF-8 decoding, exact matching, validation, errors, and writes |
| `ApplyPatch` | `tools/apply_patch.rs` | Parser, matching, newline handling, path validation, shell commands, operation order, retry detection, and reports |
| `UpdatePlan` | `tools/update_plan.rs` | Shared state, required arguments, validation, trimming, rendering, and errors |
| `ToolSummarizer` | `hooks/tool_summarizer.rs` | Defaults, exclusions, truncation, summary selection, Git diffs, prompt, logging, and model failures |
| `ContextCompression` | `hooks/context_compression.rs` and `hooks/context_compression/original_instructions.rs` | Original v1 thresholds, counter order, history filtering, plan extraction, Git diffs, prompt, and plain-text instruction recovery |
| `LazyExecutor` | `lazy_executor.rs` | Shared initialization, retries, streaming, startup status, cleanup, and download delegation |
| `ToolExecutorExt` and download helpers | `traits.rs`, `executor_download.rs` | Lifecycle, scoped delegation, chunking, size checks, binary transport, checksum, and error cleanup |

The two prompt templates are copied verbatim from `templates/tool_summarizer.md` and
`templates/conversation_summary.md`. The separate provider-specific `Read`/`Edit`/`Write` tools
and context compression v2 are not exposed.

## Extraction adapters

- Imports target `swiftide-core` and `swiftide-agents` directly. Internal application registration
  and Quacker wiring are removed; callers register tools and hooks explicitly.
- `ReadFile` wraps the source's two file-reading entry points in one Rust type. The default
  registers `read_file`; `with_line_numbers()` registers `read_file_with_line_numbers`.
  Both call the original shared window implementation. Tool names and descriptions, arguments,
  schemas, and missing-argument errors retain the source derive macro's contracts.
- The reader, `UpdatePlan`, and original-instructions recovery implement `Tool` directly instead
  of using the facade derive macro, which would create a crate dependency cycle. Their generated
  contracts are retained, including schema descriptions and required fields. Public plan
  documentation is kept out of model-facing schemas.
- A private template adapter embeds the two templates and renders them with Tera. Neither
  template depends on application templates, custom filters, or includes.
- `ToolExecutorExt` retains the generic source contract; application executor implementations
  are excluded. The local executor has no cleanup work. A local boxed-future alias replaces
  the source's equivalent futures alias.
- Feature gates, public documentation, `no_run` examples, and builder documentation are added.
  The public builder `build` methods delegate to the original generated builder logic.
  `ContextCompression::builder()` is a convenience for its default builder.

## Verification and behavior changes

Runtime function bodies were compared against the verified master revision, ignoring comments,
whitespace, import paths, and the listed type names. Templates were checked byte for byte.
Tests exercise source paging, line truncation, window refusal, and both read contracts, patch operations and failures, exact
replacement, plan contracts, hook defaults and failures, initialization, and binary downloads.
Public examples compile as doctests, and each feature is checked independently.

This extraction retains upstream runtime behavior. The following fixes were merged in Fluyt
before being carried over with regression tests:

| Upstream PR | Merge revision | Extracted changes |
| --- | --- | --- |
| [#2592](https://github.com/bosun-ai/fluyt/pull/2592) | `95d63f834a99c083367638cb3dcb7a28dd27e56c` | Download cleanup begins only after the destination is opened; rejected remote files preserve an existing destination |
| [#2593](https://github.com/bosun-ai/fluyt/pull/2593) | `09b19a2cbea733866c293ed137c4430b89ce8972` | Patch moves reject source and destination referring to the same file, including symlinks and hard links |
| [#2594](https://github.com/bosun-ai/fluyt/pull/2594) | `0446f4610cb620964b7bb4d0f2058dbca1cb728b` | Patch updates preserve trailing blank lines |
| [#2595](https://github.com/bosun-ai/fluyt/pull/2595) | `56c86cbd281563a6430a3daa812ccb1c5309e1c1` | Exact replacements and patch updates reject invalid UTF-8 without changing bytes |
| [#2597](https://github.com/bosun-ai/fluyt/pull/2597) | `484bcddbfa8b02068f513daee75b582717b4107d` | Plan steps reject line breaks, explanation schema states the required contract, and compression recovers only the rendered plan block |

No changes from open compression PR #2596 or provider read PR #2598 are included. The generic
reader retains its existing contracts. Compression retry, final-answer, and multipart instruction
recovery behavior remains the original implementation. Public patch documentation additionally
explains symlink handling and executor isolation without changing runtime behavior.
