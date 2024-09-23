# Changelog

All notable changes to this project will be documented in this file.

## [0.12.3](https://github.com/bosun-ai/swiftide/releases/tag/0.12.3) - 2024-09-23

### Added

- [da5df22](https://github.com/bosun-ai/swiftide/commit/da5df2230da81e9fe1e6ab74150511cbe1e3d769) *(tree-sitter)* Implement Serialize and Deserialize for SupportedLanguages ([#314](https://github.com/bosun-ai/swiftide/pull/314))

### Fixed

- [a756148](https://github.com/bosun-ai/swiftide/commit/a756148f85faa15b1a79db8ec8106f0e15e4d6a2) *(tree-sitter)* Fix javascript and improve tests ([#313](https://github.com/bosun-ai/swiftide/pull/313))

### Other

- [5d52288](https://github.com/bosun-ai/swiftide/commit/5d5228803bc0e90730598eac7973443944f749e3) *(readme)* Add blog links and update features ([#312](https://github.com/bosun-ai/swiftide/pull/312))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.12.2...0.12.3



## [0.12.2](https://github.com/bosun-ai/swiftide/releases/tag/0.12.2) - 2024-09-18

### Other

- [d84814e](https://github.com/bosun-ai/swiftide/commit/d84814eef1bf12e485053fb69fb658d963100789) *(uncategorized)* Fix broken documentation links and other cargo doc warnings ([#304](https://github.com/bosun-ai/swiftide/pull/304))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.12.1...0.12.2



## [0.12.1](https://github.com/bosun-ai/swiftide/releases/tag/0.12.1) - 2024-09-16

### Added

- [ec227d2](https://github.com/bosun-ai/swiftide/commit/ec227d25b987b7fd63ab1b3862ef19b14632bd04) *(indexing,query)* Add concise info log with transformation name

- [081a248](https://github.com/bosun-ai/swiftide/commit/081a248e67292c1800837315ec53583be5e0cb82) *(query)* Improve query performance similar to indexing in 0.12

- [01cf579](https://github.com/bosun-ai/swiftide/commit/01cf579922a877bb78e0de20114ade501e5a63db) *(query)* Add query_mut for reusable query pipelines

- [8029926](https://github.com/bosun-ai/swiftide/commit/80299269054eb440e55a42667a7bcc9ba6514a7b) *(query,indexing)* Add duration in log output on pipeline completion

### Fixed

- [d62b047](https://github.com/bosun-ai/swiftide/commit/d62b0478872e460956607f52b72470b76eb32d91) *(ci)* Update testcontainer images and fix tests

- [39b6ecb](https://github.com/bosun-ai/swiftide/commit/39b6ecb6175e5233b129f94876f95182b8bfcdc3) *(core)* Truncate long strings safely when printing debug logs

- [16e9c74](https://github.com/bosun-ai/swiftide/commit/16e9c7455829100b9ae82305e5a1d2568264af9f) *(openai)* Reduce debug verbosity

- [6914d60](https://github.com/bosun-ai/swiftide/commit/6914d607717294467cddffa867c3d25038243fc1) *(qdrant)* Reduce debug verbosity when storing nodes

- [3d13889](https://github.com/bosun-ai/swiftide/commit/3d1388973b5e2a135256ae288d47dbde0399487f) *(query)* Reduce and improve debugging verbosity

- [133cf1d](https://github.com/bosun-ai/swiftide/commit/133cf1d0be09049ca3e90b45675a965bb2464cb2) *(query)* Remove verbose debug and skip self in instrumentation

- [a871c61](https://github.com/bosun-ai/swiftide/commit/a871c61ad52ed181d6f9cb6a66ed07bccaadee08) *(uncategorized)* Fmt

- [ce17981](https://github.com/bosun-ai/swiftide/commit/ce179819ab75460453236723c7f9a89fd61fb99a) *(uncategorized)* Clippy

### Other

- [214ee8d](https://github.com/bosun-ai/swiftide/commit/214ee8d2850f61c275fe5b743ba63ae8acb618ec) *(readme)* Add link to latest release post


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.12.0...0.12.1



## [0.12.0](https://github.com/bosun-ai/swiftide/releases/tag/0.12.0) - 2024-09-13

### Added

- [e902cb7](https://github.com/bosun-ai/swiftide/commit/e902cb7487221d3e88f13d88532da081e6ef8611) *(query)* Add support for filters in SimilaritySingleEmbedding ([#298](https://github.com/bosun-ai/swiftide/pull/298))

- [f158960](https://github.com/bosun-ai/swiftide/commit/f1589604d1e0cb42a07d5a48080e3d7ecb90ee38) *(uncategorized)* Major performance improvements ([#291](https://github.com/bosun-ai/swiftide/pull/291))

### Fixed

- [45d8a57](https://github.com/bosun-ai/swiftide/commit/45d8a57d1afb4f16ad76b15236308d753cf45743) *(ci)* Use llm-cov preview via nightly and improve test coverage ([#289](https://github.com/bosun-ai/swiftide/pull/289))

- [501dd39](https://github.com/bosun-ai/swiftide/commit/501dd391aed6fe6bdec1a2baeba114489604f153) *(deps)* Update rust crate redis to 0.27 ([#294](https://github.com/bosun-ai/swiftide/pull/294))

- [f95f806](https://github.com/bosun-ai/swiftide/commit/f95f806a0701b14a3cad5da307c27c01325a264d) *(indexing)* Debugging nodes should respect utf8 char boundaries

- [f8314cc](https://github.com/bosun-ai/swiftide/commit/f8314ccdbe16ad7e6691899dd01f81a61b20180f) *(indexing)* Limit logged chunk to max 100 chars ([#292](https://github.com/bosun-ai/swiftide/pull/292))

- [9464ca1](https://github.com/bosun-ai/swiftide/commit/9464ca123f08d8dfba3f1bfabb57e9af97018534) *(uncategorized)* Bad embed error propagation ([#293](https://github.com/bosun-ai/swiftide/pull/293))

- [8595553](https://github.com/bosun-ai/swiftide/commit/859555334d7e4129215b9f084d9f9840fac5ce36) *(uncategorized)* Implement into_stream_boxed for all loaders

### Other

- [37c4bd9](https://github.com/bosun-ai/swiftide/commit/37c4bd9f9ac97646adb2c4b99b8f7bf0bee4c794) *(deps)* Update treesitter ([#296](https://github.com/bosun-ai/swiftide/pull/296))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.11.1...0.12.0



## [0.11.1](https://github.com/bosun-ai/swiftide/releases/tag/0.11.1) - 2024-09-10

### Fixed

- [dfa546b](https://github.com/bosun-ai/swiftide/commit/dfa546b310e71a7cb78a927cc8f0ee4e2046a592) *(uncategorized)* Add missing parquet feature flag


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.11.0...0.11.1



## [0.11.0](https://github.com/bosun-ai/swiftide/releases/tag/0.11.0) - 2024-09-08

### Added

- [bdf17ad](https://github.com/bosun-ai/swiftide/commit/bdf17adf5d3addc84aaf45ad893b816cb46431e3) *(indexing)* Parquet loader ([#279](https://github.com/bosun-ai/swiftide/pull/279))

- [a98dbcb](https://github.com/bosun-ai/swiftide/commit/a98dbcb455d33f0537cea4d3614da95f1a4b6554) *(integrations)* Add ollama embeddings support ([#278](https://github.com/bosun-ai/swiftide/pull/278))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.10.0...0.11.0


# Changelog

All notable changes to this project will be documented in this file.

## [0.10.0](https://github.com/bosun-ai/swiftide/releases/tag/0.10.0) - 2024-09-06

BREAKING CHANGE: Indexing nodes now have their ID calculated using UUIDv3 via MD5 as the previous algorithm was unreliable and broke in 1.81. Added benefit that collision chance is even smaller. This means that when indexing again, nodes will have different IDs and upsert will not work. Backwards compatibility is non-trivial. If this is a huge issue, ping us on discord and we will look into it.

### Added

- [57fe4aa](https://github.com/bosun-ai/swiftide/commit/57fe4aa73b1b98dd8eac87c6440e0f2a0c66d4e8) *(indexing)* Use UUIDv3 for indexing node ids ([#277](https://github.com/bosun-ai/swiftide/pull/277))

### Fixed

- [5a724df](https://github.com/bosun-ai/swiftide/commit/5a724df895d35cfa606721d611afd073a23191de) *(uncategorized)* Rust 1.81 support ([#275](https://github.com/bosun-ai/swiftide/pull/275))

### Other

- [3711f6f](https://github.com/bosun-ai/swiftide/commit/3711f6fb2b51e97e4606b744cc963c04b44b6963) *(readme)* Fix date ([#273](https://github.com/bosun-ai/swiftide/pull/273))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.9.2...0.10.0



## [0.9.2](https://github.com/bosun-ai/swiftide/releases/tag/0.9.2) - 2024-09-04

### Added

- [84e9bae](https://github.com/bosun-ai/swiftide/commit/84e9baefb366f0a949ae7dcbdd8f97931da0b4be) *(indexing)* Add chunker for text with text_splitter ([#270](https://github.com/bosun-ai/swiftide/pull/270))

- [387fbf2](https://github.com/bosun-ai/swiftide/commit/387fbf29c2bce06284548f9af146bb3969562761) *(query)* Hybrid search for qdrant in query pipeline ([#260](https://github.com/bosun-ai/swiftide/pull/260))

### Fixed

- [6e92b12](https://github.com/bosun-ai/swiftide/commit/6e92b12faa020f12ef5e770282e7b2e854f4910c) *(deps)* Update rust crate text-splitter to 0.16 ([#267](https://github.com/bosun-ai/swiftide/pull/267))

### Other

- [1dc4c90](https://github.com/bosun-ai/swiftide/commit/1dc4c90436c9c8c8d0eb080e300afce53090c73e) *(readme)* Add new blog links

- [064c7e1](https://github.com/bosun-ai/swiftide/commit/064c7e157775a7aaf9628a39f941be35ce0be99a) *(readme)* Update intro


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.9.1...0.9.2



## [0.9.1](https://github.com/bosun-ai/swiftide/releases/tag/0.9.1) - 2024-09-01

### Added

- [b891f93](https://github.com/bosun-ai/swiftide/commit/b891f932e43b9c76198d238bcde73a6bb1dfbfdb) *(integrations)* Add fluvio as loader support ([#243](https://github.com/bosun-ai/swiftide/pull/243))

- [c00b6c8](https://github.com/bosun-ai/swiftide/commit/c00b6c8f08fca46451387f3034d3d53805f3e401) *(query)* Ragas support ([#236](https://github.com/bosun-ai/swiftide/pull/236))

- [a1250c1](https://github.com/bosun-ai/swiftide/commit/a1250c1cef57e2b74760fd31772e106993a3b079) *(uncategorized)* LanceDB support ([#254](https://github.com/bosun-ai/swiftide/pull/254))

### Fixed

- [d5a76ae](https://github.com/bosun-ai/swiftide/commit/d5a76aef7890fd2c17f720cfb43dafc7333c3bf9) *(deps)* Update rust crate fastembed to v4 ([#250](https://github.com/bosun-ai/swiftide/pull/250))

- [cc7ec08](https://github.com/bosun-ai/swiftide/commit/cc7ec0849d7398561c1ff1c48037458e7d4e23fa) *(deps)* Update rust crate spider to v2 ([#237](https://github.com/bosun-ai/swiftide/pull/237))

### Other

- [fb381b8](https://github.com/bosun-ai/swiftide/commit/fb381b8896a5fc863a4185445ce51fefb99e6c11) *(readme)* Copy improvements ([#261](https://github.com/bosun-ai/swiftide/pull/261))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.9.0...0.9.1



## [0.9.0](https://github.com/bosun-ai/swiftide/releases/tag/0.9.0) - 2024-08-15

### Added

- [2443933](https://github.com/bosun-ai/swiftide/commit/24439339a9b935befcbcc92e56c01c5048605138) *(qdrant)* Add access to inner client for custom operations ([#242](https://github.com/bosun-ai/swiftide/pull/242))

- [4fff613](https://github.com/bosun-ai/swiftide/commit/4fff613b461e8df993327cb364cabc65cd5901d8) *(query)* Add concurrency on query pipeline and add query_all

### Fixed

- [8a1cc69](https://github.com/bosun-ai/swiftide/commit/8a1cc69712b4361893c0564c7d6f7d1ed21e5710) *(query)* After retrieval current transormation should be empty

### Other

- [3d213b4](https://github.com/bosun-ai/swiftide/commit/3d213b40d0b2d1dd259dd22ba99614fedae64353) *(readme)* Add link to 0.8 release

- [e9d0016](https://github.com/bosun-ai/swiftide/commit/e9d00160148807a8e2d1df1582e6ea85cfd2d8d0) *(indexing,integrations)* Move tree-sitter dependencies to integrations ([#235](https://github.com/bosun-ai/swiftide/pull/235))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.8.0...0.9.0



## [0.8.0](https://github.com/bosun-ai/swiftide/releases/tag/0.8.0) - 2024-08-12

### Added

- [67336f1](https://github.com/bosun-ai/swiftide/commit/67336f1d9c7fde474bdddfd0054b40656df244e0) *(indexing)* Sparse vector support with Splade and Qdrant ([#222](https://github.com/bosun-ai/swiftide/pull/222))

- [2e25ad4](https://github.com/bosun-ai/swiftide/commit/2e25ad4b999a8562a472e086a91020ec4f8300d8) *(indexing)* Default LLM for indexing pipeline and boilerplate Transformer macro ([#227](https://github.com/bosun-ai/swiftide/pull/227))

- [e728a7c](https://github.com/bosun-ai/swiftide/commit/e728a7c7a2fcf7b22c31e5d6c66a896f634f6901) *(uncategorized)* Code outlines in chunk metadata ([#137](https://github.com/bosun-ai/swiftide/pull/137))

### Fixed

- [3cce606](https://github.com/bosun-ai/swiftide/commit/3cce60698cb59a0f1d3902e85ff6b07555f6de58) *(deps)* Update rust crate text-splitter to 0.15 ([#224](https://github.com/bosun-ai/swiftide/pull/224))

### Other

- [4970a68](https://github.com/bosun-ai/swiftide/commit/4970a683acccc71503e64044dc02addaf2e9c87c) *(readme)* Fix discord links

- [b3f04de](https://github.com/bosun-ai/swiftide/commit/b3f04defe94e5b26876c8d99049f4d87b5f2dc18) *(readme)* Add link to discord ([#219](https://github.com/bosun-ai/swiftide/pull/219))

- [73d1649](https://github.com/bosun-ai/swiftide/commit/73d1649ca8427aa69170f6451eac55316581ed9a) *(readme)* Add Ollama support to README


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.7.1...0.8.0


# Changelog

All notable changes to this project will be documented in this file.

## [0.7.1](https://github.com/bosun-ai/swiftide/releases/tag/0.7.1) - 2024-08-04

### Features

- [53e662b](https://github.com/bosun-ai/swiftide/commit/53e662b8c30f6ac6d11863685d3850ab48397766) *(ci)* Add cargo deny to lint dependencies ([#213](https://github.com/bosun-ai/swiftide/pull/213))

- [b2d31e5](https://github.com/bosun-ai/swiftide/commit/b2d31e555cb8da525513490e7603df1f6b2bfa5b) *(integrations)* Add ollama support ([#214](https://github.com/bosun-ai/swiftide/pull/214))
  
- [9eb5894](https://github.com/bosun-ai/swiftide/commit/9eb589416c2a56f9942b6f6bed3771cec6acebaf) *(query)* Support arbitrary closures in all steps ([#215](https://github.com/bosun-ai/swiftide/pull/215))

### Documentation

- [f7accde](https://github.com/bosun-ai/swiftide/commit/f7accdeecf01efc291503282554257846725ce57) *(readme)* Add 0.7 announcement

- [ba07ab9](https://github.com/bosun-ai/swiftide/commit/ba07ab93722d974ac93ed5d4a22bf53317bc11ae) *(readme)* Readme improvements

- [1539393](https://github.com/bosun-ai/swiftide/commit/15393932dd756af134a12f7954faa75893f8c3fb) *(readme)* Update README.md


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.7.0...0.7.1


<!-- generated by git-cliff -->


## [0.7.0](https://github.com/bosun-ai/swiftide/releases/tag/0.7.0) - 2024-07-28

### Features

- [ec1fb04](https://github.com/bosun-ai/swiftide/commit/ec1fb04573ab75fe140cbeff17bc3179e316ff0c) *(indexing)* Metadata as first class citizen ([#204](https://github.com/bosun-ai/swiftide/pull/204))

- [16bafe4](https://github.com/bosun-ai/swiftide/commit/16bafe4da8c98adcf90f5bb63070832201c405b9) *(swiftide)* Rework workspace preparing for swiftide-query ([#199](https://github.com/bosun-ai/swiftide/pull/199))

- [63694d2](https://github.com/bosun-ai/swiftide/commit/63694d2892a7c97a7e7fc42664d550c5acd7bb12) *(swiftide-query)* Query pipeline v1 ([#189](https://github.com/bosun-ai/swiftide/pull/189))

### Documentation

- [2114aa4](https://github.com/bosun-ai/swiftide/commit/2114aa4394f4eda2e6465e1adb5602ae1b3ff61f) *(readme)* Add copy on the query pipeline


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.7...0.7.0


<!-- generated by git-cliff -->


## [0.6.7](https://github.com/bosun-ai/swiftide/releases/tag/0.6.7) - 2024-07-22

### Features

- [beea449](https://github.com/bosun-ai/swiftide/commit/beea449301b89fde1915c5336a071760c1963c75) *(prompt)* Add Into for strings to PromptTemplate ([#193](https://github.com/bosun-ai/swiftide/pull/193))

- [f3091f7](https://github.com/bosun-ai/swiftide/commit/f3091f72c74e816f6b9b8aefab058d610becb625) *(transformers)* References and definitions from code ([#186](https://github.com/bosun-ai/swiftide/pull/186))

### Documentation

- [97a572e](https://github.com/bosun-ai/swiftide/commit/97a572ec2e3728bbac82c889bf5129b048e61e0c) *(readme)* Add blog posts and update doc link ([#194](https://github.com/bosun-ai/swiftide/pull/194))

- [504fe26](https://github.com/bosun-ai/swiftide/commit/504fe2632cf4add506dfb189c17d6e4ecf6f3824) *(pipeline)* Add note that closures can also be used as transformers


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.6...0.6.7


<!-- generated by git-cliff -->


## [0.6.6](https://github.com/bosun-ai/swiftide/releases/tag/0.6.6) - 2024-07-16

### Features

- [d1c642a](https://github.com/bosun-ai/swiftide/commit/d1c642aa4ee9b373e395a78591dd36fa0379a4ff) *(groq)* Add SimplePrompt support for Groq ([#183](https://github.com/bosun-ai/swiftide/pull/183))

### Documentation

- [143c7c9](https://github.com/bosun-ai/swiftide/commit/143c7c9c2638737166f23f2ef8106b7675f6e19b) *(readme)* Fix typo ([#180](https://github.com/bosun-ai/swiftide/pull/180))

- [d393181](https://github.com/bosun-ai/swiftide/commit/d3931818146bff72499ebfcc0d0e8c8bb13a760d) *(docsrs)* Scrape examples and fix links ([#184](https://github.com/bosun-ai/swiftide/pull/184))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.5...0.6.6


<!-- generated by git-cliff -->


## [0.6.5](https://github.com/bosun-ai/swiftide/releases/tag/0.6.5) - 2024-07-15

### Features

- [0065c7a](https://github.com/bosun-ai/swiftide/commit/0065c7a7fd1289ea227391dd7b9bd51c905290d5) *(prompt)* Add extending the prompt repository ([#178](https://github.com/bosun-ai/swiftide/pull/178))

### Documentation

- [b95b395](https://github.com/bosun-ai/swiftide/commit/b95b3955f89ed231cc156dab749ee7bb8be98ee5) *(swiftide)* Documentation improvements and cleanup ([#176](https://github.com/bosun-ai/swiftide/pull/176))

### Miscellaneous Tasks

- [73d5fa3](https://github.com/bosun-ai/swiftide/commit/73d5fa37d23f53919769c2ffe45db2e3832270ef) *(traits)* Cleanup unused batch size in `BatchableTransformer` ([#177](https://github.com/bosun-ai/swiftide/pull/177))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.4...0.7.0


<!-- generated by git-cliff -->


## [0.6.4](https://github.com/bosun-ai/swiftide/releases/tag/0.6.4) - 2024-07-14

### Bug Fixes

- [b54691f](https://github.com/bosun-ai/swiftide/commit/b54691f769e2d0ac7886938b6e837551926eea2f) *(prompts)* Include default prompts in crate ([#174](https://github.com/bosun-ai/swiftide/pull/174))


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.3...0.6.4


<!-- generated by git-cliff -->


## [0.6.3](https://github.com/bosun-ai/swiftide/releases/tag/0.6.3) - 2024-07-14

### Bug Fixes

- [47418b5](https://github.com/bosun-ai/swiftide/commit/47418b5d729aef1e2ff77dabd7e29b5131512b01) *(prompts)* Fix breaking issue with prompts not found


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.1...0.6.3


<!-- generated by git-cliff -->


<!-- generated by git-cliff -->


## [0.6.1](https://github.com/bosun-ai/swiftide/releases/tag/0.6.1) - 2024-07-12

### Documentation

- [085709f](https://github.com/bosun-ai/swiftide/commit/085709fd767bab7153b2222907fc500ad4412570) *(docsrs)* Disable unstable and rustdoc scraping


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/0.6.0...0.6.1


<!-- generated by git-cliff -->


## [0.6.0](https://github.com/bosun-ai/swiftide/releases/tag/0.6.0) - 2024-07-12

### Features

- [70ea268](https://github.com/bosun-ai/swiftide/commit/70ea268b19e564af83bb834f56d406a05e02e9cd) *(prompts)* Add prompts as first class citizens (#145) by @timonv in [#145](https://github.com/bosun-ai/swiftide/pull/145)

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

- [699cfe4](https://github.com/bosun-ai/swiftide/commit/699cfe44fb0e3baddba695ad09836caec7cb30a6) *(uncategorized)* Embed modes and named vectors (#123) by @pwalski in [#123](https://github.com/bosun-ai/swiftide/pull/123)

````text
Added named vector support to qdrant. A pipeline can now have its embed
  mode configured, either per field, chunk and metadata combined (default)
  or both. Vectors need to be configured on the qdrant client side.

  See `examples/store_multiple_vectors.rs` for an example.

  Shoutout to @pwalski for the contribution. Closes #62.

  ---------
````

### Bug Fixes

- [9334934](https://github.com/bosun-ai/swiftide/commit/9334934e4af92b35dbc61e1f92aa90abac29ca12) *(chunkcode)* Use correct chunksizes (#122) by @timonv in [#122](https://github.com/bosun-ai/swiftide/pull/122)

- [c5bf796](https://github.com/bosun-ai/swiftide/commit/c5bf7960ca6bec498cdc987fe7676acfef702e5b) *(ci)* Add clippy back to ci (#147) by @timonv in [#147](https://github.com/bosun-ai/swiftide/pull/147)

- [4c9ed77](https://github.com/bosun-ai/swiftide/commit/4c9ed77c85b7dd0e8722388b930d169cd2e5a5c7) *(ci)* Properly check if contributors are present by @timonv

- [5de6af4](https://github.com/bosun-ai/swiftide/commit/5de6af42b9a1e95b0fbd54659c0d590db1d76222) *(ci)* Only add contributors if present by @timonv

- [eb8364e](https://github.com/bosun-ai/swiftide/commit/eb8364e08a9202476cca6b60fbdfbb31fe0e1c3d) *(ci)* Try overriding the github repo for git cliff by @timonv

- [c3aee48](https://github.com/bosun-ai/swiftide/commit/c3aee48647915af04f4c2cef76b40ad0ef92e6bb) *(deps)* Update rust crate spider to v1.98.9 (#146) by @renovate[bot] in [#146](https://github.com/bosun-ai/swiftide/pull/146)

- [6d75f14](https://github.com/bosun-ai/swiftide/commit/6d75f145da7f9a26dc11dff64a161cb1e686dc96) *(deps)* Update rust crate htmd to v0.1.6 (#144) by @renovate[bot] in [#144](https://github.com/bosun-ai/swiftide/pull/144)

- [a691d61](https://github.com/bosun-ai/swiftide/commit/a691d614d46d3bcff2811f354cd805f9d229d4e0) *(deps)* Update rust crate async-openai to v0.23.4 (#136) by @renovate[bot] in [#136](https://github.com/bosun-ai/swiftide/pull/136)

- [8e22937](https://github.com/bosun-ai/swiftide/commit/8e22937427b928524dacf2b446feeff726b6a5e1) *(deps)* Update rust crate aws-sdk-bedrockruntime to v1.39.0 (#143) by @renovate[bot] in [#143](https://github.com/bosun-ai/swiftide/pull/143)

- [bf3b677](https://github.com/bosun-ai/swiftide/commit/bf3b6778103e33abcdb4979196fef458b09c7dc0) *(deps)* Update rust crate fastembed to v3.9.0 (#141) by @renovate[bot] in [#141](https://github.com/bosun-ai/swiftide/pull/141)

- [2b13523](https://github.com/bosun-ai/swiftide/commit/2b1352322e574b62cb30268b35c6b510122f0584) *(deps)* Update rust crate fastembed to v3.7.1 (#135) by @renovate[bot] in [#135](https://github.com/bosun-ai/swiftide/pull/135)

- [dd32ef3](https://github.com/bosun-ai/swiftide/commit/dd32ef3b1be7cd6888d2961053d0b3c1a882e1a4) *(deps)* Update rust crate async-trait to v0.1.81 (#134) by @renovate[bot] in [#134](https://github.com/bosun-ai/swiftide/pull/134)

- [adc4bf7](https://github.com/bosun-ai/swiftide/commit/adc4bf789f679079fcc9fac38f4a7b8f98816844) *(deps)* Update aws-sdk-rust monorepo (#125) by @renovate[bot] in [#125](https://github.com/bosun-ai/swiftide/pull/125)

- [7af97b5](https://github.com/bosun-ai/swiftide/commit/7af97b589ca45f2b966ea2f61ebef341c881f1f9) *(deps)* Update rust crate spider to v1.98.7 (#124) by @renovate[bot] in [#124](https://github.com/bosun-ai/swiftide/pull/124)

- [ff92abd](https://github.com/bosun-ai/swiftide/commit/ff92abd95908365c72d96abff37e0284df8fed32) *(deps)* Update rust crate tree-sitter-javascript to v0.21.4 (#126) by @renovate[bot] in [#126](https://github.com/bosun-ai/swiftide/pull/126)

- [9c261b8](https://github.com/bosun-ai/swiftide/commit/9c261b87dde2e0caaff0e496d15681466844daf4) *(deps)* Update rust crate text-splitter to v0.14.1 (#127) by @renovate[bot] in [#127](https://github.com/bosun-ai/swiftide/pull/127)

- [28f5b04](https://github.com/bosun-ai/swiftide/commit/28f5b048f5acd977915ae20463f8fbb473dfab9a) *(deps)* Update rust crate tree-sitter-typescript to v0.21.2 (#128) by @renovate[bot] in [#128](https://github.com/bosun-ai/swiftide/pull/128)

- [dfc76dd](https://github.com/bosun-ai/swiftide/commit/dfc76ddfc23d9314fe88c8362bf53d7865a03302) *(deps)* Update rust crate serde to v1.0.204 (#129) by @renovate[bot] in [#129](https://github.com/bosun-ai/swiftide/pull/129)

- [3b98334](https://github.com/bosun-ai/swiftide/commit/3b98334b2bf78cfe9c957bfa1dd3cd7c939b6c39) *(deps)* Update rust crate serde_json to v1.0.120 (#115) by @renovate[bot] in [#115](https://github.com/bosun-ai/swiftide/pull/115)

- [7357fea](https://github.com/bosun-ai/swiftide/commit/7357fea0a8cd826904b0545e80d4d1a1659df064) *(deps)* Update rust crate spider to v1.98.6 (#119) by @renovate[bot] in [#119](https://github.com/bosun-ai/swiftide/pull/119)

- [353cd9e](https://github.com/bosun-ai/swiftide/commit/353cd9ed36fcf6fb8f1db255d8b5f4a914ca8496) *(qdrant)* Upgrade and better defaults (#118) by @timonv in [#118](https://github.com/bosun-ai/swiftide/pull/118)

````text
- **fix(deps): update rust crate qdrant-client to v1.10.1**
  - **fix(qdrant): upgrade to new qdrant with sensible defaults**
  - **feat(qdrant): safe to clone with internal arc**

  ---------
````

- [b53636c](https://github.com/bosun-ai/swiftide/commit/b53636cbd8f179f248cc6672aaf658863982c603) *(uncategorized)* Inability to store only some of `EmbeddedField`s (#139) by @pwalski in [#139](https://github.com/bosun-ai/swiftide/pull/139)
Fixes:#138

---------

### Documentation

- [8405c9e](https://github.com/bosun-ai/swiftide/commit/8405c9efedef944156c2904eb709ba79aa4d82de) *(contributing)* Add guidelines on code design (#113) by @timonv in [#113](https://github.com/bosun-ai/swiftide/pull/113)

- [5691ac9](https://github.com/bosun-ai/swiftide/commit/5691ac930fd6547c3f0166b64ead0ae647c38883) *(readme)* Add preproduction warning by @timonv

- [4c40e27](https://github.com/bosun-ai/swiftide/commit/4c40e27e5c6735305c70696ddf71dd5f95d03bbb) *(readme)* Add back coverage badge by @timonv

- [3e447fe](https://github.com/bosun-ai/swiftide/commit/3e447feab83a4bf8d7d9d8220fe1b92dede9af79) *(readme)* Link to CONTRIBUTING (#114) by @timonv in [#114](https://github.com/bosun-ai/swiftide/pull/114)

- [37af322](https://github.com/bosun-ai/swiftide/commit/37af3225b4c3464aa4ed67f8f456c26f3d445507) *(rustdocs)* Rewrite the initial landing page (#149) by @timonv in [#149](https://github.com/bosun-ai/swiftide/pull/149)

````text
- **Add homepage and badges to cargo toml**
  - **documentation landing page improvements**
````

- [7686c2d](https://github.com/bosun-ai/swiftide/commit/7686c2d449b5df0fddc08b111174357d47459f86) *(uncategorized)* Templated prompts are now a major feature by @timonv

### Performance

- [ea8f823](https://github.com/bosun-ai/swiftide/commit/ea8f8236cdd9c588e55ef78f9eac27db1f13b2d9) *(uncategorized)* Improve local build performance and crate cleanup (#148) by @timonv in [#148](https://github.com/bosun-ai/swiftide/pull/148)

````text
- **tune cargo for faster builds**
  - **perf(swiftide): increase local build performance**
````

### Miscellaneous Tasks

- [364e13d](https://github.com/bosun-ai/swiftide/commit/364e13d83285317a1fb99889f6d74ad32b58c482) *(swiftide)* Loosen up dependencies (#140) by @timonv in [#140](https://github.com/bosun-ai/swiftide/pull/140)

````text
Loosen up dependencies so swiftide is a bit more flexible to add to
  existing projects
````

- [3d235dd](https://github.com/bosun-ai/swiftide/commit/3d235ddba9bda8cd925da8007dac229dcb1c485b) *(uncategorized)* Release

- [d2a9ea1](https://github.com/bosun-ai/swiftide/commit/d2a9ea1e7afa6f192bf9c32bbb54d9bb6e46472e) *(uncategorized)* Enable clippy pedantic (#132) by @timonv in [#132](https://github.com/bosun-ai/swiftide/pull/132)

- [51c114c](https://github.com/bosun-ai/swiftide/commit/51c114ceb06db840c4952d3d0f694bfbf266681c) *(uncategorized)* Various tooling & community improvements (#131) by @timonv in [#131](https://github.com/bosun-ai/swiftide/pull/131)

````text
- **fix(ci): ensure clippy runs with all features**
  - **chore(ci): coverage using llvm-cov**
  - **chore: drastically improve changelog generation**
  - **chore(ci): add sanity checks for pull requests**
  - **chore(ci): split jobs and add typos**
````

- [84dd65d](https://github.com/bosun-ai/swiftide/commit/84dd65dc6c0ff4595f27ed061a4f4c0a2dae7202) *(uncategorized)* Rename all mentions of ingest to index (#130) by @timonv in [#130](https://github.com/bosun-ai/swiftide/pull/130) [**breaking**]

````text
Swiftide is not an ingestion pipeline (loading data), but an indexing
  pipeline (prepping for search).

  There is now a temporary, deprecated re-export to match the previous api.
````

### New Contributors
* @pwalski made their first contribution in [#139](https://github.com/bosun-ai/swiftide/pull/139)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.5.0...0.6.0


## [swiftide-v0.5.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.5.0) - 2024-07-01

### Features

- [8812fbf](https://github.com/bosun-ai/swiftide/commit/8812fbf30b882b68bf25f3d56b3ddf17af0bcb7a) *(ingestion_pipeline)* Build a pipeline from a stream by @timonv

- [5aeb3a7](https://github.com/bosun-ai/swiftide/commit/5aeb3a7fb75b21b2f24b111e9640ea4985b2e316) *(ingestion_pipeline)* Splitting and merging streams by @timonv

- [6a88651](https://github.com/bosun-ai/swiftide/commit/6a88651df8c6b91add03acfc071fb9479545b8af) *(ingestion_pipeline)* Implement filter (#109) by @timonv in [#109](https://github.com/bosun-ai/swiftide/pull/109)

- [6101bed](https://github.com/bosun-ai/swiftide/commit/6101bed812c5167eb87a4093d66005140517598d) *(uncategorized)* AWS bedrock support (#92) by @timonv in [#92](https://github.com/bosun-ai/swiftide/pull/92)

````text
Adds an integration with AWS Bedrock, implementing SimplePrompt for
  Anthropic and Titan models. More can be added if there is a need. Same
  for the embedding models.
````

### Bug Fixes

- [17a2be1](https://github.com/bosun-ai/swiftide/commit/17a2be1de6c0f3bda137501db4b1703f9ed0b1c5) *(changelog)* Add scope by @timonv

- [46752db](https://github.com/bosun-ai/swiftide/commit/46752dbfc8ccd578ddba915fd6cd6509e3e6fb14) *(ci)* Add concurrency configuration by @timonv

- [b155de6](https://github.com/bosun-ai/swiftide/commit/b155de6387ddfe64d1a177b31c8e1ed93739b2c9) *(ci)* Fix naming of github actions by @timonv

- [2dbf14c](https://github.com/bosun-ai/swiftide/commit/2dbf14c34bed2ee40ab79c0a46d011cd20882bda) *(ci)* Fix benchmarks in ci by @timonv

- [3cc2e06](https://github.com/bosun-ai/swiftide/commit/3cc2e06b279b4ad3bd22cc0c5a36b63f1a32b90a) *(ci)* Fix release-plz changelog parsing by @timonv

- [2650605](https://github.com/bosun-ai/swiftide/commit/2650605fa05f97c21bf0ab07cc7ef769efaac906) *(deps)* Update rust crate serde_json to v1.0.119 (#110) by @renovate[bot] in [#110](https://github.com/bosun-ai/swiftide/pull/110)

- [5c16c8e](https://github.com/bosun-ai/swiftide/commit/5c16c8e8fd732588021e01c887ddde82deb8b982) *(deps)* Update rust crate strum to v0.26.3 (#101) by @renovate[bot] in [#101](https://github.com/bosun-ai/swiftide/pull/101)

- [52cf37b](https://github.com/bosun-ai/swiftide/commit/52cf37bde5a1fbac7b34b1e21e697d6f4640fd92) *(deps)* Update rust crate fastembed to v3.7.0 (#104) by @renovate[bot] in [#104](https://github.com/bosun-ai/swiftide/pull/104)

- [2401414](https://github.com/bosun-ai/swiftide/commit/240141461060a41efd4ce245a25952ece1095bdc) *(deps)* Update rust crate text-splitter to 0.14.0 (#105) by @renovate[bot] in [#105](https://github.com/bosun-ai/swiftide/pull/105)

- [4c019eb](https://github.com/bosun-ai/swiftide/commit/4c019eb9d39766e870bcd6b9cb59cc350c8abae8) *(deps)* Update rust crate htmd to v0.1.5 (#96) by @renovate[bot] in [#96](https://github.com/bosun-ai/swiftide/pull/96)

- [8e15004](https://github.com/bosun-ai/swiftide/commit/8e150045373af95c65809a0c97af813591193e33) *(deps)* Update rust crate serde_json to v1.0.118 (#99) by @renovate[bot] in [#99](https://github.com/bosun-ai/swiftide/pull/99)

- [9b4ef81](https://github.com/bosun-ai/swiftide/commit/9b4ef816d7dcab52be499fc1eb88b688d3e75b97) *(deps)* Update rust crate spider to v1.98.3 (#100) by @renovate[bot] in [#100](https://github.com/bosun-ai/swiftide/pull/100)

- [a12cce2](https://github.com/bosun-ai/swiftide/commit/a12cce230032eebe2f7ff1aa9cdc85b8fc200eb1) *(openai)* Add tests for builder by @timonv in [#108](https://github.com/bosun-ai/swiftide/pull/108)

- [963919b](https://github.com/bosun-ai/swiftide/commit/963919b0947faeb7d96931c19e524453ad4a0007) *(transformers)* Fix too small chunks being retained and api by @timonv [**breaking**]

- [cba981a](https://github.com/bosun-ai/swiftide/commit/cba981a317a80173eff2946fc551d1a36ec40f65) *(uncategorized)* Replace unwrap with expect and add comment on panic by @timonv in [#111](https://github.com/bosun-ai/swiftide/pull/111)

- [6430af7](https://github.com/bosun-ai/swiftide/commit/6430af7b57eecb7fdb954cd89ade4547b8e92dbd) *(uncategorized)* Use native cargo bench format and only run benchmarks crate by @timonv

- [2c31513](https://github.com/bosun-ai/swiftide/commit/2c31513a0ded87addd0519bbfdd63b5abed29f73) *(uncategorized)* Just use keepachangelog by @timonv

- [e8198d8](https://github.com/bosun-ai/swiftide/commit/e8198d81354bbca2c21ca08b9522d02b8c93173b) *(uncategorized)* Use git cliff manually for changelog generation by @timonv

- [5e8da00](https://github.com/bosun-ai/swiftide/commit/5e8da008ce08a23377672a046a4cedd48d4cf30c) *(uncategorized)* Fix oversight in ingestion pipeline tests by @timonv

### Documentation

- [929410c](https://github.com/bosun-ai/swiftide/commit/929410cb1c2d81b6ffaec4c948c891472835429d) *(readme)* Add diagram to the readme (#107) by @timonv in [#107](https://github.com/bosun-ai/swiftide/pull/107)

- [b014f43](https://github.com/bosun-ai/swiftide/commit/b014f43aa187881160245b4356f95afe2c6fe98c) *(uncategorized)* Improve documentation across the project (#112) by @timonv in [#112](https://github.com/bosun-ai/swiftide/pull/112)

### Miscellaneous Tasks

- [206e432](https://github.com/bosun-ai/swiftide/commit/206e432dd291dd6a4592a6fb5f890049595311cb) *(ci)* Add support for merge queues by @timonv

- [e243212](https://github.com/bosun-ai/swiftide/commit/e2432123f0dfc48147ebed13fe6e3efec3ff7b3f) *(ci)* Enable continous benchmarking and improve benchmarks (#98) by @timonv in [#98](https://github.com/bosun-ai/swiftide/pull/98)

- [a8b02a3](https://github.com/bosun-ai/swiftide/commit/a8b02a3779bcce3671ea21d479fd4ceda30316c0) *(uncategorized)* Release v0.5.0 (#103) by @github-actions[bot] in [#103](https://github.com/bosun-ai/swiftide/pull/103)

- [162c6ef](https://github.com/bosun-ai/swiftide/commit/162c6ef2a07e40b8607b0ab6773909521f0bb798) *(uncategorized)* Ensure feat is always in Added by @timonv

- [5f09c11](https://github.com/bosun-ai/swiftide/commit/5f09c116f418cecb96fb1e86161333908d1a4d70) *(uncategorized)* Add initial benchmarks by @timonv

- [b953638](https://github.com/bosun-ai/swiftide/commit/b953638e613978cc3763b6b765e2c9ff21a1074d) *(uncategorized)* Configure Renovate (#94) by @renovate[bot] in [#94](https://github.com/bosun-ai/swiftide/pull/94)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.3...swiftide-v0.5.0


## [swiftide-v0.4.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.3) - 2024-06-28

### Bug Fixes

- [ab3dc86](https://github.com/bosun-ai/swiftide/commit/ab3dc861490a0d1ab94f96e741e09c860094ebc0) *(memory_storage)* Fallback to incremental counter when missing id by @timonv

### Documentation

- [4076092](https://github.com/bosun-ai/swiftide/commit/40760929d24e20631d0552d87bdbb4fdf9195453) *(readme)* Clean up and consistent badge styles by @timonv in [#93](https://github.com/bosun-ai/swiftide/pull/93)

- [dad3e02](https://github.com/bosun-ai/swiftide/commit/dad3e02fdc8a57e9de16832090c44c536e7e394b) *(readme)* Add ci badge by @timonv

### Miscellaneous Tasks

- [1ebbc2f](https://github.com/bosun-ai/swiftide/commit/1ebbc2fe01d6647abbc77850bc35ec93328891dd) *(uncategorized)* Manual release-plz update by @timonv

- [bdebc24](https://github.com/bosun-ai/swiftide/commit/bdebc241507e9f55998e96ca4aece530363716af) *(uncategorized)* Clippy by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.2...swiftide-v0.4.3


## [swiftide-v0.4.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.2) - 2024-06-26

### Features

- [926cc0c](https://github.com/bosun-ai/swiftide/commit/926cc0cca46023bcc3097a97b10ce03ae1fc3cc2) *(ingestion_stream)* Implement into for Result<Vec<IngestionNode>> by @timonv

### Bug Fixes

- [3143308](https://github.com/bosun-ai/swiftide/commit/3143308136ec4e71c8a5f9a127119e475329c1a2) *(embed)* Panic if number of embeddings and node are equal by @timonv in [#91](https://github.com/bosun-ai/swiftide/pull/91)

### Refactor

- [d285874](https://github.com/bosun-ai/swiftide/commit/d28587448d7fe342a79ac687cd5d7ee27354cae6) *(ingestion_pipeline)* Log_all combines other log helpers by @timonv

### Documentation

- [0660d5b](https://github.com/bosun-ai/swiftide/commit/0660d5b08aed15d62f077363eae80f621ddaa510) *(uncategorized)* Readme updates by @timonv in [#89](https://github.com/bosun-ai/swiftide/pull/89)

- [47aa378](https://github.com/bosun-ai/swiftide/commit/47aa378c4a70c47a2b313b6eca8dcf02b4723963) *(uncategorized)* Create CONTRIBUTING.md by @timonv in [#88](https://github.com/bosun-ai/swiftide/pull/88)

### Miscellaneous Tasks

- [c5a1540](https://github.com/bosun-ai/swiftide/commit/c5a15402f3292f45c1ad09b2f23c37fcb35c1da1) *(uncategorized)* Release by @github-actions[bot] in [#90](https://github.com/bosun-ai/swiftide/pull/90)

- [5ed08bb](https://github.com/bosun-ai/swiftide/commit/5ed08bb259b7544d3e4f2acdeef56231aa32e17c) *(uncategorized)* Cleanup changelog by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.1...swiftide-v0.4.2


## [swiftide-v0.4.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.1) - 2024-06-24

### Features

- [3898ee7](https://github.com/bosun-ai/swiftide/commit/3898ee7d6273ee7034848f9ab08fd85613cb5b32) *(memory_storage)* Can be cloned safely preserving storage by @timonv

- [92052bf](https://github.com/bosun-ai/swiftide/commit/92052bfdbca8951620f6d016768d252e793ecb5d) *(transformers)* Allow for arbitrary closures as transformers and batchable transformers by @timonv in [#84](https://github.com/bosun-ai/swiftide/pull/84)

### Miscellaneous Tasks

- [d1192e8](https://github.com/bosun-ai/swiftide/commit/d1192e80367f8dae17ec3761f6b4c7bc15ab56ef) *(uncategorized)* Release by @github-actions[bot] in [#85](https://github.com/bosun-ai/swiftide/pull/85)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.0...swiftide-v0.4.1


## [swiftide-v0.4.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.0) - 2024-06-23

### Features

- [1567940](https://github.com/bosun-ai/swiftide/commit/15679409032e9be347fbe8838a308ff0d09768b8) *(benchmarks)* Add benchmark for simple local pipeline by @timonv in [#82](https://github.com/bosun-ai/swiftide/pull/82)

- [477a284](https://github.com/bosun-ai/swiftide/commit/477a284597359472988ecde372e080f60aab0804) *(benchmarks)* Add benchmark for the file loader by @timonv

- [f4341ba](https://github.com/bosun-ai/swiftide/commit/f4341babe5807b268ce86a88e0df4bfc6d756de4) *(ci)* Single changelog for all (future) crates in root (#57) by @timonv in [#57](https://github.com/bosun-ai/swiftide/pull/57)

- [2228d84](https://github.com/bosun-ai/swiftide/commit/2228d84ccaad491e2c3cd0feb948050ad2872cf0) *(examples)* Example for markdown with all metadata by @timonv

- [9a1e12d](https://github.com/bosun-ai/swiftide/commit/9a1e12d34e02fe2292ce679251b96d61be74c884) *(examples,scraping)* Add example scraping and ingesting a url by @timonv

- [4d5c68e](https://github.com/bosun-ai/swiftide/commit/4d5c68e7bb09fae18832e2a453f114df5ba32ce1) *(ingestion_node)* Improved human readable Debug by @timonv

- [15deeb7](https://github.com/bosun-ai/swiftide/commit/15deeb72ca2e131e8554fa9cbefa3ef369de752a) *(ingestion_node)* Add constructor with defaults by @timonv

- [062107b](https://github.com/bosun-ai/swiftide/commit/062107b46474766640c38266f6fd6c27a95d4b57) *(ingestion_pipeline)* Implement throttling a pipeline (#77) by @timonv in [#77](https://github.com/bosun-ai/swiftide/pull/77)

- [a5051b7](https://github.com/bosun-ai/swiftide/commit/a5051b79b2ce62d41dd93f7b34a1a065d9878732) *(ingestion_pipeline)* Optional error filtering and logging (#75) by @timonv in [#75](https://github.com/bosun-ai/swiftide/pull/75)
Closes #73 and closes #64

Introduces various methods for stream inspection and skipping errors at
a desired stage in the stream.

- [a2ffc78](https://github.com/bosun-ai/swiftide/commit/a2ffc78f6d25769b9b7894f1f0703d51242023d4) *(ingestion_stream)* Improved stream developer experience (#81) by @timonv in [#81](https://github.com/bosun-ai/swiftide/pull/81)

````text
Improves stream ergonomics by providing convenient helpers and `Into`
  for streams, vectors and iterators that match the internal type.

  This means that in many cases, trait implementers can simply call
  `.into()` instead of manually constructing a stream. In the case it's an
  iterator, they can now use `IngestionStream::iter(<IntoIterator>)`
  instead.
````

- [9004323](https://github.com/bosun-ai/swiftide/commit/9004323dc5b11a3556a47e11fb8912ffc49f1e9e) *(integrations)* Implement Persist for Redis (#80) by @timonv in [#80](https://github.com/bosun-ai/swiftide/pull/80) [**breaking**]

- [d260674](https://github.com/bosun-ai/swiftide/commit/d2606745de8b22dcdf02e244d1b044efe12c6ac7) *(integrations)* Support fastembed (#60) by @timonv in [#60](https://github.com/bosun-ai/swiftide/pull/60) [**breaking**]

````text
Adds support for FastEmbed with various models. Includes a breaking change, renaming the Embed trait to EmbeddingModel.
````

- [eb84dd2](https://github.com/bosun-ai/swiftide/commit/eb84dd27c61a1b3a4a52a53cc0404203eac729e8) *(integrations,transformers)* Add transformer for converting html to markdown by @timonv

- [6d37051](https://github.com/bosun-ai/swiftide/commit/6d37051a9c2ef24ea7eb3815efcf9692df0d70ce) *(loaders)* Add scraping using `spider` by @timonv

- [ef7dcea](https://github.com/bosun-ai/swiftide/commit/ef7dcea45bfc336e7defcaac36bb5a6ff27d5acd) *(loaders)* File loader performance improvements by @timonv

- [2351867](https://github.com/bosun-ai/swiftide/commit/235186707182e8c39b8f22c6dd9d54eb32f7d1e5) *(persist)* In memory storage for testing, experimentation and debugging by @timonv

- [4d5d650](https://github.com/bosun-ai/swiftide/commit/4d5d650f235395aa81816637d559de39853e1db1) *(traits)* Add automock for simpleprompt by @timonv

- [bd6f887](https://github.com/bosun-ai/swiftide/commit/bd6f8876d010d23f651fd26a48d6775c17c98e94) *(transformers)* Add transformers for title, summary and keywords by @timonv

### Bug Fixes

- [7cbfc4e](https://github.com/bosun-ai/swiftide/commit/7cbfc4e13745ee5a6776a97fc6db06608fae8e81) *(ingestion_pipeline)* Concurrency does not work when spawned (#76) by @timonv in [#76](https://github.com/bosun-ai/swiftide/pull/76)

````text
Currency does did not work as expected. When spawning via `Tokio::spawn`
  the future would be polled directly, and any concurrency setting would
  not be respected. Because it had to be removed, improved tracing for
  each step as well.
````

### Documentation

- [53ed920](https://github.com/bosun-ai/swiftide/commit/53ed9206835da1172295e296119ee9a883605f18) *(uncategorized)* Hide the table of contents by @timonv

### Miscellaneous Tasks

- [7dde8a0](https://github.com/bosun-ai/swiftide/commit/7dde8a0811c7504b807b3ef9f508ce4be24967b8) *(ci)* Code coverage reporting (#58) by @timonv in [#58](https://github.com/bosun-ai/swiftide/pull/58)

````text
Post test coverage to Coveralls

  Also enabled --all-features when running tests in ci, just to be sure
````

- [cb7a2cd](https://github.com/bosun-ai/swiftide/commit/cb7a2cd3a72f306a0b46556caee0a25c7ba2c0e0) *(scraping)* Exclude spider from test coverage by @timonv in [#83](https://github.com/bosun-ai/swiftide/pull/83)

- [7767588](https://github.com/bosun-ai/swiftide/commit/77675884a2eeb0aab6ce57dccd2a260f5a973197) *(transformers)* Improve test coverage by @timonv in [#79](https://github.com/bosun-ai/swiftide/pull/79)

- [3bc43ab](https://github.com/bosun-ai/swiftide/commit/3bc43ab74e01341d978b04263612d4516633cb6c) *(uncategorized)* Release v0.4.0 (#78) by @github-actions[bot] in [#78](https://github.com/bosun-ai/swiftide/pull/78)

- [f6656be](https://github.com/bosun-ai/swiftide/commit/f6656becd199762843a59b0f86871753360a08f0) *(uncategorized)* Cargo update by @timonv

- [f251895](https://github.com/bosun-ai/swiftide/commit/f2518950427ef758fd57e6e6189ce600adf19940) *(uncategorized)* Documentation and feature flag cleanup (#69) by @timonv in [#69](https://github.com/bosun-ai/swiftide/pull/69)

````text
With fastembed added our dependencies become rather heavy. By default
  now disable all integrations and either provide 'all' or cherry pick
  integrations.
````

- [d6d0215](https://github.com/bosun-ai/swiftide/commit/d6d021560a05508add07a72f4f438d3ea3f1cb2c) *(uncategorized)* Properly quote crate name in changelog by @timonv

- [3b7c0db](https://github.com/bosun-ai/swiftide/commit/3b7c0dbc2f020ce84a5da5691ee6eb415df2d466) *(uncategorized)* Move changelog to root by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.3...swiftide-v0.4.0


## [swiftide-v0.3.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.3) - 2024-06-16

### Features

- [bdaed53](https://github.com/bosun-ai/swiftide/commit/bdaed5334b3e122f803370cc688dd2f662db0b8d) *(integrations)* Clone and debug for integrations by @timonv

- [fd63dff](https://github.com/bosun-ai/swiftide/commit/fd63dffb4f0b11bb9fa4fadc7b076463eca111a6) *(transformers)* Builder and clone for MetadataQAText by @timonv in [#55](https://github.com/bosun-ai/swiftide/pull/55)

- [e18e7fa](https://github.com/bosun-ai/swiftide/commit/e18e7fafae3007f1980bb617b7a72dd605720d74) *(transformers)* Builder and clone for MetadataQACode by @timonv

- [c074cc0](https://github.com/bosun-ai/swiftide/commit/c074cc0edb8b0314de15f9a096699e3e744c9f33) *(transformers)* Builder for chunk_markdown by @timonv

- [318e538](https://github.com/bosun-ai/swiftide/commit/318e538acb30ca516a780b5cc42c8ab2ed91cd6b) *(transformers)* Builder and clone for chunk_code by @timonv

### Miscellaneous Tasks

- [678106c](https://github.com/bosun-ai/swiftide/commit/678106c01b7791311a24425c22ea39366b664033) *(ci)* Pretty names for pipelines (#54) by @timonv in [#54](https://github.com/bosun-ai/swiftide/pull/54)

- [f5b674d](https://github.com/bosun-ai/swiftide/commit/f5b674de04c0d6e15a3f76bc5d3612ae345a9090) *(uncategorized)* Release v0.3.3 (#56) by @github-actions[bot] in [#56](https://github.com/bosun-ai/swiftide/pull/56)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.2...swiftide-v0.3.3


## [swiftide-v0.3.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.2) - 2024-06-16

### Features

- [b211002](https://github.com/bosun-ai/swiftide/commit/b211002e40ef16ef240e142c0178b04636a4f9aa) *(integrations)* Qdrant and openai builder should be consistent (#52) by @timonv in [#52](https://github.com/bosun-ai/swiftide/pull/52)

### Miscellaneous Tasks

- [ba6d71c](https://github.com/bosun-ai/swiftide/commit/ba6d71cc6930b74fee5e01380f4f8526914333e1) *(uncategorized)* Release v0.3.2 (#53) by @github-actions[bot] in [#53](https://github.com/bosun-ai/swiftide/pull/53)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.1...swiftide-v0.3.2


## [swiftide-v0.3.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.1) - 2024-06-15

### Documentation

- [7d79b64](https://github.com/bosun-ai/swiftide/commit/7d79b645d2e4f7da05b4c9952a1ceb79583572b3) *(uncategorized)* Fixing some grammar typos on README.md (#51) by @hectorip in [#51](https://github.com/bosun-ai/swiftide/pull/51)

- [6f63866](https://github.com/bosun-ai/swiftide/commit/6f6386693f3f6e0328eedaa4fb69cd8d0694574b) *(uncategorized)* We love feedback <3 by @timonv

### Miscellaneous Tasks

- [1f6a0f9](https://github.com/bosun-ai/swiftide/commit/1f6a0f961fb7855fbeb2493e9e70d2963c6ee018) *(uncategorized)* Release v0.3.1 (#50) by @github-actions[bot] in [#50](https://github.com/bosun-ai/swiftide/pull/50)

### New Contributors
* @hectorip made their first contribution in [#51](https://github.com/bosun-ai/swiftide/pull/51)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.0...swiftide-v0.3.1


## [swiftide-v0.3.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.0) - 2024-06-14

### Features

- [1f0cd28](https://github.com/bosun-ai/swiftide/commit/1f0cd28ce4c02a39dbab7dd3c3f789798644daa3) *(ingestion_pipeline)* Early return if any error encountered (#49) by @timonv in [#49](https://github.com/bosun-ai/swiftide/pull/49)

- [cd055f1](https://github.com/bosun-ai/swiftide/commit/cd055f19096daa802fe7fc34763bfdfd87c1ec41) *(ingestion_pipeline)* Concurrency improvements (#48) by @timonv in [#48](https://github.com/bosun-ai/swiftide/pull/48)

- [745b8ed](https://github.com/bosun-ai/swiftide/commit/745b8ed7e58f76e415501e6219ecec65551d1897) *(ingestion_pipeline)* Support chained storage backends (#46) by @timonv in [#46](https://github.com/bosun-ai/swiftide/pull/46) [**breaking**]

````text
Pipeline now supports multiple storage backends. This makes the order of adding storage important. Changed the name of the method to reflect that.
````

- [fa74939](https://github.com/bosun-ai/swiftide/commit/fa74939b30bd31301e3f80c407f153b5d96aa007) *(uncategorized)* Configurable concurrency for transformers and chunkers (#47) by @timonv in [#47](https://github.com/bosun-ai/swiftide/pull/47)

### Documentation

- [473e60e](https://github.com/bosun-ai/swiftide/commit/473e60ecf9356e2fcabe68245f8bb8be7373cdfb) *(uncategorized)* Update linkedin link by @timonv

### Miscellaneous Tasks

- [f51e668](https://github.com/bosun-ai/swiftide/commit/f51e668508b23ee0cb17790b77b94a3aad9daaa5) *(uncategorized)* Release v0.3.0 (#45) by @github-actions[bot] in [#45](https://github.com/bosun-ai/swiftide/pull/45)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.1...swiftide-v0.3.0


## [swiftide-v0.2.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.2.1) - 2024-06-13

### Documentation

- [e330ab9](https://github.com/bosun-ai/swiftide/commit/e330ab92d7e8d3f806280fa781f0e1b179d9b900) *(uncategorized)* Fix documentation link by @timonv

- [cb9b4fe](https://github.com/bosun-ai/swiftide/commit/cb9b4feec1c3654f5067f9478b1a7cf59040a9fe) *(uncategorized)* Add link to bosun by @timonv

### Miscellaneous Tasks

- [ac81e33](https://github.com/bosun-ai/swiftide/commit/ac81e33a494b58af435c324e69fe2158c9ab8f4b) *(uncategorized)* Release v0.2.1 (#44) by @github-actions[bot] in [#44](https://github.com/bosun-ai/swiftide/pull/44)


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.0...swiftide-v0.2.1


## [swiftide-v0.2.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.2.0) - 2024-06-13

### Features

- [9ec93be](https://github.com/bosun-ai/swiftide/commit/9ec93be110bd047c7e276714c48df236b1a235d7) *(uncategorized)* Api improvements with example (#10) by @timonv in [#10](https://github.com/bosun-ai/swiftide/pull/10)

### Bug Fixes

- [5b7ffd7](https://github.com/bosun-ai/swiftide/commit/5b7ffd7368a2688f70892fe37f28c0baea7ad54f) *(uncategorized)* Fmt by @timonv

- [42f8008](https://github.com/bosun-ai/swiftide/commit/42f80086042c659aef74ddd0ea1463c84650938d) *(uncategorized)* Clippy & fmt by @timonv

### Documentation

- [090ef1b](https://github.com/bosun-ai/swiftide/commit/090ef1b38684afca8dbcbfe31a8debc2328042e5) *(swiftide)* Documented file swiftide/src/integrations/openai/simple_prompt.rs (#19) by @bosun-ai[bot] in [#19](https://github.com/bosun-ai/swiftide/pull/19)

- [c932897](https://github.com/bosun-ai/swiftide/commit/c93289740806d9283ba488dd640dad5e4339e07d) *(swiftide)* Documented file swiftide/src/transformers/metadata_qa_code.rs (#34) by @bosun-ai[bot] in [#34](https://github.com/bosun-ai/swiftide/pull/34)

- [305a641](https://github.com/bosun-ai/swiftide/commit/305a64149f015539823d748915e42ad440a7b4b4) *(swiftide)* Documented file swiftide/src/transformers/openai_embed.rs (#35) by @bosun-ai[bot] in [#35](https://github.com/bosun-ai/swiftide/pull/35)

- [fb428d1](https://github.com/bosun-ai/swiftide/commit/fb428d1e250eded80d4edc8ccc0c9a9b840fc065) *(swiftide)* Documented file swiftide/src/transformers/metadata_qa_text.rs (#36) by @bosun-ai[bot] in [#36](https://github.com/bosun-ai/swiftide/pull/36)

- [ebd0a5d](https://github.com/bosun-ai/swiftide/commit/ebd0a5dda940c5ef8c2b795ee8ab56e468726869) *(swiftide)* Documented file swiftide/src/transformers/chunk_code.rs (#39) by @bosun-ai[bot] in [#39](https://github.com/bosun-ai/swiftide/pull/39)

- [289687e](https://github.com/bosun-ai/swiftide/commit/289687e1a6c0a9555a6cbecb24951522529f9e1a) *(swiftide)* Documented file swiftide/src/loaders/mod.rs (#40) by @bosun-ai[bot] in [#40](https://github.com/bosun-ai/swiftide/pull/40)

- [a78e68e](https://github.com/bosun-ai/swiftide/commit/a78e68e347dc3791957eeaf0f0adc050aeac1741) *(swiftide)* Documented file swiftide/tests/ingestion_pipeline.rs (#41) by @bosun-ai[bot] in [#41](https://github.com/bosun-ai/swiftide/pull/41)

- [502939f](https://github.com/bosun-ai/swiftide/commit/502939fcb5f56b7549b97bb99d4d121bf030835f) *(swiftide)* Documented file swiftide/src/integrations/treesitter/supported_languages.rs (#26) by @bosun-ai[bot] in [#26](https://github.com/bosun-ai/swiftide/pull/26)

- [14e24c3](https://github.com/bosun-ai/swiftide/commit/14e24c30d28dc6272a5eb8275e758a2a989d66be) *(swiftide)* Documented file swiftide/src/ingestion/mod.rs (#28) by @bosun-ai[bot] in [#28](https://github.com/bosun-ai/swiftide/pull/28)

- [d572c88](https://github.com/bosun-ai/swiftide/commit/d572c88f2b4cfc4bbdd7bd5ca93f7fd8460f1cb0) *(swiftide)* Documented file swiftide/src/integrations/qdrant/ingestion_node.rs (#20) by @bosun-ai[bot] in [#20](https://github.com/bosun-ai/swiftide/pull/20)

- [7688c99](https://github.com/bosun-ai/swiftide/commit/7688c993125a129204739fc7cd8d23d0ebfc9022) *(swiftide)* Documented file swiftide/src/integrations/qdrant/mod.rs (#22) by @bosun-ai[bot] in [#22](https://github.com/bosun-ai/swiftide/pull/22)

- [6240a26](https://github.com/bosun-ai/swiftide/commit/6240a260b582034970d2ee46da9f5234cf317820) *(swiftide)* Documented file swiftide/src/integrations/redis/mod.rs (#23) by @bosun-ai[bot] in [#23](https://github.com/bosun-ai/swiftide/pull/23)

- [7229af8](https://github.com/bosun-ai/swiftide/commit/7229af8535daa450ebafd6c45c322222a2dd12a0) *(swiftide)* Documented file swiftide/src/integrations/qdrant/persist.rs (#24) by @bosun-ai[bot] in [#24](https://github.com/bosun-ai/swiftide/pull/24)

- [29fce74](https://github.com/bosun-ai/swiftide/commit/29fce7437042f1f287987011825b57c58c180696) *(swiftide)* Documented file swiftide/src/integrations/redis/node_cache.rs (#29) by @bosun-ai[bot] in [#29](https://github.com/bosun-ai/swiftide/pull/29)

- [b319c0d](https://github.com/bosun-ai/swiftide/commit/b319c0d484db65d3a4594347e70770b8fac39e10) *(swiftide)* Documented file swiftide/src/integrations/treesitter/splitter.rs (#30) by @bosun-ai[bot] in [#30](https://github.com/bosun-ai/swiftide/pull/30)

- [2ea5a84](https://github.com/bosun-ai/swiftide/commit/2ea5a8445c8df7ef36e5fbc25f13c870e5a4dfd5) *(swiftide)* Documented file swiftide/src/integrations/openai/mod.rs (#21) by @bosun-ai[bot] in [#21](https://github.com/bosun-ai/swiftide/pull/21)

- [755cd47](https://github.com/bosun-ai/swiftide/commit/755cd47ad00e562818162cf78e6df0c5daa99d14) *(swiftide)* Documented file swiftide/src/ingestion/ingestion_node.rs (#15) by @bosun-ai[bot] in [#15](https://github.com/bosun-ai/swiftide/pull/15)

- [7abccc2](https://github.com/bosun-ai/swiftide/commit/7abccc2af890c8369a2b46940f35274080b3cb61) *(swiftide)* Documented file swiftide/src/ingestion/ingestion_stream.rs (#16) by @bosun-ai[bot] in [#16](https://github.com/bosun-ai/swiftide/pull/16)

- [95a6200](https://github.com/bosun-ai/swiftide/commit/95a62008be1869e581ecaa0586a48cfbb6a7606a) *(swiftide)* Documented file swiftide/src/ingestion/ingestion_pipeline.rs (#14) by @bosun-ai[bot] in [#14](https://github.com/bosun-ai/swiftide/pull/14)

- [a717f3d](https://github.com/bosun-ai/swiftide/commit/a717f3d5a68d9c79f9b8d85d8cb8979100dc3949) *(uncategorized)* Template links should be underscores by @timonv

- [7cfcc83](https://github.com/bosun-ai/swiftide/commit/7cfcc83eec29d8bed44172b497d4468b0b67d293) *(uncategorized)* Update readme template links and fix template by @timonv

### Miscellaneous Tasks

- [bb6cf2b](https://github.com/bosun-ai/swiftide/commit/bb6cf2ba164e4b61486bed650dddc0a590f63cd5) *(uncategorized)* Release v0.2.0 (#43) by @github-actions[bot] in [#43](https://github.com/bosun-ai/swiftide/pull/43)

- [ef88b89](https://github.com/bosun-ai/swiftide/commit/ef88b89f1e3cff1821603677892f5c20dcba9a51) *(uncategorized)* Release v0.1.0 (#8) by @github-actions[bot] in [#8](https://github.com/bosun-ai/swiftide/pull/8)

### New Contributors
* @bosun-ai[bot] made their first contribution in [#19](https://github.com/bosun-ai/swiftide/pull/19)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.1.0...swiftide-v0.2.0


## [v0.1.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.1.0) - 2024-06-13

### Features

- [0d9796b](https://github.com/bosun-ai/swiftide/commit/0d9796b2d3c54805dab7f8b8dbc979558a7062d6) *(ci)* Set up basic test and release actions (#1) by @timonv in [#1](https://github.com/bosun-ai/swiftide/pull/1)

- [2a6e503](https://github.com/bosun-ai/swiftide/commit/2a6e503e8abdab83ead7b8e62f39e222fa9f45d1) *(doc)* Setup basic readme (#5) by @timonv in [#5](https://github.com/bosun-ai/swiftide/pull/5)

- [b8f9166](https://github.com/bosun-ai/swiftide/commit/b8f9166e1d5419cf0d2cc6b6f0b2378241850574) *(fluyt)* Significant tracing improvements (#368) by @timonv

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

- [0986136](https://github.com/bosun-ai/swiftide/commit/098613622a7018318f2fffe0d51cd17822bf2313) *(fluyt/code_ops)* Add languages to chunker and range for chunk size (#334) by @timonv

````text
* feat(fluyt/code_ops): add more treesitter languages

  * fix: clippy + fmt

  * feat(fluyt/code_ops): implement builder and support range

  * feat(fluyt/code_ops): implement range limits for code chunking

  * feat(fluyt/indexing): code chunking supports size
````

- [f10bc30](https://github.com/bosun-ai/swiftide/commit/f10bc304b0b2e28281c90e57b6613c274dc20727) *(ingestion_pipeline)* Default concurrency is the number of cpus (#6) by @timonv in [#6](https://github.com/bosun-ai/swiftide/pull/6)

- [054b560](https://github.com/bosun-ai/swiftide/commit/054b560571b4a4398a551837536fb8fbff13c149) *(uncategorized)* Fix build and add feature flags for all integrations by @timonv

- [7453ddc](https://github.com/bosun-ai/swiftide/commit/7453ddc387feb17906ae851a17695f4c8232ee19) *(uncategorized)* Replace databuoy with new ingestion pipeline (#322) by @timonv

### Bug Fixes

- [fdf4be3](https://github.com/bosun-ai/swiftide/commit/fdf4be3d0967229a9dd84f568b0697fea4ddd341) *(fluyt)* Ensure minimal tracing by @timonv

- [458801c](https://github.com/bosun-ai/swiftide/commit/458801c16f9111c1070878c3a82a319701ae379c) *(uncategorized)* Properly connect to redis over tls by @timonv

- [bb905a3](https://github.com/bosun-ai/swiftide/commit/bb905a30d871ea3b238c3bc5cfd1d96724c8d4eb) *(uncategorized)* Use rustls on redis and log errors by @timonv

- [389b0f1](https://github.com/bosun-ai/swiftide/commit/389b0f12039f29703bc8bb71919b8067fadf5a8e) *(uncategorized)* Add debug info to qdrant setup by @timonv

### Refactor

- [0d342ea](https://github.com/bosun-ai/swiftide/commit/0d342eab747bc5f44adaa5b6131c30c09b1172a2) *(uncategorized)* Models as first class citizens (#318) by @timonv

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

### Miscellaneous Tasks

- [ce6e465](https://github.com/bosun-ai/swiftide/commit/ce6e465d4fb12e2bbc7547738b5fbe5133ec2d5a) *(fluyt)* Add verbose log on checking if index exists by @timonv

- [0ae98a7](https://github.com/bosun-ai/swiftide/commit/0ae98a772a751ddc60dd1d8e1606f9bdab4e04fd) *(uncategorized)* Cleanup Cargo keywords by @timonv

- [1036d56](https://github.com/bosun-ai/swiftide/commit/1036d565d8d9740ab55995095d495e582ce643d8) *(uncategorized)* Configure cargo toml (#7) by @timonv in [#7](https://github.com/bosun-ai/swiftide/pull/7)

- [4d79d27](https://github.com/bosun-ai/swiftide/commit/4d79d27709e3fed32c1b1f2c1f8dbeae1721d714) *(uncategorized)* Tests, tests, tests (#4) by @timonv in [#4](https://github.com/bosun-ai/swiftide/pull/4)

- [8e22e0e](https://github.com/bosun-ai/swiftide/commit/8e22e0ef82fffa4f907b0e2cccd1c4e010ffbd01) *(uncategorized)* Cleanup by @timonv

- [e717b7f](https://github.com/bosun-ai/swiftide/commit/e717b7f0b1311b11ed4690e7e11d9fdf53d4a81b) *(uncategorized)* Update issue templates by @timonv

- [44524fb](https://github.com/bosun-ai/swiftide/commit/44524fb51523291b9137fbdcaff9133a9a80c58a) *(uncategorized)* Restructure repository and rename (#3) by @timonv in [#3](https://github.com/bosun-ai/swiftide/pull/3)

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

- [730d879](https://github.com/bosun-ai/swiftide/commit/730d879e76c867c2097aef83bbbfa1211a053bdc) *(uncategorized)* Create LICENSE by @timonv

- [1f17d84](https://github.com/bosun-ai/swiftide/commit/1f17d84cc218602a480b27974f23f64c4269134f) *(uncategorized)* Cargo update by @timonv

- [951f496](https://github.com/bosun-ai/swiftide/commit/951f496498b35f7687fb556e5bf7f931a662ff8a) *(uncategorized)* Clean up more crates by @timonv

- [7ee8799](https://github.com/bosun-ai/swiftide/commit/7ee8799aeccc56fb0c14dbe68a7126cabfb40dd3) *(uncategorized)* Remove more crates and update by @timonv

- [cccdaf5](https://github.com/bosun-ai/swiftide/commit/cccdaf567744d58e0ee8ffcc8636f3b35090778f) *(uncategorized)* Remove more unused dependencies by @timonv

- [da004c6](https://github.com/bosun-ai/swiftide/commit/da004c6fcf82579c3c75414cb9f04f02530e2e31) *(uncategorized)* Start cleaning up dependencies by @timonv

- [f595f3d](https://github.com/bosun-ai/swiftide/commit/f595f3dae88bb4da5f4bbf6c5fe4f04abb4b7db3) *(uncategorized)* Add rust-toolchain on stable by @timonv

- [6967b0d](https://github.com/bosun-ai/swiftide/commit/6967b0d5b6221f7620161969865fb31959fc93b8) *(uncategorized)* Make indexing extraction compile by @tinco

### New Contributors
* @tinco made their first contribution


<!-- generated by git-cliff -->

