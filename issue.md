# Issue #1845: logic: preserve existential integrity guards and register refusal diagnostics

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Post-merge review of [the native integrity PR](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1844) identified two concrete corrections.

1. Positive existential guards are rejected by direct atom lowering. The co-occurrence frontend actually emits such guards. Lower them with capture-avoiding binder renaming, preserving outer variables and sibling scopes. Universal guards must remain unsupported; do not restore indiscriminate quantifier stripping.
2. The new native builtin refusal diagnostics are absent from both the diagnostic code catalog and eager registration. Include both kinds and verify the actual registration contract.

Review evidence: [existential guards](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1844#discussion_r4304300670) and [diagnostic registration](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1844#discussion_r4304300673).

Acceptance:

- Reproduce the co-occurrence failure through authored source, the real frontend, compiler and enactment gate. Verify both missing-side violations, both-present success and both-absent behavior.
- Test fresh existential bindings, shadowing, sibling independence, unsupported universal guards and unsafe free operands. Preserve all-or-nothing residue and the existing node-kind and comparison semantics.
- Register both diagnostic kinds exactly once and test their presence independently of the catalog itself.
- Run focused Make checks, strict affected Clippy and normal hooks. Refresh only the explicitly authorized tiny witnesses if semantic source changes, then run their read-only consumers. No test may construct the repository corpus, and no full local producer or gate is authorized.
- Resolve the two actual review findings, publish the correction and integrate only through ghprsq. Hosted production acceptance remains attached to the original open integrity issue.

The separate suggestion to query processor availability is declined under the explicit user prohibition on machine probing and resource caps. It is not part of this correction.

Principles 4, 7, and 17: preserve canonical laws, executable enforcement and one diagnostic authority.


## Comments (0)

