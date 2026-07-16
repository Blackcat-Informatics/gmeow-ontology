<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Grounded use case — the data axis: openEHR blood pressure ⇄ GMEOW (⇄ FHIR)

> **Grounds:** `take1.md` §§4–6, 9, 11–13 against *real* data — the GECCO `Blutdruck` template,
> from the [Genkidata](https://github.com/Berlin-Institute-of-Health/Genkidata) corpus:
> `compositions/blood_pressure.json` (RM instance) and `opts/Blutdruck.opt` (the Operational
> Template). Companion: `usecase_openehr_taskplan_rchops21.md` (the process axis).
>
> **Claim under test:** GMEOW can *functionally replace* openEHR for this archetype —
> down-projection yields `⟨ valid openEHR file ⊕ gmeow complement ⟩` that (a) validates under
> `Blutdruck.opt` and (b) back-transforms losslessly (`u ∘ d = id_GMEOW`). This document does the
> hand-worked round trip, runs the **in-band complement slot test**, and reports the result and
> its honest caveats.

---

## 1. The real data

`blood_pressure.json` is canonical-JSON openEHR. Its load-bearing facts (verbatim):

- **Two-level identity.** `COMPOSITION` (`openEHR-EHR-COMPOSITION.registereintrag.v1`,
  `template_id "Blutdruck"`) → `content[OBSERVATION]` (`openEHR-EHR-OBSERVATION.blood_pressure.v2`).
- **The claim spine.** `composer` (PARTY_IDENTIFIED, a FHIR Practitioner URL), `context.start_time`
  `2012-09-17T00:00:00+02:00`, `language de`, `territory DE`.
- **A real FHIR provenance trail.** `feeder_audit.originating_system_item_ids[0]` =
  `{ id: "Observation/816ddebd-ef90-4d6a-9c97-cba47eafb292/_history/1", type: "fhir_logical_id" }`,
  `originating_system_audit.system_id = "FHIR-Bridge"`. *This composition is itself a
  down-projection from FHIR* — which is why this case is a triangle (§7).
- **The HISTORY of dated values.** `data` (HISTORY) → `events[POINT_EVENT]` (`time` =
  `2012-09-17…`) → `data` (ITEM_TREE) → two `ELEMENT`s:
  - `at0004` "Systolisch" → `DV_QUANTITY { units: "mm[Hg]", magnitude: 1.0 }`
  - `at0005` "Diastolisch" → `DV_QUANTITY { units: "mm[Hg]", magnitude: 60.0 }`
  (`magnitude 1.0` is a synthetic test value; treat values as structural, not clinical.)
- **The witness.** Every node carries an `archetype_node_id` (`at0001`, `at0003`, `at0004`,
  `at0005`, `at0006`). *The leaf `DV_QUANTITY` means "systolic BP" only via this path.*

`Blutdruck.opt` (the AM layer) constrains it with 108 `occurrences`/`existence`/`cardinality`
nodes. The systolic `C_DV_QUANTITY` (lines 465–501) declares:

- `property = openehr::pressure` (terminology `openehr`),
- a `magnitude` interval with `lower_included = true`, **`upper_included = false`** (a half-open
  `[lo, hi)`), `lower_unbounded = false`,
- `units = mm[Hg]`.

---

## 2. The GMEOW canonical form (YAMATO-refined)

The canonical object is **not** a flat `{systolic: 1.0}`. It is the YAMATO quality ladder
(`take1.md` §13.3) over the attributed-claim spine (P9/P14), authored in `logic:` (P17). Sketch:

```turtle
# --- The persistent quality (YAMATO: ONE enduring quality; values change, identity persists) ---
:patientP a gmeow:Person .
:sysBP-of-P a gmeow:Quality ;                 # the patient's systolic blood pressure, enduring (gmeow:Quality ⊑ logic:Quality)
    gmeow:bearer           :patientP ;        # the by-reference inherence prop (Principle 5), not raw gufo:inheresIn
    logic:genericQuality   gmeow:pressure ;   # YAMATO generic quality  (== OPT property openehr::pressure)
    logic:qualityRole      gmeow:systolicRole . # YAMATO quality-role in the arterial-BP context (== at0004)

# --- One dated observation = one result of that persistent quality (P9 unified observation) ---
:obs1 a gmeow:Observation ;
    gmeow:observationOf    :sysBP-of-P ;       # attaches to the enduring quality, not free-floating
    gmeow:observedAt       "2012-09-17T00:00:00+02:00"^^xsd:dateTime ;
    gmeow:observationResult :meas1 ;
    gmeow:committer        :practitionerX ;     # == composer
    gmeow:accordingTo      :clinicStandpointDE .

# --- The measured dimension: pressure = M·L⁻¹·T⁻², grounded structurally in math: ---
:pressureDimension a math:DerivedDimension ;
    math:baseDimensionExponent
        [ a math:DimensionExponent ; math:exponentOfDimension math:massDimension ; math:exponentNumerator 1 ; math:exponentDenominator 1 ] ,
        [ a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ; math:exponentNumerator -1 ; math:exponentDenominator 1 ] ,
        [ a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ; math:exponentNumerator -2 ; math:exponentDenominator 1 ] .

# --- The measurement: YAMATO unit-independent true quantity vs the framed measured value (P11) ---
:meas1 a math:Quantity ;                       # the framed measured value (the observation's result-wrapper)
    gmeow:trueQuantity     [ a gmeow:Magnitude ; gmeow:dimension gmeow:pressure ] ;  # frame-independent magnitude (⊑ logic:trueQuantity)
    math:hasDimension     :pressureDimension ; # M·L⁻¹·T⁻², carried explicitly in the math grounding
    math:quantityValue    1.0 ;
    gmeow:unit             qudt:MilliM_HG ;     # the unit belongs to the measured value, by reference (QUDT)
    gmeow:hasReferenceFrame :clinicFrameDE .    # the frame; a value without its frame is ill-formed (P11)

# --- Coreference by reference (P5), NOT owl:sameAs ---
:obs1 gmeow:authorityLink <fhir:Observation/816ddebd-…/_history/1> ;
      skos:exactMatch     <fhir:…> .           # the FHIR logical id, carried not collapsed
```

Diastolic is the symmetric second quality/role (`at0005`, role `gmeow:diastolicRole`). Note four
things the canonical form holds that the openEHR leaf does **not**: the persistent-quality
identity (`:sysBP-of-P`), the generic↔role ladder, the four quantitative axes + determinacy on
the observation, and the standpoint index (`:clinicStandpointDE`). These are `S ∖ im(get)` (§4).

---

## 3. The `get` leg (down-projection `d`: GMEOW → openEHR file)

`get` is mnemomorphic: it emits the openEHR tree *and the path-witness is the structure itself*.
The lowering is mechanical and exact for the RM slice:

| GMEOW | → openEHR (`blood_pressure.json` node) |
|---|---|
| `:obs1 gmeow:observedAt` | `data.events[0].time` + `context.start_time` |
| `:sysBP-of-P` (role `systolicRole`) | `ELEMENT @archetype_node_id at0004`, `name "Systolisch"` |
| `:meas1 {value 1.0, unit mmHg}` | `DV_QUANTITY { magnitude: 1.0, units: "mm[Hg]" }` |
| `gmeow:pressure` (generic quality) | (implicit; asserted by the archetype `property`) |
| `:practitionerX` | `composer` (PARTY_IDENTIFIED) |
| `gmeow:authorityLink <fhir:…>` | `feeder_audit.originating_system_item_ids[0]` |
| the HISTORY of `:obs*` of `:sysBP-of-P` | `data.events[]` (one POINT_EVENT per observation) |

The persistent quality `:sysBP-of-P` projects to the **HISTORY container** (`data`), and each
dated `:obs` to one `POINT_EVENT` — YAMATO's "one quality, many dated results" *is* openEHR's
HISTORY structure. The `archetype_node_id` path *is* the witness `get` retains.

---

## 4. `S ∖ im(get)` — what the openEHR slice cannot hold

Everything in the canonical form that has no native RM home:

1. the **persistent-quality identity** `:sysBP-of-P` (RM has the HISTORY but no first-class
   enduring-quality node the events are *of*);
2. the **generic-quality↔role ladder** as data (the archetype *implies* `property=pressure` but
   the RM instance does not carry `logic:genericQuality`/`logic:qualityRole` reifications);
3. the **four axes + determinacy** on the observation (`confidence`, `evidenceStrength`, `weight`,
   `probability`, `Determinacy`) — RM has no slot;
4. the **standpoint index** `:clinicStandpointDE` and any **multi-vantage** competing claims;
5. the **RDF-1.2 reifier identities** that make every triple a citable statement.

These are exactly the complement `gmeow_additions` (`take1.md` §13.2).

---

## 5. The in-band complement slot test (the crux)

**Question (`take1.md` §13.4-Q1, §17):** is there a carrier in a `Blutdruck.opt`-valid
composition for every item in §4, that the validator ignores? Enumerating the real slots:

| Candidate carrier | RM type | Free-form? | OPT-constrained? | Verdict |
|---|---|---|---|---|
| `archetype_details` | ARCHETYPED | no (fixed `archetype_id`/`template_id`/`rm_version`) | — | ✗ cannot inject |
| ELEMENT in the data tree | ITEM_TREE | no | **yes** — only `at0004`/`at0005` admitted | ✗ violates cardinality |
| `context.other_context` | ITEM_STRUCTURE | archetyped | yes (context archetype) | ✗ risky / slot-dependent |
| `links` (every LOCATABLE) | LINK[] | semi (typed URI targets) | no (RM-level) | ✓ for reifier/coref URIs — but `LINK.target` is a `DV_EHR_URI`, whose `Scheme_valid` RM invariant forces the `ehr` scheme (a coref pointer must be `ehr://…`, never a bare `urn:`) |
| **`feeder_audit.original_content`** | **DV_ENCAPSULATED → DV_PARSABLE** | **yes (arbitrary string + formalism)** | **no (RM-level)** | ✓✓ **bulk carrier** |
| `feeder_audit…other_details` | ITEM_STRUCTURE | archetyped-but-open | no (RM-level) | ✓ for structured bits |

**Result: the complement carrier exists. The slot test PASSES for this archetype.** The bulk
complement rides as `feeder_audit.original_content` typed `DV_PARSABLE { formalism:
"text/turtle", value: "<the gmeow_additions as RDF-1.2 Turtle, keyed by archetype_node_id path>" }`;
reifier/coreference identities ride as `links`; structured key bits in `…other_details`. All
three are **RM-level** metadata that `Blutdruck.opt` does not constrain (OPTs constrain archetyped
*content*, not the audit envelope), so the openEHR slice still validates — clause (a) holds.

**Honest caveats (these go to the breakout, not under the rug):**

- **Semantic propriety.** `feeder_audit` means "lineage from a *feeder* system." GMEOW is the
  *canonical source*, not a feeder — using it as the complement carrier is a mild semantic
  stretch. Mechanically valid; ontologically borderline. The clean alternatives are a dedicated
  RM extension or the **content-hash-bound sidecar** (`take1.md` §13.2). Recommendation: support
  *both* — `feeder_audit`/`links` for single-file self-containment, sidecar for purity — and let
  the consumer pick.
- **Empirical validation — PASS observed via the standalone lane (not CI-reproduced).** "RM permits
  `DV_PARSABLE` here" was a spec reading; the standalone lane `validations/openehr-bloodpressure/`
  checks it against real reference validators (EHRbase CDR, pinned image `ehrbase/ehrbase:2.15.0`;
  and the Archie RM validator). Running the lane uploads `Blutdruck.opt` and POSTs both
  compositions; the recorded outcome is that `source` and `augmented` each return 201 — both
  validate. This is reproducible **on demand** via `make -C validations/openehr-bloodpressure`; it
  is deliberately **outside `make check`** (it needs Docker/Java) so the green is not asserted by CI
  — re-run the lane to confirm. One real RM invariant the by-hand `links` reading missed surfaced
  and is now fixed: `LINK.target` is a `DV_EHR_URI`, so its value must use the `ehr` scheme
  (`Scheme_valid`) — the augmented coref `LINK` uses `ehr://…`. The bulk carrier
  (`feeder_audit.original_content`) was never the obstacle.
- **The composition's existing `feeder_audit` already carries the FHIR trail.** The complement is
  *additive* (extend `other_details` / set `original_content`); it must not clobber the FHIR
  lineage. Non-destructive by construction.

**Realized as a concrete artifact.** `fixtures/blood_pressure.augmented.json` is the actual `d(g)`:
the unmodified RM slice (`fixtures/blood_pressure.source.json`, vendored from Genkidata, Apache-2.0)
plus the GMEOW complement (`fixtures/blood_pressure.complement.ttl`) carried in
`feeder_audit.original_content` (DV_PARSABLE `text/turtle`) + a COMPOSITION `LINK` (its
`DV_EHR_URI` target on the `ehr` scheme) — generated non-destructively (the systolic `DV_QUANTITY`
and the FHIR lineage are byte-preserved). The empirical step (run it through EHRbase/Archie to
confirm clause (a)) is the standalone lane `validations/openehr-bloodpressure/` — it needs a running
CDR / Java, outside GMEOW's Docker-free gate, so it is a `make -C validations/openehr-bloodpressure`
probe, not part of `make check`. **Recorded outcome: both compositions validate (PASS) against the
pinned EHRbase image — reproduce on demand via the lane; it is not re-run by CI.** The in-gate
bounded recovery + full fixture round trip is proven by the conformance case
`correspondence/openehr-bloodpressure-section-retraction` (the complete three-edge source path is
executed through get and candidate put and every source atom is recovered) and
`crates/logic-compile/tests/openehr_bloodpressure_roundtrip.rs` (the larger data proof: `u` re-lifts
the RM slice via the `rmPath` witness, unions with the complement, and the reconstruction equals the
golden source `blood_pressure.source.ttl`).

---

## 6. The `put` leg (up-projection `u`) and the `u ∘ d = id_GMEOW` check

`u` reads the openEHR slice **and** the complement:

- the RM slice gives the framed measurement, the dated event, the composer, the path-witness;
- the complement gives back `:sysBP-of-P` identity, the generic↔role ladder, the four axes +
  determinacy, the standpoint, and the reifier identities.

Because `get` was mnemomorphic and the complement carries exactly `S ∖ im(get)`, **nothing the
retraction needs was discarded** → `u(d(g)) = g` on the canonical IR. Two complementary checks
back this, and it is worth being precise about what each proves. The **bounded query-class**
Round-trip / Mnemomorphism gates (`take1.md` §15.3–§15.4, conformance case
`correspondence/openehr-bloodpressure-section-retraction`) execute the complete three-edge source
path through get and candidate put, proving that all declared path atoms recover; the SeqPath's
structural inverse alone is not accepted as evidence. The **full fixture data** proof
(`crates/logic-compile/tests/openehr_bloodpressure_roundtrip.rs`) re-lifts the RM `DV_QUANTITY`
values through the `rmPath` witness (`at0004`/`at0005`), unions them with the parsed complement, and
asserts the canonicalization equals the golden source `blood_pressure.source.ttl` — so corrupting
the RM magnitude fails the test. Together they realize the **section/retraction rung**:
`:sysBP-of-P` round-trips via the persistent-quality identity carried in the complement; the framed
value round-trips via the RM `DV_QUANTITY`; the standpoint/axes round-trip via the Turtle blob keyed
by `at0004`.

Without the complement (RM slice alone), `u` would be a **candidate preimage only** (`take1.md`
§6.1): it could not recover the standpoint, the axes, or the persistent-quality identity — it
would *reconstruct* a plausible GMEOW graph, not *recover* the original. The complement is what
moves this cell from lossy-lens to section/retraction.

---

## 7. The FHIR triangle and the colimit merge (`take1.md` §8.2)

The `feeder_audit` shows the same blood-pressure fact also lives as a **FHIR `Observation`**. So
there are two external views of one fact:

```text
        FHIR Observation/816ddebd…          openEHR Blutdruck (blood_pressure.json)
                       \                            /
                        \   put (ingest)           / put (ingest)
                         ▼                         ▼
                       GMEOW  :sysBP-of-P + :obs1  (one persistent quality, one observation)
```

Combining them is a **colimit/pushout along the GMEOW apex**, not a sequential composition. The
two ingests must *glue* on the shared fact **without `owl:sameAs` collapse** (P5/P9): the FHIR
logical-id and the openEHR archetype-path both attach as `gmeow:authorityLink`/`skos:exactMatch`
to the *same* `:obs1`, and if they disagreed (e.g. different magnitudes) the merge would record
**two contested standpoint-indexed claims**, not silently pick one. This is the concrete instance
that motivates promoting merge/colimit to a first-class axis.

---

## 8. ADL → FOL: the half-open interval, exactly

`Blutdruck.opt`'s systolic `C_DV_QUANTITY` lowers to a `logic:` validation-shape:

```text
shape SystolicMeasurement:
    on  ?m where ?m gmeow:observationOf ?q  ∧  ?q logic:qualityRole gmeow:systolicRole
    require  ?m gmeow:unit qudt:MilliM_HG
    require  lo ≤ ?m.value  ∧  ?m.value < hi          # [lo, hi):  lower_included, NOT upper_included
```

The **half-open boundary** (`upper_included = false`) is a first-class flag in the lowering, not
an off-by-one approximation. This is exercised for real: the native reader
`crates/shacl/src/openehr_opt.rs` reads the systolic/diastolic `C_DV_QUANTITY` `magnitude` block
straight out of the vendored `Blutdruck.opt` (`[0, 1000)` mm[Hg], `lower_included=true`,
`upper_included=false`) and lowers it — `lower_included → sh:minInclusive`,
`upper_included=false → sh:maxExclusive` (never `sh:maxInclusive`). The test
`crates/shacl/tests/bloodpressure_halfopen.rs` drives that reader end-to-end and asserts the
produced shape rejects `value == 1000`; flipping the OPT's `upper_included` to `true` fails the
test. This is the sharp ADL-fidelity check (`take1.md` §13.4-Q2) on boundary inclusivity for the
blood-pressure magnitude — the narrow slice of a general OPT→SHACL constraint lowering.
(Scope note: only the magnitude-interval slice is lowered here; the general ADL2/OPT constraint
lowering across the CKM is a separate roadmap capability.) `logic:` full-FOL strictly exceeds the
ADL constraint here (it can also state cross-field constraints ADL cannot, e.g. systolic >
diastolic) — the "augment" beyond subsumption.

---

## 9. Law / gate / loss-ledger summary

| `take1.md` law / gate | This case |
|---|---|
| Validation (§13.1.1) | **PASS observed via the standalone lane, not CI** — `source` and `augmented` both validate under `Blutdruck.opt` against pinned EHRbase `2.15.0` (`validations/openehr-bloodpressure/`, reproduce on demand); complement in RM-level `feeder_audit`/`links`, the `LINK.target` `DV_EHR_URI` on the `ehr` scheme |
| Lossless subsumption `u∘d=id` (§13.1.2) | holds — the data test reconstructs `S` from the RM slice re-lifted via the `rmPath` witness ∪ the complement and asserts it equals the golden `blood_pressure.source.ttl`; load-bearing (corrupting an RM magnitude fails) — **section/retraction rung** (`crates/logic-compile/tests/openehr_bloodpressure_roundtrip.rs`) |
| Store-replacement `d∘u≅o` (§13.1.3) | holds for faithful instances — RM slice regenerated incl. the half-open interval read from the OPT (`crates/shacl/src/openehr_opt.rs`, `crates/shacl/tests/bloodpressure_halfopen.rs`) |
| Round-trip gate (§15.3) | passes by native execution over the declared complete three-edge source case (`correspondence/openehr-bloodpressure-section-retraction`); full RM-slice + complement recovery is proven by the reconstruction test above |
| Mnemomorphism gate (§15.4) | passes — witness = `archetype_node_id` path + complement |
| Loss ledger (§15.6) | **exact** for the RM slice + complement; **under-approximation** for any consumer that drops the complement (RM-only reader) — that reader gets a valid-but-lessened view, declared |

**Boundary finding:** for `openEHR-EHR-OBSERVATION.blood_pressure.v2`, *no field in `S ∖ im(get)`
forces a validation-vs-losslessness tradeoff* — the complement fits in RM-level transparent
slots. "Perfectly replace openEHR" **holds for this archetype** — now confirmed empirically (both
compositions validate in EHRbase), modulo the one remaining honest caveat in §5 (semantic propriety
of `feeder_audit` as the canonical-source carrier). That is exactly the falsifiable, nameable result
`take1.md` §17 asks for — here, a *positive* one. The probe also caught and fixed one real RM
invariant the by-hand reading missed (`LINK.target` must be an `ehr`-scheme `DV_EHR_URI`).
