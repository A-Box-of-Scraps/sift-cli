use std::{collections::BTreeSet, path::PathBuf};

use serde::Serialize;

use crate::{Error, Result, tokenize};

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub text: String,
    pub limit: usize,
    pub path: Option<String>,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: 5,
            path: None,
        }
    }

    pub(crate) fn prepare(&self) -> Result<PreparedQuery> {
        if !(1..=100).contains(&self.limit) {
            return Err(Error::InvalidOptions(
                "limit must be between 1 and 100".into(),
            ));
        }
        if self.text.len() > 4096 {
            return Err(Error::InvalidOptions("query exceeds 4096 bytes".into()));
        }
        let terms: BTreeSet<_> = tokenize::terms(&self.text)
            .into_iter()
            .filter(|term| term.chars().any(char::is_alphanumeric))
            .collect();
        if terms.is_empty() || terms.len() > 64 {
            return Err(Error::InvalidOptions(
                "query must contain between 1 and 64 searchable terms".into(),
            ));
        }
        let path = self
            .path
            .as_deref()
            .map(normalize_path)
            .transpose()?
            .flatten();
        Ok(PreparedQuery {
            terms: terms.into_iter().collect(),
            limit: self.limit,
            path,
        })
    }
}

fn normalize_path(path: &str) -> Result<Option<String>> {
    if path.starts_with('/') || path.contains('\0') || path.split('/').any(|part| part == "..") {
        return Err(Error::InvalidOptions(
            "path filter must be root-relative without parent traversal".into(),
        ));
    }
    let normalized = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    Ok((!normalized.is_empty()).then_some(normalized))
}

pub(crate) struct PreparedQuery {
    pub terms: Vec<String>,
    pub limit: usize,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub root_id: String,
    pub root_name: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub snippet: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryResponse {
    pub schema_version: u32,
    pub handle: PathBuf,
    pub results: Vec<SearchResult>,
}

pub(crate) fn overlaps(candidate: &SearchResult, results: &[SearchResult]) -> bool {
    results.iter().any(|result| {
        result.root_id == candidate.root_id
            && result.path == candidate.path
            && result.start_byte < candidate.end_byte
            && candidate.start_byte < result.end_byte
    })
}
