use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use flate2::{write::GzEncoder, Compression};

fn archive() -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (path, body) in [
        (
            "main.tex",
            "\\documentclass{article}\n\\title{An example title}\n",
        ),
        ("sections/intro.tex", "Hello.\n"),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, path, body.as_bytes())
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn registry(dir: &Path, sha256: &str) {
    let tarball = dir.join("plain.tar.gz");
    let artifact = serde_json::json!({ "url": tarball, "sha256": sha256, "size": 1 });
    let index = serde_json::json!({
        "schema": 1,
        "generated_at": "2026-11-14T00:00:00Z",
        "templates": [{
            "name": "plain-article", "engine": "latex", "title": "Plain article",
            "description": "A short article with one section.", "authors": ["Someone"],
            "license": "MIT", "categories": ["article"], "official": true,
            "universities": ["UC Chile"],
            "latest": "1.0.0", "updated": "2026-11-14",
            "versions": [{
                "version": "1.0.0", "published": "2026-11-14", "entrypoint": "main.tex",
                "compiler": "xelatex", "bib": "none",
                "variables": { "title": { "prompt": "Title", "default": "An example title", "files": ["main.tex"] } },
                "tarball": artifact, "zip": artifact,
                "files": ["main.tex", "sections/intro.tex"]
            }]
        }, {
            "name": "typ-notes", "engine": "typst", "title": "Notes",
            "description": "Lecture notes.", "authors": ["Someone"], "license": "MIT",
            "categories": ["notes"], "latest": "0.1.0", "updated": "2026-10-01",
            "versions": []
        }]
    });
    fs::write(dir.join("index.json"), index.to_string()).unwrap();
}

