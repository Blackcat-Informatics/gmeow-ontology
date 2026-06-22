# GMEOW Lexicon Mapping

> **Mapping target:** OntoLex-Lemon, LIME, Lexicog, Morph, FrAC, SKOS/SKOS-XL, PROV-O, Web Annotation, CRMinf

GMEOW's lexicon module provides first-class modeling for lexical items, concrete forms, usage attestations, and etymological derivations. It builds on the language-state/variety layer and the universal observation stack (standpoint facility).

## LexicalItem & LexicalForm

A `LexicalItem` is the abstract lexical or constructional object — a word, morpheme, phrase, idiom, symbol, sign, or construction. It carries no surface form itself; forms are linked via `hasLexicalForm`.

A `LexicalForm` is a concrete manifestation: written, spoken, signed, rendered, reconstructed, normalized, transliterated, or translated. The surface representation is `formRepresentation`; the kind is a `LexicalFormType` value.

```turtle
ex:cat a gmeow:LexicalItem ;
    gmeow:lexicalItemLanguage ex:english ;
    gmeow:hasLexicalForm ex:catWritten, ex:catSpoken, ex:catReconstructed .

ex:catWritten a gmeow:LexicalForm ;
    gmeow:formRepresentation "cat" ;
    gmeow:formType gmeow:formWritten .

ex:catSpoken a gmeow:LexicalForm ;
    gmeow:formRepresentation "/kæt/" ;
    gmeow:formType gmeow:formSpoken .

ex:catReconstructed a gmeow:LexicalForm ;
    gmeow:formRepresentation "*kattōn" ;
    gmeow:formType gmeow:formReconstructed .
```

A single lexical item may have many coexisting forms. None is privileged (Principle 9). Polyglot items are modeled as separate `LexicalItem` instances linked by translation, each with its own `lexicalItemLanguage`.

### OntoLex alignment

- `LexicalItem` aligns to `ontolex:LexicalEntry` by reference.
- `LexicalForm` aligns to `ontolex:Form` by reference.
- `formRepresentation` aligns to `ontolex:writtenRep` for written forms (lossy for spoken/reconstructed).
- `hasLexicalForm` aligns to `ontolex:lexicalForm`.

The projection layer handles the directional mapping; the ontology module does not import OntoLex axioms (Principle 5).

## UsageAttestation — evidence, not truth

> **Attestation records that a form was observed; it does not assert that a proposed interpretation is correct.**

`UsageAttestation` is an `Observation + Relator` that records evidence: a form was seen in a source, a corpus, an inscription, a platform, or a community. It is evidence, not truth (Principle 12). Interpretation — reading, translation, etymology — lives in the claim layer, not the evidence layer.

```turtle
ex:yeetAttestation a gmeow:UsageAttestation ;
    gmeow:attestedForm ex:yeetSpoken ;
    gmeow:attestedInLanguage ex:english ;
    gmeow:attestedInSource ex:twitterCorpus2020 ;
    gmeow:attestedInContext ex:genZCommunity ;
    gmeow:attestationInterval ex:yeetInterval ;
    gmeow:confidence 0.95 .
```

Because `UsageAttestation` is an `Observation`, it inherits the universal claim stack: `vantage` (who asserts the attestation), `observedFeature` (the attested form, via `attestedForm ⊑ observedFeature`), `confidence`, `validFrom`/`validUntil`, and `assertedAt`. Temporal interpretation and evidence evaluation live in the SPARQL/query layer, not the reasoner (Principle 12).

## Etymology as a claim graph

> **Etymology is not a flat origin string. It is a graph of provenance-rich, standpointed claims.**

`EtymologicalDerivation` is an `Observation + Relator` linking a source lexical item/form to a target lexical item/form, carrying:

- `derivationKind` — borrowing, calque, semantic shift, sound change, compounding, etc.
- `derivationEvidence` — supporting `UsageAttestation` or `Source` nodes
- `confidence`, `accordingTo`, `validFrom`/`validUntil` — inherited from `Observation`

Multiple derivations for the same target coexist without privilege (Principle 9). A superseded derivation is suppressed with `displayable false`, never erased (Principle 10).

```turtle
ex:derivationAlgebraBorrowing a gmeow:EtymologicalDerivation ;
    gmeow:etymonSource ex:alJabr ;
    gmeow:derivationTarget ex:algebra ;
    gmeow:derivationKind gmeow:derivationBorrowing ;
    gmeow:confidence 0.85 .

ex:ax-derivation-borrowing a owl:Axiom ;
    owl:annotatedSource   ex:derivationAlgebraBorrowing ;
    owl:annotatedProperty gmeow:derivationKind ;
    owl:annotatedTarget   gmeow:derivationBorrowing ;
    gmeow:accordingTo ex:standpoint-etymologist-a ;
    gmeow:confidence 0.85 ;
    gmeow:validFrom "0800-01-01T00:00:00Z"^^xsd:dateTime .
```

## Reconstructed proto-forms

