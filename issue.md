# Issue #1646: schemas: recover the developer-surface capability lost in the purrdf emitter cutover

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, enhancement, foundational, cross-cutting, module-enhancement, quality, rust, complexity-medium

## Body

Drained from the local `.deficiencies` ledger (recorded 2026-07-12 and 2026-07-17). Re-verified against current `main`.

## Problem

Cutting the schema emitters over to purrdf's `CompiledSchema` basis unified the derive-source (good, and intended) but dropped a set of shipped developer-surface capabilities that were never re-homed. Recorded at the time as a user-approved capability regression; nothing tracks recovering it.

### Pydantic: a flat module with no ontology linkage

`packages/python/gmeow_models/` is now `__about__.py`, `__init__.py`, `_base.py`, `models.py` (2.6 MB), `py.typed`. Verified on the shipped package:

- `grep -c definitionDigest packages/python/gmeow_models/models.py` -> **0**
- `grep -c json_schema_extra packages/python/gmeow_models/models.py` -> **0**

So the following are gone, not re-homed (`crates/pipeline/src/stages/pydantic.rs:19-35` documents each):

- **Per-slice modules.** Each class used to route into its owning slice's module (`gmeow_models/<slice>.py`, ~40 modules) so a slice's models were independently importable, with the slice's tier/DOI header in the module docstring. Everything is now one flat `models.py`.
- **Docs-digest linkage.** `json_schema_extra` carried the term's `gmeow:definitionDigest`, and the class docstring carried definition / when-to-use / avoid / how-to-use, worked examples, a runnable usage doctest, and a docs back-link URL. A model class now carries only its compiled-schema identity — no term-prose linkage at all.
- **Generated package `README.md`.**

Upstream feature request: `Blackcat-Informatics/purrdf` issue 130 (per-slice module routing, per-class docs/definition-digest linkage, caller-supplied version stamping).

### Developer schemas now cover exactly the SHACL shape union

LinkML / TypeScript / GraphQL moved from a hand-rolled OWL to LinkML projection onto the SHACL-derived basis. Consequence: OWL-only / RBox properties with no derived SHACL property shape are no longer in these surfaces. Concretely `gmeow:resentDate` and `gmeow:resentMessageId` (email slice, `gmeow:graphBoxRole gmeow:boxRBox`, non-functional DatatypeProperties) are absent from `generated/schemas/gmeow.linkml.yaml`, `gmeow.schema.json`, and `models.py`. They remain canonical in the OWL/RDF layer, enforced by the slice's structural assertion — but a developer reading the schemas cannot see them. Upstream: purrdf issue 134.

### Two further accepted format changes

- JSON-LD serialization emits an empty `@context` (examples/goldens now carry full IRIs). Upstream: purrdf issue 135.
- `gmeow export` CSVW is purrdf's generic lossless RDF-1.2-in-CSV carrier (large), replacing the retired curated class/property/individual term-dictionary tables. Upstream: purrdf issue 136.

### There is now no live-Python conformance at all

The ledger recorded that live-Python conformance was off the merge-blocking gate, running only in `make maint-pydantic-conformance`. Both are now gone: no `maint-pydantic-conformance` target exists in the Makefile, and `packages/python/tests/` does not exist (`test_conformance.py` was deleted as asserting a layout that no longer exists). The only remaining check is the Rust structural agreement gate `emitted_models_agree_with_packed_schema_defs` — which proves the emitted `.py` agrees with the compiled `$defs` but never runs a Python interpreter. A package that structurally agrees yet fails to import passes every gate in the repo.

## Work

- [ ] Decide per capability: recover in-repo, wait on the upstream issue, or ratify the loss in `docs/schema-projections.md`.
- [ ] Restore some form of term-to-model linkage (`definitionDigest` and a docs back-link are the minimum that makes the wheel navigable).
- [ ] Re-establish a live import + `model_json_schema()` agreement check somewhere — off-gate is fine, absent is not.
- [ ] Decide whether OWL-only/RBox properties should be surfaced in the developer schemas or documented as deliberately out of scope.

## Acceptance

- A consumer of the wheel can get from a model class back to its ontology term.
- The generated package is proven to import, in some lane.


## Comments (7)

