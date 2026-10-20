GMEOW's existing DL service constructors now accept shared DatasetView inputs through PurRDF's native parser and reasoner. Fallible reads retain their original typed native cause in diagnostics, and callers can borrow the native proof-checking context while retaining the reasoning session.

Advance the substrate dependency consistently across the root and fuzz manifests and locks to its landed shared-view delivery. Preserve native append-admission causes and classify signature refusals by native variants. Retain the prepared class result by moving its unchanged cached value, and clarify the division between GMEOW's semantic authority and PurRDF's execution mechanisms (Principles 4 and 7).

Related to #1803. Positive world updates and production class-engine replacement remain unfinished; this PR does not close that issue.

Validation: 103 selected synthetic tests pass, including shared-view read failures, borrowed proof verification, class cases, GTS admission, signature policy and release rejection. Strict producer-independent Clippy, producer-recipe admission and normal signed hooks pass. The review finding about recorded canonicalization is fixed and verified.

Hosted CI is waived under the maintainer's explicit instruction. No local optimized producer, corpus materialization or full gate ran; corpus-backed acceptance remains unverified.
