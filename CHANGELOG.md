# Changelog

Changes to the `texshelf` and `typshelf` commands. The user guide is at
[docs.jjsm.science/latex/texshelf](https://docs.jjsm.science/latex/texshelf).

## Unreleased

- Output adapts to the width of the terminal. `search` drops columns as the
  window narrows and then switches to two lines per template, `info` wraps
  long values, and nothing else is printed wider than the window. Links are
  never shortened, so a long one runs past the edge. Piped output is laid
  out for 100 columns, or for `COLUMNS` when it is set. Warnings and errors
  use the width of the terminal they go to.
- New look for every command: results line up behind a green verb
  (`Fetched`, `Verified`, `Created`, `Next`), official templates are marked
  with a green dot (●), `search` shows version, stars (★) and license when
  there is room, and `--help` is coloured.
- `init` downloads and checks the template before it asks for the variables,
  so a bad download fails before you type anything.
- `tap add` with a local folder works on Windows. It used to fail with
  "os error 123".
- `tex install` recognises a MiKTeX or TeX Live that is already installed on
  Windows. It looked for `pdflatex` without the `.exe`.

## 0.2.1 - 2026-11-15

- Colour in the output when it goes to a terminal. Set `NO_COLOR` to turn it
  off. Piped output and `--json` never have colour.
- `tex install` with no name lets you pick with the arrow keys. Esc cancels.
- `texshelf search | head` ends quietly instead of failing on the closed pipe.

## 0.2.0 - 2026-11-15

- `tex install [name]` installs TinyTeX, Tectonic, MacTeX, TeX Live, MiKTeX or
  Typst with the vendor's own command. `--print` shows the command without
  running it.
- Taps: `tap add <name> <address>`, `tap list` and `tap remove` add templates
  from another registry, such as a university's. They appear as
  `<tap>/<template>`, are never marked official, and the main registry wins on
  a name clash.
- `cargo install texshelf` works: releases now publish `texshelf-core` and
  `texshelf` to crates.io.

## 0.1.0 - 2026-11-14

First release, with prebuilt binaries for macOS, Linux and Windows.

- `init`, `search`, `info`, `view`, `open` and `update` for using templates.
- `doctor` lists the TeX and Typst tools installed and the packages a document
  needs that are missing.
- `new-template` and `publish` for adding a template of your own, and
  `validate` for checking a manifest.
- `completions` for bash, zsh, fish and PowerShell.
