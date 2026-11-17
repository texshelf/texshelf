<img src="https://raw.githubusercontent.com/texshelf/brand/main/logo/lockup.svg" alt="texshelf" width="240">

[![Release](https://img.shields.io/github/v/release/texshelf/texshelf)](https://github.com/texshelf/texshelf/releases/latest)
[![crates.io](https://img.shields.io/crates/v/texshelf)](https://crates.io/crates/texshelf)
[![CI](https://github.com/texshelf/texshelf/actions/workflows/ci.yml/badge.svg)](https://github.com/texshelf/texshelf/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/texshelf/texshelf)](https://github.com/texshelf/texshelf/stargazers)

Start a LaTeX or Typst project from a template, the way `uv init` or
`cargo new` start a code project.

```console
$ texshelf search thesis
NAME                    ENGINE    CATEGORY  VERSION    STARS  DESCRIPTION
phd-thesis              pdflatex  thesis    2026.4.28  ★ 902  PhD thesis class following the Camb…
osuc-trabajo-de-titulo  pdflatex  thesis    2026.9.17  ★ 68   Degree project (trabajo de título)…

2 templates. Details: texshelf info <name>

$ texshelf init phd-thesis my-thesis
   Fetched phd-thesis 2026.4.28
  Verified sha256 matches the index
   Created my-thesis/ (59 files, MIT, git repository)
      Next cd my-thesis && latexmk -pdf thesis.tex
```

One install gives you two commands. `texshelf` lists LaTeX templates by default
and `typshelf` lists Typst ones. They are the same program otherwise, and either
one can create a project from any template. Browse the templates with previews
at [tex.jjsm.science](https://tex.jjsm.science).

## Star History

<a href="https://www.star-history.com/?repos=texshelf%2Ftexshelf&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=texshelf/texshelf&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=texshelf/texshelf&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=texshelf/texshelf&type=date&legend=top-left" />
 </picture>
</a>


## Install

### macOS and Linux

With Homebrew:

```console
brew install texshelf/tap/texshelf
```

Without Homebrew, run the installer. It downloads the latest release, checks
its SHA-256 and puts `texshelf` and `typshelf` in `~/.local/bin`. On Linux it
needs `curl` and `tar`, and builds exist for x86-64 and ARM64.

```console
curl -fsSL https://tex.jjsm.science/install.sh | sh
```

If it says the folder is not on your `PATH`, add it (to `~/.bashrc` instead
if that is your shell's file) and open a new terminal:

```console
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

### Windows

In PowerShell, without administrator mode:

```powershell
irm https://tex.jjsm.science/install.ps1 | iex
```

It downloads the latest release, checks its SHA-256, puts `texshelf.exe` and
`typshelf.exe` in `%LOCALAPPDATA%\texshelf\bin` and adds that folder to your
`PATH`. Open a new PowerShell window afterwards.

### From source

With [Rust](https://rustup.rs) 1.82 or newer:

```console
cargo install texshelf
```

Or the current `main` branch:

```console
cargo install --git https://github.com/texshelf/texshelf texshelf
```

Check the install with `texshelf --version`. Both installers read
`TEXSHELF_INSTALL_DIR` to install somewhere else and `TEXSHELF_VERSION` to pin
a release, such as `v0.2.1`. texshelf does not include a compiler:
`texshelf doctor` lists the TeX and Typst tools it finds, and
`texshelf tex install` installs one if you have none.

## Commands

The full guide is at
[docs.jjsm.science/latex/texshelf](https://docs.jjsm.science/latex/texshelf),
written in [texshelf/docs](https://github.com/texshelf/docs). What changed in
each release is in [CHANGELOG.md](CHANGELOG.md).

| Command | What it does |
| --- | --- |
| `init <name>[@version] [dir]` | Download a template, check its checksum, unpack it, fill in its variables and run `git init` |
| `search [words]` | Search the index. Filter with `--category`, `--license`, `--official`; order with `--sort stars\|newest\|name\|official` |
| `info <name>` | Show license, versions, variables and file count |
| `view <name>` | Open the prebuilt PDF preview, with no TeX install needed |
| `open <name>` | Open the template in Overleaf, or in the Typst web app when it is on Typst Universe |
| `update` | Refresh the local copy of the index (also happens by itself once a day) |
| `doctor [file.tex]` | List the TeX and Typst tools installed, and the packages a document uses that are missing |
| `tex install [name]` | Install TinyTeX, Tectonic, full TeX Live, MiKTeX or Typst with the vendor's own command |
| `tap add <name> <address>` | Use templates from another registry, such as your university's. Also `tap list` and `tap remove`. |
| `new-template <dir>` | Start a template of your own, with a starter document and `texshelf.toml` |
| `publish` | Submit the template in the current folder to the registry |
| `validate <manifest>` | Check a registry manifest before you submit it |
| `completions <shell>` | Print a completion script for bash, zsh, fish or PowerShell |

`--engine latex|typst|all` overrides which templates `search` shows. `search`
and `info` take `--json`.

`init` asks for each variable a template declares, such as the title. Pass
`--var title="Lab 3"` to answer in advance, or `--yes` to keep every default.
It never compiles anything; it prints the command that does.

## How it works

The [registry](https://github.com/texshelf/registry) is a Git repository with
one small manifest per template version. Each manifest points at a commit in
the author's own repository. CI fetches that commit, compiles it, and publishes
a `.tar.gz`, a `.zip`, a PDF preview and a thumbnail, along with an `index.json`
that lists everything.

The CLI downloads `index.json` once a day and searches it locally. `init`
downloads one archive and refuses to unpack it if its SHA-256 differs from the
one in the index. Archives may only contain regular files with relative paths.

| Variable | Purpose |
| --- | --- |
| `TEXSHELF_INDEX_URL` | Use another index. A local file path works too. |
| `TEXSHELF_CACHE_DIR` | Where the index, archives and previews are cached |
| `TEXSHELF_CONFIG_DIR` | Where `config.toml`, which lists your taps, is kept |

## Repository layout

| Crate | Contents |
| --- | --- |
| `crates/texshelf-core` | Manifest format and validation, index search, download and extraction |
| `crates/texshelf-cli` | The `texshelf` and `typshelf` binaries |
| `crates/texshelf-registry` | `texshelf-registry`, which builds the index and archives. The registry's CI runs it. |

```console
cargo test --workspace
cargo run --bin texshelf -- search cv
```

To try the CLI against templates you built yourself, build a registry into a
folder and point the CLI at it:

```console
cargo run --bin texshelf-registry -- build --registry ../registry --out /tmp/shelf/v1 --base-url /tmp/shelf/v1
TEXSHELF_INDEX_URL=/tmp/shelf/v1/index.json cargo run --bin texshelf -- init article
```

## Contributing

The quickest contribution is a template. Run `texshelf publish` in its folder,
or [request one](https://github.com/texshelf/registry/issues/new/choose) if it
is not yours. Bug reports and pull requests for the CLI are welcome in this
repository. The
[contributing guide](https://github.com/texshelf/.github/blob/main/CONTRIBUTING.md)
covers both, and everyone taking part follows the
[code of conduct](https://github.com/texshelf/.github/blob/main/CODE_OF_CONDUCT.md).

If texshelf saved you some setup time, a star helps. Homebrew accepts a
project into its main repository, where `brew install texshelf` works without
the tap prefix, once it has 225 of them.

## License

MIT or Apache-2.0, at your option. Templates keep their own licenses, and the
documents you write from them are yours.

Created and maintained by [Juan José Sánchez Medina](https://jjsm.science).
