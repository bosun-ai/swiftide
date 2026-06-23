# Temporary Swiftide Regression Tests

This standalone crate is intentionally not listed in the root workspace. Normal
workspace commands such as `cargo test --workspace` will not run it.

Run the deterministic schema regressions:

```sh
cargo test --manifest-path temporary-regression-tests/Cargo.toml
```

Run live OpenAI tool-calling regression:

```sh
OPENAI_API_KEY=... \
SWIFTIDE_REGRESSION_OPENAI_MODEL=... \
cargo test --manifest-path temporary-regression-tests/Cargo.toml \
  --features live-openai --test live_llm -- --ignored --nocapture
```

On the current `fix/bedrock` branch this command stops at the OpenAI
integration compile regression before it reaches the live request. Re-run it
after that compile issue is fixed.

Run live Anthropic tool-calling regression:

```sh
ANTHROPIC_API_KEY=... \
SWIFTIDE_REGRESSION_ANTHROPIC_MODEL=... \
cargo test --manifest-path temporary-regression-tests/Cargo.toml \
  --features live-anthropic --test live_llm -- --ignored --nocapture
```

Run live Bedrock tool-calling regression:

```sh
AWS_REGION=... \
SWIFTIDE_REGRESSION_BEDROCK_MODEL=... \
cargo test --manifest-path temporary-regression-tests/Cargo.toml \
  --features live-bedrock --test live_llm -- --ignored --nocapture
```

The live tests ask the model to call a nested-argument tool while the prompt omits optional
nested fields. They pass when the provider returns a tool call whose arguments deserialize into the
Rust tool argument type. Optional fields may be absent or explicit `null`.
