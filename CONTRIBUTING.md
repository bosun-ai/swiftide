# Contribution guidelines

Swiftide is in a very early stage and we are aware that we do lack features for the wider community. Contributions are very welcome. :tada: 

## Feature requests and feedback

We love them, please let us know what you would like. Use one of the templates provided.

## Code design

* Simple, thin wrappers with sane defaults
* Provide a builder (derive_builder) for easy customization
* Keep Rust complexity (Arc/Box/Lifetimes/Pinning ...) encapsulated and away from library users
* Adhere to [Rust api naming](https://rust-lang.github.io/api-guidelines/naming.html) as much as possible

## Bug reports

It happens, but we still love them.

## Submitting pull requests

If you have a great idea, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

If you just want to contribute (bless you!), see [our issues](https://github.com/bosun-ai/swiftide/issues).

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

Make sure that:

* Public functions are documented in code
* Documentation is updated in the [user documentation](https://github.com/bosun-ai/swiftide-website)
* Tests are added
* Verified performance with benchmarks if applicable
