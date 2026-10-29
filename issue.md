# Issue #1712: logic/mappings: salvage the correspondence frontend — dsl/mappings cells as logic:Correspondence IR

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, rust

## Body

## Why

`origin/paudley/1092-correspondence-frontend` (`7578645da`, 13 commits, 65 files) is **4501 commits behind `main`** — the single most stale live branch in the repo. It predates the PurRDF cutover, the grounding-slice consolidation, and the `owl:`-retirement. A direct merge is not viable.

Retained, not deleted, so the original remains recoverable.

**Check for subsumption before implementing.** Substantial grounding and correspondence work has landed since this branch was cut; parts of it may already exist on `main` in a better form. The first task is an honest overlap audit, not a port.

## What is on the branch

Five staged tasks (F5 Task 1 → Task 4 plus cleanup):

- `b10caf816` F5 Task 1 — transpile `dsl/mappings` cells to `logic:Correspondence` IR
- `420572602` F5 Task 2 — re-seat alignment lowerings on the typed correspondence + overclaim case
- `d1eea6312` F5 Task 3a — migrate alignment + FnO soundness checks to a native oxigraph-free home
- `0d9c2a743` F5 Task 3b — delete the retired oxigraph lints; wire consumers to the native soundness pass
- `a45e6cde2` F5 Task 4 — retire the orphaned alignment/projection pytests; reconcile the constitution

Principal files:

```
crates/logic-compile/src/projections/correspondence_frontend.rs
crates/logic-compile/src/projections/correspondence_soundness.rs
crates/logic-compile/src/projections/{edoal,sssom,sparql}.rs
crates/pipeline/src/stages/correspondence_lower.rs
crates/pipeline/src/stages/correspondence_soundness.rs
crates/slice/src/alignment_lint.rs
crates/pipeline/tests/{correspondence_soundness,edoal_sparql_correspondence,sssom_correspondence}_parity.rs
```

## Scope

1. **Overlap audit against current `main`** — determine what the landed grounding/correspondence surface already provides.
2. **`dsl/mappings` cells → `logic:Correspondence` IR** as the canonical authoring path, with EDOAL/SSSOM/FnO remaining *generated lowerings* (Principle 17), never authorities.
3. **Native, oxigraph-free soundness checking** for alignment and FnO, replacing the retired lints.

## Constraints

- Grounding correspondences are shipped ontology content in the meta-level `graph/correspondence-laws` graph. Read `docs/GROUNDING.md` and `docs/foundational-bridging.md` before touching this surface.
- SSSOM is not the authority.
- The branch contains Python removals (`crates/native/python/`, pytests). Adding any Python back requires explicit authorization; the direction of travel is Rust-first.

## Current-code reconciliation — 2026-09-08

Source audit against main ee8b5e1b650f1948b95ecac9aa91b4366e14aa45 (remote main verified at that revision) and the preserved producer branch f9c3e6307fb1c5ae861dc02483de73856ab89832. This is code inspection; no new test run is claimed by this audit. The branch age above is historical intake data.

| Original task | Current code evidence | Disposition |
|---|---|---|
| DSL -> correspondence IR | logic-compile/projections/correspondence_frontend.rs; transpile_correspondences_indexed; frontend tests | Already present; extend under #1741, do not port old frontend |
| Typed alignment lowerings/overclaim | pipeline/stages/correspondence_lower.rs materializes typed program + lookup; SSSOM/EDOAL/SPARQL lowerings consume judgments | Present but still partly re-read DSL/string legs; full shared-IR completion #1741/#1744/#1745 |
| Native alignment/FnO soundness | logic-compile/projections/correspondence_soundness.rs and pipeline edge; seven checks | Present; preserve exact findings and fix typed transfer under #1741 |
| Retired oxigraph lint removal | old crates/slice/src/alignment_lint.rs path absent; native corpus finding snapshot replaces retired engine parity | Subsumed; retain GMEOW finding-content check, not a duplicate RDF/SPARQL engine suite |
| Python retirement/governance | old native Python path absent; governance/constitution.ttl binds native correspondence pass and finding test | Already represented; do not restore Python or obsolete checks |

Verified remaining defects/gaps: correspondence_exec reparses source/forward/recovered/canonical text; raw term keys lose literal direction; maintainer target-axiom reinterning substitutes triple terms with urn:gmeow:unsupported-triple-term and loses literal direction. Composition currently computes rung/status in Rust without the full axis/loss/indexed-merge calculus. These are implementation defects owned by #1741/#1742, with certified fusion #1743, target/exemplar completion #1744 and production adoption #1745.

The complete user-approved scope is now #1704. Keep this historical reconciliation issue open until its retained obligations are demonstrated by the owning implementation; recording ownership is not a claim that the bugs are fixed. Principles 1, 4, 5, 6, 7, 17–19.


## Comments (0)

