# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.0...swiftide-v0.2.1) - 2024-06-13

### Other
- fix documentation link
- Add link to bosun

## [0.2.0](https://github.com/bosun-ai/swiftide/compare/swiftide-v0.1.0...swiftide-v0.2.0) - 2024-06-13

### Added
- api improvements with example ([#10](https://github.com/bosun-ai/swiftide/pull/10))
- feat/readme improvements ([#11](https://github.com/bosun-ai/swiftide/pull/11))

### Fixed
- fmt
- clippy & fmt

### Other
- update readme template links and fix template
- *(swiftide)* documented file swiftide/src/integrations/openai/simple_prompt.rs ([#19](https://github.com/bosun-ai/swiftide/pull/19))
- *(swiftide)* documented file swiftide/src/transformers/metadata_qa_code.rs ([#34](https://github.com/bosun-ai/swiftide/pull/34))
- *(swiftide)* documented file swiftide/src/transformers/openai_embed.rs ([#35](https://github.com/bosun-ai/swiftide/pull/35))
- *(swiftide)* documented file swiftide/src/transformers/metadata_qa_text.rs ([#36](https://github.com/bosun-ai/swiftide/pull/36))
- *(swiftide)* documented file swiftide/src/transformers/chunk_code.rs ([#39](https://github.com/bosun-ai/swiftide/pull/39))
- *(swiftide)* documented file swiftide/src/loaders/mod.rs ([#40](https://github.com/bosun-ai/swiftide/pull/40))
- *(swiftide)* documented file swiftide/tests/ingestion_pipeline.rs ([#41](https://github.com/bosun-ai/swiftide/pull/41))
- *(swiftide)* documented file swiftide/src/integrations/treesitter/supported_languages.rs ([#26](https://github.com/bosun-ai/swiftide/pull/26))
- *(swiftide)* documented file swiftide/src/ingestion/mod.rs ([#28](https://github.com/bosun-ai/swiftide/pull/28))
- *(swiftide)* documented file swiftide/src/integrations/qdrant/ingestion_node.rs ([#20](https://github.com/bosun-ai/swiftide/pull/20))
- *(swiftide)* documented file swiftide/src/integrations/qdrant/mod.rs ([#22](https://github.com/bosun-ai/swiftide/pull/22))
- *(swiftide)* documented file swiftide/src/integrations/redis/mod.rs ([#23](https://github.com/bosun-ai/swiftide/pull/23))
- *(swiftide)* documented file swiftide/src/integrations/qdrant/persist.rs ([#24](https://github.com/bosun-ai/swiftide/pull/24))
- *(swiftide)* documented file swiftide/src/integrations/redis/node_cache.rs ([#29](https://github.com/bosun-ai/swiftide/pull/29))
- *(swiftide)* documented file swiftide/src/integrations/treesitter/splitter.rs ([#30](https://github.com/bosun-ai/swiftide/pull/30))
- *(swiftide)* documented file swiftide/src/integrations/openai/mod.rs ([#21](https://github.com/bosun-ai/swiftide/pull/21))
- *(swiftide)* documented file swiftide/src/ingestion/ingestion_node.rs ([#15](https://github.com/bosun-ai/swiftide/pull/15))
- *(swiftide)* documented file swiftide/src/ingestion/ingestion_stream.rs ([#16](https://github.com/bosun-ai/swiftide/pull/16))
- *(swiftide)* documented file swiftide/src/ingestion/ingestion_pipeline.rs ([#14](https://github.com/bosun-ai/swiftide/pull/14))
- release v0.1.0 ([#8](https://github.com/bosun-ai/swiftide/pull/8))

## [0.1.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.1.0) - 2024-06-13

### Added
- *(doc)* setup basic readme ([#5](https://github.com/bosun-ai/swiftide/pull/5))
- *(ingestion_pipeline)* default concurrency is the number of cpus ([#6](https://github.com/bosun-ai/swiftide/pull/6))
- fix build and add feature flags for all integrations

### Other
- cleanup Cargo keywords
- configure cargo toml ([#7](https://github.com/bosun-ai/swiftide/pull/7))
- tests, tests, tests ([#4](https://github.com/bosun-ai/swiftide/pull/4))
- cleanup
- restructure repository and rename ([#3](https://github.com/bosun-ai/swiftide/pull/3))
