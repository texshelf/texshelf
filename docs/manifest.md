# Manifest format

A manifest describes one version of one template. It lives in the registry at
`templates/<name>/<version>.toml`. The folder and file names have to match the
`name` and `version` inside.

```toml
schema = 1

[template]
name = "arxiv-style"
version = "2024.1.2"
engine = "latex"
title = "arXiv-style preprint"
description = "Preprint layout based on the NIPS style, for papers posted to arXiv or bioRxiv."
authors = ["George Kour"]
license = "MIT"
keywords = ["arxiv", "preprint", "paper"]
categories = ["article"]
entrypoint = "template.tex"

[source]
repo = "https://github.com/kourgeorge/arxiv-style"
rev = "920514696f6e7270cab0558fd97c44515c63b4c4"
exclude = ["template.pdf"]

[latex]
compiler = "pdflatex"
bib = "bibtex"

[variables.title]
prompt = "Title"
default = "A template for the arxiv style"
files = ["template.tex"]
```

## [template]

| Key | Required | Meaning |
| --- | --- | --- |
| `name` | yes | 3 to 40 characters: a-z, 0-9 and `-` |
| `version` | yes | Three numbers, `1.2.0`. Imported templates without releases use the date of the pinned commit, `2024.1.2`. |
| `engine` | yes | `latex` or `typst` |
| `title` | yes | Name shown on the website |
| `description` | yes | One sentence, at most 200 characters |
| `authors` | yes | The template's authors, not the person submitting it |
| `license` | yes | SPDX identifier. Accepted: MIT, MIT-0, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unlicense, CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0, LPPL-1.3c, GPL-2.0-only, GPL-2.0-or-later, GPL-3.0-only, GPL-3.0-or-later, MPL-2.0 |
| `categories` | yes | One or more of: article, journal, thesis, report, lab-report, homework, exam, presentation, poster, cv, letter, book, notes, cheatsheet, other |
| `entrypoint` | yes | The file to compile, relative to the template folder |
| `keywords` | no | Extra search terms |
| `repository` | no | Project page, when it differs from `source.repo` |
| `universe` | no | Typst only: the template's name on Typst Universe, which enables the Typst web app link |

## [source]

| Key | Required | Meaning |
| --- | --- | --- |
| `repo` | yes | `https://` URL of the Git repository |
| `rev` | yes | Full 40-character commit hash. Branches and tags are not accepted, because they can move. |
| `path` | no | Folder inside the repository that becomes the project root. Defaults to the repository root. |
| `exclude` | no | Glob patterns for files to leave out, such as `"*.pdf"` or `"docs/**"` |

Everything under `path` that is not excluded is packed. Symlinks are rejected,
so exclude them or replace them with real files. A template may hold at most
2,000 files and 25 MB.

## [latex]

Only for `engine = "latex"`.

| Key | Default | Values |
| --- | --- | --- |
| `compiler` | `pdflatex` | `pdflatex`, `xelatex`, `lualatex` |
| `bib` | `none` | `none`, `bibtex`, `biber` |
| `shell_escape` | `false` | `true` if the document needs `-shell-escape`. The CLI and website warn users about it. |

## [variables.<key>]

A variable is a piece of text that `init` replaces. The template keeps real
example text, so it compiles untouched, and `init` swaps that exact text for
the user's answer.

| Key | Required | Meaning |
| --- | --- | --- |
| `prompt` | yes | The question shown to the user |
| `default` | yes | The answer used with `--yes` |
| `files` | yes | Files in which to replace the text |
| `match` | no | The text to replace, when it differs from `default` |

The build fails if the text does not appear in any of the listed files, which
catches a placeholder that was reworded upstream.

## [registry]

Set by registry maintainers, not by authors. `official = true` marks templates
the texshelf project maintains. `verified = true` marks templates published by
their own author.
