use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path};

use semver::Version;
use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;

pub const CATEGORIES: &[&str] = &[
    "article",
    "journal",
    "thesis",
    "report",
    "lab-report",
    "homework",
    "exam",
    "presentation",
    "poster",
    "cv",
    "letter",
    "book",
    "notes",
    "cheatsheet",
    "other",
];

pub const LICENSES: &[&str] = &[
    "MIT",
    "MIT-0",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unlicense",
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
    "LPPL-1.3c",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "MPL-2.0",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Latex,
    Typst,
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Engine::Latex => "latex",
            Engine::Typst => "typst",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compiler {
    #[default]
    Pdflatex,
    Xelatex,
    Lualatex,
}

impl fmt::Display for Compiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Compiler::Pdflatex => "pdflatex",
            Compiler::Xelatex => "xelatex",
            Compiler::Lualatex => "lualatex",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bib {
    #[default]
    None,
    Bibtex,
    Biber,
}

impl fmt::Display for Bib {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Bib::None => "none",
            Bib::Bibtex => "bibtex",
            Bib::Biber => "biber",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub template: Template,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latex: Option<Latex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typst: Option<Typst>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, Variable>,
    #[serde(default, skip_serializing_if = "RegistryMeta::is_unset")]
    pub registry: RegistryMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub name: String,
    pub version: Version,
    pub engine: Engine,
    pub title: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub entrypoint: String,
    /// Universities or institutions this template is made for, such as "UC Chile".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub universities: Vec<String>,
    /// Name of the same template on Typst Universe, if it is published there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub universe: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repo: String,
    pub rev: String,
    #[serde(default = "dot")]
    pub path: String,
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn dot() -> String {
    ".".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Latex {
    #[serde(default)]
    pub compiler: Compiler,
    #[serde(default)]
    pub bib: Bib,
    #[serde(default)]
    pub shell_escape: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Typst {
    /// Oldest Typst compiler version the template works with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub prompt: String,
    pub default: String,
    /// Text to replace in `files`. Falls back to `default` when absent.
    #[serde(default, rename = "match", skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    pub files: Vec<String>,
}

impl Variable {
    pub fn needle(&self) -> &str {
        self.pattern.as_deref().unwrap_or(&self.default)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryMeta {
    #[serde(default)]
    pub official: bool,
    #[serde(default)]
    pub verified: bool,
}

impl RegistryMeta {
    fn is_unset(&self) -> bool {
        !self.official && !self.verified
    }
}

impl Manifest {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Returns every problem found, so an author can fix them in one pass.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let t = &self.template;

        if self.schema != SCHEMA {
            errors.push(format!("schema must be {SCHEMA}, found {}", self.schema));
        }
        if !valid_name(&t.name) {
            errors.push(format!(
                "name `{}` must be 3-40 characters of a-z, 0-9 and `-`, and start and end with a letter or digit",
                t.name
            ));
        }
        if t.title.trim().is_empty() {
            errors.push("title is empty".into());
        }
        if t.description.trim().is_empty() || t.description.chars().count() > 200 {
            errors.push("description must be 1-200 characters".into());
        }
        if t.authors.is_empty() {
            errors.push("authors is empty".into());
        }
        if t.universities.len() > 5 {
            errors.push("universities lists more than 5 names".into());
        }
        for u in &t.universities {
            if !(2..=60).contains(&u.trim().chars().count()) {
                errors.push(format!("university `{u}` must be 2-60 characters"));
            }
        }
        if t.engine == Engine::Latex && self.typst.is_some() {
            errors.push("[typst] is only allowed when engine = \"typst\"".into());
        }
        if !LICENSES.contains(&t.license.as_str()) {
            errors.push(format!(
                "license `{}` is not an accepted SPDX identifier (accepted: {})",
                t.license,
                LICENSES.join(", ")
            ));
        }
        if t.categories.is_empty() {
            errors.push("categories is empty".into());
        }
        for c in &t.categories {
            if !CATEGORIES.contains(&c.as_str()) {
                errors.push(format!(
                    "unknown category `{c}` (known: {})",
                    CATEGORIES.join(", ")
                ));
            }
        }
        let ext = match t.engine {
            Engine::Latex => ".tex",
            Engine::Typst => ".typ",
        };
        if !t.entrypoint.ends_with(ext) {
            errors.push(format!(
                "entrypoint `{}` must end in {ext} for engine {}",
                t.entrypoint, t.engine
            ));
        }
        if !safe_relative(&t.entrypoint) {
            errors.push(format!(
                "entrypoint `{}` must be a relative path inside the template",
                t.entrypoint
            ));
        }
        if t.engine == Engine::Typst && self.latex.is_some() {
            errors.push("[latex] is only allowed when engine = \"latex\"".into());
        }
        if t.engine == Engine::Latex && t.universe.is_some() {
            errors.push("universe is only allowed when engine = \"typst\"".into());
        }

        let s = &self.source;
        if !s.repo.starts_with("https://") {
            errors.push(format!("source.repo `{}` must be an https:// URL", s.repo));
        }
        if s.rev.len() != 40 || !s.rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            errors.push("source.rev must be a full 40-character commit hash".into());
        }
        if s.path != "." && !safe_relative(&s.path) {
            errors.push(format!(
                "source.path `{}` must be a relative path inside the repository",
                s.path
            ));
        }

        for (key, var) in &self.variables {
            if var.needle().is_empty() {
                errors.push(format!(
                    "variable `{key}` has nothing to replace: set `default` or `match`"
                ));
            }
            if var.files.is_empty() {
                errors.push(format!("variable `{key}` lists no files"));
            }
            for f in &var.files {
                if !safe_relative(f) {
                    errors.push(format!(
                        "variable `{key}`: `{f}` must be a relative path inside the template"
                    ));
                }
            }
        }
        errors
    }
}

pub fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    (3..=40).contains(&b.len())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
}

/// True for a non-empty relative path with no `..`, root or drive components.
pub fn safe_relative(path: &str) -> bool {
    if path.is_empty() || path.contains('\\') {
        return false;
    }
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
schema = 1

[template]
name = "plain-article"
version = "1.0.0"
engine = "latex"
title = "Plain article"
description = "A short article."
authors = ["Someone"]
license = "MIT"
categories = ["article"]
entrypoint = "main.tex"

[source]
repo = "https://github.com/texshelf/templates"
rev = "0123456789abcdef0123456789abcdef01234567"
path = "article"

[variables.title]
prompt = "Title"
default = "An example title"
files = ["main.tex"]
"#;

    #[test]
    fn accepts_a_valid_manifest() {
        let m = Manifest::parse(GOOD).unwrap();
        assert!(m.validate().is_empty(), "{:?}", m.validate());
        assert_eq!(m.variables["title"].needle(), "An example title");
    }

    #[test]
    fn reports_every_problem() {
        let text = GOOD
            .replace("plain-article", "Plain_Article")
            .replace("\"MIT\"", "\"Proprietary\"")
            .replace("main.tex\"\n\n[source]", "../main.typ\"\n\n[source]")
            .replace("0123456789abcdef0123456789abcdef01234567", "main");
        let errors = Manifest::parse(&text).unwrap().validate();
        assert_eq!(errors.len(), 5, "{errors:?}");
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(Manifest::parse(&GOOD.replace("schema = 1", "schema = 1\nextra = true")).is_err());
    }

    #[test]
    fn relative_paths() {
        assert!(safe_relative("a/b.tex"));
        assert!(!safe_relative("../b.tex"));
        assert!(!safe_relative("/etc/passwd"));
        assert!(!safe_relative("a\\b.tex"));
        assert!(!safe_relative(""));
    }
}
