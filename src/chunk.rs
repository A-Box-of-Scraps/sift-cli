pub const MAX_CHUNK_BYTES: usize = 2048;
const WINDOW_LINES: usize = 32;
const OVERLAP_LINES: usize = 4;
pub(crate) const PREPROCESSING_CONFIG: &str = "lines=32;overlap=4;max_bytes=2048;tokens=code-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: &'a str,
}

pub fn chunk_text(text: &str) -> Vec<Chunk<'_>> {
    let mut line_starts: Vec<usize> = vec![0];
    line_starts.extend(
        text.match_indices('\n')
            .map(|(offset, _)| offset + 1)
            .filter(|offset| *offset < text.len()),
    );
    let mut chunks: Vec<Chunk<'_>> = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let first_line = line_starts.partition_point(|offset| *offset <= start) - 1;
        let line_end = line_starts
            .get(first_line + WINDOW_LINES)
            .copied()
            .unwrap_or(text.len());
        let mut end = line_end.min(start + MAX_CHUNK_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let last_line = line_starts.partition_point(|offset| *offset < end);
        chunks.push(Chunk {
            start_line: first_line + 1,
            end_line: last_line,
            start_byte: start,
            end_byte: end,
            text: &text[start..end],
        });
        if end == text.len() {
            break;
        }
        let overlap = line_starts
            .get(last_line.saturating_sub(OVERLAP_LINES))
            .copied()
            .unwrap_or(end);
        start = if overlap > start && overlap < end {
            overlap
        } else {
            end
        };
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_coverage(text: &str) {
        let chunks: Vec<Chunk<'_>> = chunk_text(text);
        let mut covered = 0;
        for chunk in &chunks {
            assert!(chunk.start_byte <= covered);
            assert!(chunk.end_byte > covered);
            assert_eq!(chunk.text, &text[chunk.start_byte..chunk.end_byte]);
            assert!(chunk.text.len() <= MAX_CHUNK_BYTES);
            assert_eq!(
                chunk.start_line,
                text[..chunk.start_byte]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1
            );
            assert_eq!(
                chunk.end_line,
                text.as_bytes()[..chunk.end_byte - 1]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
                    + 1
            );
            covered = chunk.end_byte;
        }
        assert_eq!(covered, text.len());
    }

    #[test]
    fn exact_offsets_cover_unicode_crlf_and_long_lines() {
        for text in [
            String::new(),
            "a\r\nb\r\n".into(),
            "\n".repeat(100),
            "\u{1f980}".repeat(3000),
            "x\n".repeat(100),
            format!("{}\nend", "x".repeat(3000)),
        ] {
            assert_coverage(&text);
        }
    }

    #[test]
    fn line_windows_overlap_without_extra_terminal_chunk() {
        let text: String = "line\n".repeat(60);
        let chunks: Vec<Chunk<'_>> = chunk_text(&text);
        assert_eq!(chunks.len(), 2);
        assert_eq!((chunks[0].start_line, chunks[0].end_line), (1, 32));
        assert_eq!((chunks[1].start_line, chunks[1].end_line), (29, 60));
        assert!(chunk_text("").is_empty());
    }
}
