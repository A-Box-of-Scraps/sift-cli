pub(crate) fn terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for token in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
    {
        let whole = token.to_lowercase();
        terms.push(whole.clone());
        for word in token.split('_').filter(|s| !s.is_empty()) {
            split_word(word, &whole, &mut terms);
        }
    }
    terms
}

fn split_word(word: &str, whole: &str, terms: &mut Vec<String>) {
    let chars: Vec<_> = word.char_indices().collect();
    let mut start = 0;
    for i in 1..chars.len() {
        let previous = chars[i - 1].1;
        let current = chars[i].1;
        let next_lower = chars.get(i + 1).is_some_and(|(_, c)| c.is_lowercase());
        if current.is_uppercase()
            && (previous.is_lowercase()
                || previous.is_numeric()
                || (previous.is_uppercase() && next_lower))
        {
            let part = word[start..chars[i].0].to_lowercase();
            if part != whole {
                terms.push(part);
            }
            start = chars[i].0;
        }
    }
    let part = word[start..].to_lowercase();
    if part != whole {
        terms.push(part);
    }
}

pub(crate) fn searchable(text: &str) -> String {
    terms(text).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_whole_identifiers_and_splits_components() {
        assert_eq!(
            terms("validateToken HTTPServer snake_case"),
            [
                "validatetoken",
                "validate",
                "token",
                "httpserver",
                "http",
                "server",
                "snake_case",
                "snake",
                "case"
            ]
        );
        assert_eq!(terms("src/auth.rs"), ["src", "auth", "rs"]);
    }
}
