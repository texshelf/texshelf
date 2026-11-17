use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use semver::Version;
use texshelf_core::fetch;
use texshelf_core::index::{Entry, Index, Query, Sort, Stats, Store, VersionEntry};
use texshelf_core::manifest::{Engine, Manifest, CATEGORIES};
use texshelf_core::{DEFAULT_INDEX_URL, SITE_URL};

use crate::style;
use crate::ui;

pub(crate) fn cache_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("TEXSHELF_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let dirs = directories::ProjectDirs::from("science", "jjsm", "texshelf")
        .context("could not find a cache folder for this user")?;
    Ok(dirs.cache_dir().to_path_buf())
}

fn store() -> Result<Store> {
    let url = std::env::var("TEXSHELF_INDEX_URL").unwrap_or_else(|_| DEFAULT_INDEX_URL.to_string());
    Ok(Store::new(url, cache_dir()?))
}

/// Loads the main index and adds every tap. A tap that cannot be read is skipped.
pub(crate) fn load() -> Result<(Index, Stats)> {
    let (mut index, mut stats) = store()?.load()?;
    for (name, tap) in crate::taps::stores(&cache_dir()?)? {
        match tap.load() {
            Ok((theirs, their_stats)) => {
                index.absorb(&name, theirs);
                stats.absorb(&name, their_stats);
            }
            Err(err) => style::warning(format!("skipping tap `{name}` ({err:#})")),
        }
    }
    Ok((index, stats))
}

fn split_spec(spec: &str) -> Result<(&str, Option<Version>)> {
    match spec.split_once('@') {
        None => Ok((spec, None)),
        Some((name, version)) => {
            let version = Version::parse(version)
                .with_context(|| format!("`{version}` is not a version like 1.2.0"))?;
            Ok((name, Some(version)))
        }
    }
}

fn resolve<'a>(index: &'a Index, spec: &str) -> Result<(&'a Entry, &'a VersionEntry)> {
    let (name, version) = split_spec(spec)?;
    let entry = index.find(name)?;
    Ok((entry, entry.version(version.as_ref())?))
}

fn git_init(dir: &Path) -> bool {
    let quiet = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    !quiet(&["rev-parse", "--is-inside-work-tree"]) && quiet(&["init"])
}

pub fn init(
    spec: &str,
    dir: Option<PathBuf>,
    vars: &[String],
    yes: bool,
    no_git: bool,
    force: bool,
) -> Result<()> {
    let (index, _) = load()?;
    let (entry, version) = resolve(&index, spec)?;
    let dir = dir.unwrap_or_else(|| PathBuf::from(entry.short_name()));

    let occupied = fs::read_dir(&dir)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if occupied && !force {
        bail!(
            "{} already has files in it. Pick another folder or pass --force.",
            dir.display()
        );
    }

    let mut values = BTreeMap::new();
    for pair in vars {
        let (key, value) = pair.split_once('=').context("--var expects KEY=VALUE")?;
        if !version.variables.contains_key(key) {
            let known: Vec<&str> = version.variables.keys().map(String::as_str).collect();
            if known.is_empty() {
                bail!("{} has no variables", entry.name);
            }
            bail!(
                "{} has no variable `{key}` (it has: {})",
                entry.name,
                known.join(", ")
            );
        }
        values.insert(key.to_string(), value.to_string());
    }
    let archives = cache_dir()?.join("archives");
    let cached = archives.join(format!("{}.tar.gz", version.tarball.sha256));
    let bytes = match fs::read(&cached) {
        Ok(bytes) if fetch::verify(&bytes, &version.tarball.sha256).is_ok() => bytes,
        _ => {
            let bytes = fetch::get(&version.tarball.url)
                .with_context(|| format!("could not download {}", version.tarball.url))?;
            fetch::verify(&bytes, &version.tarball.sha256)?;
            fs::create_dir_all(&archives)?;
            fs::write(&cached, &bytes)?;
            bytes
        }
    };

    let w = ui::width();
    ui::show(ui::fetched(
        &entry.name,
        &version.version.to_string(),
        entry.engine,
        w,
    ));
    let interactive = !yes && style::interactive();
    for (key, var) in &version.variables {
        if values.contains_key(key) {
            continue;
        }
        let value = if interactive {
            style::input(&var.prompt, &var.default)?
        } else {
            var.default.clone()
        };
        values.insert(key.clone(), value);
    }

    fs::create_dir_all(&dir)?;
    let files = fetch::extract(&bytes, &dir)?;
    let missing = fetch::apply_variables(&dir, &version.variables, &values)?;
    fs::write(
        dir.join(".texshelf.lock"),
        format!(
            "template = \"{}\"\nversion = \"{}\"\nengine = \"{}\"\nsha256 = \"{}\"\n",
            entry.name, version.version, entry.engine, version.tarball.sha256
        ),
    )?;
    let repo = !no_git && git_init(&dir);

    let details = format!(
        "{} {}, {}{}",
        files.len(),
        if files.len() == 1 { "file" } else { "files" },
        entry.license,
        if repo { ", git repository" } else { "" }
    );
    let hint = entry.compile_hint(version);
    let next = if dir == Path::new(".") {
        hint
    } else {
        format!("cd {} && {hint}", dir.display())
    };
    ui::show(ui::created(
        dir.display().to_string().trim_end_matches('/'),
        &details,
        &next,
        w,
    ));
    for key in missing {
        style::note(format!(
            "the placeholder for `{key}` was not found, so it was left unchanged"
        ));
    }
    if version.shell_escape {
        style::note("this template needs shell-escape, which lets the document run commands");
    }
    if let Some(tap) = entry.tap() {
        style::note(format!(
            "this template comes from the tap `{tap}`, which texshelf does not review"
        ));
    }
    Ok(())
}

