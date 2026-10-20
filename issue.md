# Issue #1652: slice-quality ratchet: widen the counted surface and ship a live-measured debt view

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, quality, logic, rust, complexity-medium

## Body

Drained from the local `.deficiencies` ledger (recorded under #1546). Re-verified against current `main`.

Blocked by: #1642 — which owns the adjacent question of whether the ratchet should net a relocation across its source and destination cells.

## Problem

The projection-vocabulary ratchet works and is green, but it guards a narrower surface than "no hand-authored construct belongs to a generated formalism" implies, and its most useful view is not shippable.

### 1. The counted structural set excludes property signatures, and OWL entirely

`countKindStructuralAxiom` for `rdfs:` counts only `rdfs:subClassOf` and `rdfs:subPropertyOf` — the subsumption taxonomy (`slices/core/slice-quality-rubric/module.ttl:569`). `rdfs:domain` / `rdfs:range` are not counted. The original narrowing was principled: counting property signatures made the projection-vocab schema's own new domain/range declarations net-new debt that the grandfather gate correctly refused to grandfather — a self-referential bootstrap. Narrowing to the subsumption taxonomy resolved it, and RBox declarations are arguably not migration debt.

Separately, declarative OWL is not guarded at all (a deliberate call at the time — "definitely no OWL"), left to the `reason-verify` gate.

Worth revisiting now the bootstrap is past: are domain/range and declarative OWL genuinely out of scope, or were they excluded to get the ratchet landed?

### 2. The grandfathered seed is large and includes cross-slice debt

The strict owner-boundary plus rdfs plus SHACL-obligation counting produced **124 grandfathered ceiling cells** (against 33 under the narrower design), including cross-slice external-reference debt such as correspondence cells sitting in non-owner core slices. This is intended debt, seeded at measured and lower-only — migration ratchets it down. But nothing tracks the burn-down, and cells that are structurally in the wrong slice will never burn down by authoring; they need re-homing.

### 3. The live-measured view is not shippable

`gmeow-dev slice-quality-projection-debt` (`crates/gmeow-dev-cli/src/dev_slice_quality.rs:1588`) is the live migration dashboard — measured residue, headroom, what to migrate next — and it needs a repo checkout to scan. The shippable consumer surface `gmeow slice projection-ceilings` (`crates/gmeow-cli/src/commands.rs:4967`) exposes the **commitments** view only: the registry plus the committed ceilings read from the bundle. A downstream consumer can see what was promised but not how close anything is to breaching.

## Work

- [ ] Decide whether `rdfs:domain`/`range` join the counted set now the bootstrap is resolved.
- [ ] Decide whether declarative OWL gets its own count kind or stays with `reason-verify`.
- [ ] Triage the 124 seeded cells: which burn down by grounding, which need re-homing, which are permanent.
- [ ] Give the shippable surface a measured/headroom view, or state why measurement must stay repo-anchored.


## Comments (4)

### coderabbitai — 2026-07-27T00:17:43Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#1387 - feat(logic): derive the full closed-world validation-shape fragment from slice constraint axioms [merged]
Blackcat-Informatics/gmeow-ontology#1469 - Derive SSSOM rows, lift rights, and publish slice-quality report [merged]
Blackcat-Informatics/gmeow-ontology#1473 - feat(logic): reasoner-safe shape grounding foundation + Shape Migration quality axis (Part of `#1193`) [merged]
Blackcat-Informatics/gmeow-ontology#1488 - feat(slice-quality): ontology-resident axis-floor commitments + data-driven multi-axis gate + coherence + shape-grounding floors [merged]
Blackcat-Informatics/gmeow-ontology#1561 - feat(grounding): close logic, math, and lang authority gaps [merged]
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

### paudley — 2026-07-27T07:10:55Z

Adjacency worth naming so these two do not get worked as one: #1201 generalizes the **projection-purity gate** (no hand-authored construct that belongs to a generated surface), while this issue widens the **projection-vocabulary ratchet** (which predicates count toward a slice's ungrounded-residue ceiling). Different mechanisms — a purity violation is categorical, a ratchet breach is quantitative — but they cover overlapping vocabulary, so whoever takes either should read the other first. In particular, a decision on #1201 about which generated-formalism constructs are authored-forbidden constrains what this issue can sensibly count.

### paudley — 2026-09-05T03:05:23Z

## Unblocked — #1642 is closed

This issue records:

> Blocked by: #1642 — which owns the adjacent question of whether the ratchet should net a relocation across its source and destination cells.

**#1642 is CLOSED** ("slice-quality: close the last 4 axis floors and adjudicate…"), landed via PR #1661 (`fix(1642,1658,1659): relocation-aware ceiling accounting; unblock main`). The relocation-accounting question this was waiting on has been adjudicated, so the blocker no longer applies.

Worth re-reading #1642's resolution before starting here: it decided the relocation-netting semantics, which bears directly on this issue's §2 (the cross-slice debt cells that "will never burn down by authoring; they need re-homing").

*Method: issue-state check only. The three substantive claims in this issue (the `countKindStructuralAxiom` narrowing, the 124 grandfathered cells, the dev-only debt view) were not re-verified.*

### paudley — 2026-09-06T02:51:21Z

Unblocking note: the stated blocker **#1642 is now CLOSED**, so this issue is no longer gated on the relocation-netting question. The four work items here can be picked up on their own merits.

