# Publishing a template

A template stays in your own Git repository. Publishing it means adding one
manifest to the [registry](https://github.com/texshelf/registry) that points at
a commit of yours.

## Before you start

- The template compiles from a clean checkout with the compiler you will declare.
- It has a license that allows redistribution. See the accepted list in
  [Manifest format](manifest.md).
- It does not need shell-escape, or you are prepared to declare that it does.
- Build output such as `.aux` files and compiled PDFs is either not committed
  or listed under `exclude`.

## Steps

1. Put a `texshelf.toml` next to your template's main file. `texshelf
   new-template <dir>` creates one with a starter document, or copy the
   `[template]`, `[latex]` or `[typst]`, and `[variables]` tables from the
   manifest of a similar template. Leave `[source]` out.
2. Commit and push the template to a public GitHub repository.
3. In the template's folder, run:

   ```console
   texshelf publish
   ```

   It reads the repository URL, the current commit and the folder path from
   Git, checks the manifest, and opens GitHub with the registry file filled in.
   Choose "Propose new file" and open the pull request. `texshelf.toml` itself
   is left out of the published template. Add `--print` to see the manifest
   and the link without opening a browser.

To do it by hand instead, fork the registry, create
`templates/<name>/<version>.toml` with a `[source]` table whose `rev` is the
full commit hash, check it with `texshelf validate`, and open a pull request.

CI first scans your files without running them. It looks for compiled
programs, script files, shell commands such as `\write18`, packages that need
shell-escape, paths that leave the template folder, and private keys or
tokens. A compiled program, an absolute path, a leaked key or a shell command
that downloads code fails the check. Script files, a `latexmkrc` and plain
shell-escape use are allowed, and are listed on the template's page so users
can see them. You can run the same scan yourself with `texshelf-registry audit`.

CI then fetches your commit and compiles it in a full TeX Live or Typst container.
The rendered PDF and thumbnail are attached to the pull request run as an
artifact so reviewers can look at them. After the merge, the template is on the
website and in the CLI within a few minutes.

## Updating a template

A published version never changes. To release an update, add a new file with a
higher version and the new `rev`. Older versions stay installable with
`texshelf init <name>@<version>`.

## Submitting someone else's template

This is welcome when the license allows it. List the original authors under
`authors`, keep their license, and do not set anything under `[registry]`.
The template page links back to their repository.

## Testing locally

With the texshelf repository checked out next to the registry:

```console
cargo run --bin texshelf-registry -- build --registry ../registry --only <name> --out /tmp/shelf/v1 --base-url /tmp/shelf/v1
TEXSHELF_INDEX_URL=/tmp/shelf/v1/index.json texshelf init <name> /tmp/try
```

The first command needs `latexmk` (or `typst`) and `pdftoppm` on your PATH to
render the preview. Pass `--no-preview` to skip that.
