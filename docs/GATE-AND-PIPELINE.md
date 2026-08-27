<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Gate and the Pipeline — Doctrine

> **Genre.** A doctrine document, not a style guide. Every principle below is
> stated as a rule, and every rule is followed by the **real defect in this
> repository** that produced it, cited by file, Make target and commit. A rule
> with no case does not belong here; a case with no rule is just history.
>
> **Audience.** Anyone who adds or changes a `make check` task, a `make heavy`
> lane, a pipeline stage, a nextest budget, a ratchet baseline, or a claim in
> help text about what a gate enforces.
>
> **Scope.** How the producer and the gate relate: who runs the pipeline, who
> grades its output, what a gate is allowed to assume, and when a gate is
> allowed to read a record instead of recomputing it.
>
> **Relation to [`docs/PIPELINE_SPINE.md`](./PIPELINE_SPINE.md).** That document
> is canonical for what happens *inside* a run — the in-memory carrier, the
> single `gmeow.gts` terminal, the superset law, and the post-pipeline fanout.
> This document is canonical for what happens *around* a run — how it is
> invoked, how it is scheduled, and how its outputs are graded. Neither
> restates the other. Where a rule here depends on a spine rule, it links to
> the section rather than paraphrasing it.

## 1. How to run the gate

| You want | Run | Notes |
| --- | --- | --- |
| To verify your work | `make check` | THE entry point. Its DAG runs the producer in update mode as its first node, then gates. One hold of the host-global lock. |
| Artifacts, no gate | `make check-sync SYNC_MODE=update` | Rarely what you want. Scope with `SYNC_OUTPUTS={generated,docs,all}`. |
| Read-only drift verification | `make check-sync` | `SYNC_MODE=check` is the default, so a bare invocation can never mutate the tree. |
| The external docs fanout | `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` | Adds the materialize-then-`build` ordering that scope alone needs. |
| A clean-clone bootstrap | `make install` | Builds only the producer, materializes, then builds the consumer CLIs. |
| To see the schedule | `cargo xtask check --explain` | A dry run. It prints the wave plan and returns **without** taking the host lock. |
| The breadth lanes | `make heavy` | CI-only. It refuses unless `CI=true` **and** a CI-vendor marker is set. Run one locally by name (`make wasm-parity`). |
| To regenerate | — | `make regen` refuses, unconditionally. See P1. |

`make check` materializes and then gates in the same run. There is never a
separate regeneration step to perform first; performing one runs the whole
pipeline twice and queues the machine behind you.

## 2. The principles

### P1 — One producer, one run

**The rule.** Exactly one invocable target runs the regeneration pipeline:
`make check-sync`. Every other lane that needs artifacts — `install`, `build`,
`commit`, `release`, `full-release`, the Pages job, CI's cold materialization,
and the `sync` node of `CHECK_DAG` — drives *that* target. A second entry point
for the same work is not a convenience; it is a duplicate run.

**The case.** `make regen` was that second entry point (commit `a76168cc6`). It
ran the same pipeline `make check` runs, and because `gmeow-dev sync` takes the
same host-global gate lock as `cargo xtask check`, the documented
`make regen && make check` habit did not regenerate twice in parallel — the
second invocation *blocked on the first*, serialising a developer against
themselves and queueing every other worktree on the machine behind both. Three
code blocks in `AGENTS.md` literally documented the double run. A third
producer call site was found and closed in the same pass: `perf-gate` shelled
to the sync CLI directly, and now routes through `make check-sync` with
`SYNC_TIMINGS_JSON`.

**The rule as implemented.** `make regen` is *poisoned*, not deprecated: the
recipe prints the replacements and exits 1, with no environment marker, no
`MAKELEVEL` escape and no override variable. An entry point that can be
re-entered by setting a variable is the same habit under a new spelling. The
replacement deliberately has no short attractive name — `check-sync` was
already the DAG's task target and CI's step name, and its help says you almost
never want it directly.

**The boundary of the rule.** `make slice-quality-gate` is deliberately *not*
poisoned, and the reason is recorded above the target in the `Makefile` so
nobody "fixes" it later. It runs no pipeline stage and takes no lock; it is a
pure enforcer of rubric binding, coat distinctiveness, the tier ratchet,
per-axis floors, residue ceilings and floor monotonicity, none of which exist
anywhere else. CI never runs `make check`, so its `slice-quality-gate` step is
the only place the ratchet runs on a pull request. Poisoning it would delete
enforcement, not duplication. **Poison a duplicate producer; never poison a
sole enforcer.**

### P2 — The host-global gate lock is a fairness queue, not an obstacle

