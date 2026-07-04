<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Named & Parametric Predicate Paths

> The **traversal** chapter of the GMEOW Logic design set: how a reusable, by-name
> graph walk — with a predicate wildcard and a bounded depth — is authored as a
> canonical `logic:` individual and projected to SPARQL property paths and Datalog.
> It makes precise the doctrine stated once in [`LOGIC.md`](LOGIC.md): `logic:`
> subsumes a standard as a fragment and then exceeds it. `logic:PathShape` is also the
> pattern substrate reused by the correspondence calculus's `get`/`put` legs
> ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)).

## What SPARQL §9 cannot say

SPARQL 1.1 property paths (Query Language §9) are the closest standard surface for
"walk the graph along these predicates." They are also a fixed grammar, and three
things a real topology query needs fall outside it:

- **No predicate wildcard.** A property path names predicates (`p`, `p1|p2`,
  `!(p1|…|pn)`). There is no operator for "follow *any* predicate," so "every node
  reachable within *n* hops, regardless of edge label" is not expressible.
- **No bounded depth `{n,m}`.** The repetition operators are `?` (0–1), `*` (0–∞)
  and `+` (1–∞). There is no `{1,3}`. "Within at most three hops" can only be
  approximated by writing out the union of sequences by hand.
- **No named or parameterized reuse.** A path is an anonymous string inlined at each
  query site. There is no `:nearbyOrgs`, and certainly no `:nearbyOrgs(?maxDepth := 2)`
  that binds the depth at the call.

These are not accidental omissions a future SPARQL revision will patch; they are the
shape of the language. `logic:` does not wait for them.

## The PathShape construct

A [`logic:PathShape`](../module.ttl) is a **named, parametric predicate-path traversal
specification** — a reusable description of how to walk the graph. It carries exactly
three facets, mirroring the issue's decomposition:

1. **A base step** — either a named predicate ([`logic:pathStepPredicate`](../module.ttl))
   *or* a predicate wildcard ([`logic:pathWildcard`](../module.ttl) `true`). The two
   are mutually exclusive: a step is one named edge XOR any edge. A shape carrying
   both (or an unrecognized `pathWildcard` literal) is malformed: the frontend
   extractor emits a diagnostic and skips the shape — it is never silently dropped
   and never causes a hard process abort, consistent with the project-wide
   frontend-extractor convention.
2. **A bounded depth** — [`logic:pathMinDepth`](../module.ttl) …
   [`logic:pathMaxDepth`](../module.ttl). The minimum defaults to one hop when absent;
   an absent maximum means *unbounded* (the `+` / transitive-closure reading). When
   the maximum is present the path is a bounded `{min,max}` traversal — the construct
   SPARQL §9 lacks. A minimum above the maximum is malformed and is surfaced as a
   diagnostic; the shape is skipped.
3. **A predicate-namespace scope** — [`logic:pathNamespaceScope`](../module.ttl), an
   IRI prefix that bounds a wildcard step to a single vocabulary. An unscoped wildcard
   matches predicates in any namespace; scoping it keeps the fan-out of an
   any-predicate walk finite and intentional. (The issue calls out unbounded
   wildcard fan-out as a risk; the namespace scope is the answer to it.)

The shape is the **canonical** form. It is not OWL, not SPARQL, not Datalog — it is
the `logic:` individual those surfaces are projected *from* (Principle 17). Authoring
it once gives a term that competency questions, rules, and tooling reference by IRI.

## Named, parametric invocation

A [`logic:PathInvocation`](../module.ttl) is the reified form of a by-name call. It
binds the shape's declared depth parameter ([`logic:pathDepthParam`](../module.ttl),
e.g. `"maxDepth"`) to a concrete value ([`logic:pathDepthArg`](../module.ttl)) via
[`logic:invokesPathShape`](../module.ttl). The surface syntax

```text
:nearbyOrgs(?maxDepth := 2)
```

is a *rendering* of that individual, not a parallel construct: parsing the call
produces the invocation, and the invocation alone is what the engine reasons over.
This is the same reification discipline the rest of the logic set uses — a parametric
call is data, not a privileged syntax.

## Two projections, and where the loss is

A PathShape is rendered to two evaluable surfaces. Each projection declares its
preservation in the loss ledger ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md),
`target_meta`), never silently.

### Datalog — exact, and the runtime

A bounded `{min,max}` path lowers to a **stratified, terminating** Datalog program by
unrolling the depth without recursion:

```prolog
edge(X, Y)  :- triple(X, P, Y).            % P restricted to the namespace scope, if set
reach1(X,Y) :- edge(X, Y).
reachK(X,Y) :- reach«K-1»(X, Z), edge(Z, Y).
reachable(X,Y) :- reachK(X, Y).            % for K in min..=max
```

An unbounded path (no maximum) lowers to the recursive transitive closure. Either way
the result is **exact** — neither missing reachable nodes nor adding spurious ones —
and the program is just ordinary Datalog, so it runs on the **existing native
least-model fixpoint engine** with no new machinery. This is the PathShape *runtime*.

> A note on reuse. The earlier design suggested reusing the graph-descent resolver as the
> runtime. That resolver performs context-aware, triple-by-triple up-projection
> lifting — it is not a generic *n*-hop traversal engine, and bending it into one
> would be the wrong tool. The honest reuse is the Datalog fixpoint above: "all nodes
> within *n* hops" *is* a least-model computation, and GMEOW already has that engine.

### SPARQL property paths — extended, with declared exit loss

The canonical PathShape also projects to the SPARQL property-path algebra. Per the
SUBSUME-EXTEND-ENHANCE doctrine, GMEOW **extends** that algebra rather than crippling
the path to fit §9:

- A named-predicate `{min,max}` path projects to a bounded-range node — losslessly
  representable in the extended algebra, and unrollable to a finite
  alternative-of-sequences (`p | p/p | p/p/p`) for a standard consumer.
- A wildcard step projects to an any-predicate node, optionally namespace-scoped.

The extension lives in the algebra, so the *canonical* SPARQL projection is faithful.
Loss appears only at the **standard-SPARQL exit gate**: a consumer restricted to
SPARQL 1.1 §9 receives the unrolled (bounded) or approximated (wildcard) form, and
that fact is recorded in the ledger against the
[`logic:PropertyPathProjection`](../module.ttl) target. Trimming happens at the exit,
never in the canon — the project's maximal-information-flow rule.

#### A declared asymmetry

The extended algebra is symmetric for **bounded depth**: GMEOW parses `p{n,m}` and
serializes it. The **predicate wildcard** is, for now, *emit-only*: it has a
serialization but no surface query syntax to parse it back from, because no
GMEOW query surface yet consumes one. This is stated here rather than hidden — when a
query surface lands, the parse direction closes the loop. A reader must not infer a
round-trip the grammar does not yet provide.

## Where this sits

| Concern | Document |
|---|---|
| The projection doctrine these surfaces obey | [`LOGIC.md`](LOGIC.md), [`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md) |
| The typed IR a PathShape compiles through | [`LOGIC-IR.md`](LOGIC-IR.md) |
| The loss ledger and preservation kinds | [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) |
| The least-model runtime that evaluates the Datalog projection | [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) |
| Transaction-logic *state* paths (a different "path") | [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md) |

The vocabulary is in [`../module.ttl`](../module.ttl); a worked example —
`:nearbyOrgs(?maxDepth := 2)` and a named-predicate `:ancestorsTo3` — is in
[`../examples/predicate-paths.ttl`](../examples/predicate-paths.ttl).
