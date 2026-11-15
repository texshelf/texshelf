use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::fetch;
use crate::manifest::{Engine, Variable};

const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub generated_at: String,
    pub templates: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
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
    #[serde(default)]
    pub universities: Vec<String>,
    #[serde(default)]
    pub official: bool,
    #[serde(default)]
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub universe: Option<String>,
    pub latest: Version,
    pub updated: String,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: Version,
    pub published: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bib: Option<String>,
    #[serde(default)]
    pub shell_escape: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, Variable>,
    pub tarball: Artifact,
    pub zip: Artifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub files: Vec<String>,
    /// Oldest Typst compiler version the template works with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typst: Option<String>,
    /// Version line of the compiler that built the preview, e.g. "typst 0.14.2".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_with: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<Audit>,
}

/// Result of the static checks the registry runs on a template's files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audit {
    pub at: String,
    pub rules: u32,
    /// "passed", "warnings" or "failed"
    pub status: String,
    pub scanned: usize,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// "high" or "warn"
    pub severity: String,
    pub rule: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub templates: BTreeMap<String, Stat>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Stat {
    #[serde(default)]
    pub stars: u64,
    #[serde(default)]
    pub downloads: u64,
}

impl Stats {
    /// Adds the stats of a tap under the same `tap/name` keys that `Index::absorb` uses.
    pub fn absorb(&mut self, tap: &str, other: Stats) {
        for (name, stat) in other.templates {
            self.templates.insert(format!("{tap}/{name}"), stat);
        }
    }
}

impl Entry {
    /// The name without its tap prefix: `lab-report` for `uc/lab-report`.
    pub fn short_name(&self) -> &str {
        self.name.rsplit('/').next().unwrap_or(&self.name)
    }

    /// The tap this template came from, or `None` for the main registry.
    pub fn tap(&self) -> Option<&str> {
        self.name.split_once('/').map(|(tap, _)| tap)
    }

    pub fn version(&self, wanted: Option<&Version>) -> Result<&VersionEntry> {
        let target = wanted.unwrap_or(&self.latest);
        self.versions
            .iter()
            .find(|v| &v.version == target)
            .with_context(|| {
                let known: Vec<String> = self
                    .versions
                    .iter()
                    .map(|v| v.version.to_string())
                    .collect();
                format!(
                    "{} has no version {target} (available: {})",
                    self.name,
                    known.join(", ")
                )
            })
    }

    pub fn compile_hint(&self, v: &VersionEntry) -> String {
        match self.engine {
            Engine::Typst => format!("typst compile {}", v.entrypoint),
            Engine::Latex => {
                let flag = match v.compiler.as_deref() {
                    Some("xelatex") => "-xelatex",
                    Some("lualatex") => "-lualatex",
                    _ => "-pdf",
                };
                format!("latexmk {flag} {}", v.entrypoint)
            }
        }
    }
}

