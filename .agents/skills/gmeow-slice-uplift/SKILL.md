---
name: gmeow-slice-uplift
description: >-
  Runs the continuous, issue-less slice-quality uplift lane — read the repo-wide prioritization, raise the weakest slice's capping axis, land a slice-local PR, then ratchet the floor.
  Use when you are working the standing slice-quality uplift lane rather than a specific assigned issue.
---

# GMEOW Slice-Quality Uplift — the Continuous Lane

This skill governs the standing, issue-less lane that continuously lifts slice
quality one capping axis at a time. It is a behavioural contract: follow it in
order, do not skip steps, and cite the [CONSTITUTION.md](../../../CONSTITUTION.md)
principles that govern every decision. The advisor itself is documented by
[`gmeow-ontology-authoring`](../../gmeow-ontology-authoring/SKILL.md) (how to read
one slice's findings and discharge them); this skill is the *loop* around it — how
the lane chooses what to work, when it terminates, and how it stays out of the way
of issue lanes.

> **Meta-rule:** The measured advisor and the committed floor gates are the truth.
> If any step below conflicts with a Constitution principle, the principle wins —
> revise the step or amend the Constitution in the same pull request, never silently
> override. Never bump a floor ahead of a real, measured uplift.

---

## 1. The lattice-ascent model

A slice's primary object is its **per-axis profile vector** — one earned quality
tier per rubric axis, a point in `[0,1]^n` over the open axis value vocabulary
(each tier is a `logic:QualityValue` along one quality dimension; the axis set is an
open vocabulary, so this skill never names an axis *count*). The **roll-up tier is
only its lossy meet projection**: the UNWEIGHTED lattice meet, i.e. `min` over the
per-axis tier ranks (`crates/slice-quality/src/prioritize.rs`,
`crates/slice-quality/src/report.rs` — "the unweighted lattice meet of every
per-axis grade"). `axisWeight` orders ties only; it never moves a rank or a score,
because that would corrupt the unweighted-meet law.

The **meet witness** is the *min-rank antichain*: the set of axes tied at the
minimum rank. `prioritize`'s **capping axis** is the heaviest-leverage member of
that antichain (least rank, ties broken by highest weight, then axis IRI). Fixing
the capping axis is a NECESSARY target and the right place to START — but raising it
alone is NOT one-step-sufficient when several axes tie at the min: the roll-up meet
rises only once EVERY axis in that antichain clears its next threshold. This is why
the lane may legitimately re-pick the same slice with a new capping axis before its
tier moves — the antichain shrinks by one each pass until the last tied axis clears
and the meet finally lifts.

The ratchet floors are the **monotonicity CERTIFICATE** — and now a real hard-fail
gate, not a reviewer promise: `tier_floor_monotonicity` and
`axis_floor_monotonicity` in `crates/slice-quality/src/gate.rs` red on any *lowering*
of a committed floor line (or the deletion of a still-live floor). A committed floor
may only be raised.

Per-slice **termination is a fixpoint of the advisory operator**: the slice is done
when `advise(slice) = ∅` — the observable `advice=0` column in the prioritization
view. Until then the operator still has work to surface, and the lane keeps the
slice in play.

---

## 2. Consume the queue

Read the repo-wide prioritization — the ranked worklist:

```bash
make slice-quality            # = gmeow-dev slice-quality --all
```

`prioritize` (`dominates` → the Pareto sweep → `prioritize` → `render_text`) emits a
deterministic view. Its literal columns and legend are the contract you read:

```text
slice-quality: repo-wide prioritization (<N> slice(s))
  ordering: roll-up tier ascending, then dominated-before-frontier, then capping axis by weight — highest-leverage uplift first
  legend: [!] Pareto-dominated (a peer slice grades >= on every axis and > on one)  [=] on the Pareto frontier
  Pareto frontier: <F> of <N> slice(s) non-dominated
  columns: <mark> <slice>  roll-up=<tier>  cap=<axis>(<tier-rank>)  advice=<n>
  [!] <slice>  roll-up=<tier>  cap=<axis>(rank N)  advice=<n>
```

- `[!]` marks a **Pareto-dominated** slice (a peer grades `>=` on every axis and `>`
  on at least one — strictly behind); `[=]` marks a slice on the **Pareto frontier**.
- `cap=<axis>(rank N)` names the **capping axis** (the meet witness) and the rank it
  is stuck at; `advice=<n>` is how many ranked advisories the slice surfaced.
- Rows sort highest-leverage first: weakest `roll-up` ascending, then dominated
  before frontier, then capping axis by weight descending. **The top row — weakest
  roll-up, dominated `[!]`, its named `cap=<axis>` — is the highest-leverage item.**

The provenance of this ordering is `prioritize.rs`; treat its doc comment as the
authority for why the top row is the right next move.

---

## 3. Pick & apply

1. **Pick** the top slice×axis whose slice is NOT already owned by an active issue
   lane or an in-flight branch (see §6). Descend the list until you reach an
   unclaimed slice; that slice's `cap=<axis>` is your target.
2. **Focus** on the one slice:

   ```bash
   make slice-quality SLICE=<slice>     # e.g. make slice-quality SLICE=slices/core/tags
   ```

3. **Apply the capping axis's ranked advice.** The advisor now surfaces, for every
   deficient axis, its rubric `gmeow:axisAdviceTemplate` as an axis-level advice item
   ranked strictly AHEAD of that axis's per-term findings (`report.rs`,
   `ADVICE_KIND_TEMPLATE`). Read the axis-level recipe first — it tells you the
   general remediation — then discharge each per-site finding beneath it. Author the
   uplift in the slice's canonical sources (`module.ttl`, `examples/`, `tests/`,
   `mappings/…`), per [`gmeow-ontology-authoring`](../../gmeow-ontology-authoring/SKILL.md);
   never satisfy a projection finding with a new hand-authored shape (Principle 17).

