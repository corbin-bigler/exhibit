//! Adapters behind the domain's ports: file decoding, PDF writing, previews, saving.

mod decode;
mod library;
mod render;
mod sink;
#[cfg(windows)]
mod wic;

pub use library::SourceStore;
pub use render::KrillaRenderer;
pub use sink::FileSink;
