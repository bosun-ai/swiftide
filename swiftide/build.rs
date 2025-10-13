use std::{fs, path::Path};

fn main() {
    let readme_path = Path::new("README.md");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_readme = Path::new(&out_dir).join("README.docs.md");

    // Read README.md
    let contents = fs::read_to_string(readme_path).expect("Failed to read README.md");

    // Replace ```rust with ```ignore
    let patched = contents.replace("```rust", "```ignore");

    // Write the modified README to OUT_DIR
    fs::write(&out_readme, patched).expect("Failed to write patched README");

    // Tell Cargo to re-run build.rs if README changes
    println!("cargo:rerun-if-changed=README.md");

    // Export the path so we can include it in lib.rs
    println!("cargo:rustc-env=DOC_README={}", out_readme.display());
}
