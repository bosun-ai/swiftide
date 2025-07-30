//! # [Swiftide] PDF Ingestion Example
//!
//! This example demonstrates how to use the `PdfLoader` to ingest a PDF file
//! and process it in an indexing pipeline.
//!
//! The pipeline will:
//! - Create a temporary PDF file with some sample text.
//! - Load the PDF file using the `PdfLoader`.
//! - Log the resulting nodes to the console.

use swiftide::{
    indexing,
    integrations::pdf::PdfLoader,
};
use swiftide::indexing::persist::MemoryStorage;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, Stream};
use temp_dir::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // 1. Create a temporary PDF file
    let temp = TempDir::new()?;
    let pdf_path = temp.path().join("test.pdf");

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
    doc.save(&pdf_path)?;

    // 2. Create a pipeline from the PdfLoader
    indexing::Pipeline::from_loader(PdfLoader::from_path(&pdf_path))
        .then_store_with(MemoryStorage::default())
        .log_all()
        .run()
        .await?;

    Ok(())
}