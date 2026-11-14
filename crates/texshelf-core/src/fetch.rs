use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use crate::manifest::Variable;

const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

/// Reads a URL. Plain paths and `file://` URLs are read from disk, which is
/// how tests and local registries work.
pub fn get(url: &str) -> Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return Ok(fs::read(path)?);
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok(fs::read(url)?);
    }
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(!bypasses_proxy(url))
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("texshelf/", env!("CARGO_PKG_VERSION")))
        .build();
    let response = agent.get(url).call().map_err(|err| match err {
        ureq::Error::Status(code, _) => anyhow::anyhow!("the server answered {code} for {url}"),
        other => anyhow::anyhow!("{other}"),
    })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        bail!("{url} is larger than {} MB", MAX_DOWNLOAD / 1024 / 1024);
    }
    Ok(bytes)
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    host.split(':').next().unwrap_or("")
}

/// True for local addresses and for hosts listed in `NO_PROXY`.
fn bypasses_proxy(url: &str) -> bool {
    let host = host(url).to_ascii_lowercase();
    if host == "localhost" || host == "127.0.0.1" {
        return true;
    }
    let list = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    list.split(',')
        .map(|entry| entry.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| entry == "*" || host == entry || host.ends_with(&format!(".{entry}")))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn verify(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("checksum mismatch: expected {expected}, got {actual}. Nothing was written.");
    }
    Ok(())
}

/// Unpacks a `.tar.gz` into `dest`. Only regular files and directories with
/// plain relative paths are accepted; anything else aborts the extraction.
pub fn extract(archive: &[u8], dest: &Path) -> Result<Vec<String>> {
    let mut tar = tar::Archive::new(GzDecoder::new(archive));
    let mut written = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if !path.components().all(|c| matches!(c, Component::Normal(_))) {
            bail!("the archive contains an unsafe path: {}", path.display());
        }
        let kind = entry.header().entry_type();
        let target = dest.join(&path);
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data)?;
            fs::write(&target, data)
                .with_context(|| format!("could not write {}", target.display()))?;
            written.push(path.to_string_lossy().replace('\\', "/"));
        } else {
            bail!(
                "the archive contains a link or special file: {}",
                path.display()
            );
        }
    }
    Ok(written)
}

/// Replaces each variable's placeholder text in its files. Returns the names
/// of variables whose text was not found anywhere.
pub fn apply_variables(
    dir: &Path,
    variables: &BTreeMap<String, Variable>,
    values: &BTreeMap<String, String>,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for (key, var) in variables {
        let Some(value) = values.get(key) else {
            continue;
        };
        if value == var.needle() {
            continue;
        }
        let mut found = false;
        for file in &var.files {
            let path = dir.join(file);
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if text.contains(var.needle()) {
                fs::write(&path, text.replace(var.needle(), value))?;
                found = true;
            }
        }
        if !found {
            missing.push(key.clone());
        }
    }
    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};

    fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (path, body) in entries {
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

    #[test]
    fn extracts_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let files = extract(
            &archive(&[("main.tex", "hi"), ("sec/a.tex", "a")]),
            dir.path(),
        )
        .unwrap();
        assert_eq!(files, ["main.tex", "sec/a.tex"]);
        assert_eq!(
            fs::read_to_string(dir.path().join("sec/a.tex")).unwrap(),
            "a"
        );
    }

    #[test]
    fn refuses_links() {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder
            .append_link(&mut header, "evil", "/etc/passwd")
            .unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(extract(&bytes, dir.path()).is_err());
    }

    #[test]
    fn reads_the_host_of_a_url() {
        assert_eq!(
            host("https://tex.jjsm.science/v1/index.json"),
            "tex.jjsm.science"
        );
        assert_eq!(host("http://user@localhost:8765/x"), "localhost");
        assert!(bypasses_proxy("http://127.0.0.1:8000/index.json"));
    }

    #[test]
    fn checksum_must_match() {
        assert!(verify(b"abc", &sha256_hex(b"abc")).is_ok());
        assert!(verify(b"abc", &sha256_hex(b"abd")).is_err());
    }

    #[test]
    fn replaces_variables() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.tex"), "\\title{An example title}").unwrap();
        let var = |default: &str| Variable {
            prompt: "?".into(),
            default: default.into(),
            pattern: None,
            files: vec!["main.tex".into()],
        };
        let variables = BTreeMap::from([
            ("title".to_string(), var("An example title")),
            ("author".to_string(), var("Nobody")),
        ]);
        let values = BTreeMap::from([
            ("title".to_string(), "Espectroscopia".to_string()),
            ("author".to_string(), "JJ".to_string()),
        ]);
        let missing = apply_variables(dir.path(), &variables, &values).unwrap();
        assert_eq!(missing, ["author"]);
        assert_eq!(
            fs::read_to_string(dir.path().join("main.tex")).unwrap(),
            "\\title{Espectroscopia}"
        );
    }
}
