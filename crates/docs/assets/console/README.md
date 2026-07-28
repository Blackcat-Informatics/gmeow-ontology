<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# `<gmeow-console>`

The standalone, offline, zero-dependency GMEOW console: the same 37-tool surface an agent
drives, in a browser tab, with no server and no network.

It is one custom element. Drop `<gmeow-console></gmeow-console>` on a page, load
`element.mjs` as a module, and you have the whole thing — shadow DOM, co-located styles,
its own engine worker.

## Panes

There is no pane list in this code. The console boots by calling `tools/list` **and**
`action_policy`, and renders one pane per tool whose policy subject is asserted
`logic:ActionSchema` and **not** `logic:McpActionSchema` — the read half of the shipped
action theory. The six governed writes (`store_claim`, `revise_belief`, `store_conjecture`,
`refute_conjecture`, `submit_candidate`, `withdraw_candidate`) are excluded *by the shipped
policy*, not by an exclusion list here. Adding a tool to the ontology's action theory adds
a pane; no JavaScript changes.

Each pane's form is rendered from that tool's own advertised JSON Schema
(`inputSchema.properties` + `inputSchema.required`), so a new argument is a new field.

Five structural panes ride alongside the derived ones:

| Pane | What it shows |
|---|---|
| **Round trip & loss lattice** | One document transcoded into every shipped target through `convert`, with the realized loss ledger read against the loss lattice the run itself derives (drop-set inclusion). A per-format failure on **your** data is recorded and rendered — the differential never aborts. |
| **Derivation structure** | The derivation DAG of the recorded session, its minimal fatal cut, the anchor cluster, and any Belnap gluts. |
| **Worked vignettes** | Real invocations over data drawn from the shipped ontology's own surfaces. Every vignette exercises RDF-1.2 quoted triples; every invented individual lives under `example.org`. |
| **Session & permalink** | The recorded trajectory, its content-addressed permalink, and the `.gts` export. |
| **About this runtime** | The documentation-distribution catalog and the derived formal-concept lattice, drawn as an inline-SVG Hasse diagram — routed through the `distribution_matrix` tool. |

## The four verbs

All four execute, all through the shipped tool surface:

* **parse / validate** — `validate_local` (Tier-1 conformance against the bundle shapes);
* **reason** — `reason_graph`, the real closure from the native structured-DL chase;
* **serialize** — `convert`, with its realized loss ledger;
* **query** — `query_local`, over your own graph or the bundle union.

`reason_graph` lives in the demand-loaded reasoning segment. The first pane that needs it
shows a **loading state** — never a failure and never a silent stall — driven by the
engine's own typed `mcp.segment-not-loaded` signal through `tieredMcp`.

## Session, permalink, export

