use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use texshelf_core::manifest::{valid_name, Engine, Manifest, CATEGORIES};

use crate::commands::{launch, load};

const MANIFEST: &str = "texshelf.toml";
const REGISTRY: &str = "https://github.com/texshelf/registry";

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn title_case(name: &str) -> String {
    let mut words = Vec::new();
    for word in name.split('-').filter(|w| !w.is_empty()) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            words.push(format!("{}{}", first.to_uppercase(), chars.as_str()));
        }
    }
    words.join(" ")
}

const LATEX_MAIN: &str = r"\documentclass[11pt,a4paper]{article}

\usepackage[T1]{fontenc}
\usepackage[utf8]{inputenc}
\usepackage[margin=2.5cm]{geometry}

\title{An Example Title}
\author{A. U. Thor}
\date{\today}

\begin{document}

\maketitle

\section{Introduction}

Replace this with the sample content people should see first.

\end{document}
";

const TYPST_MAIN: &str = r#"#let title = "An Example Title"
#let author = "A. U. Thor"

#set document(title: title, author: author)
#set page(paper: "a4", margin: 2.5cm)
#set text(size: 11pt)

#align(center)[
  #text(size: 18pt, weight: "bold", title) \
  #author
]

= Introduction

Replace this with the sample content people should see first.
"#;

const LATEX_IGNORE: &str =
    "*.aux\n*.bbl\n*.blg\n*.fdb_latexmk\n*.fls\n*.log\n*.out\n*.synctex.gz\n*.toc\nmain.pdf\n";

/// Creates a folder with a starter document and a manifest to fill in.
pub fn new_template(
    bin: &str,
    dir: &Path,
    engine: Engine,
    name: Option<String>,
    category: &str,
) -> Result<()> {
    let name = match name {
        Some(name) => name,
        None => dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_lowercase)
            .context("could not take a template name from the folder; pass --name")?,
    };
    if !valid_name(&name) {
        bail!("`{name}` is not a valid template name: use 3-40 characters of a-z, 0-9 and `-`. Pass --name to set one.");
    }
    if !CATEGORIES.contains(&category) {
        bail!(
            "unknown category `{category}` (known: {})",
            CATEGORIES.join(", ")
        );
    }
    let occupied = fs::read_dir(dir)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if occupied {
        bail!("{} already has files in it", dir.display());
    }
    fs::create_dir_all(dir)?;

    let author = git(Path::new("."), &["config", "user.name"])
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Your Name".into());
    let (entry, body, engine_table) = match engine {
        Engine::Latex => (
            "main.tex",
            LATEX_MAIN,
            "[latex]\ncompiler = \"pdflatex\"\nbib = \"none\"\n",
        ),
        Engine::Typst => ("main.typ", TYPST_MAIN, "[typst]\ncompiler = \"0.12.0\"\n"),
    };
    let manifest = format!(
        r#"schema = 1

[template]
name = "{name}"
version = "0.1.0"
engine = "{engine}"
title = "{title}"
description = "One sentence that says what this template is for."
authors = ["{author}"]
license = "MIT"
keywords = []
categories = ["{category}"]
entrypoint = "{entry}"

{engine_table}
[variables.title]
prompt = "Title"
default = "An Example Title"
files = ["{entry}"]

[variables.author]
prompt = "Author"
default = "A. U. Thor"
files = ["{entry}"]
"#,
        title = title_case(&name),
        author = author.replace('"', "'"),
    );
    fs::write(dir.join(MANIFEST), manifest)?;
    fs::write(dir.join(entry), body)?;
    if engine == Engine::Latex {
        fs::write(dir.join(".gitignore"), LATEX_IGNORE)?;
    }

    println!(
        "{} {} with {entry} and {MANIFEST}",
        crate::style::good("Created"),
        crate::style::strong(dir.display())
    );
    println!("Next:");
    println!("  1. Write the template and fill in {MANIFEST} (description, license, variables).");
    println!("  2. Commit it and push it to a public GitHub repository, with a LICENSE file.");
    println!("  3. Run `{bin} publish` in this folder to open the registry pull request.");
    Ok(())
}

