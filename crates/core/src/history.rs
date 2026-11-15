//! Command history and presets for the launcher, stored in
//! `~/.config/easyplots/history.toml` (next to `config.toml`).
//!
//! History: plain command strings, newest first. Presets: named commands to reuse with
//! another file ("same plot, different file"): `preset NAME FILE` draws the preset's
//! command with FILE in place of its first file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default, rename = "preset")]
    presets: Vec<Preset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub command: String,
}

#[derive(Debug)]
pub struct History {
    path: PathBuf,
    cap: usize,
    commands: Vec<String>,
    presets: Vec<Preset>,
}

impl History {
    /// Load the history file; a missing or unreadable file starts an empty history.
    pub fn load(path: &Path, cap: usize) -> History {
        let file = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str::<File>(&t).ok())
            .unwrap_or_default();
        let mut h = History {
            path: path.to_path_buf(),
            cap: cap.max(1),
            commands: file.commands,
            presets: file.presets,
        };
        h.commands.truncate(h.cap);
        h
    }

    pub fn commands(&self) -> &[String] {
        &self.commands
    }

    pub fn presets(&self) -> &[Preset] {
        &self.presets
    }

    pub fn preset(&self, name: &str) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Save `command` as a preset called `name` (replacing one of that name). The command
    /// must be valid; names are one word so `preset NAME FILE` can be typed.
    pub fn save_preset(&mut self, name: &str, command: &str) -> Result<(), Error> {
        let name = name.trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(Error::new("a preset name is one word, e.g. thesis-line"));
        }
        crate::command::parse(command)?;
        self.presets.retain(|p| !p.name.eq_ignore_ascii_case(name));
        self.presets.push(Preset {
            name: name.to_owned(),
            command: command.trim().to_owned(),
        });
        self.presets.sort_by_key(|p| p.name.to_ascii_lowercase());
        self.save()
    }

    pub fn delete_preset(&mut self, name: &str) -> Result<(), Error> {
        self.presets.retain(|p| !p.name.eq_ignore_ascii_case(name));
        self.save()
    }

    /// The files of the latest commands, newest first, each once, as typed (`run_a.csv`,
    /// `lab/run 3.csv`). Commands that no longer parse are skipped.
    pub fn recent_files(&self, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.commands {
            let Ok(spec) = crate::command::parse(c) else {
                continue;
            };
            for l in spec.panels.iter().flat_map(|p| &p.layers) {
                if !out.contains(&l.file.value) {
                    out.push(l.file.value.clone());
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        out
    }

    /// Add a command to the front (moving it there if it was already listed) and save.
    pub fn push(&mut self, command: &str) -> Result<(), Error> {
        let command = command.trim();
        if command.is_empty() {
            return Ok(());
        }
        self.commands.retain(|c| c != command);
        self.commands.insert(0, command.to_owned());
        self.commands.truncate(self.cap);
        self.save()
    }

    fn save(&self) -> Result<(), Error> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::new(format!("cannot create {}: {e}", dir.display())))?;
        }
        let text = toml::to_string(&File {
            commands: self.commands.clone(),
            presets: self.presets.clone(),
        })
        .map_err(|e| Error::new(e.to_string()))?;
        std::fs::write(
            &self.path,
            format!("# EasyPlots command history, newest first\n{text}"),
        )
        .map_err(|e| Error::new(format!("cannot write {}: {e}", self.path.display())))
    }
}

/// The preset's command with `file` in place of its first file (and of every later layer
/// that used the same file).
pub fn with_file(command: &str, file: &str) -> Result<String, Error> {
    let spec = crate::command::parse(command)?;
    let first = spec
        .panels
        .first()
        .and_then(|p| p.layers.first())
        .map(|l| l.file.value.clone())
        .ok_or_else(|| Error::new("the preset has no file"))?;
    let mut spans: Vec<std::ops::Range<usize>> = spec
        .panels
        .iter()
        .flat_map(|p| &p.layers)
        .filter(|l| l.file.value == first)
        .map(|l| l.file.span.clone())
        .collect();
    spans.sort_by_key(|s| std::cmp::Reverse(s.start));
    let mut out = command.to_owned();
    let typed = crate::complete::quote_if_needed(file.trim());
    for span in spans {
        out.replace_range(span, &typed);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_swap_the_file() {
        assert_eq!(
            with_file(
                "line a.csv x=t y=temp + a.csv y=p ; hist b.csv y=v",
                "lab/run 3.csv"
            )
            .unwrap(),
            "line \"lab/run 3.csv\" x=t y=temp + \"lab/run 3.csv\" y=p ; hist b.csv y=v"
        );
        let path =
            std::env::temp_dir().join(format!("easyplots-presets-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut h = History::load(&path, 10);
        h.push("bar x.csv").unwrap();
        h.save_preset("thesis", "line a.csv x=t y=temp --style=thesis")
            .unwrap();
        assert!(h.save_preset("two words", "line a.csv").is_err());
        assert!(h.save_preset("bad", "lnie a.csv").is_err());
        let again = History::load(&path, 10);
        assert_eq!(
            again.preset("THESIS").unwrap().command,
            "line a.csv x=t y=temp --style=thesis"
        );
        assert_eq!(again.commands(), ["bar x.csv"]);
        let mut again = again;
        again.delete_preset("thesis").unwrap();
        assert!(History::load(&path, 10).presets().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn newest_first_without_duplicates_and_capped() {
        let path =
            std::env::temp_dir().join(format!("easyplots-history-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut h = History::load(&path, 3);
        for c in [
            "line a.csv",
            "bar b.csv",
            "line a.csv",
            "scatter c.csv",
            "line \"x y.csv\"",
        ] {
            h.push(c).unwrap();
        }
        assert_eq!(
            h.commands(),
            ["line \"x y.csv\"", "scatter c.csv", "line a.csv"]
        );
        let again = History::load(&path, 10);
        assert_eq!(again.commands(), h.commands());
        let _ = std::fs::remove_file(&path);
    }
}
