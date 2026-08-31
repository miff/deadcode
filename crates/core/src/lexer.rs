use std::collections::HashMap;

/// Split source into (code with comments and string literals removed,
/// the extracted string-literal contents).
///
/// Newlines inside removed regions are preserved so line numbers computed on
/// the original text stay valid; the stripped text is only used for counting.
pub fn split_code_and_strings(src: &str) -> (String, String) {
    let b = src.as_bytes();
    let mut code = String::with_capacity(src.len());
    let mut strings = String::new();
    let mut i = 0;

    while i < b.len() {
        // line comment
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // block comment (Swift allows nesting)
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let mut depth = 1;
            i += 2;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    if b[i] == b'\n' {
                        code.push('\n');
                    }
                    i += 1;
                }
            }
            continue;
        }
        // multiline / raw string
        if b[i] == b'"' && i + 2 < b.len() && b[i + 1] == b'"' && b[i + 2] == b'"' {
            i += 3;
            let start = i;
            while i + 2 < b.len() && !(b[i] == b'"' && b[i + 1] == b'"' && b[i + 2] == b'"') {
                if b[i] == b'\n' {
                    code.push('\n');
                }
                i += 1;
            }
            push_slice(&mut strings, src, start, i);
            i = (i + 3).min(b.len());
            continue;
        }
        // ordinary string literal
        if b[i] == b'"' {
            i += 1;
            let start = i;
            while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            push_slice(&mut strings, src, start, i);
            i = (i + 1).min(b.len());
            continue;
        }

        let n = utf8_len(b[i]);
        let end = (i + n).min(src.len());
        if src.is_char_boundary(i) && src.is_char_boundary(end) {
            code.push_str(&src[i..end]);
        }
        i = end.max(i + 1);
    }

    (code, strings)
}

fn push_slice(out: &mut String, src: &str, start: usize, end: usize) {
    let (s, e) = (start.min(src.len()), end.min(src.len()));
    if s <= e && src.is_char_boundary(s) && src.is_char_boundary(e) {
        out.push_str(&src[s..e]);
        out.push('\n');
    }
}

pub fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else if first >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

/// Count every identifier-like token in `text` into `map`.
///
/// One pass over the corpus, so scan cost is linear in total bytes rather
/// than (declarations x bytes).
pub fn tally(text: &str, map: &mut HashMap<String, usize>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'_' || b[i].is_ascii_alphabetic() {
            let start = i;
            while i < b.len() && (b[i] == b'_' || b[i].is_ascii_alphanumeric()) {
                i += 1;
            }
            *map.entry(text[start..i].to_string()).or_insert(0) += 1;
        } else {
            i += utf8_len(b[i]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_line_comments() {
        let (code, _) = split_code_and_strings("let a = 1 // mentions ghostName\nlet b = 2");
        assert!(!code.contains("ghostName"));
        assert!(code.contains("let b"));
    }

    #[test]
    fn strips_nested_block_comments() {
        let (code, _) = split_code_and_strings("a /* x /* y */ z */ b");
        assert!(!code.contains('x'));
        assert!(code.contains('a') && code.contains('b'));
    }

    #[test]
    fn captures_string_contents_separately() {
        let (code, strings) = split_code_and_strings(r#"let k = "LegacyViewController""#);
        assert!(!code.contains("LegacyViewController"));
        assert!(strings.contains("LegacyViewController"));
    }

    #[test]
    fn handles_escaped_quotes() {
        let (code, _) = split_code_and_strings(r#"let s = "a\"b"; let after = 1"#);
        assert!(code.contains("after"));
    }

    #[test]
    fn tally_counts_whole_identifiers() {
        let mut m = HashMap::new();
        tally("foo foobar foo_bar foo", &mut m);
        assert_eq!(m.get("foo"), Some(&2));
        assert_eq!(m.get("foobar"), Some(&1));
    }

    #[test]
    fn survives_non_ascii() {
        let (code, _) = split_code_and_strings("let naziv = \"Niš\"\nlet drugi = 1");
        assert!(code.contains("drugi"));
    }
}
