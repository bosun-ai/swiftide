# Demo run

A real run of the final build on the Swiftide repository itself: the 7 commits between `v0.33.0`
and the showcase changes on `master`, released as `0.34.0`.

- **Endpoint:** an `OpenAI`-compatible gateway at `http://127.0.0.1:8080/v1` (Bifrost in front of
  AWS Bedrock), using chat completions.
- **Model:** `claude-sonnet-5-5`.
- **Approval:** `y` was piped into stdin; piped answers are echoed, so it shows after the prompt.

```sh
# from the repository root, after `cargo build -p release-notes-agent`
export OPENAI_BASE_URL=http://127.0.0.1:8080/v1
export OPENAI_API_KEY=...   # the gateway's key
printf 'y\n' | target/debug/release-notes-agent --since v0.33.0 --version 0.34.0 --model claude-sonnet-5-5
```

The tool calls in each group run in parallel, so their results
print in completion order. The draft streams in while the model writes the `write_changelog`
argument. `RELEASE_NOTES.md` did not exist yet, so the notes went into a new file. It was deleted
after the run to keep the tree clean; its content is reproduced below.

````text
Drafting release notes for the 7 commits since v0.33.0 with claude-sonnet-5-5 at http://127.0.0.1:8080/v1 (chat completions)
  → git_log_since v0.33.0
  git_log_since ✓ 346 lines
  → git_diff swiftide-integrations/Cargo.toml 3b1ad22e^..3b1ad22e
  → git_diff …/src/chat_completion/chat_completion_response.rs 90bcadc7^..90bcadc7
  → git_diff swiftide-agents-contrib/Cargo.toml 71fa4587^..71fa4587
  → git_diff Cargo.toml 71fa4587^..71fa4587
  → git_diff swiftide-integrations/src/aws_bedrock_v2/mod.rs 3b1ad22e^..3b1ad22e
  → git_diff swiftide/Cargo.toml 71fa4587^..71fa4587
  git_diff ✓ 15 lines
  git_diff ✓ 72 lines
  git_diff ✓ 75 lines
  git_diff ✓ No changes to Cargo.toml in 71fa4587^..71fa4587.
  git_diff ✓ 18 lines
  git_diff ✓ 32 lines
  → git_diff Cargo.toml 3b1ad22e^..3b1ad22e
  → git_diff …ftide-integrations/src/openai/chat_completion.rs 90bcadc7^..90bcadc7
  → git_diff swiftide/Cargo.toml 27d50db4^..27d50db4
  git_diff ✓ No changes to Cargo.toml in 3b1ad22e^..3b1ad22e.
  git_diff ✓ 125 lines
  git_diff ✓ 17 lines
## 0.34.0

This release adds the opt-in `swiftide-agents-contrib` crate and Perl support for tree-sitter. It also fixes duplicated text in OpenAI streaming and resolves security advisories, which removes AWS Console login from the Bedrock client.
...
  → write_changelog <32 lines>
  write_changelog ⏸ waiting for approval

── proposed: 32 lines into a new file, RELEASE_NOTES.md
Write these release notes? [y/N] y
  → write_changelog <32 lines>
  write_changelog ✓ wrote RELEASE_NOTES.md
  → stop (version 0.34.0: highlights 2, breaking 1, fixes 1)
  stop ■ done

✔ RELEASE_NOTES.md

