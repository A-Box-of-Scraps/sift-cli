mod backend;
mod chunk;
mod document;
mod error;
mod input;
mod paths;
mod query;
mod snapshot;
mod tokenize;

pub use chunk::{Chunk, MAX_CHUNK_BYTES, chunk_text};
pub use document::{Document, RootInfo, TextDocument};
pub use error::{Error, Result};
pub use input::{DiscoveryOptions, IgnoreMode, IndexRequest, MAX_FILE_BYTES};
pub use paths::data_directory;
pub use query::{QueryResponse, SearchQuery, SearchResult};
pub use snapshot::{SnapshotHandle, SnapshotInfo, SnapshotStore};
