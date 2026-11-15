# Taps

A tap is a registry someone else runs. Use one when templates should not go
through the public registry: a course's lab report, a department's thesis
class, or templates that are still drafts.

## Use a tap

```console
texshelf tap add uc https://example.org/templates/v1
texshelf search lab
texshelf init uc/lab-report
```

The address is the folder that holds the tap's `index.json`, or the file
itself. It has to be an `https://` address or a folder on your machine. The
list of taps is kept in `config.toml` in your settings folder:

```toml
[taps]
uc = "https://example.org/templates/v1/index.json"
```

Templates from a tap are checked against the SHA-256 in that tap's index, the
same way as public ones. Nobody at texshelf has looked at them, though, so
`init` and `info` say which tap a template came from.

## Host a tap

A tap is the output of `texshelf-registry build`, served as static files. It
needs no server code.

1. Create a repository with the same layout as the
   [public registry](https://github.com/texshelf/registry): one manifest per
   version at `templates/<name>/<version>.toml`. The
   [manifest reference](manifest.md) describes the fields.
2. Build it. `--base-url` is the address the output folder will be served from.

   ```console
   cargo install --git https://github.com/texshelf/texshelf texshelf-registry
   texshelf-registry build --registry . --out public/v1 \
     --base-url https://example.org/templates/v1
   ```

   Previews are compiled with the TeX and Typst tools on the machine that runs
   the build. Pass `--no-preview` to skip them.
3. Upload `public/` anywhere that serves files over https.

To try a tap before publishing it, build with a local folder as the base
address and add that folder:

```console
texshelf-registry build --registry . --out /tmp/tap/v1 --base-url /tmp/tap/v1
texshelf tap add draft /tmp/tap/v1
```

### GitHub Pages

This workflow rebuilds the tap on every push and publishes it at
`https://<owner>.github.io/<repository>/v1`. Turn on Pages for the repository
first, with "GitHub Actions" as the source.

```yaml
name: Publish tap

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

jobs:
  build:
    runs-on: ubuntu-latest
    container: texlive/texlive:latest
    steps:
      - run: |
          apt-get update
          apt-get install -y --no-install-recommends git curl ca-certificates build-essential pkg-config poppler-utils
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: typst-community/setup-typst@v4
      - run: cargo install --locked --git https://github.com/texshelf/texshelf --root tool texshelf-registry
      - run: |
          tool/bin/texshelf-registry build --registry . --out public/v1 \
            --base-url "https://${{ github.repository_owner }}.github.io/${{ github.event.repository.name }}/v1"
      - uses: actions/upload-pages-artifact@v3
        with:
          path: public

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
    steps:
      - uses: actions/deploy-pages@v4
```

A private tap works the same way from any host that serves the files only to
your network. texshelf sends no credentials, so it cannot read a tap that asks
for a login.
