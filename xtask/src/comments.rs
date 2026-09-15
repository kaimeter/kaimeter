//! Comment extraction and the repository comment policy.

use std::path::Path;

/// Comment marker syntax recognised by the scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Syntax {
    /// Rust line and block comments.
    Rust,
    /// Hash comments used by TOML, shell, PowerShell and YAML.
    Hash,
}

/// Returns the comment syntax for files covered by the policy.
///
/// Prose and data files (Markdown, JSON, CFF) carry no code comments and
/// are out of scope.
pub(crate) fn syntax_for(path: &Path) -> Option<Syntax> {
    if path.starts_with(".githooks") {
        return Some(Syntax::Hash);
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => Some(Syntax::Rust),
        Some("toml" | "sh" | "bash" | "ps1" | "yml" | "yaml") => Some(Syntax::Hash),
        _ => None,
    }
}

/// Policy violation found in one comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Violation {
    /// A forbidden marker token was used.
    Marker(&'static str),
    /// The comment restates code instead of explaining it.
    CommentedOutCode,
}

impl Violation {
    /// Human-readable description used in reports.
    pub(crate) fn describe(self) -> String {
        match self {
            Self::Marker(marker) => format!("forbidden marker `{marker}`"),
            Self::CommentedOutCode => "commented-out code".to_owned(),
        }
    }
}

/// Marker tokens rejected by the policy.
const FORBIDDEN_MARKERS: [&str; 4] = ["TODO", "FIXME", "HACK", "XXX"];

/// Structured annotations sanctioned by the whitepaper for rule code.
const ALLOWED_ANNOTATIONS: [&str; 3] = ["@legal", "@source", "@since"];

/// Keywords that can start a Rust item declaration.
const ITEM_KEYWORDS: [&str; 9] = [
    "fn", "pub", "impl", "struct", "enum", "trait", "unsafe", "async", "extern",
];

/// Classifies one comment, returning the first violation found.
pub(crate) fn violation(text: &str) -> Option<Violation> {
    if let Some(marker) = forbidden_marker(text) {
        return Some(Violation::Marker(marker));
    }
    if has_annotation(text) {
        return None;
    }
    if looks_like_code(normalize(text)) {
        return Some(Violation::CommentedOutCode);
    }
    None
}

fn forbidden_marker(text: &str) -> Option<&'static str> {
    FORBIDDEN_MARKERS
        .into_iter()
        .find(|marker| contains_word(text, marker))
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut search_start = 0;
    while let Some(offset) = haystack[search_start..].find(needle) {
        let start = search_start + offset;
        let end = start + needle.len();
        let left_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let right_ok = end == bytes.len() || !is_word_byte(bytes[end]);
        if left_ok && right_ok {
            return true;
        }
        search_start = start + 1;
    }
    false
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn has_annotation(text: &str) -> bool {
    ALLOWED_ANNOTATIONS
        .into_iter()
        .any(|annotation| text.contains(annotation))
}

fn normalize(text: &str) -> &str {
    text.trim().trim_start_matches(['/', '!', '*']).trim()
}

fn looks_like_code(text: &str) -> bool {
    ends_statement(text)
        || text.starts_with("#[")
        || starts_call(text)
        || starts_assignment(text)
        || starts_item(text)
}

fn ends_statement(text: &str) -> bool {
    text.ends_with(';') || text.ends_with('{') || text.ends_with('}') || text.ends_with("=>")
}

fn starts_call(text: &str) -> bool {
    let Some(mut rest) = consume_ident(text) else {
        return false;
    };
    rest = rest.trim_start();
    while let Some(after) = rest.strip_prefix("::") {
        let Some(next) = consume_ident(after) else {
            return false;
        };
        rest = next.trim_start();
    }
    rest.starts_with('(')
}

