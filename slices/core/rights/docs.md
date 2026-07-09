<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rights — alignment & projection reference

The alignment and projection companion for the rights slice.
Everything here is **authored once** in the rights slice (`module.ttl`,
`mappings/`, examples, tests, and competency queries) and consumed by the
registered generators (Principle 4). Generated SSSOM, EDOAL, FnO, SPARQL,
SHACL/ShEx, OWL, and compatibility views are projections of the canonical source;
they are not competing authoring surfaces.

## Terms

| Term | Kind | Role |
|---|---|---|
| `gmeow:RightsStatement` | class (`gufo:Relator`) | the machine-readable rights situation of an asset (the ODRL Policy idea) |
| `gmeow:Copyright` | class (`gufo:Relator`) | copyright (work × holder × year × notice × status) |
| `gmeow:License` | class (`gufo:SubKind` ⊑ `gmeow:Agreement`) | a licence as an agreement |
| `gmeow:Trademark` / `gmeow:Mark` | class (`gufo:Relator`) / class (⊑ `gmeow:InformationObject`) | the trademark right / the mark it protects |
| `gmeow:Rule` → `gmeow:Permission` / `gmeow:Prohibition` / `gmeow:Duty` | classes (`gufo:Relator`) | the ODRL deontic rule trio |
| `gmeow:RightsAction` | value vocabulary (`gufo:QualityValue`) | the regulated action (reproduce, distribute, derive, …) |
| `gmeow:LicenseFamily` / `gmeow:TrademarkStatus` / `gmeow:CopyrightStatus` | value vocabularies | open status / family values |
| `gmeow:hasLicense` / `gmeow:hasCopyright` / `gmeow:hasTrademark` / `gmeow:hasRightsStatement` | object properties | flat-first attach points |
| `gmeow:copyrightHolder` / `gmeow:trademarkHolder` | object properties (⊑ `gmeow:wasAttributedTo`) | holder attribution |
| `gmeow:licensor` / `gmeow:licensee` | object properties (⊑ `gmeow:hasParty`) | licence parties |
| `gmeow:spdxLicenseId` | datatype property | the SPDX License List short identifier |
| `gmeow:attributionText` | datatype property | the credit line (schema:creditText / cc:attributionName) |

## Term anchors

### gmeow:RightsStatement · gmeow:statementAbout · gmeow:hasRightsStatement · gmeow:rightsType · gmeow:RightsType

The machine-readable rights situation of an asset (the ODRL Policy idea): a
`RightsStatement` `statementAbout` the asset it governs (the ODRL target),
attached flat by `hasRightsStatement`. `rightsType` names the kind(s) of IP right
it concerns — `RightsType` being an open value vocabulary (copyright, trademark,
patent, trade secret, moral rights, database right, …) linked to Wikidata/WIPO.

### gmeow:Copyright · gmeow:hasCopyright · gmeow:copyrightWork · gmeow:copyrightHolder · gmeow:copyrightYear · gmeow:copyrightNotice · gmeow:copyrightStatus · gmeow:CopyrightStatus

The copyright relator and its posts — the protected `copyrightWork`, the
`copyrightHolder` (⊑ `wasAttributedTo`), the `copyrightYear`, the free-text
`copyrightNotice`, and the `copyrightStatus` (a `CopyrightStatus` value:
in-copyright, public-domain, …) — attached flat by `hasCopyright`.

### gmeow:License · gmeow:LicenseFamily · gmeow:hasLicense · gmeow:licensedWork · gmeow:licensor · gmeow:licensee · gmeow:licenseFamily · gmeow:licenseText · gmeow:spdxLicenseId · gmeow:spdxLicenseName · gmeow:isOsiApproved

A licence as an `Agreement` (`SubKind`): its `licensedWork`, its `licensor` and
`licensee` parties (⊑ `hasParty`), its `licenseFamily` (a `LicenseFamily` value —
CC, copyleft, permissive, proprietary), and the SPDX handles
(`spdxLicenseId` / `spdxLicenseName` / `licenseText` / `isOsiApproved`). Attached
flat by `hasLicense`.

### gmeow:Trademark · gmeow:Mark · gmeow:hasTrademark · gmeow:trademarkHolder · gmeow:trademarkMark · gmeow:trademarkStatus · gmeow:TrademarkStatus · gmeow:markText · gmeow:registrationNumber

The trademark right (`Trademark`, a relator) over the `Mark` it protects (an
`InformationObject`, with `markText` and `registrationNumber`): `trademarkHolder`
the holder (⊑ `wasAttributedTo`), `trademarkMark` the protected mark, and
`trademarkStatus` (a `TrademarkStatus` value) the registration state. Attached
flat by `hasTrademark`.

