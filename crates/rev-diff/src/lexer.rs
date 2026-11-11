use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Comment,
    String,
    Number,
    Keyword,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub bytes: Range<usize>,
    pub kind: TokenKind,
}

/// Cheap, line-local tinting, not a language grammar. Tokenize only visible
/// source, with a bounded input length. UTF-8 byte boundaries remain valid.
pub fn tokens(text: &str) -> Vec<Token> {
    let mut end = text.len().min(32_768);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = &text[..end];
    let mut tokens = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len().min(32_768) {
        let start = i;
        let byte = bytes[i];
        i += 1;
        let kind = if byte == b'/' && bytes.get(i) == Some(&b'/') || byte == b'#' {
            i = bytes.len();
            Some(TokenKind::Comment)
        } else if matches!(byte, b'"' | b'\'' | b'`') {
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == byte {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            Some(TokenKind::String)
        } else if byte.is_ascii_digit() {
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'.' | b'_'))
            {
                i += 1;
            }
            Some(TokenKind::Number)
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            matches!(
                &text[start..i],
                "fn" | "let"
                    | "mut"
                    | "pub"
                    | "use"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "trait"
                    | "match"
                    | "if"
                    | "else"
                    | "for"
                    | "while"
                    | "loop"
                    | "return"
                    | "break"
                    | "const"
                    | "function"
                    | "class"
                    | "import"
                    | "export"
                    | "from"
                    | "async"
                    | "await"
                    | "type"
                    | "interface"
                    | "new"
                    | "true"
                    | "false"
                    | "null"
                    | "None"
                    | "Some"
                    | "def"
                    | "in"
                    | "try"
                    | "catch"
                    | "throw"
                    | "self"
                    | "Self"
            )
            .then_some(TokenKind::Keyword)
        } else {
            while !text.is_char_boundary(i) {
                i += 1;
            }
            None
        };
        // An escaped multibyte scalar may have been scanned byte by byte.
        while i < bytes.len() && !text.is_char_boundary(i) {
            i += 1;
        }
        if let Some(kind) = kind {
            tokens.push(Token {
                bytes: start..i,
                kind,
            });
        }
    }
    tokens
}
