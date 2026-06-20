<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Typed Intermediate Representation

> The compiler's intermediate representation. The IR is
> the single typed structure every `logic:` source compiles into and every projection compiles
> out of. Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)); the surface profiles that
> select how the IR is evaluated are defined in [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md), and the
> model-theoretic meaning of its constructs in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md).

## What the IR is

The IR is **a typed unified logic IR with a full-FOL formula core**, not a Horn or Datalog
fragment with extensions. The IR is unified because it holds more than object-level formulas:
transactions, action schemas, validation shapes, and meta-formulas all live in the same typed
structure. A `logic:` program is parsed once into this typed IR; every output — OWL, Datalog, N3,
the Common Logic dialects, the canonical RDF 1.2 serialization — is a projection *of* the IR; and
every external dialect ingested is parsed *into* the same IR. There is exactly one IR; the surface
a request targets is a facet of the reasoning contract, not a different internal form.

**Predicates and types are reified as ordinary objects** (a HiLog-style reflection). When the
foundation needs to quantify over a predicate or a type, it quantifies over the *object* that
reifies that predicate or type, not over a genuine predicate variable. The object level therefore
stays first-order — quantifiers range over individuals in the domain of discourse, some of which
happen to be reified predicates and types — while still expressing what reads as quantification
over predicates and types. This is a deliberate design choice: a first-order object level with
reflected types, rather than admitting genuine predicate variables (which would push the IR beyond
first-order). This is why the formula core is full-FOL: the core quantifiers are first-order, and
higher-order-looking statements are recovered through reification rather than by extending the
logic.

"Datalog plus negation-as-failure" is one evaluable subset of the IR, reached by lowering, not the
ceiling of what the IR can hold.

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

A typed unified logic IR must pin down the decisions that informal rule languages leave implicit.
Each is a declared property of the IR, never an unstated convention:

- **Equality and congruence** — whether equality is asserted, derived, or absent, and the
  congruence it licenses.
- **No unique-name assumption by default** — distinctness is asserted, not presumed from distinct
  names; a unique-name policy is an opt-in (the `Equality` facet of the contract).
- **Datatype semantics** — the value spaces and comparisons for typed literals.
- **Existential witnesses and Skolem terms** — how existentials are witnessed, with Skolem identity
  scoped so a witness is the same across re-evaluations *of the same existential in the same
  setting*. A witness's identity is determined by (at least) the source formula it witnesses, the
  binding of that formula's free variables, the context or world in which it is introduced, and the
  governing reasoning contract. Scoping identity this way is what keeps witnesses for distinct
  formulas, distinct bindings, or different worlds from collapsing into one term by accident.
- **Variable hygiene and alpha-equivalence** — bound-variable renaming is meaning-preserving; the
  canonical form is alpha-normalized so equal-up-to-renaming formulas share one identity.
- **Domain-closure assumptions** — whether the domain is closed (only named individuals exist) is
  declared, never assumed.
- **Explicit versus default negation** — strong negation and negation-as-failure are different
  nodes, never conflated (and selected per contract).
- **Formula-versus-program operator typing** — connectives over formulas and combinators over
  transaction programs are typed apart; serial composition is not conjunction.
- **Quantification over reified predicates and types** — where the foundation's instantiation
  reasoning requires ranging over a predicate or a type, it ranges over the object that reifies it
  (HiLog-style), keeping the object level first-order; the reification layer is tracked so reflected
  chains stay coherent.
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
- **complete but possibly unsound (over-approximation)** — the target does not miss answers; it may
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
formulas, witnesses, and derivations. Two programs that normalize to the same typed structure under
alpha-renaming, RDF graph isomorphism, ordering normalization of commutative collections, and the
explicitly declared rewrite system share one canonical IR identity. This is **structural** identity,
not a claim to decide general semantic equivalence: two programs that happen to mean the same thing
but normalize to different structures have different canonical IRs, because deciding semantic
equivalence for the full IR is not reducible to content addressing. Round-tripping a program through
any faithful projection and back yields the same canonical IR. This canonical identity is the anchor
the conformance contract checks against and the basis for the content-addressed provenance that
proofs and explanations cite.

## Constitutional alignment

One canonical form; every surface a generated projection of it, each carrying an honest
preservation judgment. The IR is where the doctrine "describe once, generate the rest" is enforced
for the reasoning layer, and where "never silently degrade" becomes a typed, machine-checked
property rather than a promise.
