<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Typed Intermediate Representation

> Status: canonical target architecture for the compiler's intermediate representation. The IR is
> the single typed structure every `logic:` source compiles into and every projection compiles
> out of. Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)); the surface profiles that
> select how the IR is evaluated are defined in [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md), and the
> model-theoretic meaning of its constructs in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md).

## What the IR is

The IR is **full first-order**, not a Horn or Datalog fragment with extensions. A `logic:` program
is parsed once into a typed IR; every output — OWL, Datalog, N3, the Common Logic dialects, the
canonical RDF 1.2 serialization — is a projection *of* the IR; and every external dialect ingested
is parsed *into* the same IR. There is exactly one IR; the surface a request targets is a facet of
the reasoning contract, not a different internal form.

The earlier framing of the IR as "Datalog plus negation-as-failure" is superseded: that fragment
is one evaluable subset of the IR, reached by lowering, not the ceiling of what the IR can hold.

## Node kinds

The IR is a typed sum, not an untyped triple bag. Every node declares its kind, and the kind
governs what may be done with it:

- **object-level formula** — an ordinary first-order formula over the domain of discourse;
- **meta-level formula** — a formula *about* formulas (a statement that quotes or ranges over
  other propositions, distinct from the propositions it mentions);
- **constraint** — an integrity condition whose violation is a finding, not a derivation;
- **derivation rule** — a head entailed from a body, the productive subset;
- **query** — a goal to be resolved, with its answer shape;
- **transaction program** — a state-changing composite over the path semantics of
  [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md);
- **action schema** — a named precondition/effect/invariant template a transaction program may
  invoke;
- **validation shape** — a closed-world data-shape condition (the SHACL-shaped subset).

Keeping these kinds distinct is what prevents a constraint from being mistaken for a rule, a query
operator from being read as a program operator, or a meta-level quotation from collapsing into the
object-level assertion it quotes.

## What the IR makes explicit

A typed first-order IR must pin down the decisions that informal rule languages leave implicit.
Each is a declared property of the IR, never an unstated convention:

- **Equality and congruence** — whether equality is asserted, derived, or absent, and the
  congruence it licenses.
- **No unique-name assumption by default** — distinctness is asserted, not presumed from distinct
  names; a unique-name policy is an opt-in (the `Equality` facet of the contract).
- **Datatype semantics** — the value spaces and comparisons for typed literals.
- **Existential witnesses and Skolem terms** — how existentials are witnessed, with stable Skolem
  identity so a witness is the same across re-evaluations.
- **Variable hygiene and alpha-equivalence** — bound-variable renaming is meaning-preserving; the
  canonical form is alpha-normalized so equal-up-to-renaming formulas share one identity.
- **Domain-closure assumptions** — whether the domain is closed (only named individuals exist) is
  declared, never assumed.
- **Explicit versus default negation** — strong negation and negation-as-failure are different
  nodes, never conflated (and selected per contract).
- **Formula-versus-program operator typing** — connectives over formulas and combinators over
  transaction programs are typed apart; serial composition is not conjunction.
- **Quantification over predicates and types** — permitted where the foundation's higher-order
  instantiation requires it, with the order tracked so chains stay coherent.
- **Stratification of any truth or `holds` predicate** — a predicate that reflects truth of other
  statements is stratified so the IR cannot encode a self-referential paradox by accident.

## Lowering and the preservation judgment

A projection lowers the IR toward a target whose expressivity is usually narrower. Lowering is
never assumed faithful. **Every lowering returns a preservation judgment** describing exactly what
the target preserves:

- **exact** — the target answers the same questions as the canonical form for the declared query
  class;
- **sound but incomplete (under-approximation)** — everything the target entails is canonically
  valid; it may miss answers;
- **complete but possibly unsound (over-approximation)** — the target will not miss answers; it may
  add some;
- **validation-only** — the target detects some invalidity but is not an entailment relation;
- **unsupported** — the construct cannot be expressed in the target at all.

A formula that lowers to `unsupported` is **carried and flagged, never dropped**, and every result
downstream of a lowering discloses which formulas the target did not evaluate (see
[`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md)). The aggregate of these
judgments is the loss ledger that accompanies the generated artifacts (see
[`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)).

## Canonical identity

The IR has a canonical form — sorted, alpha-normalized, with content-addressed identity for
formulas, witnesses, and derivations. Two programs that mean the same thing have the same canonical
IR; round-tripping a program through any faithful projection and back yields the same canonical IR.
This canonical identity is the anchor the conformance contract checks against and the basis for the
content-addressed provenance that proofs and explanations cite.

## Constitutional alignment

One canonical form; every surface a generated projection of it, each carrying an honest
preservation judgment. The IR is where the doctrine "describe once, generate the rest" is enforced
for the reasoning layer, and where "never silently degrade" becomes a typed, machine-checked
property rather than a promise.
