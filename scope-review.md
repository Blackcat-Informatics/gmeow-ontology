# Scope review

This is a self-review, not an independent audit.

Two concrete counterexamples showed that the original violation compiler could erase quantifiers and change the law. Both new tests failed before the correction. The compiler now treats native kind tests only as atomic consequences or their direct negations. Guarded universal consequences retain their binding requirements. Ordinary existential obligations and prohibited positive existential patterns are handled explicitly; unsupported alternation and binder collisions remain visible residue.

A second inspection checked capture between sibling expressions. Free names are reserved before a prohibited pattern is flattened, so a local existential cannot bind a name that is free in another conjunct. The added regressions cover this case, an existential relation shadowing the focus, and an existential wrapped around a negation.

The final selected run passed all 54 tests. It includes the actual authored-law frontend and enactment gate, every supported node kind, native equality and distinctness, required-single metadata, native error ownership, existing math behavior, and compiler scope safety. Strict affected all-target Clippy also passed with warnings treated as errors. Neither result establishes a full workspace or production-corpus pass.

Canonical ontology and structural-query sources remain unchanged. Unsupported laws do not leave partially emitted violation rules. Equality remains owned by the upstream native API. The source-policy scanner still checks its complete inventory and failure controls without querying the host.

The final tiny witness refresh and its read-only consumers are still running. Normal hooks, publication, complete PR feedback, and ghprsq integration remain required. Hosted generation and fixture-prefix acceptance must remain visibly pending until actual results arrive.
