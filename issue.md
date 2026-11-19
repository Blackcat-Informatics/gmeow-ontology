# Issue #2930: Execute native property laws from canonical characteristic records

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, documentation, enhancement, cross-cutting, logic, rust, complexity-hard

## Body

Part of #2748. Blocked by: nothing. Blocks: #2748.

## Problem

The native schema engine reads functional and inverse-functional characteristic records directly. Transitivity, symmetry, asymmetry and irreflexivity have native laws, but those laws read only property type markers. Their canonical `logic:PropertyCharacteristicAssertion` record forms therefore remain `CHARACTERISTIC_RECORD_NOT_READ` boundaries.

The composed Principia consumer exposes six such boundaries: transitivity, asymmetry and irreflexivity for each of two authored relations. All selected source remains present; treating the records as metadata would incorrectly discard their laws.

## Required behavior

- Execute the already-supported transitive, symmetric, asymmetric and irreflexive laws from canonical characteristic records as well as marker forms. Reuse the existing native schema mechanism and shared record reader.
- Preserve the actual characteristic and relation premises in derivations and clash evidence. Do not manufacture a marker assertion in place of the source record.
- Admit a record only when its selected native law is implemented. Preserve malformed and unsupported characteristic diagnostics, graph and source selection, context boundaries, and independent object-level facts.
- Preserve existing functional and inverse-functional behavior. Reflexivity and other unsupported semantics remain explicit parent requirements.
- Describe the current supported record forms and their limits in the reasoning documentation.

## Acceptance

- Focused behavior checks show the record forms produce their native closure or clash, including a clash that requires transitivity before asymmetry can detect it.
- Irreflexive self-relations are refuted; unrelated or unsupported records cannot make a selection appear complete.
- Existing source/accounting and real CLI controls remain valid.
- The composed consumer is compared against the retained baseline: all 212 frozen documents, all original 67,044 anchors and the additional 21,039 physical annotation occurrences remain accounted for. The six characteristic boundaries are removed by execution, with every unrelated blocker still reported.

Principles 4, 7 and 17.


## Comments (0)

