use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use flate2::{write::GzEncoder, Compression};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use texshelf_core::fetch::sha256_hex;
use texshelf_core::index::{Artifact, Entry, Index, VersionEntry};
use texshelf_core::manifest::{Compiler, Engine, Manifest, SCHEMA};
use walkdir::WalkDir;

use crate::util;

const MAX_BYTES: u64 = 25 * 1024 * 1024;
const MAX_FILES: usize = 2_000;
const COMPILE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(clap::Args)]
pub struct Args {
    /// Registry checkout containing templates/
    #[arg(long, default_value = ".")]
    registry: PathBuf,
    /// Output directory, served as /v1
    #[arg(long)]
    out: PathBuf,
    /// Public URL the output directory is served from
    #[arg(long, default_value = "https://tex.jjsm.science/v1")]
    base_url: String,
    /// Output of an earlier build; unchanged versions are copied from it
    #[arg(long)]
    previous: Option<PathBuf>,
    /// Use a local checkout for a repository: URL=PATH
    #[arg(long = "map", value_name = "URL=PATH")]
    maps: Vec<String>,
    /// Skip compiling previews
    #[arg(long)]
    no_preview: bool,
    /// Fail when a preview does not compile
    #[arg(long)]
    strict: bool,
    /// Build only these templates
    #[arg(long)]
    only: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    manifest_sha256: String,
    base_url: String,
    entry: VersionEntry,
}

pub struct Loaded {
    pub manifest: Manifest,
    pub sha256: String,
}

pub fn load_manifests(registry: &Path) -> Result<Vec<Loaded>> {
    let root = registry.join("templates");
    let mut loaded = Vec::new();
    let mut problems = Vec::new();
    let walker = WalkDir::new(&root)
        .min_depth(2)
        .max_depth(2)
        .sort_by_file_name();
    for item in walker {
        let item = item.with_context(|| format!("could not read {}", root.display()))?;
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let shown = path
            .strip_prefix(registry)
            .unwrap_or(path)
            .display()
            .to_string();
        let text = fs::read_to_string(path)?;
        let manifest = match Manifest::parse(&text) {
            Ok(m) => m,
            Err(err) => {
                problems.push(format!("{shown}: {err}"));
                continue;
            }
        };
        let dir = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        let stem = path.file_stem().and_then(|n| n.to_str());
        if dir != Some(manifest.template.name.as_str()) {
            problems.push(format!("{shown}: the folder name must match template.name"));
        }
        if stem != Some(manifest.template.version.to_string().as_str()) {
            problems.push(format!(
                "{shown}: the file name must match template.version"
            ));
        }
        problems.extend(
            manifest
                .validate()
                .into_iter()
                .map(|e| format!("{shown}: {e}")),
        );
        loaded.push(Loaded {
            manifest,
            sha256: sha256_hex(text.as_bytes()),
        });
    }
    if !problems.is_empty() {
        bail!(
            "{} problem(s) in the registry:\n  {}",
            problems.len(),
            problems.join("\n  ")
        );
    }
    if loaded.is_empty() {
        bail!("no manifests found under {}", root.display());
    }
    Ok(loaded)
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .context("could not run git")?;
    if !status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

fn checkout(
    repo: &str,
    rev: &str,
    maps: &BTreeMap<String, PathBuf>,
    work: &Path,
) -> Result<PathBuf> {
    if let Some(local) = maps.get(repo) {
        return Ok(local.clone());
    }
    let dir = work.join("src").join(rev);
    if dir.join(".git").exists() {
        return Ok(dir);
    }
    fs::create_dir_all(&dir)?;
    git(&dir, &["init", "-q"])?;
    git(&dir, &["fetch", "-q", "--depth", "1", repo, rev])
        .with_context(|| format!("could not fetch {rev} from {repo}"))?;
    git(&dir, &["checkout", "-q", "FETCH_HEAD"])?;
    Ok(dir)
}

fn globs(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        builder.add(Glob::new(p).with_context(|| format!("bad exclude pattern `{p}`"))?);
    }
    Ok(builder.build()?)
}