---

## 4. Land

1. Run the full local gate — nothing narrower:

   ```bash
   make check
   ```

2. Ship **one small slice-local PR per slice** (or a tight batch of closely-related
   slices when they share a mechanical fix). Keep the diff inside the slice you
   uplifted; do not fan out edits across unrelated slices.
3. If any canonical source changed the bundle, regenerate under the
   land-one-at-a-time discipline: `generated/dist/gmeow.gts` is `merge=ours`, so
   `make regenerate` it in-PR and let bundle-touching PRs land **one at a time** to
   avoid a stale-bundle race (Principle 7).

---

## 5. Ratchet the floor

When a slice GENUINELY reaches a higher tier (measured, not hoped-for), pin the new
guarantee in the SAME PR:

- Add an **additive** line to
  [`governance/slice-quality-floors.tsv`](../../../governance/slice-quality-floors.tsv)
  (format `<slice-iri>\t<tier-local-name>`). An addition is always allowed and
  touches its own line only, so merge contention is near-zero.
- Also mind the per-axis ratchet file
  [`governance/slice-quality-axis-floors.tsv`](../../../governance/slice-quality-axis-floors.tsv)
  (format `<slice-iri>\t<axis-local-name>\t<floor:f64>`), e.g. when you raise a
  slice's measured `axisGmn1Coverage`.
- **Raise-only, never ahead of a real uplift.** This is now hard-fail-enforced by the
  floor-monotonicity gate, not a reviewer promise. The verdict names to keep straight
  (mirrored from `crates/slice-quality/src/gate.rs`):
  - `MeasuredBelowDeclared` — the slice no longer holds the tier its manifest declares;
  - `DeclaredBelowFloor` — the manifest declaration was lowered beneath the committed
    tier floor;
  - `MeasuredBelowFloor` — a per-axis measured score fell below its committed axis floor.

The floor lines are the certificate of §1's monotonicity: they encode "this ground is
held" so a later regression reds instead of silently eroding.

---

## 6. Contention rule

The lane is a background citizen. It **yields to issue lanes**:

- Never touch a slice an in-flight branch or an active issue lane owns — skip it in
  the prioritization and take the next unclaimed row.
- PRs stay **slice-local** (§4).
- `gmeow.gts` regeneration lands **one PR at a time**.

Two of these shards are machine-enforced, one is doctrine CI cannot see — know which:

- **Machine-enforced:** floor monotonicity — a lowered or still-live-deleted floor
  line reds `make slice-quality-gate` (and thus `make check`) via
  `tier_floor_monotonicity` / `axis_floor_monotonicity`.
- **Doctrine (CI cannot see it):** *don't touch a slice an in-flight branch owns* and
  *keep every PR slice-local*. Nothing in the gate knows which branch claimed which
  slice; these are discipline you uphold, not checks that catch you.

---

## 7. Sweep work is never an issue