0.34.0 — This release adds the opt-in `swiftide-agents-contrib` crate and Perl support for tree-sitter. It also fixes duplicated text in OpenAI streaming and resolves security advisories, which removes AWS Console login from the Bedrock client.
  highlights (2)
    • Agent tools, hooks and executors in `swiftide-agents-contrib` [#1207]
    • Perl support for tree-sitter [#1198]
  breaking changes (1)
    • AWS Console login removed from the Bedrock client [#1215]
  fixes (1)
    • Repeated text in OpenAI streaming [#1216]
````

The streamed draft is cut after its first lines with `...`; it is the same as the file below.

## Resulting `RELEASE_NOTES.md`

````markdown
## 0.34.0

This release adds the opt-in `swiftide-agents-contrib` crate and Perl support for tree-sitter. It also fixes duplicated text in OpenAI streaming and resolves security advisories, which removes AWS Console login from the Bedrock client.

### Highlights

- **Agent tools, hooks and executors in `swiftide-agents-contrib`**: The new crate has file tools, a planning tool, context-management hooks and a lazy executor. Nothing is enabled by default. Through the `swiftide` crate, enable the matching feature, and the code is available under `swiftide::agents_contrib`. ([#1207](https://github.com/bosun-ai/swiftide/pull/1207))

  | `swiftide` feature | Provides |
  | --- | --- |
  | `agents-contrib-file-tools` | File reading with line ranges, exact text replacement, and patches |
  | `agents-contrib-planning` | An in-memory plan tool |
  | `agents-contrib-context` | Tool-output summarization and conversation compression hooks |
  | `agents-contrib-lazy-executor` | An executor that starts on its first operation |

  ```toml
  swiftide = { version = "0.34", features = ["agents-contrib-file-tools", "agents-contrib-planning"] }
  ```

- **Perl support for tree-sitter**: `SupportedLanguages::Perl` parses `.pm`, `.pl` and `.t` files. The code outliner and the defs/refs code tree don't support Perl yet and panic with `unimplemented!()` if used with it. ([#1198](https://github.com/bosun-ai/swiftide/pull/1198))

  ```toml
  swiftide = { version = "0.34", features = ["tree-sitter-perl"] }
  ```

### Breaking changes

- **AWS Console login removed from the Bedrock client**: The `credentials-login` feature of `aws-config` is no longer enabled, to resolve security advisories in its dependency chain. The default `AwsBedrock` client no longer accepts AWS Console login sessions. Use another credential provider, such as environment variables, a profile, SSO or an IAM role, or pass a configured Bedrock client. The same update moves `pulldown-cmark`, `zstd` and `zstd-safe` to patched releases. ([#1215](https://github.com/bosun-ai/swiftide/pull/1215))

### Fixes

- **Repeated text in OpenAI streaming**: The final response from `complete_stream` repeated the last text chunk. It now carries no new text, so concatenating the chunks gives the message once. ([#1216](https://github.com/bosun-ai/swiftide/pull/1216))
````

## How the notes compare to the history

`git log --oneline v0.33.0..HEAD`:

```text
7e2181e9 chore(examples): scaffold showcase example crates
90bcadc7 fix(openai): avoid repeating streamed text deltas (#1216)
3b1ad22e fix(deps): resolve security advisories (#1215)
71fa4587 feat(agents): add optional contrib tools and context hooks (#1207)
11640df7 fix(ci): restore a master-only Rust dependency cache and drop debugi...
27d50db4 feat(integrations): add Perl to SupportedLanguages behind tree-sitte...
b657f669 chore(deps): bump mockall from 0.14.0 to 0.15.0 (#1142)
```

Each of the four user-facing pull requests has one entry. The CI fix, the scaffolding of these
examples and the `mockall` bump (a dev-dependency) are left out, as the prompt asks. Checked
against the diffs:

- #1207: the `agents-contrib-*` features are on `swiftide/Cargo.toml`, and the crate is
  re-exported as `swiftide::agents_contrib`. The lazy executor defers setup until an agent first
  needs a command or a file.
- #1198: `tree-sitter-perl` is a `swiftide` feature, and the `unimplemented!()` caveat for the
  outliner and the defs/refs code tree comes from the pull request description.
- #1215: the only code change removes `credentials-login` from `aws-config`, so it is listed as a
  breaking change rather than as the "Documentation" fix the bot summary in the commit calls it.
- #1216: the entry names `complete_stream`, the call users make, and the symptom; it does not name
  the internal accumulator method that changed.
