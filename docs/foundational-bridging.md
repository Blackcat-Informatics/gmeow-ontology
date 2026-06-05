<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Foundational bridging — gUFO ↔ BFO (and the upper-ontology spine)

> **Status.** BFO 2020 (ISO/IEC 21838-2) is bridged and verified. DOLCE/DUL and SUMO
> are planned next (see [Extending to a new upper ontology](#extending-to-a-new-upper-ontology)).
> Authoring source: [`mapping-dsl/foundational/`](../mapping-dsl/foundational/). Issue #40,
> the final phase of the reasoning-depth epic #35.

GMEOW grounds every class in **gUFO** ([`docs/reasoning.md`](./reasoning.md) — the
meta-grounding pun, issue #37). But gUFO itself was an **island**: it grounded GMEOW
without being aligned to any *other* top-level ontology, so a GMEOW graph could not
interoperate with the OBO-Foundry / ISO/IEC 21838 world (BFO) or the descriptive
DOLCE/SUMO lineage. This module bridges that spine.

It is [Principle 5](../CONSTITUTION.md) (*maximal bridging — by reference*) applied
**recursively**: the same discipline that links GMEOW's surface terms to FOAF / schema.org /
PROV-O is applied to GMEOW's *foundational* terms. And it costs nothing in the reasoned core,
because it is **link-only** — we assert `skos:closeMatch` triples and import nothing.

---

## The one thing to get right: bridge gUFO's *nature*, not its *stereotypes*

GMEOW puns each class with a gUFO **stereotype meta-class** — `gufo:Kind`, `gufo:SubKind`,
`gufo:Category`, `gufo:Relator`, `gufo:EventType`, … (the OntoUML stereotype system, issue #37).
**BFO has no meta-level.** It is a flat realist taxonomy of *ground categories* — `entity`,
`continuant`, `occurrent`, `material entity`, `quality`, … There is no BFO term that corresponds
to "Kind" or "Role"; those are *modes of classification*, not *categories of being*.

So the bridge aligns gUFO's **nature / category classes** (`gufo:Endurant`, `gufo:Object`,
`gufo:Event`, `gufo:Relator`, `gufo:Quality`, …) — the classes that say *what kind of thing an
individual is* — to BFO's ground categories. Mapping a stereotype (`gufo:Kind`) to a BFO class
would be a **category error**. This is why the cells live on `gufo:` subjects, never on the
GMEOW classes or the stereotypes.

Every cell is `skos:closeMatch`, never `owl:equivalentClass`: UFO and BFO reconstruct their
categories on **different philosophical bases** (UFO is cognitively/linguistically motivated;
BFO is a realist 3+1D ontology), so no pair is exactly equivalent. `closeMatch` is the honest
predicate and — being SKOS, not OWL — keeps the bridge out of the DL TBox entirely.

---

## The bridge (gUFO → BFO 2020)

Authored in [`mapping-dsl/foundational/gufo-bfo.ttl`](../mapping-dsl/foundational/gufo-bfo.ttl),
compiled to [`mappings/gmeow-foundational.sssom.tsv`](../mappings/gmeow-foundational.sssom.tsv).

| gUFO (nature) | → | BFO class | conf | rationale |
|---|---|---|---|---|
| `gufo:Endurant` | closeMatch | `BFO_0000002` *continuant* | 0.9 | endures through time, keeps identity |
| `gufo:Object` | closeMatch | `BFO_0000040` *material entity* | 0.8 | independent endurant with matter; its subclasses are material |
| `gufo:FunctionalComplex` | closeMatch | `BFO_0000030` *object* | 0.85 | a maximal, causally-unified material whole |
| `gufo:Collection` | closeMatch | `BFO_0000027` *object aggregate* | 0.85 | a mereological sum of objects |
| `gufo:Relator` | closeMatch | `BFO_0000020` *specifically dependent continuant* | 0.8 | a reified relationship dependent on its relata |
| `gufo:Quality` | closeMatch | `BFO_0000019` *quality* | 0.85 | an intrinsic aspect measurable in a value space |
| `gufo:Event` | closeMatch | `BFO_0000003` *occurrent* | 0.85 | occurs/happens in time |

**Two cells deliberately diverge from the sketch in issue #40**, because Principle 1 (*SOTA by
being SOTA*) says model it correctly rather than inherit a weak mapping:

- **`gufo:Object` → `material entity` (`BFO_0000040`), not `object` (`BFO_0000030`).** `gufo:Object`
  is *any* independent endurant; its gUFO subclasses `FunctionalComplex` / `Collection` / `Quantity`
  span BFO's *object* / *object aggregate* / *fiat object part* — i.e. exactly `material entity`,
  the union of those three. `BFO_0000030` *object* is the narrower causally-unified case, which is
  why `gufo:FunctionalComplex` (a structured whole) is the cell that maps there.
- **`gufo:Event` → `occurrent` (`BFO_0000003`), not `process` (`BFO_0000015`).** A `gufo:Event`
  *may be instantaneous*; BFO classes instantaneous happenings as *process boundaries*, not
  *processes*. `occurrent` is the correct superclass that covers both.

### Recorded gaps (categories with no BFO counterpart)

Honesty over coverage — these are **not** forced into a cell:

| gUFO | why no BFO cell |
|---|---|
| `gufo:Situation` | BFO has no reified *state of affairs / configuration*. (DOLCE **does** — `dul:Situation` — so this gap closes when DOLCE lands.) |
| `gufo:AbstractIndividual`, `gufo:QualityValue` | BFO is a **realist** ontology with no abstracta — quality *values* (a value space) have no home in BFO. |
| all gUFO **stereotypes** (`Kind`, `SubKind`, `Category`, `RoleMixin`, `EventType`, `SituationType`, `AbstractIndividualType`) | BFO has no meta-level (see above). |

Coverage is therefore **7 of the ~9 bridgeable nature categories GMEOW uses**; the remainder are
genuine ontological gaps, documented rather than papered over (the "no silent caps" rule).

---

## How it is verified (and why you can trust it offline)

The bridge has a two-rail verification, so a mistyped or invented BFO IRI fails the gate:

1. **Offline, deterministic (always runs).** `gmeow refresh-target-axioms --target bfo` vendors a
   minimal snapshot of BFO's **class facts** to [`imports/targets/bfo.ttl`](../imports/targets/)
   (every `owl:Class`, its in-namespace `rdfs:subClassOf` parents, and its `rdfs:label`).
   [`tests/test_foundational_bridging.py`](../tests/test_foundational_bridging.py) asserts that
   **every emitted `bfo:` IRI is a declared `owl:Class` in that snapshot, with an `object_label`
   matching BFO's own label**. CI needs no network.
2. **Online, freshness (network-marked).** `test_vendored_snapshot_matches_live_bfo`
   (`@pytest.mark.network`) re-fetches live BFO and re-checks the same IRIs + labels, so the
   offline snapshot cannot silently rot. Run it with `uv run pytest -m network` or `make quality`.

The snapshot lives in `imports/targets/` — a **subdirectory** of `imports/` that
`graph.iter_import_files()` does **not** glob (it globs `imports/*.ttl` non-recursively). So no
BFO axiom ever enters the reasoned import closure or the published CC BY 4.0 artifact. The
`test_bridge_is_link_only_no_import` test asserts exactly this: zero BFO classes in the merged
reasoned graph.

> **Why a *class*-shaped snapshot?** The same `imports/targets/` machinery (issue #25) vendors
> *property*-axiom snapshots (domain/range/inverse) for the alignment-direction linter. Upper
> ontologies are bridged at the **class** level, so `fetch_target_axioms()` switches snapshot
> *shape* on the target's `kind` (`AlignmentTarget.kind == "upper"` → class facts; `schema` /
> `concept_scheme` → property axioms). See [`src/gmeow_tools/target_axioms.py`](../src/gmeow_tools/target_axioms.py).

---

## Maintaining this bridge

### Refreshing the BFO snapshot

BFO 2020 is stable (ISO standard), so the snapshot rarely changes. To refresh after a BFO
release — or if the network freshness test ever fails:

```bash
uv run gmeow refresh-target-axioms --target bfo   # re-vendors imports/targets/bfo.ttl
uv run gmeow compile-mappings --check             # confirm no mapping drift
uv run pytest tests/test_foundational_bridging.py # offline cell + IRI verification
uv run pytest tests/test_foundational_bridging.py -m network  # vs live BFO
git add imports/targets/bfo.ttl                   # commit the refreshed snapshot
```

If the freshness test fails because BFO **renamed or removed** a class you reference, fix the
cell in `mapping-dsl/foundational/gufo-bfo.ttl` (the snapshot is the source of truth for *what
exists*; the DSL is the source of truth for *what we claim*) and recompile.

### Editing or adding a cell

1. Edit [`mapping-dsl/foundational/gufo-bfo.ttl`](../mapping-dsl/foundational/gufo-bfo.ttl).
   Copy an existing `gmeow:TermEquivalence` block; set `alignSubject` to a `gufo:` **nature**
   class, `alignObject` to a `bfo:` class, keep `alignPredicate skos:closeMatch`, set a
   calibrated `confidence` and the BFO `objectLabel`, and explain non-obvious choices in
   `gmeow:comment`. **Verify the BFO IRI first** — `grep BFO_00000NN imports/targets/bfo.ttl`,
   or look it up at <https://ontobee.org/ontology/BFO>.
2. Recompile and verify: `uv run gmeow compile-mappings && uv run gmeow compile-mappings --check`.
3. If the cell is one a reader would expect to see asserted, add it to `EXPECTED_CELLS` in
   [`tests/test_foundational_bridging.py`](../tests/test_foundational_bridging.py) — the IRI +
   label are then verified automatically.
4. Run the full alignment gate: `make compile-check && make lint-alignment && uv run pytest`.

Do **not** hand-edit `mappings/gmeow-foundational.sssom.tsv` — it is generated; the no-drift gate
(`compile-mappings --check`) will reject hand edits.

### Extending to a new upper ontology

The infrastructure already generalises to any upper ontology (DOLCE/DUL, SUMO, UMBEL). To add one
— e.g. **DOLCE/DUL**, which would close the `gufo:Situation` gap via `dul:Situation`:

1. **Register the target** in [`src/gmeow_tools/config.py`](../src/gmeow_tools/config.py): a
   `PREFIXES` entry and an `ALIGNMENT_TARGETS` entry with `kind="upper"`. *Mind the license.*
   `policy_for_license` classifies DOLCE/DUL (LGPL) as **REFERENCE_ONLY** — so its axioms may be
   *linked* but never *vendored*. (BFO is CC-BY-4.0 → IMPORT_OK, which is why we can vendor its
   snapshot.) `dolce` and `umbel` are already registered.
2. **Pick the snapshot strategy by policy:**
   - *IMPORT_OK* (e.g. a CC-BY upper ontology): add a `TARGET_SOURCES` entry in
     [`target_axioms.py`](../src/gmeow_tools/target_axioms.py) and
     `gmeow refresh-target-axioms --target <prefix>` — you get an offline class snapshot for free,
     and the verification test works exactly as BFO's does.
   - *REFERENCE_ONLY* (DOLCE/DUL): `refresh_snapshot` **refuses** to vendor it. Verify its IRIs
     with a **network-only** test (`@pytest.mark.network`) that fetches the live ontology, plus a
     tiny hand-authored fixture under `tests/fixtures/target_axioms/` for the handful of classes
     you reference offline — the same pattern schema.org uses.
3. **Author** `mapping-dsl/foundational/gufo-<name>.ttl` (one mapping set, `skos:closeMatch`
   cells). Recompile; the recursive `rglob` in `load_dsl()` discovers the new file automatically —
   no compiler change needed.
4. **Document** the new bridge and its gaps in this file, and tick it off below.

> **SUMO** is lowest-payoff and not yet registered in `config.py`; it is the natural third target
> after DOLCE. Tracked as a follow-up to #40/#35.

---

## References

- gUFO — NEMO/UFES lightweight OWL 2 DL UFO: <http://purl.org/nemo/gufo#>; Almeida et al.,
  *gUFO: A Lightweight Implementation of the Unified Foundational Ontology (UFO)*.
- BFO 2020 = ISO/IEC 21838-2:2021; OBO-Foundry top-level: <https://github.com/BFO-ontology/BFO-2020>;
  class IRIs browsable at <https://ontobee.org/ontology/BFO>.
- UFO ↔ BFO ↔ DOLCE correspondences: Guizzardi et al., *UFO* (Applied Ontology 17(1), 2022);
  Trojahn et al., *Foundational ontologies meet ontology matching: a survey* (SWJ 2022).
- CONSTITUTION Principles **1** (SOTA by being SOTA), **5** (maximal bridging — by reference),
  **7** (verified by construction), **8** (FAIR).
- The single-source alignment stack: [`docs/projections.md`](./projections.md); the gUFO
  meta-grounding it bridges from: [`docs/reasoning.md`](./reasoning.md).