**The rule.** At most one GMEOW gate runs on the host at a time, across every
user and every worktree. There is deliberately no override.

**Why there is no override.** A gate takes every CPU on the box by design, so
two concurrent gates are slower *in aggregate* than two serialised ones. Worse,
a CPU-starved gate goes red on **timing** rather than on **content**, which
teaches people to re-run reds instead of reading them. On a shared machine the
lock *is* the queue; an escape hatch is simply a way for one worktree to take
the host from everybody else.

**The case (the hatch).** An override env var, `GMEOW_TASK_HOST_LOCK`, existed
and was documented "for CI runners or tests that want a scoped lock instead of
the machine-wide one". CI does not run on the development box, and a CI runner
is single-tenant — it acquires this lock uncontended and needs no hatch. The
override's only live effect was letting one worktree take the host from thirty
colleagues. It was removed in commit `6bb22778d`.

**The case (the tmpfs).** The lock file lived at `/tmp/gmeow-task/…`. Both
`/tmp` and `/var/tmp` are world-writable and sticky, so both keep the lock as
ONE file for the whole host — but `/tmp` is a tmpfs on this platform, and a
tmpfs clear (or a `systemd-tmpfiles` age-out during a multi-hour run) deletes
the file out from under a **live holder**, after which a second gate creates a
fresh inode and both run. That is exactly the interference the lock exists to
prevent, and it was observed happening. It now lives at
`/var/tmp/gmeow-task/host-runner.lock`
(`crates/xtask/src/main.rs::host_lock_path`, kept byte-identical to
`crates/gmeow-dev-cli/src/dev_sync.rs::host_lock_path` so both agree) —
disk-backed, preserved across reboots, and outside every worktree, so neither
the checkout-reset daemon nor `git clean` can remove it.

**Reaping is narrow, and the narrowness is the point.** A leftover file whose
permissions deny `open(O_RDWR)` to everyone but its long-dead creator bricks the
gate host-wide, so `open_lock_file` unlinks a lock whose recorded owner pid is
readable **and** provably dead. A `WouldBlock` with a dead recorded pid is
**not** reaped: the kernel releases a dead process's `flock`, so that state
means a *live* process holds the lock and merely failed to rewrite its owner
record. Unlinking there would run two gates at once. An inode-identity check
after `try_lock` closes the swap window, with a bounded retry that refuses
rather than risk two concurrent gates.

**What is not a hatch.** `GMEOW_TASK_LOCK_ROOT` / `GMEOW_TASK_LOCK_TOKEN` are a
re-entrancy handshake: a `sync` invoked as a descendant of a gate that already
holds the lock, for the same canonical root and carrying the token its parent
set, skips re-locking. It cannot be used to start a second gate.
`cargo xtask check --explain`, `xtask list` and `xtask receipt create` take no
lock at all — a dry run that does no work has no business holding the machine's
one gate slot.

### P3 — The pipeline RECORDS; the gate GRADES

**The rule.** A pipeline stage produces and attaches a record. It never decides
whether the repository passes. Grading lives in the gate.

**The case.** `crates/pipeline/src/stages/validate.rs` runs whole-corpus SHACL
and returns `Ok(StageOutput { … })` regardless of what it found — the findings
become `generated/diagnostics/shacl.{json,sarif,html,nq}` and the carrier's
`graph/diagnostics`, and nothing in the stage turns a violation into a
failure. The grading happens in `gmeow-dev validate`
(`crates/gmeow-dev-cli/src/dev_validate.rs`) and `gmeow-dev slice-quality-gate`.

**Why the separation is load-bearing.** It is precisely what makes P4 — "read
the record instead of recomputing it" — legitimate rather than a producer
marking its own homework. If a stage graded, then a gate reading the stage's
record would be reading a verdict the producer chose. Because the stage only
records, the gate that reads it still applies every floor, ceiling and ratchet
itself. Keep the separation when adding stages: a stage that can fail the build
on *content* (as opposed to failing because it could not produce its output) has
taken the gate's job.

### P4 — Do not recompute what the producer already recorded — but prove freshness first

**The rule.** A whole-corpus computation runs once per gate. A gate that needs
its result reads the record. A gate that reads a record **must refuse a stale or
unstamped one**, and refusing must be a hard failure — never a skip, never a
silent pass.

**The case.** Three whole-corpus computations ran twice per gate (commit
`1055d73c6`):

- the slice-quality sweep scored all 84 slices at the DAG root and all 84 again
  in the gate;
- `DocsModel::discover` costs about 12 s and was built three to four times per
  gate;
- merged SHACL ran a second full pass over the same authored corpus.

