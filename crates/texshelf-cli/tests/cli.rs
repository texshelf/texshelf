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