### gmeow:Rule · gmeow:Permission · gmeow:Prohibition · gmeow:Duty · gmeow:hasPermission · gmeow:hasProhibition · gmeow:hasDuty · gmeow:ruleAction · gmeow:RightsAction · gmeow:ruleTarget · gmeow:ruleAssignee · gmeow:ruleConstraint · gmeow:ruleConsequence

The ODRL deontic rule trio — `Permission`, `Prohibition`, `Duty` (relators
specialising `Rule`) — carried by `hasPermission` / `hasProhibition` / `hasDuty`.
Each rule names exactly one `ruleAction` (a `RightsAction` value: reproduce,
distribute, derive, …), its `ruleTarget` asset, its `ruleAssignee`, any
`ruleConstraint`, and a duty's `ruleConsequence` remedy chain.

### gmeow:Constraint · gmeow:AtomicConstraint · gmeow:LeftOperand · gmeow:ConstraintOperator · gmeow:leftOperand · gmeow:constraintOperator · gmeow:rightOperand · gmeow:rightOperandReference

The ODRL constraint algebra: an `AtomicConstraint` is one comparison — a
`leftOperand` (the dimension tested, a `LeftOperand` value: dateTime, spatial,
count, purpose, …), a `constraintOperator` (a `ConstraintOperator` value: eq, lt,
lteq, …), and a `rightOperand` literal or `rightOperandReference` resource. A
licence's temporal bound is an `AtomicConstraint` over the dateTime operand.

### gmeow:LogicalConstraint · gmeow:ConstraintLogic · gmeow:constraintLogic · gmeow:logicConstraintMember · gmeow:ConflictStrategy · gmeow:conflictStrategy

A `LogicalConstraint` is a boolean combination of constraints under a
`constraintLogic` operator (a `ConstraintLogic` value: and / or / xone /
andSequence) over its `logicConstraintMember` constraints. `conflictStrategy` (a
`ConflictStrategy` value: perm / prohibit / invalid) resolves policy conflicts.

### gmeow:PrivacyNotice · gmeow:hasPrivacyNotice · gmeow:hasDataController · gmeow:hasDataSubject

The privacy layer: a `PrivacyNotice` (the dpv/schema PrivacyPolicy counterpart)
attached domain-free by `hasPrivacyNotice`; `hasDataController` the agent
determining processing purposes and means; `hasDataSubject` the individual whose
personal data is governed.

### gmeow:attributionText · gmeow:attributionUrl · gmeow:acquireLicensePage · gmeow:usageInfo · gmeow:conditionsOfAccess · gmeow:isAccessibleForFree

The schema.org / CC surface fields a rights statement carries: the credit line
(`attributionText` = schema:creditText / cc:attributionName) and link
(`attributionUrl`), the licence-acquisition page (`acquireLicensePage`),
free-text `usageInfo` and `conditionsOfAccess`, and the
`isAccessibleForFree` flag.

## SSSOM alignments (`slices/core/rights/mappings/equivalences.ttl`)

All by reference (Principle 5) — GMEOW never imports an external axiom. The
registered mappings generator lowers the slice-local records to
`mappings/gmeow-rights.sssom.tsv`. Wikidata rights links live beside the owning
slice mappings and lower to `gmeow-wikidata.sssom.tsv`; QID/PID syntax is gated
offline, with live checks reserved for maintainer refresh lanes.

