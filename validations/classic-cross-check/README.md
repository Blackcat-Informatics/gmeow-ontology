<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# classic-cross-check validator-zoo lane

Confirms, against the **classical reasoners' own reference implementations**
(ELK, HermiT, ROBOT, Apache Jena, and the upstream `owlrl` OWL 2 RL engine),
that GMEOW's **native** reasoning / RL-closure / RDF-1.2-statement pipeline
agrees with — or strictly subsumes — what those black-box oracles decide.

This is the **oracle** half of the correspondence-calculus doctrine
(`docs/APPLIED_CATEGORY_THEORY/take1.md` §10.2): the native Rust engine
(`gmeow_logic`, delivered under META-EPIC #1087, F1–F7) is the **authority**;
the classical engines are **demoted to conformance oracles**, subsumed
fragment-by-fragment and gated by the divergence ledger. The native side is
proven inside the repo's Docker-free `make check` gate (native EL/DL reasoning,
RL closure in `crates/logic`, statement round-trip in `crates/pipeline`, and the
foundation-discipline goldens in `crates/logic/src/foundation`). This lane is the
**external, empirical cross-check** against the classical engines — tooling that
needs Docker and/or a JVM, outside GMEOW's Docker-free gate — so it lives here,
run on demand, **never in CI or `make check`**.

## What it uses

- Native GMEOW: the built `gmeow_logic` / `gmeow_rdf` extensions and the
  shared `gmeow_tools` helpers from the repository (run via the repo's `uv`
  environment — a forward dependency, exactly like the sibling
  `openehr-bloodpressure` lane reading `../../docs/.../fixtures`).
- Classical oracles: ELK + HermiT (Docker), ROBOT (Docker), Apache Jena
  (Docker) for the RDF-1.2 statement codec cross-check, and `owlrl` + `rdflib`
  (Python) for the OWL 2 RL agreement lane.

Nothing in the mainline repository references this lane: no `make` target, no CI
workflow, no `gmeow_dev` subcommand, no `pytest` collection path.

## How to run

Prerequisites: Docker + the Docker Compose plugin (ELK/HermiT/ROBOT/Jena), and
the repository's built environment (`make native-py` at the repo root once).

```bash
make -C validations/classic-cross-check help        # list the lane targets
make -C validations/classic-cross-check test        # Docker-free unit tests
make -C validations/classic-cross-check validate    # full lane (needs Docker)
```

Sub-targets: `reasoning-cases`, `statements-check`, `slme`, `oracle-gold`.

## Output contract

Each target reports a single falsifiable outcome — the native verdict **agrees
with / subsumes** the oracle verdict, or a **named divergence** (the exact
`NativeOnly` / `OracleOnly` / `DlGap` row the divergence ledger flags). The
enforced lanes fail hard on any divergence; the diagnostic lanes emit the
agreement matrix + per-tool timing through the `gmeow-diagnostics` SARIF rail.

## Honest caveat

This lane needs heavy external tooling (Docker images, a JVM) precisely because
it lives outside the Docker-free `make check` gate. When the required image or
runtime is absent, the target **fails hard** (it does not silently skip) — a
missing oracle is a run boundary to report, never a faked green.
