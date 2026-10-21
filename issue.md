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


## Comments (0)

