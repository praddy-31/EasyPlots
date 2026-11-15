//! One error type for everything the owner might see, with an optional pointer into the
//! command text so the launcher and terminal can underline the problem.

use std::fmt;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub message: String,
    /// Byte range in the command string that the error is about, if any.
    pub span: Option<Range<usize>>,
    /// A ready fix: replace this byte range of the command with this text (⇥ in the
    /// launcher puts it in).
    pub fix: Option<(Range<usize>, String)>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            span: None,
            fix: None,
        }
    }

    pub fn at(message: impl Into<String>, span: Range<usize>) -> Self {
        Error {
            message: message.into(),
            span: Some(span),
            fix: None,
        }
    }

    /// Attach a span unless one is already set.
    pub fn or_at(mut self, span: Range<usize>) -> Self {
        self.span.get_or_insert(span);
        self
    }

    /// Format the error with the command and a `^^^` marker under the problem.
    pub fn display_with(&self, command: &str) -> String {
        let Some(span) = &self.span else {
            return self.message.clone();
        };
        let start = command[..span.start.min(command.len())].chars().count();
        let len = command[span.start.min(command.len())..span.end.min(command.len())]
            .chars()
            .count()
            .max(1);
        format!(
            "{}\n  {}\n  {}{}",
            self.message,
            command,
            " ".repeat(start),
            "^".repeat(len)
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// The closest candidate to `word` (case-insensitive), if it's close enough to be a likely typo.
pub fn did_you_mean<'a>(
    word: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let word = word.to_lowercase();
    let limit = (word.chars().count() / 3).max(1) + usize::from(word.chars().count() > 6);
    candidates
        .into_iter()
        .map(|c| (edit_distance(&word, &c.to_lowercase()), c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// " — did you mean "x"?" or an empty string.
pub fn suggestion<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    did_you_mean(word, candidates)
        .map(|c| format!(" — did you mean \"{c}\"?"))
        .unwrap_or_default()
}

/// Edit distance where swapping two neighbouring letters counts as one typo
/// (optimal string alignment), so "tmep" is one step from "temp".
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_close_matches_only() {
        assert_eq!(
            did_you_mean("tmep", ["time", "temp", "pressure"]),
            Some("temp")
        );
        assert_eq!(
            did_you_mean("Pressur", ["time", "pressure"]),
            Some("pressure")
        );
        assert_eq!(did_you_mean("xyz", ["time", "temp"]), None);
    }

    #[test]
    fn caret_points_at_span() {
        let e = Error::at("bad", 5..9);
        let shown = e.display_with("line tmep");
        assert!(shown.ends_with("       ^^^^"), "{shown}");
    }
}
