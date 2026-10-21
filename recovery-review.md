# Independent focused recovery review

No new correctness defect found in the finite guard. Both numeric fields recovered into JSON, classifierScore and thresholdApplied, pass required_decimal. The guard rejects NaN, positive/negative infinity and f64 overflow before constructing a successful capture; actual CLI recover invokes native ToJson/write_pretty only on Ok(capture). The new tiny-RDF test exercises both properties independently; the existing finite put/get control remains positive coverage.

Coverage improvement to address: also assert subject and relevant predicate in the refusal, beyond finite-decimal text, since actionable field identity is part of the repair. Runtime qualification is running; this source-only review makes no execution claim. No source mutation, builds, model operations or corpus production occurred in review.

The initial38-test rerun passed all38controls, including the new ten-case nonfinite recovery control. The suggested actionable-field coverage is now added: derive affected subject identities from the tiny emitted graph and assert each refusal names its predicate and one of those subjects. Only the test changed; final package rerun is active.
