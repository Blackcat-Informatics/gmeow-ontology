<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Grounding fixtures for the openEHR ⇄ GMEOW correspondence use cases

These artifacts ground the data-axis and process-axis use cases
(`../usecase_openehr_bloodpressure.md`, `../usecase_openehr_taskplan_rchops21.md`) and the spec
(`../take1.md` §13) against real data.

## Files

| File | What it is | Provenance |
|---|---|---|
| `blood_pressure.source.json` | the unmodified openEHR `Blutdruck` COMPOSITION (RM instance) | **vendored** from [Genkidata](https://github.com/Berlin-Institute-of-Health/Genkidata) `compositions/blood_pressure.json`, © Berlin Institute of Health, **Apache-2.0** (see *Attribution*) |
| `blood_pressure.complement.ttl` | the GMEOW in-band complement (`S ∖ im(get)`) | GMEOW (CC-BY-4.0) |
| `blood_pressure.augmented.json` | `⟨ openEHR file ⊕ gmeow complement ⟩` — the down-projection artifact `d(g)`; the complement rides in `feeder_audit.original_content` (DV_PARSABLE, `text/turtle`) + a COMPOSITION `LINK` | derived (source Apache-2.0 + GMEOW complement CC-BY-4.0) |
| `rchops21.plan.ttl` | RCHOPS21 chemotherapy as a GMEOW `logic:Plan` | GMEOW (CC-BY-4.0) rendering derived from openEHR PROC 1.6.0; openEHR DLM **not** vendored (cited only) |

## The empirical slot test (the part to run against the validator zoo)

`take1.md` §13.4-Q1 / §17 asks whether the in-band complement is **validation-transparent**: does
`blood_pressure.augmented.json` still validate under `Blutdruck.opt`, exactly as
`blood_pressure.source.json` does? The artifact is built so this is a single probe. The spec-level
prediction is **PASS** (the complement rides in RM-level `feeder_audit`/`links`, which the OPT does
not constrain — OPTs constrain archetyped *content*, not the audit envelope). To confirm
empirically against real tooling (requires Java / a running CDR — not in GMEOW's Docker-free gate):

```sh
# Option A — EHRbase (a running openEHR CDR): upload the OPT, then both compositions.
#   POST /rest/openehr/v1/definition/template/adl1.4   (Blutdruck.opt)
#   POST /rest/openehr/v1/ehr/{ehr_id}/composition      (each .json)   → expect 204/201 for BOTH
#
# Option B — Archie (org.openehr:archie) RM/template validation in a tiny Java/Kotlin harness:
#   load Blutdruck.opt → build the in-memory template → validate(parse(<file>))
#   expect zero errors for BOTH source and augmented.
#
# PASS  ⇒ "perfectly replace openEHR" holds for openEHR-EHR-OBSERVATION.blood_pressure.v2.
# FAIL  ⇒ the rejecting field is the exact, nameable boundary of GMEOW's subsumption (record it).
```

Then verify the round trip `u(d(g)) = g`: parse `blood_pressure.augmented.json`, read the RM slice
**and** the `feeder_audit.original_content` Turtle, and confirm the reconstructed GMEOW graph is
canonical-IR-identical to `blood_pressure.complement.ttl` ⊕ the RM-derived facts (the Round-trip
gate, `take1.md` §15.3).

## Attribution (Apache-2.0, Genkidata)

`blood_pressure.source.json` (and the structure of `blood_pressure.augmented.json`) derive from
the Genkidata project — OpenEHR sample data, Berlin Institute of Health — licensed Apache-2.0.
The underlying `Blutdruck` template and the `openEHR-EHR-OBSERVATION.blood_pressure.v2` /
`...COMPOSITION.registereintrag.v1` archetypes are from the GECCO / NUM dataset (Peter L. Reichertz
Institut für Medizinische Informatik). No warranty; values are synthetic (systolic magnitude `1.0`
is a test value, not clinical data).
