//! Repository-wide comment scan.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::comments;

/// Aggregated results of one check run.
#[derive(Debug, Default)]
pub(crate) struct Report {
    files: u64,
    non_blank_lines: u64,
    comment_lines: u64,
    violations: Vec<String>,
}

impl Report {
    fn finish(&self) -> ExitCode {
        if self.violations.is_empty() {
            self.report_success();
            return ExitCode::SUCCESS;
        }
        self.report_failures();
        ExitCode::FAILURE
    }

    fn report_success(&self) {
        println!("xtask: comment check passed across {} file(s)", self.files);
        println!(
            "xtask: comment ratio {}/{} non-blank lines ({}%, informational)",
            self.comment_lines,
            self.non_blank_lines,
            ratio_percent(self.comment_lines, self.non_blank_lines)
        );
    }

    fn report_failures(&self) {
        for entry in &self.violations {
            eprintln!("xtask: {entry}");
        }
        eprintln!(
            "xtask: {} comment violation(s) in {} file(s)",
            self.violations.len(),
            self.files
        );
    }
}

/// Runs the comment policy check over tracked sources and configuration.
pub(crate) fn check_comments() -> ExitCode {
    let root = match repository_root() {
        Ok(root) => root,
        Err(message) => return fail(&message),
    };
    let files = match tracked_files(&root) {
        Ok(files) => files,
        Err(message) => return fail(&message),
    };
    let mut report = Report::default();
    for file in &files {
        let path = Path::new(file);
        let Some(syntax) = comments::syntax_for(path) else {
            continue;
        };
        if let Err(message) = scan_file(&root, file, syntax, &mut report) {
            return fail(&message);
        }
        report.files += 1;
    }
    report.finish()
}

fn fail(message: &str) -> ExitCode {
    eprintln!("xtask: {message}");
    ExitCode::FAILURE
}

fn repository_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err("git rev-parse failed; run inside a checkout".to_owned());
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_error| "git returned non-UTF-8 output".to_owned())?;
    Ok(PathBuf::from(text.trim()))
}

fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err("git ls-files failed; run inside a checkout".to_owned());
    }
    Ok(parse_null_separated(&output.stdout))
}

fn parse_null_separated(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| String::from_utf8_lossy(entry).into_owned())
        .collect()
}

fn scan_file(
    root: &Path,
    relative: &str,
    syntax: comments::Syntax,
    report: &mut Report,
) -> Result<(), String> {
    let content =
        fs::read_to_string(root.join(relative)).map_err(|error| format!("{relative}: {error}"))?;
    let mut scanner = comments::Scanner::new(syntax);
    for (index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        report.non_blank_lines += 1;
        let Some(text) = scanner.comment(line) else {
            continue;
        };
        report.comment_lines += 1;
        if let Some(violation) = comments::violation(&text) {
            let number = index + 1;
            report
                .violations
                .push(format!("{relative}:{number}: {}", violation.describe()));
        }
    }
    Ok(())
}

fn ratio_percent(comment_lines: u64, non_blank_lines: u64) -> String {
    if non_blank_lines == 0 {
        return "0.0".to_owned();
    }
    let per_mille = comment_lines.saturating_mul(1000) / non_blank_lines;
    let whole = per_mille / 10;
    let tenth = per_mille % 10;
    format!("{whole}.{tenth}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_null_separated_git_output() {
        assert_eq!(
            parse_null_separated(b"a\0b\0"),
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert!(parse_null_separated(b"").is_empty());
    }

    #[test]
    fn formats_ratio_in_tenths() {
        assert_eq!(ratio_percent(0, 0), "0.0");
        assert_eq!(ratio_percent(1, 3), "33.3");
        assert_eq!(ratio_percent(34, 277), "12.2");
    }
}
