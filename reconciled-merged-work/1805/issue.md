# Issue #1805: logic: rule projection uses RDF 1.1 classic reification (rdf:Statement); move to RDF 1.2 triple terms

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Defect

The `logic:` program's RDF projection (`crates/logic-compile/src/projections/rdf.rs`, the "Rules as logic:Rule nodes" block) encodes every rule's head, body and negated-body atom with **RDF 1.1 classic reification**: `rdf:type rdf:Statement` plus `rdf:subject`, `rdf:predicate` and `rdf:object`. `.goals` requires RDF 1.2 only, with no RDF 1.1 shortcuts.

## Observed consequence

These atom nodes reach the `graph/logic` class world: 232 `rdf:type rdf:Statement` assertions such as `logic:rule/_000001/body/0000`. The OWL-Direct reverse mapping in purrdf 3.0.0 reports `rdf:Statement` as an `unrecognized-term` boundary, so that world's consistency verdict cannot be exact. A one-triple probe (`<s> rdf:type rdf:Statement`) reproduces it.

Probed on the same engine, the RDF 1.2 form `<b0> rdf:reifies <<( <x> <p> <y> )>>` decides with **no** boundary.

## Why this is not a mechanical swap

- Atom variables are encoded as plain literals (`"?x"`), including in subject position. An RDF 1.2 triple term cannot take a literal subject, so variables need a term representation that is valid in every triple-term position.
- The compiler frontend reads this projection back (`frontend.rs` reads `rdf:Statement` nodes), so the reader moves with the writer.
- Triple terms travel in `gmeow.gts`'s statement layer, not its base quads. A comment in `frontend.rs` gives that as the reason classic reification was chosen for one slice-authored obligation form. The rule encoding has to be carried through that layer losslessly.

## Requirement

Encode `logic:` rule atoms in RDF 1.2: triple terms reached by `rdf:reifies`, with variables as terms that are valid in every triple-term position. Round-trip byte-identically through the frontend, carry the encoding losslessly to `gmeow.gts`, and remove the RDF 1.1 reification form from the rule projection (greenfield, no dual encoding).


## Comments (0)

