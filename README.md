<details>
  <summary>Table of Contents</summary>

<!--toc:start-->

- [About The Project](#about-the-project)
- [Latest updates on our blog :fire:](#latest-updates-on-our-blog-fire)
- [Example](#example)
- [Vision](#vision)
- [Features](#features)
  - [In detail](#in-detail)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Installation](#installation)
- [Usage and concepts](#usage-and-concepts)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
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
Fast, streaming indexing and query library for AI applications, written in Rust
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
  </p>
</div>

<!-- ABOUT THE PROJECT -->

## About The Project

<!-- [![Product Name Screen Shot][product-screenshot]](https://example.com) -->

Swiftide is a data indexing, processing and query library, tailored for Retrieval Augmented Generation (RAG). When building applications with large language models (LLM), these LLMs need access to external resources. Data needs to be transformed, enriched, split up, embedded, and persisted. It is build in Rust, using parallel, asynchronous streams and is blazingly fast.

With Swiftide, you can build your AI application from idea to production in a few lines of code.

<div align="center">
    <img src="https://github.com/bosun-ai/swiftide/blob/master/images/rag-dark.svg" alt="RAG" width="100%" >
</div>

While working with other Python-based tooling, frustrations arose around performance, stability, and ease of use. Thus, Swiftide was born. Indexing performance went from tens of minutes to a few seconds.

Part of the [bosun.ai](https://bosun.ai) project. An upcoming platform for autonomous code improvement.

We <3 feedback: project ideas, suggestions, and complaints are very welcome. Feel free to open an issue.

> [!CAUTION]
> Swiftide is under heavy development and can have breaking changes while we work towards 1.0. Documentation here might fall short of all features, and despite our efforts be slightly outdated. Expect bugs. We recommend to always keep an eye on our [github](https://github.com/bosun-ai/swiftide) and [api documentation](https://docs.rs/swiftide/latest/swiftide/). If you found an issue or have any kind of feedback we'd love to hear from you in an issue.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Latest updates on our blog :fire:

- [Building a code question answering pipeline](https://bosun.ai/posts/indexing-and-querying-code-with-swiftide/) (2024-07-13)
- [Release - Swiftide 0.6](https://bosun.ai/posts/swiftide-0-6/) (2024-07-12)
- [Release - Swiftide 0.5](https://bosun.ai/posts/swiftide-0-5/) (2024-07-1)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Example

```rust
indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .filter_cached(Redis::try_from_url(
            redis_url,
            "swiftide-examples",
        )?)
        .then_chunk(ChunkCode::try_for_language_and_chunk_size(
            "rust",
            10..2048,
        )?)
        .then(MetadataQACode::new(openai_client.clone()))
        .then_in_batch(10, Embed::new(openai_client.clone()))
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .build()?,
        )
        .run()
        .await?;
```

_You can find more examples in [/examples](https://github.com/bosun-ai/swiftide/tree/master/examples)_

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Vision

Our goal is to create a fast, extendable platform for data indexing and querying to further the development of automated LLM applications, with an easy-to-use and easy-to-extend api.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Features

- Fast streaming indexing pipeline with async, parallel processing
- Experimental query pipeline
- Integrations with OpenAI, Groq, Redis, Qdrant, FastEmbed, and Treesitter
- A variety of loaders, transformers, and embedders and other common, generic tools
- Bring your own transformers by extending straightforward traits
- Splitting and merging pipelines
- Jinja-like templating for prompts
- Store into multiple backends
- `tracing` supported for logging and tracing, see /examples and the `tracing` crate for more information.

### In detail

| **Feature**                                  | **Details**                                                                                                                                                          |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Supported Large Language Model providers** | OpenAI (and Azure) - All models and embeddings <br> AWS Bedrock - Anthropic and Titan <br> Groq - All models                                                         |
| **Loading data**                             | Files <br> Scraping <br> Other pipelines and streams                                                                                                                 |
| **Transformers and metadata generation**     | Generate Question and answerers for both text and code (Hyde) <br> Summaries, titles and queries via an LLM <br> Extract definitions and references with tree-sitter |
| **Splitting and chunking**                   | Markdown <br> Code (with tree-sitter)                                                                                                                                |
| **Storage**                                  | Qdrant <br> Redis                                                                                                                                                    |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- GETTING STARTED -->

## Getting Started

### Prerequisites

Make sure you have the rust toolchain installed. [rustup](https://rustup.rs) Is the recommended approach.

To use OpenAI, an API key is required. Note that by default `async_openai` uses the `OPENAI_API_KEY` environment variables.

Other integrations will need to be installed accordingly.

### Installation

1. Set up a new Rust project
2. Add swiftide

   ```sh
   cargo add swiftide
   ```

3. Enable the features of integrations you would like to have or use 'all' in your `Cargo.toml`
4. Write a pipeline (see our examples and documentation)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->

## Usage and concepts

Before building your stream, you need to enable and configure any integrations required. See /examples.

A stream starts with a Loader that emits Nodes. For instance, with the Fileloader each file is a Node.

You can then slice and dice, augment, and filter nodes. Each different kind of step in the pipeline requires different traits. This enables extension.

Nodes have a path, chunk and metadata. Currently metadata is copied over when chunking and _always_ embedded when using the OpenAIEmbed transformer.

- **from_loader** `(impl Loader)` starting point of the stream, creates and emits Nodes
- **filter_cached** `(impl NodeCache)` filters cached nodes
- **then** `(impl Transformer)` transforms the node and puts it on the stream
- **then_in_batch** `(impl BatchTransformer)` transforms multiple nodes and puts them on the stream
- **then_chunk** `(impl ChunkerTransformer)` transforms a single node and emits multiple nodes
- **then_store_with** `(impl Storage)` stores the nodes in a storage backend, this can be chained

Additionally, several generic transformers are implemented. They take implementers of `SimplePrompt` and `EmbedModel` to do their things.

> [!NOTE]
> No integrations are enabled by default as some are code heavy. Either cherry-pick the integrations you need or use the "all" feature flag.

> [!WARNING]
> Due to the performance, chunking before adding metadata gives rate limit errors on OpenAI very fast, especially with faster models like 3.5-turbo. Be aware.

_For more examples, please refer to /examples and the [Documentation](https://docs.rs/swiftide/latest/swiftide/)_

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ROADMAP -->

## Roadmap

See the [open issues](https://github.com/bosun-ai/swiftide/issues) for a full list of proposed features (and known issues).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTRIBUTING -->

## Contributing

Swiftide is in a very early stage and we are aware that we lack features for the wider community. Contributions are very welcome. :tada:

If you have a great idea, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

If you just want to contribute (bless you!), see [our issues](https://github.com/bosun-ai/swiftide/issues).

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

See [CONTRIBUTING](https://github.com/bosun-ai/swiftide/blob/master/CONTRIBUTING.md) for more

<p align="right">(<a href="#readme-top">back to top</a>)</p>

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
