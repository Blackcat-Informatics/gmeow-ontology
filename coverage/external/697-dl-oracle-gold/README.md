<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# #697 native ⊇ oracle DL gold

A frozen, **oracle-generated** conformance corpus proving that GMEOW's native,
Docker-free reasoner catches every inconsistency / unsatisfiable class the classic
Java OWL 2 DL oracle (HermiT) catches — `native ⊇ oracle` (issue #697 criteria 2
and 4). The gold here is the **oracle's** verdict, frozen from a real HermiT run;
the native engine is then asserted to reproduce it OFFLINE.

## Layout

| path | role |
| --- | --- |
| `datasets/*.ttl` | curated minimal OWL/gUFO ontologies (we author the **axioms**) |
| `expected/*.json` | the **frozen oracle verdict** per dataset (the oracle authors the **verdict**) |

Each `expected/<stem>.json` records: `consistent`, the sorted set of
`unsatisfiable_classes`, the producing oracle + Docker image + UTC timestamp +
dataset license, and a cross-reference ELK verdict (recorded for context only —
ELK is the EL oracle and silently ignores beyond-EL axioms, so it is **not** the
authority).

## Construct families covered

| dataset | family | HermiT verdict |
| --- | --- | --- |
| `disjoint-subclass-unsat.ttl` | class disjointness ⇒ unsatisfiable class | consistent, 1 unsat |
| `complementof-unsat.ttl` | `owl:complementOf` ⇒ unsatisfiable class | consistent, 1 unsat |
| `disjoint-instance-inconsistent.ttl` | disjointness on an individual | inconsistent |
| `some-all-values-clash.ttl` | `∃p.C ⊓ ∀p.D`, C ⊓ D = ⊥ (beyond-EL; ELK misses it) | inconsistent |
| `max-cardinality-clash.ttl` | `≤1 p` with two distinct fillers | inconsistent |
| `qualified-cardinality-clash.ttl` | `≤1 p.C` with two distinct C-fillers | inconsistent |
| `oneof-nominal-clash.ttl` | nominal / `owl:oneOf` closure | inconsistent |
| `domain-range-clash.ttl` | `rdfs:range` inference into a disjointness clash | inconsistent |
| `property-chain-clash.ttl` | `owl:propertyChainAxiom` ⇒ range ⇒ disjointness clash | inconsistent |
| `consistent-baseline.ttl` | satisfiable sanity baseline (restrictions + chain) | consistent, 0 unsat |

## Provenance + honesty

* **Producing oracle:** ROBOT/HermiT, image `obolibrary/robot:v1.9.7`
  (HermiT is the sound-and-complete OWL 2 DL reasoner; the authority for every
  beyond-EL family).
* **Dataset license:** CC-BY-4.0 (hand-authored here; same license as the gUFO
  alignment the project dogfoods).
* The frozen verdicts are **never** hand-typed to match native and **never**
  edited to make a test pass (issue #697 honesty doctrine). They come straight
  from a real HermiT run.

## Regenerating the gold

```sh
make maint-697-oracle-gold      # non-required maintainer lane; needs Docker + ROBOT image
```

This re-runs HermiT (and, where the dataset is EL-decidable, ELK) in Docker over
every `datasets/*.ttl` and rewrites every `expected/*.json`. It is the only step
that needs Docker/Java; the conformance gate below does not.

## The offline gate

`crates/conformance/tests/dl_oracle_gold.rs` —
`native_reasoner_is_a_superset_of_the_frozen_oracle_gold` — loads each dataset,
runs native `gmeow_logic::reason::reason_all`, and asserts native ⊇ oracle:

* an oracle **inconsistency** must be a native inconsistency;
* an oracle **unsatisfiable class** must be in native's unsat set (native may be a
  superset — that is allowed and is the point).

It needs **no Docker, Java, or network** and is deterministic, so it runs in the
default `cargo nextest` / `make conformance` gate.

## Full-bundle oracle: BLOCKED (documented)

Running the oracle over the *entire* GMEOW bundle (`gmeow-dev reason --mode docker
--reasoner hermit|ELK`) is currently **blocked** at the ROBOT OWL 2 DL profile
check, reproduced 2026-06-25:

```text
PROFILE VIOLATION ERROR https://blackcatinformatics.ca/gmeow/full violates profile DL
  Use of reserved vocabulary for class IRI: rdfs:Resource
    [Declaration(Class(rdfs:Resource))]
  Use of reserved vocabulary for class IRI: rdfs:Resource
    [ObjectPropertyRange(<…/usesTerm> rdfs:Resource)]
```

`gmeow:usesTerm`'s `rdfs:Resource` range (and a `Declaration(Class(rdfs:Resource))`)
sit outside OWL 2 DL, so ROBOT/HermiT refuses to reason over the merged bundle.
This is the PR's known blocker; the curated small datasets above are where
HermiT/ELK run clean, so the frozen gold is scoped to them — not faked over the
bundle.
