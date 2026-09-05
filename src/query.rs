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

pub(crate) fn select(candidates: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut deferred = Vec::new();
    for candidate in candidates {
        if overlaps(&candidate, &results) {
            continue;
        }
        let repeated_file = results
            .iter()
            .filter(|result: &&SearchResult| {
                result.root_id == candidate.root_id && result.path == candidate.path
            })
            .count()
            >= 2;
        if repeated_file
            || results
                .iter()
                .any(|result| near_duplicate(&candidate.snippet, &result.snippet))
        {
            deferred.push(candidate);
        } else {
            results.push(candidate);
            if results.len() == limit {
                return results;
            }
        }
    }
    for candidate in deferred {
        if !overlaps(&candidate, &results) {
            results.push(candidate);
            if results.len() == limit {
                break;
            }
        }
    }
    results
}

fn near_duplicate(left: &str, right: &str) -> bool {
    let tokens = |text: &str| -> BTreeSet<String> {
        text.split_whitespace().map(str::to_lowercase).collect()
    };
    let left = tokens(left);
    let right = tokens(right);
    let union = left.union(&right).count();
    union > 0 && left.intersection(&right).count() * 100 >= union * 85
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(root: &str, path: &str, start: usize, snippet: &str) -> SearchResult {
        SearchResult {
            id: format!("{root}-{path}-{start}"),
            root_id: root.into(),
            root_name: root.into(),
            path: path.into(),
            start_line: start + 1,
            end_line: start + 10,
            start_byte: start,
            end_byte: start + 10,
            snippet: snippet.into(),
            truncated: false,
        }
    }

    #[test]
    fn similar_excerpts_are_deferred_but_remain_available() {
        let candidates = vec![
            hit("a", "first", 0, "one two three four five six seven"),
            hit("a", "copy", 0, "one two three four five six seven eight"),
            hit("a", "other", 0, "different implementation"),
        ];
        let results = select(candidates.clone(), 2);
        assert_eq!(results[1].path, "other");
        assert_eq!(select(candidates, 3)[2].path, "copy");
        assert!(!near_duplicate("", ""));
        assert!(near_duplicate(" A  b\n", "a b"));
    }

    #[test]
    fn file_quota_is_soft_and_root_scoped() {
        let candidates = vec![
            hit("a", "same", 0, "first"),
            hit("a", "same", 20, "second"),
            hit("a", "same", 40, "third"),
            hit("b", "same", 0, "fourth"),
        ];
        let results = select(candidates.clone(), 3);
        assert_eq!(results[2].root_id, "b");
        assert_eq!(select(candidates, 4)[3].snippet, "third");
    }

    #[test]
    fn deferred_excerpts_do_not_overlap_later_selections() {
        let results = select(
            vec![
                hit("a", "first", 0, "copy"),
                hit("a", "second", 0, "copy"),
                hit("a", "second", 5, "different"),
            ],
            3,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[1].start_byte, 5);
    }
}
