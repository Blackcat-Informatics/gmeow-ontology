# Issue #2931: Preserve actual collection roles and owners through theory selection

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, cross-cutting, logic, rust, complexity-hard

## Body

Part of #2781 and #2748. Blocked by: nothing. Blocks: #2781 and #2748.

## Problem

Non-class collection cells are emitted by `lift_list_statement` as `AxiomSource::ClassExpression` at the collection head. The source inventory consequently invents a class definition for that head. Selection can then admit the cells through this synthetic class role even when their actual property-list owner is excluded.

Source inspection establishes the incorrect role and membership path. A runtime verdict consequence has not been measured.

## Required behavior

- Record each collection's actual source owner and semantic role through lowering, canonical deduplication, the versioned source trace, inventory and selected views.
- A property or value collection does not create a class definition merely because its RDF cells were lowered.
- Shared cells retain every actual contributing owner. An excluded owner cannot introduce an independent root; an included owner cannot lose its source contribution through early cell deduplication.
- Preserve graph placement, physical source carriers, strict list diagnostics and unrelated facts. Do not admit a previously unsupported list law by changing its accounting label.

## Acceptance

- With actual modules M and N, a property chain owned by N contributes no executable list-cell axioms when only M is requested.
- A genuinely shared collection follows each actual owner and role, including when one owner is excluded.
- Genuine class definitions and malformed-list refusals retain their behavior.
- All source occurrences remain inventoried. Any removal of synthetic class obligations and resulting trace identities is explicitly reconciled.
- Focused native/compiler and actual CLI checks cover the corrected behavior; the applicable doctrine states the source-ownership contract.

Principles 4, 7 and 17.


## Captured comment summary

The captured comment links the module-composition change in pull request 2929 and offers planning controls. It contains no technical finding or additional implementation requirement. This summary omits interface instructions and promotional text; the issue body and all other captured fields are retained.

Source: https://github.com/Blackcat-Informatics/gmeow-ontology/issues/2931#issuecomment-6980036722

Original Markdown SHA-256: `287346e0f0e5ca6d5ca889a02da49074328f03ab3169e281b4031aed01a60934`. Original JSON SHA-256: `2a708eb9bacc34b8342c06afa75e5f487315fd482266c418d96b7a630cd4a644`.
