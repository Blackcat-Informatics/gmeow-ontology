The [hosted descendant run](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39691822332) passed the repaired catalog-carrier path and completed reasoning, then generation A reached the ONNX graph-isomorphism failure again.

The diagnostic now supplies the complete actual canonical graph: 68,936 UTF-8 bytes and 294 quads, compared with the fixture's 64,052 bytes and 275 quads. The recovered payload exactly matches its announced byte count and quad lines, with only runner timestamp prefixes removed. The full failure log and exact payload are preserved for semantic review.

The fixture is untouched. Every semantic difference must be checked against the lifter before any expected-output change; a textual difference count alone does not justify blessing it. No local producer or duplicate hosted run was started. Other jobs in this run remain pending.
