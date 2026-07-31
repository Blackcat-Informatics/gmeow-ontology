<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# `<gmeow-console>`

The standalone, offline, zero-dependency GMEOW console: the same 38-tool surface an agent
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

An antecedent is not always an entity. A proof-carrying answer cites its **premises**, and a
premise is a *statement*, so such an antecedent is minted as its own content-addressed
reifier over its own `rdf:reifies <<( s p o )>>` and cited by that — the annotation says
"this conclusion was derived from THAT statement", and a reader recovers the premise itself
rather than a name for it. The premise is reified, never asserted: a triple term does not
assert, so the session records that the engine cited the premise, not that the console
independently claims it. Reifiers are content-addressed, so one premise cited by several
conclusions is one node.

The **permalink** is `<content-address>.<base64url payload>` over the *invocation list*
only — never the results, so a link replays against the reader's own engine. A digest
mismatch is refused, not best-effort replayed.

The **`.gts` export** carries two graphs: the trajectory in the default graph, and the
engine's store as it stood at export time in a named `gmeow:sessionStoreSegment` graph. The
store is read through **`store_segment`**, the one tool that serializes it — `recall`
answers a *query* with a ranked, truncated view of matching claims, which is not a snapshot
of anything. Coverage is judged per holder: a collection that reported state which nothing
carried refuses the export by name, because half a session snapshot is not a session
snapshot. That same segment re-seeds a store for a replay, so an exported session runs again
against a native `gmeow mcp` and answers byte-identically.

## No optionality

A missing asset, a failed digest or an unavailable engine is a **visible hard error**: the
element dispatches `gmeow-console-error`, the shell renders it in `#error-banner`
(`role=alert`), and the pane shows the failure in place. Nothing degrades quietly. The one
deliberate exception is segment deferral, which is progress and is shown as such in
`#status-banner` (`role=status`).

<!-- __GMEOW_CONSOLE_SITE_SECTIONS__ — the deployed tree's offline contract, PWA shell and measured byte table are substituted here; they document files the published package does not ship. -->

## Install

```sh
npm install @blackcatinformatics/gmeow-console
```

```html
<script type="module" src="./node_modules/@blackcatinformatics/gmeow-console/element.mjs"></script>
<gmeow-console></gmeow-console>
```

Serve the page over HTTP (a module worker and a streaming `WebAssembly` instantiation are
both refused from `file://`) and the console boots — no build step, no bundler, no import
map, nothing to configure.

**The package is self-contained.** It ships the element (`element.mjs`), its engine worker,
the DOM-free session module (a second entry,
`@blackcatinformatics/gmeow-console/session.mjs`), the vignette gallery, the TypeScript
declarations for both entries, **and the entire engine payload the worker boots over**,
under `pkg/`: the browser transport, the client-side BLAKE3, the always-resident core wasm
image, the demand-loaded reasoning segment, the integrity manifest, and the ontology
snapshot itself. That payload is staged into the package by its own `prepack` step,
straight out of `gmeow-dev console-assemble` — the one producer that assembles the console —
so the published bytes are the assembled bytes and there is nothing to copy by hand.

It does **not** ship the standalone *site* shell — `index.html`, `manifest.webmanifest` and
`sw.mjs`, which `gmeow-dev console-assemble` emits into the deployed tree instead. `sw.mjs`
in particular carries a generated pre-cache list and cache digest that only an assembled
tree can fill in, so publishing it here would ship an offline surface that caches nothing.

Nothing about the payload is optional. The worker imports its transport as
`./pkg/mcp-transport.mjs`, which is the same specifier that resolves in the assembled site
tree (where `console/pkg/mcp-transport.mjs` is a generated forwarder to the shared
`assets/mcp-transport.mjs`, so the site carries one engine copy rather than two). Point the
transport at a different snapshot with `configure({ assetBase })` if you have one; the
default is the payload beside it.

The engines are additionally published on their own, for a consumer that wants the wasm
without the element: `@blackcatinformatics/gmeow-mcp-core-wasm` (first load) and
`@blackcatinformatics/gmeow-mcp-wasm` (the demand-loaded reasoning segment).

### No runtime CDN loading

The console **never fetches code from a CDN at runtime**, and neither does anything else
this repository ships. A CDN URL is an install-time convenience for a hand-written page;
no module here contains one.

That is a consequence of the offline contract, not a preference. The deployed tree's service
worker pre-caches every member of its shell with `cache.addAll`, which rejects the whole
install if any member is missing — a member on a third-party origin would be an install that
fails, or an offline console whose engine silently is not there. The engine images are
additionally pinned by a BLAKE3 digest manifest and verified against the bytes that shipped,
which a third-party origin can neither offer nor be held to.
