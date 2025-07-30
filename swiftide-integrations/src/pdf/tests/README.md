# PDF Integration Tests

This directory contains integration tests for the PDF loader functionality.

## Test Files

To run the integration tests, you need to provide test PDF files:

1. `data/cv.pdf` - A single-page CV PDF for basic testing
2. `data/multi_page.pdf` - A multi-page PDF for testing page-by-page extraction

## Running Tests

To run the integration tests, use the following command:

```bash
cargo test -p swiftide-integrations --features pdf -- pdf::loader::tests::test_pdf_loader_stream_real_cv
cargo test -p swiftide-integrations --features pdf -- pdf::loader::tests::test_pdf_loader_stream_real_multi_page
```

Note: These tests are marked as `#[ignore]` by default since they require external test files. Remove the `#[ignore]` attribute or use `--ignored` flag to run them.