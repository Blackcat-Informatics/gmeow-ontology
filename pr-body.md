ONNX types now use framed structural identities, so concrete extent `3`, symbolic extent `3`, unknown extents and delimiter-bearing symbol names retain distinct type/shape records. Input and initializer declarations receive explicit mathematical value-space Set domains; later compatible annotations are retained and conflicting metadata refuses.

The optional math-owned `expressionValueSpace` link keeps type descriptors and admitted values distinct. Symbolic extent declarations retain the natural-number domain. Legacy initializer/input identity is preserved. Principles 4, 5, 7, 17 and 19 govern this repair.

Validation: all59 ONNX lift cases passed on the byte-identical Rust source transferred from the parent qualification; strict producer-independent workspace/all-target Clippy passed. Formatting and normal hooks apply. Generated math-owner/slice/translation-floor, installed CLI, corpus and full-gate acceptance remain unrun under the recorded campaign exception; pending CI does not delay sequential merge.

Refs #1902, #1897, #1433. Full model-serving/arbitrary-RDF tensor acceptance remains in #1897, including the preserved failing native datatype-cardinality regression owned by #1803. This change contains the independently qualified ONNX metadata repair.