fn run(bin: &str, home: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .env("TEXSHELF_INDEX_URL", home.join("index.json"))
        .env("TEXSHELF_CACHE_DIR", home.join("cache"))
        .env("TEXSHELF_CONFIG_DIR", home.join("config"))
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn setup() -> (tempfile::TempDir, tempfile::TempDir) {
    let home = tempfile::tempdir().unwrap();
    let bytes = archive();
    fs::write(home.path().join("plain.tar.gz"), &bytes).unwrap();
    registry(home.path(), &texshelf_core::fetch::sha256_hex(&bytes));
    (home, tempfile::tempdir().unwrap())
}

const TEXSHELF: &str = env!("CARGO_BIN_EXE_texshelf");
const TYPSHELF: &str = env!("CARGO_BIN_EXE_typshelf");

#[test]
fn init_creates_a_project_and_fills_variables() {
    let (home, work) = setup();
    let out = run(
        TEXSHELF,
        home.path(),
        work.path(),
        &[
            "init",
            "plain-article",
            "report",
            "--var",
            "title=Espectroscopia",
            "--no-git",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("Created report/ from plain-article 1.0.0 (MIT, 2 files)"),
        "{stdout}"
    );
    assert!(stdout.contains("latexmk -xelatex main.tex"), "{stdout}");

    let project = work.path().join("report");
    assert!(fs::read_to_string(project.join("main.tex"))
        .unwrap()
        .contains("\\title{Espectroscopia}"));
    assert!(project.join("sections/intro.tex").exists());
    assert!(fs::read_to_string(project.join(".texshelf.lock"))
        .unwrap()
        .contains("version = \"1.0.0\""));
    assert!(!project.join(".git").exists());

    let again = run(
        TEXSHELF,
        home.path(),
        work.path(),
        &["init", "plain-article", "report", "--yes"],
    );
    assert!(!again.status.success());
    assert!(text(&again.stderr).contains("already has files"));
}

#[test]
fn init_refuses_a_tampered_archive() {
    let (home, work) = setup();
    fs::write(home.path().join("plain.tar.gz"), b"not the archive").unwrap();
    let out = run(
        TEXSHELF,
        home.path(),
        work.path(),
        &["init", "plain-article", "--yes"],
    );
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("checksum mismatch"));
    assert!(!work.path().join("plain-article").exists());
}

#[test]
fn init_rejects_unknown_names_versions_and_variables() {
    let (home, work) = setup();
    let cases: [(&[&str], &str); 3] = [
        (&["init", "plain"], "Did you mean: plain-article?"),
        (&["init", "plain-article@2.0.0"], "has no version 2.0.0"),
        (
            &["init", "plain-article", "--var", "colour=red"],
            "has no variable `colour`",
        ),
    ];
    for (args, expected) in cases {
        let out = run(TEXSHELF, home.path(), work.path(), args);
        assert!(!out.status.success());
        assert!(
            text(&out.stderr).contains(expected),
            "{}",
            text(&out.stderr)
        );
    }
}

#[test]
fn each_command_defaults_to_its_own_engine() {
    let (home, work) = setup();
    let latex = text(&run(TEXSHELF, home.path(), work.path(), &["search"]).stdout);
    assert!(
        latex.contains("plain-article") && !latex.contains("typ-notes"),
        "{latex}"
    );
    let typst = text(&run(TYPSHELF, home.path(), work.path(), &["search"]).stdout);
    assert!(
        typst.contains("typ-notes") && !typst.contains("plain-article"),
        "{typst}"
    );
    let all = text(
        &run(
            TYPSHELF,
            home.path(),
            work.path(),
            &["search", "--engine", "all", "--json"],
        )
        .stdout,
    );
    let rows: serde_json::Value = serde_json::from_str(&all).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
}

#[test]
fn search_filters_by_university_and_shows_the_compiler() {
    let (home, work) = setup();
    let search = |args: &[&str]| text(&run(TEXSHELF, home.path(), work.path(), args).stdout);
    let hit = search(&["search", "--university", "Cambridge", "uc chile"]);
    assert!(
        hit.contains("plain-article") && hit.contains("xelatex"),
        "{hit}"
    );
    let miss = search(&["search", "--university", "Cambridge"]);
    assert!(miss.contains("No templates match"), "{miss}");
}

#[test]
fn open_prints_an_overleaf_link_with_the_compiler() {
    let (home, work) = setup();
    let out = run(
        TEXSHELF,
        home.path(),
        work.path(),
        &["open", "plain-article", "--print"],
    );
    let link = text(&out.stdout);
    assert!(
        link.starts_with("https://www.overleaf.com/docs?snip_uri="),
        "{link}"
    );
    assert!(link.trim_end().ends_with("&engine=xelatex"), "{link}");
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn new_template_then_publish_builds_a_registry_manifest() {
    let (home, work) = setup();
    let repo = work.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:someone/templates.git",
        ],
    );

    let made = run(
        TEXSHELF,
        home.path(),
        &repo,
        &["new-template", "lab-report", "--category", "lab-report"],
    );
    assert!(made.status.success(), "{}", text(&made.stderr));
    let dir = repo.join("lab-report");
    assert!(dir.join("main.tex").exists() && dir.join("texshelf.toml").exists());

    let dirty = run(TEXSHELF, home.path(), &dir, &["publish", "--print"]);
    assert!(
        text(&dirty.stderr).contains("uncommitted changes"),
        "{}",
        text(&dirty.stderr)
    );

    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "Add template"]);
    let unpushed = run(TEXSHELF, home.path(), &dir, &["publish", "--print"]);
    assert!(
        text(&unpushed.stderr).contains("git push"),
        "{}",
        text(&unpushed.stderr)
    );

    git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let out = run(TEXSHELF, home.path(), &dir, &["publish", "--print"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let printed = text(&out.stdout);
    assert!(
        printed.starts_with("templates/lab-report/0.1.0.toml"),
        "{printed}"
    );
    assert!(printed.contains("repo = \"https://github.com/someone/templates\""));
    assert!(printed.contains("path = \"lab-report\""));
    assert!(printed.contains("exclude = [\"texshelf.toml\"]"));
    assert!(!printed.contains("[registry]"));
    assert!(printed.contains("https://github.com/texshelf/registry/new/main?filename=templates%2Flab-report%2F0.1.0.toml&value=schema"));
}

fn tap_registry(dir: &Path, tarball: &Path, sha256: &str) {
    let artifact = serde_json::json!({ "url": tarball, "sha256": sha256, "size": 1 });
    let template = |name: &str| {
        serde_json::json!({
            "name": name, "engine": "latex", "title": "Lab report",
            "description": "Report for the second-year lab.", "authors": ["Someone"],
            "license": "MIT", "categories": ["lab-report"], "official": true,
            "latest": "2.0.0", "updated": "2026-11-15",
            "versions": [{
                "version": "2.0.0", "published": "2026-11-15", "entrypoint": "main.tex",
                "tarball": artifact, "zip": artifact, "files": ["main.tex"]
            }]
        })
    };
    let index = serde_json::json!({
        "schema": 1,
        "generated_at": "2026-11-15T00:00:00Z",
        "templates": [template("lab-report"), template("plain-article")]
    });
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("index.json"), index.to_string()).unwrap();
}

