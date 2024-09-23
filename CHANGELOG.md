# Changelog

All notable changes to this project will be documented in this file.


## [unreleased]

### New features

- [da5df22] *(tree-sitter)* Implement Serialize and Deserialize for SupportedLanguages (#314) by @timonv

### Bug fixes

- [afce14e] *(ci)* Avoid protoc rate limit (#315) by @timonv

- [530dcd8] *(ci)* Fix concurrency conflict and trigger discord right after (#316) by @timonv

- [d433d99] *(ci)* Remove discord publish for now by @timonv

- [a756148] *(tree-sitter)* Fix javascript and improve tests (#313) by @timonv

````text
As learned from #309, test coverage for the refs defs transformer was
  not great. There _are_ more tests in code_tree. Turns out, with the
  latest treesitter update, javascript broke as it was the only language
  not covered at all.
````

- [9cc4535] Ignore lexicon-core warning for now and update deps (#310) by @timonv

### Docs

- [5d52288] *(readme)* Add blog links and update features (#312) by @timonv



## [v0.12.2](https://github.com/bosun-ai/swiftide/releases/tag/v0.12.2) - 2024-09-20

### Miscellaneous

- [80d4928] Release by @github-actions[bot]

### Docs

- [d84814e] Fix broken documentation links and other cargo doc warnings (#304) by @tinco

````text
Running `cargo doc --all-features` resulted in a lot of warnings.
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.12.1...v0.12.2


## [v0.12.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.12.1) - 2024-09-16

### New features

- [ec227d2] *(indexing,query)* Add concise info log with transformation name by @timonv

- [01cf579] *(query)* Add query_mut for reusable query pipelines by @timonv

- [081a248] *(query)* Improve query performance similar to indexing in 0.12 by @timonv

- [8029926] *(query,indexing)* Add duration in log output on pipeline completion by @timonv

### Bug fixes

- [d62b047] *(ci)* Update testcontainer images and fix tests by @timonv

- [39b6ecb] *(core)* Truncate long strings safely when printing debug logs by @timonv

- [8b8ceb9] *(deps)* Update redis by @timonv

- [16e9c74] *(openai)* Reduce debug verbosity by @timonv

- [6914d60] *(qdrant)* Reduce debug verbosity when storing nodes by @timonv

- [3d13889] *(query)* Reduce and improve debugging verbosity by @timonv

- [133cf1d] *(query)* Remove verbose debug and skip self in instrumentation by @timonv

- [ce17981] Clippy by @timonv

- [a871c61] Fmt by @timonv

### Miscellaneous

- [3c7c736] Release by @github-actions[bot]

### Docs

- [214ee8d] *(readme)* Add link to latest release post by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.12.0...v0.12.1


## [v0.12.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.12.0) - 2024-09-13

### New features

- [e902cb7] *(query)* Add support for filters in SimilaritySingleEmbedding (#298) by @timonv

````text
Adds support for filters for Qdrant and Lancedb in
  SimilaritySingleEmbedding. Also fixes several small bugs and brings
  improved tests.
````

- [f158960] Major performance improvements (#291) by @timonv

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

- [45d8a57] *(ci)* Use llm-cov preview via nightly and improve test coverage (#289) by @timonv

````text
Fix test coverage in CI. Simplified the trait bounds on the query
  pipeline for now to make it all work and fit together, and added more
  tests to assert boxed versions of trait objects work in tests.
````

- [501dd39] *(deps)* Update rust crate redis to 0.27 (#294) by @renovate[bot]

- [f8314cc] *(indexing)* Limit logged chunk to max 100 chars (#292) by @timonv

- [f95f806] *(indexing)* Debugging nodes should respect utf8 char boundaries by @timonv

- [8595553] Implement into_stream_boxed for all loaders by @timonv

- [9464ca1] Bad embed error propagation (#293) by @timonv

````text
- **fix(indexing): Limit logged chunk to max 100 chars**
  - **fix: Embed transformers must correctly propagate errors**
````

### Miscellaneous

- [c74f1e5] *(deps)* Update rust crate lancedb to 0.10 (#288) by @renovate[bot]

- [408f30a] *(deps)* Update testcontainers (#295) by @timonv

- [37c4bd9] *(deps)* Update treesitter (#296) by @timonv

- [8d9e954] Cargo update by @timonv

- [55c944d] Release v0.12.0 (#290) by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.11.1...v0.12.0


## [v0.11.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.11.1) - 2024-09-10

### New features

- [3c9491b] Implemtent traits T for Box<T> for indexing and query traits (#285) by @timonv

````text
When working with trait objects, some pipeline steps now allow for
  Box<dyn Trait> as well.
````

### Bug fixes

- [dfa546b] Add missing parquet feature flag by @timonv

### Miscellaneous

- [1887755] Release v0.11.1 (#284) by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.11.0...v0.11.1


## [v0.11.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.11.0) - 2024-09-08

### New features

- [bdf17ad] *(indexing)* Parquet loader (#279) by @timonv

````text
Ingest and index data from parquet files.
````

- [a98dbcb] *(integrations)* Add ollama embeddings support (#278) by @ephraimkunz

````text
Update to the most recent ollama-rs, which exposes the batch embedding
  API Ollama exposes (https://github.com/pepperoni21/ollama-rs/pull/61).
  This allows the Ollama struct in Swiftide to implement `EmbeddingModel`.

  Use the same pattern that the OpenAI struct uses to manage separate
  embedding and prompt models.

  ---------
````

### Bug fixes

- [873795b] *(ci)* Re-enable coverage via Coverals with tarpaulin (#280) by @timonv

### Miscellaneous

- [465de7f] Update CHANGELOG.md with breaking change by @timonv

- [a960ebf] Release v0.11.0 (#283) by @github-actions[bot]

### New Contributors
* @ephraimkunz made their first contribution in [#278](https://github.com/bosun-ai/swiftide/pull/278)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.10.0...v0.11.0


## [v0.10.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.10.0) - 2024-09-06

### New features

- [57fe4aa] *(indexing)* Use UUIDv3 for indexing node ids (#277) by @timonv [**breaking**]

````text
Use UUIDv3 to generate node ids for storage and cache. This is more
  reliable than the previous u64 hashing, with less chance for collision.
  Additionally, the previous hash algorithm changes over Rust releases and
  should not be used.
````
Closes #272 and needed for proper Rust 1.81 support as in #275
BREAKING CHANGE:All generated ids are now UUIDs, meaning all persisted
data needs to be purged or manually updated, as default upserts will
fail. There is no backwards compatibility.

### Bug fixes

- [5a724df] Rust 1.81 support (#275) by @timonv [**breaking**]

````text
Fixing id generation properly as per #272, will be merged in together.

  - **Clippy**
  - **fix(qdrant)!: Default hasher changed in Rust 1.81**
````

### Miscellaneous

- [807e902] Release v0.10.0 (#274) by @github-actions[bot] [**breaking**]
BREAKING CHANGE:Indexing nodes now have their ID calculated using UUIDv3 via MD5 as the previous algorithm was unreliable and broke in 1.81. Added benefit that collision chance is even smaller. This means that when indexing again, nodes will have different IDs and upsert will not work. Backwards compatibility is non-trivial. If this is a huge issue, ping us on discord and we will look into it.

### Added

-
[57fe4aa](https://github.com/bosun-ai/swiftide/commit/57fe4aa73b1b98dd8eac87c6440e0f2a0c66d4e8)
*(indexing)* Use UUIDv3 for indexing node ids
([#277](https://github.com/bosun-ai/swiftide/pull/277))

### Fixed

-
[5a724df](https://github.com/bosun-ai/swiftide/commit/5a724df895d35cfa606721d611afd073a23191de)
*(uncategorized)* Rust 1.81 support
([#275](https://github.com/bosun-ai/swiftide/pull/275))

### Other

-
[3711f6f](https://github.com/bosun-ai/swiftide/commit/3711f6fb2b51e97e4606b744cc963c04b44b6963)
*(readme)* Fix date
([#273](https://github.com/bosun-ai/swiftide/pull/273))


**Full Changelog**:
https://github.com/bosun-ai/swiftide/compare/0.9.2...0.10.0
</blockquote>


</p></details>

---
This PR was generated with
[release-plz](https://github.com/MarcoIeni/release-plz/).

### Docs

- [3711f6f] *(readme)* Fix date (#273) by @dzvon

````text
I suppose this should be 09-02.
````

### New Contributors
* @dzvon made their first contribution in [#273](https://github.com/bosun-ai/swiftide/pull/273)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.9.2...v0.10.0


## [v0.9.2](https://github.com/bosun-ai/swiftide/releases/tag/v0.9.2) - 2024-09-04

### New features

- [84e9bae] *(indexing)* Add chunker for text with text_splitter (#270) by @timonv

- [387fbf2] *(query)* Hybrid search for qdrant in query pipeline (#260) by @timonv

````text
Implement hybrid search for qdrant with their new Fusion search. Example
  in /examples includes an indexing and query pipeline, included the
  example answer as well.
````

### Bug fixes

- [baefe8e] *(ci)* Trigger discord update on released by @timonv

- [6e92b12] *(deps)* Update rust crate text-splitter to 0.16 (#267) by @renovate[bot]

### Miscellaneous

- [de35fa9] Release v0.9.2 (#266) by @github-actions[bot]

### Docs

- [064c7e1] *(readme)* Update intro by @timonv

- [1dc4c90] *(readme)* Add new blog links by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.9.1...v0.9.2


## [v0.9.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.9.1) - 2024-09-01

### New features

- [b891f93] *(integrations)* Add fluvio as loader support (#243) by @timonv

````text
Adds Fluvio as a loader support, enabling Swiftide indexing streams to
  process messages from a Fluvio topic.
````
Closes #240

- [c00b6c8] *(query)* Ragas support (#236) by @timonv

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
TODO:- [x] Test it with Ragas
- [x] Add more tests

- [a1250c1] LanceDB support (#254) by @timonv

````text
Add LanceDB support for indexing and querying. LanceDB separates compute
  from storage, where storage can be local or hosted elsewhere.
````
Closes #239

### Bug fixes

- [e15a0b2] *(ci)* Trigger discord release updates on release created by @timonv

````text
Published was not triggering.
````

- [a450e61] *(deps)* Update rust crate quote to v1.0.37 (#252) by @renovate[bot]

- [c071ada] *(deps)* Update rust crate reqwest to v0.12.7 (#251) by @renovate[bot]

- [fbc498c] *(deps)* Update rust crate syn to v2.0.76 (#249) by @renovate[bot]

- [f92376d] *(deps)* Update rust crate aws-sdk-bedrockruntime to v1.46.0 (#247) by @renovate[bot]

- [cc7ec08] *(deps)* Update rust crate spider to v2 (#237) by @renovate[bot]

- [d5a76ae] *(deps)* Update rust crate fastembed to v4 (#250) by @renovate[bot]

- [732a166] Remove no default features from futures-util by @timonv

### Miscellaneous

- [ba5f1de] *(deps)* Update rust crate tokio to v1.39.3 (#248) by @renovate[bot]
[#&#8203;6772]:https://togithub.com/tokio-rs/tokio/pull/6772

</details>

---

### Configuration

📅 **Schedule**: Branch creation - At any time (no schedule defined),
Automerge - At any time (no schedule defined).

🚦 **Automerge**: Disabled by config. Please merge this manually once you
are satisfied.

♻ **Rebasing**: Whenever PR becomes conflicted, or you tick the
rebase/retry checkbox.

🔕 **Ignore**: Close this PR and you won't be reminded about these
updates again.

---

- [ ] <!-- rebase-check -->If you want to rebase/retry this PR, check
this box

---

This PR was generated by [Mend Renovate](https://mend.io/renovate/).
View the [repository job
log](https://developer.mend.io/github/bosun-ai/swiftide).

<!--renovate-debug:eyJjcmVhdGVkSW5WZXIiOiIzOC4yNi4xIiwidXBkYXRlZEluVmVyIjoiMzguNTYuMCIsInRhcmdldEJyYW5jaCI6Im1hc3RlciIsImxhYmVscyI6W119-->

- [7f38aae] *(deps)* Update actions/checkout action to v4 (#246) by @renovate[bot]

- [2a179bc] *(deps)* Update rust crate async-openai to 0.24 (#255) by @renovate[bot]

- [9b257da] Default features cleanup (#262) by @timonv

````text
Integrations are messy and pull a lot in. A potential solution is to
  disable default features, only add what is actually required, and put
  the responsibility at users if they need anything specific. Feature
  unification should then take care of the rest.
````

- [2e0484d] Release v0.9.1 (#257) by @github-actions[bot]

### Docs

- [fb381b8] *(readme)* Copy improvements (#261) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.9.0...v0.9.1


## [v0.9.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.9.0) - 2024-08-15

### New features

- [49ad014] *(ci)* Publish releases to discord (#245) by @timonv

- [2443933] *(qdrant)* Add access to inner client for custom operations (#242) by @timonv

- [4fff613] *(query)* Add concurrency on query pipeline and add query_all by @timonv

### Bug fixes

- [4e31c0a] *(deps)* Update rust crate aws-sdk-bedrockruntime to v1.44.0 (#244) by @renovate[bot]

- [501321f] *(deps)* Update rust crate spider to v1.99.37 (#230) by @renovate[bot]

- [8a1cc69] *(query)* After retrieval current transormation should be empty by @timonv

### Miscellaneous

- [bc51cd0] *(deps)* Update rust crate serde_json to v1.0.125 (#238) by @renovate[bot]

- [05aa00c] *(deps)* Update rust crate qdrant-client to v1.11.1 (#231) by @renovate[bot]

- [e9d0016] *(indexing,integrations)* Move tree-sitter dependencies to integrations (#235) by @timonv

````text
Removes the dependency of indexing on integrations, resulting in much
  faster builds when developing on indexing.
````

- [0903310] Release (#229) by @github-actions[bot]

### Docs

- [3d213b4] *(readme)* Add link to 0.8 release by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.8.0...v0.9.0


## [v0.8.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.8.0) - 2024-08-12

### New features

- [2e25ad4] *(indexing)* Default LLM for indexing pipeline and boilerplate Transformer macro (#227) by @timonv [**breaking**]

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
BREAKING CHANGE:Introduces `WithIndexingDefaults` and
`WithBatchIndexingDefaults` trait constraints for transformers. They can
be used as a marker
with a noop (i.e. just `impl WithIndexingDefaults for MyTransformer
{}`). However, when implemented fully, they can be used to provide
defaults from the pipeline to your transformers.

- [67336f1] *(indexing)* Sparse vector support with Splade and Qdrant (#222) by @timonv

````text
Adds Sparse vector support to the indexing pipeline, enabling hybrid
  search for vector databases. The design should work for any form of
  Sparse embedding, and works with existing embedding modes and multiple
  named vectors. Additionally, added `try_default_sparse` to FastEmbed,
  using Splade, so it's fully usuable.

  Hybrid search in the query pipeline coming soon.
````

- [e728a7c] Code outlines in chunk metadata (#137) by @tinco

````text
Added a transformer that generates outlines for code files using tree sitter. And another that compresses the outline to be more relevant to chunks. Additionally added a step to the metadata QA tool that uses the outline to improve the contextual awareness during QA generation.
````

### Bug fixes

- [9613f50] *(ci)* Only show remote github url if present in changelog by @timonv

- [1ff2855] *(deps)* Update rust crate fastembed to v3.14.1 (#217) by @renovate[bot]

- [d6323b9] *(deps)* Update rust crate regex to v1.10.6 (#218) by @renovate[bot]

- [53f3f56] *(deps)* Update rust crate redis to v0.26.1 (#212) by @renovate[bot]

- [1a0097c] *(deps)* Update rust crate spider to v1.99.30 (#211) by @renovate[bot]
https://github.com/user-attachments/assets/e2b995e1-ef33-462e-9652-febdee56935a

**Full Changelog**:
https://github.com/spider-rs/spider/compare/v1.99.21...v1.99.30

###
[`v1.99.28`](https://togithub.com/spider-rs/spider/compare/v1.99.27...v1.99.28)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.27...v1.99.28)

###
[`v1.99.25`](https://togithub.com/spider-rs/spider/compare/v1.99.24...v1.99.25)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.24...v1.99.25)

###
[`v1.99.24`](https://togithub.com/spider-rs/spider/compare/v1.99.23...v1.99.24)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.23...v1.99.24)

###
[`v1.99.23`](https://togithub.com/spider-rs/spider/compare/v1.99.21...v1.99.23)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.21...v1.99.23)

###
[`v1.99.21`](https://togithub.com/spider-rs/spider/releases/tag/v1.99.21)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.20...v1.99.21)

### Whats Changed

You can now block ads over the network when using chrome and
chrome_intercept using the `adblock` feature flag.

**Full Changelog**:
https://github.com/spider-rs/spider/compare/v1.99.18...v1.99.21

###
[`v1.99.20`](https://togithub.com/spider-rs/spider/compare/v1.99.19...v1.99.20)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.19...v1.99.20)

###
[`v1.99.19`](https://togithub.com/spider-rs/spider/compare/v1.99.18...v1.99.19)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.18...v1.99.19)

###
[`v1.99.18`](https://togithub.com/spider-rs/spider/releases/tag/v1.99.18)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.17...v1.99.18)

### Whats Changed

1. chore(fs,chrome): fix chrome fs storing
\[[#&#8203;198](https://togithub.com/spider-rs/spider/issues/198)]

Thanks for the help [@&#8203;haijd](https://togithub.com/haijd)

**Full Changelog**:
https://github.com/spider-rs/spider/compare/v1.99.16...v1.99.18

###
[`v1.99.17`](https://togithub.com/spider-rs/spider/compare/v1.99.16...v1.99.17)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.16...v1.99.17)

###
[`v1.99.16`](https://togithub.com/spider-rs/spider/releases/tag/v1.99.16)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.99.15...v1.99.16)

#### What's Changed

- Fixing clap issues
[#&#8203;195](https://togithub.com/spider-rs/spider/issues/195) by
[@&#8203;jmikedupont2](https://togithub.com/jmikedupont2) in
[https://github.com/spider-rs/spider/pull/196](https://togithub.com/spider-rs/spider/pull/196)
-   Fix chrome fingerprint and initial document scripts setup
- Perf improvements for smart mode handling assets with compile time
constant map

#### New Contributors

- [@&#8203;jmikedupont2](https://togithub.com/jmikedupont2) made their
first contribution in
[https://github.com/spider-rs/spider/pull/196](https://togithub.com/spider-rs/spider/pull/196)

**Full Changelog**:
https://github.com/spider-rs/spider/compare/v1.99.10...v1.99.16

</details>

---

### Configuration

📅 **Schedule**: Branch creation - At any time (no schedule defined),
Automerge - At any time (no schedule defined).

🚦 **Automerge**: Disabled by config. Please merge this manually once you
are satisfied.

♻ **Rebasing**: Whenever PR becomes conflicted, or you tick the
rebase/retry checkbox.

🔕 **Ignore**: Close this PR and you won't be reminded about this update
again.

---

- [ ] <!-- rebase-check -->If you want to rebase/retry this PR, check
this box

---

This PR was generated by [Mend
Renovate](https://www.mend.io/free-developer-tools/renovate/). View the
[repository job
log](https://developer.mend.io/github/bosun-ai/swiftide).

<!--renovate-debug:eyJjcmVhdGVkSW5WZXIiOiIzNy40NDAuNyIsInVwZGF0ZWRJblZlciI6IjM4LjIwLjEiLCJ0YXJnZXRCcmFuY2giOiJtYXN0ZXIiLCJsYWJlbHMiOltdfQ==-->

- [dc7412b] *(deps)* Update aws-sdk-rust monorepo (#223) by @renovate[bot]

- [53be5d2] *(deps)* Update rust crate syn to v2.0.74 (#228) by @renovate[bot]

- [3cce606] *(deps)* Update rust crate text-splitter to 0.15 (#224) by @renovate[bot]

### Miscellaneous

- [acc1f58] *(ci)* Fix changelog for releases by @timonv

- [92b91f7] *(deps)* Update rust crate serde to v1.0.205 (#221) by @renovate[bot]

- [88c947f] *(deps)* Update embarkstudios/cargo-deny-action action to v2 (#216) by @renovate[bot]

- [ed07b27] *(deps)* Update rust crate serde_json to v1.0.124 (#226) by @renovate[bot]

- [b52b9f0] Release (#220) by @github-actions[bot]

### Docs

- [73d1649] *(readme)* Add Ollama support to README by @timonv

- [b3f04de] *(readme)* Add link to discord (#219) by @timonv

- [4970a68] *(readme)* Fix discord links by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.7.1...v0.8.0


## [v0.7.1](https://github.com/bosun-ai/swiftide/releases/tag/v0.7.1) - 2024-08-04

### New features

- [53e662b] *(ci)* Add cargo deny to lint dependencies (#213) by @timonv

- [b2d31e5] *(integrations)* Add ollama support (#214) by @tinco

- [9eb5894] *(query)* Add support for closures in all steps (#215) by @timonv

### Bug fixes

- [72b1ab1] *(deps)* Update rust crate fastembed to v3.14.0 (#209) by @renovate[bot]

### Miscellaneous

- [b16ece8] *(deps)* Update rust crate qdrant-client to v1.10.3 (#197) by @renovate[bot]

- [1c6c1cf] *(deps)* Update rust crate serde_json to v1.0.122 (#208) by @renovate[bot]

- [c0f3cfe] Release (#207) by @github-actions[bot]

### Docs

- [1539393] *(readme)* Update README.md by @timonv

- [ba07ab9] *(readme)* Readme improvements by @timonv

- [f7accde] *(readme)* Add 0.7 announcement by @timonv

- [084548f] *(readme)* Clarify on closures by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.7.0...v0.7.1


## [swiftide-v0.7.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.7.0) - 2024-07-28

### New features

- [ec1fb04] *(indexing)* Metadata as first class citizen (#204) by @timonv

````text
Adds our own implementation for metadata, internally still using a
  BTreeMap. The Value type is now a `serde_json::Value` enum. This allows
  us to store the metadata in the same format as the rest of the document,
  and also allows us to use values programmatically later.

  As is, all current meta data is still stored as Strings.
````
Closes #162

- [16bafe4] *(swiftide)* Rework workspace preparing for swiftide-query (#199) by @timonv [**breaking**]

````text
Splits up the project into multiple small, unpublished crates. Boosts
  compile times, makes the code a bit easier to grok and enables
  swiftide-query to be build separately.
````
BREAKING CHANGE:All indexing related tools are now in
`swiftide::indexing`

- [63694d2] *(swiftide-query)* Query pipeline v1 (#189) by @timonv

### Bug fixes

- [e72641b] *(ci)* Set versions in dependencies by @timonv

- [5d9738e] *(deps)* Update rust crate fastembed to v3.11.1 (#196) by @renovate[bot]

- [ee3aad3] *(deps)* Update rust crate aws-sdk-bedrockruntime to v1.42.0 (#195) by @renovate[bot]

- [be0f31d] *(deps)* Update rust crate spider to v1.99.11 (#190) by @renovate[bot]

- [ffc9681] *(deps)* Update rust crate redis to 0.26 (#203) by @renovate[bot]

- [dd04453] *(swiftide)* Update main lockfile by @timonv

- [bafd907] Update all cargo package descriptions by @timonv

### Miscellaneous

- [2938329] *(deps)* Update rust crate tokio to v1.39.2 (#198) by @renovate[bot]
[#&#8203;6722]:https://togithub.com/tokio-rs/tokio/pull/6722

###
[`v1.39.1`](https://togithub.com/tokio-rs/tokio/releases/tag/tokio-1.39.1):
Tokio v1.39.1

[Compare
Source](https://togithub.com/tokio-rs/tokio/compare/tokio-1.39.0...tokio-1.39.1)

##### 1.39.1 (July 23rd, 2024)

This release reverts "time: avoid traversing entries in the time wheel
twice" because it contains a bug. ([#&#8203;6715])
[#&#8203;6715]:https://togithub.com/tokio-rs/tokio/pull/6715

</details>

---

### Configuration

📅 **Schedule**: Branch creation - At any time (no schedule defined),
Automerge - At any time (no schedule defined).

🚦 **Automerge**: Disabled by config. Please merge this manually once you
are satisfied.

♻ **Rebasing**: Whenever PR becomes conflicted, or you tick the
rebase/retry checkbox.

🔕 **Ignore**: Close this PR and you won't be reminded about these
updates again.

---

- [ ] <!-- rebase-check -->If you want to rebase/retry this PR, check
this box

---

This PR was generated by [Mend
Renovate](https://www.mend.io/free-developer-tools/renovate/). View the
[repository job
log](https://developer.mend.io/github/bosun-ai/swiftide).

<!--renovate-debug:eyJjcmVhdGVkSW5WZXIiOiIzNy40MzguMCIsInVwZGF0ZWRJblZlciI6IjM3LjQzOC4wIiwidGFyZ2V0QnJhbmNoIjoibWFzdGVyIiwibGFiZWxzIjpbXX0=-->

- [15f4189] Release v0.7.0 (#200) by @github-actions[bot]

### Docs

- [2114aa4] *(readme)* Add copy on the query pipeline by @timonv

- [573aff6] *(indexing)* Document the default prompt templates and their context (#206) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.7...swiftide-v0.7.0


## [swiftide-v0.6.7](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.7) - 2024-07-23

### New features

- [beea449] *(prompt)* Add Into for strings to PromptTemplate (#193) by @timonv

- [f3091f7] *(transformers)* References and definitions from code (#186) by @timonv

### Bug fixes

- [a167184] *(deps)* Update rust crate tokio to v1.38.1 (#185) by @renovate[bot]
[#&#8203;6682]:https://togithub.com/tokio-rs/tokio/pull/6682
[#&#8203;6683]:https://togithub.com/tokio-rs/tokio/pull/6683

</details>

---

### Configuration

📅 **Schedule**: Branch creation - At any time (no schedule defined),
Automerge - At any time (no schedule defined).

🚦 **Automerge**: Disabled by config. Please merge this manually once you
are satisfied.

♻ **Rebasing**: Whenever PR becomes conflicted, or you tick the
rebase/retry checkbox.

🔕 **Ignore**: Close this PR and you won't be reminded about these
updates again.

---

- [ ] <!-- rebase-check -->If you want to rebase/retry this PR, check
this box

---

This PR has been generated by [Mend
Renovate](https://www.mend.io/free-developer-tools/renovate/). View
repository job log
[here](https://developer.mend.io/github/bosun-ai/swiftide).

<!--renovate-debug:eyJjcmVhdGVkSW5WZXIiOiIzNy40MzEuNCIsInVwZGF0ZWRJblZlciI6IjM3LjQzMS40IiwidGFyZ2V0QnJhbmNoIjoibWFzdGVyIiwibGFiZWxzIjpbXX0=-->

### Miscellaneous

- [942b50c] *(deps)* Update rust crate wiremock to v0.6.1 (#192) by @renovate[bot]

- [4cd56f3] *(deps)* Update rust crate mockall to 0.13.0 (#191) by @renovate[bot]

- [65cceff] *(deps)* Update rust crate testcontainers to v0.20.1 (#188) by @renovate[bot]

- [a44c303] Release v0.6.7 (#187) by @github-actions[bot]

### Docs

- [97a572e] *(readme)* Add blog posts and update doc link (#194) by @timonv

- [504fe26] *(pipeline)* Add note that closures can also be used as transformers by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.6...swiftide-v0.6.7


## [swiftide-v0.6.6](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.6) - 2024-07-16

### New features

- [d1c642a] *(groq)* Add SimplePrompt support for Groq (#183) by @timonv

````text
Adds simple prompt support for Groq by using async_openai. ~~Needs some
  double checks~~. Works great.
````

### Bug fixes

- [52e27f8] *(deps)* Update rust crate fastembed to v3.9.1 (#171) by @renovate[bot]

- [312c213] *(deps)* Update rust crate spider to v1.99.5 (#170) by @renovate[bot]

- [5d4a814] *(deps)* Update rust crate aws-sdk-bedrockruntime to v1.40.0 (#169) by @renovate[bot]

### Miscellaneous

- [abf63f1] Release v0.6.6 (#182) by @github-actions[bot]

### Docs

- [143c7c9] *(readme)* Fix typo (#180) by @eltociear

- [d393181] *(docsrs)* Scrape examples and fix links (#184) by @timonv

### New Contributors
* @eltociear made their first contribution in [#180](https://github.com/bosun-ai/swiftide/pull/180)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.5...swiftide-v0.6.6


## [swiftide-v0.6.5](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.5) - 2024-07-15

### New features

- [0065c7a] *(prompt)* Add extending the prompt repository (#178) by @timonv

### Bug fixes

- [b54691f] *(prompts)* Include default prompts in crate (#174) by @timonv

````text
- **add prompts to crate**
  - **load prompts via cargo manifest dir**
````

- [3c297bb] *(swiftide)* Remove include from Cargo.toml by @timonv

### Miscellaneous

- [73d5fa3] *(traits)* Cleanup unused batch size in `BatchableTransformer` (#177) by @timonv

- [c62b605] Release v0.6.4 (#175) by @github-actions[bot]

- [c6343e5] Release v0.6.5 (#181) by @timonv

````text
## 🤖 New release
  * `swiftide`: 0.6.4 -> 0.6.5

  <details><summary><i><b>Changelog</b></i></summary><p>

  <blockquote>

  ## [0.6.5](https://github.com/bosun-ai/swiftide/releases/tag/0.6.5) -
  2024-07-15

  ### Features

  -
  [0065c7a](https://github.com/bosun-ai/swiftide/commit/0065c7a7fd1289ea227391dd7b9bd51c905290d5)
  *(prompt)* Add extending the prompt repository
  ([#178](https://github.com/bosun-ai/swiftide/pull/178))

  ### Documentation

  -
  [b95b395](https://github.com/bosun-ai/swiftide/commit/b95b3955f89ed231cc156dab749ee7bb8be98ee5)
  *(swiftide)* Documentation improvements and cleanup
  ([#176](https://github.com/bosun-ai/swiftide/pull/176))

  ### Miscellaneous Tasks

  -
  [73d5fa3](https://github.com/bosun-ai/swiftide/commit/73d5fa37d23f53919769c2ffe45db2e3832270ef)
  *(traits)* Cleanup unused batch size in `BatchableTransformer`
  ([#177](https://github.com/bosun-ai/swiftide/pull/177))


  **Full Changelog**:
  https://github.com/bosun-ai/swiftide/compare/0.6.4...0.7.0


  <!-- generated by git-cliff -->
  </blockquote>


  </p></details>

  ---
  This PR was generated with
  [release-plz](https://github.com/MarcoIeni/release-plz/).
````

### Docs

- [b95b395] *(swiftide)* Documentation improvements and cleanup (#176) by @timonv

````text
- **chore: remove ingestion stream**
  - **Documentation and grammar**
````


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.3...swiftide-v0.6.5


## [swiftide-v0.6.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.3) - 2024-07-14

### Bug fixes

- [47418b5] *(prompts)* Fix breaking issue with prompts not found by @timonv
Closes #172

### Miscellaneous

- [dbca9c8] Release v0.6.3 (#173) by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.2...swiftide-v0.6.3


## [swiftide-v0.6.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.2) - 2024-07-12

### Miscellaneous

- [2b682b2] *(deps)* Limit feature flags on qdrant to fix docsrs by @timonv

- [69621f6] Release v0.6.2 (#168) by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.1...swiftide-v0.6.2


## [swiftide-v0.6.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.1) - 2024-07-12

### Miscellaneous

- [aae7ab1] *(deps)* Patch update all by @timonv

- [b490d3f] Release v0.6.1 (#167) by @github-actions[bot]

### Docs

- [085709f] *(docsrs)* Disable unstable and rustdoc scraping by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.6.0...swiftide-v0.6.1


## [swiftide-v0.6.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.6.0) - 2024-07-12

### New features

- [70ea268] *(prompts)* Add prompts as first class citizens (#145) by @timonv

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

- [699cfe4] Embed modes and named vectors (#123) by @pwalski

````text
Added named vector support to qdrant. A pipeline can now have its embed
  mode configured, either per field, chunk and metadata combined (default)
  or both. Vectors need to be configured on the qdrant client side.

  See `examples/store_multiple_vectors.rs` for an example.

  Shoutout to @pwalski for the contribution. Closes #62.

  ---------
````

### Bug fixes

- [9334934] *(chunkcode)* Use correct chunksizes (#122) by @timonv

- [eb8364e] *(ci)* Try overriding the github repo for git cliff by @timonv

- [5de6af4] *(ci)* Only add contributors if present by @timonv

- [4c9ed77] *(ci)* Properly check if contributors are present by @timonv

- [c5bf796] *(ci)* Add clippy back to ci (#147) by @timonv

- [7357fea] *(deps)* Update rust crate spider to v1.98.6 (#119) by @renovate[bot]

- [3b98334] *(deps)* Update rust crate serde_json to v1.0.120 (#115) by @renovate[bot]

- [dfc76dd] *(deps)* Update rust crate serde to v1.0.204 (#129) by @renovate[bot]

- [28f5b04] *(deps)* Update rust crate tree-sitter-typescript to v0.21.2 (#128) by @renovate[bot]

- [9c261b8] *(deps)* Update rust crate text-splitter to v0.14.1 (#127) by @renovate[bot]

- [ff92abd] *(deps)* Update rust crate tree-sitter-javascript to v0.21.4 (#126) by @renovate[bot]

- [7af97b5] *(deps)* Update rust crate spider to v1.98.7 (#124) by @renovate[bot]

- [adc4bf7] *(deps)* Update aws-sdk-rust monorepo (#125) by @renovate[bot]

- [dd32ef3] *(deps)* Update rust crate async-trait to v0.1.81 (#134) by @renovate[bot]

- [2b13523] *(deps)* Update rust crate fastembed to v3.7.1 (#135) by @renovate[bot]

- [bf3b677] *(deps)* Update rust crate fastembed to v3.9.0 (#141) by @renovate[bot]

- [8e22937] *(deps)* Update rust crate aws-sdk-bedrockruntime to v1.39.0 (#143) by @renovate[bot]

- [a691d61] *(deps)* Update rust crate async-openai to v0.23.4 (#136) by @renovate[bot]

- [6d75f14] *(deps)* Update rust crate htmd to v0.1.6 (#144) by @renovate[bot]

- [c3aee48] *(deps)* Update rust crate spider to v1.98.9 (#146) by @renovate[bot]

- [353cd9e] *(qdrant)* Upgrade and better defaults (#118) by @timonv

````text
- **fix(deps): update rust crate qdrant-client to v1.10.1**
  - **fix(qdrant): upgrade to new qdrant with sensible defaults**
  - **feat(qdrant): safe to clone with internal arc**

  ---------
````

- [b53636c] Inability to store only some of `EmbeddedField`s (#139) by @pwalski
Fixes:#138

---------

### Performance

- [ea8f823] Improve local build performance and crate cleanup (#148) by @timonv

````text
- **tune cargo for faster builds**
  - **perf(swiftide): increase local build performance**
````

### Miscellaneous

- [7a8843a] *(deps)* Update rust crate testcontainers to 0.20.0 (#133) by @renovate[bot]

- [364e13d] *(swiftide)* Loosen up dependencies (#140) by @timonv

````text
Loosen up dependencies so swiftide is a bit more flexible to add to
  existing projects
````

- [84dd65d] Rename all mentions of ingest to index (#130) by @timonv [**breaking**]

````text
Swiftide is not an ingestion pipeline (loading data), but an indexing
  pipeline (prepping for search).

  There is now a temporary, deprecated re-export to match the previous api.
````

- [51c114c] Various tooling & community improvements (#131) by @timonv

````text
- **fix(ci): ensure clippy runs with all features**
  - **chore(ci): coverage using llvm-cov**
  - **chore: drastically improve changelog generation**
  - **chore(ci): add sanity checks for pull requests**
  - **chore(ci): split jobs and add typos**
````

- [d2a9ea1] Enable clippy pedantic (#132) by @timonv

- [4078264] Release v0.6.0 (#166) by @github-actions[bot]

### Docs

- [8405c9e] *(contributing)* Add guidelines on code design (#113) by @timonv

- [3e447fe] *(readme)* Link to CONTRIBUTING (#114) by @timonv

- [4c40e27] *(readme)* Add back coverage badge by @timonv

- [5691ac9] *(readme)* Add preproduction warning by @timonv

- [37af322] *(rustdocs)* Rewrite the initial landing page (#149) by @timonv

````text
- **Add homepage and badges to cargo toml**
  - **documentation landing page improvements**
````

- [7686c2d] Templated prompts are now a major feature by @timonv

### New Contributors
* @pwalski made their first contribution in [#139](https://github.com/bosun-ai/swiftide/pull/139)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.5.0...swiftide-v0.6.0


## [swiftide-v0.5.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.5.0) - 2024-07-01

### New features

- [6a88651] *(ingestion_pipeline)* Implement filter (#109) by @timonv

- [5aeb3a7] *(ingestion_pipeline)* Splitting and merging streams by @timonv

- [8812fbf] *(ingestion_pipeline)* Build a pipeline from a stream by @timonv

- [6101bed] AWS bedrock support (#92) by @timonv

````text
Adds an integration with AWS Bedrock, implementing SimplePrompt for
  Anthropic and Titan models. More can be added if there is a need. Same
  for the embedding models.
````

### Bug fixes

- [17a2be1] *(changelog)* Add scope by @timonv

- [3cc2e06] *(ci)* Fix release-plz changelog parsing by @timonv

- [2dbf14c] *(ci)* Fix benchmarks in ci by @timonv

- [b155de6] *(ci)* Fix naming of github actions by @timonv

- [46752db] *(ci)* Add concurrency configuration by @timonv

- [9b4ef81] *(deps)* Update rust crate spider to v1.98.3 (#100) by @renovate[bot]
#[tokio::main]
async fn main() {
    let mut website: Website = Website::new("https://rsseau.fr/en/");

    website.with_whitelist_url(Some(vec!["/books".into()]));

    let mut rx2: tokio::sync::broadcast::Receiver<spider::page::Page> =
        website.subscribe(0).unwrap();
    let mut stdout = tokio::io::stdout();

    let join_handle = tokio::spawn(async move {
        while let Ok(res) = rx2.recv().await {
            let _ = stdout
                .write_all(format!("- {}\n", res.get_url()).as_bytes())
                .await;
        }
        stdout
    });

    let start = std::time::Instant::now();
    website.crawl().await;
    website.unsubscribe();
    let duration = start.elapsed();
    let mut stdout = join_handle.await.unwrap();

    let _ = stdout
        .write_all(
            format!(
                "Time elapsed in website.crawl() is: {:?} for total pages: {:?}",
                duration,
                website.get_links().len()
            )
            .as_bytes(),
        )
        .await;
}
```

**Full Changelog**:
https://github.com/spider-rs/spider/compare/v1.97.14...v1.98.3

###
[`v1.98.2`](https://togithub.com/spider-rs/spider/compare/v1.98.1...v1.98.2)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.98.1...v1.98.2)

###
[`v1.98.1`](https://togithub.com/spider-rs/spider/compare/v1.98.0...v1.98.1)

[Compare
Source](https://togithub.com/spider-rs/spider/compare/v1.98.0...v1.98.1)

</details>

---

### Configuration

📅 **Schedule**: Branch creation - At any time (no schedule defined),
Automerge - At any time (no schedule defined).

🚦 **Automerge**: Disabled by config. Please merge this manually once you
are satisfied.

♻ **Rebasing**: Whenever PR becomes conflicted, or you tick the
rebase/retry checkbox.

🔕 **Ignore**: Close this PR and you won't be reminded about this update
again.

---

- [ ] <!-- rebase-check -->If you want to rebase/retry this PR, check
this box

---

This PR has been generated by [Mend
Renovate](https://www.mend.io/free-developer-tools/renovate/). View
repository job log
[here](https://developer.mend.io/github/bosun-ai/swiftide).

<!--renovate-debug:eyJjcmVhdGVkSW5WZXIiOiIzNy40MjAuMSIsInVwZGF0ZWRJblZlciI6IjM3LjQyMC4xIiwidGFyZ2V0QnJhbmNoIjoibWFzdGVyIiwibGFiZWxzIjpbXX0=-->

- [8e15004] *(deps)* Update rust crate serde_json to v1.0.118 (#99) by @renovate[bot]

- [4c019eb] *(deps)* Update rust crate htmd to v0.1.5 (#96) by @renovate[bot]

- [2401414] *(deps)* Update rust crate text-splitter to 0.14.0 (#105) by @renovate[bot]

- [52cf37b] *(deps)* Update rust crate fastembed to v3.7.0 (#104) by @renovate[bot]

- [5c16c8e] *(deps)* Update rust crate strum to v0.26.3 (#101) by @renovate[bot]

- [2650605] *(deps)* Update rust crate serde_json to v1.0.119 (#110) by @renovate[bot]

- [a12cce2] *(openai)* Add tests for builder by @timonv

- [963919b] *(transformers)* Fix too small chunks being retained and api by @timonv [**breaking**]

- [5e8da00] Fix oversight in ingestion pipeline tests by @timonv

- [e8198d8] Use git cliff manually for changelog generation by @timonv

- [2c31513] Just use keepachangelog by @timonv

- [6430af7] Use native cargo bench format and only run benchmarks crate by @timonv

- [cba981a] Replace unwrap with expect and add comment on panic by @timonv

### Miscellaneous

- [e243212] *(ci)* Enable continous benchmarking and improve benchmarks (#98) by @timonv

- [206e432] *(ci)* Add support for merge queues by @timonv

- [8a2541e] *(deps)* Update qdrant/qdrant docker tag to v1.9.7 (#95) by @renovate[bot]

- [5c33624] *(deps)* Update rust crate testcontainers to 0.19.0 (#102) by @renovate[bot]

- [b953638] Configure Renovate (#94) by @renovate[bot]

- [5f09c11] Add initial benchmarks by @timonv

- [162c6ef] Ensure feat is always in Added by @timonv

- [a8b02a3] Release v0.5.0 (#103) by @github-actions[bot]
[0.5.0]:https://github.com///compare/0.1.0..0.5.0

<!-- generated by git-cliff -->
</blockquote>


</p></details>

---
This PR was generated with
[release-plz](https://github.com/MarcoIeni/release-plz/).

### Docs

- [929410c] *(readme)* Add diagram to the readme (#107) by @timonv

- [b014f43] Improve documentation across the project (#112) by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.3...swiftide-v0.5.0


## [swiftide-v0.4.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.3) - 2024-06-28

### Bug fixes

- [ab3dc86] *(memory_storage)* Fallback to incremental counter when missing id by @timonv

### Miscellaneous

- [bdebc24] Clippy by @timonv

- [1ebbc2f] Manual release-plz update by @timonv

### Docs

- [dad3e02] *(readme)* Add ci badge by @timonv

- [4076092] *(readme)* Clean up and consistent badge styles by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.2...swiftide-v0.4.3


## [swiftide-v0.4.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.2) - 2024-06-26

### New features

- [926cc0c] *(ingestion_stream)* Implement into for Result<Vec<IngestionNode>> by @timonv

### Bug fixes

- [3143308] *(embed)* Panic if number of embeddings and node are equal by @timonv

### Miscellaneous

- [5ed08bb] Cleanup changelog by @timonv

- [c5a1540] Release by @github-actions[bot]

### Docs

- [47aa378] Create CONTRIBUTING.md by @timonv

- [0660d5b] Readme updates by @timonv

### Refactor

- [d285874] *(ingestion_pipeline)* Log_all combines other log helpers by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.1...swiftide-v0.4.2


## [swiftide-v0.4.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.1) - 2024-06-24

### New features

- [3898ee7] *(memory_storage)* Can be cloned safely preserving storage by @timonv

- [92052bf] *(transformers)* Allow for arbitrary closures as transformers and batchable transformers by @timonv

### Miscellaneous

- [d1192e8] Release by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.4.0...swiftide-v0.4.1


## [swiftide-v0.4.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.4.0) - 2024-06-23

### New features

- [477a284] *(benchmarks)* Add benchmark for the file loader by @timonv

- [1567940] *(benchmarks)* Add benchmark for simple local pipeline by @timonv

- [f4341ba] *(ci)* Single changelog for all (future) crates in root (#57) by @timonv

- [2228d84] *(examples)* Example for markdown with all metadata by @timonv

- [9a1e12d] *(examples,scraping)* Add example scraping and ingesting a url by @timonv

- [15deeb7] *(ingestion_node)* Add constructor with defaults by @timonv

- [4d5c68e] *(ingestion_node)* Improved human readable Debug by @timonv

- [a5051b7] *(ingestion_pipeline)* Optional error filtering and logging (#75) by @timonv
Closes #73 and closes #64

Introduces various methods for stream inspection and skipping errors at
a desired stage in the stream.

- [062107b] *(ingestion_pipeline)* Implement throttling a pipeline (#77) by @timonv

- [a2ffc78] *(ingestion_stream)* Improved stream developer experience (#81) by @timonv

````text
Improves stream ergonomics by providing convenient helpers and `Into`
  for streams, vectors and iterators that match the internal type.

  This means that in many cases, trait implementers can simply call
  `.into()` instead of manually constructing a stream. In the case it's an
  iterator, they can now use `IngestionStream::iter(<IntoIterator>)`
  instead.
````

- [d260674] *(integrations)* Support fastembed (#60) by @timonv [**breaking**]

````text
Adds support for FastEmbed with various models. Includes a breaking change, renaming the Embed trait to EmbeddingModel.
````

- [9004323] *(integrations)* Implement Persist for Redis (#80) by @timonv [**breaking**]

- [eb84dd2] *(integrations,transformers)* Add transformer for converting html to markdown by @timonv

- [ef7dcea] *(loaders)* File loader performance improvements by @timonv

- [6d37051] *(loaders)* Add scraping using `spider` by @timonv

- [2351867] *(persist)* In memory storage for testing, experimentation and debugging by @timonv

- [4d5d650] *(traits)* Add automock for simpleprompt by @timonv

- [bd6f887] *(transformers)* Add transformers for title, summary and keywords by @timonv

### Bug fixes

- [7cbfc4e] *(ingestion_pipeline)* Concurrency does not work when spawned (#76) by @timonv

````text
Currency does did not work as expected. When spawning via `Tokio::spawn`
  the future would be polled directly, and any concurrency setting would
  not be respected. Because it had to be removed, improved tracing for
  each step as well.
````

### Miscellaneous

- [7dde8a0] *(ci)* Code coverage reporting (#58) by @timonv

````text
Post test coverage to Coveralls

  Also enabled --all-features when running tests in ci, just to be sure
````

- [cb7a2cd] *(scraping)* Exclude spider from test coverage by @timonv

- [7767588] *(transformers)* Improve test coverage by @timonv

- [3b7c0db] Move changelog to root by @timonv

- [d6d0215] Properly quote crate name in changelog by @timonv

- [f251895] Documentation and feature flag cleanup (#69) by @timonv

````text
With fastembed added our dependencies become rather heavy. By default
  now disable all integrations and either provide 'all' or cherry pick
  integrations.
````

- [f6656be] Cargo update by @timonv

- [3bc43ab] Release v0.4.0 (#78) by @github-actions[bot]

### Docs

- [53ed920] Hide the table of contents by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.3...swiftide-v0.4.0


## [swiftide-v0.3.3](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.3) - 2024-06-16

### New features

- [bdaed53] *(integrations)* Clone and debug for integrations by @timonv

- [318e538] *(transformers)* Builder and clone for chunk_code by @timonv

- [c074cc0] *(transformers)* Builder for chunk_markdown by @timonv

- [e18e7fa] *(transformers)* Builder and clone for MetadataQACode by @timonv

- [fd63dff] *(transformers)* Builder and clone for MetadataQAText by @timonv

### Miscellaneous

- [678106c] *(ci)* Pretty names for pipelines (#54) by @timonv

- [f5b674d] Release v0.3.3 (#56) by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.2...swiftide-v0.3.3


## [swiftide-v0.3.2](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.2) - 2024-06-16

### New features

- [b211002] *(integrations)* Qdrant and openai builder should be consistent (#52) by @timonv

### Miscellaneous

- [ba6d71c] Release v0.3.2 (#53) by @github-actions[bot]


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.1...swiftide-v0.3.2


## [swiftide-v0.3.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.1) - 2024-06-15

### Miscellaneous

- [1f6a0f9] Release v0.3.1 (#50) by @github-actions[bot]

### Docs

- [6f63866] We love feedback <3 by @timonv

- [7d79b64] Fixing some grammar typos on README.md (#51) by @hectorip

### New Contributors
* @hectorip made their first contribution in [#51](https://github.com/bosun-ai/swiftide/pull/51)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.3.0...swiftide-v0.3.1


## [swiftide-v0.3.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.3.0) - 2024-06-14

### New features

- [745b8ed] *(ingestion_pipeline)* Support chained storage backends (#46) by @timonv [**breaking**]

````text
Pipeline now supports multiple storage backends. This makes the order of adding storage important. Changed the name of the method to reflect that.
````

- [cd055f1] *(ingestion_pipeline)* Concurrency improvements (#48) by @timonv

- [1f0cd28] *(ingestion_pipeline)* Early return if any error encountered (#49) by @timonv

- [fa74939] Configurable concurrency for transformers and chunkers (#47) by @timonv

### Miscellaneous

- [f51e668] Release v0.3.0 (#45) by @github-actions[bot]

### Docs

- [473e60e] Update linkedin link by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.1...swiftide-v0.3.0


## [swiftide-v0.2.1](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.2.1) - 2024-06-13

### Miscellaneous

- [ac81e33] Release v0.2.1 (#44) by @github-actions[bot]

### Docs

- [cb9b4fe] Add link to bosun by @timonv

- [e330ab9] Fix documentation link by @timonv


**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/swiftide-v0.2.0...swiftide-v0.2.1


## [swiftide-v0.2.0](https://github.com/bosun-ai/swiftide/releases/tag/swiftide-v0.2.0) - 2024-06-13

### New features

- [9ec93be] Api improvements with example (#10) by @timonv

### Bug fixes

- [42f8008] Clippy & fmt by @timonv

- [5b7ffd7] Fmt by @timonv

### Miscellaneous

- [ef88b89] Release v0.1.0 (#8) by @github-actions[bot]

- [bb6cf2b] Release v0.2.0 (#43) by @github-actions[bot]

### Docs

- [95a6200] *(swiftide)* Documented file swiftide/src/ingestion/ingestion_pipeline.rs (#14) by @bosun-ai[bot]

- [7abccc2] *(swiftide)* Documented file swiftide/src/ingestion/ingestion_stream.rs (#16) by @bosun-ai[bot]

- [755cd47] *(swiftide)* Documented file swiftide/src/ingestion/ingestion_node.rs (#15) by @bosun-ai[bot]

- [2ea5a84] *(swiftide)* Documented file swiftide/src/integrations/openai/mod.rs (#21) by @bosun-ai[bot]

- [b319c0d] *(swiftide)* Documented file swiftide/src/integrations/treesitter/splitter.rs (#30) by @bosun-ai[bot]

- [29fce74] *(swiftide)* Documented file swiftide/src/integrations/redis/node_cache.rs (#29) by @bosun-ai[bot]

- [7229af8] *(swiftide)* Documented file swiftide/src/integrations/qdrant/persist.rs (#24) by @bosun-ai[bot]

- [6240a26] *(swiftide)* Documented file swiftide/src/integrations/redis/mod.rs (#23) by @bosun-ai[bot]

- [7688c99] *(swiftide)* Documented file swiftide/src/integrations/qdrant/mod.rs (#22) by @bosun-ai[bot]

- [d572c88] *(swiftide)* Documented file swiftide/src/integrations/qdrant/ingestion_node.rs (#20) by @bosun-ai[bot]

- [14e24c3] *(swiftide)* Documented file swiftide/src/ingestion/mod.rs (#28) by @bosun-ai[bot]

- [502939f] *(swiftide)* Documented file swiftide/src/integrations/treesitter/supported_languages.rs (#26) by @bosun-ai[bot]

- [a78e68e] *(swiftide)* Documented file swiftide/tests/ingestion_pipeline.rs (#41) by @bosun-ai[bot]

- [289687e] *(swiftide)* Documented file swiftide/src/loaders/mod.rs (#40) by @bosun-ai[bot]

- [ebd0a5d] *(swiftide)* Documented file swiftide/src/transformers/chunk_code.rs (#39) by @bosun-ai[bot]

- [fb428d1] *(swiftide)* Documented file swiftide/src/transformers/metadata_qa_text.rs (#36) by @bosun-ai[bot]

- [305a641] *(swiftide)* Documented file swiftide/src/transformers/openai_embed.rs (#35) by @bosun-ai[bot]

- [c932897] *(swiftide)* Documented file swiftide/src/transformers/metadata_qa_code.rs (#34) by @bosun-ai[bot]

- [090ef1b] *(swiftide)* Documented file swiftide/src/integrations/openai/simple_prompt.rs (#19) by @bosun-ai[bot]

- [7cfcc83] Update readme template links and fix template by @timonv

- [a717f3d] Template links should be underscores by @timonv

### New Contributors
* @bosun-ai[bot] made their first contribution in [#19](https://github.com/bosun-ai/swiftide/pull/19)

**Full Changelog**: https://github.com/bosun-ai/swiftide/compare/v0.1.0...swiftide-v0.2.0


## [v0.1.0](https://github.com/bosun-ai/swiftide/releases/tag/v0.1.0) - 2024-06-13

### New features

- [0d9796b] *(ci)* Set up basic test and release actions (#1) by @timonv

- [2a6e503] *(doc)* Setup basic readme (#5) by @timonv

- [b8f9166] *(fluyt)* Significant tracing improvements (#368) by @timonv

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

- [0986136] *(fluyt/code_ops)* Add languages to chunker and range for chunk size (#334) by @timonv

````text
* feat(fluyt/code_ops): add more treesitter languages

  * fix: clippy + fmt

  * feat(fluyt/code_ops): implement builder and support range

  * feat(fluyt/code_ops): implement range limits for code chunking

  * feat(fluyt/indexing): code chunking supports size
````

- [f10bc30] *(ingestion_pipeline)* Default concurrency is the number of cpus (#6) by @timonv

- [7453ddc] Replace databuoy with new ingestion pipeline (#322) by @timonv

- [054b560] Fix build and add feature flags for all integrations by @timonv

### Bug fixes

- [fdf4be3] *(fluyt)* Ensure minimal tracing by @timonv

- [389b0f1] Add debug info to qdrant setup by @timonv

- [bb905a3] Use rustls on redis and log errors by @timonv

- [458801c] Properly connect to redis over tls by @timonv

### Miscellaneous

- [ce6e465] *(fluyt)* Add verbose log on checking if index exists by @timonv

- [6967b0d] Make indexing extraction compile by @tinco

- [f595f3d] Add rust-toolchain on stable by @timonv

- [da004c6] Start cleaning up dependencies by @timonv

- [cccdaf5] Remove more unused dependencies by @timonv

- [7ee8799] Remove more crates and update by @timonv

- [951f496] Clean up more crates by @timonv

- [1f17d84] Cargo update by @timonv

- [730d879] Create LICENSE by @timonv

- [44524fb] Restructure repository and rename (#3) by @timonv

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

- [e717b7f] Update issue templates by @timonv

- [8e22e0e] Cleanup by @timonv

- [4d79d27] Tests, tests, tests (#4) by @timonv

- [1036d56] Configure cargo toml (#7) by @timonv

- [0ae98a7] Cleanup Cargo keywords by @timonv

### Refactor

- [0d342ea] Models as first class citizens (#318) by @timonv

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



