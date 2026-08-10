<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# concepts

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/concepts` · **tier: core**

GMEOW Concepts are socially sustained representational categories — the categories, schemas, and abstractions that agents hold and apply to entities. A `gmeow:Concept` is **not** the gUFO universal (which lives at the foundational-ontology meta-level); it is a representational object sustained by usage and by standpoint-indexed categorization claims. The slice is deliberately flat-first: `gmeow:instanceOfConcept` covers the cheap 80% case, and a reified `gmeow:ConceptCategorization` StandpointClaim is promoted when vantage, graded typicality, temporal scope, or provenance must be first-class (Principle 4). Categorizations are standpoint-indexed and coexist without a single winner (Principle 9), concept structure is expressed through `gmeow:subsumes` and `gmeow:composedOf`, and conceptual change is modeled as a time-scoped `gmeow:ConceptTenure` whose prior intensions are closed and suppressed rather than deleted (Principle 10). <!-- codespell:ignore -->

## Terms

The concept class, its flat and reified categorization surfaces, concept-structure relations, and the tenure pattern for conceptual change.

### gmeow:Concept

A category, schema, or abstraction held by an agent — a socially sustained cognitive representation. This is **not** the gUFO universal; it lives in the domain of representational objects and is bound to agents through standpoint-indexed categorization claims.

### gmeow:instanceOfConcept

The flat shortcut that asserts an entity is categorized under a concept. Domain `gmeow:Entity`, range `gmeow:Concept`, non-functional: an entity may instantiate many concepts and competing categorizations coexist (Principle 9). Promote to `gmeow:ConceptCategorization` when vantage or graded membership matters.

### gmeow:ConceptCategorization

A standpoint-indexed claim that an entity falls under a concept — the reified form of `gmeow:instanceOfConcept`. It is a `gmeow:StandpointClaim`: `gmeow:vantage` names the categorizing standpoint, `gmeow:observedFeature` is the categorized entity, and `gmeow:observationResult` is the `gmeow:Concept`. Reuses `gmeow:typicality`, `gmeow:confidence`, `gmeow:accordingTo`, and `gmeow:claimModality` from sibling slices.

### gmeow:typicality

A graded degree of membership in `[0,1]`, attached to a `gmeow:ConceptCategorization`. This is prototype-theoretic typicality: a robin is a more typical bird than a penguin. It is solver-layer metadata, not an OWL reasoner entailment, and is distinct from `gmeow:confidence` (how sure the standpoint is) and `gmeow:claimModality` (the qualitative stance).

### gmeow:subsumes

A broader/narrower relation between two `gmeow:Concept` individuals. Non-functional and not declared transitive at the core: concept hierarchies may be polyhierarchical and source-specific, and transitive closure is computed by the solver layer (Principle 12). The bridge to `skos:broader` lives in the mapping DSL, never as an OWL axiom here.

### gmeow:composedOf

A compound concept is composed of one or more constituent concepts. Non-functional: a compound may have several valid decomposition schemes that coexist (Principle 9). Use for the internal structure of a concept, not for mereological parts of physical objects.

### gmeow:ConceptTenure

The reified, time-scoped fact that a concept had a particular intension over an interval — conceptual change made first-class. A concept's definition, applicability, or usage may revise over time; the old intension is retained as a closed tenure suppressed with `gmeow:displayable false`, never deleted (Principle 10). <!-- codespell:ignore -->

### gmeow:conceptHoldsFor

Binds a `gmeow:ConceptTenure` to the `gmeow:Concept` whose intension it time-scopes. Functional: one tenure records exactly one concept, while a concept may have many tenures over time. <!-- codespell:ignore -->

### Reuse of `gmeow:Determinacy` / `gmeow:hasDeterminacy` from kernel

Concept boundaries are often vague, fuzzy, or disputed. Rather than minting new concept-specific vocabulary, this slice reuses `gmeow:Determinacy` and `gmeow:hasDeterminacy`. A `gmeow:Concept` or a `gmeow:ConceptCategorization` may declare `gmeow:hasDeterminacy gmeow:determinacyVague` (or `fuzzy`, `probabilistic`, `disputed`) to record the ontic character of its boundaries. This is orthogonal to `gmeow:confidence` (epistemic certainty) and to `gmeow:typicality` (graded membership). The fuzzy/probabilistic arithmetic lives in the solver layer (Principle 12).

## Dependencies

Depends on `kernel` (`gmeow:SocialObject`, `gmeow:Entity`, `gmeow:Determinacy`, `gmeow:hasDeterminacy`, `gmeow:displayable`), `observations` (`gmeow:StandpointClaim`, `gmeow:observedFeature`, `gmeow:observationResult`), `standpoint` (`gmeow:StandpointClaim`, `gmeow:vantage`, `gmeow:claimModality`, `gmeow:accordingTo`), `temporal` (`gmeow:TimeScopedRelation`, `gmeow:TimeInterval`, `gmeow:duringInterval`, `gmeow:startedAtTime`, `gmeow:endedAtTime`), and `provenance` (`gmeow:confidence`).

## External alignment

The concepts mapping set is authored in
[`slices/core/concepts/mappings/equivalences.ttl`](./mappings/equivalences.ttl) and compiled to
`generated/mappings/gmeow-concepts.sssom.tsv` (materialized by `make check`).
All alignments are by reference (Principle 5); GMEOW never imports an external axiom.

| GMEOW term | External target(s) | Predicate | Note |
|---|---|---|---|
| `gmeow:Concept` | `skos:Concept` | `skos:closeMatch` | both are units of thought / categories in a concept scheme; GMEOW's concept is a socially sustained representational object, not the gUFO universal |
| `gmeow:Concept` | `ontolex:LexicalConcept` | `skos:relatedMatch` | bridge to the future lexicon slice: a lexical concept is a related but narrower language-bound notion |
| `gmeow:subsumes` | `skos:broader` | `skos:closeMatch` | broader/narrower direction is the same after domain inversion; the bridge is authored in the mapping DSL, not as an OWL sub-property axiom |

Conceptual-space and prototype-theoretic alignments are referenced in prose only. `gmeow:typicality` is inspired by Rosch prototype theory (graded category membership around a central prototype), and `gmeow:subsumes` / `gmeow:composedOf` are compatible with Gärdenfors conceptual-space accounts of similarity and conceptual structure. These are design notes, not resolvable RDF rows, so they do not appear in the SSSOM file.

## Example

See [`slices/core/concepts/examples/conceptual-change.ttl`](./examples/conceptual-change.ttl) for a worked example that models the change in the concept "planet" around the 2006 IAU redefinition, and the coexistence of competing standpoint-indexed categorizations of Pluto (Principle 9).
