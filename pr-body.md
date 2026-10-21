Adds `gmeow math evaluate` for the explicit `math-exact-decimal-v1` profile. Canonical arithmetic ASTs bind mathematical symbols to dimensionless input quantities and execute through existing native numeric rules. The command derives a fresh Quantity and normal proof artifact; malformed or conflicting ASTs, unsupported operators, missing inputs, division by zero and nonterminating decimal quotients fail without publishing partial output. Valid additional native consequences stay in the result and proof.

Exact division selects the upstream exact policy through a distinct canonical relation. Ordinary RDF numeric division retains its current policy. This is the shared prerequisite for #1894 and #1717; monetary amounts, currency frames, rates, prices and finance callers remain pending.

Validation: all six final evaluator tests passed, including exact unbounded arithmetic, source proofs, identity, required refusals and subclass consequences. Seventeen unchanged numeric/transport cases retain their passing coverage from the prior integrated run. Strict producer-independent Clippy passed with warnings denied. Normal commit/push hooks passed.

Public CLI compile/runtime, generated authored-module consumers, corpus, installed acceptance and full gates remain unverified under the current campaign exception. The authentic mandatory codebook is absent locally; no placeholder or weakened build guard is used. These limits keep both issues open.

Principles 4, 7, 12, 17 and 18 govern canonical ownership, evidence, derived output separation and shared execution. Refs #1894 and #1717.
