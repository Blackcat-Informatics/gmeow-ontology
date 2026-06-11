# TQL — the GMEOW Temporal Query Language

TQL is a small **query algebra** over the events model, realized as parameterized
SPARQL 1.1 queries (`slices/core/temporal/queries/tql/*.rq`) — *not* a bespoke temporal-query
engine. GMEOW aligns to the temporal-query literature (stSPARQL, T-SPARQL,
SPARQL-ST) **by reference** (Constitution Principle 5): the model carries the Allen
interval algebra and the four clocks, and standard SPARQL 1.1 property paths compute
the transitive temporal closures with **no materializing reasoner**.

## What makes it work

Two model features (added in the #41 temporal deepening) make events temporally
queryable in plain SPARQL:

1. **Allen relations between events** — `gmeow:before`/`after`/`meets`/`metBy`/
   `overlaps`/`overlappedBy`/`starts`/`startedBy`/`during`/`contains`/`finishes`/
   `finishedBy`/`coincidesWith`, aligned to OWL-Time `interval*`, the Time Event
   Ontology (TEO), and ISO-TimeML TLINK relTypes. `before`/`after` and
   `during`/`contains` are transitive, so `gmeow:before+` is the transitive
   ordering closure — computed by the **property path**, not a reasoner.
2. **The four clocks** — `validFrom`/`validUntil` (when the fact holds),
   `assertedAt` (when observed), `recordedNoLaterThan` (carrier bound) — enable
   the bitemporal query.

A subclass-aware type test (`?e rdf:type/rdfs:subClassOf* gmeow:Event`) lets the
queries reach `gmeow:LifeEvent` / `gmeow:Activity` occurrences over the **asserted**
graph (the TBox subclass axioms are queried directly), and `(gmeow:during|^gmeow:contains)+`
traverses the declared `owl:inverseOf` pairs without materialization.

## The query toolkit

| Query | Parameters | Answers |
|---|---|---|
| `allen-closure` | — | every transitively-ordered `(earlier, later)` event pair |
| `before-event` | `focus` | events temporally before a focus event (Allen closure ∪ effective-datetime) |
| `during-event` | `focus` | events temporally within a focus event |
| `timeline` | — | every event with its effective start instant, ordered |
| `overlapping-window` | `windowStart`, `windowEnd` | events overlapping a time window |
| `bitemporal` | `validAt`, `asOf` | claims valid at `validAt` and asserted by `asOf` — standpoint-indexed, **no winner** (#43) |

## Running it

```bash
gmeow temporal timeline --data my-events.ttl
gmeow temporal before-event --data my-events.ttl --focus https://example.org/e/reception
gmeow temporal overlapping-window --data my-events.ttl \
    --window-start 2015-06-20T00:00:00Z --window-end 2015-06-20T23:59:59Z
gmeow temporal bitemporal --data my-claims.ttl \
    --valid-at 1895-01-01T00:00:00Z --as-of 2020-01-01T00:00:00Z
```

Parameters are bound via rdflib `initBindings` — values never touch the query text,
so there is **no injection surface**. The Python API is `gmeow_tools.temporal_query`:

```python
from rdflib import URIRef
from gmeow_tools.temporal_query import run_temporal_query
rows = run_temporal_query("before-event", graph, {"focus": URIRef("…/reception")})
```

## Why not a new query language?

A bespoke temporal-SPARQL **syntax** (a new parser + evaluator) would re-implement
stSPARQL / T-SPARQL — exactly the "rewrite someone else's tooling" that Principle 5
forbids. GMEOW instead makes the *model* carry the temporal structure (Allen
relations + four clocks) so that **standard SPARQL 1.1** — which every triplestore
already speaks — is the temporal query language. TQL is the curated algebra of those
queries, the temporal counterpart of the projection layer's closed algebra.

## See also

- `ontology/modules/events.ttl` — the Allen relations, `Duration`, `RecurrenceRule`,
  and the ISO-TimeML tense/aspect attributes.
- `mapping-dsl/equivalences/events.ttl` — the OWL-Time / TEO / ISO-TimeML alignments.
- `dist/gmeow-example-owl-time.ttl` — the events projected to pure OWL-Time
  `interval*` relations (an OWL-Time-aware reasoner runs interval-algebra inference
  over it).