/// True for names a registry may use: lowercase letters, digits and hyphens.
pub fn is_slug(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Index {
    /// Adds the templates of a tap, renamed to `tap/name`. Returns how many were added.
    ///
    /// A tap cannot mark its own templates as official or verified, and entries
    /// whose names are not plain slugs are left out.
    pub fn absorb(&mut self, tap: &str, other: Index) -> usize {
        let before = self.templates.len();
        for mut entry in other.templates {
            if !is_slug(&entry.name) {
                continue;
            }
            entry.name = format!("{tap}/{}", entry.name);
            entry.official = false;
            entry.verified = false;
            self.templates.push(entry);
        }
        self.templates.len() - before
    }

    pub fn find(&self, name: &str) -> Result<&Entry> {
        if let Some(entry) = self.templates.iter().find(|t| t.name == name) {
            return Ok(entry);
        }
        if !name.contains('/') {
            let tail = format!("/{name}");
            let tapped: Vec<&Entry> = self
                .templates
                .iter()
                .filter(|t| t.name.ends_with(&tail))
                .collect();
            match tapped.as_slice() {
                [] => {}
                [only] => return Ok(only),
                several => {
                    let names: Vec<&str> = several.iter().map(|t| t.name.as_str()).collect();
                    bail!(
                        "more than one tap has a template named `{name}`. Pick one: {}",
                        names.join(", ")
                    );
                }
            }
        }
        let near: Vec<&str> = self
            .templates
            .iter()
            .filter(|t| t.name.contains(name) || name.contains(t.name.as_str()))
            .map(|t| t.name.as_str())
            .take(5)
            .collect();
        if near.is_empty() {
            bail!("no template named `{name}`. Try `search {name}`.");
        }
        bail!(
            "no template named `{name}`. Did you mean: {}?",
            near.join(", ")
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Relevance,
    Downloads,
    Stars,
    Newest,
    Name,
    Official,
}

#[derive(Debug, Default)]
pub struct Query<'a> {
    pub text: &'a str,
    pub engine: Option<Engine>,
    pub category: Option<&'a str>,
    pub license: Option<&'a str>,
    pub official_only: bool,
    /// Keep templates made for any of these universities. Matches part of a name, ignoring case.
    pub universities: &'a [String],
}

fn score(entry: &Entry, words: &[String]) -> u32 {
    let name = entry.name.to_lowercase();
    let title = entry.title.to_lowercase();
    let description = entry.description.to_lowercase();
    let mut total = 0;
    for w in words {
        let mut s = 0;
        if name == *w {
            s += 100;
        } else if name.contains(w.as_str()) {
            s += 50;
        }
        if title.contains(w.as_str()) {
            s += 20;
        }
        if entry.categories.iter().any(|c| c == w) {
            s += 20;
        }
        if entry
            .keywords
            .iter()
            .any(|k| k.to_lowercase().contains(w.as_str()))
        {
            s += 10;
        }
        if description.contains(w.as_str()) {
            s += 5;
        }
        if s == 0 {
            return 0;
        }
        total += s;
    }
    total
}

fn made_for(entry: &Entry, wanted: &[String]) -> bool {
    wanted.iter().any(|w| {
        let w = w.trim().to_lowercase();
        !w.is_empty()
            && entry
                .universities
                .iter()
                .any(|u| u.to_lowercase().contains(&w))
    })
}

/// Filters and orders the index. Every word of the query has to match somewhere.
pub fn search<'a>(index: &'a Index, stats: &Stats, query: &Query, sort: Sort) -> Vec<&'a Entry> {
    let words: Vec<String> = query
        .text
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let mut hits: Vec<(u32, &Entry)> = index
        .templates
        .iter()
        .filter(|t| query.engine.is_none_or(|e| t.engine == e))
        .filter(|t| {
            query
                .category
                .is_none_or(|c| t.categories.iter().any(|x| x == c))
        })
        .filter(|t| {
            query
                .license
                .is_none_or(|l| t.license.eq_ignore_ascii_case(l))
        })
        .filter(|t| !query.official_only || t.official)
        .filter(|t| query.universities.is_empty() || made_for(t, query.universities))
        .filter_map(|t| {
            if words.is_empty() {
                return Some((0, t));
            }
            match score(t, &words) {
                0 => None,
                s => Some((s, t)),
            }
        })
        .collect();

    let stat = |t: &Entry| stats.templates.get(&t.name).copied().unwrap_or_default();
    hits.sort_by(|(sa, a), (sb, b)| {
        let primary = match sort {
            Sort::Relevance => sb.cmp(sa).then(b.official.cmp(&a.official)),
            Sort::Downloads => stat(b).downloads.cmp(&stat(a).downloads),
            Sort::Stars => stat(b).stars.cmp(&stat(a).stars),
            Sort::Newest => b.updated.cmp(&a.updated),
            Sort::Name => std::cmp::Ordering::Equal,
            Sort::Official => b
                .official
                .cmp(&a.official)
                .then(b.verified.cmp(&a.verified)),
        };
        primary.then_with(|| a.name.cmp(&b.name))
    });
    hits.into_iter().map(|(_, t)| t).collect()
}

/// A local copy of `index.json` and `stats.json`, refreshed once a day.
pub struct Store {
    pub url: String,
    pub dir: PathBuf,
}

impl Store {
    pub fn new(url: String, dir: PathBuf) -> Self {
        Self { url, dir }
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    fn stats_path(&self) -> PathBuf {
        self.dir.join("stats.json")
    }

    fn stats_url(&self) -> Option<String> {
        self.url
            .strip_suffix("index.json")
            .map(|base| format!("{base}stats.json"))
    }

    fn stale(&self) -> bool {
        let age = fs::metadata(self.index_path())
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok());
        age.is_none_or(|a| a > MAX_AGE)
    }

    /// Downloads the index and stats. Returns the number of templates.
    pub fn refresh(&self) -> Result<usize> {
        let bytes =
            fetch::get(&self.url).with_context(|| format!("could not fetch {}", self.url))?;
        let index: Index = serde_json::from_slice(&bytes).context("the index is not valid JSON")?;
        fs::create_dir_all(&self.dir)?;
        write_atomic(&self.index_path(), &bytes)?;
        if let Some(url) = self.stats_url() {
            match fetch::get(&url) {
                Ok(stats) if serde_json::from_slice::<Stats>(&stats).is_ok() => {
                    write_atomic(&self.stats_path(), &stats)?
                }
                _ => {}
            }
        }
        Ok(index.templates.len())
    }

