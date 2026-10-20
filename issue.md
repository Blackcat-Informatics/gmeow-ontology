# Issue #1438: math: realize the projections layer — MathML/OpenMath/Data-Cube/STATO/QUDT emitters + projection failure classes

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic, rust, complexity-hard

## Body

Child of #1433. MATHEMATICS-PROJECTIONS.md is design + loss-ledger reuse only: no emitter exists for any target (MathML, OpenMath, RDF Data Cube, STATO, QUDT), all 6 projection failure classes (MissingPreservationKind, UnrecordedProjectionLoss, ExactPreservationViolated, …) are missing, and the inbound refusal contracts are unimplemented. Blocked by the pipeline-integration issue (the emitters need a registered math generator stage to live in).

## Work

Per the charter: outbound lossy lowerings with a loss-ledger row per emission (reusing `logic:preservationKind` verbatim per the grounding contract), the projection failure classes with counter-examples, and the inbound refusal contracts (an inbound artifact that cannot lift hard-fails with a typed diagnostic — the same lift-fully-or-hard-fail rule as the bridges). Expression rendering rides the registered rendering seam (`math:ExpressionRendering ⊑ lang:Rendering`): MathML/LaTeX read `lang:renderedContent`/`renderingConvention`/`renderingPreservation` off the math record, as the module already promises.

## Acceptance

At least MathML + QUDT emitters registered and producing `generated/projections/math/` artifacts from gmeow.gts content; all 6 projection failure classes enforced with fixtures; projection CQs registered; loss rows visible in the ledger.

## Comments (3)

### coderabbitai — 2026-07-08T03:56:00Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#563 - feat: notation projection layer (fixes `#318`) [closed]
Blackcat-Informatics/gmeow-ontology#1189 - feat(1093): F6 OpenEHR blood-pressure section/retraction — in-band complement round-trip + half-open ADL fidelity + empirical validator PASS [merged]
Blackcat-Informatics/gmeow-ontology#1321 - feat(math): analysis, topology & geometry — binder core + named-complement gate (ANALYSIS-AND-GEOMETRY) [merged]
Blackcat-Informatics/gmeow-ontology#1339 - feat(math): ingestion bridges — R, ONNX, proof-as-process (BRIDGES) [merged]
Blackcat-Informatics/gmeow-ontology#1390 - feat(diagnostics): Phase 4 producer sweep + loss unification + tri-grounding (closes `#1349`) [merged]
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

### paudley — 2026-09-05T03:07:12Z

## The "projections layer is absent" framing may no longer be accurate — needs a closer read than I gave it

Checked against `main` @ c2b03aba6 while inventorying open issues. A shallow scan finds each named target vocabulary already referenced in `crates/`:

| surface | files referencing it |
|---|---|
| QUDT | 29 |
| OpenMath | 4 |
| STATO | 3 |
| MathML | 2 |

I did **not** establish whether these are realized emitters with the projection failure classes this issue specifies, or incidental references (vocabulary constants, grounding declarations, test fixtures). The QUDT spread in particular suggests real integration; MathML at 2 files suggests little.

Flagging rather than concluding: whoever picks this up should first re-baseline which of the five projections genuinely exist, because the issue is currently scoped as if none do. That re-baseline may shrink this from complexity-hard to something much smaller.

Related: #1437 (math into the carrier) is now largely landed — `gmeow-math` is linked into the pipeline and `crates/pipeline/src/stages/math_producers.rs` exists — so the emitter home this issue depends on is in place.

*Method: case-insensitive file-count grep only. Deliberately not a verdict.*

### paudley — 2026-10-20T09:40:04Z

# Reject contradictory exact math projection records

Principles 4, 7, 17 and 19. Current-source audit of the older math projection
issue finds all six failure identities, five native projection-ledger checks and
probability-specific exactness checks already present. The five target emitters
and conservative inbound artifact contracts are not implemented. Mapping catalogs
and hand-authored demonstrators are not executable emitters. Preserve that full
parent scope; do not close it on this unit.

## Task 1: Repair the existing exactness admission

The native math ProjectionRecord checks currently accept ExactPreservation with
unsupportedConstruct entries, because every lossy check skips the exact branch.
Reject that internal contradiction with the existing ExactPreservationViolated
identity. Also reject an exact claim combined with another preservation kind.
Retain the existing probability completeness checks and explicitly lossy records.
Update the canonical failure definition, paired French/Mandarin translations and
conformance doctrine. Add a registered positive/negative canonical fixture pair.

## Task 2: Qualify and integrate

Exercise the production structural_lint_dataset entry on tiny authored datasets:
exact with literal/IRI drops, contradictory polarity, exact with no drops, and
lossy with recorded drops. Test named-graph records as well. Tests consume authored
inputs and never produce a repository corpus. Run focused Make checks, strict
producer-independent Clippy, source recipe admission and normal signed hooks.
No local optimized producer, corpus construction, sync or full gate. Publish and
merge sequentially through ghprsq under the standing CI waiver; clean this unit's
branch/worktree, preserve unfinished siblings and update the shared queue.

No independent reviewer tool is exposed; no independent audit is claimed. Full
canonical conformance and real outbound/inbound projection acceptance remain open.


