# Requirement and failure-path review

The intentional finance migration removed the old monetaryValue declaration and its global carrier. MonetaryAmount now owns its required decimal quantityValue and maximum-one decimal restriction; the generic math property remains free of a global functional characteristic. The source ledger must reflect that intended retirement. Hosted production reports exactly that lone missing IRI, and canonical Turtle inputs are unchanged since that run.

The integrity implementation is untouched. The source diff removes exactly one ledger IRI, adds none, and preserves sorted uniqueness. Existing independent tiny-graph tests cover reintroduced OWL markers, orphan carriers, duplicates and named missing/unexpected ledger drift. Corrections to the documentation name the real pipeline owner rather than a nonexistent corpus pinning test.

Independent source review found no blocking concern. Full merged-corpus equality and downstream financial production acceptance remain unrun locally and must come from hosted production. No source or expectation weakening is authorized by the CI exception.