/// What compiles the template: pdflatex, xelatex, lualatex or typst.
fn built_with(entry: &Entry) -> String {
    entry
        .version(None)
        .ok()
        .and_then(|v| v.compiler.clone())
        .unwrap_or_else(|| entry.engine.to_string())
}

#[allow(clippy::too_many_arguments)]
pub fn search(
    text: &str,
    engine: Option<Engine>,
    sort: Option<Sort>,
    category: Option<&str>,
    license: Option<&str>,
    official_only: bool,
    universities: &[String],
    limit: usize,
    json: bool,
    bin: &str,
) -> Result<()> {
    if let Some(c) = category {
        if !CATEGORIES.contains(&c) {
            bail!("unknown category `{c}` (known: {})", CATEGORIES.join(", "));
        }
    }
    let (index, stats) = load()?;
    let query = Query {
        text,
        engine,
        category,
        license,
        official_only,
        universities,
    };
    let sort = sort.unwrap_or(if text.trim().is_empty() {
        Sort::Official
    } else {
        Sort::Relevance
    });
    let hits = texshelf_core::index::search(&index, &stats, &query, sort);
    let shown = &hits[..hits.len().min(limit)];
    let stat = |e: &Entry| stats.templates.get(&e.name).copied().unwrap_or_default();

    if json {
        let rows: Vec<serde_json::Value> = shown
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name, "engine": e.engine, "title": e.title,
                    "description": e.description, "license": e.license,
                    "categories": e.categories, "official": e.official,
                    "universities": e.universities, "compiler": built_with(e),
                    "latest": e.latest.to_string(), "updated": e.updated,
                    "stars": stat(e).stars, "downloads": stat(e).downloads,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if shown.is_empty() {
        println!("No templates match. Try fewer words, or `--engine all`.");
        return Ok(());
    }

    let rows: Vec<ui::Row> = shown
        .iter()
        .map(|e| ui::Row {
            name: e.name.clone(),
            engine: e.engine,
            compiler: built_with(e),
            category: e.categories.first().cloned().unwrap_or_default(),
            version: e.latest.to_string(),
            stars: stat(e).stars,
            license: e.license.clone(),
            description: e.description.clone(),
            official: e.official,
        })
        .collect();
    ui::show(ui::search(
        &rows,
        hits.len() - shown.len(),
        bin,
        ui::width(),
    ));
    Ok(())
}

pub fn info(bin: &str, spec: &str, json: bool) -> Result<()> {
    let (index, stats) = load()?;
    let (entry, version) = resolve(&index, spec)?;
    if json {
        println!("{}", serde_json::to_string_pretty(entry)?);
        return Ok(());
    }
    let stat = stats
        .templates
        .get(&entry.name)
        .copied()
        .unwrap_or_default();
    let mut fields: Vec<(&'static str, String, ui::Tone)> = Vec::new();
    let mut field = |label, value: String| fields.push((label, value, ui::Tone::Plain));
    field("Title", entry.title.clone());
    field(
        "Engine",
        match (&version.compiler, &version.typst) {
            (Some(c), _) => format!("{} ({c})", entry.engine),
            (None, Some(v)) => format!("{} ({v} or newer)", entry.engine),
            (None, None) => entry.engine.to_string(),
        },
    );
    if let Some(tap) = entry.tap() {
        field("Tap", format!("{tap} (not reviewed by texshelf)"));
    }
    if !entry.universities.is_empty() {
        field("University", entry.universities.join(", "));
    }
    field("License", entry.license.clone());
    field("Authors", entry.authors.join(", "));
    field("Categories", entry.categories.join(", "));
    if let Some(repo) = &entry.repository {
        field("Source", repo.clone());
    }
    field("Stars", ui::stars(stat.stars));
    field("Published", version.published.clone());
    field(
        "Versions",
        entry
            .versions
            .iter()
            .map(|v| v.version.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
    field(
        "Files",
        format!(
            "{} ({} kB packed), starting from {}",
            version.files.len(),
            version.tarball.size.div_ceil(1024),
            version.entrypoint
        ),
    );
    if let Some(audit) = &version.audit {
        let day = audit.at.split('T').next().unwrap_or(&audit.at);
        let (result, tone) = match audit.findings.len() {
            0 => ("passed".to_string(), ui::Tone::Good),
            n => (
                format!(
                    "{} ({n} finding{})",
                    audit.status,
                    if n == 1 { "" } else { "s" }
                ),
                ui::Tone::Bad,
            ),
        };
        fields.push(("Audit", format!("{result}, checked {day}"), tone));
    }
    let card = ui::Card {
        name: entry.name.clone(),
        version: version.version.to_string(),
        official: entry.official,
        description: entry.description.clone(),
        fields,
        variables: version
            .variables
            .iter()
            .map(|(key, var)| {
                (
                    key.clone(),
                    format!("{} (default: {})", var.prompt, var.default),
                )
            })
            .collect(),
        init: format!("{bin} init {}", entry.name),
        link: entry
            .tap()
            .is_none()
            .then(|| format!("{SITE_URL}/t/{}", entry.name)),
    };
    ui::show(ui::info(&card, ui::width()));
    Ok(())
}

pub(crate) fn launch(target: &str) {
    println!("{target}");
    if let Err(err) = open::that(target) {
        style::warning(format!("could not open it automatically ({err})"));
    }
}

pub fn view(spec: &str, web: bool) -> Result<()> {
    let (index, _) = load()?;
    let (entry, version) = resolve(&index, spec)?;
    if web {
        if entry.tap().is_some() {
            bail!(
                "{} comes from a tap, so it has no page on {SITE_URL}",
                entry.name
            );
        }
        launch(&format!("{SITE_URL}/t/{}", entry.name));
        return Ok(());
    }
    let Some(url) = &version.preview else {
        bail!(
            "{} has no prebuilt preview. Try `view {} --web`.",
            entry.name,
            entry.name
        );
    };
    let dir = cache_dir()?.join("previews");
    let path = dir.join(format!(
        "{}-{}.pdf",
        entry.name.replace('/', "-"),
        version.version
    ));
    if !path.exists() {
        let bytes = fetch::get(url).with_context(|| format!("could not download {url}"))?;
        fs::create_dir_all(&dir)?;
        fs::write(&path, bytes)?;
    }
    launch(&path.display().to_string());
    Ok(())
}

/// The link that opens this template in its engine's web editor, if one exists.
pub fn editor_link(entry: &Entry, version: &VersionEntry) -> Option<String> {
    match entry.engine {
        Engine::Latex => {
            let mut link = format!("https://www.overleaf.com/docs?snip_uri={}", version.zip.url);
            if let Some(compiler) = version.compiler.as_deref().filter(|c| *c != "pdflatex") {
                link.push_str(&format!("&engine={compiler}"));
            }
            Some(link)
        }
        Engine::Typst => entry
            .universe
            .as_ref()
            .map(|name| format!("https://typst.app/?template={name}&version=latest")),
    }
}

pub fn open(spec: &str, print: bool) -> Result<()> {
    let (index, _) = load()?;
    let (entry, version) = resolve(&index, spec)?;
    let Some(link) = editor_link(entry, version) else {
        bail!(
            "{} is not on Typst Universe, so the Typst web app cannot open it. Use `init {}` instead.",
            entry.name,
            entry.name
        );
    };
    if print {
        println!("{link}");
    } else {
        launch(&link);
    }
    Ok(())
}

pub fn update() -> Result<()> {
    let store = store()?;
    let count = store.refresh()?;
    println!(
        "{}",
        ui::done(
            "Updated",
            &format!("{count} templates in the index ({})", store.url)
        )
    );
    for (name, tap) in crate::taps::stores(&cache_dir()?)? {
        match tap.refresh() {
            Ok(count) => println!(
                "{}",
                ui::done(
                    "Updated",
                    &format!("{count} templates in the tap `{name}` ({})", tap.url)
                )
            ),
            Err(err) => style::warning(format!("could not refresh the tap `{name}` ({err:#})")),
        }
    }
    Ok(())
}

pub fn validate(path: &Path) -> Result<()> {
    let text =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let manifest = Manifest::parse(&text)?;
    let errors = manifest.validate();
    if !errors.is_empty() {
        bail!("{} problem(s):\n  {}", errors.len(), errors.join("\n  "));
    }
    println!(
        "{}",
        ui::done(
            "Valid",
            &format!("{} {}", manifest.template.name, manifest.template.version)
        )
    );
    Ok(())
}
