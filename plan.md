# Preserve failed producer parity evidence

Hosted generation for ONNX metadata PR1903 failed exact graph isomorphism against slices/grounding/math/tests/fixtures/lifted-onnx.ttl. The job retained the executable and build receipt, but no candidate graph. Current error drops the canonical graph already computed at the production parity boundary.

This unit changes only the failure diagnostic: retain producer label, named graph and fixture path, expected/actual UTF-8 byte and quad counts, the complete framed actual canonical N-Quads, and deterministic textual expected-only/actual-only canonical lines. Blank-node relabeling makes textual differences potentially noisy; these are explicitly not minimal semantic differences.

The exact equality comparison and terminal StageFailed remain. Failure bytes never become successful products, cache receipts or fixture selectors. No fixture edit, weakened gate, second producer or local corpus construction. Principles 4 and 7 govern preserved source ownership and reviewable evidence.

Qualification: two independently authored tiny string controls for exact Unicode byte counts, complete payload and empty payload; producer-independent workspace Clippy, source hygiene, formatting and normal hooks. Hosted production remains required to retrieve the actual ONNX output before any fixture repair. This unit does not discharge issue1902 parity acceptance or native completion blocker1897/1803.
