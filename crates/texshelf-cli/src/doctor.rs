use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::ui;

const TOOLS: &[(&str, &str, &str)] = &[
    ("pdflatex", "--version", "LaTeX compiler"),
    ("xelatex", "--version", "LaTeX compiler with system fonts"),
    ("lualatex", "--version", "LaTeX compiler with system fonts"),
    ("latexmk", "--version", "runs LaTeX as many times as needed"),
    ("bibtex", "--version", "bibliographies"),
    ("biber", "--version", "bibliographies for biblatex"),
    ("tectonic", "--version", "self-contained LaTeX engine"),
    ("typst", "--version", "Typst compiler"),
    ("git", "--version", "version control for new projects"),
];

fn version(tool: &str, flag: &str) -> Option<String> {
    let output = Command::new(tool).arg(flag).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
}

/// Collects the names inside `\usepackage{...}`, `\RequirePackage{...}` and
/// `\documentclass{...}`, with the file extension LaTeX would look for.
fn requirements(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in source.lines() {
        let line = line.split('%').next().unwrap_or("");
        for (command, ext) in [
            ("\\usepackage", "sty"),
            ("\\RequirePackage", "sty"),
            ("\\documentclass", "cls"),
        ] {
            let mut rest = line;
            while let Some(at) = rest.find(command) {
                rest = &rest[at + command.len()..];
                let mut body = rest.trim_start();
                if body.starts_with('[') {
                    match body.find(']') {
                        Some(end) => body = body[end + 1..].trim_start(),
                        None => break,
                    }
                }
                let Some(inner) = body.strip_prefix('{') else {
                    continue;
                };
                let Some(end) = inner.find('}') else { break };
                for name in inner[..end].split(',') {
                    let name = name.trim();
                    if !name.is_empty() && !name.contains('\\') && !name.contains('#') {
                        found.insert(format!("{name}.{ext}"));
                    }
                }
            }
        }
    }
    found
}

fn check_packages(file: &Path) -> Result<()> {
    let source =
        fs::read_to_string(file).with_context(|| format!("could not read {}", file.display()))?;
    let dir = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let needed = requirements(&source);
    let mut missing = Vec::new();
    for name in &needed {
        if dir.join(name).exists() {
            continue;
        }
        let found = Command::new("kpsewhich")
            .arg(name)
            .current_dir(dir)
            .output()
            .context("kpsewhich is not installed, so packages cannot be checked")?
            .status
            .success();
        if !found {
            missing.push(name.as_str());
        }
    }
    let names: Vec<&str> = missing
        .iter()
        .filter_map(|m| m.rsplit_once('.').map(|(n, _)| n))
        .collect();
    let fix = (!names.is_empty()).then(|| format!("tlmgr install {}", names.join(" ")));
    let missing: Vec<String> = missing.iter().map(|m| m.to_string()).collect();
    ui::show(ui::packages(
        &file.display().to_string(),
        needed.len(),
        &missing,
        fix.as_deref(),
        ui::width(),
    ));
    Ok(())
}

pub fn run(file: Option<PathBuf>) -> Result<()> {
    let found: Vec<ui::Tool> = TOOLS
        .iter()
        .map(|(name, flag, purpose)| ui::Tool {
            name,
            version: version(name, flag),
            purpose,
        })
        .collect();
    let any_latex = found
        .iter()
        .any(|t| t.version.is_some() && (t.name.ends_with("latex") || t.name == "tectonic"));
    ui::show(ui::tools(&found, ui::width()));
    if !any_latex {
        println!("\nNo LaTeX compiler found. TinyTeX (https://yihui.org/tinytex/) is a small one to start with.");
    }
    let file = file.or_else(|| Some(PathBuf::from("main.tex")).filter(|p| p.exists()));
    if let Some(file) = file {
        check_packages(&file)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_packages_and_class() {
        let source = "\\documentclass[11pt]{article}\n\\usepackage{amsmath, graphicx}\n% \\usepackage{ghost}\n\\usepackage[utf8]{inputenc}\\usepackage{siunitx}";
        let got: Vec<String> = super::requirements(source).into_iter().collect();
        assert_eq!(
            got,
            [
                "amsmath.sty",
                "article.cls",
                "graphicx.sty",
                "inputenc.sty",
                "siunitx.sty"
            ]
        );
    }
}