### coderabbitai — 2026-07-27T00:16:37Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#608 - feat(shacl): Python seam swap to gmeow_shacl behind a dual-run cross-check (`#578`) [closed]
Blackcat-Informatics/gmeow-ontology#989 - fix: port schema emission to Rust [merged]
Blackcat-Informatics/gmeow-ontology#1480 - feat(docs): max-info content enrichment — diagnostics/loss/entailments/fixtures/competency/notation/pipeline-DAG panels (`#1404`) [merged]
Blackcat-Informatics/gmeow-ontology#1507 - feat(pipeline): Pydantic v2 functional documentation surface (gmeow_models) packed in gmeow.gts [merged]
Blackcat-Informatics/gmeow-ontology#1573 - feat(logic): execute genuine correspondence recovery [merged]
</details>

---
<details>
<summary>📝 Issue Planner</summary>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!

### paudley — 2026-09-05T03:06:28Z

## Re-verified against `main` @ c2b03aba6 — every claim still holds

Checked while inventorying open issues. Nothing here has been resolved by drift:

| claim | verified |
|---|---|
| flat module, no per-slice routing | 4 tracked `.py` files under `packages/python/gmeow_models/` (`__about__`, `__init__`, `_base`, `models`) |
| `definitionDigest` linkage gone | `grep -c definitionDigest models.py` → **0** |
| `json_schema_extra` gone | `grep -c json_schema_extra models.py` → **0** |
| no `maint-pydantic-conformance` | 0 matches in `Makefile` |
| no live-Python conformance | `packages/python/tests/` — 0 tracked files |

The last row is the one worth surfacing: as this issue puts it, *"A package that structurally agrees yet fails to import passes every gate in the repo."* That is still true, and it is the cheapest of the four work items to fix — an off-gate import + `model_json_schema()` agreement check would close the "absent is not fine" gap without waiting on any upstream purrdf issue.

*Method: tracked-file and content greps against `origin/main`.*

### paudley — 2026-09-05T07:34:11Z

## Correction — my previous comment recommended the wrong direction

I wrote that restoring a live Python import + `model_json_schema()` agreement check was "the cheapest of the four work items to fix". That recommendation was wrong and I am retracting it.

**Adding it would add Python surface**, which `CLAUDE.md` puts behind explicit authorization:

> **RUST-FIRST, Python-surface** — core work is Rust. **Adding ANY Python (code, tests, fixtures, orchestration) requires explicit authorization; if you think you may be writing Python, you are probably doing the wrong thing.**

I read the "absent is not fine" line in this issue as a gap to close without asking what the surface is *for*.

**What the tree actually contains.** The entire Python footprint is a generated projection — four files under `packages/python/gmeow_models/` (`__about__`, `__init__`, `_base`, `models`), plus `packages/python/pyproject.toml` and `.python-version`. Nothing hand-authored. `models.py` is emitted from the compiled schema.

**This reframes all four work items.** Three of them — per-slice module routing, per-class docs/`definitionDigest` linkage, a live import check — would each *deepen* a Python surface rather than keep it thin. They are recovery of capability in a direction the repo is deliberately moving away from.

That leaves this issue's own third option as the live one:

> Decide per capability: recover in-repo, wait on the upstream issue, or **ratify the loss** in `docs/schema-projections.md`.

Recommend re-scoping around ratification, and adding the prior question the issue does not currently ask: **who consumes the Pydantic wheel?** If the answer is nobody, the honest outcome is retiring the projection rather than documenting a regression in a surface with no consumer — and the LinkML / TypeScript / GraphQL developer schemas, which are not Python, are where the developer-surface question actually lives.

The fourth item — whether OWL-only / RBox properties like `gmeow:resentDate` should appear in the developer schemas at all — is unaffected by any of this and remains a real open question on a non-Python surface.

*Correcting the record rather than leaving a recommendation that would have sent the next reader into authorization-required work.*

### paudley — 2026-09-05T07:34:52Z

## Refinement — the distinction that matters is emitter-side vs Python-side

My previous comment overcorrected. `packages/python/gmeow_models/` is a **Pydantic export of the ontology** — a generated projection peer to the LinkML, TypeScript, GraphQL and JSON Schema surfaces, not hand-authored Python. Framing it as a surface to retire was wrong; it is a projection like any other, and the regressions this issue records are real losses in it.

What actually splits the four work items is **where the fix lives**, not whether Python is involved:

**Emitter-side — Rust, no authorization question.** These are changes to `crates/pipeline/src/stages/pydantic.rs` that make it emit a richer export. The output happens to be Python; the work is not.

- Per-slice module routing (`gmeow_models/<slice>.py`) with the slice's tier/DOI header.
- Per-class `definitionDigest` in `json_schema_extra`, plus the prose coat and docs back-link in the class docstring.
- The generated package `README.md`.

An ontology export that cannot be navigated back to the term it projects is a weaker export, and fixing that is ordinary emitter work.