Each removal needed a prerequisite, and each prerequisite was a real defect
rather than a mere gap.

**Prerequisite 1 — the record was lossy.** The axis IRI, which is the key the
per-axis floor gate is indexed by, was never recorded: it appeared 2720 times in
the artifact, every one of them inside prose. `gmeow:qualityDimension` cannot
stand in — sixteen axes map to twelve dimensions — so `gmeow:assessmentAxis` was
authored as a first-class predicate in
`slices/core/slice-quality-rubric/module.ttl`, with a structural cell asserting
it is not aliased to the dimension. Scores were written through `{:.6}` while
the gate compares against a floor at `f64::EPSILON` tolerance — a rounding error
nine orders of magnitude larger than the comparator's — so a score below a floor
by less than 5e-7 rounded up and turned FAIL into PASS. Demonstrated on live
data: `axisGmn1Coverage` at 0.9999995 against a floor of 1.0 now fails and
previously passed. `crates/slice-quality/src/report.rs` now separates
`fmt_score` (display prose only, still `{:.6}`) from `exact_score` (the shortest
round-tripping `f64` lexical). See P9.

**Prerequisite 2 — the cache key was incomplete.** A content-addressed
cross-process cache for the docs model already existed and only the test lane
used it, because its key missed two workspace path dependencies on the model
path — including `crates/ns`, which supplies the vocabulary deciding which
predicates `discover` reads. Rather than name the two, `cache_key` in
`crates/docs/src/fixture.rs` now folds `gmeow-docs`' full transitive path-dep
closure, and `live_manifest_closure_is_closed_and_reaches_the_model` re-derives
that closure from the live manifests and asserts it is transitively closed, so a
new path dependency reds instead of silently reopening the hole. A
`gmeow:cqQueryFile` resolving outside the hashed roots became an error rather
than an unhashed input. `doc-lint` went from 1m17s cold to 1.96s warm, output
byte-identical.

The cache has since been split along the crate boundary so that every model
consumer can reach it: the model envelope, the shared cache key, the shared
payload digest and the shared atomic writer live in
`crates/docs-model/src/fixture.rs`, and only the rendered-site / mdBook envelopes
remain in `crates/docs/src/fixture.rs`. The key is unchanged — its derived
implementation closure is still rooted at `crates/docs`, because the renderer's
bytes must move the key its site and book caches hang off, and `crates/docs`
depends on `crates/docs-model` so the model's own closure is a subset. The split
is what lets `gmeow-slice-quality`'s `DocMaturity` axis read the cache: an edge
from that crate to `gmeow-docs` would close a first-party cycle, because
`gmeow-docs` dev-depends on `gmeow-mcp` and `gmeow-mcp` depends on
`gmeow-slice-quality`, and the layering scan counts dev-dependencies.

**Prerequisite 3 — the record could not prove it was current.** The naive claim
that the second merged-SHACL pass was a bit-for-bit repeat was wrong twice over.
`make validate` has no `sync` prerequisite, so its record can be arbitrarily
old; and its Phase 8 reads `generated/shapes` off disk while the stage
structurally never does, which made Phase 8 the only check that catches
`generated/shapes` drift. So the drift detection was preserved rather than lost:
the stage stamps a `shaclInputDigest` over the authored sources **and** the
effective union members
(`crates/pipeline/src/stages/validate.rs::shacl_input_digest`), and the gate
recomputes that digest from disk — deliberately reading `generated/shapes` to do
it — and hard-fails on absence, mismatch, or a record it cannot parse. A missing
record is never a skip. The two shape unions are now identical by construction
rather than by accident: the raw-text concatenation is deleted and both sides
use `load_shapes`. `make validate` now runs in about 2 s.

**The freshness rule, generalised.** `make slice-quality-gate` has no `sync`
prerequisite either, so the quality corpus carries a `gmeow:versionFingerprint`
over the same scored-source authority the pipeline's own cache key uses, and
`crates/slice-quality/src/read.rs::verify_fresh` recomputes it and refuses a
stale or unstamped record. The same module hard-fails on a grade set that does
not match the rubric, a tier off the ladder, or a missing roll-up — completeness
as well as freshness, because a truncated record must never be mistaken for a
passing one.

### P5 — A dependency edge must name the read that forces it

**The rule.** A `make check` task depends on `sync` **if and only if** it reads
a `generated/` artifact, and every such edge carries a comment naming the exact
read. A task that reads only authored sources starts in wave 0, concurrently
with the producer. The doctrine is stated in the module documentation of
`crates/xtask/src/main.rs` and enforced by its `ROOT` / `AFTER_SYNC` constants
and their tests.

