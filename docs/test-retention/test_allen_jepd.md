# Retention: `tests/test_allen_jepd.py`

**Category:** Domain invariant → slicetest cells (partial)

## What it tests

Tests for Allen interval relations and JEPD disjointness (issue #67).

## What moved

All module-local structural assertions migrated to
`slices/core/temporal/tests/structural.ttl`:

| Pytest function | DSL cell IRI |
|---|---|
| `test_all_interval_level_allen_relations_exist` | `ex:saAllIntervalLevelAllenRelationsExist` |
| `test_interval_before_and_after_are_transitive` | `ex:saIntervalBeforeAndAfterAreTransitive` |
| `test_interval_coincides_with_is_symmetric_and_transitive` | `ex:saIntervalCoincidesWithIsSymmetricAndTransitive` |
| `test_no_event_interval_property_disjointness_in_owl` | `ex:saNoEventIntervalPropertyDisjointnessInOwl` |

## What stays in pytest

`test_no_owl_all_disjoint_properties_over_interval_relations` — a whole-graph
sweep over every `owl:AllDisjointProperties` to ensure no interval-level Allen
relation is grouped into an OWL disjoint-properties axiom. A finite
module-scoped SPARQL ASK cannot express this universal guard.

## What is needed to retire the remaining function

Either a Rust-native whole-ontology scan for `owl:AllDisjointProperties`
membership of non-simple properties, or a SHACL shape that closes over the
entire merged graph. The slicetest harness evaluates one slice module at a time
by design, so this cross-ontology sweep stays in pytest for now.
