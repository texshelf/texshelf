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
| `--engine latex\|typst\|all` | Which engine to list |
| `--limit <n>` | Rows to print, 30 by default |
| `--json` | Print a JSON array instead of a table |

Download counts are not collected yet, so `--sort downloads` currently leaves
the order alphabetical.

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

## validate

`validate <manifest.toml>` checks a registry manifest and lists every problem
it finds. See [Manifest format](manifest.md).

## Environment variables

| Variable | Purpose |
| --- | --- |
| `TEXSHELF_INDEX_URL` | URL or file path of another `index.json` |
| `TEXSHELF_CACHE_DIR` | Cache folder for the index, archives and previews |
| `HTTPS_PROXY`, `NO_PROXY` | Honoured for downloads |
