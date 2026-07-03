# Retention: `tests/test_calendar.py`

**Category:** Domain invariant → slicetest cells (partial)

## What it tests

The calendar and scheduling slice (#62) — remaining pytest guards.

## What moved

| Pytest function | New home | DSL cell IRI |
|---|---|---|
| `test_organizer_and_attendee_roles_exist` | `slices/core/events/tests/structural.ttl` | `ex:saOrganizerAndAttendeeRolesExist` |

## What stays in pytest

* `test_calendar_temporal_datatypes_are_datetime_or_duration` — complex
  blank-node union + cardinality check on `gmeow:reminderTrigger`
  (`len(range_nodes)==1`, `owl:unionOf` list walk); not expressible as a simple
  module-scoped ASK.
* `test_calendar_axes_are_independent` — `itertools.combinations` sweep over 10
  orthogonal properties (45 pairs × 4 assertions = 180 checks); converting to a
  finite blacklist would silently narrow coverage.

## What is needed to retire the remaining functions

* A Rust-native blank-node list walker or a SHACL shape for the union datatype
  cardinality of `gmeow:reminderTrigger`.
* A Rust-native orthogonality guard over all property pairs, or acceptance that
  the dynamic pair-sweep is the most compact expression.
