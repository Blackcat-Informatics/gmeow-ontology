# Issue #2800: [term] Assertion category on a claim: self-description, published evidence, derived interpretation

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, enhancement, foundational, logic, terms

## Body

## Request

A consumer publishing claim records as RDF 1.2 statement reifiers needs to say, on the claim itself, what kind of assertion it is. Three cases are needed:

1. self-description by the subject of the claim;
2. a statement resting on externally published evidence;
3. a derived interpretation (inferred or computed from other statements).

## What exists

`gmeow:hasEvidenceClass` with `gmeow:evidenceSELF`, `gmeow:evidenceSelfControlledSite`, `gmeow:evidenceOfficialSource`, `gmeow:evidenceIndependentTradePress` and `gmeow:sourceIndependence` (evidence slice) describe the warrant of a citation, on a `gmeow:CitationAct`. `gmeow:observationType gmeow:observationTypeDerived` (observations slice) marks an observation as derived. Nothing states the category for a claim that is a bare statement reifier.

## Required

- One way to state the assertion category on a statement reifier, grounded in the existing evidence and observation vocabulary; no second, competing category vocabulary. Either the existing properties are admitted on a reifier with their definitions revised, or one property is added whose values are related to the existing individuals by axioms.
- The three cases above are each expressible, and the relation between a claim's category and the evidence classes of the citations it rests on is stated (they may differ: a self-description can cite an official source).
- A worked example with one claim of each category, a competency question that returns claims by category, and translations.
- The category never ranks claims: co-equal claims of different categories stay co-equal (Principle 9).

Principles 2, 4, 9.


## Comments (1)

### paudley — 2026-11-15T01:49:51Z

Blocks: #2795 (Release 0.4.0), by the maintainer's decision.

