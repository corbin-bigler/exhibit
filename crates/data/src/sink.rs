use std::path::Path;

use exhibit_domain::ports::{DocumentSink, PortError};

/// Writes next to the target and renames over it, so a failed export never leaves half a file.
pub struct FileSink;

impl DocumentSink for FileSink {
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), PortError> {
        let temp = path.with_extension("pdf.partial");
        std::fs::write(&temp, bytes).map_err(|e| PortError(format!("couldn't write {}: {e}", temp.display())))?;
        std::fs::rename(&temp, path).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            PortError(format!("couldn't save {}: {e}", path.display()))
        })
    }
}