A reconstructed form (e.g. PIE *wódr̥) is a `LexicalForm` with `formType gmeow:formReconstructed`. Its status as a reconstruction — not a universally accepted truth — is recorded via standpoint-indexed `owl:Axiom` reifiers:

```turtle
ex:pieWaterForm a gmeow:LexicalForm ;
    gmeow:formRepresentation "*wódr̥" ;
    gmeow:formType gmeow:formReconstructed .

ex:ax-reconstruction-claim a owl:Axiom ;
    owl:annotatedSource   ex:pieWaterForm ;
    owl:annotatedProperty gmeow:formType ;
    owl:annotatedTarget   gmeow:formReconstructed ;
    gmeow:accordingTo ex:standpoint-linguist-reconstructionist ;
    gmeow:confidence 0.70 ;
    gmeow:standpointModality gmeow:probable .
```

The reconstruction is a standpointed claim with modality `probable`, not an axiom of the universal standpoint.

## One attestation, multiple readings

A single `UsageAttestation` (an oracle bone inscription) can support two competing `LexicalForm` readings. Each reading is a separate standpointed claim via `wasDerivedFrom` + `owl:Axiom` reifier:

```turtle
ex:readingA a gmeow:LexicalForm ;
    gmeow:formRepresentation "reading-A (sun)" ;
    gmeow:formType gmeow:formNormalized ;
    gmeow:wasDerivedFrom ex:oracleBoneInscription .

ex:ax-reading-a a owl:Axiom ;
    owl:annotatedSource   ex:readingA ;
    owl:annotatedProperty gmeow:wasDerivedFrom ;
    owl:annotatedTarget   ex:oracleBoneInscription ;
    gmeow:accordingTo ex:standpoint-epigrapher-a ;
    gmeow:confidence 0.80 .
```

The attestation is evidence; the reading is interpretation. The separation is structural, not merely conventional (Principle 12).

## Projection roadmap

| GMEOW term | Target vocabulary | Status |
|---|---|---|
| `LexicalItem` | `ontolex:LexicalEntry` | Implemented |
| `LexicalForm` | `ontolex:Form` | Implemented |
| `formRepresentation` | `ontolex:writtenRep` | Implemented (lossy for non-written) |
| `hasLexicalForm` | `ontolex:lexicalForm` | Implemented |
| `lexicalItemLanguage` | `lime:language` | Implemented |
| `UsageAttestation` | `prov:Entity` / `oa:Annotation` | Staged |
| `EtymologicalDerivation` | `prov:Derivation` / CRMinf | Staged |
| `LexicalFormType` | `skos:Concept` | Staged |
| `DerivationKind` | `skos:Concept` | Staged |
| Frequency data | `frac:Frequency` | Staged |
| Morphological analysis | `morph:Morph` | Staged |
| Lexicographic resources | `lexicog:LexicographicResource` | Staged |

Full Lexicog, Morph, FrAC, SKOS-XL, Web Annotation, and CRMinf projections are documented but staged for future work.

## Terms

### gmeow:LexicalItem · gmeow:lexicalItemLanguage · gmeow:hasLexicalForm

A `LexicalItem` is the abstract lexical or constructional object — word, morpheme,
phrase, idiom, symbol, sign, or construction — carrying no surface form itself.
`lexicalItemLanguage` functionally fixes its language (mint a distinct item per
language); `hasLexicalForm` links its concrete forms.

### gmeow:LexicalForm · gmeow:formOf · gmeow:formRepresentation · gmeow:formType · gmeow:LexicalFormType · gmeow:formTransliterationScheme

A `LexicalForm` is a concrete manifestation of an item — `formOf` is the inverse of
`hasLexicalForm`. `formRepresentation` is the functional surface string;
`formType` draws the open `LexicalFormType` vocabulary (written, spoken, signed,
rendered, reconstructed, normalized, transliterated, translated); a transliterated
form names its `formTransliterationScheme`. Many forms coexist, none privileged.

### gmeow:UsageAttestation · gmeow:attestedForm · gmeow:attestedInLanguage · gmeow:attestedInSource · gmeow:attestedInContext · gmeow:attestedOnCarrier · gmeow:attestationInterval

An `Observation + Relator` recording evidence — a form was seen in a source,
corpus, inscription, platform, or community — not truth (Principle 12). The
`attested*` properties bind the form, language, source, context, and physical
carrier; `attestationInterval` carries the period. `attestedForm ⊑ observedFeature`,
so it inherits the universal claim stack (vantage, confidence, validity).

### gmeow:EtymologicalDerivation · gmeow:etymonSource · gmeow:derivationTarget · gmeow:derivationKind · gmeow:DerivationKind · gmeow:derivationEvidence

An `Observation + Relator` linking a source lexical item/form to a target, making
etymology a graph of provenance-rich, standpointed claims rather than a flat origin
string. `derivationKind` draws the open `DerivationKind` vocabulary (borrowing,
calque, inheritance, semantic shift, sound change, compounding, affixation,
clipping, back-formation, reanalysis, folk etymology, spelling change,
reconstruction, unknown origin); `derivationEvidence` cites supporting attestations
or sources. Competing derivations coexist; superseded ones are `displayable false`.
