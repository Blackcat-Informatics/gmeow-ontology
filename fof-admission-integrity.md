# Live FOF admission integrity

The full logic issue/PR objective remains active and incomplete. The previous
service/controller turn made verified progress. No authorization for producer
environment/CPU fingerprinting has arrived; the existing producer authentication
is unchanged and has not been rerun. No commit, push, issue closure, external
message, PR creation or merge occurred in this continuation.

## Verified defect

Review of the actual complete-canon proof path found that SourceAdmission exposed
mutable/serializable status, rows, profile, selection and symbol tables, while its
matches check compared only source/program digests. A caller could delete refused
semantic units, set Complete and obtain a partial proof problem with the original
source identity. This was reproduced before repair: the new negative control
returned Complete where Blocked was required. The writer is the actual CLI path
called immediately after SourceAdmission::classical_fof in commands/prove.rs.

## Repair

The native compiler constructor now binds every serialized admission field with
a private live evidence commitment. The digest is streamed directly into the
existing BLAKE3 sink, without an admission-sized second buffer, RDF serialization,
reparse, second source compilation, new dependency or relaxed gate. The commitment
is not serialized or deserialized. Receipts remain inspection data and require
fresh selected-source admission before projection. Clones of unchanged live
decisions remain valid; mutable public inspection data cannot change an authorized
decision silently. Future serialized fields automatically participate in the
evidence commitment.

The projector validates unchanged live evidence and exact source/program identity
before interpreting any semantic rows. Invalid or reconstructed receipt data
publishes no sentences or proof problem. The source digest also now binds the
complete sorted named-graph catalogue, including empty context declarations, under
the new prepared-source-v3 identity tag. Statement, reifier, annotation and complete
document-receipt bindings remain. No unsupported semantic unit is removed, changed
to success or silently excluded by this repair.

## Verification

- raw/fof-admission-integrity-reproducer-v1.log: terminal FAIL before repair;
  1 selected test, actual Complete versus required Blocked. Nextest
  822529c9-f33b-4749-9147-65bfe228fae6, build 48.02s, run 0.090s.
- raw/fof-admission-integrity-fmt-v3.log: PASS.
- raw/fof-admission-integrity-clippy-v1.log: terminal PASS, 1m07s;
  producer-independent workspace/all targets with warnings denied. The four
  corpus-dependent consumer packages remain excluded by the existing target.
- raw/fof-admission-integrity-lint-v1.log: terminal PASS, repository lint including
  corpus-purity and formatting seals. git diff --check passed.
- raw/fof-admission-integrity-runtime-v1.log: terminal PASS, all 16 tptp::tests
  controls; nextest ac666c46-733e-4185-aa6e-fe4914a7457a, build 5m14s,
  run 0.181s. The additional frontend::admission_tests filter matched no cases;
  no formula-resource-admission module coverage is claimed from that filter.

New controls cover deletion of required-unit blockers; refusal of deserialized
receipt replay even with an injected private-field spelling; successful fresh
source re-admission and live cloning; profile/selection/symbol edits; changed
assertions retaining old unit IDs; and distinct empty context declarations with
the same default-graph assertion. All passed. Existing ordinary assertions,
schema laws, quotation refusal, literal identity, characteristic laws and exact
document/base selection remain in the passing FOF selection.

## Remaining full-goal requirements

The initial next action was to extend certified class-expression admission.
The verified integrity defect needed repair before allowing more semantic units.
Class-expression, standpoint/modal, contract-owner and bound-term guard semantics
remain required. No fresh full-canon blocker count was measured, and no zero-blocker
or prepared-canon completion claim follows from these controls.

Producer/consumer qualification still requires resolution of the explicit
fingerprinting authorization question described in native-service-cli-diagnostics.md.
Its Git package-ID repair is tested but has not been exercised by an authorized
producer retry. Complete prepared-canon production/embedding, typed request
composition, native/external/blinded proof acceptance, corpus/browser acceptance,
full local and hosted gates, independent final review and ghprsq merges remain
required. The source-level work can continue independently; this is progress,
not an external impasse or a reduction of the goal.