| GMEOW | predicate | external target |
|---|---|---|
| `gmeow:RightsStatement` | closeMatch / relatedMatch | `odrl:Policy`, `odrl:Set`, `dcterms:RightsStatement`, `premis:RightsStatement` |
| `gmeow:hasRightsStatement` | closeMatch | `odrl:hasPolicy` |
| `gmeow:Permission` / `Prohibition` / `Duty` | closeMatch | `odrl:Permission` / `odrl:Prohibition` / `odrl:Duty` |
| `gmeow:hasPermission` / `hasProhibition` / `hasDuty` | closeMatch / relatedMatch | `odrl:permission` / `odrl:prohibition` / `odrl:obligation`; `cc:permits` / `cc:prohibits` / `cc:requires` |
| `gmeow:ruleAction` | closeMatch | `odrl:action` |
| `gmeow:statementAbout` / `gmeow:ruleTarget` | closeMatch | `odrl:target` |
| `gmeow:ruleAssignee` / `gmeow:licensee` | closeMatch | `odrl:assignee` |
| `gmeow:licensor` | closeMatch | `odrl:assigner` |
| `gmeow:License` | closeMatch | `odrl:Offer`, `cc:License`, `dcterms:LicenseDocument`, `spdx:License`, `wd:Q79719` |
| `gmeow:hasLicense` | exactMatch / closeMatch / relatedMatch | `dcterms:license`, `schema:license`, `cc:license`, `spdx:licenseDeclared` / `spdx:licenseConcluded`, `wd:P275` |
| `gmeow:spdxLicenseId` | closeMatch | `spdx:licenseId` |
| `gmeow:copyrightHolder` | exactMatch / closeMatch | `dcterms:rightsHolder`, `schema:copyrightHolder`, `wd:P3931` |
| `gmeow:copyrightYear` / `gmeow:copyrightNotice` | exactMatch | `schema:copyrightYear` / `schema:copyrightNotice` |
| `gmeow:attributionText` | closeMatch | `schema:creditText`, `cc:attributionName` |
| `gmeow:Copyright` / `gmeow:Trademark` / `gmeow:Mark` | closeMatch | `wd:Q1297822` / `wd:Q167270` / `wd:Q431289`; `gmeow:Mark` closeMatch `schema:Brand` |
| `gmeow:copyrightStatus` values | closeMatch | `<rightsstatements.org/vocab/InC,NKC,CNE>`, CC public-domain mark, `wd:Q19652` |
| `gmeow:licenseFamily*` values | closeMatch | `wd:Q284742` (CC), `wd:Q1139274` (copyleft), `wd:Q3238057` (proprietary), `wd:Q1437937` (permissive) |
| `gmeow:RightsAction` values | exactMatch / closeMatch | `odrl:reproduce` / `distribute` / `derive` / `commercialize` / `present` / `use` / `extract` / `attribute` / `shareAlike` / `obtainConsent`; `cc:Reproduction` / `Distribution` / `DerivativeWorks` / `CommercialUse` / `Attribution` / `ShareAlike` / `Notice` |

**WIPO** trademark concepts are reached via Wikidata (`wd:Q167270` trademark,
`wd:P1716` brand) rather than a fabricated `wipo:` namespace IRI.

### Standards bridged (the full target set)

- **ODRL 2.2** — the deontic policy model: the Policy/Set/Offer, the Permission/
  Prohibition/Duty rule trio, the **complete action vocabulary** (≈47 actions), the
  **constraint algebra** (≈34 left operands, 12 operators), the **logical-constraint**
  operators (and/or/xone/andSequence), and the **conflict-resolution** strategy.
- **CC REL** — `cc:License`, `cc:license`, `permits`/`requires`/`prohibits`, the
  permission/requirement/prohibition value classes, `cc:attributionName` /
  `cc:attributionURL` / `cc:morePermissions` / `cc:legalcode` / `cc:Work`.
- **Dublin Core** — `dcterms:license`, `LicenseDocument`, `rightsHolder`, `rights`,
  `accessRights`, `dateCopyrighted`, and `dc:rights` (elements 1.1).
- **schema.org** — `copyrightHolder` / `copyrightYear` / `copyrightNotice` /
  `license` / `creditText` / `acquireLicensePage` / `usageInfo` /
  `isAccessibleForFree` / `conditionsOfAccess` / `Brand` /
  `DigitalDocumentPermission`.
- **SPDX** — `spdx:License` / `ListedLicense` / `licenseId` / `name` / `licenseText`
  / `isOsiApproved` / `licenseDeclared` / `licenseConcluded`.
- **RightsStatements.org** — all **twelve** standardized statements (InC, InC-OW-EU,
  InC-EDU, InC-NC, InC-RUU, NoC-CR, NoC-NC, NoC-OKLR, NoC-US, NKC, CNE, UND) +
  the CC public-domain mark.
- **PREMIS 3** — `premis:Copyright` / `License` / `RightsStatus` / `act` / `allows`
  / `restriction` (the preservation rights basis; IRIs verified against the LOC
  ontology — PREMIS 3 has no `RightsStatement` class).
- **W3C Ontology for Media Resources** (`ma-ont`) — `ma:copyright` /
  `isCopyrightedBy` / `hasPolicy` / `hasPermissions`.