    /// Loads the cached index, refreshing it first when it is missing or a day old.
    /// A failed refresh falls back to the cached copy.
    pub fn load(&self) -> Result<(Index, Stats)> {
        if self.stale() {
            if let Err(err) = self.refresh() {
                if !self.index_path().exists() {
                    return Err(err);
                }
                eprintln!("warning: using the cached index ({err:#})");
            }
        }
        let text = fs::read(self.index_path())?;
        let index: Index =
            serde_json::from_slice(&text).context("the cached index is corrupt; run `update`")?;
        if index.schema != crate::manifest::SCHEMA {
            bail!(
                "the registry uses index schema {}, which this version does not read. Please upgrade.",
                index.schema
            );
        }
        let stats = fs::read(self.stats_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Ok((index, stats))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, engine: Engine, official: bool, updated: &str, cats: &[&str]) -> Entry {
        Entry {
            name: name.into(),
            engine,
            title: name.replace('-', " "),
            description: format!("A template called {name}"),
            authors: vec!["x".into()],
            license: "MIT".into(),
            repository: None,
            keywords: vec![],
            categories: cats.iter().map(|c| c.to_string()).collect(),
            universities: vec![],
            official,
            verified: false,
            universe: None,
            latest: Version::new(1, 0, 0),
            updated: updated.into(),
            versions: vec![],
        }
    }

    fn sample() -> (Index, Stats) {
        let index = Index {
            schema: 1,
            generated_at: String::new(),
            templates: vec![
                entry("article", Engine::Latex, true, "2026-01-01", &["article"]),
                entry(
                    "arxiv-style",
                    Engine::Latex,
                    false,
                    "2026-03-01",
                    &["article"],
                ),
                entry(
                    "charged-ieee",
                    Engine::Typst,
                    false,
                    "2026-02-01",
                    &["journal"],
                ),
            ],
        };
        let mut stats = Stats::default();
        stats.templates.insert(
            "arxiv-style".into(),
            Stat {
                stars: 900,
                downloads: 3,
            },
        );
        stats.templates.insert(
            "article".into(),
            Stat {
                stars: 1,
                downloads: 40,
            },
        );
        (index, stats)
    }

    fn names(hits: Vec<&Entry>) -> Vec<&str> {
        hits.into_iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn exact_name_ranks_first() {
        let (index, stats) = sample();
        let q = Query {
            text: "article",
            ..Default::default()
        };
        assert_eq!(
            names(search(&index, &stats, &q, Sort::Relevance)),
            ["article", "arxiv-style"]
        );
    }

    #[test]
    fn sorts_and_filters() {
        let (index, stats) = sample();
        let all = Query::default();
        assert_eq!(
            names(search(&index, &stats, &all, Sort::Stars))[0],
            "arxiv-style"
        );
        assert_eq!(
            names(search(&index, &stats, &all, Sort::Downloads))[0],
            "article"
        );
        assert_eq!(
            names(search(&index, &stats, &all, Sort::Newest))[0],
            "arxiv-style"
        );
        let typst = Query {
            engine: Some(Engine::Typst),
            ..Default::default()
        };
        assert_eq!(
            names(search(&index, &stats, &typst, Sort::Name)),
            ["charged-ieee"]
        );
        let none = Query {
            text: "article thesis",
            ..Default::default()
        };
        assert!(search(&index, &stats, &none, Sort::Relevance).is_empty());
    }

    #[test]
    fn filters_by_university() {
        let (mut index, stats) = sample();
        index.templates[0].universities = vec!["UC Chile".into()];
        index.templates[1].universities = vec!["University of Cambridge".into(), "MIT".into()];
        let wanted = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        let find = |list: &[String]| {
            let q = Query {
                universities: list,
                ..Default::default()
            };
            search(&index, &stats, &q, Sort::Name)
                .into_iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(find(&wanted(&["uc chile"])), ["article"]);
        assert_eq!(find(&wanted(&["cambridge"])), ["arxiv-style"]);
        assert_eq!(
            find(&wanted(&["UC Chile", "MIT"])),
            ["article", "arxiv-style"]
        );
        assert!(find(&wanted(&["Oxford"])).is_empty());
    }

    #[test]
    fn taps_are_prefixed_and_found_by_short_name() {
        let (mut index, mut stats) = sample();
        let tap = |names: &[&str]| Index {
            schema: 1,
            generated_at: String::new(),
            templates: names
                .iter()
                .map(|n| entry(n, Engine::Latex, true, "2026-04-01", &["report"]))
                .collect(),
        };
        assert_eq!(
            index.absorb("uc", tap(&["lab-report", "article", "../x"])),
            2
        );
        let mut theirs = Stats::default();
        theirs
            .templates
            .insert("lab-report".into(), Stat::default());
        stats.absorb("uc", theirs);
        assert!(stats.templates.contains_key("uc/lab-report"));

        let found = index.find("lab-report").unwrap();
        assert_eq!(found.name, "uc/lab-report");
        assert_eq!(
            (found.short_name(), found.tap()),
            ("lab-report", Some("uc"))
        );
        assert!(!found.official);
        assert_eq!(index.find("article").unwrap().name, "article");
        assert_eq!(index.find("uc/article").unwrap().name, "uc/article");

        index.absorb("lab", tap(&["lab-report"]));
        let err = index.find("lab-report").unwrap_err().to_string();
        assert!(
            err.contains("uc/lab-report") && err.contains("lab/lab-report"),
            "{err}"
        );
    }

    #[test]
    fn find_suggests_close_names() {
        let (index, _) = sample();
        let err = index.find("arxiv").unwrap_err().to_string();
        assert!(err.contains("arxiv-style"), "{err}");
    }
}
