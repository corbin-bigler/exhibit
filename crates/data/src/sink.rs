use std::path::PathBuf;

use exhibit_domain::ports::{DocumentSink, PortError};

/// Writes next to the target and renames over it, so a failed export never leaves half a file.
pub struct FileSink {
    path: PathBuf,
}

impl FileSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl DocumentSink for FileSink {
    fn write(&self, bytes: &[u8]) -> Result<(), PortError> {
        let path = &self.path;
        let temp = path.with_extension("pdf.partial");
        std::fs::write(&temp, bytes).map_err(|e| PortError(format!("couldn't write {}: {e}", temp.display())))?;
        std::fs::rename(&temp, path).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            PortError(format!("couldn't save {}: {e}", path.display()))
        })
    }
}

/// Keeps the file in memory, for shells that save it themselves (the browser).
#[derive(Default)]
pub struct MemorySink {
    bytes: std::sync::Mutex<Vec<u8>>,
}

impl MemorySink {
    /// The last file written; empty before any write.
    pub fn take(&self) -> Vec<u8> {
        self.bytes.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default()
    }
}

impl DocumentSink for MemorySink {
    fn write(&self, bytes: &[u8]) -> Result<(), PortError> {
        *self.bytes.lock().map_err(|_| PortError("export buffer is unavailable".into()))? = bytes.to_vec();
        Ok(())
    }
}
