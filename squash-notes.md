fix(logic): require a term index only on ordered argument and binder carriers (Principles 4, 7, 17)

`logic:TermCarrierIndexConstraint` required `logic:termIndex` on every `logic:TermCarrier`, although a carrier
in an RDF 1.2 triple-term position of a rule atom needs none, so 169 rule-variable carriers in the logic and
lang modules were reported as violations. The index is now required exactly where the compiler requires it:
on a carrier that is the object of `logic:argument` (the kept constraint) or of `logic:quantifiedVariable`
(the new `logic:BoundVariableCarrierIndexConstraint`). Both are `logic:Constraint` records with integrity
formulas; they are two because the compiler does not accept a disjunctive guard.

The slice's structural assertion reads the two formulas and all five carrier value kinds; the existing
counter-example is a true operand; a binder counter-example and a pass witness are added; conformance cells
pin their source shapes. The design note states the index rule and lists `logic:termApplication` among the
value kinds. French and Chinese entries cover every changed or new literal. One test in the logic compiler
protects the lowering of an objects-of guard to a `sh:targetObjectsOf` SPARQL constraint.

Verified: independent review PASS at 243454441; the whole diff read. A temporary probe read both constraints
through the production reader, derived the two objects-of targets and ran the lowered shapes in the linked
SHACL engine (0 findings on the well-formed fixture, 1 on each counter-example from its own shape, 0 on a
rule with carriers in triple-term positions). Two independent scans of authored Turtle found only the two
intended counter-examples without an index. `make lint` and the catalog checks pass. Pull request checks:
helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red as on
main.

Not shown: no producer pass, so that the 169 recorded findings clear, that the structural and conformance
cells pass in the harness, and the cost of the larger focus set are unproven. `make i18n-lint` was not run.
The reviewed-coverage golden is not refreshed; it is already far behind main and this adds 16 keys to that
drift.
