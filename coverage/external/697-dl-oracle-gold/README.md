<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# native ⊇ oracle DL gold

A frozen, **oracle-generated** conformance corpus proving that GMEOW's native,
Docker-free reasoner catches every inconsistency / unsatisfiable class the external
OWL 2 DL oracle catches — `native ⊇ oracle` (criteria 2
and 4 of the cross-check policy). The gold here is the **oracle's** verdict, frozen from a real external OWL 2 DL oracle run;
the native engine is then asserted to reproduce it OFFLINE.

## Layout

| path | role |
| --- | --- |
| `datasets/*.ttl` | curated minimal OWL/gUFO ontologies (we author the **axioms**) |
| `expected/*.json` | the **frozen oracle verdict** per dataset (the oracle authors the **verdict**) |

Each `expected/<stem>.json` records: `consistent`, the sorted set of
`unsatisfiable_classes`, the producing oracle + Docker image + UTC timestamp +
dataset license, and a cross-reference EL-oracle verdict (recorded for context only —
the EL oracle silently ignores beyond-EL axioms, so it is **not** the
authority).

## Construct families covered

| dataset | family | DL oracle verdict |
| --- | --- | --- |
| `disjoint-subclass-unsat.ttl` | class disjointness ⇒ unsatisfiable class | consistent, 1 unsat |
| `complementof-unsat.ttl` | `owl:complementOf` ⇒ unsatisfiable class | consistent, 1 unsat |
| `disjoint-instance-inconsistent.ttl` | disjointness on an individual | inconsistent |
| `some-all-values-clash.ttl` | `∃p.C ⊓ ∀p.D`, C ⊓ D = ⊥ (beyond-EL; the EL oracle misses it) | inconsistent |
| `max-cardinality-clash.ttl` | `≤1 p` with two distinct fillers | inconsistent |
| `qualified-cardinality-clash.ttl` | `≤1 p.C` with two distinct C-fillers | inconsistent |
| `oneof-nominal-clash.ttl` | nominal / `owl:oneOf` closure | inconsistent |
| `domain-range-clash.ttl` | `rdfs:range` inference into a disjointness clash | inconsistent |
| `property-chain-clash.ttl` | `owl:propertyChainAxiom` ⇒ range ⇒ disjointness clash | inconsistent |
| `consistent-baseline.ttl` | satisfiable sanity baseline (restrictions + chain) | consistent, 0 unsat |

## Provenance + honesty

* **Producing oracle:** an external OWL 2 DL oracle in a pinned Docker image
  (a sound-and-complete OWL 2 DL reasoner; the authority for every
  beyond-EL family).
* **Dataset license:** CC-BY-4.0 (hand-authored here; same license as the gUFO
  alignment the project dogfoods).
* The frozen verdicts are **never** hand-typed to match native and **never**
  edited to make a test pass (honesty doctrine). They come straight
  from a real external OWL 2 DL oracle run.

## Regenerating the gold

The gold is **permanently frozen** from its historical external OWL 2 DL oracle run: the
Docker/Java external DL oracle stack (and its regeneration lane) has been **removed**, so the
`expected/*.json` verdicts are no longer regenerated in-repo. There is **no live
differential reasoning oracle on any gate** — the `purrdf::entail`-vs-native cross-check
lane was retired end-to-end. The reasoner that runs on-gate is GMEOW's own native,
in-process, Docker-free `logic:` reasoner, exercised by `make reason-verify`, and it is
the **sole reasoning authority**; `purrdf` remains only a runtime RDF-parsing dependency,
not a live reasoning oracle. This frozen corpus is kept as an independent
external-oracle-authored baseline that the native reasoner must still strictly cover,
checked offline by the gate below.

## The offline gate

`crates/conformance/tests/dl_oracle_gold.rs` —
`native_reasoner_is_a_superset_of_the_frozen_oracle_gold` — loads each dataset,
runs native `gmeow_logic::reason::reason_all`, and asserts native ⊇ oracle:

* an oracle **inconsistency** must be a native inconsistency;
* an oracle **unsatisfiable class** must be in native's unsat set (native may be a
  superset — that is allowed and is the point).

It needs **no Docker, Java, or network** and is deterministic, so it runs in the
default `cargo nextest` / `make conformance` gate.

## Full-bundle oracle: historically BLOCKED, now removed

Running the external oracle over the *entire* GMEOW bundle was never possible: the
Docker reasoning path (`gmeow-dev reason --mode docker`) has since been **removed**
entirely — it now hard-fails, since the native binary embeds no external DL oracle container
stack. Even before removal it was **blocked** at the external OWL 2 DL profile check
(reproduced 2026-06-25):

```text
PROFILE VIOLATION ERROR https://blackcatinformatics.ca/gmeow/full violates profile DL
  Use of reserved vocabulary for class IRI: rdfs:Resource
    [Declaration(Class(rdfs:Resource))]
  Use of reserved vocabulary for class IRI: rdfs:Resource
    [ObjectPropertyRange(<…/usesTerm> rdfs:Resource)]
```

`gmeow:usesTerm`'s `rdfs:Resource` range (and a `Declaration(Class(rdfs:Resource))`)
sit outside OWL 2 DL, so the external DL oracle refused to reason over the merged bundle. The
curated small datasets above are where the external DL/EL oracles ran clean, so the frozen gold is
scoped to them — not faked over the bundle. Full-bundle reasoning is now handled solely by
the native `logic:` EL/DL engine (`make reason-verify`), which needs no OWL 2 DL profile
conformance and has no live differential oracle counterpart.