**Python-side — authorization-gated.** Only one item is:

- A live import + `model_json_schema()` agreement check. That needs Python test infrastructure in the repo (`packages/python/tests/` was deleted), which `CLAUDE.md` puts behind explicit authorization.

The gap it addresses is still real — *"a package that structurally agrees yet fails to import passes every gate"* — but the remedy is a decision, not a task to pick up. Worth noting an alternative that stays Rust-side: the emitter could assert its own output parses as valid Python via a Rust-side syntax check, which catches the "fails to import" class of defect without a Python interpreter in the loop. That is weaker than a real import, and the difference should be stated rather than glossed.

**Neither** — the OWL-only / RBox question (`gmeow:resentDate`, `gmeow:resentMessageId` absent from the developer schemas) is about the SHACL-union basis, and applies equally to LinkML, TypeScript and GraphQL. Nothing to do with Python.

So: three items are ordinary Rust emitter work on an export surface, one needs a decision, one is a modelling question on a different surface. My earlier "recommend re-scoping around ratification" applies only to the second.

### paudley — 2026-09-06T15:30:33Z

## The Pydantic half is no longer upstream-blocked — purrdf 1.1.0 already ships the API

This issue records the per-slice modules and docs-digest linkage as waiting on **purrdf issue 130** (*"per-slice module routing, per-class docs/definition-digest linkage, caller-supplied version stamping"*). Re-checked against the pinned substrate: **that request has been implemented.** The capability is now recoverable in this repository, with no upstream dependency left.

`purrdf-shapes 1.1.0` exposes exactly the three things that were lost:

| lost capability | purrdf 1.1.0 API |
| --- | --- |
| per-slice module routing | `PydanticConfig::with_topology(PydanticPackageTopology)`; `PydanticModuleConfig::{path, docstring}` |
| per-class docs + digest linkage | `PydanticClassConfig::{docstring, json_schema_extra}` |
| caller-supplied version stamping | `PydanticConfig::with_version_stamp(PydanticVersionStamp)` |

The repo already knows this, and says so — `crates/pipeline/src/stages/pydantic.rs:29-33`:

> PurRDF **now supports** caller-owned routed topology and class metadata, but this GMEOW stage still selects its flat compatibility configuration and supplies neither. The missing per-slice and ontology-linkage surface is live, durably tracked developer-surface work, not an authorized regression.

So the remaining work is entirely in `pydantic_config()`, which today calls `PydanticConfig::new(...)` and stops. Verified against the shipped wheel: **1,328 classes in one flat `models.py`**, and `grep definitionDigest` / `json_schema_extra` still return **0**.

### One structural consequence worth knowing before starting

Class metadata and module routing are **not separable**. `docstring` and `json_schema_extra` live on `PydanticClassConfig`, which only exists inside a `PydanticPackageTopology`, and `emit_pydantic` switches to routed rendering the moment a topology is present (`let routed = config.topology().is_some();`). There is no way to attach the digest linkage while keeping the flat single-module layout.

That makes this one change, not two: recovering the linkage necessarily restores per-slice modules at the same time, and replaces the single 2.6 MB `models.py` with roughly one module per owning slice.

Two constraints that fall out:

1. **The topology must be a total partition.** `PydanticPackageTopology::new` rejects unknown routes, unused modules, and prefix collisions, so *every* definition must be routed — including the synthesized `…Enum` / `_…EnumValue` helpers and shared types like `Annotation`, which have no single owning slice and need a declared home.
2. **The wheel layout changes.** `gmeow_models.models` stops being the import surface. Per #1654 no real publish has ever run, so there are no existing consumers to break — but the packaging assertions in `pypi-publish-gmeow.yml` (`__init__.py`, `py.typed`) and `emitted_models_agree_with_packed_schema_defs` both need to hold against a routed tree.

### What this does not change

The other three upstream items are still genuinely upstream: OWL-only/RBox properties absent from the SHACL-derived surfaces (purrdf 134), the empty JSON-LD `@context` (135), and the CSVW carrier swap (136). And the live-import check remains authorization-gated — it is the one item here that would add Python to the repo.

I have not started the implementation; recording the unblock first so the issue stops describing a blocker that no longer exists.

### paudley — 2026-09-06T15:32:39Z

### Addendum: the removed renderer left three orphaned snapshots — and they are the exact spec

`crates/pipeline/src/stages/snapshots/` still holds three insta goldens from the deleted slice-routed renderer:

```
gmeow_pipeline__stages__pydantic__tests__models_python_init.snap
gmeow_pipeline__stages__pydantic__tests__models_python_synthetic_demo.snap
gmeow_pipeline__stages__pydantic__tests__models_python_tags_module.snap
```

**No test references any of them** (`grep assert_snapshot crates/pipeline/src/stages/pydantic.rs` → nothing), and nothing in the repo runs insta with `--unreferenced=reject`, so they sit there silently describing output the pipeline no longer produces. On their own that is a small cleanup.

They are far more useful than that, though: `models_python_tags_module.snap` is a **complete specification of what has to be rebuilt**, down to the byte. It records the module docstring shape:

```
"""GMEOW models — slice GMEOW Tags Module.

Slice IRI:  https://blackcatinformatics.ca/gmeow/slices/tags
Tier:       core
DOI:        —
```

and the exact class metadata this issue asks to recover:

```python
json_schema_extra={"$id": "https://blackcatinformatics.ca/gmeow/Tagging",
                   "curie": "gmeow:Tagging",
                   "definitionDigest": "blake3:8fc46ba35cc4081583b665506bd8bab417b05223285c67f41fdc1a37c219b38b",
                   "iri": "https://blackcatinformatics.ca/gmeow/Tagging"}
```

plus the class docstring carrying definition / when-to-use / when-to-avoid / how-to-use / examples, and a **runnable doctest** that is itself the acceptance criterion in miniature:

```python
>>> from gmeow_models.tags import Tagging
>>> Tagging.model_config["json_schema_extra"]["curie"]
'gmeow:Tagging'
```

So they should not be deleted as dead weight until the restoration lands — they are the oracle to restore against, and then they become live goldens again.

### Notes for the implementation

- `gmeow:definitionDigest` is produced by `stage-term-manifest` (`crates/pipeline/src/stages/term_manifest.rs`), so `stage-export-pydantic` needs a new dependency edge on it — which in this repo means editing three places together (`Stage::consumes()`, `full_spec()`, and `gmeow:dataflowConsumes`), or the mismatch only reds at regeneration rather than at `cargo test`.
- The compiled `$defs` carry **no** `$id`: keys are bare local names (`AccessibilityAssertion`), with synthesized `…Enum` / `_…EnumValue` siblings. The IRI has to come from the shape union's `sh:targetClass` (or be derived as `gmeow:<Key>` and *verified* against the ontology store, which `enriched_compiled_schema` already loads).
- `PydanticPackageTopology::new` demands a total partition, so the synthesized enum helpers and shared types (`Annotation`) need a declared home module rather than being left unrouted.

### paudley — 2026-10-20T09:06:20Z

# Restore ontology-linked model exports

Principles 4, 7 and 17: keep the shared compiled schema as the only structural
source and use PurRDF's routed renderer, with GMEOW-owned slice and term metadata.
The existing flat emitter drops navigation already supported upstream. Recover
slice modules, class definition digests, documentation coats and links, package
README, and total routing of synthesized/shared schema definitions. Remove stale
flat-layout assumptions and obsolete renderer snapshots within this scope.

## Task 1: Source and render the metadata

Use the existing documentation discovery path with this run's term manifest and
constraint catalog products, never stale generated files. Declare both edges in
the stage, executable spec and ontology DAG. Include documentation source inputs
and bump the stage implementation version. Route actual ontology classes via the
shared schema namespace codec; assign generated schema helpers an explicit shared
module without fabricating ontology identities. Missing class ownership or digest
must fail. Emit the package README from the same selected topology.

## Task 2: Verify the actual renderer and consumers

Run synthetic renderer controls for multiple slices, shared/helper definitions,
metadata, documentation, cross-module references and missing metadata refusal.
Update authenticated package-consumer assertions for routed output. Remove the
three orphan snapshots from the retired renderer once their contract is covered
by live tests. Run focused Make validation and normal signed hooks. Do not run
local optimized producer, fixture production, sync or full gate.

## Task 3: Publish, integrate and clean

Publish and merge sequentially through ghprsq under the standing CI waiver.
Preserve the parent issue: actual generated corpus acceptance and the live Python
import/schema lane are not established by Rust structural checks. The issue's
comments explicitly reserve new Python test/orchestration surface for separate
authorization. Do not create that surface or misrepresent parsing as import.
Document the current SHACL-derived developer-schema boundary rather than silently
inventing closed-world constraints for OWL/RBox-only properties. Preserve the
existing JSON-LD/CSVW contracts; no unrelated upstream emitter reimplementation.

No independent reviewer tool is exposed; no independent review is claimed.


