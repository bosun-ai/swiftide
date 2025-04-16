# Changelog

All notable changes to this project will be documented in this file.

## [0.25.0](https://github.com/bosun-ai/swiftide/compare/v0.24.0...v0.25.0) - 2025-04-16

### New features

- [4959ddf](https://github.com/bosun-ai/swiftide/commit/4959ddfe00e0424215dd9bd3e8a6acb579cc056c) *(agents)*  Restore agents from an existing message history ([#742](https://github.com/bosun-ai/swiftide/pull/742))

- [6efd15b](https://github.com/bosun-ai/swiftide/commit/6efd15bf7b88d8f8656c4017676baf03a3bb510e) *(agents)*  Agents now take an Into Prompt when queried ([#743](https://github.com/bosun-ai/swiftide/pull/743))

### Bug fixes

- [5db4de2](https://github.com/bosun-ai/swiftide/commit/5db4de2f0deb2028f5ffaf28b4d26336840e908c) *(agents)*  Properly support nullable types for MCP tools ([#740](https://github.com/bosun-ai/swiftide/pull/740))

- [dd2ca86](https://github.com/bosun-ai/swiftide/commit/dd2ca86b214e8268262075a513711d6b9c793115) *(agents)*  Do not log twice if mcp failed to stop

- [5fea2e2](https://github.com/bosun-ai/swiftide/commit/5fea2e2acdca0782f88d4274bb8e106b48e1efe4) *(indexing)*  Split pipeline concurrently ([#749](https://github.com/bosun-ai/swiftide/pull/749))

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies

- [0f2605a](https://github.com/bosun-ai/swiftide/commit/0f2605a61240d2c99e10ce6f5a91e6568343a78b)  Pretty print RAGAS output ([#745](https://github.com/bosun-ai/swiftide/pull/745))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.24.0...0.25.0



## [0.24.0](https://github.com/bosun-ai/swiftide/compare/v0.23.0...v0.24.0) - 2025-04-11

### New features

- [3117fc6](https://github.com/bosun-ai/swiftide/commit/3117fc62c146b0bf0949adb3cfe4e6c7f40427f7)  Introduce LanguageModelError for LLM traits and an optional backoff decorator ([#630](https://github.com/bosun-ai/swiftide/pull/630))

### Bug fixes

- [0134dae](https://github.com/bosun-ai/swiftide/commit/0134daebef5d47035e986d30e1fa8f2c751c2c48) *(agents)*  Gracefully stop mcp service on drop ([#734](https://github.com/bosun-ai/swiftide/pull/734))

### Miscellaneous

- [e872c5b](https://github.com/bosun-ai/swiftide/commit/e872c5b24388754b371d9f0c7faad8647ad4733b)  Core test utils available behind feature flag ([#730](https://github.com/bosun-ai/swiftide/pull/730))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.23.0...0.24.0



## [0.23.0](https://github.com/bosun-ai/swiftide/compare/v0.22.8...v0.23.0) - 2025-04-08

### New features

- [fca4165](https://github.com/bosun-ai/swiftide/commit/fca4165c5be4b14cdc3d20ed8215ef64c5fd69a9) *(agents)*  Return typed errors and yield error in `on_stop` ([#725](https://github.com/bosun-ai/swiftide/pull/725))

- [29352e6](https://github.com/bosun-ai/swiftide/commit/29352e6d3dc51779f3202e0e9936bf72e0b61605) *(agents)*  Add `on_stop` hook and `stop` now takes a `StopReason` ([#724](https://github.com/bosun-ai/swiftide/pull/724))

- [a85cd8e](https://github.com/bosun-ai/swiftide/commit/a85cd8e2d014f198685ee6bfcfdf17f7f34acf91) *(macros)*  Support generics in Derive for tools ([#720](https://github.com/bosun-ai/swiftide/pull/720))

- [52c44e9](https://github.com/bosun-ai/swiftide/commit/52c44e9b610c0ba4bf144881c36eacc3a0d10e53)  Agent mcp client support  ([#658](https://github.com/bosun-ai/swiftide/pull/658))

````text
Adds support for agents to use tools from MCP servers. All transports
  are supported via the `rmcp` crate.

  Additionally adds the possibility to add toolboxes to agents (of which
  MCP is one). Tool boxes declare their available tools at runtime, like
  tool box.
````

### Miscellaneous

- [69706ec](https://github.com/bosun-ai/swiftide/commit/69706ec6630b70ea9d332c151637418736437a99)  [**breaking**] Remove templates ([#716](https://github.com/bosun-ai/swiftide/pull/716))

````text
Template / prompt interface got confusing and bloated. This removes
  `Template` fully, and changes Prompt such that it can either ref to a
  one-off, or to a template named compiled in the swiftide repository.
````

**BREAKING CHANGE**: This removes `Template` from Swiftide and simplifies
the whole setup significantly. The internal Swiftide Tera repository can
still be extended like with Templates. Same behaviour with less code and
abstractions.


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.8...0.23.0



## [0.22.8](https://github.com/bosun-ai/swiftide/compare/v0.22.7...v0.22.8) - 2025-04-02

### Bug fixes

- [6b4dfca](https://github.com/bosun-ai/swiftide/commit/6b4dfca822f39b3700d60e6ea31b9b48ccd6d56f)  Tool macros should work with latest darling version ([#712](https://github.com/bosun-ai/swiftide/pull/712))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.7...0.22.8



## [0.22.7](https://github.com/bosun-ai/swiftide/compare/v0.22.6...v0.22.7) - 2025-03-30

### Bug fixes

- [b0001fb](https://github.com/bosun-ai/swiftide/commit/b0001fbb12cf6bb85fc4d5a8ef0968219e8c78db) *(duckdb)*  Upsert is now opt in as it requires duckdb >= 1.2 ([#708](https://github.com/bosun-ai/swiftide/pull/708))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.6...0.22.7



## [0.22.6](https://github.com/bosun-ai/swiftide/compare/v0.22.5...v0.22.6) - 2025-03-27

### New features

- [a05b3c8](https://github.com/bosun-ai/swiftide/commit/a05b3c8e7c4224c060215c34490b2ea7729592bf) *(macros)*  Support optional values and make them even nicer to use ([#703](https://github.com/bosun-ai/swiftide/pull/703))

### Bug fixes

- [1866d5a](https://github.com/bosun-ai/swiftide/commit/1866d5a081f40123e607208d04403fb98f34c057) *(integrations)*  Loosen up duckdb requirements even more and make it more flexible for version requirements ([#706](https://github.com/bosun-ai/swiftide/pull/706))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.5...0.22.6


# Changelog

All notable changes to this project will be documented in this file.

## [0.22.5](https://github.com/bosun-ai/swiftide/compare/v0.22.4...v0.22.5) - 2025-03-23

### New features

- [eb4e044](https://github.com/bosun-ai/swiftide/commit/eb4e0442293e17722743aa2b88d8dd7582dd9236)  Estimate tokens for OpenAI like apis with tiktoken-rs ([#699](https://github.com/bosun-ai/swiftide/pull/699))

### Miscellaneous

- [345c57a](https://github.com/bosun-ai/swiftide/commit/345c57a663dd0d315a28f0927c5d598ba21d019d)  Improve file loader logging ([#695](https://github.com/bosun-ai/swiftide/pull/695))

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.4...0.22.5



## [0.22.4](https://github.com/bosun-ai/swiftide/compare/v0.22.3...v0.22.4) - 2025-03-17

### Bug fixes

- [4ec00bb](https://github.com/bosun-ai/swiftide/commit/4ec00bb0fed214f27629f32569406bfa2c786dd7) *(integrations)*  Add chrono/utc feature flag when using qdrant ([#684](https://github.com/bosun-ai/swiftide/pull/684))

````text
The Qdrant integration calls chrono::Utc::now(), which requires the now
  feature flag to be enabled in the chrono crate when using qdrant
````

- [0b204d9](https://github.com/bosun-ai/swiftide/commit/0b204d90a68978bb4b75516c537a56d665771c55)  Ensure `groq`, `fastembed`, `test-utils` features compile individually ([#689](https://github.com/bosun-ai/swiftide/pull/689))

### Miscellaneous

- [bd4ef97](https://github.com/bosun-ai/swiftide/commit/bd4ef97f2b9207b5ac03d610b76bdb3440e3d5c0)  Include filenames in errors in file io ([#694](https://github.com/bosun-ai/swiftide/pull/694))

````text
Uses fs-err crate to automatically include filenames in the error
  messages
````

- [9453e06](https://github.com/bosun-ai/swiftide/commit/9453e06d5338c99cec5f51b085739cc30a5f12be)  Use std::sync::Mutex instead of tokio mutex ([#693](https://github.com/bosun-ai/swiftide/pull/693))

- [b3456e2](https://github.com/bosun-ai/swiftide/commit/b3456e25af99f661aff1779ae5f2d4da460f128c)  Log qdrant setup messages at debug level ([#696](https://github.com/bosun-ai/swiftide/pull/696))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.3...0.22.4



## [0.22.3](https://github.com/bosun-ai/swiftide/compare/v0.22.2...v0.22.3) - 2025-03-13

### Miscellaneous

- [834fcd3](https://github.com/bosun-ai/swiftide/commit/834fcd3b2270904bcfe8998a7015de15626128a8)  Update duckdb to 1.2.1 ([#680](https://github.com/bosun-ai/swiftide/pull/680))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.2...0.22.3



## [0.22.2](https://github.com/bosun-ai/swiftide/compare/v0.22.1...v0.22.2) - 2025-03-11

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies

- [e1c097d](https://github.com/bosun-ai/swiftide/commit/e1c097da885374ec9320c1847a7dda7c5d9d41cb)  Disable default features on all dependencies ([#675](https://github.com/bosun-ai/swiftide/pull/675))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.1...0.22.2


# Changelog

All notable changes to this project will be documented in this file.

## [0.22.1](https://github.com/bosun-ai/swiftide/compare/v0.22.0...v0.22.1) - 2025-03-09

### New features

- [474d612](https://github.com/bosun-ai/swiftide/commit/474d6122596e71132e35fcb181302dfed7794561) *(integrations)*  Add Duckdb support ([#578](https://github.com/bosun-ai/swiftide/pull/578))

````text
Adds support for Duckdb. Persist, Retrieve (Simple and Custom), and
  NodeCache are implemented. Metadata and full upsert are not. Once 1.2
  has its issues fixed, it's easy to add.
````

- [4cf417c](https://github.com/bosun-ai/swiftide/commit/4cf417c6a818fbec2641ad6576b4843412902bf6) *(treesitter)*  C and C++ support for splitter only ([#663](https://github.com/bosun-ai/swiftide/pull/663))


### Bug fixes

- [590eaeb](https://github.com/bosun-ai/swiftide/commit/590eaeb3c6b5c14c56c925e038528326f88508a1) *(integrations)*  Make openai parallel_tool_calls an Option ([#664](https://github.com/bosun-ai/swiftide/pull/664))

````text
o3-mini needs to omit parallel_tool_calls - so we need to allow for a
  None option to not include that field
````

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies

- [d864c7e](https://github.com/bosun-ai/swiftide/commit/d864c7e72ba01d3f187e4f6ab6ad3e6244ae0dc4)  Downgrade duckdb to 1.1.1 and fix ci ([#671](https://github.com/bosun-ai/swiftide/pull/671))

- [9b685b3](https://github.com/bosun-ai/swiftide/commit/9b685b3281d9694c5faa58890a9aba32cba90f1c)  Update and loosen deps ([#670](https://github.com/bosun-ai/swiftide/pull/670))

- [a64ca16](https://github.com/bosun-ai/swiftide/commit/a64ca1656b903a680cc70ac7b33ac40d9d356d4a)  Tokio_stream features should include `time`


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.22.0...0.22.1



## [0.22.0](https://github.com/bosun-ai/swiftide/compare/v0.21.1...v0.22.0) - 2025-03-03

### New features

- [a754846](https://github.com/bosun-ai/swiftide/commit/a7548463367023d3e5a3a25dd84f06632b372f18) *(agents)*  Implement Serialize and Deserialize for chat messages

````text
Persist, retry later, evaluate it completions in a script, you name it.
````

- [0a592c6](https://github.com/bosun-ai/swiftide/commit/0a592c67621f3eba4ad6e0bfd5a539e19963cf17) *(indexing)*  Add `iter()` for file loader ([#655](https://github.com/bosun-ai/swiftide/pull/655))

````text
Allows playing with the iterator outside of the stream.

  Relates to https://github.com/bosun-ai/kwaak/issues/337
````

- [57116e9](https://github.com/bosun-ai/swiftide/commit/57116e9a30c722f47398be61838cc1ef4d0bbfac)  Groq ChatCompletion ([#650](https://github.com/bosun-ai/swiftide/pull/650))

````text
Use the new generics to _just-make-it-work_.
````

- [4fd3259](https://github.com/bosun-ai/swiftide/commit/4fd325921555a14552e33b2481bc9dfcf0c313fc)  Continue Agent on Tool Failure ([#628](https://github.com/bosun-ai/swiftide/pull/628))

````text
Ensure tool calls and responses are always balanced, even when the tool retry limit is reached
  https://github.com/bosun-ai/kwaak/issues/313
````

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.21.1...0.22.0



## [0.21.1](https://github.com/bosun-ai/swiftide/compare/v0.21.0...v0.21.1) - 2025-02-28

### Bug fixes

- [f418c5e](https://github.com/bosun-ai/swiftide/commit/f418c5ee2f0d3ee87fb3715ec6b1d7ecc80bf714) *(ci)*  Run just a single real rerank test to please the flaky gods

- [e387e82](https://github.com/bosun-ai/swiftide/commit/e387e826200e1bc0a608e1f680537751cfc17969) *(lancedb)*  Update Lancedb to 0.17 and pin Arrow to a lower version

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.21.0...0.21.1



## [0.21.0](https://github.com/bosun-ai/swiftide/compare/v0.20.1...v0.21.0) - 2025-02-25

### New features

- [12a9873](https://github.com/bosun-ai/swiftide/commit/12a98736ab171c25d860000bb95b1e6e318758fb) *(agents)*  Improve flexibility for tool generation (#641)

````text
Previously ToolSpec and name in the `Tool` trait worked with static.
  With these changes, there is a lot more flexibility, allowing for i.e.
  run-time tool generation.
````

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.20.1...0.21.0



## [0.20.1](https://github.com/bosun-ai/swiftide/compare/v0.20.0...v0.20.1) - 2025-02-21

### Bug fixes

- [0aa1248](https://github.com/bosun-ai/swiftide/commit/0aa124819d836f37d1fcaf88e6f88b5affb46cf9) *(indexing)*  Handle invalid utf-8 in fileloader lossy (#632)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.20.0...0.20.1


# Changelog

All notable changes to this project will be documented in this file.

## [0.20.0](https://github.com/bosun-ai/swiftide/compare/v0.19.0...v0.20.0) - 2025-02-18

### New features

- [5d85d14](https://github.com/bosun-ai/swiftide/commit/5d85d142339d24c793bd89a907652bede0d1c94d) *(agents)*  Add support for numbers, arrays and booleans in tool args (#562)

````text
Add support for numbers, arrays and boolean types in the
  `#[swiftide_macros::tool]` attribute macro. For enum and object a custom
  implementation is now properly supported as well, but not via the macro.
  For now, tools using Derive also still need a custom implementation.
````

- [b09afed](https://github.com/bosun-ai/swiftide/commit/b09afed72d463d8b59ffa2b325eb6a747c88c87f) *(query)*  Add support for reranking with `Fastembed` and multi-document retrieval (#508)


### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.19.0...0.20.0



## [0.19.0](https://github.com/bosun-ai/swiftide/compare/v0.18.2...v0.19.0) - 2025-02-13

### New features

- [fa5112c](https://github.com/bosun-ai/swiftide/commit/fa5112c9224fdf5984d26db669f04dedc8ebb561) *(agents)*  By default retry failed tools with LLM up to 3 times (#609)

````text
Specifically meant for LLMs sending invalid JSON, these tool calls are
  now retried by feeding back the error into the LLM up to a limit
  (default 3).
````

- [14f4778](https://github.com/bosun-ai/swiftide/commit/14f47780b4294be3a9fa3670aa18a952ad7e9d6e) *(integrations)*  Parallel tool calling in OpenAI is now configurable (#611)

````text
Adds support reasoning models in agents and for chat completions.
````

- [37a1a2c](https://github.com/bosun-ai/swiftide/commit/37a1a2c7bfd152db56ed929e0ea1ab99080e640d) *(integrations)*  Add system prompts as `system` instead of message in Anthropic requests

### Bug fixes

- [ab27c75](https://github.com/bosun-ai/swiftide/commit/ab27c75b8f4a971cb61e88b26d94231afd35c871) *(agents)*  Add back anyhow catch all for failed tools

- [2388f18](https://github.com/bosun-ai/swiftide/commit/2388f187966d996ede4ff42c71521238b63d129c) *(agents)*  Use name/arg hash on tool retries (#612)

- [da55664](https://github.com/bosun-ai/swiftide/commit/da5566473e3f8874fce427ceb48a15d002737d07) *(integrations)*  Scraper should stop when finished (#614)

### Miscellaneous

- [990a8ea](https://github.com/bosun-ai/swiftide/commit/990a8eaeffdbd447bb05a0b01aa65a39a7c9cacf) *(deps)*  Update tree-sitter (#616)

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.18.2...0.19.0



## [0.18.2](https://github.com/bosun-ai/swiftide/compare/v0.18.1...v0.18.2) - 2025-02-11

### New features

- [50ffa15](https://github.com/bosun-ai/swiftide/commit/50ffa156e28bb085a61a376bab71c135bc09622f)  Anthropic support for prompts and agents (#602)

### Bug fixes

- [8cf70e0](https://github.com/bosun-ai/swiftide/commit/8cf70e08787d1376ba20001cc9346767d8bd84ef) *(integrations)*  Ensure anthropic tool call format is consistent with specs

### Miscellaneous

- [98176c6](https://github.com/bosun-ai/swiftide/commit/98176c603b61e3971ca5583f9f4346eb5b962d51)  Clippy


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.18.1...0.18.2


# Changelog

All notable changes to this project will be documented in this file.

## [0.18.1](https://github.com/bosun-ai/swiftide/compare/v0.18.0...v0.18.1) - 2025-02-09

### New features

- [78bf0e0](https://github.com/bosun-ai/swiftide/commit/78bf0e004049c852d4e32c0cd67725675b1250f9) *(agents)*  Add optional limit for agent iterations (#599)

- [592e5a2](https://github.com/bosun-ai/swiftide/commit/592e5a2ca4b0f09ba6a9b20cef105539cb7a7909) *(integrations)*  Support Azure openai via generics (#596)

- [c8f2eed](https://github.com/bosun-ai/swiftide/commit/c8f2eed9964341ac2dad611fc730dc234436430a) *(tree-sitter)*  Add solidity support (#597)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.18.0...0.18.1


# Changelog

All notable changes to this project will be documented in this file.

## [0.18.0](https://github.com/bosun-ai/swiftide/compare/v0.17.5...v0.18.0) - 2025-02-01

### New features

- [de46656](https://github.com/bosun-ai/swiftide/commit/de46656f80c5cf68cc192d21b5f34eb3e0667a14) *(agents)*  Add `on_start` hook (#586)

- [c551f1b](https://github.com/bosun-ai/swiftide/commit/c551f1becfd1750ce480a00221a34908db61e42f) *(integrations)*  OpenRouter support (#589)

````text
Adds OpenRouter support. OpenRouter allows you to use any LLM via their
  own api (with a minor upsell).
````

### Bug fixes

- [3ea5839](https://github.com/bosun-ai/swiftide/commit/3ea583971c0d2cc5ef0594eaf764ea149bacd1d8) *(redb)*  Disable per-node tracing

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.lock dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.17.5...0.18.0



## [0.17.5](https://github.com/bosun-ai/swiftide/compare/v0.17.4...v0.17.5) - 2025-01-27

### New features

- [825a52e](https://github.com/bosun-ai/swiftide/commit/825a52e70a74e4621d370485346a78d61bf5d7a9) *(agents)*  Tool description now also accepts paths (i.e. a const) (#580)

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.lock dependencies

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.17.4...0.17.5



## [0.17.4](https://github.com/bosun-ai/swiftide/compare/v0.17.3...v0.17.4) - 2025-01-24

### Bug fixes

- [0d9e250](https://github.com/bosun-ai/swiftide/commit/0d9e250e2512fe9c66d5dfd2ac688dcd56bd07e9) *(tracing)*  Use `or_current()` to prevent orphaned tracing spans (#573)

````text
When a span is emitted that would be selected by the subscriber, but we
  instrument its closure with a span that would not be selected by the
  subscriber, the span would be emitted as an orphan (with a new
  `trace_id`) making them hard to find and cluttering dashboards.

  This situation is also documented here:
  https://docs.rs/tracing/latest/tracing/struct.Span.html#method.or_current
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.17.3...0.17.4



## [0.17.3](https://github.com/bosun-ai/swiftide/compare/v0.17.2...v0.17.3) - 2025-01-24

### New features

- [8e22442](https://github.com/bosun-ai/swiftide/commit/8e2244241f16fff77591cf04f40725ad0b05ca81) *(integrations)*  Support Qdrant 1.13 (#571)

### Bug fixes

- [c5408a9](https://github.com/bosun-ai/swiftide/commit/c5408a96fbed6207022eb493da8d2cbb0fea7ca6) *(agents)*  Io::Error should always be a NonZeroExit error for tool executors (#570)

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.lock dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.17.2...0.17.3



## [0.17.2](https://github.com/bosun-ai/swiftide/compare/v0.17.1...v0.17.2) - 2025-01-21

### Bug fixes

- [47db5ab](https://github.com/bosun-ai/swiftide/commit/47db5ab138384a6c235a90024470e9ab96751cc8) *(agents)*  Redrive uses the correct pointer and works as intended


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.17.1...0.17.2



## [0.17.1](https://github.com/bosun-ai/swiftide/compare/v0.17.0...v0.17.1) - 2025-01-20

### New features

- [e4e4468](https://github.com/bosun-ai/swiftide/commit/e4e44681b65b07b5f1e987ce468bdcda61eb30da) *(agents)*  Implement AgentContext for smart dyn pointers

- [70181d9](https://github.com/bosun-ai/swiftide/commit/70181d9642aa2c0a351b9f42be1a8cdbd83c9075) *(agents)*  Add pub accessor for agent context (#558)

- [274d9d4](https://github.com/bosun-ai/swiftide/commit/274d9d46f39ac2e28361c4881c6f8f7e20dd8753) *(agents)*  Preprocess tool calls to fix common, fixable errors (#560)

````text
OpenAI has a tendency to sometimes send double keys. With this, Swiftide
  will now take the first key and ignore any duplicates after that. Sets the stage for any future preprocessing before it gets strictly parsed by serde.
````

- [0f0f491](https://github.com/bosun-ai/swiftide/commit/0f0f491b2621ad82389a57bdb521fcf4021b7d7a) *(integrations)*  Add Dashscope support  (#543)

````text
---------
````

### Bug fixes

- [b2b15ac](https://github.com/bosun-ai/swiftide/commit/b2b15ac073e4f6b035239791a056fbdf6f6e704e) *(openai)*  Enable strict mode for tool calls (#561)

````text
Ensures openai sticks much better to the schema and avoids accidental
  mistakes.
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.17.0...0.17.1



## [0.17.0](https://github.com/bosun-ai/swiftide/compare/v0.16.4...v0.17.0) - 2025-01-16

### New features

- [835c35e](https://github.com/bosun-ai/swiftide/commit/835c35e7d74811daa90f7ca747054d1919633058) *(agents)*  Redrive completions manually on failure (#551)

````text
Sometimes LLMs fail a completion without deterministic errors, or the
  user case where you just want to retry. `redrive` can now be called on a
  context, popping any new messages (if any), and making the messages
  available again to the agent.
````

- [f83f3f0](https://github.com/bosun-ai/swiftide/commit/f83f3f03bbf6a9591b54521dde91bf1a5ed19c5c) *(agents)*  Implement ToolExecutor for common dyn pointers (#549)

- [7f85735](https://github.com/bosun-ai/swiftide/commit/7f857358e46e825494ba927dffb33c3afa0d762e) *(query)*  Add custom lancedb query generation for lancedb search (#518)

- [ce4e34b](https://github.com/bosun-ai/swiftide/commit/ce4e34be42ce1a0ab69770d03695bd67f99a8739) *(tree-sitter)*  Add golang support (#552)

````text
Seems someone conveniently forgot to add Golang support for the
  splitter.
````

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.lock dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.16.4...0.17.0



## [0.16.4](https://github.com/bosun-ai/swiftide/compare/v0.16.3...v0.16.4) - 2025-01-12

### New features

- [c919484](https://github.com/bosun-ai/swiftide/commit/c9194845faa12b8a0fcecdd65f8ec9d3d221ba08)  Ollama via async-openai with chatcompletion support (#545)

````text
Adds support for chatcompletions (agents) for ollama. SimplePrompt and embeddings now use async-openai underneath.

  Copy pasted as I expect some differences in the future.
````

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.16.3...0.16.4



## [0.16.3](https://github.com/bosun-ai/swiftide/compare/v0.16.2...v0.16.3) - 2025-01-10

### New features

- [b66bd79](https://github.com/bosun-ai/swiftide/commit/b66bd79070772d7e1bfe10a22531ccfd6501fc2a) *(fastembed)*  Add support for jina v2 code (#541)

````text
Add support for jina v2 code in fastembed.
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.16.2...0.16.3



## [0.16.2](https://github.com/bosun-ai/swiftide/compare/v0.16.1...v0.16.2) - 2025-01-08

### Bug fixes

- [2226755](https://github.com/bosun-ai/swiftide/commit/2226755f367d9006870a2dea2063655a7901d427)  Explicit cast on tools to Box<dyn> to make analyzer happy (#536)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.16.1...0.16.2



## [0.16.1](https://github.com/bosun-ai/swiftide/compare/v0.16.0...v0.16.1) - 2025-01-06

### Bug fixes

- [d198bb0](https://github.com/bosun-ai/swiftide/commit/d198bb0807f5d5b12a51bc76721cc945be8e65b9) *(prompts)*  Skip rendering prompts if no context and forward as is (#530)

````text
Fixes an issue if strings suddenly include jinja style values by
  mistake. Bonus performance boost.
````

- [4e8d59f](https://github.com/bosun-ai/swiftide/commit/4e8d59fbc0fbe72dd0f8d6a95e6e335280eb88e3) *(redb)*  Log errors and return uncached instead of panicing (#531)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.16.0...0.16.1



## [0.16.0](https://github.com/bosun-ai/swiftide/compare/v0.15.0...v0.16.0) - 2025-01-02

### New features

- [52e341e](https://github.com/bosun-ai/swiftide/commit/52e341ee9777d04f9fb07054980ba087c55c033e) *(lancedb)*  Public method for opening table (#514)

- [3254bd3](https://github.com/bosun-ai/swiftide/commit/3254bd34d0eeb038c8aa6ea56ac2940b3ca81960) *(query)*  Generic templates with document rendering (#520)

````text
Reworks `PromptTemplate` to a more generic `Template`, such that they
  can also be used elsewhere. This deprecates `PromptTemplate`.

  As an example, an optional `Template` in the `Simple` answer
  transformer, which can be used to customize the output of retrieved
  documents. This has excellent synergy with the metadata changes in #504.
````

- [235780b](https://github.com/bosun-ai/swiftide/commit/235780b941a0805b69541f0f4c55c3404091baa8) *(query)*  Documents as first class citizens (#504)

````text
For simple RAG, just adding the content of a retrieved document might be
  enough. However, in more complex use cases, you might want to add
  metadata as well, as is or for conditional formatting.

  For instance, when dealing with large amounts of chunked code, providing
  the path goes a long way. If generated metadata is good enough, could be
  useful as well.

  With this retrieved Documents are treated as first class citizens,
  including any metadata as well. Additionally, this also paves the way
  for multi retrieval (and multi modal).
````

- [584695e](https://github.com/bosun-ai/swiftide/commit/584695e4841a3c9341e521b81e9f254270b3416e) *(query)*  Add custom SQL query generation for pgvector search (#478)

````text
Adds support for custom retrieval queries with the sqlx query builder for PGVector. Puts down the fundamentals for custom query building for any retriever.

  ---------
````

- [b55bf0b](https://github.com/bosun-ai/swiftide/commit/b55bf0b318042459a6983cf725078c4da662618b) *(redb)*  Public database and table definition (#510)

- [176378f](https://github.com/bosun-ai/swiftide/commit/176378f846ddecc3ddba74f6b423338b793f29b4)  Implement traits for all Arc dynamic dispatch (#513)

````text
If you use i.e. a `Persist` or a `NodeCache` outside swiftide as well, and you already have it Arc'ed, now it just works.
````

- [dc9881e](https://github.com/bosun-ai/swiftide/commit/dc9881e48da7fb5dc744ef33b1c356b4152d00d3)  Allow opt out of pipeline debug truncation

### Bug fixes

- [2831101](https://github.com/bosun-ai/swiftide/commit/2831101daa2928b5507116d9eb907d98fb77bf50) *(lancedb)*  Metadata should be nullable in lancedb (#515)

- [c35df55](https://github.com/bosun-ai/swiftide/commit/c35df5525d4d88cfb9ada89a060e1ab512b471af) *(macros)*  Explicit box dyn cast fixing Rust Analyzer troubles (#523)

### Miscellaneous

- [1bbbb0e](https://github.com/bosun-ai/swiftide/commit/1bbbb0e548cafa527c34856bd9ac6f76aca2ab5f)  Clippy


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.15.0...0.16.0



## [0.15.0](https://github.com/bosun-ai/swiftide/compare/v0.14.4...v0.15.0) - 2024-12-23

### New features

- [a1b9a2d](https://github.com/bosun-ai/swiftide/commit/a1b9a2d37715420d3e2cc80d731e3713a22c7c50) *(query)*  Ensure concrete names for transformations are used when debugging (#496)

- [7779c44](https://github.com/bosun-ai/swiftide/commit/7779c44de3581ac865ac808637c473525d27cabb) *(query)*  Ensure query pipeline consistently debug logs in all other stages too

- [55dde88](https://github.com/bosun-ai/swiftide/commit/55dde88df888b60a7ccae5a68ba03d20bc1f57df) *(query)*  Debug full retrieved documents when debug mode is enabled (#495)

- [66031ba](https://github.com/bosun-ai/swiftide/commit/66031ba27b946add0533775423d468abb3187604) *(query)*  Log query pipeline answer on debug (#497)

### Miscellaneous

- [d255772](https://github.com/bosun-ai/swiftide/commit/d255772cc933c839e3aaaffccd343acf75dcb251) *(agents)*  Rename `CommandError::FailedWithOutput` to `CommandError::NonZeroExit` (#484)

````text
Better describes what is going on. I.e. `rg` exits with 1 if nothing is
  found, tests generally do the same if they fail.
````

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.14.4...0.15.0



## [0.14.4](https://github.com/bosun-ai/swiftide/compare/v0.14.3...v0.14.4) - 2024-12-11

### New features

- [7211559](https://github.com/bosun-ai/swiftide/commit/7211559936d8b5e16a3b42f9c90b42a39426be8a) *(agents)*  **EXPERIMENTAL** Agents in Swiftide (#463)

````text
Agents are coming to Swiftide! We are still ironing out all the kinks,
  while we make it ready for a proper release. You can already experiment
  with agents, see the rustdocs for documentation, and an example in
  `/examples`, and feel free to contact us via github or discord. Better
  documentation, examples, and tutorials are coming soon.

  Run completions in a loop, define tools with two handy macros, customize
  the agent by hooking in on lifecycle events, and much more.

  Besides documentation, expect a big release for what we build this for
  soon! 🎉
````

- [3751f49](https://github.com/bosun-ai/swiftide/commit/3751f49201c71398144a8913a4443f452534def2) *(query)*  Add support for single embedding retrieval with PGVector (#406)

### Miscellaneous

- [5ce4d21](https://github.com/bosun-ai/swiftide/commit/5ce4d21725ff9b0bb7f9da8fe026075fde9fc9a5)  Clippy and deps fixes for 1.83 (#467)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.14.3...0.14.4



## [0.14.3](https://github.com/bosun-ai/swiftide/compare/v0.14.2...v0.14.3) - 2024-11-20

### New features

- [1774b84](https://github.com/bosun-ai/swiftide/commit/1774b84f00a83fe69af4a2b6a6daf397d4d9b32d) *(integrations)*  Add PGVector support for indexing ([#392](https://github.com/bosun-ai/swiftide/pull/392))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.14.2...0.14.3



## [0.14.2](https://github.com/bosun-ai/swiftide/compare/v0.14.1...v0.14.2) - 2024-11-08

### Bug fixes

- [3924322](https://github.com/bosun-ai/swiftide/commit/39243224d739a76cf2b60204fc67819055b7bc6f) *(querying)*  Query pipeline is now properly send and sync when possible ([#425](https://github.com/bosun-ai/swiftide/pull/425))

### Miscellaneous

- [52198f7](https://github.com/bosun-ai/swiftide/commit/52198f7fe76376a42c1fec8945bda4bf3e6971d4)  Improve local dev build speed ([#434](https://github.com/bosun-ai/swiftide/pull/434))

````text
- **Tokio on rt-multi-thread only**
  - **Remove manual checks from lancedb integration test**
  - **Ensure all deps in workspace manifest**
  - **Remove unused deps**
  - **Remove examples and benchmarks from default members**
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.14.1...0.14.2



## [0.14.1](https://github.com/bosun-ai/swiftide/compare/v0.14.0...v0.14.1) - 2024-10-27

### Bug fixes

- [5bbcd55](https://github.com/bosun-ai/swiftide/commit/5bbcd55de65d73d7908e91c96f120928edb6b388)  Revert 0.14 release as mistralrs is unpublished ([#417](https://github.com/bosun-ai/swiftide/pull/417))

````text
Revert the 0.14 release as `mistralrs` is unpublished and unfortunately
  cannot be released.
````

### Miscellaneous

- [07c2661](https://github.com/bosun-ai/swiftide/commit/07c2661b7a7cdf75cdba12fab0ca91866793f727)  Re-release 0.14 without mistralrs ([#419](https://github.com/bosun-ai/swiftide/pull/419))

````text
- **Revert "fix: Revert 0.14 release as mistralrs is unpublished
  ([#417](https://github.com/bosun-ai/swiftide/pull/417))"**
  - **Fix changelog**
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.14.0...0.14.1



## [0.14.0](https://github.com/bosun-ai/swiftide/compare/v0.13.4...v0.14.0) - 2024-10-27

### Bug fixes

- [551a9cb](https://github.com/bosun-ai/swiftide/commit/551a9cb769293e42e15bae5dca3ab677be0ee8ea) *(indexing)*  [**breaking**] Node ID no longer memoized ([#414](https://github.com/bosun-ai/swiftide/pull/414))

````text
As @shamb0 pointed out in [#392](https://github.com/bosun-ai/swiftide/pull/392), there is a potential issue where Node
  ids are get cached before chunking or other transformations, breaking
  upserts and potentially resulting in data loss.
````

**BREAKING CHANGE**: This PR reworks Nodes with a builder API and a private
id. Hence, manually creating nodes no longer works. In the future, all
the fields are likely to follow the same pattern, so that we can
decouple the inner fields from the Node's implementation.

- [c091ffa](https://github.com/bosun-ai/swiftide/commit/c091ffa6be792b0bd7bb03d604e26e40b2adfda8) *(indexing)*  Use atomics for key generation in memory storage ([#415](https://github.com/bosun-ai/swiftide/pull/415))

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.13.4...0.14.0



## [0.13.4](https://github.com/bosun-ai/swiftide/compare/v0.13.3...v0.13.4) - 2024-10-21

### Bug fixes

- [47455fb](https://github.com/bosun-ai/swiftide/commit/47455fb04197a4b51142e2fb4c980e42ac54d11e) *(indexing)*  Visibility of ChunkMarkdown builder should be public

- [2b3b401](https://github.com/bosun-ai/swiftide/commit/2b3b401dcddb2cb32214850b9b4dbb0481943d38) *(indexing)*  Improve splitters consistency and provide defaults ([#403](https://github.com/bosun-ai/swiftide/pull/403))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.13.3...0.13.4


# Changelog

All notable changes to this project will be documented in this file.

## [0.13.3](https://github.com/bosun-ai/swiftide/compare/v0.13.2...v0.13.3) - 2024-10-11

### Bug fixes

- [2647f16](https://github.com/bosun-ai/swiftide/commit/2647f16dc164eb5230d8f7c6d71e31663000cb0d) *(deps)*  Update rust crate text-splitter to 0.17 ([#366](https://github.com/bosun-ai/swiftide/pull/366))

- [d74d85b](https://github.com/bosun-ai/swiftide/commit/d74d85be3bd98706349eff373c16443b9c45c4f0) *(indexing)*  Add missing `Embed::batch_size` implementation ([#378](https://github.com/bosun-ai/swiftide/pull/378))

- [95f78d3](https://github.com/bosun-ai/swiftide/commit/95f78d3412951c099df33149c57817338a76553d) *(tree-sitter)*  Compile regex only once ([#371](https://github.com/bosun-ai/swiftide/pull/371))

````text
Regex compilation is not cheap, use a static with a oncelock instead.
````

### Miscellaneous

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.13.2...0.13.3



## [0.13.2](https://github.com/bosun-ai/swiftide/compare/v0.13.1...v0.13.2) - 2024-10-05

### New features

- [4b13aa7](https://github.com/bosun-ai/swiftide/commit/4b13aa7d76dfc7270870682e2f757f066a99ba4e) *(core)*  Add support for cloning all trait objects ([#355](https://github.com/bosun-ai/swiftide/pull/355))

````text
For instance, if you have a `Box<dyn SimplePrompt>`, you can now clone
  into an owned copy and more effectively use the available generics. This
  also works for borrowed trait objects.
````

- [ed3da52](https://github.com/bosun-ai/swiftide/commit/ed3da52cf89b2384ec6f07c610c591b3eda2fa28) *(indexing)*  Support Redb as embedable nodecache ([#346](https://github.com/bosun-ai/swiftide/pull/346))

````text
Adds support for Redb as an embeddable node cache, allowing full local
  app development without needing external services.
````

### Bug fixes

- [06f8336](https://github.com/bosun-ai/swiftide/commit/06f83361c52010a451e8b775ce9c5d67057edbc5) *(indexing)*  Ensure `name()` returns concrete name on trait objects ([#351](https://github.com/bosun-ai/swiftide/pull/351))

### Miscellaneous

- [8237c28](https://github.com/bosun-ai/swiftide/commit/8237c2890df681c48117188e80cbad914b91e0fd) *(core)*  Mock traits for testing should not have their docs hidden

- [0000000](https://github.com/bosun-ai/swiftide/commit/0000000)  Update Cargo.toml dependencies


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.13.1...0.13.2



## [0.13.1](https://github.com/bosun-ai/swiftide/compare/v0.13.0...v0.13.1) - 2024-10-02

### Bug fixes

- [e6d9ec2](https://github.com/bosun-ai/swiftide/commit/e6d9ec2fe034c9d36fd730c969555c459606d42f) *(lancedb)*  Should not error if table exists ([#349](https://github.com/bosun-ai/swiftide/pull/349))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.13.0...0.13.1



## [0.13.0](https://github.com/bosun-ai/swiftide/compare/v0.12.3...v0.13.0) - 2024-09-26

### New features

- [7d8a57f](https://github.com/bosun-ai/swiftide/commit/7d8a57f54b2c73267dfaa3b3a32079b11d9b32bc) *(indexing)*  [**breaking**] Removed duplication of batch_size ([#336](https://github.com/bosun-ai/swiftide/pull/336))

**BREAKING CHANGE**: The batch size of batch transformers when indexing is
now configured on the batch transformer. If no batch size or default is
configured, a configurable default is used from the pipeline. The
default batch size is 256.

- [fd110c8](https://github.com/bosun-ai/swiftide/commit/fd110c8efeb3af538d4e51d033b6df02e90e05d9) *(tree-sitter)*  Add support for Java 22 ([#309](https://github.com/bosun-ai/swiftide/pull/309))

### Bug fixes

- [23b96e0](https://github.com/bosun-ai/swiftide/commit/23b96e08b4e0f10f5faea0b193b404c9cd03f47f) *(tree-sitter)* [**breaking**]  SupportedLanguages are now non-exhaustive ([#331](https://github.com/bosun-ai/swiftide/pull/331))

**BREAKING CHANGE**: SupportedLanguages are now non-exhaustive. This means that matching on SupportedLanguages will now require a catch-all arm.
This change was made to allow for future languages to be added without breaking changes.

### Miscellaneous

- [923a8f0](https://github.com/bosun-ai/swiftide/commit/923a8f0663e7d2b7138f54069f7a74c3cf6663ed) *(fastembed,qdrant)*  Better batching defaults ([#334](https://github.com/bosun-ai/swiftide/pull/334))

```text
Qdrant and FastEmbed now have a default batch size, removing the need to set it manually. The default batch size is 50 and 256 respectively.
```

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.12.3...0.13.0



## [0.12.3](https://github.com/bosun-ai/swiftide/releases/tag/0.12.3) - 2024-09-23

### New features

- [da5df22](https://github.com/bosun-ai/swiftide/commit/da5df2230da81e9fe1e6ab74150511cbe1e3d769) *(tree-sitter)*  Implement Serialize and Deserialize for SupportedLanguages ([#314](https://github.com/bosun-ai/swiftide/pull/314))

### Bug fixes

- [a756148](https://github.com/bosun-ai/swiftide/commit/a756148f85faa15b1a79db8ec8106f0e15e4d6a2) *(tree-sitter)*  Fix javascript and improve tests ([#313](https://github.com/bosun-ai/swiftide/pull/313))

````text
As learned from [#309](https://github.com/bosun-ai/swiftide/pull/309), test coverage for the refs defs transformer was
  not great. There _are_ more tests in code_tree. Turns out, with the
  latest treesitter update, javascript broke as it was the only language
  not covered at all.
````

### Miscellaneous

- [e8e9d80](https://github.com/bosun-ai/swiftide/commit/e8e9d80f2b4fbfe7ca2818dc542ca0a907a17da5) *(docs)*  Add documentation to query module ([#276](https://github.com/bosun-ai/swiftide/pull/276))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.12.2...0.12.3




## [v0.12.2](https://github.com/bosun-ai/swiftide/releases/tag/v0.12.2) - 2024-09-20

### Docs

- [d84814e](https://github.com/bosun-ai/swiftide/commit/d84814eef1bf12e485053fb69fb658d963100789)  Fix broken documentation links and other cargo doc warnings (#304) by @tinco

````text
Running `cargo doc --all-features` resulted in a lot of warnings.
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.12.1...v0.12.2


## [v0.12.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.12.1) - 2024-09-16

### New features

- [ec227d2](https://github.com/bosun-ai/swiftide/commit/ec227d25b987b7fd63ab1b3862ef19b14632bd04) *(indexing,query)*  Add concise info log with transformation name by @timonv

- [01cf579](https://github.com/bosun-ai/swiftide/commit/01cf579922a877bb78e0de20114ade501e5a63db) *(query)*  Add query_mut for reusable query pipelines by @timonv

- [081a248](https://github.com/bosun-ai/swiftide/commit/081a248e67292c1800837315ec53583be5e0cb82) *(query)*  Improve query performance similar to indexing in 0.12 by @timonv

- [8029926](https://github.com/bosun-ai/swiftide/commit/80299269054eb440e55a42667a7bcc9ba6514a7b) *(query,indexing)*  Add duration in log output on pipeline completion by @timonv

### Bug fixes

- [39b6ecb](https://github.com/bosun-ai/swiftide/commit/39b6ecb6175e5233b129f94876f95182b8bfcdc3) *(core)*  Truncate long strings safely when printing debug logs by @timonv

- [8b8ceb9](https://github.com/bosun-ai/swiftide/commit/8b8ceb9266827857859481c1fc4a0f0c40805e33) *(deps)*  Update redis by @timonv

- [16e9c74](https://github.com/bosun-ai/swiftide/commit/16e9c7455829100b9ae82305e5a1d2568264af9f) *(openai)*  Reduce debug verbosity by @timonv

- [6914d60](https://github.com/bosun-ai/swiftide/commit/6914d607717294467cddffa867c3d25038243fc1) *(qdrant)*  Reduce debug verbosity when storing nodes by @timonv

- [3d13889](https://github.com/bosun-ai/swiftide/commit/3d1388973b5e2a135256ae288d47dbde0399487f) *(query)*  Reduce and improve debugging verbosity by @timonv

- [133cf1d](https://github.com/bosun-ai/swiftide/commit/133cf1d0be09049ca3e90b45675a965bb2464cb2) *(query)*  Remove verbose debug and skip self in instrumentation by @timonv

- [ce17981](https://github.com/bosun-ai/swiftide/commit/ce179819ab75460453236723c7f9a89fd61fb99a)  Clippy by @timonv

- [a871c61](https://github.com/bosun-ai/swiftide/commit/a871c61ad52ed181d6f9cb6a66ed07bccaadee08)  Fmt by @timonv

### Miscellaneous

- [d62b047](https://github.com/bosun-ai/swiftide/commit/d62b0478872e460956607f52b72470b76eb32d91) *(ci)*  Update testcontainer images and fix tests by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.12.0...v0.12.1


## [v0.12.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.12.0) - 2024-09-13

### New features

- [e902cb7](https://github.com/bosun-ai/swiftide/commit/e902cb7487221d3e88f13d88532da081e6ef8611) *(query)*  Add support for filters in SimilaritySingleEmbedding (#298) by @timonv

````text
Adds support for filters for Qdrant and Lancedb in
  SimilaritySingleEmbedding. Also fixes several small bugs and brings
  improved tests.
````

- [f158960](https://github.com/bosun-ai/swiftide/commit/f1589604d1e0cb42a07d5a48080e3d7ecb90ee38)  Major performance improvements (#291) by @timonv

````text
Futures that do not yield were not run in parallel properly. With this
  futures are spawned on a tokio worker thread by default.

  When embedding (fastembed) and storing a 85k row dataset, there's a
  ~1.35x performance improvement:
  <img width="621" alt="image"
  src="https://github.com/user-attachments/assets/ba2d4d96-8d4a-44f1-b02d-6ac2af0cedb7">

  ~~Need to do one more test with IO bound futures as well. Pretty huge,
  not that it was slow.~~

  With IO bound openai it's 1.5x.
````

### Bug fixes

- [f8314cc](https://github.com/bosun-ai/swiftide/commit/f8314ccdbe16ad7e6691899dd01f81a61b20180f) *(indexing)*  Limit logged chunk to max 100 chars (#292) by @timonv

- [f95f806](https://github.com/bosun-ai/swiftide/commit/f95f806a0701b14a3cad5da307c27c01325a264d) *(indexing)*  Debugging nodes should respect utf8 char boundaries by @timonv

- [8595553](https://github.com/bosun-ai/swiftide/commit/859555334d7e4129215b9f084d9f9840fac5ce36)  Implement into_stream_boxed for all loaders by @timonv

- [9464ca1](https://github.com/bosun-ai/swiftide/commit/9464ca123f08d8dfba3f1bfabb57e9af97018534)  Bad embed error propagation (#293) by @timonv

````text
- **fix(indexing): Limit logged chunk to max 100 chars**
  - **fix: Embed transformers must correctly propagate errors**
````

### Miscellaneous

- [45d8a57](https://github.com/bosun-ai/swiftide/commit/45d8a57d1afb4f16ad76b15236308d753cf45743) *(ci)*  Use llm-cov preview via nightly and improve test coverage (#289) by @timonv

````text
Fix test coverage in CI. Simplified the trait bounds on the query
  pipeline for now to make it all work and fit together, and added more
  tests to assert boxed versions of trait objects work in tests.
````

- [408f30a](https://github.com/bosun-ai/swiftide/commit/408f30ad8d007394ba971b314d399fcd378ffb61) *(deps)*  Update testcontainers (#295) by @timonv

- [37c4bd9](https://github.com/bosun-ai/swiftide/commit/37c4bd9f9ac97646adb2c4b99b8f7bf0bee4c794) *(deps)*  Update treesitter (#296) by @timonv

- [8d9e954](https://github.com/bosun-ai/swiftide/commit/8d9e9548ccc1b39e302ee42dd5058f50df13270f)  Cargo update by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.11.1...v0.12.0


## [v0.11.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.11.1) - 2024-09-10

### New features

- [3c9491b](https://github.com/bosun-ai/swiftide/commit/3c9491b8e1ce31a030eaac53f56890629a087f70)  Implemtent traits T for Box<T> for indexing and query traits (#285) by @timonv

````text
When working with trait objects, some pipeline steps now allow for
  Box<dyn Trait> as well.
````

### Bug fixes

- [dfa546b](https://github.com/bosun-ai/swiftide/commit/dfa546b310e71a7cb78a927cc8f0ee4e2046a592)  Add missing parquet feature flag by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.11.0...v0.11.1


## [v0.11.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.11.0) - 2024-09-08

### New features

- [bdf17ad](https://github.com/bosun-ai/swiftide/commit/bdf17adf5d3addc84aaf45ad893b816cb46431e3) *(indexing)*  Parquet loader (#279) by @timonv

````text
Ingest and index data from parquet files.
````

- [a98dbcb](https://github.com/bosun-ai/swiftide/commit/a98dbcb455d33f0537cea4d3614da95f1a4b6554) *(integrations)*  Add ollama embeddings support (#278) by @ephraimkunz

````text
Update to the most recent ollama-rs, which exposes the batch embedding
  API Ollama exposes (https://github.com/pepperoni21/ollama-rs/pull/61).
  This allows the Ollama struct in Swiftide to implement `EmbeddingModel`.

  Use the same pattern that the OpenAI struct uses to manage separate
  embedding and prompt models.

  ---------
````

### Miscellaneous

- [873795b](https://github.com/bosun-ai/swiftide/commit/873795b31b3facb0cf5efa724cb391f7bf387fb0) *(ci)*  Re-enable coverage via Coverals with tarpaulin (#280) by @timonv

- [465de7f](https://github.com/bosun-ai/swiftide/commit/465de7fc952d66f4cd15002ef39aab0e7ec3ac26)  Update CHANGELOG.md with breaking change by @timonv

### New Contributors
* @ephraimkunz made their first contribution in [#278](https://github.com/bosun-ai/swiftide/pull/278)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.10.0...v0.11.0


## [v0.10.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.10.0) - 2024-09-06

### Bug fixes

- [5a724df](https://github.com/bosun-ai/swiftide/commit/5a724df895d35cfa606721d611afd073a23191de)  [**breaking**] Rust 1.81 support (#275) by @timonv

````text
Fixing id generation properly as per #272, will be merged in together.

  - **Clippy**
  - **fix(qdrant)!: Default hasher changed in Rust 1.81**
````

**BREAKING CHANGE**: Rust 1.81 support (#275)

### Docs

- [3711f6f](https://github.com/bosun-ai/swiftide/commit/3711f6fb2b51e97e4606b744cc963c04b44b6963) *(readme)*  Fix date (#273) by @dzvon

````text
I suppose this should be 09-02.
````

### New Contributors
* @dzvon made their first contribution in [#273](https://github.com/bosun-ai/swiftide/pull/273)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.9.2...v0.10.0


## [v0.9.2](https://github.com/bosun-ai/swiftide/releases/tag/v0.9.2) - 2024-09-04

### New features

- [84e9bae](https://github.com/bosun-ai/swiftide/commit/84e9baefb366f0a949ae7dcbdd8f97931da0b4be) *(indexing)*  Add chunker for text with text_splitter (#270) by @timonv

- [387fbf2](https://github.com/bosun-ai/swiftide/commit/387fbf29c2bce06284548f9af146bb3969562761) *(query)*  Hybrid search for qdrant in query pipeline (#260) by @timonv

````text
Implement hybrid search for qdrant with their new Fusion search. Example
  in /examples includes an indexing and query pipeline, included the
  example answer as well.
````

### Docs

- [064c7e1](https://github.com/bosun-ai/swiftide/commit/064c7e157775a7aaf9628a39f941be35ce0be99a) *(readme)*  Update intro by @timonv

- [1dc4c90](https://github.com/bosun-ai/swiftide/commit/1dc4c90436c9c8c8d0eb080e300afce53090c73e) *(readme)*  Add new blog links by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.9.1...v0.9.2


## [v0.9.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.9.1) - 2024-09-01

### New features

- [b891f93](https://github.com/bosun-ai/swiftide/commit/b891f932e43b9c76198d238bcde73a6bb1dfbfdb) *(integrations)*  Add fluvio as loader support (#243) by @timonv

````text
Adds Fluvio as a loader support, enabling Swiftide indexing streams to
  process messages from a Fluvio topic.
````

- [c00b6c8](https://github.com/bosun-ai/swiftide/commit/c00b6c8f08fca46451387f3034d3d53805f3e401) *(query)*  Ragas support (#236) by @timonv

````text
Work in progress on support for ragas as per
  https://github.com/explodinggradients/ragas/issues/1165 and #232

  Add an optional evaluator to a pipeline. Evaluators need to handle
  transformation events in the query pipeline. The Ragas evaluator
  captures the transformations as per
  https://docs.ragas.io/en/latest/howtos/applications/data_preparation.html.

  You can find a working notebook here
  https://github.com/bosun-ai/swiftide-tutorial/blob/c510788a625215f46575415161659edf26fc1fd5/ragas/notebook.ipynb
  with a pipeline using it here
  https://github.com/bosun-ai/swiftide-tutorial/pull/1
````

- [a1250c1](https://github.com/bosun-ai/swiftide/commit/a1250c1cef57e2b74760fd31772e106993a3b079)  LanceDB support (#254) by @timonv

````text
Add LanceDB support for indexing and querying. LanceDB separates compute
  from storage, where storage can be local or hosted elsewhere.
````

### Bug fixes

- [f92376d](https://github.com/bosun-ai/swiftide/commit/f92376d551a3bf4fe39d81a64c4328a742677669) *(deps)*  Update rust crate aws-sdk-bedrockruntime to v1.46.0 (#247) by @renovate[bot]

- [732a166](https://github.com/bosun-ai/swiftide/commit/732a166f388d4aefaeec694103e3d1ff57655d69)  Remove no default features from futures-util by @timonv

### Miscellaneous

- [9b257da](https://github.com/bosun-ai/swiftide/commit/9b257dadea6c07f720ac4ea447342b2f6d91d0ec)  Default features cleanup (#262) by @timonv

````text
Integrations are messy and pull a lot in. A potential solution is to
  disable default features, only add what is actually required, and put
  the responsibility at users if they need anything specific. Feature
  unification should then take care of the rest.
````

### Docs

- [fb381b8](https://github.com/bosun-ai/swiftide/commit/fb381b8896a5fc863a4185445ce51fefb99e6c11) *(readme)*  Copy improvements (#261) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.9.0...v0.9.1


## [v0.9.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.9.0) - 2024-08-15

### New features

- [2443933](https://github.com/bosun-ai/swiftide/commit/24439339a9b935befcbcc92e56c01c5048605138) *(qdrant)*  Add access to inner client for custom operations (#242) by @timonv

- [4fff613](https://github.com/bosun-ai/swiftide/commit/4fff613b461e8df993327cb364cabc65cd5901d8) *(query)*  Add concurrency on query pipeline and add query_all by @timonv

### Bug fixes

- [4e31c0a](https://github.com/bosun-ai/swiftide/commit/4e31c0a6cdc6b33e4055f611dc48d3aebf7514ae) *(deps)*  Update rust crate aws-sdk-bedrockruntime to v1.44.0 (#244) by @renovate[bot]

- [501321f](https://github.com/bosun-ai/swiftide/commit/501321f811a0eec8d1b367f7c7f33b1dfd29d2b6) *(deps)*  Update rust crate spider to v1.99.37 (#230) by @renovate[bot]

- [8a1cc69](https://github.com/bosun-ai/swiftide/commit/8a1cc69712b4361893c0564c7d6f7d1ed21e5710) *(query)*  After retrieval current transormation should be empty by @timonv

### Miscellaneous

- [e9d0016](https://github.com/bosun-ai/swiftide/commit/e9d00160148807a8e2d1df1582e6ea85cfd2d8d0) *(indexing,integrations)*  Move tree-sitter dependencies to integrations (#235) by @timonv

````text
Removes the dependency of indexing on integrations, resulting in much
  faster builds when developing on indexing.
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.8.0...v0.9.0


## [v0.8.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.8.0) - 2024-08-12

### New features

- [2e25ad4](https://github.com/bosun-ai/swiftide/commit/2e25ad4b999a8562a472e086a91020ec4f8300d8) *(indexing)*  [**breaking**] Default LLM for indexing pipeline and boilerplate Transformer macro (#227) by @timonv

````text
Add setting a default LLM for an indexing pipeline, avoiding the need to
  clone multiple times.

  More importantly, introduced `swiftide-macros` with
  `#[swiftide_macros::indexing_transformer]` that generates
  all boilerplate code used for internal transformers. This ensures all
  transformers are consistent and makes them
  easy to change in the future. This is a big win for maintainability and
  ease to extend. Users are encouraged to use the macro
  as well.
````

**BREAKING CHANGE**: Introduces `WithIndexingDefaults` and
`WithBatchIndexingDefaults` trait constraints for transformers. They can
be used as a marker
with a noop (i.e. just `impl WithIndexingDefaults for MyTransformer
{}`). However, when implemented fully, they can be used to provide
defaults from the pipeline to your transformers.

- [67336f1](https://github.com/bosun-ai/swiftide/commit/67336f1d9c7fde474bdddfd0054b40656df244e0) *(indexing)*  Sparse vector support with Splade and Qdrant (#222) by @timonv

````text
Adds Sparse vector support to the indexing pipeline, enabling hybrid
  search for vector databases. The design should work for any form of
  Sparse embedding, and works with existing embedding modes and multiple
  named vectors. Additionally, added `try_default_sparse` to FastEmbed,
  using Splade, so it's fully usuable.

  Hybrid search in the query pipeline coming soon.
````

- [e728a7c](https://github.com/bosun-ai/swiftide/commit/e728a7c7a2fcf7b22c31e5d6c66a896f634f6901)  Code outlines in chunk metadata (#137) by @tinco

````text
Added a transformer that generates outlines for code files using tree sitter. And another that compresses the outline to be more relevant to chunks. Additionally added a step to the metadata QA tool that uses the outline to improve the contextual awareness during QA generation.
````

### Bug fixes

- [dc7412b](https://github.com/bosun-ai/swiftide/commit/dc7412beda4377e8a6222b3ad576f0a1af332533) *(deps)*  Update aws-sdk-rust monorepo (#223) by @renovate[bot]

### Miscellaneous

- [9613f50](https://github.com/bosun-ai/swiftide/commit/9613f50c0036b42411cd3a3014f54b592fe4958a) *(ci)*  Only show remote github url if present in changelog by @timonv

### Docs

- [73d1649](https://github.com/bosun-ai/swiftide/commit/73d1649ca8427aa69170f6451eac55316581ed9a) *(readme)*  Add Ollama support to README by @timonv

- [b3f04de](https://github.com/bosun-ai/swiftide/commit/b3f04defe94e5b26876c8d99049f4d87b5f2dc18) *(readme)*  Add link to discord (#219) by @timonv

- [4970a68](https://github.com/bosun-ai/swiftide/commit/4970a683acccc71503e64044dc02addaf2e9c87c) *(readme)*  Fix discord links by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.7.1...v0.8.0


## [v0.7.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.7.1) - 2024-08-04

### New features

- [b2d31e5](https://github.com/bosun-ai/swiftide/commit/b2d31e555cb8da525513490e7603df1f6b2bfa5b) *(integrations)*  Add ollama support (#214) by @tinco

- [9eb5894](https://github.com/bosun-ai/swiftide/commit/9eb589416c2a56f9942b6f6bed3771cec6acebaf) *(query)*  Add support for closures in all steps (#215) by @timonv

### Miscellaneous

- [53e662b](https://github.com/bosun-ai/swiftide/commit/53e662b8c30f6ac6d11863685d3850ab48397766) *(ci)*  Add cargo deny to lint dependencies (#213) by @timonv

### Docs

- [1539393](https://github.com/bosun-ai/swiftide/commit/15393932dd756af134a12f7954faa75893f8c3fb) *(readme)*  Update README.md by @timonv

- [ba07ab9](https://github.com/bosun-ai/swiftide/commit/ba07ab93722d974ac93ed5d4a22bf53317bc11ae) *(readme)*  Readme improvements by @timonv

- [f7accde](https://github.com/bosun-ai/swiftide/commit/f7accdeecf01efc291503282554257846725ce57) *(readme)*  Add 0.7 announcement by @timonv

- [084548f](https://github.com/bosun-ai/swiftide/commit/084548f0fbfbb8cf6d359585f30c8e2593565681) *(readme)*  Clarify on closures by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.7.0...v0.7.1


## [swiftide-v0.7.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.7.0) - 2024-07-28

### New features

- [ec1fb04](https://github.com/bosun-ai/swiftide/commit/ec1fb04573ab75fe140cbeff17bc3179e316ff0c) *(indexing)*  Metadata as first class citizen (#204) by @timonv

````text
Adds our own implementation for metadata, internally still using a
  BTreeMap. The Value type is now a `serde_json::Value` enum. This allows
  us to store the metadata in the same format as the rest of the document,
  and also allows us to use values programmatically later.

  As is, all current meta data is still stored as Strings.
````

- [16bafe4](https://github.com/bosun-ai/swiftide/commit/16bafe4da8c98adcf90f5bb63070832201c405b9) *(swiftide)*  [**breaking**] Rework workspace preparing for swiftide-query (#199) by @timonv

````text
Splits up the project into multiple small, unpublished crates. Boosts
  compile times, makes the code a bit easier to grok and enables
  swiftide-query to be build separately.
````

**BREAKING CHANGE**: All indexing related tools are now in

- [63694d2](https://github.com/bosun-ai/swiftide/commit/63694d2892a7c97a7e7fc42664d550c5acd7bb12) *(swiftide-query)*  Query pipeline v1 (#189) by @timonv

### Bug fixes

- [ee3aad3](https://github.com/bosun-ai/swiftide/commit/ee3aad37a40eb9f18c9a3082ad6826ff4b6c7245) *(deps)*  Update rust crate aws-sdk-bedrockruntime to v1.42.0 (#195) by @renovate[bot]

- [be0f31d](https://github.com/bosun-ai/swiftide/commit/be0f31de4f0c7842e23628fd6144cc4406c165c0) *(deps)*  Update rust crate spider to v1.99.11 (#190) by @renovate[bot]

- [dd04453](https://github.com/bosun-ai/swiftide/commit/dd04453ecb8d04326929780e9e52155b37d731e2) *(swiftide)*  Update main lockfile by @timonv

- [bafd907](https://github.com/bosun-ai/swiftide/commit/bafd90706346c3e208390f1296f10e2c17ad61b1)  Update all cargo package descriptions by @timonv

### Miscellaneous

- [e72641b](https://github.com/bosun-ai/swiftide/commit/e72641b677cfd1b21e98fd74552728dbe3e7a9bc) *(ci)*  Set versions in dependencies by @timonv

### Docs

- [2114aa4](https://github.com/bosun-ai/swiftide/commit/2114aa4394f4eda2e6465e1adb5602ae1b3ff61f) *(readme)*  Add copy on the query pipeline by @timonv

- [573aff6](https://github.com/bosun-ai/swiftide/commit/573aff6fee3f891bae61e92e131dd15425cefc29) *(indexing)*  Document the default prompt templates and their context (#206) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.7...swiftide-v0.7.0


## [swiftide-v0.6.7](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.7) - 2024-07-23

### New features

- [beea449](https://github.com/bosun-ai/swiftide/commit/beea449301b89fde1915c5336a071760c1963c75) *(prompt)*  Add Into for strings to PromptTemplate (#193) by @timonv

- [f3091f7](https://github.com/bosun-ai/swiftide/commit/f3091f72c74e816f6b9b8aefab058d610becb625) *(transformers)*  References and definitions from code (#186) by @timonv

### Docs

- [97a572e](https://github.com/bosun-ai/swiftide/commit/97a572ec2e3728bbac82c889bf5129b048e61e0c) *(readme)*  Add blog posts and update doc link (#194) by @timonv

- [504fe26](https://github.com/bosun-ai/swiftide/commit/504fe2632cf4add506dfb189c17d6e4ecf6f3824) *(pipeline)*  Add note that closures can also be used as transformers by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.6...swiftide-v0.6.7


## [swiftide-v0.6.6](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.6) - 2024-07-16

### New features

- [d1c642a](https://github.com/bosun-ai/swiftide/commit/d1c642aa4ee9b373e395a78591dd36fa0379a4ff) *(groq)*  Add SimplePrompt support for Groq (#183) by @timonv

````text
Adds simple prompt support for Groq by using async_openai. ~~Needs some
  double checks~~. Works great.
````

### Bug fixes

- [5d4a814](https://github.com/bosun-ai/swiftide/commit/5d4a8145b6952b2f4f9a1f144913673eeb3aaf24) *(deps)*  Update rust crate aws-sdk-bedrockruntime to v1.40.0 (#169) by @renovate[bot]

### Docs

- [143c7c9](https://github.com/bosun-ai/swiftide/commit/143c7c9c2638737166f23f2ef8106b7675f6e19b) *(readme)*  Fix typo (#180) by @eltociear

- [d393181](https://github.com/bosun-ai/swiftide/commit/d3931818146bff72499ebfcc0d0e8c8bb13a760d) *(docsrs)*  Scrape examples and fix links (#184) by @timonv

### New Contributors
* @eltociear made their first contribution in [#180](https://github.com/bosun-ai/swiftide/pull/180)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.5...swiftide-v0.6.6


## [swiftide-v0.6.5](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.5) - 2024-07-15

### New features

- [0065c7a](https://github.com/bosun-ai/swiftide/commit/0065c7a7fd1289ea227391dd7b9bd51c905290d5) *(prompt)*  Add extending the prompt repository (#178) by @timonv

### Bug fixes

- [b54691f](https://github.com/bosun-ai/swiftide/commit/b54691f769e2d0ac7886938b6e837551926eea2f) *(prompts)*  Include default prompts in crate (#174) by @timonv

````text
- **add prompts to crate**
  - **load prompts via cargo manifest dir**
````

- [3c297bb](https://github.com/bosun-ai/swiftide/commit/3c297bbb85fd3ae9b411a691024f622702da3617) *(swiftide)*  Remove include from Cargo.toml by @timonv

### Miscellaneous

- [73d5fa3](https://github.com/bosun-ai/swiftide/commit/73d5fa37d23f53919769c2ffe45db2e3832270ef) *(traits)*  Cleanup unused batch size in `BatchableTransformer` (#177) by @timonv

### Docs

- [b95b395](https://github.com/bosun-ai/swiftide/commit/b95b3955f89ed231cc156dab749ee7bb8be98ee5) *(swiftide)*  Documentation improvements and cleanup (#176) by @timonv

````text
- **chore: remove ingestion stream**
  - **Documentation and grammar**
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.3...swiftide-v0.6.5


## [swiftide-v0.6.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.3) - 2024-07-14

### Bug fixes

- [47418b5](https://github.com/bosun-ai/swiftide/commit/47418b5d729aef1e2ff77dabd7e29b5131512b01) *(prompts)*  Fix breaking issue with prompts not found by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.2...swiftide-v0.6.3


## [swiftide-v0.6.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.2) - 2024-07-12

### Miscellaneous

- [2b682b2](https://github.com/bosun-ai/swiftide/commit/2b682b28fd146fac2c61f1ee430534a04b9fa7ce) *(deps)*  Limit feature flags on qdrant to fix docsrs by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.1...swiftide-v0.6.2


## [swiftide-v0.6.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.1) - 2024-07-12

### Miscellaneous

- [aae7ab1](https://github.com/bosun-ai/swiftide/commit/aae7ab18f8c9509fd19f83695e4eca942c377043) *(deps)*  Patch update all by @timonv

### Docs

- [085709f](https://github.com/bosun-ai/swiftide/commit/085709fd767bab7153b2222907fc500ad4412570) *(docsrs)*  Disable unstable and rustdoc scraping by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.0...swiftide-v0.6.1


## [swiftide-v0.6.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.0) - 2024-07-12

### New features

- [70ea268](https://github.com/bosun-ai/swiftide/commit/70ea268b19e564af83bb834f56d406a05e02e9cd) *(prompts)*  Add prompts as first class citizens (#145) by @timonv

````text
Adds Prompts as first class citizens. This is a breaking change as
  SimplePrompt with just a a `&str` is no longer allowed.

  This introduces `Prompt` and `PromptTemplate`. A template uses jinja
  style templating build on tera. Templates can be converted into prompts,
  and have context added. A prompt is then send to something that prompts,
  i.e. openai or bedrock.

  Additional prompts can be added either compiled or as one-offs.
  Additionally, it's perfectly fine to prompt with just a string as well,
  just provide an `.into()`.

  For future development, some LLMs really benefit from system prompts,
  which this would enable. For the query pipeline we can also take a much
  more structured approach with composed templates and conditionals.
````

- [699cfe4](https://github.com/bosun-ai/swiftide/commit/699cfe44fb0e3baddba695ad09836caec7cb30a6)  Embed modes and named vectors (#123) by @pwalski

````text
Added named vector support to qdrant. A pipeline can now have its embed
  mode configured, either per field, chunk and metadata combined (default)
  or both. Vectors need to be configured on the qdrant client side.

  See `examples/store_multiple_vectors.rs` for an example.

  Shoutout to @pwalski for the contribution. Closes #62.

  ---------
````

### Bug fixes

- [9334934](https://github.com/bosun-ai/swiftide/commit/9334934e4af92b35dbc61e1f92aa90abac29ca12) *(chunkcode)*  Use correct chunksizes (#122) by @timonv

- [dfc76dd](https://github.com/bosun-ai/swiftide/commit/dfc76ddfc23d9314fe88c8362bf53d7865a03302) *(deps)*  Update rust crate serde to v1.0.204 (#129) by @renovate[bot]

- [28f5b04](https://github.com/bosun-ai/swiftide/commit/28f5b048f5acd977915ae20463f8fbb473dfab9a) *(deps)*  Update rust crate tree-sitter-typescript to v0.21.2 (#128) by @renovate[bot]

- [9c261b8](https://github.com/bosun-ai/swiftide/commit/9c261b87dde2e0caaff0e496d15681466844daf4) *(deps)*  Update rust crate text-splitter to v0.14.1 (#127) by @renovate[bot]

- [ff92abd](https://github.com/bosun-ai/swiftide/commit/ff92abd95908365c72d96abff37e0284df8fed32) *(deps)*  Update rust crate tree-sitter-javascript to v0.21.4 (#126) by @renovate[bot]

- [7af97b5](https://github.com/bosun-ai/swiftide/commit/7af97b589ca45f2b966ea2f61ebef341c881f1f9) *(deps)*  Update rust crate spider to v1.98.7 (#124) by @renovate[bot]

- [adc4bf7](https://github.com/bosun-ai/swiftide/commit/adc4bf789f679079fcc9fac38f4a7b8f98816844) *(deps)*  Update aws-sdk-rust monorepo (#125) by @renovate[bot]

- [dd32ef3](https://github.com/bosun-ai/swiftide/commit/dd32ef3b1be7cd6888d2961053d0b3c1a882e1a4) *(deps)*  Update rust crate async-trait to v0.1.81 (#134) by @renovate[bot]

- [2b13523](https://github.com/bosun-ai/swiftide/commit/2b1352322e574b62cb30268b35c6b510122f0584) *(deps)*  Update rust crate fastembed to v3.7.1 (#135) by @renovate[bot]

- [8e22937](https://github.com/bosun-ai/swiftide/commit/8e22937427b928524dacf2b446feeff726b6a5e1) *(deps)*  Update rust crate aws-sdk-bedrockruntime to v1.39.0 (#143) by @renovate[bot]

- [353cd9e](https://github.com/bosun-ai/swiftide/commit/353cd9ed36fcf6fb8f1db255d8b5f4a914ca8496) *(qdrant)*  Upgrade and better defaults (#118) by @timonv

````text
- **fix(deps): update rust crate qdrant-client to v1.10.1**
  - **fix(qdrant): upgrade to new qdrant with sensible defaults**
  - **feat(qdrant): safe to clone with internal arc**

  ---------
````

- [b53636c](https://github.com/bosun-ai/swiftide/commit/b53636cbd8f179f248cc6672aaf658863982c603)  Inability to store only some of `EmbeddedField`s (#139) by @pwalski

### Performance

- [ea8f823](https://github.com/bosun-ai/swiftide/commit/ea8f8236cdd9c588e55ef78f9eac27db1f13b2d9)  Improve local build performance and crate cleanup (#148) by @timonv

````text
- **tune cargo for faster builds**
  - **perf(swiftide): increase local build performance**
````

### Miscellaneous

- [eb8364e](https://github.com/bosun-ai/swiftide/commit/eb8364e08a9202476cca6b60fbdfbb31fe0e1c3d) *(ci)*  Try overriding the github repo for git cliff by @timonv

- [5de6af4](https://github.com/bosun-ai/swiftide/commit/5de6af42b9a1e95b0fbd54659c0d590db1d76222) *(ci)*  Only add contributors if present by @timonv

- [4c9ed77](https://github.com/bosun-ai/swiftide/commit/4c9ed77c85b7dd0e8722388b930d169cd2e5a5c7) *(ci)*  Properly check if contributors are present by @timonv

- [c5bf796](https://github.com/bosun-ai/swiftide/commit/c5bf7960ca6bec498cdc987fe7676acfef702e5b) *(ci)*  Add clippy back to ci (#147) by @timonv

- [7a8843a](https://github.com/bosun-ai/swiftide/commit/7a8843ab9e64b623870ebe49079ec976aae56d5c) *(deps)*  Update rust crate testcontainers to 0.20.0 (#133) by @renovate[bot]

- [364e13d](https://github.com/bosun-ai/swiftide/commit/364e13d83285317a1fb99889f6d74ad32b58c482) *(swiftide)*  Loosen up dependencies (#140) by @timonv

````text
Loosen up dependencies so swiftide is a bit more flexible to add to
  existing projects
````

- [84dd65d](https://github.com/bosun-ai/swiftide/commit/84dd65dc6c0ff4595f27ed061a4f4c0a2dae7202)  [**breaking**] Rename all mentions of ingest to index (#130) by @timonv

````text
Swiftide is not an ingestion pipeline (loading data), but an indexing
  pipeline (prepping for search).

  There is now a temporary, deprecated re-export to match the previous api.
````

**BREAKING CHANGE**: rename all mentions of ingest to index (#130)

- [51c114c](https://github.com/bosun-ai/swiftide/commit/51c114ceb06db840c4952d3d0f694bfbf266681c)  Various tooling & community improvements (#131) by @timonv

````text
- **fix(ci): ensure clippy runs with all features**
  - **chore(ci): coverage using llvm-cov**
  - **chore: drastically improve changelog generation**
  - **chore(ci): add sanity checks for pull requests**
  - **chore(ci): split jobs and add typos**
````

- [d2a9ea1](https://github.com/bosun-ai/swiftide/commit/d2a9ea1e7afa6f192bf9c32bbb54d9bb6e46472e)  Enable clippy pedantic (#132) by @timonv

### Docs

- [8405c9e](https://github.com/bosun-ai/swiftide/commit/8405c9efedef944156c2904eb709ba79aa4d82de) *(contributing)*  Add guidelines on code design (#113) by @timonv

- [3e447fe](https://github.com/bosun-ai/swiftide/commit/3e447feab83a4bf8d7d9d8220fe1b92dede9af79) *(readme)*  Link to CONTRIBUTING (#114) by @timonv

- [4c40e27](https://github.com/bosun-ai/swiftide/commit/4c40e27e5c6735305c70696ddf71dd5f95d03bbb) *(readme)*  Add back coverage badge by @timonv

- [5691ac9](https://github.com/bosun-ai/swiftide/commit/5691ac930fd6547c3f0166b64ead0ae647c38883) *(readme)*  Add preproduction warning by @timonv

- [37af322](https://github.com/bosun-ai/swiftide/commit/37af3225b4c3464aa4ed67f8f456c26f3d445507) *(rustdocs)*  Rewrite the initial landing page (#149) by @timonv

````text
- **Add homepage and badges to cargo toml**
  - **documentation landing page improvements**
````

- [7686c2d](https://github.com/bosun-ai/swiftide/commit/7686c2d449b5df0fddc08b111174357d47459f86)  Templated prompts are now a major feature by @timonv

### New Contributors
* @pwalski made their first contribution in [#139](https://github.com/bosun-ai/swiftide/pull/139)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.5.0...swiftide-v0.6.0


## [swiftide-v0.5.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.5.0) - 2024-07-01

### New features

- [6a88651](https://github.com/bosun-ai/swiftide/commit/6a88651df8c6b91add03acfc071fb9479545b8af) *(ingestion_pipeline)*  Implement filter (#109) by @timonv

- [5aeb3a7](https://github.com/bosun-ai/swiftide/commit/5aeb3a7fb75b21b2f24b111e9640ea4985b2e316) *(ingestion_pipeline)*  Splitting and merging streams by @timonv

- [8812fbf](https://github.com/bosun-ai/swiftide/commit/8812fbf30b882b68bf25f3d56b3ddf17af0bcb7a) *(ingestion_pipeline)*  Build a pipeline from a stream by @timonv

- [6101bed](https://github.com/bosun-ai/swiftide/commit/6101bed812c5167eb87a4093d66005140517598d)  AWS bedrock support (#92) by @timonv

````text
Adds an integration with AWS Bedrock, implementing SimplePrompt for
  Anthropic and Titan models. More can be added if there is a need. Same
  for the embedding models.
````

### Bug fixes

- [17a2be1](https://github.com/bosun-ai/swiftide/commit/17a2be1de6c0f3bda137501db4b1703f9ed0b1c5) *(changelog)*  Add scope by @timonv

- [a12cce2](https://github.com/bosun-ai/swiftide/commit/a12cce230032eebe2f7ff1aa9cdc85b8fc200eb1) *(openai)*  Add tests for builder by @timonv

- [963919b](https://github.com/bosun-ai/swiftide/commit/963919b0947faeb7d96931c19e524453ad4a0007) *(transformers)*  [**breaking**] Fix too small chunks being retained and api by @timonv

**BREAKING CHANGE**: Fix too small chunks being retained and api

- [5e8da00](https://github.com/bosun-ai/swiftide/commit/5e8da008ce08a23377672a046a4cedd48d4cf30c)  Fix oversight in ingestion pipeline tests by @timonv

- [e8198d8](https://github.com/bosun-ai/swiftide/commit/e8198d81354bbca2c21ca08b9522d02b8c93173b)  Use git cliff manually for changelog generation by @timonv

- [2c31513](https://github.com/bosun-ai/swiftide/commit/2c31513a0ded87addd0519bbfdd63b5abed29f73)  Just use keepachangelog by @timonv

- [6430af7](https://github.com/bosun-ai/swiftide/commit/6430af7b57eecb7fdb954cd89ade4547b8e92dbd)  Use native cargo bench format and only run benchmarks crate by @timonv

- [cba981a](https://github.com/bosun-ai/swiftide/commit/cba981a317a80173eff2946fc551d1a36ec40f65)  Replace unwrap with expect and add comment on panic by @timonv

### Miscellaneous

- [e243212](https://github.com/bosun-ai/swiftide/commit/e2432123f0dfc48147ebed13fe6e3efec3ff7b3f) *(ci)*  Enable continous benchmarking and improve benchmarks (#98) by @timonv

- [2dbf14c](https://github.com/bosun-ai/swiftide/commit/2dbf14c34bed2ee40ab79c0a46d011cd20882bda) *(ci)*  Fix benchmarks in ci by @timonv

- [b155de6](https://github.com/bosun-ai/swiftide/commit/b155de6387ddfe64d1a177b31c8e1ed93739b2c9) *(ci)*  Fix naming of github actions by @timonv

- [206e432](https://github.com/bosun-ai/swiftide/commit/206e432dd291dd6a4592a6fb5f890049595311cb) *(ci)*  Add support for merge queues by @timonv

- [46752db](https://github.com/bosun-ai/swiftide/commit/46752dbfc8ccd578ddba915fd6cd6509e3e6fb14) *(ci)*  Add concurrency configuration by @timonv

- [5f09c11](https://github.com/bosun-ai/swiftide/commit/5f09c116f418cecb96fb1e86161333908d1a4d70)  Add initial benchmarks by @timonv

- [162c6ef](https://github.com/bosun-ai/swiftide/commit/162c6ef2a07e40b8607b0ab6773909521f0bb798)  Ensure feat is always in Added by @timonv

### Docs

- [929410c](https://github.com/bosun-ai/swiftide/commit/929410cb1c2d81b6ffaec4c948c891472835429d) *(readme)*  Add diagram to the readme (#107) by @timonv

- [b014f43](https://github.com/bosun-ai/swiftide/commit/b014f43aa187881160245b4356f95afe2c6fe98c)  Improve documentation across the project (#112) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.3...swiftide-v0.5.0


## [swiftide-v0.4.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.3) - 2024-06-28

### Bug fixes

- [ab3dc86](https://github.com/bosun-ai/swiftide/commit/ab3dc861490a0d1ab94f96e741e09c860094ebc0) *(memory_storage)*  Fallback to incremental counter when missing id by @timonv

### Miscellaneous

- [bdebc24](https://github.com/bosun-ai/swiftide/commit/bdebc241507e9f55998e96ca4aece530363716af)  Clippy by @timonv

### Docs

- [dad3e02](https://github.com/bosun-ai/swiftide/commit/dad3e02fdc8a57e9de16832090c44c536e7e394b) *(readme)*  Add ci badge by @timonv

- [4076092](https://github.com/bosun-ai/swiftide/commit/40760929d24e20631d0552d87bdbb4fdf9195453) *(readme)*  Clean up and consistent badge styles by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.2...swiftide-v0.4.3


## [swiftide-v0.4.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.2) - 2024-06-26

### New features

- [926cc0c](https://github.com/bosun-ai/swiftide/commit/926cc0cca46023bcc3097a97b10ce03ae1fc3cc2) *(ingestion_stream)*  Implement into for Result<Vec<IngestionNode>> by @timonv

### Bug fixes

- [3143308](https://github.com/bosun-ai/swiftide/commit/3143308136ec4e71c8a5f9a127119e475329c1a2) *(embed)*  Panic if number of embeddings and node are equal by @timonv

### Miscellaneous

- [5ed08bb](https://github.com/bosun-ai/swiftide/commit/5ed08bb259b7544d3e4f2acdeef56231aa32e17c)  Cleanup changelog by @timonv

### Docs

- [47aa378](https://github.com/bosun-ai/swiftide/commit/47aa378c4a70c47a2b313b6eca8dcf02b4723963)  Create CONTRIBUTING.md by @timonv

- [0660d5b](https://github.com/bosun-ai/swiftide/commit/0660d5b08aed15d62f077363eae80f621ddaa510)  Readme updates by @timonv

### Refactor

- [d285874](https://github.com/bosun-ai/swiftide/commit/d28587448d7fe342a79ac687cd5d7ee27354cae6) *(ingestion_pipeline)*  Log_all combines other log helpers by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.1...swiftide-v0.4.2


## [swiftide-v0.4.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.1) - 2024-06-24

### New features

- [3898ee7](https://github.com/bosun-ai/swiftide/commit/3898ee7d6273ee7034848f9ab08fd85613cb5b32) *(memory_storage)*  Can be cloned safely preserving storage by @timonv

- [92052bf](https://github.com/bosun-ai/swiftide/commit/92052bfdbca8951620f6d016768d252e793ecb5d) *(transformers)*  Allow for arbitrary closures as transformers and batchable transformers by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.0...swiftide-v0.4.1


## [swiftide-v0.4.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.0) - 2024-06-23

### New features

- [477a284](https://github.com/bosun-ai/swiftide/commit/477a284597359472988ecde372e080f60aab0804) *(benchmarks)*  Add benchmark for the file loader by @timonv

- [1567940](https://github.com/bosun-ai/swiftide/commit/15679409032e9be347fbe8838a308ff0d09768b8) *(benchmarks)*  Add benchmark for simple local pipeline by @timonv

- [2228d84](https://github.com/bosun-ai/swiftide/commit/2228d84ccaad491e2c3cd0feb948050ad2872cf0) *(examples)*  Example for markdown with all metadata by @timonv

- [9a1e12d](https://github.com/bosun-ai/swiftide/commit/9a1e12d34e02fe2292ce679251b96d61be74c884) *(examples,scraping)*  Add example scraping and ingesting a url by @timonv

- [15deeb7](https://github.com/bosun-ai/swiftide/commit/15deeb72ca2e131e8554fa9cbefa3ef369de752a) *(ingestion_node)*  Add constructor with defaults by @timonv

- [4d5c68e](https://github.com/bosun-ai/swiftide/commit/4d5c68e7bb09fae18832e2a453f114df5ba32ce1) *(ingestion_node)*  Improved human readable Debug by @timonv

- [a5051b7](https://github.com/bosun-ai/swiftide/commit/a5051b79b2ce62d41dd93f7b34a1a065d9878732) *(ingestion_pipeline)*  Optional error filtering and logging (#75) by @timonv

- [062107b](https://github.com/bosun-ai/swiftide/commit/062107b46474766640c38266f6fd6c27a95d4b57) *(ingestion_pipeline)*  Implement throttling a pipeline (#77) by @timonv

- [a2ffc78](https://github.com/bosun-ai/swiftide/commit/a2ffc78f6d25769b9b7894f1f0703d51242023d4) *(ingestion_stream)*  Improved stream developer experience (#81) by @timonv

````text
Improves stream ergonomics by providing convenient helpers and `Into`
  for streams, vectors and iterators that match the internal type.

  This means that in many cases, trait implementers can simply call
  `.into()` instead of manually constructing a stream. In the case it's an
  iterator, they can now use `IngestionStream::iter(<IntoIterator>)`
  instead.
````

- [d260674](https://github.com/bosun-ai/swiftide/commit/d2606745de8b22dcdf02e244d1b044efe12c6ac7) *(integrations)*  [**breaking**] Support fastembed (#60) by @timonv

````text
Adds support for FastEmbed with various models. Includes a breaking change, renaming the Embed trait to EmbeddingModel.
````

**BREAKING CHANGE**: support fastembed (#60)

- [9004323](https://github.com/bosun-ai/swiftide/commit/9004323dc5b11a3556a47e11fb8912ffc49f1e9e) *(integrations)*  [**breaking**] Implement Persist for Redis (#80) by @timonv

**BREAKING CHANGE**: implement Persist for Redis (#80)

- [eb84dd2](https://github.com/bosun-ai/swiftide/commit/eb84dd27c61a1b3a4a52a53cc0404203eac729e8) *(integrations,transformers)*  Add transformer for converting html to markdown by @timonv

- [ef7dcea](https://github.com/bosun-ai/swiftide/commit/ef7dcea45bfc336e7defcaac36bb5a6ff27d5acd) *(loaders)*  File loader performance improvements by @timonv

- [6d37051](https://github.com/bosun-ai/swiftide/commit/6d37051a9c2ef24ea7eb3815efcf9692df0d70ce) *(loaders)*  Add scraping using `spider` by @timonv

- [2351867](https://github.com/bosun-ai/swiftide/commit/235186707182e8c39b8f22c6dd9d54eb32f7d1e5) *(persist)*  In memory storage for testing, experimentation and debugging by @timonv

- [4d5d650](https://github.com/bosun-ai/swiftide/commit/4d5d650f235395aa81816637d559de39853e1db1) *(traits)*  Add automock for simpleprompt by @timonv

- [bd6f887](https://github.com/bosun-ai/swiftide/commit/bd6f8876d010d23f651fd26a48d6775c17c98e94) *(transformers)*  Add transformers for title, summary and keywords by @timonv

### Bug fixes

- [7cbfc4e](https://github.com/bosun-ai/swiftide/commit/7cbfc4e13745ee5a6776a97fc6db06608fae8e81) *(ingestion_pipeline)*  Concurrency does not work when spawned (#76) by @timonv

````text
Currency does did not work as expected. When spawning via `Tokio::spawn`
  the future would be polled directly, and any concurrency setting would
  not be respected. Because it had to be removed, improved tracing for
  each step as well.
````

### Miscellaneous

- [f4341ba](https://github.com/bosun-ai/swiftide/commit/f4341babe5807b268ce86a88e0df4bfc6d756de4) *(ci)*  Single changelog for all (future) crates in root (#57) by @timonv

- [7dde8a0](https://github.com/bosun-ai/swiftide/commit/7dde8a0811c7504b807b3ef9f508ce4be24967b8) *(ci)*  Code coverage reporting (#58) by @timonv

````text
Post test coverage to Coveralls

  Also enabled --all-features when running tests in ci, just to be sure
````

- [cb7a2cd](https://github.com/bosun-ai/swiftide/commit/cb7a2cd3a72f306a0b46556caee0a25c7ba2c0e0) *(scraping)*  Exclude spider from test coverage by @timonv

- [7767588](https://github.com/bosun-ai/swiftide/commit/77675884a2eeb0aab6ce57dccd2a260f5a973197) *(transformers)*  Improve test coverage by @timonv

- [3b7c0db](https://github.com/bosun-ai/swiftide/commit/3b7c0dbc2f020ce84a5da5691ee6eb415df2d466)  Move changelog to root by @timonv

- [d6d0215](https://github.com/bosun-ai/swiftide/commit/d6d021560a05508add07a72f4f438d3ea3f1cb2c)  Properly quote crate name in changelog by @timonv

- [f251895](https://github.com/bosun-ai/swiftide/commit/f2518950427ef758fd57e6e6189ce600adf19940)  Documentation and feature flag cleanup (#69) by @timonv

````text
With fastembed added our dependencies become rather heavy. By default
  now disable all integrations and either provide 'all' or cherry pick
  integrations.
````

- [f6656be](https://github.com/bosun-ai/swiftide/commit/f6656becd199762843a59b0f86871753360a08f0)  Cargo update by @timonv

### Docs

- [53ed920](https://github.com/bosun-ai/swiftide/commit/53ed9206835da1172295e296119ee9a883605f18)  Hide the table of contents by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.3...swiftide-v0.4.0


## [swiftide-v0.3.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.3) - 2024-06-16

### New features

- [bdaed53](https://github.com/bosun-ai/swiftide/commit/bdaed5334b3e122f803370cc688dd2f662db0b8d) *(integrations)*  Clone and debug for integrations by @timonv

- [318e538](https://github.com/bosun-ai/swiftide/commit/318e538acb30ca516a780b5cc42c8ab2ed91cd6b) *(transformers)*  Builder and clone for chunk_code by @timonv

- [c074cc0](https://github.com/bosun-ai/swiftide/commit/c074cc0edb8b0314de15f9a096699e3e744c9f33) *(transformers)*  Builder for chunk_markdown by @timonv

- [e18e7fa](https://github.com/bosun-ai/swiftide/commit/e18e7fafae3007f1980bb617b7a72dd605720d74) *(transformers)*  Builder and clone for MetadataQACode by @timonv

- [fd63dff](https://github.com/bosun-ai/swiftide/commit/fd63dffb4f0b11bb9fa4fadc7b076463eca111a6) *(transformers)*  Builder and clone for MetadataQAText by @timonv

### Miscellaneous

- [678106c](https://github.com/bosun-ai/swiftide/commit/678106c01b7791311a24425c22ea39366b664033) *(ci)*  Pretty names for pipelines (#54) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.2...swiftide-v0.3.3


## [swiftide-v0.3.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.2) - 2024-06-16

### New features

- [b211002](https://github.com/bosun-ai/swiftide/commit/b211002e40ef16ef240e142c0178b04636a4f9aa) *(integrations)*  Qdrant and openai builder should be consistent (#52) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.1...swiftide-v0.3.2


## [swiftide-v0.3.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.1) - 2024-06-15

### Docs

- [6f63866](https://github.com/bosun-ai/swiftide/commit/6f6386693f3f6e0328eedaa4fb69cd8d0694574b)  We love feedback <3 by @timonv

- [7d79b64](https://github.com/bosun-ai/swiftide/commit/7d79b645d2e4f7da05b4c9952a1ceb79583572b3)  Fixing some grammar typos on README.md (#51) by @hectorip

### New Contributors
* @hectorip made their first contribution in [#51](https://github.com/bosun-ai/swiftide/pull/51)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.0...swiftide-v0.3.1


## [swiftide-v0.3.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.0) - 2024-06-14

### New features

- [745b8ed](https://github.com/bosun-ai/swiftide/commit/745b8ed7e58f76e415501e6219ecec65551d1897) *(ingestion_pipeline)*  [**breaking**] Support chained storage backends (#46) by @timonv

````text
Pipeline now supports multiple storage backends. This makes the order of adding storage important. Changed the name of the method to reflect that.
````

**BREAKING CHANGE**: support chained storage backends (#46)

- [cd055f1](https://github.com/bosun-ai/swiftide/commit/cd055f19096daa802fe7fc34763bfdfd87c1ec41) *(ingestion_pipeline)*  Concurrency improvements (#48) by @timonv

- [1f0cd28](https://github.com/bosun-ai/swiftide/commit/1f0cd28ce4c02a39dbab7dd3c3f789798644daa3) *(ingestion_pipeline)*  Early return if any error encountered (#49) by @timonv

- [fa74939](https://github.com/bosun-ai/swiftide/commit/fa74939b30bd31301e3f80c407f153b5d96aa007)  Configurable concurrency for transformers and chunkers (#47) by @timonv

### Docs

- [473e60e](https://github.com/bosun-ai/swiftide/commit/473e60ecf9356e2fcabe68245f8bb8be7373cdfb)  Update linkedin link by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.1...swiftide-v0.3.0


## [swiftide-v0.2.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.2.1) - 2024-06-13

### Docs

- [cb9b4fe](https://github.com/bosun-ai/swiftide/commit/cb9b4feec1c3654f5067f9478b1a7cf59040a9fe)  Add link to bosun by @timonv

- [e330ab9](https://github.com/bosun-ai/swiftide/commit/e330ab92d7e8d3f806280fa781f0e1b179d9b900)  Fix documentation link by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.0...swiftide-v0.2.1


## [swiftide-v0.2.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.2.0) - 2024-06-13

### New features

- [9ec93be](https://github.com/bosun-ai/swiftide/commit/9ec93be110bd047c7e276714c48df236b1a235d7)  Api improvements with example (#10) by @timonv

### Bug fixes

- [42f8008](https://github.com/bosun-ai/swiftide/commit/42f80086042c659aef74ddd0ea1463c84650938d)  Clippy & fmt by @timonv

- [5b7ffd7](https://github.com/bosun-ai/swiftide/commit/5b7ffd7368a2688f70892fe37f28c0baea7ad54f)  Fmt by @timonv

### Docs

- [95a6200](https://github.com/bosun-ai/swiftide/commit/95a62008be1869e581ecaa0586a48cfbb6a7606a) *(swiftide)*  Documented file swiftide/src/ingestion/ingestion_pipeline.rs (#14) by @bosun-ai[bot]

- [7abccc2](https://github.com/bosun-ai/swiftide/commit/7abccc2af890c8369a2b46940f35274080b3cb61) *(swiftide)*  Documented file swiftide/src/ingestion/ingestion_stream.rs (#16) by @bosun-ai[bot]

- [755cd47](https://github.com/bosun-ai/swiftide/commit/755cd47ad00e562818162cf78e6df0c5daa99d14) *(swiftide)*  Documented file swiftide/src/ingestion/ingestion_node.rs (#15) by @bosun-ai[bot]

- [2ea5a84](https://github.com/bosun-ai/swiftide/commit/2ea5a8445c8df7ef36e5fbc25f13c870e5a4dfd5) *(swiftide)*  Documented file swiftide/src/integrations/openai/mod.rs (#21) by @bosun-ai[bot]

- [b319c0d](https://github.com/bosun-ai/swiftide/commit/b319c0d484db65d3a4594347e70770b8fac39e10) *(swiftide)*  Documented file swiftide/src/integrations/treesitter/splitter.rs (#30) by @bosun-ai[bot]

- [29fce74](https://github.com/bosun-ai/swiftide/commit/29fce7437042f1f287987011825b57c58c180696) *(swiftide)*  Documented file swiftide/src/integrations/redis/node_cache.rs (#29) by @bosun-ai[bot]

- [7229af8](https://github.com/bosun-ai/swiftide/commit/7229af8535daa450ebafd6c45c322222a2dd12a0) *(swiftide)*  Documented file swiftide/src/integrations/qdrant/persist.rs (#24) by @bosun-ai[bot]

- [6240a26](https://github.com/bosun-ai/swiftide/commit/6240a260b582034970d2ee46da9f5234cf317820) *(swiftide)*  Documented file swiftide/src/integrations/redis/mod.rs (#23) by @bosun-ai[bot]

- [7688c99](https://github.com/bosun-ai/swiftide/commit/7688c993125a129204739fc7cd8d23d0ebfc9022) *(swiftide)*  Documented file swiftide/src/integrations/qdrant/mod.rs (#22) by @bosun-ai[bot]

- [d572c88](https://github.com/bosun-ai/swiftide/commit/d572c88f2b4cfc4bbdd7bd5ca93f7fd8460f1cb0) *(swiftide)*  Documented file swiftide/src/integrations/qdrant/ingestion_node.rs (#20) by @bosun-ai[bot]

- [14e24c3](https://github.com/bosun-ai/swiftide/commit/14e24c30d28dc6272a5eb8275e758a2a989d66be) *(swiftide)*  Documented file swiftide/src/ingestion/mod.rs (#28) by @bosun-ai[bot]

- [502939f](https://github.com/bosun-ai/swiftide/commit/502939fcb5f56b7549b97bb99d4d121bf030835f) *(swiftide)*  Documented file swiftide/src/integrations/treesitter/supported_languages.rs (#26) by @bosun-ai[bot]

- [a78e68e](https://github.com/bosun-ai/swiftide/commit/a78e68e347dc3791957eeaf0f0adc050aeac1741) *(swiftide)*  Documented file swiftide/tests/ingestion_pipeline.rs (#41) by @bosun-ai[bot]

- [289687e](https://github.com/bosun-ai/swiftide/commit/289687e1a6c0a9555a6cbecb24951522529f9e1a) *(swiftide)*  Documented file swiftide/src/loaders/mod.rs (#40) by @bosun-ai[bot]

- [ebd0a5d](https://github.com/bosun-ai/swiftide/commit/ebd0a5dda940c5ef8c2b795ee8ab56e468726869) *(swiftide)*  Documented file swiftide/src/transformers/chunk_code.rs (#39) by @bosun-ai[bot]

- [fb428d1](https://github.com/bosun-ai/swiftide/commit/fb428d1e250eded80d4edc8ccc0c9a9b840fc065) *(swiftide)*  Documented file swiftide/src/transformers/metadata_qa_text.rs (#36) by @bosun-ai[bot]

- [305a641](https://github.com/bosun-ai/swiftide/commit/305a64149f015539823d748915e42ad440a7b4b4) *(swiftide)*  Documented file swiftide/src/transformers/openai_embed.rs (#35) by @bosun-ai[bot]

- [c932897](https://github.com/bosun-ai/swiftide/commit/c93289740806d9283ba488dd640dad5e4339e07d) *(swiftide)*  Documented file swiftide/src/transformers/metadata_qa_code.rs (#34) by @bosun-ai[bot]

- [090ef1b](https://github.com/bosun-ai/swiftide/commit/090ef1b38684afca8dbcbfe31a8debc2328042e5) *(swiftide)*  Documented file swiftide/src/integrations/openai/simple_prompt.rs (#19) by @bosun-ai[bot]

- [7cfcc83](https://github.com/bosun-ai/swiftide/commit/7cfcc83eec29d8bed44172b497d4468b0b67d293)  Update readme template links and fix template by @timonv

- [a717f3d](https://github.com/bosun-ai/swiftide/commit/a717f3d5a68d9c79f9b8d85d8cb8979100dc3949)  Template links should be underscores by @timonv

### New Contributors
* @bosun-ai[bot] made their first contribution in [#19](https://github.com/bosun-ai/swiftide/pull/19)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.1.0...swiftide-v0.2.0


## [v0.1.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.1.0) - 2024-06-13

### New features

- [2a6e503](https://github.com/bosun-ai/swiftide/commit/2a6e503e8abdab83ead7b8e62f39e222fa9f45d1) *(doc)*  Setup basic readme (#5) by @timonv

- [b8f9166](https://github.com/bosun-ai/swiftide/commit/b8f9166e1d5419cf0d2cc6b6f0b2378241850574) *(fluyt)*  Significant tracing improvements (#368) by @timonv

````text
* fix(fluyt): remove unnecessary cloning and unwraps

  * fix(fluyt): also set target correctly on manual spans

  * fix(fluyt): do not capture raw result

  * feat(fluyt): nicer tracing for ingestion pipeline

  * fix(fluyt): remove instrumentation on lazy methods

  * feat(fluyt): add useful metadata to the root span

  * fix(fluyt): fix dangling spans in ingestion pipeline

  * fix(fluyt): do not log codebase in rag utils
````

- [0986136](https://github.com/bosun-ai/swiftide/commit/098613622a7018318f2fffe0d51cd17822bf2313) *(fluyt/code_ops)*  Add languages to chunker and range for chunk size (#334) by @timonv

````text
* feat(fluyt/code_ops): add more treesitter languages

  * fix: clippy + fmt

  * feat(fluyt/code_ops): implement builder and support range

  * feat(fluyt/code_ops): implement range limits for code chunking

  * feat(fluyt/indexing): code chunking supports size
````

- [f10bc30](https://github.com/bosun-ai/swiftide/commit/f10bc304b0b2e28281c90e57b6613c274dc20727) *(ingestion_pipeline)*  Default concurrency is the number of cpus (#6) by @timonv

- [7453ddc](https://github.com/bosun-ai/swiftide/commit/7453ddc387feb17906ae851a17695f4c8232ee19)  Replace databuoy with new ingestion pipeline (#322) by @timonv

- [054b560](https://github.com/bosun-ai/swiftide/commit/054b560571b4a4398a551837536fb8fbff13c149)  Fix build and add feature flags for all integrations by @timonv

### Bug fixes

- [fdf4be3](https://github.com/bosun-ai/swiftide/commit/fdf4be3d0967229a9dd84f568b0697fea4ddd341) *(fluyt)*  Ensure minimal tracing by @timonv

- [389b0f1](https://github.com/bosun-ai/swiftide/commit/389b0f12039f29703bc8bb71919b8067fadf5a8e)  Add debug info to qdrant setup by @timonv

- [bb905a3](https://github.com/bosun-ai/swiftide/commit/bb905a30d871ea3b238c3bc5cfd1d96724c8d4eb)  Use rustls on redis and log errors by @timonv

- [458801c](https://github.com/bosun-ai/swiftide/commit/458801c16f9111c1070878c3a82a319701ae379c)  Properly connect to redis over tls by @timonv

### Miscellaneous

- [ce6e465](https://github.com/bosun-ai/swiftide/commit/ce6e465d4fb12e2bbc7547738b5fbe5133ec2d5a) *(fluyt)*  Add verbose log on checking if index exists by @timonv

- [6967b0d](https://github.com/bosun-ai/swiftide/commit/6967b0d5b6221f7620161969865fb31959fc93b8)  Make indexing extraction compile by @tinco

- [f595f3d](https://github.com/bosun-ai/swiftide/commit/f595f3dae88bb4da5f4bbf6c5fe4f04abb4b7db3)  Add rust-toolchain on stable by @timonv

- [da004c6](https://github.com/bosun-ai/swiftide/commit/da004c6fcf82579c3c75414cb9f04f02530e2e31)  Start cleaning up dependencies by @timonv

- [cccdaf5](https://github.com/bosun-ai/swiftide/commit/cccdaf567744d58e0ee8ffcc8636f3b35090778f)  Remove more unused dependencies by @timonv

- [7ee8799](https://github.com/bosun-ai/swiftide/commit/7ee8799aeccc56fb0c14dbe68a7126cabfb40dd3)  Remove more crates and update by @timonv

- [951f496](https://github.com/bosun-ai/swiftide/commit/951f496498b35f7687fb556e5bf7f931a662ff8a)  Clean up more crates by @timonv

- [1f17d84](https://github.com/bosun-ai/swiftide/commit/1f17d84cc218602a480b27974f23f64c4269134f)  Cargo update by @timonv

- [730d879](https://github.com/bosun-ai/swiftide/commit/730d879e76c867c2097aef83bbbfa1211a053bdc)  Create LICENSE by @timonv

- [44524fb](https://github.com/bosun-ai/swiftide/commit/44524fb51523291b9137fbdcaff9133a9a80c58a)  Restructure repository and rename (#3) by @timonv

````text
* chore: move traits around

  * chore: move crates to root folder

  * chore: restructure and make it compile

  * chore: remove infrastructure

  * fix: make it compile

  * fix: clippy

  * chore: remove min rust version

  * chore: cargo update

  * chore: remove code_ops

  * chore: settle on swiftide
````

- [e717b7f](https://github.com/bosun-ai/swiftide/commit/e717b7f0b1311b11ed4690e7e11d9fdf53d4a81b)  Update issue templates by @timonv

- [8e22e0e](https://github.com/bosun-ai/swiftide/commit/8e22e0ef82fffa4f907b0e2cccd1c4e010ffbd01)  Cleanup by @timonv

- [4d79d27](https://github.com/bosun-ai/swiftide/commit/4d79d27709e3fed32c1b1f2c1f8dbeae1721d714)  Tests, tests, tests (#4) by @timonv

- [1036d56](https://github.com/bosun-ai/swiftide/commit/1036d565d8d9740ab55995095d495e582ce643d8)  Configure cargo toml (#7) by @timonv

- [0ae98a7](https://github.com/bosun-ai/swiftide/commit/0ae98a772a751ddc60dd1d8e1606f9bdab4e04fd)  Cleanup Cargo keywords by @timonv

### Refactor

- [0d342ea](https://github.com/bosun-ai/swiftide/commit/0d342eab747bc5f44adaa5b6131c30c09b1172a2)  Models as first class citizens (#318) by @timonv

````text
* refactor: refactor common datastructures to /models

  * refactor: promote to first class citizens

  * fix: clippy

  * fix: remove duplication in http handler

  * fix: clippy

  * fix: fmt

  * feat: update for latest change

  * fix(fluyt/models): doctest
````



