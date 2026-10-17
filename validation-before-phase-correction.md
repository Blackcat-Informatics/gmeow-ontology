# Current validation

Published head bc230b3f6 replaces the serialized explanation parser and RawDerivation with the authenticated native Reasoning payload retained by stage-snapshot. Three obsolete formatters were deleted. Modal source worlds, nested quoted terms and directional literal identity survive publication. The shared reader retains owner, graph, native commitment, payload and invariant checks. The sink still declares exactly its existing three carrier inputs; the original reason carrier remains releasable.

Qualification: native-term-entailments-settled-tests.log records 19/19 passing focused synthetic cases, including native term panels, shared-reader tampering/refusal checks, snapshot lifetime, sink inputs and the existing try-it goldens. native-term-entailments-settled-clippy.log records strict producer-independent workspace/all-target Clippy passing in 3m19. Normal commit hooks and push passed; their exact logs are native-term-entailments-commit.log and native-term-entailments-push.log. No test produced corpus.

Hosted run 38981058280 is running for this head; lint passed, producer-build and Rust prebuild were in progress when captured. Full generation and dependent gates have not passed. Prior run 38964521632 on a6c9c9986 failed at stage-gts-sink on the obsolete derivation namespace assumption, after reasoning took 4,763,339ms. Exact generation-a-current.log, generation-b-current.log and fixture-prefix-current.log remain evidence of that failure, not current acceptance.

Prior unchanged fixture/recovery/docs evidence remains scoped to its source: 47 combined fixture/recovery cases, 42 recovery controls and 43 documentation/native-observation controls passed, as did strict Clippy and normal hooks. Current-head independent completion acceptance is not established. Native semantic completion, prepared canon and reasoning performance remain open. No speedup is claimed. No local optimized producer, fingerprint, full gate or machine probe ran.

Earlier chronological evidence remains in validation-before-native-term-panels.md. See native-term-entailments-report.md for the delivered repair and original failure.
