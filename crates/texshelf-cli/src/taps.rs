use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use texshelf_core::index::{is_slug, Store};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    taps: BTreeMap<String, String>,
}

fn config_path() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("TEXSHELF_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("config.toml"));
    }
    let dirs = directories::ProjectDirs::from("science", "jjsm", "texshelf")
        .context("could not find a settings folder for this user")?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn read() -> Result<Config> {
    let path = config_path()?;
    match fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(err) => Err(err).with_context(|| format!("could not read {}", path.display())),
    }
}

fn write(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, toml::to_string(config)?)
        .with_context(|| format!("could not write {}", path.display()))
}

/// Turns what the user typed into the address of an `index.json`.
fn index_url(given: &str) -> Result<String> {
    let given = given.trim_end_matches('/');
    let url = if given.starts_with("https://") {
        given.to_string()
    } else if given.starts_with("http://") {
        bail!("a tap has to be served over https, because its index holds the checksums");
    } else {
        fs::canonicalize(given)
            .with_context(|| {
                format!("`{given}` is neither an https address nor a path that exists")
            })?
            .display()
            .to_string()
    };
    Ok(if url.ends_with(".json") {
        url
    } else {
        format!("{url}/index.json")
    })
}

/// Every tap as (name, store), in name order.
pub fn stores(cache: &Path) -> Result<Vec<(String, Store)>> {
    Ok(read()?
        .taps
        .into_iter()
        .filter(|(name, _)| is_slug(name))
        .map(|(name, url)| {
            let dir = cache.join("taps").join(&name);
            (name, Store::new(url, dir))
        })
        .collect())
}

pub fn add(cache: &Path, name: &str, given: &str) -> Result<()> {
    if !is_slug(name) || name.len() < 2 || name.len() > 20 {
        bail!("a tap name is 2 to 20 lowercase letters, digits or hyphens");
    }
    let mut config = read()?;
    let url = index_url(given)?;
    let store = Store::new(url.clone(), cache.join("taps").join(name));
    let count = store
        .refresh()
        .with_context(|| format!("`{url}` is not a texshelf index"))?;
    let replaced = config.taps.insert(name.to_string(), url.clone()).is_some();
    write(&config)?;
    println!(
        "{} tap `{name}` with {count} template{} ({url})",
        if replaced { "Updated" } else { "Added" },
        if count == 1 { "" } else { "s" }
    );
    println!("Its templates are listed as {name}/<template>.");
    println!("note: texshelf does not review or audit templates from taps");
    Ok(())
}

pub fn remove(cache: &Path, name: &str) -> Result<()> {
    let mut config = read()?;
    if config.taps.remove(name).is_none() {
        bail!("there is no tap named `{name}`");
    }
    write(&config)?;
    let dir = cache.join("taps").join(name);
    if is_slug(name) && dir.exists() {
        fs::remove_dir_all(&dir).ok();
    }
    println!("Removed tap `{name}`");
    Ok(())
}

pub fn list() -> Result<()> {
    let config = read()?;
    if config.taps.is_empty() {
        println!("No taps. Add one with: tap add <name> <address of its index.json>");
        return Ok(());
    }
    let width = config.taps.keys().map(String::len).max().unwrap_or(0);
    for (name, url) in &config.taps {
        println!("{name:<width$}  {url}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn completes_addresses() {
        let url = |given| super::index_url(given).unwrap();
        assert_eq!(
            url("https://a.example/v1/"),
            "https://a.example/v1/index.json"
        );
        assert_eq!(url("https://a.example/x.json"), "https://a.example/x.json");
        assert!(super::index_url("http://a.example/v1").is_err());
        assert!(super::index_url("/no/such/folder").is_err());
    }
}
