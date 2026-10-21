Added the subtype-to-QualityAssessment shortcuts, assessedEntity values on complete examples, and wrong-type inherited-assessment controls. This exercises the relevant hierarchy injected by the Tier-1 validator.

The suggested missing-assessedEntity negative would change the contract: QualityAssessment has a someValuesFrom existential without a closed-world ClosureEntry. The frontend deliberately projects it as supplied-value typing without minCount. Both subtypes retain positive unasserted-value controls, and the docs explain the boundary. derive_validation_shapes is unchanged.

The authenticated quality/math consumer remains unexecuted under the explicit campaign exception. Compilation and independent synthetic qualification are recorded separately.
