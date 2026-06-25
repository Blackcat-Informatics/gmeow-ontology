# GMEOW Editor Extensions

`gmeow-lsp` provides LSP diagnostics for `.ttl` (Turtle) and `.logic` files.

## Installing the binary

The server is installed via:

```sh
cargo install --path crates/lsp
```

or from a release artifact. It is built with the repo's pinned nightly
toolchain (see `rust-toolchain.toml`).

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
