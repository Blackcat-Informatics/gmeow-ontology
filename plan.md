# Preserve metadata validation without annotation-role restrictions

Continuation of issue 1803 after merged PR 1840. The full logic objective and class delegation/deletion remain open. Principles 4, 7, 17 and 18 govern this increment. No local corpus production, host probes, hook bypass or weakened native admission is authorized. Existing source law and slice documents require metadata annotation semantics to remain intact.

## Task 1: Replace invalid source restrictions with canonical integrity

Status: committed as ee262a12f. Ten canonical records replace invalid annotation restrictions and preserve existing ownership/failure semantics; the companion label requirement is also repaired.

Preserve declarations for skos:definition and gmeow:enforcesFailureClass, docOwnerSlice, accordingTo and profileDescriptor. Replace their invalid OWL-style annotation-role restrictions in profiles, documentation, logic and lang with existing canonical constraint sugar: requiredness, resource/IRI kind and same-path forbidden distinct values for at-most-one resource metadata. Preserve failure classes and exact class scope. Remove obsolete closure entries and misleading source commentary belonging to the invalid restrictions. Keep actual object/data restrictions unchanged.

## Task 2: Demonstrate equivalent validation and native admission

Status: locally qualified. Thirteen distinct selected checks have passing coverage, including final native proof/equivalence, exact source identity and refreshed read-only witness controls. Strict producer-independent Clippy and hooks passed. The obsolete legacy owner block is removed. Authenticated source/corpus assertions require fresh hosted production and remain unrun locally.

Use a tiny synthetic canonical source through the actual frontend, generated SHACL and native validation. Compare its required-single-IRI rule with an independent three-facet fixture on good, missing, multiple, equal-denotation-but-distinct-IRI, blank, literal and unrelated-class cases. Assert expected focus nodes, not only mutual agreement. Retain annotation kind and no logical role restrictions. Add authenticated production consumer/source assertions for the changed source, without test-triggered construction. Fresh corpus acceptance remains hosted-only. Delete replaced legacy validation blocks only after the behavioral equivalence control passes. Run focused synthetic checks, strict Clippy, formatting and normal hooks.

## Task 3: Publish and integrate

Status: PR 1841 published. Hosted lint/helpers passed; larger builds and CodeRabbit pending, with no substantive feedback at final capture. Integration is authorized with those checks pending. Archive and cleanup remain to execute; full issue 1803 remains open.

Create a scoped PR that advances rather than closes 1803; collect CodeRabbit feedback and use ghprsq under existing authorization for pending hosted CI. Archive evidence before owned cleanup. Continue metadata admission qualification and native proof/stop/store/delta delegation work; do not claim this increment completes the issue.