#[test]
fn a_tap_adds_templates_under_its_own_name() {
    let (home, work) = setup();
    let bytes = fs::read(home.path().join("plain.tar.gz")).unwrap();
    let tap = home.path().join("uc");
    tap_registry(
        &tap,
        &home.path().join("plain.tar.gz"),
        &texshelf_core::fetch::sha256_hex(&bytes),
    );
    let run = |args: &[&str]| run(TEXSHELF, home.path(), work.path(), args);

    let refused = run(&["tap", "add", "UC", tap.to_str().unwrap()]);
    assert!(!refused.status.success());
    let added = run(&["tap", "add", "uc", tap.to_str().unwrap()]);
    assert!(added.status.success(), "{}", text(&added.stderr));
    assert!(text(&added.stdout).contains("2 templates"));
    assert!(text(&run(&["tap", "list"]).stdout).contains("index.json"));

    let found = text(&run(&["search", "lab"]).stdout);
    assert!(found.contains("uc/lab-report"), "{found}");
    assert!(!found.contains("(official)"), "{found}");
    let info = text(&run(&["info", "uc/lab-report"]).stdout);
    assert!(info.contains("not reviewed"), "{info}");
    assert!(!info.contains("/t/uc"), "{info}");

    let made = run(&["init", "lab-report", "--yes", "--no-git"]);
    assert!(made.status.success(), "{}", text(&made.stderr));
    assert!(text(&made.stdout).contains("comes from the tap `uc`"));
    assert!(work.path().join("lab-report/main.tex").exists());

    let main = run(&["info", "plain-article"]);
    assert!(text(&main.stdout).contains("(official)"));
    let theirs = run(&["init", "uc/plain-article", "theirs", "--yes", "--no-git"]);
    assert!(theirs.status.success(), "{}", text(&theirs.stderr));

    assert!(run(&["tap", "remove", "uc"]).status.success());
    assert!(!run(&["info", "lab-report"]).status.success());
    assert!(!run(&["tap", "remove", "uc"]).status.success());
}

#[test]
fn a_broken_tap_does_not_stop_the_main_registry() {
    let (home, work) = setup();
    let config = home.path().join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("config.toml"),
        "[taps]\ngone = \"/no/such/index.json\"\n",
    )
    .unwrap();
    let out = run(TEXSHELF, home.path(), work.path(), &["search", "plain"]);
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("plain-article"));
    assert!(text(&out.stderr).contains("skipping tap `gone`"));
}

#[test]
fn tex_install_lists_choices_and_prints_commands() {
    let (home, work) = setup();
    let listed = run(TEXSHELF, home.path(), work.path(), &["tex", "install"]);
    assert!(listed.status.success());
    let out = text(&listed.stdout);
    assert!(out.contains("tectonic") && out.contains("typst"), "{out}");
    assert!(out.contains("tex install <name>"), "{out}");

    let typst_first = run(TYPSHELF, home.path(), work.path(), &["tex", "install"]);
    assert!(text(&typst_first.stdout).contains("1. typst"));

    let printed = run(
        TEXSHELF,
        home.path(),
        work.path(),
        &["tex", "install", "typst", "--print"],
    );
    assert!(printed.status.success());
    assert!(!text(&printed.stdout).trim().is_empty());

    let unasked = run(
        TEXSHELF,
        home.path(),
        work.path(),
        &["tex", "install", "nope"],
    );
    assert!(text(&unasked.stderr).contains("choices:"));
}
