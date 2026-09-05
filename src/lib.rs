mod backend;
mod error;
mod paths;
mod snapshot;

pub use error::{Error, Result};
pub use paths::data_directory;
pub use snapshot::{SnapshotHandle, SnapshotInfo, SnapshotStore};