**The case (edges that bought nothing).** Every task depended on `sync` whether
or not it read `generated/` (commit `6bb22778d`). Three gate tasks were verified
to read authored sources only and now start in wave 0 alongside `sync` itself:
`check-lint` (pre-commit hygiene over the git-tracked tree, and `/generated/` is
gitignored in full, so no hook can see a generated artifact), `crate-check`
(scans `crates/`, `slices/`, `dsl/`) and `i18n-lint` (walks slices' `.po` files
and authored Turtle). A serial hop was removed in the same pass:
`coherence-gate-teeth` waited behind `reason-verify` for output it never
consumed. The honest result is stated rather than oversold — twelve of the
nineteen remaining tasks genuinely do read `generated/`, so this buys wave-0
concurrency without taking `sync` off the critical path.

**The case (a justification that was not true).** `slice-quality-gate`'s edge
was justified by `generated/governance/slice-quality-axis-floors.tsv`. That path
is only echoed as a human pointer inside a per-axis floor violation message and
is never read — the gate projects its floors from the ontology-resident rubric.
The real forcing read was found and recorded (commit `e6fcf7948`), and all ten
`AFTER_SYNC` justifications were re-checked against the code; the other nine were
accurate. **A justification that names a path appearing only in an error string
is a false claim about the build, and P11 applies to it.**

**Corollary.** Splitting a node that declares itself serial is part of the same
rule. The explicit fixture producer now sits between `rust-build` and nextest;
clippy and doctests remain independent siblings, while carrier/coherence proofs
run inside the one nextest inventory. `rust-gate` survives as an aggregate alias
because `AGENTS.md` references it, but the gate no longer runs it, so nothing
executes twice.

The fixture handoff is a selector, not a second traversal. The producer records
the exact action context and receipt for every test-consumed persistent leaf in
`.cache/gmeow-sync/test-fixture-manifest-v2.json`. The test runner pins that
file's SHA-256, and each loader opens the selected action directly. This matters
when a persistent leaf depends on a deliberately nonpersistent carrier stage:
trying to rediscover the leaf key from currently persistent dependency receipts
would either miss or pressure the runner to execute the DAG again. The recorded
context removes that work while preserving fail-closed receipt and blob checks.
The selector also records the producer-profile bundle-import receipt and every
bundle-derived corpus-artifact action. Consumers compiled under the test profile
load those selected producer actions; they do not derive a false miss from their
own profile identity, and they never repair a miss by importing the corpus.

### P6 — Local gate versus CI-only `heavy`

**The rule that decides it.** A task stays on `make check` if it fails **fast**
and **deterministically** on *this branch's own changes*. It moves to
`make heavy` if any of these hold:

1. it is a soak or repeat-for-confidence loop;
2. its runtime is set by corpus breadth rather than by the edit under test;
3. it SKIPs locally — critical-path occupancy with no local signal.

**The case.** Three lanes moved (commit `6bb22778d`). `bench-soak` is a
repeat-for-confidence window (criterion 1). `acceptance` globs every external
coverage fixture and round-trips each (criterion 2). `wasm-parity` builds five
crates for `wasm32` in release plus `wasm-bindgen`, `wasm-opt` and four Node
suites, and SKIPs outright when the target or `node` is absent — frequently a
no-op occupying the critical path on a developer's gate (criterion 3), while
hard-failing in CI so the parity criterion is never silently unverified on the
gating path.

`medium-consumer-surface` joined them later, and it is the sharpest illustration
of criterion 2 *within one axis*. The former `medium_identity_gate` emitted the
whole carrier under a second medium from inside a test; it was removed because a
test may never rebuild the corpus. Its useful contract is now split between the
read-only `medium_bundle` audit of the ONE producer-authenticated deliverable and
`medium_codec_composition`, which proves `decode(encode(x)) = x` over every
declared chain without executing the DAG. The two breadth-heavy consumer suites
— `medium_cli` and `medium_gate` — remain in the CI-only
`medium-consumer-surface` lane. The membership decision is therefore per
contract: artifact inspection stays cheap and read-only, codec laws use bounded
inputs, and no test launches another producer.

**The refusal.** `make heavy` requires `CI=true` **and** a CI-vendor marker
(`GITHUB_ACTIONS`, `GITLAB_CI`, or `BUILDKITE_BUILD_ID`), so a developer who
happens to have exported `CI` cannot trip it by accident. There is no override
flag: to run one of these deliberately, invoke it by name.

