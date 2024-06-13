<!-- Improved compatibility of back to top link: See: https://github.com/othneildrew/Best-README-Template/pull/73 -->

<a name="readme-top"></a>

<!-- PROJECT SHIELDS -->
<!--
*** I'm using markdown "reference style" links for readability.
*** Reference links are enclosed in brackets [ ] instead of parentheses ( ).
*** See the bottom of this document for the declaration of the reference variables
*** for contributors-url, forks-url, etc. This is an optional, concise syntax you may use.
*** https://www.markdownguide.org/basic-syntax/#reference-style-links
-->

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![MIT License][license-shield]][license-url]
[![LinkedIn][linkedin-shield]][linkedin-url]

<!-- PROJECT LOGO -->
<br />
<div align="center">
  <a href="https://github.com/bosun-ai/swiftide">
    <img src="images/logo.png" alt="Logo" width="250" height="250">
  </a>

<h3 align="center">Swiftide</h3>

  <p align="center">
    Blazing fast asynchronous, parallel file ingestion and processing for RAG.
    <br />
    <a href="https://github.com/bosun-ai/swiftide"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="https://github.com/bosun-ai/swiftide">View Demo</a>
    ·
    <a href="https://github.com/bosun-ai/swiftide/issues/new?labels=bug&template=bug-report---.md">Report Bug</a>
    ·
    <a href="https://github.com/bosun-ai/swiftide/issues/new?labels=enhancement&template=feature-request---.md">Request Feature</a>
  </p>
</div>

<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#example">Example</a></li>
      </ul>
      <ul>
        <li><a href="#features">Features</a></li>
      </ul>
      <ul>
        <li><a href="#vision">Vision</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

<!-- ABOUT THE PROJECT -->

## About The Project

<!-- [![Product Name Screen Shot][product-screenshot]](https://example.com) -->

Swiftide is a straightforward, easy-to-use, easy-to-extend asynchronous file ingestion and processing system. It is designed to be used in a RAG (Research Augmented Generation) system. It is built to be fast and efficient, with a focus on parallel processing and asynchronous operations.

While working with other Python based tooling, frustrations arose around performance, stability and ease-of-use. Ingestion performance went from tens of minutes to a few seconds.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Example

tbd

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Features

tbd

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Vision

Our goal is to create an extremely fast, extendable platform for ingestion and querying to further the development of automated LLM applications, with an easy-to-use and easy-to-extend api.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- GETTING STARTED -->

## Getting Started

### Prerequisites

Make sure you have the rust toolchain installed. [rustup](https://rustup.rs) Is the recommended approach.

To use OpenAI, a api key is required. Note that by default `async_openai` uses the `OPENAI_API_KEY` environment variables.

Other integrations will need to be installed accordingly.

### Installation

1. Set up a new Rust project
2. Add swiftide
   ```sh
   cargo add swiftide
   ```
3. Write a pipeline (see our examples and documentation)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->

## Usage and concepts

tbd

_For more examples, please refer to /examples and the [Documentation](https://example.com)_

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ROADMAP -->

## Roadmap

- [ ] Python / Nodejs bindings
- [ ] Multiple storage and sparse vector support
  - [ ] Nested Feature

See the [open issues](https://github.com/bosun-ai/swiftide/issues) for a full list of proposed features (and known issues).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTRIBUTING -->

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- LICENSE -->

## License

Distributed under the MIT License. See `LICENSE` for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTACT -->

## Contact

Your Name - [@twitter_handle](https://twitter.com/twitter_handle) - email@email_client.com

Project Link: [https://github.com/bosun-ai/swiftide](https://github.com/bosun-ai/swiftide)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ACKNOWLEDGMENTS -->

## Acknowledgments

- []()
- []()
- []()

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->

[contributors-shield]: https://img.shields.io/github/contributors/bosun-ai/swiftide.svg?style=for-the-badge
[contributors-url]: https://github.com/bosun-ai/swiftide/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/bosun-ai/swiftide.svg?style=for-the-badge
[forks-url]: https://github.com/bosun-ai/swiftide/network/members
[stars-shield]: https://img.shields.io/github/stars/bosun-ai/swiftide.svg?style=for-the-badge
[stars-url]: https://github.com/bosun-ai/swiftide/stargazers
[issues-shield]: https://img.shields.io/github/issues/bosun-ai/swiftide.svg?style=for-the-badge
[issues-url]: https://github.com/bosun-ai/swiftide/issues
[license-shield]: https://img.shields.io/github/license/bosun-ai/swiftide.svg?style=for-the-badge
[license-url]: https://github.com/bosun-ai/swiftide/blob/master/LICENSE.txt
[linkedin-shield]: https://img.shields.io/badge/-LinkedIn-black.svg?style=for-the-badge&logo=linkedin&colorB=555
[linkedin-url]: https://linkedin.com/in/linkedin_username
[product-screenshot]: images/screenshot.png
