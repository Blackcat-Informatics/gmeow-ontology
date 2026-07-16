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
| `blood_pressure.source.ttl` | the reconstructed canonical GMEOW source `S` = the complement ∪ the two asserted `gmeow:measuredValue` leaves re-lifted from the RM slice; the golden for `u(d(S)) = S` | GMEOW (CC-BY-4.0) |
| `blood_pressure.augmented.json` | `⟨ openEHR file ⊕ gmeow complement ⟩` — the down-projection artifact `d(g)`; the complement rides in `feeder_audit.original_content` (DV_PARSABLE, `text/turtle`) + a COMPOSITION `LINK` whose `DV_EHR_URI` target uses the `ehr` scheme (see *The empirical slot test*) | derived (source Apache-2.0 + GMEOW complement CC-BY-4.0) |
| `rchops21.plan.ttl` | RCHOPS21 chemotherapy as a GMEOW `logic:Plan` | GMEOW (CC-BY-4.0) rendering derived from openEHR PROC 1.6.0; openEHR DLM **not** vendored (cited only) |

## Executable proofs (in GMEOW's gate)

The round trip is proven two ways inside `make check` — no external tooling required:

- **Bounded query-class recovery** — the conformance case
  `conformance/logic/cases/correspondence/openehr-bloodpressure-section-retraction/` authors the
  YAMATO canonical graph + a `logic:Correspondence` (`SectionRetraction`, mnemomorphic) whose
  `gmeow:SeqPath` get-leg encodes the `archetype_node_id` path witness and whose
  `logic:RecoveryCase` declares all three source edges. The native executor constructs the view,
  runs candidate put, and recovers every declared source atom; the round-trip + mnemomorphism gates
  pass (`expected/correspondence-gates.json`). This is genuine execution over the bounded case, not
  acceptance of `put = get.invert()`; the larger fixture recovery is the next proof.
- **Data** — `crates/logic-compile/tests/openehr_bloodpressure_roundtrip.rs` reads the *real*
  fixtures and actually reconstructs `S`: the RM systolic/diastolic `DV_QUANTITY` + FHIR lineage are
  byte-preserved between `source` and `augmented` (`d` is additive), and `u` re-lifts each
  `DV_QUANTITY` through the complement's `gmeow:rmPath` witness (`at0004`/`at0005`), unions the
  minted `gmeow:measuredValue` leaves with the parsed complement, and asserts the canonicalization
  equals the golden `blood_pressure.source.ttl`. Because the leaves are read from the RM slice at
  test time, corrupting an RM magnitude fails the test — this is the `u∘d=id` proof, not a
  copy-equality check. (A separate assertion confirms the embedded complement canonicalizes equal to
  `blood_pressure.complement.ttl`, i.e. the carrier survives the RDF-1.2 parser byte-losslessly.)
- **ADL fidelity** — `crates/shacl/src/openehr_opt.rs` reads the systolic/diastolic `C_DV_QUANTITY`
  `magnitude` block straight out of the vendored `Blutdruck.opt` and lowers the half-open
  `[0, 1000)` mm[Hg] to `sh:minInclusive` + `sh:maxExclusive` (never `sh:maxInclusive`);
  `crates/shacl/tests/bloodpressure_halfopen.rs` drives that reader and asserts `value == hi` is
  rejected. Flipping the OPT's `upper_included` to `true` fails the test — boundary inclusivity is
  read from the OPT, not hand-transcribed. (Only the magnitude-interval slice is lowered; the
  general OPT→SHACL constraint lowering is a separate roadmap capability.)

## The empirical slot test (the validator-zoo lane)

`take1.md` §13.4-Q1 / §17 asks whether the in-band complement is **validation-transparent**: does
`blood_pressure.augmented.json` still validate under `Blutdruck.opt`, exactly as
`blood_pressure.source.json` does? This is the external, empirical half — it needs an openEHR CDR or
the Archie Java library, outside GMEOW's Docker-free gate — so it lives in the standalone lane
[`validations/openehr-bloodpressure/`](../../../validations/openehr-bloodpressure/) (vendored
`Blutdruck.opt` + an EHRbase Docker probe). Run it with `make -C validations/openehr-bloodpressure`.

**Result: PASS (observed via the lane, not CI)** — running the lane against the pinned EHRbase image
(`ehrbase/ehrbase:2.15.0`) has both `source` and `augmented` validate under the real `Blutdruck.opt`
(`POST …/ehr/{id}/composition` → 201 for both). This is reproducible on demand via
`make -C validations/openehr-bloodpressure`; it is not re-run by `make check`/CI, so re-run the lane
to confirm rather than trusting this line. On that evidence, "perfectly replace openEHR" holds for
`openEHR-EHR-OBSERVATION.blood_pressure.v2`.

The probe also corrected one real openEHR RM-invariant defect the by-hand `links` analysis missed:
a COMPOSITION `LINK.target` is typed `DV_EHR_URI`, whose `Scheme_valid` invariant requires the `ehr`
URI scheme — so the complement-pointer `LINK` must use `ehr://…`, not a bare `urn:`. The bulk
complement carrier (`feeder_audit.original_content`, a `DV_PARSABLE`) is RM-level and OPT-transparent
and was never the obstacle; only the redundant coreference `LINK` needed the valid scheme.

## Attribution (Apache-2.0, Genkidata)

`blood_pressure.source.json` (and the structure of `blood_pressure.augmented.json`) derive from
the Genkidata project — OpenEHR sample data, Berlin Institute of Health — licensed Apache-2.0.
The underlying `Blutdruck` template and the `openEHR-EHR-OBSERVATION.blood_pressure.v2` /
`...COMPOSITION.registereintrag.v1` archetypes are from the GECCO / NUM dataset (Peter L. Reichertz
Institut für Medizinische Informatik). No warranty; values are synthetic (systolic magnitude `1.0`
is a test value, not clinical data).