**Moving a task must not remove it from PRs, and that is verified.**
`the_heavy_lane_still_runs_on_every_pr` in
`crates/gmeow-dev-cli/tests/make_gate_contract.rs` asserts four things
mechanically: the `Makefile`'s `HEAVY_TASKS` is exactly the contract's set; the
`heavy` recipe hard-fails when `CI` is unset or false and actually runs every
member; `ci.yml` invokes `make heavy` and no longer runs any of those targets
directly; and the aggregate quality gate's `needs:` list includes the `heavy`
job. A scheduling decision that silently became a coverage cut would red this
test.

### P7 — A test that reds on machine load is a broken test, not a flaky one

**The rule.** A test whose verdict depends on how busy the machine is teaches
people to re-run reds instead of reading them — the very failure the host lock
(P2) exists to prevent. Whole-corpus proofs get the backstop their peers have.

**The case.** The full gate ran 18 of 19 tasks green and `nextest` failed on
exactly one test, by **timeout** rather than by content (commit `bab6aa35c`).
The OBI planned-process bridge proof (`gmeow-pipeline`, binary
`obi_planned_process_projection`) drives both producer legs — the typed
correspondence frontend and the SSSOM lowering — over the real corpus and
measures about 118 s uncontended. `.config/nextest.toml`'s default group is
`slow-timeout = 60s, terminate-after = 2`, i.e. a 120 s kill, so it passed at
118 s on a quiet box and timed out under load **in the same run** where sibling
whole-corpus proofs ran 300 to 585 s and passed — because those carry the 300 s
backstop and this one never got one. It now carries the same override. Verified
in isolation: 98.7 s, passes.

**The honest distinction.** This is a *misconfiguration* fix: the assertions are
untouched and only the time the test is allowed changes, and the test was
already proven to pass on content. Re-thresholding a budget to make a genuine
content failure stop reporting is not this, and is never permitted. Before
extending a budget, prove the test passes uncontended; if it does not, the
budget is not the problem.

### P8 — A gate that cannot fail is worse than none

**The rule.** From the caller's side, a gate that ran and found nothing is
indistinguishable from a gate that could not find anything. Prove teeth by
**breaking the input and watching it red**, through the production entry point —
not by calling the checking function directly.

**The case (an empty marker set).** `enactment_gate_markers` returned
`Ok(Vec::new())` from both arms, and `compiled_rules()` was
`OnceLock::get_or_init(Vec::new)` — the gate compiled zero laws (commit
`af50775f2`). The observed-not-derived guard, which enforces the enactment
kernel's hardest safety law (the engine may describe, validate and certify
external-effect records but never **derive** one), ran `reject_banned_heads`
over that provably-empty vector: had the shipped reasoner derived a
`logic:EffectAttempt` or a `logic:ExternalEffectReceipt`, `verify()` would have
returned a clean report (commit `903572138`). Five `#[cfg(test)]` unit tests
called the guard function directly and stayed green throughout, which is exactly
how a guard with no input ships.

**How the teeth were proved.** `verify()` now runs `reject_banned_heads` over
`derived_rows` — the derived, non-EDB edges of the reasoned closure
(`crates/logic/src/verify.rs`) — so an asserted effect record written down by a
dispatching organ still reaches the kernel as a legitimate observation.
`crates/logic/tests/enactment_observed_not_derived.rs` drives the production
`verify()` entry point and never `reject_banned_heads` directly; the banned head
is not injected into a private row-set, it is **derived** by the shipped EL
type-propagation rule from ordinary asserted RDFS, which is the shape a real
regression would take. Both red cases were confirmed against the pre-fix wiring.
Controls pin the guard's narrowness so it cannot pass by refusing everything —
the old prefix-based subject test condemned `logic:ActionableFrontier` and
`logic:FrontierEntry`, which the kernel exists to derive.

**Residue is load-bearing, not decorative.** The lowering into violation rules
is a total function into rules ⊕ flagged residue, and `reject_unenforced_laws`
hard-fails when a constraint that carries a `gmeow:enforcesFailureClass` fails to
lower — such a law is then enforced by nothing. The census is pinned by a test:
45 authored `logic:Constraint`s, 25 lower into 56 rules, 20 decline (every one
for `no-enforces-failure-class`), and all 23 enactment-kernel laws are in the 25.

**The case (an allowlist with no premise).** `example_sweep`'s `NON_CONFORMANT`
allowlist excluded 100% of an issue's worked examples — all eight — on the
stated premise that they are "conformant in the merged bundle (`make validate`
passes)". That premise was false: `make validate` does not validate examples at
all, because the pipeline's authored-file set is the root ontology plus
`slices/**/module.ttl` plus `imports/`, and `examples/` is absent. Those entries
asserted something no gate had ever checked. Each example is now validated
unioned with its module's TBox, and the allowlist fell from 117 to 35 entries —
81 of the 82 removed were already passing (commit `023fbee8a`). The union did
not disable the check it was accused of dodging, and that too was proven by
breaking the input: corrupting a value still raises `sh:class`, and removing a
required property still raises `sh:MinCount`.