/// `git@github.com:owner/repo.git` and friends as `https://github.com/owner/repo`.
pub fn https_remote(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let rest = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("https://github.com/"))?;
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && !owner.is_empty() && !repo.is_empty())
        .then(|| format!("https://github.com/{owner}/{repo}"))
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Builds the registry manifest for the template in `dir` from its
/// texshelf.toml and the state of its Git repository.
pub fn registry_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join(MANIFEST);
    let text = fs::read_to_string(&path).with_context(|| {
        format!(
            "no {MANIFEST} in {}. Run `new-template` to create one.",
            dir.display()
        )
    })?;
    let mut table: toml::Table =
        toml::from_str(&text).with_context(|| format!("{MANIFEST} is not valid TOML"))?;

    if git(dir, &["rev-parse", "--is-inside-work-tree"]).is_none() {
        bail!("{} is not inside a Git repository", dir.display());
    }
    let dirty = git(dir, &["status", "--porcelain", "--", "."]).unwrap_or_default();
    if !dirty.is_empty() {
        bail!("there are uncommitted changes here. Commit them first, so the published commit matches what you see.");
    }
    let remote = git(dir, &["remote", "get-url", "origin"])
        .context("this repository has no `origin` remote")?;
    let repo = https_remote(&remote)
        .with_context(|| format!("the origin remote `{remote}` is not a GitHub repository"))?;
    let rev = git(dir, &["rev-parse", "HEAD"]).context("this repository has no commits")?;
    let pushed = git(dir, &["branch", "-r", "--contains", "HEAD"]).unwrap_or_default();
    if pushed.is_empty() {
        bail!(
            "commit {} is not on GitHub yet. Run `git push` first.",
            &rev[..12]
        );
    }
    let prefix = git(dir, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    let prefix = prefix.trim_end_matches('/');

    let mut exclude: Vec<toml::Value> = table
        .get("source")
        .and_then(|s| s.get("exclude"))
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    if !exclude.iter().any(|e| e.as_str() == Some(MANIFEST)) {
        exclude.push(MANIFEST.into());
    }
    let mut source = toml::Table::new();
    source.insert("repo".into(), repo.into());
    source.insert("rev".into(), rev.into());
    source.insert(
        "path".into(),
        if prefix.is_empty() { "." } else { prefix }.into(),
    );
    source.insert("exclude".into(), exclude.into());
    table.insert("source".into(), source.into());
    table.remove("registry");

    let manifest: Manifest = table
        .try_into()
        .with_context(|| format!("{MANIFEST} has a problem"))?;
    let mut errors = manifest.validate();
    if !dir.join(&manifest.template.entrypoint).is_file() {
        errors.push(format!(
            "entrypoint `{}` does not exist",
            manifest.template.entrypoint
        ));
    }
    for (key, var) in &manifest.variables {
        let found = var.files.iter().any(|f| {
            fs::read_to_string(dir.join(f))
                .map(|text| text.contains(var.needle()))
                .unwrap_or(false)
        });
        if !found {
            errors.push(format!(
                "variable `{key}`: the text `{}` does not appear in its files",
                var.needle()
            ));
        }
    }
    if !errors.is_empty() {
        bail!(
            "{MANIFEST} has {} problem(s):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }
    Ok(manifest)
}

pub fn publish(dir: Option<PathBuf>, print: bool) -> Result<()> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let manifest = registry_manifest(&dir)?;
    let t = &manifest.template;

    if let Ok((index, _)) = load() {
        if let Some(entry) = index.templates.iter().find(|e| e.name == t.name) {
            if entry.versions.iter().any(|v| v.version == t.version) {
                bail!(
                    "{} {} is already published. Raise `version` in {MANIFEST}, commit and push, then run this again.",
                    t.name,
                    t.version
                );
            }
        }
    }

    let body = toml::to_string(&manifest)?;
    let file = format!("templates/{}/{}.toml", t.name, t.version);
    let url = format!(
        "{REGISTRY}/new/main?filename={}&value={}",
        percent_encode(&file),
        percent_encode(&body)
    );
    println!("{file}\n\n{body}");
    if print {
        println!("{url}");
        return Ok(());
    }
    println!("Opening GitHub with this file filled in. Choose \"Propose new file\", then open the pull request.");
    println!("If the browser does not open, use this link:\n{url}");
    launch(&url);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes() {
        let want = Some("https://github.com/jj-sm/TeX-AST".to_string());
        assert_eq!(https_remote("git@github.com:jj-sm/TeX-AST.git"), want);
        assert_eq!(https_remote("https://github.com/jj-sm/TeX-AST.git"), want);
        assert_eq!(https_remote("https://github.com/jj-sm/TeX-AST/"), want);
        assert_eq!(https_remote("https://gitlab.com/a/b"), None);
        assert_eq!(https_remote("https://github.com/a"), None);
    }

    #[test]
    fn encoding() {
        assert_eq!(
            percent_encode("a b/c=\"é\"\n"),
            "a%20b%2Fc%3D%22%C3%A9%22%0A"
        );
    }

    #[test]
    fn titles() {
        assert_eq!(title_case("uc-lab-report"), "Uc Lab Report");
    }
}