/// Lists the template's files as (relative path, absolute path), sorted.
fn stage(root: &Path, exclude: &[String]) -> Result<Vec<(String, PathBuf)>> {
    if !root.is_dir() {
        bail!("source.path does not exist in the repository");
    }
    let excluded = globs(exclude)?;
    let mut files = Vec::new();
    let mut total = 0u64;
    let walker = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git");
    for item in walker {
        let item = item?;
        let rel = item
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() || excluded.is_match(&rel) {
            continue;
        }
        let kind = item.file_type();
        if kind.is_symlink() {
            bail!("`{rel}` is a symlink; exclude it or replace it with a real file");
        }
        if kind.is_file() {
            total += item.metadata()?.len();
            files.push((rel, item.path().to_path_buf()));
        }
    }
    if files.len() > MAX_FILES {
        bail!("{} files, the limit is {MAX_FILES}", files.len());
    }
    if total > MAX_BYTES {
        bail!(
            "{:.1} MB, the limit is {} MB; exclude build output and large binaries",
            total as f64 / 1_048_576.0,
            MAX_BYTES / 1_048_576
        );
    }
    Ok(files)
}

fn tar_gz(files: &[(String, PathBuf)]) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::best()));
    for (rel, abs) in files {
        let data = fs::read(abs)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        builder.append_data(&mut header, rel, data.as_slice())?;
    }
    Ok(builder.into_inner()?.finish()?)
}

