use ::swiftide::{
    ingestion::{IngestionPipeline, IngestionStream},
    Loader,
};
use pyo3::prelude::*;

#[pyclass(name = "IngestionPipeline")]
struct PythonIngestionPipeline {
    inner: IngestionPipeline,
}

#[pyclass(name = "Loader")]
struct PythonLoader(Box<dyn Loader + Send>);

impl Loader for PythonLoader {
    fn into_stream(self: Box<Self>) -> IngestionStream {
        let loader = self.0;
        loader.into_stream()
    }
}

#[pymethods]
impl PythonIngestionPipeline {
    fn from_loader(loader: PythonLoader) -> Self {
        Self {
            inner: IngestionPipeline::from_loader(loader),
        }
    }
}

#[pymodule]
fn swiftide(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // TODO: Setup logging here

    Ok(())
}
