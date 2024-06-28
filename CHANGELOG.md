# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Details
#### Changed
- Configure Renovate
- AWS bedrock support
- Enable continous benchmarking and improve benchmarks

#### Fixed
- Fix oversight in ingestion pipeline tests
- Fix release-plz changelog parsing
- Fix benchmarks in ci

## [swiftide-v0.4.3] - 2024-06-28
### Details
#### Added
- Add ci badge

#### Changed
- Clean up and consistent badge styles
- Clippy
- Manual release-plz update

#### Fixed
- Fallback to incremental counter when missing id

## [swiftide-v0.4.2] - 2024-06-26
### Details
#### Changed
- Cleanup changelog
- Create CONTRIBUTING.md
- Readme updates
- Log_all combines other log helpers
- Implement into for Result<Vec<IngestionNode>>
- Release

#### Fixed
- Panic if number of embeddings and node are equal

## [swiftide-v0.4.1] - 2024-06-24
### Details
#### Changed
- Can be cloned safely preserving storage
- Allow for arbitrary closures as transformers and batchable transformers
- Release

## [swiftide-v0.4.0] - 2024-06-23
### Details
#### Added
- Support fastembed
- Add constructor with defaults
- Add automock for simpleprompt
- Add transformers for title, summary and keywords
- Add benchmark for the file loader
- Add benchmark for simple local pipeline
- Add scraping using `spider`
- Add transformer for converting html to markdown

#### Changed
- Single changelog for all (future) crates in root
- Code coverage reporting
- Move changelog to root
- Properly quote crate name in changelog
- Documentation and feature flag cleanup
- Hide the table of contents
- Optional error filtering and logging
- Cargo update
- Implement throttling a pipeline
- Implement Persist for Redis
- Example for markdown with all metadata
- Improved human readable Debug
- Improve test coverage
- Improved stream developer experience
- File loader performance improvements
- In memory storage for testing, experimentation and debugging
- Add example scraping and ingesting a url
- Exclude spider from test coverage
- Release v0.4.0

#### Fixed
- Concurrency does not work when spawned

## [swiftide-v0.3.3] - 2024-06-16
### Details
#### Changed
- Pretty names for pipelines
- Clone and debug for integrations
- Builder and clone for chunk_code
- Builder for chunk_markdown
- Builder and clone for MetadataQACode
- Builder and clone for MetadataQAText
- Release v0.3.3

## [swiftide-v0.3.2] - 2024-06-16
### Details
#### Changed
- Qdrant and openai builder should be consistent
- Release v0.3.2

## [swiftide-v0.3.1] - 2024-06-15
### Details
#### Changed
- We love feedback <3
- Fixing some grammar typos on README.md
- Release v0.3.1

## [swiftide-v0.3.0] - 2024-06-14
### Details
#### Added
- Support chained storage backends

#### Changed
- Update linkedin link
- Concurrency improvements
- Configurable concurrency for transformers and chunkers
- Early return if any error encountered
- Release v0.3.0

## [swiftide-v0.2.1] - 2024-06-13
### Details
#### Changed
- Add link to bosun
- Release v0.2.1

#### Fixed
- Fix documentation link

## [swiftide-v0.2.0] - 2024-06-13
### Details
#### Changed
- Release v0.1.0
- Api improvements with example
- Documented file swiftide/src/ingestion/ingestion_pipeline.rs
- Documented file swiftide/src/ingestion/ingestion_stream.rs
- Documented file swiftide/src/ingestion/ingestion_node.rs
- Documented file swiftide/src/integrations/openai/mod.rs
- Documented file swiftide/src/integrations/treesitter/splitter.rs
- Documented file swiftide/src/integrations/redis/node_cache.rs
- Documented file swiftide/src/integrations/qdrant/persist.rs
- Documented file swiftide/src/integrations/redis/mod.rs
- Documented file swiftide/src/integrations/qdrant/mod.rs
- Documented file swiftide/src/integrations/qdrant/ingestion_node.rs
- Documented file swiftide/src/ingestion/mod.rs
- Documented file swiftide/src/integrations/treesitter/supported_languages.rs
- Documented file swiftide/tests/ingestion_pipeline.rs
- Documented file swiftide/src/loaders/mod.rs
- Documented file swiftide/src/transformers/chunk_code.rs
- Documented file swiftide/src/transformers/metadata_qa_text.rs
- Documented file swiftide/src/transformers/openai_embed.rs
- Documented file swiftide/src/transformers/metadata_qa_code.rs
- Documented file swiftide/src/integrations/openai/simple_prompt.rs
- Update readme template links and fix template
- Template links should be underscores
- Release v0.2.0

#### Fixed
- Clippy & fmt
- Fmt

## [0.1.0] - 2024-06-13
### Details
#### Added
- Add languages to chunker and range for chunk size
- Add debug info to qdrant setup
- Add verbose log on checking if index exists
- Add rust-toolchain on stable

#### Changed
- Replace databuoy with new ingestion pipeline
- Models as first class citizens
- Significant tracing improvements
- Make indexing extraction compile
- Start cleaning up dependencies
- Clean up more crates
- Cargo update
- Create LICENSE
- Restructure repository and rename
- Update issue templates
- Cleanup
- Tests, tests, tests
- Set up basic test and release actions
- Configure cargo toml
- Default concurrency is the number of cpus
- Setup basic readme
- Cleanup Cargo keywords

#### Fixed
- Ensure minimal tracing
- Use rustls on redis and log errors
- Properly connect to redis over tls
- Fix build and add feature flags for all integrations

#### Removed
- Remove more unused dependencies
- Remove more crates and update

[unreleased]: https://github.com///compare/swiftide-v0.4.3..HEAD
[swiftide-v0.4.3]: https://github.com///compare/swiftide-v0.4.2..swiftide-v0.4.3
[swiftide-v0.4.2]: https://github.com///compare/swiftide-v0.4.1..swiftide-v0.4.2
[swiftide-v0.4.1]: https://github.com///compare/swiftide-v0.4.0..swiftide-v0.4.1
[swiftide-v0.4.0]: https://github.com///compare/swiftide-v0.3.3..swiftide-v0.4.0
[swiftide-v0.3.3]: https://github.com///compare/swiftide-v0.3.2..swiftide-v0.3.3
[swiftide-v0.3.2]: https://github.com///compare/swiftide-v0.3.1..swiftide-v0.3.2
[swiftide-v0.3.1]: https://github.com///compare/swiftide-v0.3.0..swiftide-v0.3.1
[swiftide-v0.3.0]: https://github.com///compare/swiftide-v0.2.1..swiftide-v0.3.0
[swiftide-v0.2.1]: https://github.com///compare/swiftide-v0.2.0..swiftide-v0.2.1
[swiftide-v0.2.0]: https://github.com///compare/v0.1.0..swiftide-v0.2.0

<!-- generated by git-cliff -->
