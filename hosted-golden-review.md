# Independent complete hosted golden review

**Not ready to copy: two added predicate identities are incorrect.** No artifact/source edits, native catalog/corpus production, builds, tests or host probes were performed. Reviewed the complete JSON delta with read-only text/JSON inspection and reconciled added contexts with the physical owning PO/source stanzas.

## Exact production evidence

Stage metadata identifies successful run `39675042447`, job `119482584407`, at source `b3c70ca83adb8eba4f3b3dc38f08c6aedfdcabf1`. The unique nonexpired artifact is `12382960680`, named for that exact head. The job log confirms the existing Make target ran `gmeow-docs-model`'s native refresh example and reported 146 catalogs / 28,009 reviewed entries before upload. The downloaded JSON is 3,011,328 bytes. The artifact ZIP digest recorded by the forge is not represented as the digest of the extracted JSON file.

## Complete delta

The existing golden has 144 catalogs / 27,763 contexts. The artifact retains every old catalog and context, adds exactly two actual DSL catalog paths and 246 contexts, and removes none. Exactly eight catalog arrays change. Every artifact array is sorted and duplicate-free; there are no unrelated array-order changes.

| Physical family, paired FR/ZH catalogs | Additions per language | Complete reconciliation |
| --- | ---: | --- |
| `dsl/mappings/i18n` | 21 | `tPredFromCorrespondence`, `mappingDslSelectedPredicateFunctionality`, `catalogFamily-schema-value`: six fields each; `TemplateAtomPredRequiredConstraint`: label, definition and message. The message identity is the blocking defect below. |
| `slices/core/slice-quality-rubric/i18n` | 6 | Six existing English fields on `projVocab-schema-value`. |
| `slices/grounding/lang/i18n` | 18 | Six fields each on `literalValue` and `literalValueSchemaBridge`; three on `Glossary`, two on `glossarySlice`, one on `glossaryEntry`. |
| `slices/grounding/logic/i18n` | 78 | Six fields each on 13 terms: `CorrespondenceSource`, `groundingSourceOwner`, `groundingSourcePath`, `groundingSourceDigest`, `compiledCorrespondence`, `sourceMatchPredicate`, `hasCorrespondenceSource`, `ProjectionBindingProgram`, `ProjectionProfileProgram`, `hasProjectionBinding`, `hasProjectionProgram`, `projectionBindingContent`, `projectionProgramContent`. |

The 244 correctly identified additions match exact current English literals on their corresponding term/predicate stanzas. All added PO translations are nonempty and nonfuzzy. The other two PO entries contain the correct English message text but encode the predicate incorrectly. No foreign term addition or unexplained catalog is present in this delta.

The DSL catalogs belong to the existing mapping-DSL ontology, `https://blackcatinformatics.ca/gmeow/mapping-dsl`: its vocabulary declares the canonical ontology, and the relevant stanzas in `vocabulary.ttl`/`catalog-families.ttl` retain that `rdfs:isDefinedBy` owner. The explicit ontology-catalog inventory registers these physical inputs as the DSL ontology; it does not invent a slice or manifest. Existing 144 slice catalog/context sets remain intact.

## Blocking context defect

Both `dsl/mappings/i18n/{fr,zh}.po` key the `TemplateAtomPredRequiredConstraint` message as:

`https://blackcatinformatics.ca/gmeow/TemplateAtomPredRequiredConstraint|logic:message`

The actual canonical predicate is `https://blackcatinformatics.ca/logic/message`. Inspected `i18n_compile::expand_predicate`: only rdfs, skos, dct/dcterms and gmeow prefixes are expanded; other spellings are returned unchanged. Accordingly the native artifact retains the literal `logic:message` key. This key does not identify the canonical message predicate for native translation lookup/merge. Successful production and the expected context count do not establish a correct source pairing for these two entries.

Use the full canonical predicate IRI in the two authored PO contexts, preserving their text, then reproduce through a new exact-source hosted run. No expansion-API change or manual artifact patch is warranted. The corrected artifact should still have 146 catalogs / 28,009 contexts and 246 additions against the existing golden, with the two unsupported predicate spellings replaced by their canonical identities. Root was notified before copying; the current downloaded artifact remains unchanged.

The rest of this complete delta is source-reconciled, but the current artifact is rejected pending that repair and reproduction. This review does not claim full i18n quality, all parent metrics/floors, installed behavior or corpus acceptance.