fn starts_assignment(text: &str) -> bool {
    let Some(rest) = consume_ident(text) else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

fn starts_item(text: &str) -> bool {
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    ITEM_KEYWORDS.contains(&first) && has_code_punctuation(text)
}

fn has_code_punctuation(text: &str) -> bool {
    text.contains('(') || text.contains('=') || text.contains('{') || text.contains("::")
}

fn consume_ident(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let end = text
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(text.len());
    Some(&text[end..])
}

/// Stateful comment extractor over the lines of one file.
#[derive(Debug)]
pub(crate) struct Scanner {
    syntax: Syntax,
    block_depth: usize,
    in_fence: bool,
}

impl Scanner {
    /// Creates a scanner for the given comment syntax.
    pub(crate) fn new(syntax: Syntax) -> Self {
        Self {
            syntax,
            block_depth: 0,
            in_fence: false,
        }
    }

    /// Returns the comment text on one line, if the line carries one.
    ///
    /// Lines inside fenced code blocks of documentation return `None`, so
    /// that doctest snippets are not mistaken for commented-out code.
    pub(crate) fn comment(&mut self, line: &str) -> Option<String> {
        let text = match self.syntax {
            Syntax::Rust => self.rust_text(line),
            Syntax::Hash => hash_text(line),
        }?;
        if self.syntax == Syntax::Rust && self.fence(&text) {
            return None;
        }
        Some(text)
    }

    fn fence(&mut self, text: &str) -> bool {
        if normalize(text).starts_with("```") {
            self.in_fence = !self.in_fence;
            return true;
        }
        self.in_fence
    }

    fn rust_text(&mut self, line: &str) -> Option<String> {
        let mut fragments = String::new();
        self.consume_line(&mut fragments, line);
        non_empty(fragments)
    }

    /// Consumes comment fragments and markers until the line is exhausted.
    fn consume_line(&mut self, fragments: &mut String, rest: &str) {
        if self.block_depth > 0 {
            self.consume_block(fragments, rest);
            return;
        }
        let Some((index, marker)) = find_comment_start(rest) else {
            return;
        };
        match marker {
            Marker::Line => fragments.push_str(&rest[index + 2..]),
            Marker::Block => {
                self.block_depth = 1;
                self.consume_line(fragments, &rest[index + 2..]);
            }
        }
    }

    /// Consumes one block comment fragment and continues past it.
    fn consume_block(&mut self, fragments: &mut String, rest: &str) {
        let Some(remainder) = self.block_fragment(fragments, rest) else {
            return;
        };
        self.consume_line(fragments, remainder);
    }

    /// Consumes block comment content, returning the unconsumed remainder.
    ///
    /// Returns `None` when the line ends inside the comment.
    fn block_fragment<'a>(&mut self, fragments: &mut String, rest: &'a str) -> Option<&'a str> {
        let open = rest.find("/*");
        let close = rest.find("*/");
        match (open, close) {
            (Some(open), Some(close)) if open < close => {
                fragments.push_str(&rest[..open]);
                self.block_depth += 1;
                Some(&rest[open + 2..])
            }
            (_, Some(close)) => {
                fragments.push_str(&rest[..close]);
                self.block_depth -= 1;
                Some(&rest[close + 2..])
            }
            _ => {
                fragments.push_str(rest);
                self.block_depth += rest.matches("/*").count();
                None
            }
        }
    }
}

/// Comment marker kinds recognised in Rust source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    /// `//`
    Line,
    /// `/*`
    Block,
}

fn find_comment_start(text: &str) -> Option<(usize, Marker)> {
    let bytes = text.as_bytes();
    let mut in_string = false;
    let mut raw_hashes: Option<usize> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(hashes) = raw_hashes {
            index = raw_step(bytes, index, hashes, &mut raw_hashes);
        } else if in_string {
            index = string_step(byte, &mut in_string, index);
        } else if let Some((hashes, next)) = raw_start(bytes, index) {
            raw_hashes = Some(hashes);
            index = next;
        } else if byte == b'"' {
            in_string = true;
            index += 1;
        } else if let Some(marker) = slash_marker(bytes, index) {
            return Some((index, marker));
        } else {
            index += 1;
        }
    }
    None
}

