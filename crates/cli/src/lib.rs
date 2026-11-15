//! `easyplots`: the terminal command. It takes the same command text as the launcher
//! (see `docs/SPEC.md`) and saves the plot in the output folder from `config.toml`.
//!
//! This is a library so the Mac app can run it too: EasyPlots.app links itself as
//! `/usr/local/bin/easyplots` and calls `run` when started under that name.

use std::path::PathBuf;

use easyplots_core::data::ColumnKind;
use easyplots_core::data::Stats;
use easyplots_core::{Config, Engine};

const HELP: &str = "\
easyplots: fast plots from CSV files

Usage:
  easyplots <command>            make a plot and save it in the output folder
  easyplots --columns <file>     list a file's columns and what they hold
  easyplots --stats <file>...    N, mean, sd, median, min and max of each number column
  easyplots --svg <command>      print the plot as SVG instead of saving it
  easyplots --time <command>     also show how long each step took
  easyplots --each \"<pattern>\" <command>   one plot per matching file: the file goes
                                 after the plot type (--each \"runs/*.csv\" line x=t y=temp)
  easyplots preset                list saved presets
  easyplots preset <name> <file>  draw a preset with another file
  easyplots again <plot> [more]  print the command a saved plot was made from; with more
                                 options after it, draw it again with those added
  easyplots --complete \"<text>\"  the launcher's suggestions for text typed so far (JSON)
  easyplots --version | --help

Examples:
  easyplots line data.csv x=time y=temp
  easyplots line data.csv x=time y=temp,pressure title=\"Run 3\"
  easyplots line a.csv x=t y=temp + b.csv x=t y=temp --out=compare.pdf
  easyplots scatter data.csv x=2 y=3 color=orange
  easyplots bar sales.csv x=month y=north,south --size=15x10cm
  easyplots hist data.csv y=temp bins=30
  easyplots scatter data.csv x=dose y=resp err=sd fit=linear
  easyplots line a.csv x=t y=temp \";\" hist a.csv y=temp   (quote the ; for the shell)

Files are looked up in the current folder, then in data_dir. Plots are saved in the
output folder. Both are set in ~/.config/easyplots/config.toml (or $EASYPLOTS_CONFIG).
The full command language is in docs/SPEC.md.";

/// Run the command with its arguments (without the program name). Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let (mode, rest) = match args.first().map(String::as_str) {
        None | Some("--help" | "-h") => {
            println!("{HELP}");
            return 0;
        }
        // For the Settings window's test: the option list and default values as JSON.
        Some("--settings-json") => {
            let info = serde_json::json!({
                "fields": easyplots_core::settings::fields(),
                "values": easyplots_core::settings::values(&Config::default()),
                "config_path": "~/.config/easyplots/config.toml",
                "presets": [],
                "warnings": [],
                "version": easyplots_core::VERSION,
            });
            println!("{info}");
            return 0;
        }
        Some("--version" | "-V") => {
            println!("easyplots {}", easyplots_core::VERSION);
            return 0;
        }
        Some(
            m @ ("--complete" | "--columns" | "--stats" | "--svg" | "--time" | "--each" | "again"
            | "preset"),
        ) => (m, &args[1..]),
        Some(_) => ("plot", args),
    };

    let config_path = Config::default_path();
    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("easyplots: {e}");
            return 1;
        }
    };
    let mut engine = Engine::new(config);
    let cwd: Option<PathBuf> = std::env::current_dir().ok();
    let command = join_args(rest);

    let result = match mode {
        "--complete" => {
            // The text as one argument, exactly as typed (no re-quoting).
            let text = rest.first().cloned().unwrap_or_default();
            let mut src = CliSource(&mut engine, cwd.clone());
            let c = easyplots_core::complete::complete(&text, text.len(), &mut src);
            println!(
                "{}",
                serde_json::json!({ "start": c.start, "end": c.end, "items": c.items })
            );
            Ok(())
        }
        "--columns" => list_columns(&mut engine, &command, cwd.as_deref()),
        "--stats" => rest
            .iter()
            .try_for_each(|file| print_stats(&mut engine, file, cwd.as_deref())),
        "again" => again(&mut engine, rest, cwd.as_deref()),
        "preset" => preset(&mut engine, &config_path, rest, cwd.as_deref()),
        "--each" => each(&mut engine, rest, cwd.as_deref()),
        "--svg" => engine
            .render(&command, cwd.as_deref())
            .map(|r| print!("{}", r.svg)),
        _ => engine.plot(&command, cwd.as_deref()).map(|saved| {
            println!("{}", saved.path.display());
            for report in &saved.fit_reports {
                println!("{report}");
            }
            // Hints go to stderr: stdout stays the path (and fit numbers) for scripts.
            if engine.config.launcher.advice {
                for a in &saved.advice {
                    eprintln!("hint: {}", a.text);
                }
            }
            if mode == "--time" {
                let t = saved.timings;
                let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
                eprintln!(
                    "parse {:.1} ms · load {:.1} ms · draw {:.1} ms · encode {:.1} ms · write {:.1} ms · total {:.1} ms",
                    ms(t.parse),
                    ms(t.load),
                    ms(t.render),
                    ms(t.encode),
                    ms(t.write),
                    ms(t.total())
                );
            }
        }),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("easyplots: {}", e.display_with(&command));
            1
        }
    }
}

