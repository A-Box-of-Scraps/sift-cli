use std::fmt::Write;

use sift::{QueryResponse, SnapshotInfo};

pub fn query(response: &QueryResponse, json: bool) -> sift::Result<String> {
    if json {
        return serde_json::to_string(response)
            .map(|text| text + "\n")
            .map_err(|error| std::io::Error::other(error).into());
    }
    if response.results.is_empty() {
        return Ok("No results.\n".into());
    }
    let mut output = String::new();
    for result in &response.results {
        if !output.is_empty() {
            output.push('\n');
        }
        writeln!(
            output,
            "{}:{}:{}-{}",
            result.root_name,
            result.path.escape_default(),
            result.start_line,
            result.end_line
        )
        .unwrap();
        output.push_str(&result.snippet);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

pub fn info(info: &SnapshotInfo) -> String {
    let mut output = format!(
        "id: {}\nbackend: {}\nformat: {}\ncreated_at_unix_seconds: {}\npreprocessing: {}\nfiles: {}\nchunks: {}\n",
        info.id,
        info.backend,
        info.format_version,
        info.created_at_unix_seconds,
        info.preprocessing_config,
        info.file_count,
        info.chunk_count,
    );
    for root in &info.roots {
        writeln!(
            output,
            "root: {} ({}) {}",
            root.name,
            root.id,
            root.location.display()
        )
        .unwrap();
    }
    output
}
