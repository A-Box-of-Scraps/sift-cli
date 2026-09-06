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
    let mut output: String = String::new();
    for result in &response.results {
        if !output.is_empty() {
            output.push('\n');
        }
        writeln!(
            output,
            "{}:{}:{}-{}",
            result.root_name.escape_default(),
            result.path.escape_default(),
            result.start_line,
            result.end_line
        )
        .unwrap();
        output.push_str(&terminal_text(&result.snippet, true));
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

pub fn info(info: &SnapshotInfo) -> String {
    let mut output: String = format!(
        "id: {}\nbackend: {}\nformat: {}\ncreated_at_unix_seconds: {}\npreprocessing: {}\nfiles: {}\nchunks: {}\n",
        info.id.escape_default(),
        info.backend.escape_default(),
        info.format_version,
        info.created_at_unix_seconds,
        info.preprocessing_config.escape_default(),
        info.file_count,
        info.chunk_count,
    );
    for root in &info.roots {
        writeln!(
            output,
            "root: {} ({}) {}",
            root.name.escape_default(),
            root.id.escape_default(),
            root.location.to_string_lossy().escape_default()
        )
        .unwrap();
    }
    output
}

pub fn terminal_text(text: &str, multiline: bool) -> String {
    let mut output: String = String::new();
    for character in text.chars() {
        if multiline && matches!(character, '\n' | '\t') {
            output.push(character);
        } else {
            output.extend(character.escape_debug());
        }
    }
    output
}

pub fn handle(path: &std::path::Path) -> sift::Result<()> {
    use std::io::{IsTerminal, Write};
    let mut stdout: std::io::StdoutLock<'_> = std::io::stdout().lock();
    if stdout.is_terminal() {
        write!(stdout, "{}", path.to_string_lossy().escape_default())?;
    } else {
        stdout.write_all(path.as_os_str().as_encoded_bytes())?;
    }
    stdout.write_all(b"\n")?;
    Ok(())
}