/// `again PLOT`: print the stored command. `again PLOT more options`: draw it again with the
/// options added at the end (they replace the stored ones of the same name), and save it
/// as a new plot.
fn again(
    engine: &mut Engine,
    rest: &[String],
    cwd: Option<&std::path::Path>,
) -> Result<(), easyplots_core::Error> {
    let Some((plot, more)) = rest.split_first() else {
        return Err(easyplots_core::Error::new(
            "again needs a plot file: easyplots again ~/Plots/name.png",
        ));
    };
    let stored = engine.recall(plot, cwd)?;
    if more.is_empty() {
        println!("{stored}");
        return Ok(());
    }
    let command = easyplots_core::command::with_more(&stored, &join_args(more));
    let saved = engine
        .plot(&command, cwd)
        .map_err(|e| easyplots_core::Error::new(e.display_with(&command)))?;
    println!("{}", saved.path.display());
    Ok(())
}

/// `--each "PATTERN" TYPE options…`: one plot per matching file. Each file is put after
/// the plot type, so `--each "runs/*.csv" line x=t y=temp` draws and saves
/// `line runs/a.csv x=t y=temp`, `line runs/b.csv x=t y=temp`… and prints each path.
fn each(
    engine: &mut Engine,
    rest: &[String],
    cwd: Option<&std::path::Path>,
) -> Result<(), easyplots_core::Error> {
    let Some((pattern, words)) = rest.split_first() else {
        return Err(easyplots_core::Error::new(
            "--each needs a pattern and a command: easyplots --each \"runs/*.csv\" line x=t y=temp",
        ));
    };
    if words.is_empty() {
        return Err(easyplots_core::Error::new(
            "--each needs the command after the pattern, without its file: \
             easyplots --each \"runs/*.csv\" line x=t y=temp",
        ));
    }
    let files = easyplots_core::figure::Lookup::new(&engine.config, cwd).matching(pattern);
    if files.is_empty() {
        return Err(easyplots_core::Error::new(format!(
            "no files match {pattern}"
        )));
    }
    let (first, options) = words.split_first().expect("checked above");
    let options = join_args(options);
    for file in files {
        let name = easyplots_core::complete::quote_if_needed(&file.display().to_string());
        let command = format!("{first} {name} {options}");
        let saved = engine
            .plot(&command, cwd)
            .map_err(|e| easyplots_core::Error::new(e.display_with(&command)))?;
        println!("{}", saved.path.display());
    }
    Ok(())
}

/// `preset`: list presets. `preset NAME FILE [more]`: draw NAME's command with FILE.
fn preset(
    engine: &mut Engine,
    config_path: &std::path::Path,
    rest: &[String],
    cwd: Option<&std::path::Path>,
) -> Result<(), easyplots_core::Error> {
    let history = easyplots_core::history::History::load(&Config::history_path(config_path), 1);
    let Some((name, rest)) = rest.split_first() else {
        if history.presets().is_empty() {
            println!("No presets yet. Save one from the launcher's preview with ⌘D.");
        }
        for p in history.presets() {
            println!("{:<16} {}", p.name, p.command);
        }
        return Ok(());
    };
    let p = history.preset(name).ok_or_else(|| {
        easyplots_core::Error::new(format!(
            "no preset \"{name}\"{}",
            easyplots_core::error::suggestion(
                name,
                history.presets().iter().map(|p| p.name.as_str())
            )
        ))
    })?;
    let Some((file, more)) = rest.split_first() else {
        println!("{}", p.command);
        return Ok(());
    };
    let mut command = easyplots_core::history::with_file(&p.command, file)?;
    if !more.is_empty() {
        command = easyplots_core::command::with_more(&command, &join_args(more));
    }
    let saved = engine
        .plot(&command, cwd)
        .map_err(|e| easyplots_core::Error::new(e.display_with(&command)))?;
    println!("{}", saved.path.display());
    Ok(())
}

/// Files and columns for `--complete`, read the way the launcher reads them.
struct CliSource<'a>(&'a mut Engine, Option<PathBuf>);