Every invocation is materialized as a `gmeow:ToolCall` carrying
`logic:instantiatesSchema` (the tool's `logic:ActionSchema`), `gmeow:atTime`, one shared
`gmeow:eventTemporalFrame`, and a `logic:properPartOf` trajectory anchor bearing
`logic:transitionFromState` — **exactly** the shape
`crates/logic/src/transaction/trajectory.rs` discovers. The console records; the shipped
native auditor folds. There is no second trajectory folder.

Derived result statements are annotated with RDF-1.2 quoted triples: a reifier
`rdf:reifies <<( s p o )>>` carrying `gmeow:derivedBy` (the call) and one
`gmeow:wasDerivedFrom` per antecedent.

The **permalink** is `<content-address>.<base64url payload>` over the *invocation list*
only — never the results, so a link replays against the reader's own engine. A digest
mismatch is refused, not best-effort replayed.

The **`.gts` export** carries two graphs: the trajectory in the default graph, and the
engine's claim/candidate store as it stood at export time in a named
`gmeow:sessionStoreSegment` graph. An export without the store is refused — half a session
snapshot is not a session snapshot.

## Local preview

The console is a static tree. Assemble it and serve it:

```sh
cargo run -q -p gmeow-dev-cli -- console-assemble --out dist/console-smoke
python3 -m http.server -d dist/console-smoke 8080   # or any static file server
```

Then open `http://localhost:8080/console/`.

`console-assemble` **refuses** an `--out` equal to or inside `ontology-docs/` or
`dist/gmeow-docs/`: those bases have exactly one writer, `make regen SYNC_OUTPUTS=docs`.

## Offline

`sw.mjs` is a cache-first service worker registered at `console/` scope. Its `SHELL` array
is **generated** by the Rust producer from the assembled key set — never hand-authored, so
it cannot drift from what actually ships. Install pre-caches every shell member with
`cache.addAll`, which rejects the whole install if any member is missing: a partially
cached shell is an offline surface that fails unpredictably later.

The engine assets live one level up under `assets/` (shared with the documentation site, so
the 7 MB core image is not duplicated) and are therefore out of the worker's scope. They are
pre-cached anyway and read back through `caches.match`, so an offline console still gets its
engine.

`manifest.webmanifest` declares `display: standalone` and `start_url: "."`, so the console
installs as a PWA.

## No optionality

A missing asset, a failed digest or an unavailable engine is a **visible hard error**: the
element dispatches `gmeow-console-error`, the shell renders it in `#error-banner`
(`role=alert`), and the pane shows the failure in place. Nothing degrades quietly. The one
deliberate exception is segment deferral, which is progress and is shown as such in
`#status-banner` (`role=status`).

## Measured bytes

Measured over the shipped, `wasm-opt -Oz`'d assets in this tree — uncompressed on-disk
bytes, no transfer encoding assumed.

**First load** — everything fetched before any pane runs:

| Asset | Bytes |
|---|---:|
| `console/index.html` | 4 416 |
| `console/element.mjs` | 34 535 |
| `console/engine.worker.mjs` | 9 555 |
| `console/session.mjs` | 17 060 |
| `console/examples/gallery.mjs` | 8 700 |
| `console/manifest.webmanifest` | 341 |
| `assets/mcp-transport.mjs` | 17 936 |
| `assets/mcp-core/index.mjs` | 6 974 |
| `assets/mcp-core/pkg/gmeow_mcp_core_wasm.js` | 14 324 |
| `assets/mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm` | 7 452 156 |
| **Code subtotal** | **7 565 997** |
| `assets/gmeow.gts` (the ontology snapshot) | 37 379 608 |
| **First-load total** | **44 945 605** |

**Demand-loaded reasoning segment** — fetched only when a pane first needs a
reasoning-segment tool, and never at all for a reader who only looks things up:

| Asset | Bytes |
|---|---:|
| `assets/mcp/index.mjs` | 2 024 |
| `assets/mcp/pkg/gmeow_mcp_wasm.js` | 12 229 |
| `assets/mcp/pkg/gmeow_mcp_wasm_bg.wasm` | 10 346 467 |
| **Segment total** | **10 360 720** |

The console does **not** load the vendored purrdf engine — that one serves the
documentation site's standalone SPARQL/describe surfaces, not the console.

## Install

```sh
npm install @gmeow/console          # not yet published
```

```html
<script type="module" src="https://cdn.jsdelivr.net/npm/@gmeow/console/element.mjs"></script>
<gmeow-console></gmeow-console>
```

The CDN form still needs the sibling `assets/` tree reachable one level up from
`element.mjs`; point the transport elsewhere with
`configure({ assetBase })` from `assets/mcp-transport.mjs` if your layout differs.

## `smoke/`

`smoke/package.json` + `smoke/package-lock.json` pin Playwright for the browser smoke lane.
Dev-only; nothing under `smoke/` is part of the shipped console. `npm ci` requires the
lockfile, which is why it is committed.

The DOM-free acceptance lanes are not part of the shipped tree — they live in the
repository at `crates/docs/assets/console/tests/*.test.mjs` and run under `node --test`
against the real engine, with no browser at all.
