# Command reference

Every command works the same in `texshelf` and `typshelf`. The only difference
is the default for `--engine`: `texshelf` shows LaTeX templates and `typshelf`
shows Typst ones.

## init

```console
texshelf init <name>[@version] [dir]
```

Creates a project from a template. Without `dir` the folder is named after the
template. Without `@version` you get the latest version.

| Option | Effect |
| --- | --- |
| `--var KEY=VALUE` | Set a template variable. Repeat it for several. |
| `-y`, `--yes` | Keep the default for every variable not set with `--var` |
| `--no-git` | Do not run `git init` |
| `--force` | Write into a folder that already has files |

`init` downloads the archive, compares its SHA-256 with the index, and unpacks
it only if they match. It then replaces each variable's placeholder text and
writes `.texshelf.lock`, which records the template, version and checksum. If
the folder is already inside a Git repository, `git init` is skipped.

Variables are asked for one by one when the terminal is interactive. In a
script, or with `--yes`, the defaults are used.

## search

```console
texshelf search [words] [--sort stars|newest|name|official|downloads|relevance]
```

Every word has to match the name, title, keywords, categories or description.
Without words it lists everything, official templates first.

| Option | Effect |
| --- | --- |
| `--category <c>` | One of: article, journal, thesis, report, lab-report, homework, exam, presentation, poster, cv, letter, book, notes, cheatsheet, other |
| `--license <id>` | An SPDX identifier such as `MIT` |
| `--official` | Only templates maintained by the texshelf project |
| `--university <name>...` | Only templates made for one of these universities, for example `--university "UC Chile" Cambridge MIT`. A name matches if it is part of the university's listed name, ignoring case. Put search words before this option, because every word after it is read as a university. |
| `--engine latex\|typst\|all` | Which engine to list |
| `--limit <n>` | Rows to print, 30 by default |
| `--json` | Print a JSON array instead of a table |

The ENGINE column shows what compiles each template: `pdflatex`, `xelatex`,
`lualatex` or `typst`. Download counts are refreshed once a day.

## info, view, open

`info <name>` prints the template's metadata, versions and variables. Add
`--json` for the raw index entry.

`view <name>` downloads the prebuilt PDF preview and opens it. `--web` opens
the template's page on the website instead.

`open <name>` opens a LaTeX template in Overleaf, which imports its zip file
into a new project. For a Typst template it opens the Typst web app, and this
only works for templates that are also on Typst Universe. `--print` prints the
link without opening a browser.

## update

Downloads the index again. The CLI also does this by itself when its copy is
more than a day old, and it falls back to the old copy when you are offline.

## doctor

```console
texshelf doctor [file.tex]
```

Lists which of pdflatex, xelatex, lualatex, latexmk, bibtex, biber, tectonic,
typst and git are installed. Given a `.tex` file (or with a `main.tex` in the
current folder), it also looks up every `\usepackage` and `\documentclass` with
`kpsewhich` and names the ones that are missing.

## tex install

```console
texshelf tex install [name] [--print] [--yes]
```

Helps you get a LaTeX or Typst compiler. texshelf downloads nothing itself. It
picks the vendor's own install command for your system, shows it, and runs it
once you agree.

With no name it lists what this system can install, with the recommended one
first, and asks which you want. `typshelf tex install` puts Typst first.

| Name | What you get | Offered on |
| --- | --- | --- |
| `tinytex` | A small TeX Live in your home folder. Add packages later with `tlmgr`. | macOS, Linux, Windows (manual download) |
| `tectonic` | One program that fetches packages the first time a document compiles | everywhere |
| `mactex` | All of TeX Live, through Homebrew | macOS |
| `texlive` | All of TeX Live, through apt or dnf | Linux |
| `miktex` | MiKTeX, through winget | Windows |
| `typst` | The Typst compiler | everywhere |

The command depends on what is installed: Homebrew, winget, pacman or Cargo is
used when present. When none fits, texshelf prints a link and the steps to do
by hand. `--print` only prints the command, and `--yes` runs it without asking.
Afterwards, `texshelf doctor` shows whether the compiler is found.

## tap

```console
texshelf tap add <name> <address>
texshelf tap list
texshelf tap remove <name>
```

A tap is another registry, for example one your university or lab runs. Its
templates show up in `search` as `<name>/<template>` and work with every
command:

```console
texshelf tap add uc https://example.org/templates/v1
texshelf search lab
texshelf init uc/lab-report informe-3
```

The short name works too (`init lab-report`) when no other tap has a template
with that name. A template in the main registry always wins over a tap's.
`update` refreshes taps along with the main index. See [Taps](taps.md) for how
to host one.

texshelf does not review or audit tap templates, and they are never marked
official. Add taps you trust.

## new-template

```console
texshelf new-template <dir> [--name <name>] [--category <c>]
```

Creates a folder with a starter document and a `texshelf.toml` to fill in.
`typshelf new-template` starts a Typst template.

## publish

```console
texshelf publish [dir] [--print]
```

Run it in a template folder that is committed and pushed to GitHub. It builds
the registry manifest from `texshelf.toml` and the Git state, validates it, and
opens a GitHub page with the file ready to propose as a pull request.

## validate

`validate <manifest.toml>` checks a registry manifest and lists every problem
it finds. See [Manifest format](manifest.md).

## Environment variables

| Variable | Purpose |
| --- | --- |
| `TEXSHELF_INDEX_URL` | URL or file path of another `index.json` |
| `TEXSHELF_CACHE_DIR` | Cache folder for the index, archives and previews |
| `TEXSHELF_CONFIG_DIR` | Folder for `config.toml`, which holds the taps |
| `HTTPS_PROXY`, `NO_PROXY` | Honoured for downloads |