/// Detects a Rust raw string opener such as `r"`, `r#"` or `br"`.
///
/// Returns the number of opening hashes and the index just past the quote.
fn raw_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let mut hashes = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'"') {
        return Some((hashes, cursor + 1));
    }
    None
}

/// Advances through a raw string, clearing the state at its terminator.
fn raw_step(bytes: &[u8], index: usize, hashes: usize, raw: &mut Option<usize>) -> usize {
    if bytes[index] != b'"' {
        return index + 1;
    }
    let end = index + 1 + hashes;
    if end <= bytes.len() && bytes[index + 1..end].iter().all(|byte| *byte == b'#') {
        *raw = None;
        return end;
    }
    index + 1
}

/// Advances one byte inside a string literal, honouring backslash escapes.
fn string_step(byte: u8, in_string: &mut bool, index: usize) -> usize {
    if byte == b'\\' {
        return index + 2;
    }
    if byte == b'"' {
        *in_string = false;
    }
    index + 1
}

/// Classifies the two bytes at `index` as a comment opener, if they are one.
fn slash_marker(bytes: &[u8], index: usize) -> Option<Marker> {
    if bytes[index] != b'/' {
        return None;
    }
    match bytes.get(index + 1) {
        Some(b'/') => Some(Marker::Line),
        Some(b'*') => Some(Marker::Block),
        _ => None,
    }
}

fn hash_text(line: &str) -> Option<String> {
    if line.starts_with("#!") {
        return None;
    }
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote.is_some() {
            index = quote_step(byte, &mut quote, index);
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
        } else if byte == b'#' {
            return non_empty(line[index + 1..].to_owned());
        } else {
            index += 1;
        }
    }
    None
}

/// Advances one byte inside a quoted value, honouring backslash escapes.
fn quote_step(byte: u8, quote: &mut Option<u8>, index: usize) -> usize {
    if byte == b'\\' {
        return index + 2;
    }
    if *quote == Some(byte) {
        *quote = None;
    }
    index + 1
}

