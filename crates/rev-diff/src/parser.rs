use std::{fmt, sync::Arc};

use crate::{File, Hunk, Line, LineKind, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Patch line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for ParseError {}

fn error(line: usize, message: impl Into<String>) -> ParseError {
    ParseError {
        line,
        message: message.into(),
    }
}

struct Building {
    file: File,
    old_path: String,
    git_prefixes: bool,
    old: usize,
    new: usize,
    old_left: usize,
    new_left: usize,
    last_old: Option<usize>,
    last_new: Option<usize>,
}

impl Building {
    fn new(path: String, source: Arc<str>, prefixes: bool) -> Self {
        let mut file = File::placeholder(path, "No textual changes");
        file.source = source;
        Self {
            file,
            old_path: String::new(),
            git_prefixes: prefixes,
            old: 0,
            new: 0,
            old_left: 0,
            new_left: 0,
            last_old: None,
            last_new: None,
        }
    }

    fn finish(mut self, at: usize) -> Result<File, ParseError> {
        self.check_complete(at)?;
        if self.file.path.is_empty() || self.file.path == "/dev/null" {
            return Err(error(at, "Missing file path"));
        }
        self.file.index_sources();
        Ok(self.file)
    }

    fn check_complete(&self, at: usize) -> Result<(), ParseError> {
        if self.old_left != 0 || self.new_left != 0 {
            return Err(error(at, "Hunk ended before its declared line counts"));
        }
        Ok(())
    }

    fn begin_hunk(&mut self, text: &str, at: usize) -> Result<(), ParseError> {
        self.check_complete(at)?;
        let mut words = text.split_whitespace();
        if words.next() != Some("@@") {
            return Err(error(at, "Expected a two-way hunk"));
        }
        let (old, old_count) = count(words.next(), '-', at)?;
        let (new, new_count) = count(words.next(), '+', at)?;
        if words.next() != Some("@@") {
            return Err(error(at, "Invalid hunk header"));
        }
        if old_count > 0 && self.last_old.is_some_and(|last| old <= last)
            || new_count > 0 && self.last_new.is_some_and(|last| new <= last)
        {
            return Err(error(at, "Overlapping or out-of-order hunks"));
        }
        self.old = old;
        self.new = new;
        self.old_left = old_count;
        self.new_left = new_count;
        self.file.message = None;
        let start = self.file.lines.len();
        self.file.hunks.push(Hunk {
            header: text.to_string(),
            lines: start..start,
        });
        Ok(())
    }

    fn source_line(&mut self, text: &str, bytes: usize, at: usize) -> Result<(), ParseError> {
        let kind = match text.as_bytes().first() {
            Some(b' ') => LineKind::Context,
            Some(b'+') => LineKind::Added,
            Some(b'-') => LineKind::Removed,
            _ => return Err(error(at, "Expected a context, added, or removed line")),
        };
        let old = kind != LineKind::Added;
        let new = kind != LineKind::Removed;
        if old && self.old_left == 0 || new && self.new_left == 0 {
            return Err(error(
                at,
                "Hunk contains more lines than its header declares",
            ));
        }
        self.file.lines.push(Line {
            kind,
            old: old.then_some(self.old),
            new: new.then_some(self.new),
            bytes: bytes + 1..bytes + text.len(),
            newline: true,
        });
        if old {
            self.last_old = Some(self.old);
            self.old += 1;
            self.old_left -= 1;
        }
        if new {
            self.last_new = Some(self.new);
            self.new += 1;
            self.new_left -= 1;
        }
        self.file.additions += usize::from(kind == LineKind::Added);
        self.file.deletions += usize::from(kind == LineKind::Removed);
        self.file.hunks.last_mut().unwrap().lines.end = self.file.lines.len();
        Ok(())
    }
}

fn count(token: Option<&str>, sign: char, at: usize) -> Result<(usize, usize), ParseError> {
    let token = token
        .and_then(|token| token.strip_prefix(sign))
        .ok_or_else(|| error(at, "Missing hunk coordinates"))?;
    let (start, count) = token.split_once(',').unwrap_or((token, "1"));
    let start: usize = start
        .parse()
        .map_err(|_| error(at, "Invalid source line number"))?;
    let count: usize = count
        .parse()
        .map_err(|_| error(at, "Invalid hunk length"))?;
    if start == 0 && count != 0 || start.checked_add(count).is_none() {
        return Err(error(at, "Hunk coordinates out of range"));
    }
    Ok((start, count))
}

/// Decode Git's byte-oriented C quoting, including octal-encoded UTF-8 names.
fn quoted(input: &str, at: usize) -> Result<(String, usize), ParseError> {
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        match byte {
            b'"' => {
                return String::from_utf8(result)
                    .map(|path| (path, i))
                    .map_err(|_| error(at, "Non-UTF-8 file path"));
            }
            b'\\' => {
                let escaped = *bytes
                    .get(i)
                    .ok_or_else(|| error(at, "Truncated path escape"))?;
                i += 1;
                if (b'0'..=b'7').contains(&escaped) {
                    let mut value = u16::from(escaped - b'0');
                    for _ in 0..2 {
                        if let Some(&digit) =
                            bytes.get(i).filter(|digit| (b'0'..=b'7').contains(digit))
                        {
                            value = value * 8 + u16::from(digit - b'0');
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    result
                        .push(u8::try_from(value).map_err(|_| error(at, "Path escape overflow"))?);
                } else {
                    result.push(match escaped {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        b'b' => 8,
                        b'f' => 12,
                        b'v' => 11,
                        b'a' => 7,
                        b'\\' => b'\\',
                        b'"' => b'"',
                        _ => return Err(error(at, "Unknown path escape")),
                    });
                }
            }
            byte => result.push(byte),
        }
    }
    Err(error(at, "Unterminated quoted file path"))
}

fn path(text: &str, at: usize) -> Result<String, ParseError> {
    if text.starts_with('"') {
        return quoted(text, at).map(|(path, _)| path);
    }
    Ok(text.split('\t').next().unwrap_or(text).to_string())
}

fn git_paths(text: &str, at: usize) -> Result<(String, String), ParseError> {
    if text.starts_with('"') {
        let (first, end) = quoted(text, at)?;
        return Ok((first, path(text[end..].trim_start(), at)?));
    }
    // Git does not quote ordinary spaces. The paired default prefixes disambiguate them.
    let (first, second) = text
        .split_once(" b/")
        .map(|(a, b)| (a.to_string(), format!("b/{b}")))
        .or_else(|| {
            text.split_once(" \"")
                .map(|(a, b)| (a.to_string(), format!("\"{b}")))
        })
        .or_else(|| {
            text.split_once(' ')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        })
        .ok_or_else(|| error(at, "Missing paired Git file paths"))?;
    Ok((first, path(&second, at)?))
}

pub(crate) fn parse(patch: &str) -> Result<Vec<File>, ParseError> {
    let source: Arc<str> = Arc::from(patch);
    let mut files = Vec::new();
    let mut current: Option<Building> = None;
    let mut offset = 0;
    let mut last_line = 0;
    for (index, raw) in patch.split_inclusive('\n').enumerate() {
        let at = index + 1;
        last_line = at;
        let text = raw.strip_suffix('\n').unwrap_or(raw);
        let metadata = text.trim_end_matches('\r');
        let bytes = offset;
        offset += raw.len();

        if metadata == "\\ No newline at end of file" {
            let line = current
                .as_mut()
                .and_then(|file| file.file.lines.last_mut())
                .ok_or_else(|| error(at, "Newline marker has no source line"))?;
            line.newline = false;
            continue;
        }
        if let Some(file) = &mut current
            && (file.old_left > 0 || file.new_left > 0)
        {
            file.source_line(text, bytes, at)?;
            continue;
        }
        if let Some(names) = metadata.strip_prefix("diff --git ") {
            if let Some(file) = current.take() {
                files.push(file.finish(at)?);
            }
            let (old, new) = git_paths(names, at)?;
            let prefixes = old.starts_with("a/") && new.starts_with("b/");
            let name = if prefixes { new[2..].to_string() } else { new };
            current = Some(Building::new(name, source.clone(), prefixes));
        } else if metadata.starts_with("diff --cc ")
            || metadata.starts_with("diff --combined ")
            || metadata.starts_with("@@@")
        {
            return Err(error(
                at,
                "Combined merge diffs are not supported; request a two-way Git diff",
            ));
        } else if let Some(name) = metadata.strip_prefix("--- ") {
            if current
                .as_ref()
                .is_some_and(|file| !file.file.hunks.is_empty())
            {
                files.push(current.take().unwrap().finish(at)?);
            }
            let file =
                current.get_or_insert_with(|| Building::new(String::new(), source.clone(), false));
            file.old_path = path(name, at)?;
        } else if let Some(file) = &mut current {
            if let Some(name) = metadata.strip_prefix("+++ ") {
                let new = path(name, at)?;
                let prefixed =
                    file.git_prefixes || file.old_path.starts_with("a/") && new.starts_with("b/");
                if file.old_path == "/dev/null" {
                    file.file.status = Status::Added;
                }
                if new == "/dev/null" {
                    file.file.status = Status::Deleted;
                }
                let chosen = if new == "/dev/null" {
                    &file.old_path
                } else {
                    &new
                };
                file.file.path =
                    if prefixed && (chosen.starts_with("a/") || chosen.starts_with("b/")) {
                        chosen[2..].to_string()
                    } else {
                        chosen.clone()
                    };
            } else if metadata.starts_with("@@ ") {
                file.begin_hunk(metadata, at)?;
            } else if let Some(name) = metadata.strip_prefix("rename to ") {
                file.file.path = path(name, at)?;
                file.file.status = Status::Renamed;
            } else if let Some(name) = metadata.strip_prefix("copy to ") {
                file.file.path = path(name, at)?;
                file.file.status = Status::Copied;
            } else if metadata.starts_with("new file mode ") {
                file.file.status = Status::Added;
            } else if metadata.starts_with("deleted file mode ") {
                file.file.status = Status::Deleted;
            } else if metadata.starts_with("Binary files ") || metadata == "GIT binary patch" {
                file.file.status = Status::Binary;
                file.file.message = Some("Binary file changed".into());
            } else if !file.file.hunks.is_empty()
                && matches!(text.as_bytes().first(), Some(b'+' | b'-' | b' '))
                && metadata != "-- "
            {
                return Err(error(at, "Unexpected source outside a hunk"));
            }
        }
    }
    if let Some(file) = current {
        files.push(file.finish(last_line + 1)?);
    }
    if files.is_empty() && !patch.trim().is_empty() {
        return Err(error(1, "No unified diff found"));
    }
    Ok(files)
}
