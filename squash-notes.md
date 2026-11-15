feat(logic): the native engine reads evidence-only scoped claims of the selected theory and names the rest (Principles 2, 9, 17)

The native engine refused any program that carried a scoped claim, because the bare-program entries never
read a source axiom: a canon that states its facts as reified claims with provenance could not be decided.

`TheorySelection::top_level_only()` is the theory of every statement whose scope selects no context.
`logic:provenance` and `logic:confidence` are evidence about a report and select no context; standpoint,
time, modality and module do. `prepare_selected_theory_input` is the one place a scoped claim becomes a fact.
It reads a ground claim into the selected world, with the exact RDF terms its carrier holds, only when the
claim is evidence only: nothing is said about its carrier except the reification structure,
`logic:provenance` and `logic:confidence`; no statement in any graph has the carrier as object or mentions it
inside a triple term at any depth, as a predicate, or in a classic statement node; and the carrier does not
name a graph. The allow-list is closed, so an unknown annotation keeps the claim out. A named graph can keep
a claim out and is never evidence for one. A reifier with no scope coordinate is a quotation and is never
read.

Every other scoped claim is a `SCOPED_CLAIM_NOT_READ` drop that gives the claim, the reason and the graph, and
the `prove` report lists each claim read with its provenance and confidence. Under the new selected-theory
admission profile an axiom's scope is no reason to refuse the program, because the ledger has accounted for
it; a rule, head or body atom scoped outside the theory still refuses. The ledger is bound to the input, the
program and the selection, and the entry refuses any other combination. The world-local profile and its
preparation identity are unchanged. `PROVE_SEMANTICS`, the README, `LOGIC-IR.md` and `LOGIC-SEMANTICS.md`
state the rule.

Verified: independent review PASS at 417fd45d7 after five rounds, which found and closed a claim that another
claim's reifier refutes being read and a refutation in a named graph being missed. The production code read
(the claim resolver, the ingress ledger, the admission profile, the selection types, the `prove` wiring); the
tests rest on the review. At that head, on a clean tree: `gmeow-logic` synthetic selection 239 passed,
`gmeow-logic-compile` 318 passed, clippy with warnings as errors clean on both crates, `make helpers-hygiene`
and `make producer-recipe` exit 0, and the `prove` CLI and unit tests 26 passed with a bundle and codebook
from an earlier producer pass embedded. Pull request checks: helpers, lint, producer-build and rust-prebuild
green; the three producer jobs and the aggregate red as on main.

Not shown: no producer pass, gate or `make check`, and no hosted run of the Rust test shards. Byte identity of
the pipeline's own outputs is by reading only; any edit under `crates/logic` moves the kernel source
commitment that production witness identities include. Five tests of a wider `gmeow-logic` selection fail at
this head and are in a failure list recorded earlier on main. The cost of the all-graph scan and how many
claims the rules leave unread on a large canon are unmeasured. Statements a rule derives about a carrier are
not considered.
