<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# GMEOW v0.2.0 Specification — The Realignment Release

> **One engine, three products.** v0.2.0 recasts GMEOW from "an ontology with tooling" to
> "grounded-AI-memory products with an ontology engine." Nothing in the canonical core is
> discarded; what changes is the front door, the audience, and the order of investment.
> Constitutional basis: Principles 13–15 (amended with this release), sharpened Principles
> 1 and 8.

## 1. Positioning

**Before (v0.1.x):** a reasoning-centric, OWL 2 DL super-vocabulary, published FAIR, with
compilers and an MCP server as supporting tooling. Primary surface: Turtle. Primary audience:
implicit (ontology-literate).

**After (v0.2.0):** a grounded agent-memory and claim-provenance substrate, shipped as three
products. Primary surfaces: `pip install gmeow`, MCP tools, a single-file format. Primary
audience: AI engineers and agents. The ontology, the reasoner gates, the FAIR publication, and
the LOD-Cloud track all continue — as the engine and its quality system, not as the pitch.

The moat, stated once: **GMEOW took epistemics seriously before the AI industry realised it had
to.** Every value is an attributed, dated, confidence-weighted, vantage-relative claim;
contradiction coexists as standpoints; revision is suppression, never erasure; provenance is
native, not bolted on. No incumbent agent-memory product has any of this. The window is open now.

## 2. The three products

### Product 1 — `gmeow` (PyPI): the ontology toolchain

The package people type: **`gmeow`** installs the full ontology tooling (`gmeow`
CLI), including validation, reasoning, documentation generation, and publication
workflows.
The `gmeow-gts` wheel carries the runnable agent-memory example used by the
quickstart.

The quickstart that *is* the product pitch:

```python
# pip install gmeow-gts        — run the example demo (P13 gate)
from gts.examples.agent_memory import Memory

mem = Memory("assistant.gts")                      # a GTS ai-package on disk

claim = mem.store(
    "Patrick prefers explicit error handling over exceptions-as-flow",
    source="conversation 2026-06-10",
    confidence=0.8,
    according_to="claude-fable-5",                  # the asserting standpoint
)

mem.recall("error handling preferences", min_confidence=0.5)

mem.revise(claim, reason="user stated the opposite for scripts",
           superseded_by=mem.store(...))           # suppression, never deletion (P10)
```

