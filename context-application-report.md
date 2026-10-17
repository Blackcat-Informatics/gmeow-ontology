Contextual application replacement — 94b5979d9

The red production-source/FOF control proved scoped unary p(a) at w and ordinary binary p(w,a) emitted an identical sentence. Native finite-context lowering also inferred a world from any ternary atom.

Replace that encoding with the canonical logic:contextualApplication family: context, reified relation and original arguments are separate coordinates. The original world-prepending translation and ternary-guessing decoder are deleted; there is no compatibility fallback. Shared native validation rejects malformed family uses, including binary RDF assertions/axioms and binary class properties. Principal-predicate metadata retains the original relation. Native modal evaluation consumes the explicit coordinates. Source profile v4 and compiler/snapshot cache versions bind the semantic change. Canonical ontology documentation and all seven French/Chinese annotations accompany it. Principles 4, 7, 12, 17, 18 and 19.

Validation:
- Exact failing reproducer: context-arity-before-fix.log.
- First expanded run: 275/277 passed; retained memo expectation and real binary-assertion admission failures: context-application-tests.log. Both corrected.
- Same 277 source/FOF/native IR/RDF/Common Logic/modal/temporal cases then passed in 0.305s: context-application-settled-tests.log. These include actual native contextual query evaluation and ternary-impersonation refusal. No test produced corpus.
- Strict producer-independent workspace/all-target Clippy passed with warnings denied: context-application-final-clippy.log.
- Complete helper hygiene passed: context-application-final-helpers.log.
- Full source lint passed before and after final translation wording: context-application-lint.log and context-application-final-lint.log.
- Normal signed commit hooks and push passed. No hook bypass.

Full hosted generation, authenticated consumers, ontology materialization/translation-floor qualification and current independent review remain unproven. This is an incremental fix, not completion of issues 1770/1768 or full canon semantics, prepared native canon, blinded installed consumer acceptance or production performance. Prior review request was rate-limited, not completed. No local optimized producer, fingerprint or host probe ran.

Latest prerequisite CI readback: pipeline run 38988869011 passed producer-build, lint and Rust prebuild. Generation (a) and fixture-prefix both failed after explicit runner shutdown signals; generation (b) remains live. The generation log demonstrated stage-docs-render completed before the shutdown. These failures do not establish a source defect or successful full qualification. Exact prefix log: context-root-fixture-prefix-failure.log.
