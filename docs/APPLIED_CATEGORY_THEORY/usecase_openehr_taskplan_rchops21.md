<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Grounded use case — the process axis: openEHR PROC (RCHOPS21) ⇄ `logic:Plan`

> **Grounds:** `take1.md` §13.5 and the canonical process model against the
> openEHR **PROC 1.6.0** process examples — specifically the **RCHOPS21** chemotherapy Task Plan
> (`process_examples.html`). Companion: `usecase_openehr_bloodpressure.md` (the data axis).
>
> **Grounding note.** Unlike the data axis (real local files in `Genkidata`), the PROC examples
> live in the published spec, not the corpus — so this case is grounded against the *spec example*
> (TP-VML diagram + DLM rules as summarized in PROC 1.6.0), not local source files.
>
> **Claim under test:** openEHR Task Planning is *one more by-reference projection target of
> `logic:Plan`* (`take1.md` §13.5), and RCHOPS21 exercises exactly the constructs a pure DAG
> **cannot** express — proving why the full `logic:Plan` superset is needed and the acyclic DAG profile is only a restriction of it.

The [normative logic design](../../slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md)
governs the execution and law claims. This case describes an authored plan, its
certified native bounded projection, and witnessed planned-skeleton recovery. Its vocabulary representation and
by-reference mappings do not establish a complete TP-VML/DLM exporter, a lawful
stateful update, or a certified composition. Those claims require executable legs
over admitted source/view/complement theories and their own evidence domains.

---

## 1. The real example (PROC 1.6.0, RCHOPS21)

RCHOPS21 is a 21-day chemotherapy cycle. From the spec, its load-bearing process features:

- **Precondition gate.** `preconditions: has_lymphoma_diagnosis` — the plan does not start unless
  a clinical predicate holds.
- **Sequential phases.** pre-medication **then** drug administration (serial ordering).
- **Conditional dose modification.** a decision/branch point modifies the dose based on a tracked
  measurement.
- **Iteration over cycles.** the plan **loops** over multiple 21-day cycles.
- **A Decision Logic Module (DLM)** supplies the conditions, with **tracked state** carrying
  *currency* (staleness) and *time windows*, verbatim:
  - `has_lymphoma_diagnosis: Boolean`
  - `neutrophils: Real currency = 3d`   (a measurement valid only if ≤ 3 days old)
  - `has_metastases: Boolean currency = 30 days time_window = tw_current_episode`
- **Terminology definitions** binding DLM variables to coded value sets.
- **Callback-driven completion** and **manual-notification** tasks (events the engine learns of
  only by being told).

Critically, the spec is explicit about the **prescriptive↔descriptive** split: "the Plan
definition … is what the TP engine executes … what the TP engine knows about the real world is
limited to what it is told," and "many actions and events can occur outside the TP computing
environment that will remain unknown to the system unless it is informed (e.g. by Manual
notification)." The executed record is ordinary openEHR **Compositions/Instructions/Actions**.

---

## 2. The GMEOW canonical form — `logic:Plan`

RCHOPS21 maps onto the `logic:Plan` canonical process model almost 1:1 (`take1.md` §13.5):

```text
logic:Plan  :rchops21
  precondition:  has_lymphoma_diagnosis                  # action-schema precondition (DLM-derived)
  body (serial ⊗, looped over cycles):
     loop over ?cycle in cycles:                          # ← LOOP: not expressible in a pure DAG
        ⊗  premedicate                                    # ActionSchema: pre-med
        ⊗  guard (neutrophils@currency≤3d ≥ threshold):   # ← GUARDED BRANCH on real condition
              then  administer(full_dose)
              else  administer(reduced_dose)              # conditional dose modification
        ⊗  observe(response)                              # observation closes the loop / cycle gate
```

