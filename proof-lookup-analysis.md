# Source-support lookup finding

Owner: issue 1792; related production blocker 1757. Principles 4, 7, 12 and 18.

Observed source path: PreparedClassAnalysis::prepare iterates source_premises,
list_fields, list_premises and admission_issues. Each calls native_support, then
NativeFamilyLedger::source_leaves. That last method constructs a BTreeMap over
every ledger proof for each individual roots request. Since native_support also
appends the current premise proofs, a linear sequence of independent premises
constructs a succession of growing whole-ledger indexes. This is concrete
superlinear work even before class-model search; it does not establish the
fraction of total production time consumed by this path.

Other production callers must be covered: ClassExecutionOutcome::validate invokes
source_leaves for each obstruction, and RefutationProof::validate invokes it for
each proof node's source support. The native evidence owner already keeps an
exact row-to-(proof identity, ledger position) cache and validates a cached slot
against the actual ledger entry. A repair must reuse that ownership and preserve
the standalone public source_leaves behavior; adding an unvalidated persistent
cache to the serialized public ledger would create a second source of truth.

Candidate direction: one exact proof lookup for a batch of read-only validation,
and an incrementally maintained lookup at the existing native evidence owner for
append-only premise admission. Check every selected identity against the actual
ledger position; reject missing references and preserve original RDF leaves,
scope, native derivation identities and intrinsic/modal/contextual leaf policy.
Do not change theorem admission or add a competing generic DL engine.

Required controls: exact asserted and derived leaf equality against the current
uncached walk; shared-DAG deduplication; missing/forward or mismatched proof
references; ledger growth and replacement; world/contract isolation; existing
class conflict/support and result-invariant tests. Measure synthetic restriction
series and proof-index work through production entry points, with the same
compiler/profile. No host-resource probes or caps. A speed claim requires actual
measurements; inspection alone is not a benchmark.

Published head b797a315e generation (b), job 116617978346, reports:
stage-reason 4646.115s; stage-validate 520.360s; stage-verify-attestation 432.233s;
stage-conformance 118.257s; stage-compile-logic 11.491s. The run fails later at
stage-docs-render with a nonconclusive valid/unsupported reasoning verdict.
These are recorded application stage timings, not host load observations. They
identify a production target but do not attribute all reasoning cost to this
lookup. Current repaired candidate is still in its full qualification retry.
