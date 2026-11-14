# texshelf

texshelf starts LaTeX and Typst projects from templates. It has a command-line
tool and a website, [tex.jjsm.science](https://tex.jjsm.science), that shows
every template with a PDF preview.

One install gives you two commands. `texshelf` lists LaTeX templates by default
and `typshelf` lists Typst ones. They are otherwise the same program.

## Install

You need [Rust](https://rustup.rs) 1.82 or newer. Prebuilt downloads and a
Homebrew formula will come with the first tagged release.

```console
cargo install --git https://github.com/texshelf/texshelf texshelf
```

## First project

```console
texshelf search article
texshelf view article
texshelf init article my-paper
cd my-paper
latexmk -pdf main.tex
```

`view` opens the template's prebuilt PDF, so you can look before you download.
`init` creates the folder, asks for the title and author, and runs `git init`.
The last line it prints is the command that compiles the document.

## Pages

- [Command reference](cli.md)
- [Manifest format](manifest.md)
- [Publishing a template](publishing.md)
