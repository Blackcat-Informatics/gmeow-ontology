# Preserve native source diagnostics through proof preparation

SourceCatalog converted a document-mapping Diag to LogicParseError(String), then
to a generic stage failure. That discarded the original kind, grade, focus and
evidence. Prepared source construction, frontend/dialect parsing, round-trip
drivers and the prover also depended on this string-only error API.

The wrapper is deleted throughout the Rust workspace, without an alias. The
compiler, prepared source, CLIF/CGIF/XCL readers and their real callers return
the shared Diag. Native failures propagate unchanged with operation/document
context. The callback regression preserves its original FormulaAdmission kind,
code, grade, focus and tags, and verifies that failure publishes no receipt.

Registered source-parse and source-read kinds retain the original live foreign
error, selected format and exact path. Arc retains the original cause while
satisfying the existing diagnostic-kind cloning contract. Empty source remains
a typed frontend refusal. File reads use the actual I/O result rather than a
separate existence check. The actual consumer prover uses its existing preserved-
diagnostic adapter, retaining outer reason codes, source identities and native
cause evidence. A consumer empty-selected-theory regression exercises this path.

Conformance, developer feedback/certification, transcode, slice reasoning,
pipeline examples, the round-trip drivers and the explicit fixture-producer
example move with this API. Obsolete identity/error adapters are deleted. The
explicit fixture producer was compiled, never invoked from a test. No runtime
dependency, fallback, profile weakening or successful program-identity change.

Validation: native-source-diag-synthetic-tests.log records 138/138 passing cases
in 0.108s. These cover source admission/FOF, all three Common Logic dialects,
conformance admission controls, source mapping and typed parser/I/O failures.
After removing the last four obsolete identity adapters, all three affected
pipeline round-trip cases passed again in 0.243s:
native-source-diag-final-roundtrip-tests.log. Strict producer-independent
workspace/all-target Clippy passed with warnings denied in 2m12:
native-source-diag-final-clippy.log. Complete helper hygiene passed:
native-source-diag-final-helpers.log. No ledger ceiling or gate was weakened.

The first expanded selection accidentally included the corpus-dependent
corpus_logic_module_common_logic_roundtrips case. It correctly FAILED CLOSED
without GMEOW_TEST_FIXTURE_MANIFEST; 138 other cases passed. Exact failure:
native-source-diag-integrated-tests.log. The corrected local diagnostic selection
excludes that authenticated consumer only; its test and CI requirement remain
unchanged. The corpus observation and new embedded consumer CLI regression are
not locally qualified. No test built corpus and no local optimized producer,
fingerprint or full gate ran.

Earlier compile attempts exposed the diagnostic macro's field/Clone contract,
old tuple-field consumers, the explicit fixture example and obsolete identity
adapters. Their failed logs are retained. These were corrected in source without
suppression, hook bypass or a compatibility wrapper.

Published as 1c3bc726a. Normal signed hooks and push passed:
native-source-diag-commit.log and native-source-diag-push.log. Hosted acceptance
is not implied by local checks.

Full hosted production/consumer acceptance, current-head review, canon semantics,
prepared native canon, blinded installed proof and production-world measurements
remain required. Neither 1762 nor 1770 is complete, and the full logic issue/PR
goal and plan.md remain in force. No pipeline speedup or final merge is claimed.
Principles 4, 7, 12 and 18.
