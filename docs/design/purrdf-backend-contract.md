<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# GMEOW requirements for the PurRDF backend

The [PurRDF Backend Contract](https://github.com/Blackcat-Informatics/purrdf/blob/main/docs/design/purrdf-backend-contract.md)
is the authority for PurRDF's backend traits and their invariants. PurRDF owns
the generic RDF, dataset, storage, parser, query and serialization mechanisms.
GMEOW owns its ontology semantics, compilation, world scope, grounding and
proof-ledger policy above those mechanisms.

This document specifies what GMEOW requires when integrating that backend.
Trait definitions and API details belong to PurRDF's contract and source.
Use the dependency selected by [`Cargo.toml`](../../Cargo.toml) and resolved by
[`Cargo.lock`](../../Cargo.lock); this document does not define a separate
trait contract or override the upstream one.

## Required backend integration

GMEOW requires one backend selected at compile time for the chosen operation.
A missing backend is a hard build/link failure. There is no backend-free mode,
runtime backend registry or degraded alternate implementation. Explicit resident,
paged and pack read paths are representations consumed through PurRDF's supported
interfaces, rather than optional fallback engines. Static generic integration
does not impose an additional erased or object-safe trait layer on PurRDF.

## Borrowed reads and scoped identity

GMEOW consumers must use the upstream borrowed and id-based read seam without
introducing per-row heap allocation or term-string cloning merely to traverse a
view. Keep borrowed values and cursors within their owner or read-guard lifetimes;
do not mutate through a shared read handle.

An id is meaningful only within the upstream identity owner or dictionary
generation that issued it. Resolve and re-intern term values when crossing those
boundaries. Do not persist, compare or reinterpret unrelated owners' numeric ids
as durable RDF identities.

## Pattern and graph selection

Use upstream pattern lookup and resolve values into the selected view's identity
space before applying id-based filters. Preserve the distinction between any
graph, the default graph and a specific named graph. GMEOW's world scope must
select the intended graph; an unsupported graph operation must fail explicitly
rather than silently widening or narrowing the request.

## Mutations and updates

Keep writes on the upstream mutation or update interface, separate from borrowed
reads. Cross-owner mutation inputs carry term values, not foreign local ids.
A selected GMEOW update requires atomic application under the backend's declared
transaction semantics; callers must retain those semantics when composing the
operation. This requirement does not introduce a GMEOW-owned storage or
transaction implementation.

## Cancellation and complete results

Dropping a resident read cursor must stop further pull-based work. A lazy or
paged operation must retain its declared provider, generation, page/byte budgets,
cancellation and deadline context. Use the upstream fallible operation boundary
when reads can fail and inspect its status before execution and after result
materialization. Preserve the first operational failure and its evidence.
Iterator exhaustion alone must never certify completeness after a provider
failure, exhausted budget, cancellation, deadline or generation mismatch.

## Diagnostics

Preserve PurRDF's structured errors and original causes when presenting GMEOW
diagnostics for parsing, loading, querying, updating or serializing. Operational
incompleteness and invalid data must remain distinguishable from a complete empty
answer. Do not swallow a backend failure, replace it with an empty RDF graph or
publish partial internal output as complete.

## Required capabilities

Use PurRDF's capability and support probes for the selected operation, including
its RDF 1.2 statement and graph requirements. A required capability that is absent
causes an explicit refusal. An explicitly selected operation that does not use
that capability may proceed under its own complete contract. Never silently drop
triple terms, reifiers, annotations, source context, loss records or lookaside
data to make a selected path appear successful.

## Shared reads and exclusive writes

Share read handles across threads only under the upstream types' supported
`Send`/`Sync` bounds. Keep each mutable handle exclusively borrowed, and use the
appropriate frozen snapshot before parallel reads. GMEOW must preserve the
backend's ownership and synchronization guarantees rather than assume every
possible backend or mutable store is concurrently shareable.

## Consumed interfaces

These names identify the upstream seams GMEOW consumes; their signatures,
invariants and object-safety properties are defined upstream.

| Interface | GMEOW use |
| --- | --- |
| `DatasetView` | Borrowed, id-based reads and selected graph access |
| `FallibleDatasetView` | Operation status and evidence when reads can fail |
| `DatasetMut` | Value-based mutations of the selected dataset |
| `TermFactory` | Interning at an explicitly owned identity boundary |
| `RdfParserBackend` | Parser ingress with structured failure |
| `SparqlEngine` | Query/update execution under the selected contract |
| `RdfSerializer` | Selected egress with explicit representation failures |
