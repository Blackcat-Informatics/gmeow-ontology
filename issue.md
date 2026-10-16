# Issue #1770: feat: certify and precompile the complete canon for consumer proof

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Problem

Composing the installed `gmeow prove` operation with the complete embedded canon currently makes every request stop before external prover execution. The exact current bundle produces 327 classical-FOF admission blockers:

- 266 `UNSUPPORTED_CLASS_EXPRESSION` (213 uncertified class-expression lowerings; 53 restriction/enumeration reachability cases)
- 51 `CONTEXT_ERASURE_FORBIDDEN` (standpoint/modal translation required)
- 7 `UNSUPPORTED_SELECTED_OWNER` (Contract owners lack a classical execution contract)
- 2 `RULE_TERM_DISTINCT_REQUIRES_BINDING_SEMANTICS`
- 1 `COMPILER_ERROR_IN_SELECTED_THEORY` (the probabilistic profile carries a measure without a declared probability model)

This is the concrete unsupported-grounding set behind the full-canon acceptance criterion in #1768. Silently dropping these units would violate the source-admission contract and cannot be justified by bounded-corpus evidence.

## Scope

- give every selected canonical unit a typed, certified FOF disposition;
- implement the missing class-expression, standpoint/modal, contract-owner, distinct-body, and probabilistic-profile semantics, or give an explicit typed proof-task selection that excludes a unit for a sound documented reason;
- preserve standpoint, provenance, complements, loss evidence, and stage contracts;
- produce the canon FOF base once in the O3/full-LTO producer and carry it in `gmeow.gts`, rather than recompiling the flattened canon on every consumer invocation;
- compose caller premises/goals with that prepared native base in memory, with no serialization/reparse step;
- make `gmeow prove` reach the selected native/external prover against the complete admitted canon from a blinded cwd;
- report the exact bundle and prepared-canon identities.

## Acceptance

- complete-canon admission has zero blockers for the supported proof profile;
- the #1755 navigation obligation works through the canonical carrier and its ordinary-domain-triple form fails with the intended grounding diagnostic;
- focused product tests cover GMEOW proof behavior without duplicating PurRDF or external-prover conformance;
- runtime evidence records elapsed time, peak memory, and intermediate bytes for prepared-canon load plus request composition.

Depends on #1712, #1737, and #1738; blocks the full semantic completion of #1768. Principles 4, 7, 12, 17, 18, and 19 apply.


## Comments (1)

### paudley — 2026-10-16T00:54:10Z

Current source audit for the historical probability-model blocker:

The authored logic module already declares `FullIndependence` as a
`ProbabilityModel`. The compiler's structural admission recognizes a declared
model under both RDF typing and native `logic:instanceOf`; it only consults the
selected default graph. This does not select an independence model for an actual
probabilistic query, which still requires its explicit model declaration.

The combined candidate strengthens the existing compiler regression to cover both
typing surfaces, require the emitted contract, and retain its uncertainty facet.
A new synthetic named-graph control requires the hard contract diagnostic when
the only model is outside the selected graph. Together with the existing absent-
model and native structural-typing controls, the selection passed 4/4. Source lint
and producer-independent strict Clippy also passed.

These test-only changes do not establish a new full-canon blocker count. The
historical 327-unit inventory belongs to its original bundle/selection and must
be remeasured on the authenticated new product; it is not a reason to invent a
model or erase graph context. The complete prepared-canon, request composition,
grounding-diagnostic, native/external proof and blinded consumer requirements
remain open. Current production CLI routing still compiles caller files, so the
one-file distribution evidence alone cannot establish those requirements.

The existing optimized producer build continues for the authenticated bootstrap.
No new PR is being opened, and no full proof or issue completion is claimed.


