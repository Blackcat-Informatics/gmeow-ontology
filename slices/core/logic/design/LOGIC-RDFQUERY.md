<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — RDFQuery as a surface over `logic:`, not a new stack

> The **query-surface** chapter of the GMEOW Logic design set: how a modern,
> composable RDF query language is framed as *another authoring surface over the
> canon* — one that compiles **into** `logic:` (which already projects out to SPARQL,
> SHACL, N3, and OWL) — rather than as a new language bolted **onto** SPARQL. It is the
> query sibling of the computation surface in [`LOGIC-SHACL-AF.md`](LOGIC-SHACL-AF.md)
> and obeys the doctrine stated once in [`LOGIC.md`](LOGIC.md) and generalized in
> [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md).
>
> **Status: design, P15-gated, language not committed.** This document fixes the
> *architecture* — where an RDFQuery surface sits relative to the canon — and names the
> concrete first consumer. It deliberately does **not** commit a grammar. A language is
> minted only once a named consumer is wired and the surface earns its keep
> (Principle 15); committing syntax before that would be the optionality the project
> forbids.
>
> **Reading this document.** The declarative present tense states the intended
> architecture; the conformance corpus and the loss ledger
> ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)) remain the enforcement of any claim a
> realization makes.

## The proposal this reframes

A recurring proposal is to grow RDF's query story by **extending SPARQL** — adding
functions, modules, pipelines, and named reusable queries to the SPARQL stack, and
publishing the result as a new query language (the external "RDFQuery" sketch invites a
W3C Community Group spec). The instinct is right that authors want composition,
reuse, and a less ceremonious surface than SPARQL 1.1. The architecture is backwards.

Under **Principle 17** and **Principle 4**, SPARQL is a *generated lossy projection* of
the canon, not a foundation to build on. Bolting a query language onto SPARQL would
inherit SPARQL's expressivity ceiling, make the projection a second source of truth, and
ground execution in "compile to SPARQL and hope" rather than in a reasoner. GMEOW
reframes RDFQuery as a **front-end over `logic:`**: the surface parses into the typed
IR ([`LOGIC-IR.md`](LOGIC-IR.md)), which already lowers out to SPARQL, SHACL, N3, and
OWL. The author gets the ergonomic surface; the canon stays the single ground; and the
reference RDFQuery is **grounded in a reasoner**, not in a transpile-and-pray pipeline.

## What GMEOW already owns

The reframing is cheap because the machinery already exists — RDFQuery is mostly a new
front-end over parts the project ships:

- **DSL → multi-surface compilation.** GMEOW already compiles authored DSLs to
  SPARQL **and** SHACL **and** RDF **and** OWL **and** N3 (the statement DSL, the
  mapping DSL, the test DSL). A query surface is one more front-end into the same IR and
  the same projectors — not a new back-end.
- **Named, composable units.** Reusable, named, parameterized query/derivation units
  are `logic:Rule` individuals, Scryer user predicates, and FnO functions — already
  first-class, content-addressed, and referenceable by IRI. Composition is rule
  composition, not string templating. The named/parametric traversal of
  [`LOGIC-PATHS.md`](LOGIC-PATHS.md) is the worked precedent: `:nearbyOrgs(?maxDepth := 2)`
  is reified data the engine reasons over, not privileged syntax.
- **Recursion and tabling.** The canon is Turing-complete with recursion, stratified
  negation, and tabled resolution behind it — the things a query author reaches SPARQL's
  limits for. A query surface over the canon inherits these for free.
- **Provenance and reasoning result.** Every answer carries content-addressed
  provenance and the typed reasoning result ([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)):
  a query is not a bag of bindings but a result with completeness, preservation, and
  derivation structure — the typed-result contract a transpile-to-SPARQL surface cannot
  give.

So RDFQuery does not need a new stack. It needs a parser into the IR, a result
presentation, and the discipline that the IR — not the surface — is canonical.

## The named consumer (P15)

The concrete first consumer this surface commits to is the **universal RDF-1.2
transcoder / language-server / SARIF surface**: the tooling that ingests RDF-family
inputs, offers editor intelligence over them, and emits diagnostics. That surface needs
a query dialect it can accept from a user, lower to `logic:`, evaluate against a
reasoner, and report on — precisely an RDFQuery front-end over the canon. Naming it
fixes the contract the eventual language must satisfy and keeps the design honest: the
surface exists to serve that consumer, and is minted when that consumer is wired, not
before.

A second surface — the `gmeow` CLI query command and the agent-memory MCP — is the same
`logic:` lowering presented through a different front door; it is a candidate later
consumer, not the committed one.

## What this document does and does not commit

- **Commits:** the architecture (RDFQuery parses *into* `logic:`, never compiles to
  SPARQL as its semantics); the canonical ground (the IR, with the existing projectors
  giving SPARQL/SHACL/N3/OWL surfaces for free); reasoner-grounded execution; and the
  named first consumer above.
- **Does not commit:** a grammar, a keyword set, or a serialization. Those are designed
  with the consumer, against the conformance corpus, when the surface is built — and the
  result is a front-end over the canon, governed by the loss ledger like every other
  surface.

## Where this sits

| Concern | Document |
|---|---|
| The projection doctrine this surface obeys | [`LOGIC.md`](LOGIC.md), [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md) |
| The typed IR an RDFQuery surface parses into | [`LOGIC-IR.md`](LOGIC-IR.md) |
| The reasoning result every answer carries | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md), [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md) |
| Named, parametric, composable units (rules, paths, functions) | [`LOGIC-PATHS.md`](LOGIC-PATHS.md), [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md) |
| The computation-surface sibling (derivation/aggregation → SHACL-AF) | [`LOGIC-SHACL-AF.md`](LOGIC-SHACL-AF.md) |
| The loss ledger and preservation contract any surface declares | [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) |

The vocabulary the canon is authored in is in [`../module.ttl`](../module.ttl); when the
RDFQuery surface is built, it lands as a front-end over that canon, not as a new
language beside it.
