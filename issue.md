# Issue #2853: Scope the native datatype wait to the writers that can change the property outcome

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, logic, rust, complexity-hard

## Body

The native datatype family waits on a world-wide completion rank, so one class-model obstruction blocks every datatype outcome in its world

Observed on the release producer log for eff6e9853: about 40,700 error findings are "Datatype World: Blocked" on logic:termIndex (range xsd:nonNegativeInteger) and logic:termVariable (range unionOf xsd:string xsd:token). Their restriction sets are empty; the withheld read is the world-level completed read, ranked by the maximum stratum of every unknown-predicate writer in the world (crates/logic/src/reason/refute/effects.rs:659-690, read at native.rs:682-690 and native.rs:1227-1252).

Required: the datatype wait is scoped to the writers that can change that property's outcome. The wait must still include any writer that can add an rdf:type to an individual carrying a value of P when the added class restricts P, and the owl:Nothing writers still block the global verdict.

Acceptance: a one-world reproducer (one termIndex statement, one class with an equivalence obstruction in scope) goes from blocked to decided for termIndex and termVariable; a world where a class restricting P can still be asserted of the individual stays waiting. One test each. One independent review.


## Comments (0)

