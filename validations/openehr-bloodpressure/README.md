<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# openEHR blood-pressure validator-zoo lane

Confirms, against an **openEHR reference validator**, the empirical half of the F6
in-band-complement claim: that the down-projection `d(g)` —
`blood_pressure.augmented.json`, a valid openEHR composition carrying the GMEOW complement
in `feeder_audit.original_content` — still validates under the real `Blutdruck.opt`
Operational Template, exactly as the unmodified `blood_pressure.source.json` does.

The **structural** and **data** halves of the round trip are already proven inside the
repo's `make check` gate (the `correspondence/openehr-bloodpressure-section-retraction`
conformance case and the `crates/logic-compile` real-data round-trip test). This lane is the
**external, empirical** confirmation, which needs an openEHR CDR or the Archie Java library —
tooling outside GMEOW's Docker-free gate — so it lives here, run on demand, never in CI.

## What it uses

- `Blutdruck.opt` — the openEHR Operational Template, **vendored** from the Genkidata
  corpus (Apache-2.0; see `Blutdruck.opt.license`). This is the AM-layer constraint the
  systolic/diastolic `C_DV_QUANTITY` half-open `[0, 1000)` mm[Hg] magnitude lives in.
- The compositions are referenced from the single source of truth,
  `../../docs/APPLIED_CATEGORY_THEORY/fixtures/` — they are **not** duplicated here.

## Option A — EHRbase CDR (Docker; the default)

Prerequisites: Docker + the Docker Compose plugin.

```bash
make -C validations/openehr-bloodpressure validate
```

This brings up EHRbase + Postgres (`docker-compose.yml`), uploads `Blutdruck.opt`, creates
an EHR, POSTs `blood_pressure.source.json` then `blood_pressure.augmented.json`, tears the
stack down, and writes the outcome to `result.txt`. Sub-targets: `make up`, `make probe`,
`make down`.

## Option B — Archie RM validator (Java)

Prerequisites: a JVM + the `org.openehr:archie` library (e.g. via `jbang`). Load
`Blutdruck.opt`, build the in-memory operational template, then parse and validate each of
`blood_pressure.source.json` and `blood_pressure.augmented.json`, expecting zero errors for
both. (Archie is invoked, not authored here — no Java/Python is vendored into this lane.)

## Output contract

The probe prints, and writes to `result.txt`, exactly one of:

- `PASS source+augmented validate under Blutdruck.opt`
- `BOUNDARY <field>: <reason>` — the exact field / HTTP status the validator rejects (the
  honest loss-ledger entry naming the subsumption boundary).

The spec-level prediction (`usecase_openehr_bloodpressure.md` §9) is **PASS**: the complement
rides in RM-level slots (`feeder_audit.original_content`, `links`) that the OPT does not
constrain.

## Honest caveat

`feeder_audit` means "lineage from a *feeder* system". GMEOW is the *canonical* source, not a
feeder, so using `feeder_audit.original_content` as the complement carrier is **mechanically
valid but ontologically borderline**. A PASS here confirms only that the OPT does not
constrain that RM-level slot (validation-transparency) — it does **not** bless the semantic
propriety of the carrier. The clean alternatives are a dedicated RM extension or a
content-hash-bound sidecar (use-case doc §5).