The lane is **issue-less by design** — it is the standing operator, not a ticket.
When someone asks for a cross-cutting sweep ("rewrite all the X in every slice",
"make every slice do Y"), do NOT file an issue and do NOT open a mega-PR. Re-scope it
into the three durable artifacts the lane already runs on:

1. a **quality axis** that measures the deficiency objectively (so every slice's gap
   becomes a ranked `advice=` item, per §8);
2. **curation docs** that teach the remediation (extend `docs/SLICE_GUIDE.md` /
   `docs/SLICE_QA.md`, or the axis's `axisAdviceTemplate`);
3. **this skill**, if the loop's shape changes.

Then the sweep discharges itself, one slice-local PR at a time, through the ordinary
prioritization — never as a one-shot issue.

---

## 8. Axis-design constraint (normative)

Every quality axis is an **objective, intrinsic `[0,1]` measure** where `1.0` is
*definitional* (the property fully holds), scored by a deterministic Rust primitive —
never expert judgement, never calibrated toward a target:

- Use **bounded fractions** only. An **unbounded ratio** (e.g. a raw density or
  count) is BANNED from a tier ladder — a ladder rung must be a bounded `[0,1]`
  fraction so the meet lattice is well-defined.
- **Never tune a score to hit a target tier.** The number reports what is; the ladder
  reads the number. A slice reaches Maximal only by genuine uplift, never by
  re-weighting or re-calibrating the axis under it.
- Tiers rise **only by genuine uplift** — the floors in §5 exist precisely so a score
  cannot quietly slide back down after a ladder claims it.

This is the same objectivity the grounding-quality memory demands: scoring axes are
intrinsic `[0,1]` measures, not knobs.

---

## 9. Worked dry-run (verification)

A re-runnable sequence — NOT a frozen capture. Substitute a real `<SLICE>` path and
the `<AXIS>` the run names; the numbers will differ every time the repo moves. (The
`slice-quality-rubric` slice self-scores and is always present as a live
demonstrator, so this loop always has at least one row to read.)

```bash
# (1) Read the ranked worklist. The TOP row is the highest-leverage item.
make slice-quality
#   → find the first `[!]` row: weakest roll-up, `cap=<AXIS>(rank N)`, advice=<n>.
#     `[!]` = Pareto-dominated (strictly behind a peer); `[=]` = on the frontier.

# (2) Focus the chosen slice; read its per-axis grades weakest-first and the
#     ranked advice — the axis-level `axisAdviceTemplate` item prints AHEAD of that
#     axis's per-term findings.
make slice-quality SLICE=<SLICE>
#   → the capping axis <AXIS> is the weakest grade; its template line is the recipe,
#     the per-site findings beneath it are the concrete work.

# (3) Apply the advice in <SLICE>'s canonical sources, then gate:
make check
#   → green means the uplift holds and no floor regressed.

# (4) If the roll-up (or an axis floor) genuinely rose, add the additive floor line
#     (§5) and re-confirm the ratchet gate stays green:
make slice-quality-gate
#   → a lowered/deleted floor reds here as MeasuredBelowDeclared / DeclaredBelowFloor
#     / MeasuredBelowFloor; a clean raise-only change passes.

# (5) Re-read the worklist: <SLICE>'s `advice=` should have dropped. When it reads
#     advice=0 the advisory operator has reached its fixpoint and the slice is done.
make slice-quality
```

Read each output for the *movement*, not an absolute: does the capping axis's rank
climb, does `advice=<n>` fall toward `0`, does the slice cross from `[!]` to `[=]`?
Those deltas — not any single frozen number — are the evidence the loop is working.

---

## See Also

- [`gmeow-developer-start-issue`](../gmeow-developer-start-issue/SKILL.md) — the issue
  lane this background lane yields to (§6)
- [`gmeow-ontology-authoring`](../../gmeow-ontology-authoring/SKILL.md) — how to read
  one slice's advisor findings and discharge them (the per-slice work §3 invokes)
- [`CONSTITUTION.md`](../../../CONSTITUTION.md) — the normative principles (16: manifest
  is the sole tier truth; 17: logic-first projection; 7: no-drift)
- [`docs/SLICE_GUIDE.md`](../../../docs/SLICE_GUIDE.md) — §9 (author in the canon,
  project the surface), §10 (gates, drift, and landing), §13 (the condensed checklist)
- [`docs/SLICE_QA.md`](../../../docs/SLICE_QA.md) — moving every test bit into the slice
  structure (the testing axes read this)