- **Wikidata / WIPO** — copyright, trademark, licence, the licence families, and the
  IP-right types (patent, industrial design, trade secret, related rights, moral
  rights, database right, plant breeders' rights), plus the Rights Expression
  Language (`wd:Q3935748`) and **MPEG-21** (`wd:Q930582`) standards. Every QID/PID
  is curl-validated against the live Wikidata API.

**IPROnto** (the UPF Intellectual Property Rights Ontology) and the **MPEG-21 / ISO/IEC
21000** REL/RDD are conceptual antecedents of this facility. IPROnto has no maintained
dereferenceable namespace and no Wikidata item, and MPEG-21 REL is XML-schema-based
with no canonical RDF namespace; GMEOW therefore bridges that IPR-ontology / REL lineage
**by reference** through ODRL, the Rights Expression Language Wikidata item, and the
MPEG-21 Wikidata item rather than fabricating IRIs (Principle 5; verify-don't-fabricate).

## Generated projections

Lossy, directional consumable views (Principle 4) — never the canonical model.
The rights slice owns the ODRL, CC REL, SPDX, Dublin Core, and schema.org rights
projection cells under `slices/core/rights/mappings/`. Each cell carries both
the executable projection binding and, where the source/target term relation is
clear, future-facing SSSOM routing fields (`emitSssom`, `sssomPredicate`,
`sssomFile`) so linkage and projection stay one authored unit as the
`logic:Correspondence` lowering becomes the single source of SHACL/ShEx/OWL and
other compatibility views.

| Profile | File | What it emits |
|---|---|---|
| **ODRL** | `queries/projections/odrl.rq` | `gmeow:RightsStatement` → `odrl:Set` with `odrl:permission` / `odrl:prohibition` / `odrl:obligation` rules (each `odrl:action` / `odrl:target` / `odrl:assignee`); `gmeow:License` → `odrl:Offer` + `odrl:assigner`. Action IRIs are the GMEOW values, `exactMatch` the ODRL action vocabulary in the SSSOM layer. |
| **CC REL** | `queries/projections/cc.rq` | `gmeow:hasLicense` → `cc:license`; `gmeow:License` → `cc:License`; `gmeow:attributionText` → `cc:attributionName`. CC's `permits`/`requires`/`prohibits` describe CC's own licence resources, linked by reference; the full deontic detail goes to ODRL. |
| **schema.org** | `queries/projections/schema-org.rq` | the Copyright relator flattens to `schema:copyrightHolder` / `copyrightYear` / `copyrightNotice`; `gmeow:hasLicense` → `schema:license`; `gmeow:attributionText` → `schema:creditText`; `gmeow:Mark` → `schema:Brand`; plus `schema:isAccessibleForFree` / `acquireLicensePage` / `usageInfo`. |
| **Dublin Core** | `queries/projections/dcterms.rq` | `gmeow:hasLicense` → `dcterms:license`; the Copyright relator flattens to `dcterms:rightsHolder` + a free-text `dcterms:rights` notice + `dcterms:dateCopyrighted` — the flat DC / DCAT rights view GMEOW already states about *itself* on the root ontology, generated for arbitrary instances. |
| **SPDX** | `queries/projections/spdx.rq` | `gmeow:License` → `spdx:License` with `spdx:licenseId` / `spdx:name` / `spdx:licenseText` / `spdx:isOsiApproved` — the SBOM / package-manager identifier view. |

The ODRL projection also emits the full **constraint algebra** (`odrl:constraint` →
`odrl:Constraint` with `odrl:leftOperand` / `odrl:operator` / `odrl:rightOperand`),
the **conflict-resolution strategy** (`odrl:conflict`), duty **consequence /
remedy** chaining (`odrl:consequence`), and `odrl:Asset` / `odrl:Party` typing —
the ODRL *deontic logic*, not just its structure (Principle 1). A licence's temporal
bound is modelled as an `odrl:dateTime` constraint; lightweight statement validity
rides `gmeow:validFrom` / `gmeow:validUntil` (the temporal module), and rights
provenance/confidence/standpoint ride the RDF-1.2 statement layer
(`dsl/statements/rights.ttl`).

The structural ODRL / CC `templateAtoms` cells carry no EDOAL cell of their own, so
the no-drift gate (`projection_spec_drift`) verifies every emitted `odrl:` / `cc:`
term against the SSSOM linkage layer — the two layers validate each other.

## Validation projections

The validation surface is a compatibility projection, not the canonical rights
model. Rights constraints belong in the slice's canonical ontology and
`logic:Correspondence`/law-bearing records; SHACL and ShEx are generated views
used to serve downstream validators. The rights contracts that must survive
those projections are:

| Contract | Severity | Enforces |
|---|---|---|
| Rights statement target | Violation | exactly one `gmeow:statementAbout` for a rights statement |
| Rule action | Violation | exactly one `gmeow:ruleAction` per rule |
| Copyright posts | Violation | one `copyrightWork` plus at least one `copyrightHolder` |
| Licence party | Violation | at least one `gmeow:licensor` |
| Trademark posts | Violation | one `trademarkMark` plus at least one `trademarkHolder` |
| Expired mark suppression | Warning | expired or cancelled trademark sets `gmeow:displayable false` (Principle 10) |
| Co-equal rights claims | Violation | no preferred or primary right (Principle 9) |