### P9 — A record must round-trip losslessly before anything reads it

**The rule.** Before a gate is allowed to read a projected record instead of
recomputing, every field it will read must be recoverable **exactly**, and a
test must pin the round trip.

**The case.** See P4, prerequisite 1: scores were projected through `{:.6}` and
read back against an `f64::EPSILON` comparator, so a floor breach smaller than
5e-7 silently flipped FAIL to PASS. The lexical form is now the shortest
round-tripping `f64` rendering — plain decimal, never scientific notation, so it
is a legal `xsd:decimal`, with `0.0` rendering `0` and `1.0` rendering `1`
exactly. `crate::report::tests::recorded_grades_round_trip_exactly` pins it, and
the reader in `crates/slice-quality/src/read.rs` states losslessness,
completeness and freshness as three enforced properties rather than three
assumptions. Rounding for **prose** is still correct and still uses `{:.6}`;
what is forbidden is a rounded value on the machine-read path.

### P10 — Ratchets move DOWN only

**The rule.** A ratchet baseline records debt. Blessing a risen baseline is
raising the gate to match the debt, which converts a burn-down ledger into a
record of surrender. Pay the coverage; do not re-bless upward. An increase is
legitimate only when an explicit forensic correction removes previously credited
invalid evidence, and then it is inspected and recorded, never concealed.

**Where the ratchets live.** The documentation coverage ratchet is the insta
snapshot in
`crates/docs/tests/lint_regression.rs::coverage_ratchet_baseline_is_recorded`
(per-code `docs/missing-*` warning counts). The slice-quality ratchets are the
tier ratchet, the per-axis committed floors, the residue ceilings, and the
monotonicity checks diffed against the `origin/main` merge base
(`gate::tier_floor_monotonicity`, `gate::axis_floor_monotonicity`,
`gate::projection_ceiling_monotonicity`).

**The case.** The full gate ran 18 of 19 tasks green and `nextest` failed on
snapshots because **one** newly minted term — `gmeow:assessmentAxis`, introduced
by the de-duplication work in P4 — carried no documentation coat, so seven
coverage axes each wanted to rise by exactly one (commit `a48aa53ab`). The
baseline was not blessed upward. The coverage was paid, and every axis ended at
or below its committed value: four back to baseline and three *lower*, because
the new artifacts also reached neighbouring terms.

**Corollary.** Any minted term needs its full documentation coat — label,
definition, scope note, example, `howToUse`, translations, fixtures, competency
roster — or it raises the ratchet. Budget the coat with the term, not after it.

### P11 — Claims in help text, comments and rationales are gates too

**The rule.** A sentence that asserts a gate enforces something is a claim about
the build. If it is false, it is a defect of the same kind as a broken assertion,
because a reader stops looking. Fix the claim or fix the code; never leave the
claim.

**The case (a live enforcement claim naming a dead target).** Four gates in
`governance/constitution.ttl` asserted their enforcement via
`meta:makeTarget "regen"` — and `meta:makeTarget` is itself defined as a target
that "must exist in the Makefile, be reachable from a gate-aggregate lane". Those
claims would have kept passing `constitution-check` while pointing at a target
that now refuses. All four were migrated to `check-sync` and re-verified in
commit `a76168cc6`.

**The case (a rationale justified by deleted vocabulary).** About 37 grounding
correspondences carried alignment confidences whose justifications had been
preserved verbatim through an earlier re-key, so they argued from terms deleted
in commit `820383b89`: `logic:Plan` to `pplan:Plan` at 0.85 was defended by
"GMEOW Procedure is broader"; `logic:Enactment` to `pplan:Activity` by "GMEOW
Execution is the event that enacts a Procedure". A reader could not check any of
those judgments. Commit `311ca56fe` re-argued each from the live `logic:` term's
own definition, changing no predicate and no confidence, and recording residual
divergence as a structured `lossyDrop` rather than quietly downgrading a
predicate the soundness snapshot pins.

**The case (a conformance claim the harness could not falsify).** A conformance
cell asserted "exactly one finding, from this law" while the branch's own passing
test proved the same report carried two. The claim could not fail, because the
harness only checked that the expected finding was **present**.
`gmeow:expectedSoleFinding` (`crates/slicetest/src/dsl.rs`,
`crates/slicetest/src/exec.rs`) makes exhaustiveness checkable, per source shape
rather than per result row — several rows of one law are one law speaking; a
second shape is a second law. It bit during rollout and caught a new fixture
tripping a second slice's law.