fn zip_archive(files: &[(String, PathBuf)]) -> Result<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (rel, abs) in files {
        writer.start_file(rel.as_str(), options)?;
        writer.write_all(&fs::read(abs)?)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn run_with_timeout(mut command: Command, log: &Path) -> Result<bool> {
    let out = File::create(log)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()
        .with_context(|| format!("could not start {:?}", command.get_program()))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if started.elapsed() > COMPILE_TIMEOUT {
            child.kill().ok();
            child.wait().ok();
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The lines of a build log worth showing: TeX errors if there are any,
/// otherwise the last few lines.
fn tail(log: &Path, lines: usize) -> String {
    let text = fs::read(log)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let errors: Vec<&str> = all
        .iter()
        .copied()
        .filter(|l| {
            l.starts_with('!')
                || l.contains("Error:")
                || l.contains("not found")
                || l.starts_with("error")
        })
        .take(lines)
        .collect();
    if errors.is_empty() {
        all[all.len().saturating_sub(lines)..].join("\n    ")
    } else {
        errors.join("\n    ")
    }
}

/// Compiles the template in a scratch copy. Returns the PDF and a PNG of page one.
fn preview(manifest: &Manifest, files: &[(String, PathBuf)]) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    let project = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    for (rel, abs) in files {
        let target = project.path().join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(abs, target)?;
        // \include writes its .aux next to the included file, under the output folder.
        if let Some(parent) = Path::new(rel).parent() {
            fs::create_dir_all(out.path().join(parent))?;
        }
    }
    let entry = &manifest.template.entrypoint;
    let stem = Path::new(entry)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    let log = out.path().join("build.log");
    let pdf = out.path().join(format!("{stem}.pdf"));

    let mut command;
    match manifest.template.engine {
        Engine::Typst => {
            command = Command::new("typst");
            command
                .args(["compile", "--root", "."])
                .arg(entry)
                .arg(&pdf);
        }
        Engine::Latex => {
            let latex = manifest.latex.clone().unwrap_or_default();
            command = Command::new("latexmk");
            command
                .arg(match latex.compiler {
                    Compiler::Pdflatex => "-pdf",
                    Compiler::Xelatex => "-xelatex",
                    Compiler::Lualatex => "-lualatex",
                })
                .args([
                    "-interaction=nonstopmode",
                    "-halt-on-error",
                    "-file-line-error",
                ])
                .arg(if latex.shell_escape {
                    "-shell-escape"
                } else {
                    "-no-shell-escape"
                })
                .arg(format!("-outdir={}", out.path().display()))
                .arg(entry);
        }
    }
    command.current_dir(project.path());
    if !run_with_timeout(command, &log)? || !pdf.exists() {
        bail!("the preview did not compile:\n    {}", tail(&log, 6));
    }
    let bytes = fs::read(&pdf)?;

    let thumb = out.path().join("thumb");
    let rendered = Command::new("pdftoppm")
        .args([
            "-png",
            "-singlefile",
            "-f",
            "1",
            "-scale-to-x",
            "640",
            "-scale-to-y",
            "-1",
        ])
        .arg(&pdf)
        .arg(&thumb)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let png = rendered
        .then(|| fs::read(thumb.with_extension("png")).ok())
        .flatten();
    Ok((bytes, png))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for item in fs::read_dir(from)? {
        let item = item?;
        if item.file_type()?.is_file() {
            fs::copy(item.path(), to.join(item.file_name()))?;
        }
    }
    Ok(())
}

fn build_version(
    loaded: &Loaded,
    args: &Args,
    maps: &BTreeMap<String, PathBuf>,
    work: &Path,
    published: String,
    dest: &Path,
) -> Result<VersionEntry> {
    let m = &loaded.manifest;
    let t = &m.template;
    let source = checkout(&m.source.repo, &m.source.rev, maps, work)?;
    let root = if m.source.path == "." {
        source
    } else {
        source.join(&m.source.path)
    };
    let files = stage(&root, &m.source.exclude)?;
    let names: Vec<String> = files.iter().map(|(rel, _)| rel.clone()).collect();

    if !names.contains(&t.entrypoint) {
        bail!(
            "entrypoint `{}` is not among the packed files",
            t.entrypoint
        );
    }
    for (key, var) in &m.variables {
        let mut found = false;
        for file in &var.files {
            let Some((_, abs)) = files.iter().find(|(rel, _)| rel == file) else {
                bail!("variable `{key}`: `{file}` is not among the packed files");
            };
            found |= fs::read_to_string(abs)
                .map(|s| s.contains(var.needle()))
                .unwrap_or(false);
        }
        if !found {
            bail!(
                "variable `{key}`: the text `{}` does not appear in its files",
                var.needle()
            );
        }
    }

    fs::create_dir_all(dest)?;
    let base = format!(
        "{}/t/{}/{}",
        args.base_url.trim_end_matches('/'),
        t.name,
        t.version
    );
    let stem = format!("{}-{}", t.name, t.version);
    let write = |ext: &str, bytes: &[u8]| -> Result<Artifact> {
        let file = format!("{stem}.{ext}");
        fs::write(dest.join(&file), bytes)?;
        Ok(Artifact {
            url: format!("{base}/{file}"),
            sha256: sha256_hex(bytes),
            size: bytes.len() as u64,
        })
    };
    let tarball = write("tar.gz", &tar_gz(&files)?)?;
    let zip = write("zip", &zip_archive(&files)?)?;

    let mut preview_url = None;
    let mut thumbnail_url = None;
    if !args.no_preview {
        match preview(m, &files) {
            Ok((pdf, png)) => {
                fs::write(dest.join("preview.pdf"), pdf)?;
                preview_url = Some(format!("{base}/preview.pdf"));
                if let Some(png) = png {
                    fs::write(dest.join("thumb.png"), png)?;
                    thumbnail_url = Some(format!("{base}/thumb.png"));
                }
            }
            Err(err) if args.strict => return Err(err),
            Err(err) => eprintln!("  warning: {err:#}"),
        }
    }

    let latex = (t.engine == Engine::Latex).then(|| m.latex.clone().unwrap_or_default());
    Ok(VersionEntry {
        version: t.version.clone(),
        published,
        entrypoint: t.entrypoint.clone(),
        compiler: latex.as_ref().map(|l| l.compiler.to_string()),
        bib: latex.as_ref().map(|l| l.bib.to_string()),
        shell_escape: latex.map(|l| l.shell_escape).unwrap_or(false),
        variables: m.variables.clone(),
        tarball,
        zip,
        preview: preview_url,
        thumbnail: thumbnail_url,
        files: names,
    })
}

pub fn run(args: Args) -> Result<()> {
    let mut maps = BTreeMap::new();
    for pair in &args.maps {
        let (url, path) = pair.split_once('=').context("--map expects URL=PATH")?;
        maps.insert(url.to_string(), PathBuf::from(path));
    }
    let manifests = load_manifests(&args.registry)?;
    let work = tempfile::tempdir()?;
    let today = util::today();
    let mut versions: BTreeMap<String, Vec<(&Loaded, VersionEntry)>> = BTreeMap::new();
    let mut failures = Vec::new();
    let (mut built, mut reused) = (0, 0);

    for loaded in &manifests {
        let t = &loaded.manifest.template;
        if !args.only.is_empty() && !args.only.contains(&t.name) {
            continue;
        }
        let sub = Path::new("t").join(&t.name).join(t.version.to_string());
        let dest = args.out.join(&sub);
        let old: Option<Meta> = args
            .previous
            .as_ref()
            .and_then(|p| fs::read(p.join(&sub).join("meta.json")).ok())
            .and_then(|b| serde_json::from_slice(&b).ok());

        let unchanged = old.as_ref().is_some_and(|o| {
            o.manifest_sha256 == loaded.sha256
                && o.base_url == args.base_url
                && (args.no_preview || o.entry.preview.is_some())
        });
        if let (true, Some(old), Some(previous)) = (unchanged, &old, &args.previous) {
            if previous.join(&sub) != dest {
                copy_dir(&previous.join(&sub), &dest)?;
            }
            versions
                .entry(t.name.clone())
                .or_default()
                .push((loaded, old.entry.clone()));
            reused += 1;
            continue;
        }

        println!("building {} {}", t.name, t.version);
        let published = old
            .map(|o| o.entry.published)
            .unwrap_or_else(|| today.clone());
        match build_version(loaded, &args, &maps, work.path(), published, &dest) {
            Ok(entry) => {
                let meta = Meta {
                    manifest_sha256: loaded.sha256.clone(),
                    base_url: args.base_url.clone(),
                    entry: entry.clone(),
                };
                fs::write(dest.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
                versions
                    .entry(t.name.clone())
                    .or_default()
                    .push((loaded, entry));
                built += 1;
            }
            Err(err) => failures.push(format!("{} {}: {err:#}", t.name, t.version)),
        }
    }
    if !failures.is_empty() {
        bail!(
            "{} template(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    let mut templates = Vec::new();
    for (name, mut list) in versions {
        list.sort_by(|a, b| b.1.version.cmp(&a.1.version));
        let (newest, entry) = &list[0];
        let t = &newest.manifest.template;
        templates.push(Entry {
            name,
            engine: t.engine,
            title: t.title.clone(),
            description: t.description.clone(),
            authors: t.authors.clone(),
            license: t.license.clone(),
            repository: t
                .repository
                .clone()
                .or_else(|| Some(newest.manifest.source.repo.clone())),
            keywords: t.keywords.clone(),
            categories: t.categories.clone(),
            official: newest.manifest.registry.official,
            verified: newest.manifest.registry.verified,
            universe: t.universe.clone(),
            latest: entry.version.clone(),
            updated: entry.published.clone(),
            versions: list.into_iter().map(|(_, e)| e).collect(),
        });
    }
    let index = Index {
        schema: SCHEMA,
        generated_at: util::timestamp(),
        templates,
    };
    fs::create_dir_all(&args.out)?;
    fs::write(args.out.join("index.json"), serde_json::to_vec(&index)?)?;
    println!(
        "{} templates in the index ({built} built, {reused} reused)",
        index.templates.len()
    );
    Ok(())
}
