<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# The console's dev lanes

Everything here is **repository-only**. Nothing under `smoke/` is part of the shipped
console: the producer's shell file set does not carry it, the published package's `files`
list does not name it, and both facts are gated — so none of it is deployed to the site,
pre-cached into a reader's offline storage, or published to a consumer. That is also why
this document lives here rather than in the console's own README, which ships in the npm
tarball and is emitted into the deployed tree: a shipped document that describes a runner
configuration and a lockfile is a document naming files its own distribution does not have.

## Assembling the tree

The console is a static tree, and every lane below drives the **assembled** one, never the
build-input tree under `crates/docs/assets/`. That distinction is the point: the console
that ships is the producer's output, and a worker importing a specifier that resolves only
in the source tree is exactly the defect these lanes exist to catch.

```sh
make console-assemble CONSOLE_OUT=dist/console-smoke
python3 -m http.server -d dist/console-smoke 8080   # or any static file server
```

Then open `http://localhost:8080/console/`.

`console-assemble` **refuses** an `--out` equal to or inside `ontology-docs/` or
`dist/gmeow-docs/`: those bases have exactly one writer, `make regen SYNC_OUTPUTS=docs`.

## The browser smoke lane

The only lane that executes the console in a real browser. Run it with
**`make console-smoke`**, which assembles the tree first and then drives it:

| Path | What it is |
|---|---|
| `package.json` + `package-lock.json` | The pinned Playwright runner, and the `smoke` script the Makefile invokes so the runner invocation is spelled once. `npm ci` requires the lockfile, which is why it is committed. |
| `playwright.config.mjs` | Chromium only, one worker, `retries: 0`. A gate that passes on the second attempt has found something, so a retry would hide it. |
| `global-setup.mjs` | Builds everything the run serves, once: the pristine assembled tree, a copy truncated mid-file, a copy with a required pre-cached asset deleted, and a scratch project with the REAL `npm pack` tarball installed — all behind one plain static server with no COOP/COEP, which is what GitHub Pages provides. |
| `lib/*.mjs` | The shared harness: the worker-scoped booted page and its `<gmeow-console>` driver, the static server, the assembled-tree reader and its perturbations, the real tool inputs, and the WASM single-threaded reader. |
| `specs/*.mjs` | The assertions: the deployed leg, the hard-error surfaces, the derived pane set, every read tool through the assembled worker, RDF-1.2 through every target, the session round trip, the measured byte partition and its ceiling, and the installed package. |

The lane drives `$CONSOLE_OUT` — the assembled tree — which `make console-smoke` documents
as overridable.

## The DOM-free acceptance lane

The acceptance lanes that need no browser live one directory up, at
`crates/docs/assets/console/tests/`. They run under `node --test` against the real engine
with no DOM at all, and are driven by `make console-test`.
