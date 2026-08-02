## Offline

`sw.mjs` is a cache-first service worker registered at `console/` scope. Its pre-cache list
and its cache name are both **generated** by the Rust producer from the assembled tree —
never hand-authored, so neither can drift from what actually ships.

What it pre-caches is **both pre-cached tiers of the table below, and nothing else**: the
page-load set (the shell, the transport, the always-resident core engine image and the
ontology snapshot) together with the install-only set (the worker itself, the PWA manifest
and its icons, and the vendored purrdf engine the published tree offers an embedder). Those
two are measured and published separately, because they answer different questions — what a
reader's page load costs, and what the worker stores so that reader can come back offline.

Install pre-caches exactly that set with `cache.addAll`, which rejects the whole install if
any member is missing — a partially cached shell is an offline surface that fails
unpredictably later. The demand-loaded reasoning segment is deliberately **not** in it:
pre-caching it at install would download the whole reasoning image — its measured size is
the demand-loaded total in the table below, and it is the largest single asset the tree
carries — for every reader who only ever looks things up, and would make "demand-loaded" a
claim the artifact contradicts. It is cached by the `fetch` handler the first time a pane
actually asks for it, and is offline-available from that moment on.

No magnitude for it is quoted here, and none is quoted anywhere else in this section. Every
byte figure in this document comes from the measured table below, which is generated from
the assembled tree on every render; a rounded figure typed into prose is a second source of
truth that goes stale the first time an engine is re-vendored, and this section carried
exactly that defect — a hand-typed size for the reasoning segment, still sitting here
unchanged after the segment grew, a few lines above the generated measurement contradicting
it. Rounding it correctly again would only restart the clock, so the producer now refuses to
ship these sections if they contain a hand-authored byte magnitude at all.

A service worker intercepts every request made by a page it controls, whatever the
request's own path — so the engine assets one level up under `assets/` (shared with the
documentation site, which is why the always-resident core image is not duplicated) are
cached and served here exactly like the shell is.

The cache name is a **BLAKE3 content digest of the assembled tree**. A cache keyed on the
shell's entry count and path length would be reused by any rebuild that kept the same
paths, serving a returning reader the previous build's bytes for ever; a content digest
changes with any byte, so `activate` deletes the old cache and the new bytes are fetched.

`manifest.webmanifest` declares `display: standalone`, `start_url: "."` and an icon set
(`icon.svg`, `icon-192.png`, `icon-512.png`, and a `maskable` `icon-maskable-512.png`), all
of which the producer emits — so the console meets a browser's installability criteria and
installs as a PWA without letterboxing. A browser reads none of them to paint the page; it
reads them when a reader installs the console, which is why they are measured below as
pre-cached rather than as page-load bytes.

## Measured bytes

<!-- __GMEOW_CONSOLE_BYTE_TABLE__ -->

There is no second engine *behind the panes*. Every widget here, and every interactive
surface of the documentation site — SPARQL, describe, conjectures — speaks JSON-RPC to the
same MCP segments listed above, over the same shipped bundle. One protocol, one engine, and
what the console can do an agent can do.

`assets/purrdf/` in the table above is not an exception to that. It is the sibling
[purrdf](https://github.com/Blackcat-Informatics/purrdf) RDF-1.2 kernel's browser build
(`MIT OR Apache-2.0`, vendored from its published npm package, refreshed by lower bound
rather than pinned), shipped so that a page **embedding** this tree can `import` an offline
RDF/JS store and run SPARQL over **its own** dataset — the one question `query_local` does
not answer, because its scopes are the shipped bundle and the frame you hand it, not a
standing dataset you keep. Nothing in the console imports it and no capability is attested
against it, which is exactly why it is measured as install-only above: no page load fetches
a byte of it, and it is pre-cached all the same, because an importable engine that was not
cached would 404 for exactly the offline reader it is there for. Its provenance, its
licensing and the rule for refreshing it are recorded in the repository, at
`crates/docs/assets/purrdf/PROVENANCE.md`.
