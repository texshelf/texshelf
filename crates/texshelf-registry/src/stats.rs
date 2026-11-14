use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use texshelf_core::index::{Index, Stat, Stats};

use crate::util;

#[derive(clap::Args)]
pub struct Args {
    /// index.json produced by `build`
    #[arg(long)]
    index: PathBuf,
    /// Where to write stats.json
    #[arg(long)]
    out: PathBuf,
    /// An earlier stats.json whose numbers are kept when a lookup fails
    #[arg(long)]
    previous: Option<PathBuf>,
    /// URL that returns download totals as a JSON object of template name to count
    #[arg(long)]
    downloads_url: Option<String>,
}

fn github_slug(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut parts = rest
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    Some(format!("{owner}/{repo}"))
}

fn stars(slug: &str, token: Option<&str>) -> Result<u64> {
    let mut request = ureq::get(&format!("https://api.github.com/repos/{slug}"))
        .timeout(Duration::from_secs(20))
        .set("User-Agent", "texshelf-registry")
        .set("Accept", "application/vnd.github+json");
    if let Some(token) = token {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let body: serde_json::Value = serde_json::from_str(&request.call()?.into_string()?)?;
    body["stargazers_count"]
        .as_u64()
        .context("the response has no stargazers_count")
}

fn downloads(url: &str) -> Result<BTreeMap<String, u64>> {
    let body = ureq::get(url)
        .timeout(Duration::from_secs(20))
        .set("User-Agent", "texshelf-registry")
        .call()?
        .into_string()?;
    Ok(serde_json::from_str(&body)?)
}

pub fn run(args: Args) -> Result<()> {
    let index: Index = serde_json::from_slice(&fs::read(&args.index)?)?;
    let previous: Stats = args
        .previous
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let token = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());
    let counted = match args.downloads_url.as_deref().map(downloads) {
        Some(Ok(map)) => map,
        Some(Err(err)) => {
            eprintln!("warning: no download counts: {err:#}");
            BTreeMap::new()
        }
        None => BTreeMap::new(),
    };

    let mut by_repo: BTreeMap<String, Option<u64>> = BTreeMap::new();
    let mut stats = Stats {
        generated_at: util::timestamp(),
        templates: BTreeMap::new(),
    };
    for entry in &index.templates {
        let old = previous
            .templates
            .get(&entry.name)
            .copied()
            .unwrap_or_default();
        let fresh = entry
            .repository
            .as_deref()
            .and_then(github_slug)
            .and_then(|slug| {
                *by_repo.entry(slug.clone()).or_insert_with(|| {
                    match stars(&slug, token.as_deref()) {
                        Ok(n) => Some(n),
                        Err(err) => {
                            eprintln!("warning: no star count for {slug}: {err:#}");
                            None
                        }
                    }
                })
            });
        stats.templates.insert(
            entry.name.clone(),
            Stat {
                stars: fresh.unwrap_or(old.stars),
                downloads: counted.get(&entry.name).copied().unwrap_or(old.downloads),
            },
        );
    }
    fs::write(&args.out, serde_json::to_vec_pretty(&stats)?)?;
    println!("wrote stats for {} templates", stats.templates.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn slugs() {
        assert_eq!(
            super::github_slug("https://github.com/posquit0/Awesome-CV").as_deref(),
            Some("posquit0/Awesome-CV")
        );
        assert_eq!(
            super::github_slug("https://github.com/a/b.git/").as_deref(),
            Some("a/b")
        );
        assert_eq!(super::github_slug("https://gitlab.com/a/b"), None);
    }
}
