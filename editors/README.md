# GMEOW Editor Extensions

`gmeow-lsp` provides LSP diagnostics for `.ttl` (Turtle) and `.logic` files.

## Installing the binary

The server is installed via:

```sh
cargo install --path crates/lsp
```

or via the release target, which builds a release-profile binary and stages it
at `dist/bin/gmeow-lsp`:

```sh
make lsp-release
# binary is at dist/bin/gmeow-lsp
```

`make release` calls `lsp-release` automatically, so `dist/bin/gmeow-lsp` is
always present after a full release run.

It is built with the repo's pinned nightly toolchain (see `rust-toolchain.toml`).

## VS Code

The extension lives in `editors/vscode/`. It connects to the `gmeow-lsp`
binary automatically on `.ttl` and `.logic` files.

The server path can be overridden via the `gmeow.lsp.serverPath` setting.

Build and package the extension with:

```sh
npm install && npm run compile
```

Packaging for distribution uses `vsce` — this is out of repo CI scope.

## Other LSP clients (Neovim, Emacs/eglot, Helix, …)

Any LSP client can connect by pointing at the binary in stdio mode:

```sh
gmeow-lsp --stdio
```

Configure your client's LSP settings to launch `gmeow-lsp --stdio` for
`text/turtle` (`.ttl`) and the `gmeow-logic` filetype (`.logic`).