**The second cut (a check that could not fail on most of its adopters).** The
first cut made the pinned `gmeow:expectedSourceShape` OPTIONAL, and let an
unpinned cell fall back to "no OTHER shape raised a finding carrying the expected
code". On a generic component that reading asserts almost nothing:
`shacl.MinCountConstraintComponent` and `shacl.SPARQLConstraintComponent` are each
raised by dozens of shapes, so every violating shape answered "yes, I am one of
them" and the intruder set came back empty. 151 of the 175 declaring cells were
unpinned, and the branch's own mutation test only exercised the PINNED path, so
nothing measured the fallback. Measuring it found 30 declaring cells that were
tripping two or three distinct laws and passing anyway. The pin is therefore
REQUIRED: soleness is a claim about WHICH law is the only one, an unnamed law
cannot carry it, and a declared claim whose strength silently depends on an absent
sibling property is the degradation the no-optionality rule forbids. An unpinned
`gmeow:expectedSoleFinding` is now a hard cell-configuration failure in `exec.rs`
and a violation of `shapes/test-dsl-shapes.ttl`, and
`an_unpinned_sole_finding_claim_is_a_hard_failure` reds against the old fallback.

**What the honest count is.** 238 cells across nine slices made the prose claim
"and NO other finding". Every violates cell in those slices now pins its law by
IRI (258 pins, up from 42). 145 cells declare and ENFORCE soleness — 92 in
`math:`, 36 in `logic:`, 10 in `lang:`, 7 in `semantic-topology`; the 30
declarations that were measured false are withdrawn. Of the 238 prose claims, 93
were measured false and that prose is deleted, replaced by an enumeration of what
the report actually carries. The causes are structural, not
sloppy fixtures, and worth naming because each is a separate piece of debt:
a partially migrated slice shipping a residual hand-authored `shapes.ttl`
alongside the derived projection of the same axioms (`concepts:`, `learning:`,
`diagnostics:`, `model-serving:`, much of `lang:`); one rule authored BOTH as an
EL-safe OWL restriction and as a `logic:Constraint` (19 `math:` cells); a class
shape and its superclass's shape both reporting one missing member; and a
cross-slice typing gap — an ordinary slice's conformance run loads only its own
module, so a `logic:preservationKind` value typed in the `logic:` module is
invisible and its qualified restriction reds (all 29 `core/preference` cells, on
two ABox individuals in `preference/module.ttl` that no fixture touches, and 4 in
`semantic-topology`). Each of those becomes sole-declarable when its cause is
retired; the rationales say so per cell.

**The open case, stated rather than hidden.** `make validate`'s help still reads
"Validate syntax, term annotations, SHACL, and DSL SHACL", but
`crates/gmeow-dev-cli/src/dev_validate.rs` never populates
`mapping_shapes_ttl` / `statement_shapes_ttl` / `test_dsl_shapes_ttl`, so phases
11-13 in `crates/validate/src/validate_all.rs` return `Ok(None)` and the DSL
SHACL pass does not run under that target. This is a filed, tracked gap and is
listed here as the live instance of the rule, not as a fixed one. Do not repeat
the help text's claim in new documentation.

### P12 — Prefer a generated lowering to a core rewrite

**The rule.** When a reasoner cannot see something, first ask whether one edge
can be **lowered** into the shape it already reads. A lowering keeps the
canonical form authoritative (Principle 17 — canon → views, exactly as SSSOM,
EDOAL and FnO do), leaves the codec, the fact-surface shape, the join keys and
provenance minting untouched, and states what it does not preserve. A core
rewrite does none of that and cannot be reviewed edge by edge.

**The case.** RDF 1.2 statement metadata was outside the reasoner: three shipped
examples carried attributions the rule set was never run over. The reasoner
already saw the reifier node and its ordinary annotations; the one thing it
could not see was the **object** of `rdf:reifies`, because that object is a
triple term and the fact surface carries IRIs, blank nodes and literals. Rather
than change `term_codec`, the EDB shape or the join keys, that single edge is
decomposed into three ordinary joinable edges —
`logic:reifiedStatementSubject` / `…Predicate` / `…Object` — in
`crates/logic/src/statement_lowering.rs`. The authored dataset is never mutated;
no triple term is ever encoded, because it is decomposed before the codec sees
one (commit `8aca67364`).

