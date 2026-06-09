<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Archaeological & Cultural-Heritage Evidence — modelling & interoperability guide

Archaeological objects are **evidence carriers**, not language facts by themselves.
The physical carrier, the visible marks, the observation/documentation event, the
reading, the transliteration, the translation, the dating evidence, the
stratigraphic / find context, and the linguistic interpretation must remain
**separable claims** (Principle 9).

This guide explains the language-facing hooks that connect GMEOW's lexicon,
attestation, and etymology layers to archaeological and cultural-heritage evidence.
General archaeological domain modeling (excavation units, stratigraphic interfaces,
artifact typology) is deferred to #129.

---

## The layer stack

| Layer | GMEOW construct | Example |
|-------|-----------------|---------|
| **Physical carrier** | `gmeow:PhysicalObject` + `gmeow:carrierType` | A clay tablet, an ostracon, a seal impression |
| **Sign-bearing feature** | `gmeow:Inscription` | The cuneiform text on the tablet |
| **Documentation event** | `gmeow:Observation` (universal stack) | The excavation, photography, or museum cataloguing |
| **Reading** | `gmeow:InscriptionReading` → `gmeow:LexicalForm` | "𒀭" read as the sign AN |
| **Transliteration** | `gmeow:InscriptionTransliteration` → `gmeow:LexicalForm` | "an" in Latin script |
| **Translation** | `gmeow:InscriptionTranslation` → `gmeow:LexicalForm` | "god" in English |
| **Dating evidence** | `gmeow:TemporalMeasurement` | Stratigraphic correlation, 3200 ± 100 BP |
| **Find context** | `gmeow:ArchaeologicalFindContext` | Trench 3, Layer 7, Uruk site |
| **Linguistic interpretation** | `gmeow:ScriptLanguageAttribution` | Sumerian language, cuneiform script |

Each layer is a **distinct node** in the graph. No single layer asserts truth for
another. A `PhysicalObject` does not "have a language"; an `Inscription` does not
"have a date"; a `Reading` does not "have a find-spot". These are all mediated by
reified `Observation` subclasses that carry `gmeow:vantage`, `gmeow:confidence`,
`gmeow:hasDeterminacy`, and `gmeow:accordingTo`.

---

## Critical anti-pattern

**Never assert "this object is in language X" as unqualified truth when the
evidence is actually "scholar/team Y reads this mark as X, with confidence Z, in
context C."**

Bad (flat, truth-asserting):

```turtle
# WRONG — collapses carrier, inscription, and interpretation into one claim
ex:tablet1 ex:claimedLanguage ex:sumerian .
```

Good (layered, standpoint-scoped):

```turtle
ex:tablet1 a gmeow:PhysicalObject ;
    gmeow:carrierType gmeow:carrierTablet .

ex:inscription1 a gmeow:Inscription ;
    gmeow:inscriptionCarrier ex:tablet1 .

ex:attribution1 a gmeow:ScriptLanguageAttribution ;
    gmeow:attributionTarget ex:inscription1 ;
    gmeow:attributedLanguage ex:sumerian ;
    gmeow:attributedScript ex:cuneiform ;
    gmeow:vantage ex:scholarA ;
    gmeow:confidence 0.85 ;
    gmeow:hasDeterminacy gmeow:determinacyCrisp .
```

The first form is a **category error**: a physical object does not have a language.
The second form keeps every claim **attributed, dated, and confidence-weighted**
(Principle 9).

---

## Competing claims coexist

Two scholars may read the same inscription differently, assign it to different
languages, or date it to different periods. In GMEOW, these are **co-existing
observations**, not edit-war candidates.

```turtle
ex:readingA a gmeow:InscriptionReading ;
    gmeow:readingOf ex:inscription1 ;
    gmeow:readingResult ex:formA ;
    gmeow:vantage ex:scholarA ;
    gmeow:confidence 0.75 .

ex:readingB a gmeow:InscriptionReading ;
    gmeow:readingOf ex:inscription1 ;
    gmeow:readingResult ex:formB ;
    gmeow:vantage ex:scholarB ;
    gmeow:confidence 0.60 .
```

Both readings are retained. Projection layers may **select** one for a specific
consumer, but the canonical source never erases the other (Principle 10:
suppression via `gmeow:displayable false`, never deletion).

---

## Undeciphered and uncertain cases

An inscription in an undeciphered script (Linear A, Rongorongo, proto-cuneiform)
carries a `gmeow:ScriptLanguageAttribution` with `gmeow:attributedNotation` but
no `gmeow:attributedLanguage` or `gmeow:attributedScript`. Its determinacy is
`gmeow:determinacyDisputed` and its confidence is low.

