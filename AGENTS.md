# Repository Guidelines

## Project Structure & Module Organization

Swiftide is a Rust workspace driven by the library in `swiftide/`, with supporting crates such as `swiftide-core/` for shared primitives, `swiftide-agents/` for agent orchestration, `swiftide-indexing/` and `swiftide-query/` for pipeline flows, and `swiftide-integrations/` for external connectors. Shared fixtures live in `swiftide-test-utils/`, while `examples/` hosts runnable demos and `benchmarks/` tracks performance scenarios. Static assets (logos and diagrams) are under `images/`.

## Build, Test, and Development Commands

- `cargo check --workspace --all-features` quickly verifies the entire workspace compiles with all feature flags enabled.
- `cargo build --workspace --all-features` compiles every crate and surfaces feature-gating issues early.
- `cargo check -p swiftide-agents` is a fast way to probe agent changes before touching the rest of the workspace.
- `cargo +nightly fmt --all` applies the repo `rustfmt.toml` (comment wrapping requires nightly); use `cargo +nightly fmt --all -- --check` to mirror CI formatting validation.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` mirrors the main lint job and keeps us aligned with the pedantic lint profile baked into `Cargo.toml`.
- `cargo test -j 2 --tests --all-features --no-fail-fast` mirrors the main CI test job for unit and integration tests.
- `cargo test --doc --all-features --no-fail-fast` mirrors the docs test job in CI.
- `cargo hack check --each-feature --no-dev-deps` mirrors the Cargo Hack feature-matrix check run in CI.
- `typos` mirrors the spelling check run in CI.
- `cargo test --workspace` is still useful locally when you want a broader default test sweep; use `RUST_LOG=info` if you need verbose diagnostics.
- Snapshot updates flow through `cargo insta review` after tests rewrite `.snap` files.

## Coding Style & Naming Conventions

Follow Rust 2024 idioms with four-space indentation. Public APIs should embrace builder patterns and the naming guidance from the Rust API Guidelines: `snake_case` for functions, `UpperCamelCase` for types, and `SCREAMING_SNAKE_CASE` constants. Avoid `unsafe` blocks—`Cargo.toml` forbids them at the workspace level. Keep comments concise so `wrap_comments = true` can format them within 100 columns.

## Testing Guidelines

Prefer focused crate runs such as `cargo test -p swiftide-integrations` when iterating, and opt into `-- --ignored` for heavier scenarios. Integration tests rely on `testcontainers`, so ensure Docker is available; keep fixtures inside `swiftide-test-utils/` to reuse container helpers. For `insta` snapshots, commit reviewed `.snap.new` diffs only after `cargo insta review` removes pending files.

## Commit & Pull Request Guidelines

Commits follow conventional syntax (`feat(agents): …`, `fix(indexing): …`) with a lowercase imperative summary. Pull request titles are also checked against the conventional commits format in CI, and titles ending in `!` receive the `breaking change` label automatically. Each PR should describe the change, link any GitHub issue, note API or schema impacts, and include before/after traces or logs when behavior changes. Update docs (README, website, or inline rustdoc) and add tests or benchmarks alongside functional work. Before requesting review, run the full lint and test suite listed above.

## Tooling & Environment Notes

The workspace pins `stable` in `rust-toolchain.toml`; use the same channel unless a nightly tool is explicitly required. Dependency hygiene is enforced with `cargo deny --workspace`, and spelling checks may run via `typos`. Store local credentials with `mise` or environment variables—never commit secrets.
