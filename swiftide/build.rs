use std::{fs, path::Path};

fn main() {
    let readme_path = Path::new("README.md");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_readme = Path::new(&out_dir).join("README.docs.md");

    // Read README.md
    let contents = fs::read_to_string(readme_path).expect("Failed to read README.md");

    let patched = contents
        // Rustdoc compiles crate docs as doctests; README snippets are validated separately.
        .replace("```rust", "```ignore")
        // README-local paths work on GitHub but not from docs.rs crate pages.
        .replace(
            "src=\"images/composition.svg\"",
            "src=\"https://raw.githubusercontent.com/bosun-ai/swiftide/master/images/composition.svg\"",
        )
        .replace(
            "[CONTRIBUTING.md](CONTRIBUTING.md)",
            "[CONTRIBUTING.md](https://github.com/bosun-ai/swiftide/blob/master/CONTRIBUTING.md)",
        )
        .replace(
            "[AGENTS.md](AGENTS.md)",
            "[AGENTS.md](https://github.com/bosun-ai/swiftide/blob/master/AGENTS.md)",
        )
        .replace(
            "[LICENSE](LICENSE)",
            "[LICENSE](https://github.com/bosun-ai/swiftide/blob/master/LICENSE)",
        );

    // Write the modified README to OUT_DIR
    fs::write(&out_readme, patched).expect("Failed to write patched README");

    // Tell Cargo to re-run build.rs if README changes
    println!("cargo:rerun-if-changed=README.md");

    // Export the path so we can include it in lib.rs
    println!("cargo:rustc-env=DOC_README={}", out_readme.display());
}