impl easyplots_core::complete::Source for CliSource<'_> {
    fn files(&mut self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
    fn columns(
        &mut self,
        file: &str,
        read: &easyplots_core::data::ReadOptions,
    ) -> Option<Vec<(String, String)>> {
        let table = self.0.columns_read(file, read, self.1.as_deref()).ok()?;
        Some(
            table
                .columns
                .iter()
                .map(|c| (c.name.clone(), kind_name(c.data.kind()).to_owned()))
                .collect(),
        )
    }
}

fn kind_name(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Number => "numbers",
        ColumnKind::Date => "dates",
        ColumnKind::Text => "text",
    }
}

fn list_columns(
    engine: &mut Engine,
    file: &str,
    cwd: Option<&std::path::Path>,
) -> Result<(), easyplots_core::Error> {
    let table = engine.columns(file, cwd)?;
    println!("{} ({} rows)", table.path.display(), table.rows);
    for (i, c) in table.columns.iter().enumerate() {
        let kind = kind_name(c.data.kind());
        println!("  {:>2}  {:<24} {kind:<8} {}", i + 1, c.name, c.summary());
    }
    Ok(())
}

fn print_stats(
    engine: &mut Engine,
    file: &str,
    cwd: Option<&std::path::Path>,
) -> Result<(), easyplots_core::Error> {
    let table = engine.columns(file, cwd)?;
    println!("{} ({} rows)", table.path.display(), table.rows);
    println!(
        "  {:<24} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "column", "N", "mean", "sd", "median", "min", "max"
    );
    for c in &table.columns {
        let easyplots_core::data::ColumnData::Number(v) = &c.data else {
            continue;
        };
        let Some(s) = Stats::of(v) else {
            continue;
        };
        let f = |v: f64| s.show(v);
        println!(
            "  {:<24} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10}",
            c.name,
            s.n,
            f(s.mean),
            f(s.sd),
            f(s.median),
            f(s.min),
            f(s.max)
        );
    }
    Ok(())
}

/// Rebuild the command text from the shell's arguments. The shell has already removed
/// quotes, so values with spaces are quoted again: `title=Run 3` → `title="Run 3"`.
/// Options that take one value per column are quoted piece by piece, because the shell
/// glues `label="Run A","Run B"` into `label=Run A,Run B` (a single label with a comma
/// needs single quotes round it in Terminal: `label='"Run A, warm"'`). An argument that
/// still holds quotes was quoted on purpose and is passed on as it is.
fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if !a.chars().any(char::is_whitespace) || a.contains('"') {
                return a.clone();
            }
            let escaped = a.replace('\\', "\\\\");
            match escaped.split_once('=') {
                Some((key, value)) if !key.contains(char::is_whitespace) => {
                    if LIST_KEYS.contains(&key.trim_start_matches('-')) {
                        let pieces: Vec<String> = top_level_pieces(value)
                            .iter()
                            .map(|v| format!("\"{v}\""))
                            .collect();
                        format!("{key}={}", pieces.join(","))
                    } else {
                        format!("{key}=\"{value}\"")
                    }
                }
                _ => format!("\"{escaped}\""),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Split at commas outside brackets: `smooth(a, 9),b` → `smooth(a, 9)` and `b`.
fn top_level_pieces(value: &str) -> Vec<&str> {
    let (mut depth, mut start, mut out) = (0i32, 0, Vec::new());
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&value[start..]);
    out
}

/// Options that take a list, one value per column (see `command.rs`).
const LIST_KEYS: &[&str] = &[
    "x", "y", "label", "color", "colour", "marker", "style", "dash", "err", "yerr", "xerr", "axis",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requotes_values_with_spaces() {
        let args: Vec<String> = ["line", "my data.csv", "title=Run 3", "y=a,b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            join_args(&args),
            r#"line "my data.csv" title="Run 3" y=a,b"#
        );
    }

    #[test]
    fn list_values_stay_lists() {
        // What the shell makes of `label="Weight (%)","Impact (%)" y="T (C)",p`.
        let args: Vec<String> = ["line", "f.csv", "label=Weight (%),Impact (%)", "y=T (C),p"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            join_args(&args),
            r#"line f.csv label="Weight (%)","Impact (%)" y="T (C)","p""#
        );
        let args = vec!["y=smooth(a, 9),b c".to_string()];
        assert_eq!(join_args(&args), r#"y="smooth(a, 9)","b c""#);
        // Quoted on purpose (single quotes in the shell): passed on unchanged.
        let args = vec![r#"label="Run A, warm""#.to_string()];
        assert_eq!(join_args(&args), r#"label="Run A, warm""#);
    }
}
