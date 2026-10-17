Generation and fixture production reject the canonical `gmeow:FlagshipScenarioFailureIriConstraint` because violation lowering reports `unsupported-consequent`.

The failure is recorded in [Generation A](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39051285193/job/117456045300) and the [fixture-prefix job](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39051285193/job/117456045340).

The constraint in `slices/grounding/logic/module.ttl` requires every `gmeow:enforcesFailureClass` value on a `gmeow:FlagshipScenario` to be an IRI. The structural tests also forbid treating this annotation as a logical object role. The frontend recognizes the node-kind declaration, but the compiler cannot produce an executable violation rule.

Repair the compiler and runtime so node-kind constraints execute against native RDF values. Preserve the failure marker, canonical source, and complete enactment gate. Do not substitute an object-property restriction or a hand-authored shape.

Acceptance:

1. Exercise the real frontend, violation compiler, and runtime with small independent fixtures. Cover valid IRIs, invalid literals and blank nodes, the other supported node kinds, missing paths, and exact source and focus identity.
2. Preserve comparison and required-single metadata checks, the annotation-property invariant, and complete enforcement of selected laws.
3. Run the affected Make checks and normal commit hooks. Tests must not produce the repository corpus. Hosted generation and fixture-prefix jobs must qualify the production constraint; the local optimized producer and full gate remain outside the authorized scope.
4. Inspect review feedback, integrate through ghprsq, and keep hosted acceptance visibly pending until it passes.

This follows Principles 4, 7, and 17: one canonical logic source, executable enforcement, and generated validation surfaces. The broader logic backlog remains in scope.
