# Fallible math and GMN continuation

This turn made source and compiler-evidence progress. The full goal stays active.
There is no external blocker: remaining compiler failures have actionable source.

Math Sink and RunFrame now return shared diagnostic Results. Actual R, ONNX and
proof emitter call graphs propagate native writes immediately, including multiline
sink chains. Sealing returns native storage, codec and UTF-8 errors instead of
panicking. Original mathematical emission choices, invariant assertions and
semantic test assertions remain. Existing pure Option helpers retain their Option
semantics; only storage-capable methods gained Result boundaries. Successful tiny
test construction/rendering is explicitly unwrapped. No new resource controls,
host probes, thread/profile changes, fallback emitters or dependencies were added
to math-lift.

GMN model hydration, canonicalization, canonical comparison, content digests,
RoundTrip preparation and storage-capable witness entry points are fallible. A
storage refusal cannot become false canonical equality or a semantic codec error.
The codec's existing serializable Gmn1Error algebra is unchanged. The shared
codec_verdict boundary retains actual Gmn1Error values in authored observations
and propagates every other diagnostic unchanged. Two synthetic controls require
exact semantic refusal preservation and exact typed operational-cause retention.
These controls compiled but have NOT executed.

GMN metrics now propagate hydration/serialization errors rather than quietly
excluding them from measurements. The explicit invalid-Turtle input domain and
semantic write/read rejection accounting remain. Equality and both serializers
reuse the same flattened GMN-0 source dataset; the reconstructed native dataset
is dropped before serialization. This removes one source hydration per measured
round trip. No elapsed-time improvement is claimed without measurement.

Projection registry targets return the shared envelope. Pure IngestDiagnostic
values retain their original typed cause through into_diag. SilentIngestDrop has
the existing grounded class metadata. NonUtf8Surface has no canonical individual
in the current slices, so its input diagnosis is retained without fabricating an
ontology class. All six other registered target implementations adapt their pure
semantic errors explicitly; pipeline target context no longer erases the cause
into a stage-error string. Existing refusal controls still inspect the exact
IngestDiagnostic fields via downcast.

The CLI digest and decode hydration/canonicalization paths report their shared
diagnostics directly. The older Turtle serializer/UTF-8 failure arms still format
native errors into strings and need the same diagnostic-fidelity repair before
full completion. Installed CLI compilation/acceptance remains unrun.

The cost-matrix leaf retains a full native diagnostic in MatrixError::Native,
exposes its original error source, and propagates canonical/serialization failure.
It adds only the existing first-party gmeow-errors crate dependency. No external
package was added. Cargo updated the root lockfile's internal dependency edge.

## Evidence

- clippy-v10: terminal FAIL exit 2, first math migration inventory.
- clippy-v11/v12/v13: terminal FAIL; an earlier commentary claiming math strict
  success was premature. Full inventories still contained an incorrect pure R
  Option return, followed by unchecked multiline sink calls and redundant
  Ok-question-mark forwarding. These were repaired, not suppressed.
- clippy-v14: terminal FAIL exit 2. Math had 13 needless-question-mark findings
  and 54 unchecked write Results. GMN integration fixtures and cost-matrix native
  callers also needed migration. All were subsequently repaired.
- raw/purrdf-main-fallible-clippy-v15.log: terminal FAIL exit 2 overall. It
  explicitly checks gmeow-math-lift, gmeow-lang-bridge and gmeow-gmn-cost-matrix
  with no diagnostics in those crates, including their all-target inventories.
  Remaining reported failures are affect-ingest (two library errors) and logic
  (47 library errors, 207 errors in its lib-test target; overlapping counts).
- raw/purrdf-main-fallible-lint-v10.log: terminal PASS exit 0 on settled source.
  v9 returned a files-modified failure during the dependency-lock refresh, while
  gitleaks itself reported no leaks. The sequential rerun passed every hook.
- Formatting v26 and git diff --check passed. No unit/runtime tests, corpus
  producer, full gate, browser rebuild, commit, push, message posting or merge.

## Next

Read the complete clippy-v15 inventory. Repair affect-ingest's sink and the actual
native logic APIs/callers for fallible builder, DatasetHandle, exact integers and
new upstream enum cases. Preserve typed native causes; never stringify them into
semantic overclaim, missing proof, inequality, coverage loss or unknown results.
Keep existing bounded list-feedback regression requirements unchanged. Qualify
the authenticated full workspace inventory after compilation succeeds, then the
explicit producer, corpus consumers and all original issue/PR acceptance work.
The Git pin and all current WIP remain uncommitted and unqualified for merge.
