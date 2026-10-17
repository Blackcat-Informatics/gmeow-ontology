# Class source-support lookup

Issue 1792: remove per-restriction rescans while preserving exact class results.
Principles 4, 7, 12, 17 and 18 apply. Source work is isolated from the running
1757 full-gate candidate. No host probes, resource wrappers, compiler/profile
changes, external reasoner duplication, corpus production in tests, or changes
to admission/unknown/unsupported verdict policy are authorized by this repair.

## Evidence and prior art

The complete issue and zero comments were read from the current brief. Prior-art
coverage finds no earlier implementation branch or citing commit, and one linked
PR (1793) addressing input-specific certification. The recurrence search finds
one repeated-unshared-reasoning-analysis trailer. Neither that search nor the
single extracted requirement line replaces the complete body contract.

Inspection finds NativeFamilyLedger::source_leaves builds a complete proof-ID
map per call. Class preparation calls it independently for each source support;
class-result validation repeats it for each obstruction; branch proof validation
repeats it for each nested frame. This establishes avoidable repeated indexing,
not the fraction of runtime it consumes. Existing NativeEvidenceIndex owns real
proof insertion and already verifies cached proof positions against ledger IDs.

## Task 1: Measure the actual class operation

Add an explicit Make benchmark lane and a synthetic qualified-restriction series
at 1250/2500/5000 descriptors with one instance-bearing restriction, exercising
the public native class diagnostic from prepared ingress through retained-proof
validation. Reject source refusal or vacuous execution in the harness. Run the
baseline before editing the hot path, then the identical lane afterward with
the same lock and profile. Retain exact baseline/candidate results. Benchmark
wall time is report-only, never a flaky correctness gate.

## Task 2: Share exact proof lookup ownership

Remove repeated full-ledger indexes from batch validation and append-only source
support admission. Preserve the public standalone leaf-query behavior and the
serialized ledger shape. Prefer an immutable per-batch proof view for validation
and the existing native evidence owner's checked proof positions for growing
admission. Each cached selected ID must resolve to that actual ledger entry;
missing references, changed scope, mismatched slots and rollback stay failures.
Do not add an unchecked serialized cache or a second theorem authority.

Acceptance: complete asserted/derived leaves and shared-DAG deduplication match
the uncached walk; missing references, changed ledger entries, world/contract
mixing and growth are covered. Existing class conflict, exhaustive-case proof,
result-invariant and native evidence tests retain exact outputs. Any additional
restriction scan found by the measurements remains in scope for repair.

## Task 3: Validate, publish and integrate

Use focused synthetic checks and normal lint/hooks. Compare benchmark series and
trace actual production callers. Update the issue with implementation and exact
validation results, commit/push normally and open the scoped PR. Refresh reviews
and required qualification before ghprsq; reuse qualified evidence where valid.
The separate running combined full gate does not qualify this later change.
No independent reviewer API is available, so no independent review is claimed.
CodeRabbit review will be requested after a concrete published implementation.

Full production reasoning and all-issue completion are separate acceptance from
this local performance repair; neither follows from a synthetic timing change.
