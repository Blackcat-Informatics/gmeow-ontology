# Issue #1902: Preserve ONNX tensor metadata identity and variable domains

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Parent: #1897; grounding owner: #1433. This is an independently deliverable ONNX metadata repair found during model-serving qualification. Full arbitrary-RDF tensor validation and model-serving acceptance remain in #1897; the concrete native datatype-cardinality completion failure remains owned by #1803.

The current lifter interns ExpressionType from a rendered label, so a concrete extent 3 and a symbolic extent named `3` can collapse. Its shape key also joins unescaped symbolic names, allowing distinct axis families to share an identity. Arbitrary input/initializer FreeVariableDeclaration nodes additionally lack their required mathematical Set domain. ExpressionType and TensorElementType are descriptors rather than Sets; naturalNumbers is appropriate for symbolic extents only.

Deliver the existing reviewed repair through the math/ONNX owners:

- Use framed structural element/rank/axis keys, preserving unknown rank, scalar rank zero, concrete/symbolic/unstated axes and delimiter/Unicode-bearing symbols.
- Add the optional math-owned expressionValueSpace relation from an ExpressionType to its separate Set of admitted values. Bind each leaf declaration to its input/header-authoritative value space; symbolic extents retain naturalNumbers.
- Preserve shared type/domain identity and legacy initializer identity. Retain compatible supplied annotations and reject incompatible element/rank/extent/equality metadata. Keep checks local to admitted metadata.
- Complete canonical annotations, French/Chinese catalogs, documentation and focused structural/runtime regressions. Use existing parsing, RDF and mathematical owners; tensor payloads and model execution are outside this repair.

Qualification: execute the real ONNX lift tests, including both declaration orders for identity collisions, input/initializer sharing, bool/string distinction, compatible refinement and refusal controls; run applicable formatting, strict lint and normal hooks. Record actual produced-owner/full acceptance separately under the current campaign exception. No test may construct the GMEOW corpus. Principles 4, 5, 7, 17 and 19 govern ownership and source identity.


## Comments (4)

### paudley — 2026-10-21T06:00:41Z

# ONNX tensor metadata repair

Issue1902 is a complete metadata identity/domain unit under1897/1433. Principles4,5,7,17,19 require one canonical math owner, lossless source identity and explicit refusal. The broader model/tensor contract and its failed native cardinality regression remain uncommitted in the preserved1897 checkout under1803; this unit does not change that test or native/compiler owners.

The existing ONNX production caller attaches tensor types from TypeProto and initializer headers. Replace rendered-label identity with framed element/known-rank/axis structural keys, and frame symbolic names inside shape keys. Preserve unknown rank, scalar rank zero, symbolic/concrete/unset arms and Unicode/delimiters. Keep display labels human-readable.

Add the canonical optional expressionValueSpace relation from ExpressionType to a separate typed Set. ONNX leaf FreeVariableDeclaration uses the first input/header Set domain; extent symbols use naturalNumbers. Retain compatible subsequent descriptors and reject supplied local element/rank/concrete/symbolic-equality conflicts, including legacy initializer-input annotations. No graph inference, tensor payload evaluation or operator-signature invention.

Reuse the two byte-identical Rust files already compiled and exercised by all59 ONNX lift tests in the1897 follow-up qualification. Seven source files transfer deliberately: those two Rust files, the expressionValueSpace module block and seven paired fields in each French/Chinese catalog, its structural assertion and bounded documentation. Generic rank/count/index/extent validation laws, model-serving profiles and failed native regression stay in1897. No donor source is deleted. Review all source and dependency deltas before reusing coverage; record any changed Rust coverage as invalidated.

Run strict producer-independent prebuild Clippy and ordinary signed hooks. Preserve the current campaign exception: no local corpus/producer/full gate; sequential ghprsq merge without waiting for CI. Generated math-owner/slice/installed/full acceptance remains explicitly unrun. Capture feedback and candidate integration, publish, merge only with ghprsq, verify exact source/archive and clean only this completed owned checkout. The parent and native owners remain open.


### paudley — 2026-10-21T08:18:18Z

Actual production acceptance remains blocked after PR #1903. The hosted generation run reached the real math-producers stage and correctly failed the ONNX producer-versus-fixture graph-isomorphism check for `slices/grounding/math/tests/fixtures/lifted-onnx.ttl`.

Evidence: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39553001747/job/119077372274. This is a semantic acceptance failure, not a runner-shutdown diagnosis. The run retained only the producer binary/build evidence, without the failed generated graph, so the current error is insufficient to review the exact new output. All59 earlier ONNX unit controls remain narrow evidence; they do not establish producer-fixture parity.

Next: repair failure diagnostics to retain the actual already-produced canonical graph, obtain that output through hosted production, and review every fixture difference against the intended metadata/domain/identity rules before updating the witness. Preserve the parity gate and native model-serving blocker; no local corpus/fixture production, blind expected-output blessing or requirement deletion. Issues #1902, #1897 and #1433 remain open. Principles 4 and 7.


### paudley — 2026-10-21T08:40:24Z

# Preserve failed producer parity evidence

Hosted generation for ONNX metadata PR1903 failed exact graph isomorphism against slices/grounding/math/tests/fixtures/lifted-onnx.ttl. The job retained the executable and build receipt, but no candidate graph. Current error drops the canonical graph already computed at the production parity boundary.

This unit changes only the failure diagnostic: retain producer label, named graph and fixture path, expected/actual UTF-8 byte and quad counts, the complete framed actual canonical N-Quads, and deterministic textual expected-only/actual-only canonical lines. Blank-node relabeling makes textual differences potentially noisy; these are explicitly not minimal semantic differences.

The exact equality comparison and terminal StageFailed remain. Failure bytes never become successful products, cache receipts or fixture selectors. No fixture edit, weakened gate, second producer or local corpus construction. Principles 4 and 7 govern preserved source ownership and reviewable evidence.

Qualification: two independently authored tiny string controls for exact Unicode byte counts, complete payload and empty payload; producer-independent workspace Clippy, source hygiene, formatting and normal hooks. Hosted production remains required to retrieve the actual ONNX output before any fixture repair. This unit does not discharge issue1902 parity acceptance or native completion blocker1897/1803.


### paudley — 2026-10-22T00:53:35Z

The [hosted descendant run](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39691822332) passed the repaired catalog-carrier path and completed reasoning, then generation A reached the ONNX graph-isomorphism failure again.

The diagnostic now supplies the complete actual canonical graph: 68,936 UTF-8 bytes and 294 quads, compared with the fixture's 64,052 bytes and 275 quads. The recovered payload exactly matches its announced byte count and quad lines, with only runner timestamp prefixes removed. The full failure log and exact payload are preserved for semantic review.

The fixture is untouched. Every semantic difference must be checked against the lifter before any expected-output change; a textual difference count alone does not justify blessing it. No local producer or duplicate hosted run was started. Other jobs in this run remain pending.