- **Tasks → `logic:ActionSchema`** (`premedicate`, `administer`) with
  precondition / effect (`ins`/`del`, supersession-not-erasure) / invariant / **resource**
  (drug, infusion chair, nurse — serialized by `competes-for-resource`, captured by the build-pipeline executor's typed `DataFlow`/resource layer) / capability /
  **observation** / **compensation** (e.g. extravasation handling).
- **DLM → `logic:` derivation rules + an observation-conditioned policy.** The DLM is openEHR's
  rule dialect; in the calculus it is a *projection target* of `logic:` rules (like SWRL/N3),
  not a source.
- **DLM `currency`/`time_window` → the typed context algebra.** `neutrophils currency = 3d`
  becomes a **valid-time freshness guard**: the guard reads `neutrophils` only within a 3-day
  valid-time window; outside it, the value is `undetermined` (not stale-but-used). This is the
  context algebra doing exactly what the DLM currency annotation specifies.
- **Terminology `definitions` → nested terminology correspondences** (`take1.md` §13.4-Q3): each
  value-set binding is a correspondence *inside* the plan correspondence.

---

## 3. The `get` leg and the DAG-profile loss

Down-projecting `:rchops21` to openEHR TP-VML + DLM is a `logic:Plan` lowering. But note the
two-tier projection the DAG profile makes explicit:

- **To the full TP-VML/DLM surface:** loops, guards, callbacks, manual-notification all have
  TP-VML constructs → a relatively faithful projection (under-approximation only where TP-VML
  lacks `logic:`'s concurrency-serializability or per-outcome compensation expressivity).
- **To the acyclic DAG profile (e.g. for a build/CWL/Airflow consumer):** the **loop over cycles** is
  *not acyclic* → the DAG profile reports `unsupported` **with the offending edge named** (the
  cycle back-edge); a consumer that must have a DAG either gets the loop **unrolled** to a
  fixed cycle count (with the unroll recorded in the loss ledger) or an honest `unsupported`. The
  plan is still valid canonically — never silently truncated.

This is the whole point of the canonical process model: RCHOPS21 exhibits **iteration, guarded
branching on real conditions, and per-branch compensation** — exactly the constructs a pure DAG
cannot express. It is the worked proof that DAG must be a *profile of* the superset, not the core.

---

## 4. YAMATO event refinements — the formal vocabulary for the seam

The three YAMATO process refinements (`take1.md` §13.5, adopted canonically in `logic:`,
by-reference bridge to YAMATO terms) each earn their keep here:

1. **action(open, on-going) vs event(closed, unitary)** — *arrive* vs *arrival*. The prescriptive
   `administer` **ActionSchema** is an *open* action (a type of on-going activity); the executed
   openEHR **Action** ("the drug was administered at 14:32") is a *closed* unitary event. "The
   administration is happening" vs "the administration occurred" is now a typed distinction, which
   plain `gmeow:Event` could not make. This is the exact pole-naming the prescriptive↔descriptive
   seam needed.
2. **causal parts vs temporal parts (causal ⊆ temporal).** `premedicate` **causally enables**
   `administer` (not merely *precedes* it); a 3-day-stale neutrophil count **causally gates** the
   dose branch. These are *causal* edges, distinct from the *temporal* `gmeow:hasSubEvent`
   nesting of cycles. The build-pipeline executor's typed `DataFlow`/resource capture is where this
   causal axis lands in the executor.
3. **process ≠ event (change-asymmetry).** The plan prescribes over *processes* (dissective,
   revisable — the protocol can be amended mid-treatment by suppression, never mutation); the
   executed record is *events* (unitary, immutable — an administration that occurred cannot
   un-occur). This is why the plan is held over an interval and revised by supersession (in the
   canonical `logic:Plan` process model), while the descriptive Actions are append-only.

---

## 5. The prescriptive↔descriptive lens — a *lossy* lens, not a mnemomorphism

The relationship between the plan and its execution record is a correspondence — but a
**lossy correspondence**, *not* a section/retraction, and **not mnemomorphic in
general**. The lossy-lens classification describes the intended relationship;
it does not establish lawful update operations. A prism and an affine
correspondence are separate rungs, each requiring its own focus and operations:

```text
   :rchops21 (prescriptive logic:Plan)
        │  realize  (get: plan → executed openEHR Instruction/Activity/Action via the ISM)
        ▼
   executed record (descriptive: Compositions/Actions)
        │  recover? (put: record → plan)
        ▼
   ??? — NOT recoverable in general
```

- **Why not mnemomorphic.** The spec is explicit: events occur *outside* the engine and are known
  only if reported. So the descriptive record is a **reality-perturbed realization** of the plan;
  the plan is *not* a function of the record. `put` (record → plan) is therefore a **candidate
  preimage** (`take1.md` §6.1), not a lawful recovery — multiple plans (or none) are consistent
  with a given record.
- **When it *is* recoverable.** If each executed Action carries the **witness** — a back-reference
  to the plan/ActionSchema it instantiates (openEHR's Instruction-State-Machine linkage:
  Instruction → Activity → Action with `instruction_details`) — recovery can resolve
  the planned skeleton if the referenced plan and schemas remain available with
  authenticated identities. A bare link does not reconstruct a deleted or changed
  plan. The admitted complement must carry or resolve the complete required witness;
  missing or corrupt selected witnesses fail admission. A profile may explicitly
  limit recovery to the planned portion and record off-plan information as loss.
- **Two distinct correspondences, never conflated** (the canonical process model's "path vs. intention vs. causation:
  connected, never identified"): (i) plan ⟷ external workflow surface (BPMN/Airflow/TP-VML — the
  §13.5 by-reference targets), and (ii) plan ⟷ its own execution record (this lossy lens). The
  calculus keeps them apart.

### Native executable surface

The installed CLI projects the caller's canonical plan and rechecks its source-bound certificate:

```console
gmeow correspondence project-plan \
  docs/APPLIED_CATEGORY_THEORY/fixtures/rchops21.plan.ttl \
  --plan urn:gmeow:plan:rchops21 --format json
```

The selected fixed count is an explicit bounded-unrolling contract. For this source it produces
six cycles and 30 prescriptive positions while retaining the condition/effect fact closures,
tracked-state freshness, both guarded alternatives, nondeterministic outcomes, compensation, and
the high-IPI conditional additions. The positions are never represented as observed events.

Witnessed recovery requires the still-available canonical plan source and the descriptive record:

```console
gmeow correspondence recover-plan \
  docs/APPLIED_CATEGORY_THEORY/fixtures/rchops21.plan.ttl \
  docs/APPLIED_CATEGORY_THEORY/fixtures/rchops21.observed.ttl \
  --plan urn:gmeow:plan:rchops21 --format json
```

Every selected occurrence must carry exactly one matching `logic:instantiatesPlan` and exactly one
`logic:instantiatesSchema`; absence or ambiguity fails admission. The returned receipt binds the
source projection certificate and the canonical observed-input digest. It recovers the planned
skeleton only. It does not infer order, timing, outcomes, unreported events, or execution of the
other guarded branch. The optimized producer emits the same two results as
`generated/logic/rchops21-plan-projection.json` and
`generated/logic/rchops21-plan-recovery.json`.

---

## 6. Law / gate / loss-ledger summary

| `take1.md` law / gate | This case |
|---|---|
| `logic:Plan` projection to TP-VML/DLM (§13.5) | under-approximation where TP-VML lacks concurrency-serializability / per-outcome compensation |
| DAG profile | **`unsupported`** for the cycle loop, offending back-edge named; or unrolled with loss recorded |
| Mnemomorphism gate (§15.4) | prescriptive↔descriptive correspondence is **not** mnemomorphic in general; planned-skeleton recovery requires an authenticated, resolvable plan/schema witness |
| Composition / merge (§8) | the DLM terminology bindings are **nested** correspondences (§13.4-Q3) |
| Loss ledger (§15.6) | an acyclic profile refuses a loop unless an explicit bounded-unrolling profile is selected; any serialization, compensation omission or off-plan loss must be selected and disclosed, never an automatic fallback |

---

## 7. Boundary findings and caveats

- **Positive:** the *prescriptive* plan (RCHOPS21 as authored) subsumes cleanly into `logic:Plan`
  — loops, guards, DLM currency, preconditions, terminology all have canonical homes. openEHR
  Task Planning is confirmed as a by-reference projection target of `logic:Plan` (§13.5).
- **Boundary:** the *descriptive* round-trip (recover the plan from the execution record) is
  **inherently lossy** — and openEHR's own spec says so. This is not a GMEOW limitation; it is a
  property of the world (events happen off-engine). The honest move is to *name* it: the
  plan→record correspondence sits on the lossy-lens rung, recoverable only to the extent the ISM
  witness is present. Do **not** claim section/retraction for the process execution lens.
- **Authored grounding.** A machine-readable RCHOPS21 exists as `fixtures/rchops21.plan.ttl` —
  GMEOW's own `logic:Plan` rendering, derived from the PROC 1.6.0 source
  (`openEHR/specifications-PROC : docs/process_examples/master05-chemo.adoc`, DLM "RCHOPS21"), with
  the loop / patient-fit guard / nondeterministic-outcome+compensation / high-IPI
  conditional-addition / tracked-state currencies all expressed canonically. The openEHR DLM/TP-VML
  is **not** vendored (specifications-PROC license is "Other"/NOASSERTION) — only cited. The
  distinctive DLM constructs are now realized natively: `currency`/time-window staleness is
  `logic:FreshnessGuard` (an out-of-window datum gates the action `logic:GateUndetermined`);
  manual-notification / callback completion is `logic:NotificationWaitSchema` (an un-signalled wait
  is pending, carrying a `logic:awaitingSignal` witness); and the Instruction-State-Machine
  plan→execution linkage is the `logic:instantiatesSchema` + `logic:instantiatesPlan` in-band
  witness. The lowering `logic:Plan → openEHR Task Planning` is wired as a by-reference projection in
  `slices/core/work-orchestration/mappings/` with its preservation judgment in the loss ledger.
- **The two axes have separate evidence domains** (`take1.md` §13 table). The
  data axis supplies bounded section/retraction evidence for the committed
  fixture and query case. The process axis supplies a certified bounded projection
  and witnessed planned-skeleton recovery for the committed plan/record pair. It
  does not reconstruct a plan from a witnessless record and does not establish a
  complete external TP-VML/DLM export. Stateful update and composition laws retain
  their own evidence domains. Bounded agreement cannot certify unrestricted fusion
  rewrites.
