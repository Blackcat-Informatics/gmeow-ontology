# Issue #1462: Native agentic-memory & music-projection loss-accounting conformance

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, music, dependencies, rust, complexity-hard, rustify

## Body

## Problem statement

The repo-wide removal of the Python surface deletes `test_agentic.py` and
`slices/extensions/music/tests/test_music_notation_projection.py`, which pinned the
**agentic-memory provenance** and **music notation-projection loss-accounting** invariants.
These have **no native Rust twin yet**. This issue tracks recreating that coverage natively.

## Coverage to recreate in Rust (native conformance)

- **Music projection loss-accounting completeness:** for every `NotationProjectionProfile`, the
  union of its representable set and its declared-loss set covers every `MusicalParameter`; there
  is no orphan `ProjectionLoss`; and there is exactly one profile per `NotationSystem`.
- **Agentic-memory provenance:** the tool-call / claim memory layer's provenance store keeps
  verbatim content below a size threshold (~5000 chars) and switches to a `blake3:` digest above it,
  with faithful readback. **Precondition:** this applies only once the memory layer is reimplemented
  natively; if that layer is not reimplemented, this coverage retires with the Python demo it was
  written against and should be explicitly dropped rather than ported.

## Consumer

The `make check` music + agentic gates — native Rust, no Python.

## Target location

`crates/validate/tests/` (music loss-accounting conformance over the music slice) and the native
memory layer's crate tests (agentic), if/when that layer exists in Rust.


## Comments (3)

### coderabbitai — 2026-07-08T08:12:48Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#622 - feat(validate): mandatory SHACL+coverage gates; Rust-native validation path (`#579`) [closed]
Blackcat-Informatics/gmeow-ontology#971 - test(867): migrate the run_shacl conformance tail — suite now 158 native tests [merged]
Blackcat-Informatics/gmeow-ontology#1131 - feat: move repo static guards to Rust gates [merged]
Blackcat-Informatics/gmeow-ontology#1185 - perf: optimize Rust gate hot paths [merged]
Blackcat-Informatics/gmeow-ontology#1388 - test(validate): migrate the SHACL/TBox black-box test cluster to native Rust conformance [merged]
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

### paudley — 2026-09-05T03:06:27Z

## The music half looks recreated; the agentic half may be retirable

Checked against `main` @ c2b03aba6.

**The Python originals are gone**, as this issue anticipated — neither `test_agentic.py` nor `test_music_notation_projection.py` is tracked.

**Music projection loss-accounting appears to have a native twin.** `crates/gmeow-dev-cli/tests/loss_bearing_domain.rs` carries 8 references to `NotationProjectionProfile` / `ProjectionLoss` / representable-set terms, and `slices/extensions/music/tests/structural.ttl` exists. Someone who knows the original invariant should confirm the native version actually covers all three clauses — representable ∪ declared-loss covers every `MusicalParameter`, no orphan `ProjectionLoss`, exactly one profile per `NotationSystem` — rather than a subset.

**The agentic-memory half turns on the precondition this issue already stated:**

> **Precondition:** this applies only once the memory layer is reimplemented natively; if that layer is not reimplemented, this coverage retires with the Python demo it was written against and should be explicitly dropped rather than ported.

That precondition is now decidable. If the tool-call/claim memory layer has not been reimplemented natively, the right action is to **explicitly drop** this half — as the issue itself directs — rather than leave it open indefinitely.

Suggested outcome: verify the music clauses, drop or keep the agentic half per the precondition, then close. This looks closeable without new implementation work.

*Method: file-existence and symbol-reference checks, not a review of what the native test asserts.*

### paudley — 2026-09-05T03:27:48Z

## Correction to my previous comment — the music half is NOT recreated

I said "Music projection loss-accounting appears to have a native twin" on the strength of symbol references in `crates/gmeow-dev-cli/tests/loss_bearing_domain.rs`. I then read what those tests actually assert, and they do not cover this issue's invariant. Correcting that rather than leaving it to mislead whoever picks this up.

**What the existing native tests actually check:**

- `crates/gmeow-dev-cli/tests/loss_bearing_domain.rs` — that declared loss appears *only* on loss-bearing subjects (`the_music_manifest_declares_loss_only_on_a_loss_bearing_subject`), and that `gmeow:NotationProjectionProfile` stays a `gmeow:Profile` / is declared `logic:subClassOf` its category. That is a **domain** check on where loss may be declared, not a completeness check.
- `crates/validate/tests/conformance_cases/conformance_music_performance.rs` — that the `MusicalParameter` class and the ten `musicalParameter*` value-vocabulary individuals **exist**. Existence, not coverage.
- `slices/extensions/music/tests/structural.ttl` — no `MusicalParameter`, `ProjectionLoss` or `NotationSystem` assertions at all.

**All three clauses of this issue remain unimplemented:**

1. for every `NotationProjectionProfile`, representable ∪ declared-loss covers every `MusicalParameter` — **no assertion anywhere**
2. no orphan `ProjectionLoss` — **no assertion anywhere**
3. exactly one profile per `NotationSystem` — **no assertion anywhere**

So the music half is real, open work: three structural assertions over the music module, most naturally as SPARQL in `slices/extensions/music/tests/structural.ttl` alongside the slice's existing structural checks.

My earlier point about the **agentic-memory half** stands unchanged and is unaffected: its precondition is now decidable, and if the memory layer was not reimplemented natively, this issue's own text directs dropping that half explicitly rather than porting it.

Net: this is **not** closeable without implementation — one bounded task (three assertions) plus one decision.

*Method: read the bodies of the two candidate native tests and the slice's structural.ttl, rather than counting symbol references as I did the first time.*