```turtle
ex:linearA a gmeow:NotationSystem .

ex:undecipheredAttribution a gmeow:ScriptLanguageAttribution ;
    gmeow:attributionTarget ex:inscription2 ;
    gmeow:attributedNotation ex:linearA ;
    gmeow:hasDeterminacy gmeow:determinacyDisputed ;
    gmeow:vantage ex:scholarC ;
    gmeow:confidence 0.40 .
```

There is **no canonical winner** and no requirement to assign a language. The
notation system stands on its own as a first-class `gmeow:NotationSystem`.

---

## Hooks into the lexicon layer

### UsageAttestation → PhysicalObject

The `gmeow:attestedOnCarrier` property links a `gmeow:UsageAttestation` to a
`gmeow:PhysicalObject`, creating the bridge from lexical evidence to
archaeological evidence.

```turtle
ex:attestation a gmeow:UsageAttestation ;
    gmeow:attestedForm ex:lexicalForm ;
    gmeow:attestedInSource ex:publishedEdition ;
    gmeow:attestedOnCarrier ex:tablet1 ;
    gmeow:confidence 0.90 .
```

### EtymologicalDerivation → inscription evidence

An etymological claim may cite a `gmeow:UsageAttestation` (or the
`gmeow:InscriptionReading` directly) as `gmeow:derivationEvidence`.

```turtle
ex:derivation a gmeow:EtymologicalDerivation ;
    gmeow:derivationSource ex:ancestorWord ;
    gmeow:derivationTarget ex:descendantWord ;
    gmeow:derivationKind gmeow:derivationBorrowing ;
    gmeow:derivationEvidence ex:attestation ;
    gmeow:confidence 0.65 .
```

### LanguageState → inscription

A `gmeow:LanguageState` may describe the language of an inscription at a
particular historical moment, reusing the existing language-state machinery.

---

## Projection lossy drops

When projecting the archaeological evidence stack to surface vocabularies,
the following information is **deliberately dropped** (Principle 4):

| Target vocabulary | What is lost |
|-------------------|--------------|
| **CIDOC-CRM** | Standpoint, confidence, determinacy, and competing claims collapse to a single E13 Attribute Assignment or E34 Inscription. The `gmeow:Inscription` → E34 mapping loses the separation between InformationObject and PhysicalObject when the consumer does not distinguish them. |
| **CRMarchaeo** | Stratigraphic detail below the A2 level is dropped; excavation events lose their observer standpoints. |
| **CRMsci** | S4 Observation loses the reified relator structure; the result is flattened to `s15:has_result`. |
| **CRMinf** | I1 Argumentation preserves the dispute structure, but the temporal scope and find context are lost. I2 Belief carries the confidence, but not the stratigraphic provenance. |
| **PROV-O** | `prov:Activity` flattens the Observation relator; `prov:Entity` for the inscription loses its carrier link. The standpoint becomes `prov:wasAssociatedWith` without the modal force. |
| **Web Annotation** | `oa:Annotation` preserves target + body, but drops confidence, determinacy, dating, find context, and competing claims (only one annotation per target→body pair). |
| **AO-Cat / ARIADNE** | Archaeological object typology is preserved, but language/script attribution, transliteration, and translation are out of scope for these catalogues. |
| **schema.org** | No suitable target for inscription, reading, or transliteration. The closest is `schema:CreativeWork` for the published edition, which loses all archaeological context. |

---

## Alignment summary

| GMEOW term | Closest external term | Confidence | Note |
|------------|----------------------|------------|------|
| `gmeow:Inscription` | `crm:E34_Inscription` | 0.95 | GMEOW is broader (marks, symbols, non-linguistic signs) |
| `gmeow:PhysicalObject` | `crm:E19_Physical_Object` | 0.90 | E24 is narrower (human-made only) |
| `gmeow:inscriptionCarrier` | `crm:P128_carries` | 0.90 | Inverse: `crm:P128i_is_carried_by` |
| `gmeow:InscriptionReading` | `crmsci:S4_Observation` | 0.85 | Also close to `crm:E13_Attribute_Assignment` |
| `gmeow:ScriptLanguageAttribution` | `crminf:I1_Argumentation` | 0.90 | I5 Inference Making when inferred from evidence |
| `gmeow:ArchaeologicalFindContext` | `crmarc:A2_Stratigraphic_Volume_Unit` | 0.70 | Loose: A2 is spatial, FindContext is a claim |
| `gmeow:InscriptionReading` | `prov:Activity` | 0.85 | Reading generates `prov:Entity` (LexicalForm) |
| `gmeow:InscriptionReading` | `oa:Annotation` | 0.75 | Lossy: drops confidence, determinacy, context |

---

## Tests

- `tests/test_archaeological_evidence.py` — structural and DL-safety guards.
- `tests/fixtures/shapes/archaeological-evidence.ttl` — SHACL-valid instance data.
- `tests/fixtures/coverage/archaeological-evidence.ttl` — competency-test coverage data.