Under the hood: the generated Pydantic models (`dist/schemas/`, already shipping) are the typed
claim objects; the flat-JSON projection (#55) is the wire shape; the GTS `ai-package` is the
persistence. The RDF 1.2 semantics are preserved exactly — the user just never sees Turtle.

### Product 2 — the grounded-memory MCP server: the agent-native interface

Promote `src/gmeow_tools/mcp_server.py` from undocumented side artifact to flagship. Add the
memory triad to the existing retrieval tools:

| Tool | Semantics |
|---|---|
| `store_claim` | reified claim with source, evidence span, confidence, standpoint, time |
| `recall` | retrieval with confidence / standpoint / staleness / displayability filters |
| `revise_belief` | supersession + `displayable false`; full audit trail retained |
| *(existing)* `object_search`, `graph_explore`, `similar_messages`, … | the retrieval substrate, already live |

Differentiation vs. every vector-store "memory": attribution, contestability, evidence, audit,
time — for free, because the engine already models them. Deliverable includes `docs/mcp-server.md`
(currently zero docs) and a one-line `claude mcp add` / config-snippet onboarding.

### Product 3 — GTS `ai-package` + the claim spine: the format and the pattern

- **GTS `ai-package`** (spec § 13) positioned as its own artifact: *the SQLite of agent memory* —
  content-addressed, append-only, signable, single-file, vendor-portable. Belief revision =
  suppression frames (§ 11); model attestation = COSE (#272). Gets its own README section and
  pitch page, not a spec subsection.
- **The claim spine** (#55): Source → Chunk → EvidenceSpan → Claim as a standalone cookbook —
  extraction prompt, JSON Schema, SHACL, audit queries, `gmeow audit` — adoptable in an
  afternoon without RDF.

## 3. Recast inventory — what we already have, renamed by role

| Existing asset | v0.1.x role | v0.2.0 role |
|---|---|---|
| `statement-dsl/` + RDF 1.2 layer | statement metadata | **the claim substrate** (P14) |
| standpoint module | contested-fact modelling | **multi-model disagreement, surfaced** |
| `gmeow:displayable` / `coarsenTo` | suppression doctrine | **belief revision + privacy API** |
| `dist/schemas/` Pydantic/JSON/TS | generated export | **the client's typed core** (P13) |
| `mcp_server.py` | undocumented utility | **flagship product surface** |
| GTS (spec, reader/writer) | transport substrate | **the memory file format** |
| `dist/llms.txt`, JSONL catalog | vocabulary index | **LLM-emission enablement** |
| OWL 2 DL + ELK/HermiT gates | identity ("reasoning-centric") | **internal QA** (P8 as amended) |
| FAIR / VoID / DCAT / DOI / LOD | publication goal | **hygiene + scholarly bridge** (continues) |
| mail / domain modules | super-vocabulary breadth | **the dogfood corpus** (P15 grandfathers) |

Nothing is deleted. Per Principle 6, what gets replaced is the *framing*, and the inferior
framing does not survive alongside the new one.

## 4. v0.2.0 deliverables

**D1 — `gmeow` on PyPI** *(new issue)*
The ontology toolchain (`gmeow` CLI) published as `gmeow`. The GTS engine ships as
`gmeow-gts`, including a runnable `gts.examples.agent_memory` example over the
GTS ai-package profile. **Gate:** the example's time-to-first-claim test runs in
CI (P13).

**D2 — Grounded-memory MCP server** *(extends #54's facet G; new issue for the triad + docs)*
`store_claim` / `recall` / `revise_belief`; `docs/mcp-server.md`; onboarding snippet.
**Gate:** suppression honored in every recall path (#282's conformance tests cover the server).

**D3 — GTS ai-package, productised** *(#267 + #272 scope, re-prioritised)*
COSE signing (#272) lands; `ai-package` profile gets a worked example (a real saved-memory
file) and its own pitch section in README. **Gate:** round-trip + signed-verify vectors.

**D4 — Claim spine cookbook** *(#55, promoted in sequence)*
Ships after #54's terms land; the worked example doubles as D1's fixture.

**D5 — Extraction eval suite** *(new issue — `gmeow-evals`)*
"Given this document, emit GMEOW claims with evidence" scored against SHACL + the audit
queries, across frontier models. The benchmark instinct of #58, aimed at the claim layer.
This is both QA (which models emit valid GMEOW?) and marketing (a leaderboard is discovery).

**D6 — Constitution amendment PR**
Principles 13–16 + amended 1 and 8 (this release's companion change, per the amendment
process); issue template gains the P15 consumer question; #280's manifest covers 13–16 when it
lands.

**D7 — Core/extensions split** *(lands inside #287, governed by Principle 16)*
`ontology/core/` + `extensions/<name>/` with a per-extension manifest (terms, core-version
dependency, alignment targets, and the P15 consumer as a machine-checked field). Each extension
compiles, reasons (extension ∪ core), and drift-gates as a unit via the #279 registry.
**Boundary decision (deliberate, constitutional):** the claim/memory engine is core by
necessity; **identity (names, gender, language, sexuality) and deception epistemics are core by
commitment** — per Principle 16, the questions "what is a person, what is a name, what is a
gender, what is a lie" are existential for AI subjects and ship where they cannot be declined.
The pip client and `llms.txt` load core by default; domain extensions (email, archaeology,
genealogy, finance, organization, places, …) opt in via `mem.use("email")`-style loading.

**Out of scope for v0.2.0:** new domain modules (P15 applies); **extension SDK / catalog /
submission machinery — deferred until a named external extension author exists (P15 applied to
the extension system itself; the bundle *format* is GTS § 13 and needs no new invention)**;
Tiers 2–4 of #44; the Rust GTS core (#277); transpiler (#34); Croissant/RO-Crate (#58) — all
sequenced behind the three products per `go-sequence.md`, which this spec re-prioritises as
follows: **D1–D5 jump the strategic queue (old Phase 6); the compliance spine (#278/#279/#287,
now carrying D7) continues in parallel as planned — it is what makes fast movement safe.**

## 5. What explicitly continues unchanged

- `make check`, the reasoner gates, the compliance-by-construction epic (#278) — the engine's
  quality system, now justified *as* the quality system.
- The LOD Cloud / FAIR / DOI track (#44 Tier 1) — pursued as hygiene and scholarly bridge,
  honestly labelled a constituency rather than the home (P1 as amended).
- The mail-corpus domain work (#131–#141) — the dogfood that keeps the engine honest.
- Every existing constitutional commitment, 1–12. The amendment *adds* the market discipline;
  it subtracts no rigor.

## 6. Release mechanics

- Version: **v0.2.0**, immutable per Principle 6; `owl:versionIRI` snapshot as usual.
- The constitution amendment and this spec merge in the same PR (the amendment process
  requires design change and amending PR to ship together — this document is that design
  change).
- Success criteria for the release, falsifiable: (1) the `gmeow-gts` example gate passes in CI;
  (2) a stranger can go from PyPI (`gmeow-gts`) to a recalled claim using only the quickstart;
  (3) one signed `ai-package` worked example ships; (4) the MCP server is documented and
  installable in one config line.