**It is not decorative.** A rule now derives that a recommendation is CONTESTED
when two co-equal vantages take opposing stances on the same statement — a rule
whose two halves shared **no variable** before the lowering, so it could not even
have been written — and it fires on the shipped example through the CLI.

**The boundary is narrow and true.** `logic:rdf12-nested-triple-term` names
exactly two residues: a statement whose own subject or object is itself a triple
term (nothing is emitted for it, and it is returned in
`StatementLowering::nested` so the residue is named rather than flattened into a
malformed IRI), and the statement's identity **as a term** — a rule may join on
the components and may not quantify over the statement itself. The boundary is
mirrored in `crate::reason::refute::retained_boundaries`, and the CLI warning
now raises only on that residue instead of on the whole surface. **A lowering
whose `logic:expressivenessBoundary` is broader than its actual residue is a
false claim (P11); one with no boundary record at all is worse.**

## 3. Checklist — adding a `make check` task

1. **Does it belong on `check` at all?** Apply P6. Soak loop, breadth-set
   runtime, or SKIPs locally → `make heavy`, plus a `make_gate_contract.rs`
   entry proving it still runs on every PR.
2. **Name the read.** If it depends on `sync`, put it in `AFTER_SYNC` with a
   comment naming the **exact** `generated/` path and the function that reads it.
   If it reads only authored sources, it is `ROOT` and starts in wave 0. Verify
   by reading the code, not the existing comment (P5).
3. **Does it recompute a whole-corpus result the pipeline already produced?** If
   so, read the record — but only after proving losslessness (P9), key
   completeness, and freshness with a stamp the gate recomputes and hard-fails
   on (P4). No stamp means no reading.
4. **Prove it can fail.** Break its input and watch it red, through the target a
   developer actually runs (P8). A green suite that only calls the checking
   function directly proves nothing.
5. **Budget it honestly.** If it is a whole-corpus proof, give it the same
   `.config/nextest.toml` backstop its peers have, and record the uncontended
   measurement in the override comment (P7).
6. **Do not add a second producer.** If it needs artifacts, depend on `sync`;
   never shell to `gmeow-dev sync` or add a new materializing target (P1).
7. **Check every claim you write.** Help text, the `AFTER_SYNC` comment, and any
   `meta:makeTarget` in `governance/constitution.ttl` must name targets that
   exist and behaviour that happens (P11).

## 4. Checklist — adding a pipeline stage

1. **Read [`docs/PIPELINE_SPINE.md`](./PIPELINE_SPINE.md) first.** The stage
   contract (§3), the single terminal (§4), the superset law (§5) and the fanout
   (§6) are canonical and are not restated here.
2. **Record; do not grade.** Attach your product to the carrier and return `Ok`.
   Failing the build on content is the gate's job (P3).
3. **Stamp what a gate will read.** If any gate will consume your record instead
   of recomputing, emit a freshness witness over the exact input authority — a
   digest over the files you consumed (`shaclInputDigest`) or a fingerprint over
   the scored source set (`gmeow:versionFingerprint`) — and make the projection
   lossless for every field a reader needs (P4, P9).
4. **Declare the edge in all three places.** A stage's dependency lives in
   `Stage::consumes()`, in `full_spec()`, and in `gmeow:dataflowConsumes` in
   `slices/core/pipeline/module.ttl`. Editing fewer than three leaves a mismatch
   that only surfaces at a full producer run.
5. **Prefer a lowering to a core change.** If the blocker is that a downstream
   engine cannot see one edge, lower that edge and record a narrow
   `logic:expressivenessBoundary` naming only what is genuinely not preserved
   (P12).
6. **Transform once.** The razor (spine §3.2) and P4 are the same rule seen from
   the two ends: compute a closure, projection or rendering a single time and let
   consumers read the attached result.

## 5. Grounding

| This doctrine | Canon |
| --- | --- |
| One producer; a second entry point is a duplicate run | Principle 4 (one canonical source) applied to the build's invocation |
| The pipeline records; the gate grades | [`docs/PIPELINE_SPINE.md`](./PIPELINE_SPINE.md) §3 (stage contract) |
| Transform once; read the record, do not recompute | [`docs/PIPELINE_SPINE.md`](./PIPELINE_SPINE.md) §3.2 (the razor) |
| A stale or unstamped record is a hard failure, never a skip | Low/no-optionality, hard-fail stance |
| A gate that cannot fail is worse than none | Principle 7 (verified by construction) |
| Ratchets move down only | The standing burn-down discipline; `CONSTITUTION.md` |
| A lowering, with an honest boundary, over a core rewrite | Principle 17 (canon → generated views) |
| False claims in help text and rationales are defects | Principle 1 (every claim checkable) |
