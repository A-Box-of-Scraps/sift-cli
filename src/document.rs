use std::path::PathBuf;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RootInfo {
    pub id: String,
    pub name: String,
    pub location: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub root_id: String,
    pub path: String,
    pub content_hash: String,
    pub text: String,
}