fn non_empty(text: String) -> Option<String> {
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_comment(line: &str) -> Option<String> {
        Scanner::new(Syntax::Rust).comment(line)
    }

    #[test]
    fn extracts_line_comment_after_code() {
        assert_eq!(rust_comment("let x = 1; // why").as_deref(), Some(" why"));
    }

    #[test]
    fn ignores_double_slash_inside_string_literal() {
        assert_eq!(rust_comment(r#"let url = "http://example.com";"#), None);
    }

    #[test]
    fn extracts_comment_after_string_literal() {
        let line = r#"let url = "http://example.com"; // note"#;
        assert_eq!(rust_comment(line).as_deref(), Some(" note"));
    }

    #[test]
    fn finds_comment_after_raw_string() {
        let line = r##"let pattern = r#"a // b"#; // note"##;
        assert_eq!(rust_comment(line).as_deref(), Some(" note"));
    }

    #[test]
    fn ignores_markers_inside_raw_strings() {
        let line = r##"let pattern = r#"a // b"#;"##;
        assert_eq!(rust_comment(line), None);
    }

    #[test]
    fn extracts_block_comment_spanning_lines() {
        let mut scanner = Scanner::new(Syntax::Rust);
        assert_eq!(scanner.comment("/* first").as_deref(), Some(" first"));
        assert_eq!(scanner.comment("second */").as_deref(), Some("second "));
        assert_eq!(scanner.comment("let x = 1;"), None);
    }

    #[test]
    fn nested_block_comments_return_to_code() {
        let mut scanner = Scanner::new(Syntax::Rust);
        assert!(scanner.comment("/* outer /* inner */").is_some());
        assert!(scanner.comment("still commented */").is_some());
        assert_eq!(scanner.comment("let value = 1;"), None);
    }

    #[test]
    fn fenced_documentation_is_exempt() {
        let mut scanner = Scanner::new(Syntax::Rust);
        assert_eq!(scanner.comment("/// ```"), None);
        assert_eq!(scanner.comment("/// let x = value;"), None);
        assert_eq!(scanner.comment("/// ```"), None);
        let after = scanner.comment("/// let x = value;").unwrap();
        assert_eq!(after, "/ let x = value;");
        assert_eq!(violation(&after), Some(Violation::CommentedOutCode));
    }

    #[test]
    fn hash_syntax_extracts_comment() {
        let mut scanner = Scanner::new(Syntax::Hash);
        assert_eq!(
            scanner.comment("edition = 2024 # why").as_deref(),
            Some(" why")
        );
    }

    #[test]
    fn hash_syntax_ignores_hash_inside_quotes() {
        let mut scanner = Scanner::new(Syntax::Hash);
        assert_eq!(
            scanner.comment(r#"key = "a # b" # why"#).as_deref(),
            Some(" why")
        );
    }

    #[test]
    fn hash_syntax_skips_shebang() {
        let mut scanner = Scanner::new(Syntax::Hash);
        assert_eq!(scanner.comment("#!/bin/sh"), None);
    }

    #[test]
    fn scope_covers_code_and_configuration() {
        assert_eq!(
            syntax_for(Path::new("xtask/src/main.rs")),
            Some(Syntax::Rust)
        );
        assert_eq!(syntax_for(Path::new("Cargo.toml")), Some(Syntax::Hash));
        assert_eq!(
            syntax_for(Path::new(".githooks/pre-commit")),
            Some(Syntax::Hash)
        );
        assert_eq!(syntax_for(Path::new("paper/build.ps1")), Some(Syntax::Hash));
        assert_eq!(
            syntax_for(Path::new(".github/workflows/ci.yml")),
            Some(Syntax::Hash)
        );
        assert_eq!(syntax_for(Path::new("paper/whitepaper.md")), None);
        assert_eq!(syntax_for(Path::new("CITATION.cff")), None);
    }

    #[test]
    fn flags_forbidden_markers() {
        assert_eq!(violation(" TODO: later"), Some(Violation::Marker("TODO")));
        assert_eq!(violation(" FIXME this"), Some(Violation::Marker("FIXME")));
        assert_eq!(
            violation(" HACK around it"),
            Some(Violation::Marker("HACK"))
        );
        assert_eq!(
            violation(" XXX placeholder"),
            Some(Violation::Marker("XXX"))
        );
    }

    #[test]
    fn markers_inside_words_are_not_flagged() {
        assert_eq!(violation(" TODOS are not markers"), None);
        assert_eq!(violation(" the HACKATHON is next year"), None);
    }

    #[test]
    fn annotations_are_exempt_from_code_detection() {
        assert_eq!(
            violation(" @legal IR (EU) 2025/2547, Annex II, B.7.1"),
            None
        );
        assert_eq!(
            violation(" @source http://data.europa.eu/eli/reg_impl/2025/2547/oj"),
            None
        );
        assert_eq!(violation(" @since bundle 2026.1.0"), None);
    }

    #[test]
    fn annotations_do_not_excuse_markers() {
        assert_eq!(
            violation(" @legal TODO reference"),
            Some(Violation::Marker("TODO"))
        );
    }

    #[test]
    fn flags_commented_out_code() {
        for text in [
            " let value = 1;",
            " x = compute(input)",
            " return result;",
            " if ready {",
            " }",
            " use crate::thing;",
            " fn apply() {",
            " pub fn apply() {}",
            " #[derive(Debug)]",
            r#" println!("done");"#,
        ] {
            assert_eq!(violation(text), Some(Violation::CommentedOutCode), "{text}");
        }
    }

    #[test]
    fn prose_is_not_flagged() {
        for text in [
            " explain why the glob cannot match",
            " CF4 emissions from anode effects, slope method.",
            " if the crate is absent (v0.1), the table still applies; inert",
            " use the default value from the corrected annex",
            " the Commission applies a mark-up of 1.0 (see Art. 4(2)).",
            " see crate::rules for the mapping table",
        ] {
            assert_eq!(violation(text), None, "{text}");
        }
    }
}
