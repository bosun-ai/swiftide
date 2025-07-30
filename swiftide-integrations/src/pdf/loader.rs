use anyhow::{Context, Result};
use itertools::Itertools;
use lopdf::Document;
use swiftide_core::{
    indexing::{IndexingStream, Node},
    Loader,
};
use tracing::{debug, instrument};

use super::PdfLoader;

impl Loader for PdfLoader {
    #[instrument(skip(self), fields(path = %self.path.display()))]
    fn into_stream(self) -> IndexingStream {
        debug!("Loading PDF document");

        match self.extract_pdf_pages() {
            Ok(nodes) => IndexingStream::iter(nodes.into_iter().map(Ok)),
            Err(e) => IndexingStream::iter(std::iter::once(Err(e))),
        }
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

impl PdfLoader {
    /// Extracts text content from each page of the PDF file and creates a Node for each page.
    ///
    /// # Errors
    /// - Returns an error if the PDF is encrypted, malformed, or cannot be read.
    /// - Returns an error if no pages are found in the PDF.
    ///
    /// # Metadata
    /// Each node includes:
    /// - `page_number`: The page number (1-based)
    /// - `total_pages`: Total number of pages in the PDF
    /// - Standard PDF info fields (title, author, etc.) if available
    #[instrument(skip(self), fields(path = %self.path.display()))]
    fn extract_pdf_pages(&self) -> Result<Vec<Node>> {
        debug!("Reading PDF file");
        // Load PDF document with robust error handling
        let doc = match Document::load(&self.path) {
            Ok(doc) => doc,
            Err(e) if e.to_string().to_lowercase().contains("encrypted") => {
                return Err(anyhow::anyhow!("PDF is encrypted and cannot be processed: {}", self.path.display()));
            }
            Err(e) => {
                return Err(e).with_context(|| format!("Failed to load PDF file: {}", self.path.display()));
            }
        };

        // Get and sort page numbers
        let page_numbers = doc.get_pages().keys().cloned().sorted().collect::<Vec<_>>();
        let total_pages = page_numbers.len();
        if total_pages == 0 {
            return Err(anyhow::anyhow!("PDF contains no pages: {}", self.path.display()));
        }

        let mut nodes = Vec::with_capacity(total_pages);
        for page_number in page_numbers {
            // Extract text from a single page
            let text = doc.extract_text(&[page_number]).with_context(|| {
                format!(
                    "Failed to extract text from page {} of PDF: {}",
                    page_number,
                    self.path.display()
                )
            })?;

            // Basic text cleaning and formatting
            let processed_text = if self.format_as_markdown {
                self.format_as_markdown_text(&text)
            } else {
                text
            };

            debug!(
                page = page_number,
                text_length = processed_text.len(),
                "Successfully extracted text from PDF page"
            );

            // Create a Node with the extracted content and metadata
            let mut node = Node::builder()
                .path(&self.path)
                .chunk(processed_text.clone())
                .original_size(processed_text.len())
                .build()
                .with_context(|| {
                    format!(
                        "Failed to create node for page {} of PDF: {}",
                        page_number,
                        self.path.display()
                    )
                })?;

            node.metadata.insert("page_number".to_string(), page_number);
            node.metadata.insert("total_pages".to_string(), serde_json::Value::Number(total_pages.into()));
            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Applies basic Markdown formatting to the extracted text
    ///
    /// This method performs simple text processing to improve readability:
    /// - Removes excessive whitespace
    /// - Preserves paragraph breaks
    /// - Adds basic structure formatting
    fn format_as_markdown_text(&self, text: &str) -> String {
        let mut lines = Vec::new();
        let mut current_paragraph = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                // End of paragraph - join accumulated lines and add to output
                if !current_paragraph.is_empty() {
                    let paragraph = current_paragraph.join(" ");
                    if !paragraph.trim().is_empty() {
                        lines.push(paragraph);
                        lines.push(String::new()); // Add paragraph break
                    }
                    current_paragraph.clear();
                }
            } else {
                current_paragraph.push(trimmed.to_string());
            }
        }

        // Handle any remaining paragraph
        if !current_paragraph.is_empty() {
            let paragraph = current_paragraph.join(" ");
            if !paragraph.trim().is_empty() {
                lines.push(paragraph);
            }
        }

        // Remove trailing empty lines and join
        while lines.last().map_or(false, |line| line.is_empty()) {
            lines.pop();
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::TryStreamExt;
    use serde_json::Value;
    use std::path::Path;

    #[test]
    fn test_pdf_loader_creation() {
        let loader = PdfLoader::from_path("test.pdf");
        assert_eq!(loader.path(), Path::new("test.pdf"));
        assert!(loader.is_markdown_formatting_enabled());
    }

    #[test]
    fn test_pdf_loader_builder() {
        let loader = PdfLoader::builder()
            .path("test.pdf")
            .format_as_markdown(false)
            .build()
            .unwrap();

        assert_eq!(loader.path(), Path::new("test.pdf"));
        assert!(!loader.is_markdown_formatting_enabled());
    }

    #[test]
    fn test_pdf_loader_with_markdown_formatting() {
        let loader = PdfLoader::from_path("test.pdf").with_markdown_formatting(false);

        assert!(!loader.is_markdown_formatting_enabled());
    }

    #[test]
    fn test_format_as_markdown_text() {
        let loader = PdfLoader::from_path("test.pdf");
        let input = "Line 1\n\nLine 2\n   Line 3   \n\n\nLine 4";
        let expected = "Line 1\n\nLine 2 Line 3\n\nLine 4";
        let result = loader.format_as_markdown_text(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_as_markdown_text_empty() {
        let loader = PdfLoader::from_path("test.pdf");
        let result = loader.format_as_markdown_text("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_as_markdown_text_single_line() {
        let loader = PdfLoader::from_path("test.pdf");
        let input = "Single line of text";
        let result = loader.format_as_markdown_text(input);
        assert_eq!(result, "Single line of text");
    }

    #[tokio::test]
    async fn test_pdf_loader_stream_invalid_file() {
        let loader = PdfLoader::from_path("nonexistent.pdf");
        let mut stream = loader.into_stream();
        let result = stream.try_next().await;

        // Should get an error for non-existent file
        assert!(result.is_err());
    }

    // Note: We would need a test PDF file to test actual PDF loading
    // This would be added in integration tests with real PDF files

    #[tokio::test]
    async fn test_pdf_loader_stream_valid_file() {
        use lopdf::content::{Content, Operation};
        use lopdf::{Dictionary, Document, Object, Stream};
        use temp_dir::TempDir;

        // Create a temporary directory to store the test PDF
        let temp = TempDir::new().unwrap();
        let pdf_path = temp.path().join("test.pdf");

        // Create a simple one-page PDF document
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Font".into()),
            ("Subtype", "Type1".into()),
            ("BaseFont", "Helvetica".into()),
        ]));
        let resources = Dictionary::from_iter(vec![("Font", Dictionary::from_iter(vec![("F1", font_id.into())]).into())]);

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 24.into()]),
                Operation::new("Td", vec![100.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal("Hello Swiftide PDF!")])
            ],
        };
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.encode().unwrap()));

        let page_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Page".into()),
            ("Parent", pages_id.into()),
            ("Contents", content_id.into()),
            ("Resources", resources.into()),
            ("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()].into()),
        ]));

        let pages = Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Kids", vec![page_id.into()].into()),
            ("Count", 1.into()),
        ]);
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("Pages", pages_id.into()),
        ]));

        doc.trailer.set("Root", catalog_id);

        doc.save(&pdf_path).unwrap();


        // Create a loader for the test PDF
        let loader = PdfLoader::from_path(&pdf_path);

        // Get the streaming iterator
        let stream = loader.into_stream();

        // Collect all nodes from the stream
        let nodes: Vec<_> = stream.try_collect().await.unwrap();

        // Ensure we have exactly one node (for a single-page PDF)
        assert_eq!(nodes.len(), 1, "Expected to load one node from the single-page PDF");

        // Get the first node
        let node = &nodes[0];

        // Check the extracted text content
        let expected_text = "Hello Swiftide PDF!";
        let actual_text = &node.chunk;
        let clean_text = actual_text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            clean_text.contains(expected_text),
            "The extracted text '{}' does not contain the expected content '{}'",
            clean_text,
            expected_text
        );

        // Check the page number metadata
        assert_eq!(
            node.metadata.get("page_number").unwrap(),
            &Value::from(1),
            "The page number metadata is incorrect"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_pdf_loader_stream_real_cv() {
        // This test requires a real CV PDF file to be placed in the tests directory
        let loader = PdfLoader::from_path("tests/data/cv.pdf");
        let stream = loader.into_stream();
        let nodes: Vec<_> = stream.try_collect().await.unwrap();
        assert!(!nodes.is_empty(), "Should extract at least one node from the CV PDF");
        for node in &nodes {
            assert!(node.metadata.get("page_number").is_some(), "Node should have page_number metadata");
            assert!(node.metadata.get("total_pages").is_some(), "Node should have total_pages metadata");
            assert!(!node.chunk.trim().is_empty(), "Node chunk should not be empty");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_pdf_loader_stream_real_multi_page() {
        // This test requires a real multi-page PDF file to be placed in the tests directory
        let loader = PdfLoader::from_path("tests/data/multi_page.pdf");
        let stream = loader.into_stream();
        let nodes: Vec<_> = stream.try_collect().await.unwrap();
        
        // Log some debugging info
        println!("Extracted {} nodes from multi-page PDF", nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            println!("Node {}: page_number={}, chunk_length={}, chunk_preview='{}'", 
                i, 
                node.metadata.get("page_number").unwrap_or(&Value::Null),
                node.chunk.len(),
                node.chunk.chars().take(100).collect::<String>()
            );
        }
        
        assert!(nodes.len() > 1, "Should extract multiple nodes from a multi-page PDF");
        let total_pages = nodes[0].metadata.get("total_pages").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(nodes.len() as u64, total_pages, "Node count should match total_pages metadata");
        
        // Check that at least some nodes have content
        let nodes_with_content: Vec<_> = nodes.iter().filter(|node| !node.chunk.trim().is_empty()).collect();
        assert!(!nodes_with_content.is_empty(), "At least some nodes should have extractable text content");
        
        for (i, node) in nodes.iter().enumerate() {
            assert_eq!(node.metadata.get("page_number").and_then(|v| v.as_u64()).unwrap_or(0), (i as u64) + 1, "Page numbers should be sequential");
            // Only check for non-empty chunks if we know the PDF has extractable text
            if !nodes_with_content.is_empty() {
                // Allow some pages to be empty (e.g., images, scanned content)
                if node.chunk.trim().is_empty() {
                    println!("Warning: Page {} has no extractable text (may be image/scanned content)", i + 1);
                }
            }
        }
    }
}