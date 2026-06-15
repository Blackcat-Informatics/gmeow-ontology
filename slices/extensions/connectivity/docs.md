<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Connectivity — traversable links, reified connections, and routes

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/connectivity` · **tier: extension**
> The universal graph layer: anything that can be traversed is said here, once.

GMEOW keeps three structural vocabularies rigorously apart: mereology (`partOf` — what is
*made of* what), RCC-8 adjacency (what *touches* what), and connectivity — what can be
*traversed* to what. This slice owns the third. The flat shortcut `gmeow:connectsTo`
covers the 80 % case; promote to a reified `Connection` when the link itself needs a
period, cost, weight, bandwidth, confidence, or standpoint (the flat-first pattern). The
constructs are deliberately universal: transit lines, network paths, citation chains,
social paths, and dependency chains are all the same machinery with a different
`RouteKind` value (Principle 9 — kinds are data, not subclasses). Path geometry,
ordering, and cost are computed by the solver layer (Principle 12), never asserted as
triples. The slice is part of the the design Location-as-reference-frame design.

Its Principle-15 consumer, declared in the manifest: **network/route topology for places
and the virtual-location realm** — the graph the solver walks when it answers "how do I
get from here to there", physically or virtually.

## The link layer

### gmeow:Connection

The reified traversable link — a `gufo:Relator` between two entities, able to bear its
own period, cost, weight, bandwidth, confidence, and standpoint. The flat shortcut is
`gmeow:connectsTo`; promote when metadata matters. Connections are admissible subjects of
the accessibility slice's `AccessibilityAssertion` — a barrier can live on the *link*,
not just the endpoint.

### gmeow:connectionSource · gmeow:connectionTarget

The two functional role properties of a `Connection`: one source, one target per relator.
Direction is the relator's, not the vocabulary's — `connectsTo` itself is neither
symmetric nor transitive at the universal level, which is precisely what lets genealogy
declare `hasSpouse`/`hasSibling` (symmetric) and `hasParent`/`hasChild` (directed) as
sub-properties without leaking axioms across domains.

## The route layer

### gmeow:Route

A traversable path through a graph of connected entities — named, typed, with a defined
start and end. A `Route` is a first-class entity (it has identity: "the Number 4 line"),
but its actual path geometry, via-ordering, and cost are the solver layer's to compute
(Principle 12). The OWL core records that the route exists and what it links.

### gmeow:routeStart · gmeow:routeEnd · gmeow:routeVia

Endpoints (functional) and intermediate points (non-functional, *unordered*). The order
of via points is deliberately not asserted — sequence is a solver computation, and
asserting it would freeze one traversal as schema truth.

### gmeow:routeKind

Functional pointer into the `RouteKind` vocabulary: one kind per route.

### gmeow:RouteKind

The open value vocabulary of route classifications (Principle 9): seeds
`routeKindTransit`, `routeKindWalking`, `routeKindFlight`, `routeKindNetwork`,
`routeKindCitation`, `routeKindSocial`, `routeKindDependency`. The breadth of the seed
list is the point — the same Route machinery serves geography, networks, scholarship,
and software.

### gmeow:routeKindAccessible

The bridge value for the accessibility slice (slice-dependency doctrine refactor): a route *computed* to
satisfy a set of `hasAccessibilityNeed` facets. The value individual lives here, beside
its value class, keeping accessibility and connectivity mutually independent sibling
extensions — the vocabulary stays open (Principle 9) and neither slice imports the other.

### gmeow:hasRouteSegment

Transitive sub-route composition, a specialization of the universal `gmeow:hasPart`
spine: a route decomposes into legs, and the reasoner derives the segment closure. This
is the one place connectivity touches mereology — a segment is a *part* of its route,
while the stations it links are merely *connected*.

### gmeow:hasRoute

The convenience hook from any thing to a route that starts from, ends at, or passes
through it — the entry point for "what lines serve this station" queries.

## The frame seed (Principle 11)

### gmeow:referenceFrameNetworkGraph

A seeded `ReferenceFrame` (connectivity spine): virtual realm, one scalar axis, crisp determinacy,
metric `metricGraphHops`. Network distance is frame-relative like every other value —
"3 hops" names its frame just as "3 km" names a spatial one. Virtual locations measure
their distances here.

## Solver layer & alignment

Everything quantitative is deferred to the solver (Principle 12): shortest path, via
ordering, cost accumulation, accessible-route satisfaction, reachability closure. The OWL
core models the graph; the solver traverses it. Alignment is deferred — transport and
network standards (GTFS, schema.org trip vocabulary) are projection targets once the
consumer demands them, not pre-paid imports.

## Dependencies

Depends on `kernel` and `places`. Depended on by genealogy (kinship bonds are
`connectsTo` sub-properties) and consumed by the accessibility solver path.
