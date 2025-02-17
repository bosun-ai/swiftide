<details>
  <summary>Table of Contents</summary>

<!--toc:start-->

- [What is Swiftide?](#what-is-swiftide)
  - [High level features](#high-level-features)
- [Latest updates on our blog :fire:](#latest-updates-on-our-blog-fire)
- [Examples](#examples)
- [Vision](#vision)
- [Features](#features)
  - [In detail](#in-detail)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Installation](#installation)
- [Usage and concepts](#usage-and-concepts)
  - [Indexing](#indexing)
  - [Querying](#querying)
- [Contributing](#contributing)
- [Core Team Members](#core-team-members)
- [License](#license)
<!--toc:end-->

</details>

<a name="readme-top"></a>

<!-- PROJECT SHIELDS -->
<!--
*** I'm using markdown "reference style" links for readability.
*** Reference links are enclosed in brackets [ ] instead of parentheses ( ).
*** See the bottom of this document for the declaration of the reference variables
*** for contributors-url, forks-url, etc. This is an optional, concise syntax you may use.
*** https://www.markdownguide.org/basic-syntax/#reference-style-links
-->

![CI](https://img.shields.io/github/actions/workflow/status/bosun-ai/swiftide/test.yml?style=flat-square)
![Coverage Status](https://img.shields.io/coverallsCoverage/github/bosun-ai/swiftide?style=flat-square)
[![Crate Badge]][Crate]
[![Docs Badge]][API Docs]
[![Contributors][contributors-shield]][contributors-url]
[![Stargazers][stars-shield]][stars-url]
![Discord](https://img.shields.io/discord/1257672801553354802?style=flat-square&link=https%3A%2F%2Fdiscord.gg%2F3jjXYen9UY)
[![MIT License][license-shield]][license-url]
[![LinkedIn][linkedin-shield]][linkedin-url]

<!-- PROJECT LOGO -->
<br />
<div align="center">
  <a href="https://github.com/bosun-ai/swiftide">
    <img src="https://github.com/bosun-ai/swiftide/blob/master/images/logo.png" alt="Logo" width="250" height="250">
  </a>

  <h3 align="center">Swiftide</h3>

  <p align="center">
Fast, streaming indexing, query, and agentic LLM applications in Rust
    <br />
    <a href="https://swiftide.rs"><strong>Read more on swiftide.rs »</strong></a>
    <br />
    <br />
    <!-- <a href="https://github.com/bosun-ai/swiftide">View Demo</a> -->
    <a href="https://docs.rs/swiftide/latest/swiftide/">API Docs</a>
    ·
    <a href="https://github.com/bosun-ai/swiftide/issues/new?labels=bug&template=bug_report.md">Report Bug</a>
    ·
    <a href="https://github.com/bosun-ai/swiftide/issues/new?labels=enhancement&template=feature_request.md">Request Feature</a>
    ·
    <a href="https://discord.gg/3jjXYen9UY">Discord</a>
  </p>
</div>

<!-- ABOUT THE PROJECT -->

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## What is Swiftide?

<!-- [![Product Name Screen Shot][product-screenshot]](https://example.com) -->

Swiftide is a Rust library for building LLM applications, enabling fast data ingestion, transformation, and indexing for effective querying and prompt injection, known as Retrieval Augmented Generation. It provides flexible building blocks for creating various agents, allowing rapid development from concept to production with minimal code.

### High level features

- Build fast, streaming indexing and querying pipelines
- Easily build agents, mix and match with previously built pipelines
- A modular and extendable API, with minimal abstractions
- Integrations with popular LLMs and storage providers
- Ready to use pipeline transformations

<div align="center">
    <img src="https://github.com/bosun-ai/swiftide/blob/master/images/rag-dark.svg" alt="RAG" width="100%" >
</div>

Part of the [bosun.ai](https://bosun.ai) project. An upcoming platform for autonomous code improvement.

We <3 feedback: project ideas, suggestions, and complaints are very welcome. Feel free to open an issue or contact us on [discord](https://discord.gg/3jjXYen9UY).

> [!CAUTION]
> Swiftide is under heavy development and can have breaking changes. Documentation might fall short of all features, and despite our efforts be slightly outdated. We recommend to always keep an eye on our [github](https://github.com/bosun-ai/swiftide) and [api documentation](https://docs.rs/swiftide/latest/swiftide/). If you found an issue or have any kind of feedback we'd love to hear from you.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Latest updates on our blog :fire:

- [Releasing kwaak with kwaak](https://bosun.ai/posts/releasing-kwaak-with-kwaak/)
- [Release - Swiftide 0.16](https://bosun.ai/posts/swiftide-0-16/)
- [Rust in LLM based tools for performance](https://bosun.ai/posts/rust-for-genai-performance/)
- [Evaluate Swiftide pipelines with Ragas](https://bosun.ai/posts/evaluating-swiftide-with-ragas/) (2024-09-15)
- [Release - Swiftide 0.12](https://bosun.ai/posts/swiftide-0-12/) (2024-09-13)
- [Local code intel with Ollama, FastEmbed and OpenTelemetry](https://bosun.ai/posts/ollama-and-telemetry/) (2024-09-04

- [Release - Swiftide 0.9](https://bosun.ai/posts/swiftide-0-9/) (2024-09-02)
- [Bring your own transformers](https://bosun.ai/posts/bring-your-own-transformers-in-swiftide/) (2024-08-13)
- [Release - Swiftide 0.8](https://bosun.ai/posts/swiftide-0-8/) (2024-08-12)
- [Release - Swiftide 0.7](https://bosun.ai/posts/swiftide-0-7/) (2024-07-28)
- [Building a code question answering pipeline](https://bosun.ai/posts/indexing-and-querying-code-with-swiftide/) (2024-07-13)
- [Release - Swiftide 0.6](https://bosun.ai/posts/swiftide-0-6/) (2024-07-12)
- [Release - Swiftide 0.5](https://bosun.ai/posts/swiftide-0-5/) (2024-07-1)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Examples

Indexing a local code project, chunking into smaller pieces, enriching the nodes with metadata, and persisting into [Qdrant](https://qdrant.tech):

```rust
indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .with_default_llm_client(openai_client.clone())
        .filter_cached(Redis::try_from_url(
            redis_url,
            "swiftide-examples",
        )?)
        .then_chunk(ChunkCode::try_for_language_and_chunk_size(
            "rust",
            10..2048,
        )?)
        .then(MetadataQACode::default())
        .then(move |node| my_own_thing(node))
        .then_in_batch(Embed::new(openai_client.clone()))
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .build()?,
        )
        .run()
        .await?;
```

Querying for an example on how to use the query pipeline:

```rust
query::Pipeline::default()
    .then_transform_query(GenerateSubquestions::from_client(
        openai_client.clone(),
    ))
    .then_transform_query(Embed::from_client(
        openai_client.clone(),
    ))
    .then_retrieve(qdrant.clone())
    .then_answer(Simple::from_client(openai_client.clone()))
    .query("How can I use the query pipeline in Swiftide?")
    .await?;
```

Running an agent that can search code:

```rust
    agents::Agent::builder()
        .llm(&openai)
        .tools(vec![search_code()])
        .build()?
        .query("In what file can I find an example of a swiftide agent?")
        .await?;
```

_You can find more detailed examples in [/examples](https://github.com/bosun-ai/swiftide/tree/master/examples)_

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Vision

Our goal is to create a fast, extendable platform for building LLM applications in Rust, to further the development of automated AI applications, with an easy-to-use and easy-to-extend api.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Features

- Fast, modular streaming indexing pipeline with async, parallel processing
- Experimental query pipeline
- Experimental agent framework
- A variety of loaders, transformers, semantic chunkers, embedders, and more
- Bring your own transformers by extending straightforward traits or use a closure
- Splitting and merging pipelines
- Jinja-like templating for prompts
- Store into multiple backends
- Integrations with OpenAI, Groq, Redis, Qdrant, Ollama, FastEmbed-rs, Fluvio, LanceDB, and Treesitter
- Evaluate pipelines with RAGAS
- Sparse vector support for hybrid search
- `tracing` supported for logging and tracing, see /examples and the `tracing` crate for more information.

### In detail

| **Feature**                                  | **Details**                                                                                                                                                          |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Supported Large Language Model providers** | OpenAI (and Azure) - All models and embeddings <br> Anthropic <br> OpenRouter <br> AWS Bedrock - Anthropic and Titan <br> Groq - All models <br> Ollama - All models |
| **Loading data**                             | Files <br> Scraping <br> Fluvio <br> Parquet <br> Other pipelines and streams                                                                                        |
| **Transformers and metadata generation**     | Generate Question and answerers for both text and code (Hyde) <br> Summaries, titles and queries via an LLM <br> Extract definitions and references with tree-sitter |
| **Splitting and chunking**                   | Markdown <br> Text (text_splitter) <br> Code (with tree-sitter)                                                                                                      |
| **Storage**                                  | Qdrant <br> Redis <br> LanceDB                                                                                                                                       |
| **Query pipeline**                           | Similarity and hybrid search, query and response transformations, and evaluation                                                                                     |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- GETTING STARTED -->

## Getting Started

### Prerequisites

Make sure you have the rust toolchain installed. [rustup](https://rustup.rs) Is the recommended approach.

To use OpenAI, an API key is required. Note that by default `async_openai` uses the `OPENAI_API_KEY` environment variables.

Other integrations might have their own requirements.

### Installation

1. Set up a new Rust project
2. Add swiftide

   ```sh
   cargo add swiftide
   ```

3. Enable the features of integrations you would like to use in your `Cargo.toml`
4. Write a pipeline (see our examples and documentation)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->

## Usage and concepts

Before building your streams, you need to enable and configure any integrations required. See /examples.

_We have a lot of examples, please refer to /examples and the [Documentation](https://docs.rs/swiftide/latest/swiftide/)_

> [!NOTE]
> No integrations are enabled by default as some are code heavy. We recommend you to cherry-pick the integrations you need. By convention flags have the same name as the integration they represent.

### Indexing

An indexing stream starts with a Loader that emits Nodes. For instance, with the Fileloader each file is a Node.

You can then slice and dice, augment, and filter nodes. Each different kind of step in the pipeline requires different traits. This enables extension.

Nodes have a path, chunk and metadata. Currently metadata is copied over when chunking and _always_ embedded when using the OpenAIEmbed transformer.

- **from_loader** `(impl Loader)` starting point of the stream, creates and emits Nodes
- **filter_cached** `(impl NodeCache)` filters cached nodes
- **then** `(impl Transformer)` transforms the node and puts it on the stream
- **then_in_batch** `(impl BatchTransformer)` transforms multiple nodes and puts them on the stream
- **then_chunk** `(impl ChunkerTransformer)` transforms a single node and emits multiple nodes
- **then_store_with** `(impl Storage)` stores the nodes in a storage backend, this can be chained

Additionally, several generic transformers are implemented. They take implementers of `SimplePrompt` and `EmbedModel` to do their things.

> [!WARNING]
> Due to the performance, chunking before adding metadata gives rate limit errors on OpenAI very fast, especially with faster models like 3.5-turbo. Be aware.

### Querying

A query stream starts with a search strategy. In the query pipeline a `Query` goes through several stages. Transformers and retrievers work together to get the right context into a prompt, before generating an answer. Transformers and Retrievers operate on different stages of the Query via a generic statemachine. Additionally, the search strategy is generic over the pipeline and Retrievers need to implement specifically for each strategy.

That sounds like a lot but, tl&dr; the query pipeline is _fully and strongly typed_.

- **Pending** The query has not been executed, and can be further transformed with transformers
- **Retrieved** Documents have been retrieved, and can be further transformed to provide context for an answer
- **Answered** The query is done

Additionally, query pipelines can also be evaluated. I.e. by [Ragas](https://ragas.io).

Similar to the indexing pipeline each step is governed by simple Traits and closures implement these traits as well.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ROADMAP -->

## Contributing

Swiftide is in a very early stage and we are aware that we lack features for the wider community. Contributions are very welcome. :tada:

If you have a great idea, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

If you just want to contribute (bless you!), see [our issues](https://github.com/bosun-ai/swiftide/issues) or join us on Discord.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

See [CONTRIBUTING](https://github.com/bosun-ai/swiftide/blob/master/CONTRIBUTING.md) for more

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Core Team Members

<table>
  <tr>
    <td align="center">
      <a href="https://github.com/timonv">
        <img
          src="https://avatars.githubusercontent.com/u/49373?s=100"
          width="100px;"
          alt=""
        />
        <br /><sub><b>timonv</b></sub>
        <br /><sub>open for swiftide consulting</sub>
      </a>
    </td>
    <td align="center">
      <a href="https://github.com/tinco">
        <img
          src="https://avatars.githubusercontent.com/u/22532?s=100"
          width="100px;"
          alt=""
        />
        <br /><sub><b>tinco</b></sub>
        <br /><br />
      </a>
    </td>
  </tr>
</table>

<!-- LICENSE -->

## License

Distributed under the MIT License. See `LICENSE` for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->

[contributors-shield]: https://img.shields.io/github/contributors/bosun-ai/swiftide.svg?style=flat-square
[contributors-url]: https://github.com/bosun-ai/swiftide/graphs/contributors
[stars-shield]: https://img.shields.io/github/stars/bosun-ai/swiftide.svg?style=flat-square
[stars-url]: https://github.com/bosun-ai/swiftide/stargazers
[license-shield]: https://img.shields.io/github/license/bosun-ai/swiftide.svg?style=flat-square
[license-url]: https://github.com/bosun-ai/swiftide/blob/master/LICENSE.txt
[linkedin-shield]: https://img.shields.io/badge/-LinkedIn-black.svg?style=flat-square&logo=linkedin&colorB=555
[linkedin-url]: https://www.linkedin.com/company/bosun-ai
[Crate Badge]: https://img.shields.io/crates/v/swiftide?logo=rust&style=flat-square&logoColor=E05D44&color=E05D44
[Crate]: https://crates.io/crates/swiftide
[Docs Badge]: https://img.shields.io/docsrs/swiftide?logo=rust&style=flat-square&logoColor=E05D44
[API Docs]: https://docs.rs/swiftide
