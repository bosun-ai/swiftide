# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## swiftide - [0.3.3](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.2...swiftide-v0.3.3) - 2024-06-16

### Added

- _(transformers)_ builder and clone for MetadataQAText
- _(transformers)_ builder and clone for MetadataQACode
- _(transformers)_ builder for chunk_markdown
- _(transformers)_ builder and clone for chunk_code
- _(integrations)_ clone and debug for integrations

## swiftide - [0.3.2](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.1...swiftide-v0.3.2) - 2024-06-16

### Added

- _(integrations)_ qdrant and openai builder should be consistent ([#52](https://github.com/bosun-ai/swiftide/pull/52))

## swiftide - [0.3.1](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.0...swiftide-v0.3.1) - 2024-06-15

### Other

- Fixing some grammar typos on README.md ([#51](https://github.com/bosun-ai/swiftide/pull/51))
- we love feedback <3

## swiftide - [0.3.0](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.1...swiftide-v0.3.0) - 2024-06-14

### Added

- _(ingestion_pipeline)_ early return if any error encountered ([#49](https://github.com/bosun-ai/swiftide/pull/49))
- configurable concurrency for transformers and chunkers ([#47](https://github.com/bosun-ai/swiftide/pull/47))
- _(ingestion_pipeline)_ concurrency improvements ([#48](https://github.com/bosun-ai/swiftide/pull/48))
- _(ingestion_pipeline)_ [**breaking**] support chained storage backends ([#46](https://github.com/bosun-ai/swiftide/pull/46))

### Other

- update linkedin link

## swiftide - [0.2.1](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.0...swiftide-v0.2.1) - 2024-06-13

### Other

- fix documentation link
- Add link to bosun

## swiftide - [0.2.0](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.1.0...swiftide-v0.2.0) - 2024-06-13

### Added

- api improvements with example ([#10](https://github.com/bosun-ai/swiftide/pull/10))
- feat/readme improvements ([#11](https://github.com/bosun-ai/swiftide/pull/11))

### Fixed

- fmt
- clippy & fmt

### Other

- update readme template links and fix template
- _(swiftide)_ documented file swiftide/src/integrations/openai/simple_prompt.rs ([#19](https://github.com/bosun-ai/swiftide/pull/19))
- _(swiftide)_ documented file swiftide/src/transformers/metadata_qa_code.rs ([#34](https://github.com/bosun-ai/swiftide/pull/34))
- _(swiftide)_ documented file swiftide/src/transformers/openai_embed.rs ([#35](https://github.com/bosun-ai/swiftide/pull/35))
- _(swiftide)_ documented file swiftide/src/transformers/metadata_qa_text.rs ([#36](https://github.com/bosun-ai/swiftide/pull/36))
- _(swiftide)_ documented file swiftide/src/transformers/chunk_code.rs ([#39](https://github.com/bosun-ai/swiftide/pull/39))
- _(swiftide)_ documented file swiftide/src/loaders/mod.rs ([#40](https://github.com/bosun-ai/swiftide/pull/40))
- _(swiftide)_ documented file swiftide/tests/ingestion_pipeline.rs ([#41](https://github.com/bosun-ai/swiftide/pull/41))
- _(swiftide)_ documented file swiftide/src/integrations/treesitter/supported_languages.rs ([#26](https://github.com/bosun-ai/swiftide/pull/26))
- _(swiftide)_ documented file swiftide/src/ingestion/mod.rs ([#28](https://github.com/bosun-ai/swiftide/pull/28))
- _(swiftide)_ documented file swiftide/src/integrations/qdrant/ingestion_node.rs ([#20](https://github.com/bosun-ai/swiftide/pull/20))
- _(swiftide)_ documented file swiftide/src/integrations/qdrant/mod.rs ([#22](https://github.com/bosun-ai/swiftide/pull/22))
- _(swiftide)_ documented file swiftide/src/integrations/redis/mod.rs ([#23](https://github.com/bosun-ai/swiftide/pull/23))
- _(swiftide)_ documented file swiftide/src/integrations/qdrant/persist.rs ([#24](https://github.com/bosun-ai/swiftide/pull/24))
- _(swiftide)_ documented file swiftide/src/integrations/redis/node_cache.rs ([#29](https://github.com/bosun-ai/swiftide/pull/29))
- _(swiftide)_ documented file swiftide/src/integrations/treesitter/splitter.rs ([#30](https://github.com/bosun-ai/swiftide/pull/30))
- _(swiftide)_ documented file swiftide/src/integrations/openai/mod.rs ([#21](https://github.com/bosun-ai/swiftide/pull/21))
- _(swiftide)_ documented file swiftide/src/ingestion/ingestion_node.rs ([#15](https://github.com/bosun-ai/swiftide/pull/15))
- _(swiftide)_ documented file swiftide/src/ingestion/ingestion_stream.rs ([#16](https://github.com/bosun-ai/swiftide/pull/16))
- _(swiftide)_ documented file swiftide/src/ingestion/ingestion_pipeline.rs ([#14](https://github.com/bosun-ai/swiftide/pull/14))
- release v0.1.0 ([#8](https://github.com/bosun-ai/swiftide/pull/8))

## swiftide - [0.1.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.1.0) - 2024-06-13

### Added

- _(doc)_ setup basic readme ([#5](https://github.com/bosun-ai/swiftide/pull/5))
- _(ingestion_pipeline)_ default concurrency is the number of cpus ([#6](https://github.com/bosun-ai/swiftide/pull/6))
- fix build and add feature flags for all integrations

### Other

- cleanup Cargo keywords
- configure cargo toml ([#7](https://github.com/bosun-ai/swiftide/pull/7))
- tests, tests, tests ([#4](https://github.com/bosun-ai/swiftide/pull/4))
- cleanup
- restructure repository and rename ([#3](https://github.com/bosun-ai/swiftide/pull/3))
