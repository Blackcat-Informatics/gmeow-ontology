<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — The Projection Contract

> The **projection charter** of the GMEOW Language design set: the generated lossy lowerings from
> the canonical `lang:` model to the external linguistic ecosystems, each carrying a preservation
> judgment in the loss ledger. Every artifact named here is a **projection of `gmeow.gts`** under
> the pipeline-spine doctrine — generated from the canon, gated for drift, and never a second
> source of truth. The canonical structures being lowered are defined in
> [`LANG-FORMS.md`](LANG-FORMS.md), [`LANG-MEANING.md`](LANG-MEANING.md), and
> [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md); the gates that police this contract are in
> [`LANG-CONFORMANCE.md`](LANG-CONFORMANCE.md); ingestion — each projection run backwards — is in
> [`LANG-RUNTIME.md`](LANG-RUNTIME.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's shapes, competency queries, and the
> projection loss ledger — not a claim that any implementation already realizes X except as those
> gates demonstrate.

## The contract

Five rules govern every projection in this charter, restated from the manifesto once and enforced
per target:

1. **Generated, never hand-authored.** Each surface is emitted from the canonical model by the
   pipeline; a hand edit to a generated surface is drift and fails the gate.
2. **Preservation is declared per target** using the existing `logic:` `preservationKind`
   vocabulary verbatim — the slice mints no near-synonyms — and recorded in the same loss ledger
   as the OWL, Datalog, SHACL, and correspondence lowerings.
3. **Unsupported constructs are enumerated.** Each target declares what it cannot carry; an
   undeclared unsupported construct is a conformance failure, not a footnote.
4. **Declared-exact projections round-trip.** Any target claiming `ExactPreservation` for a
   fragment is held to section/retraction on the conformance corpus.
5. **External identifiers name alignments, not identity** — inherited unchanged.

## The targets

### OntoLex-Lemon — the lexicon surface

The primary lexical projection: `lang:Lexeme` → `ontolex:LexicalEntry`, `lang:WordForm` →
`ontolex:Form` (with UD-aligned features lowered to lexinfo properties), `lang:Sense` →
`ontolex:LexicalSense`, entity-kind denotation records → `ontolex:denotes`/`ontolex:reference`.

**Declared loss.** Lemon has no vantage, no interpretation acts, no co-resident readings with
held support, and no preservation-judged translation — the epistemic layer flattens. Denotation
kinds beyond entity/class reference (`lang:denotesLogicFormula` and kin) have no Lemon target and
are enumerated unsupported. Judgment: faithful for form/sense/reference structure, lossy for
epistemic shape.

### WordNet / ILI — the sense-inventory alignment

Not an emission target but an **alignment surface**: `lang:Sense` individuals align to synsets via
`gmeow:TermEquivalence` records lowered as `logic:Correspondence`, with the interlingual index as
the cross-language pivot where it helps. **Declared loss.** A synset is coarser than a GMEOW
sense with its contexts; alignment confidence is recorded per mapping, and synset membership is
never imported as sense identity.

### Universal Dependencies / CoNLL-U — the morphosyntax surface

Analyzed composed forms lower to CoNLL-U: word forms to token rows, `lang:MorphFeature` pairs to
the FEATS column, `lang:slotRole` individuals to UD relations, `lang:formHead` to HEAD. This is
also the highest-traffic **ingestion** surface (backwards, in
[`LANG-RUNTIME.md`](LANG-RUNTIME.md)).

**Declared loss.** CoNLL-U is per-sentence and tree-shaped: cross-sentence structure, readings
beyond the single tree (ambiguity!), senses, denotations, and all provenance drop. The projection
therefore emits one file per *reading*, never a silently chosen winner. Judgment: faithful for
single-reading morphosyntax, lossy above it.

### ISO 24617 SemAF / AMR — the meaning-annotation surface

Denotation records whose targets are `logic:` formulas lower to meaning-graph annotations: AMR
graphs and SemAF (dialogue acts from `lang:communicativeForce`, semantic roles read off the
lowered `logic:` predicates). **Declared loss.** AMR has no quantifier scope, no modality depth,
and no vantage; role inventories are coarser than full-FOL structure. The lowering is
program-dependent: disclosed, judged per emission, never treated
as the meaning itself.

### NIF / Web Annotation — the stand-off anchoring surface

Surface forms with declared normalization anchor NIF/OA selectors (offsets, quotes) for
interoperability with annotation tooling. **Declared loss.** Offsets bind to one surface form —
re-encoding invalidates them by design; the projection records which surface (with which
normalization) each anchor addresses, which is exactly the invariant NIF itself leaves implicit.

### TEI — the document surface

Document-scale composed forms and their renderings lower to TEI encodings for scholarly
interchange. **Declared loss.** TEI carries structure and some analysis but not denotation
records, readings, or preservation judgments; the emission enumerates the dropped strata.

### BCP-47 / ISO 639 / ISO 15924 — the identification surface

Sign systems, varieties, scripts, and orthographies emit their registry identifiers — the
existing `gmeow:bcp47Tag` discipline, now generated from variety structure (`fr` + Canada →
`fr-CA`) rather than asserted. **Declared loss.** Tags are lossy by construction (no history, no
variety relations, no orthography split); the tag-emission map is total over systems that have
registry entries and records the systems that do not, which is data, not failure.

### EBNF / ABNF — the grammar surface

`lang:Grammar` objects emit ISO 14977 EBNF and RFC 5234 ABNF files. For formal sign systems this
is the strongest projection in the charter: the emitted grammar is held to **round-trip**
(parse-back to an isomorphic grammar object) and claims `ExactPreservation` for the
context-free fragment, gated by section/retraction on the conformance corpus — the `lang:` peer
of the CL-dialect round-trip bar. **Declared loss.** Rule provenance, licensing links from forms,
and versioning drop into the file's comments at best; non-context-free side conditions are
enumerated unsupported per emission.

### GMN — the model-notation surface

The token-compact model surface of [`LANG-GMN.md`](LANG-GMN.md): the GMN writer emits the GMN-1
notation (`gmeow:gmnModelNotation`) from the GMN-0 narrow-waist normal form — the forward/put leg,
registered through this charter's registry like every other target, with its preservation
**derived** from the executed dialect crossings (`gmeow:gmnCorrNormalToGmn` and kin), never
declared. **Preservation kind `ExactPreservation`.** The GMN-0 → GMN-1 crossing is a
section/retraction with a retained mnemomorphic witness: full IRIs invert through the codebook's
version-pinned, injectivity-gated alias bijection; confidence and annotations ride BY REFERENCE
(never inlined, never lost). **Enumerated unsupported constructs: none within its covered
domain.** The declared domain is the ENTIRE GMN-0 normal form; the codec is total over the
grounding slices' GMN-0 (logic, lang, math) now (hard-fail on any uncovered grounding construct —
`lang:GmnUncoveredTerm`, never a silent drop), and coverage of every other slice's GMN-0 is a
measured, gated slice-quality axis (`axisGmn1Coverage`) rather than a silently assumed drop.
**Round-trip verdict: discharged by execution.** The claim is honest only once the GMN-1
round-trip gate (a `superset.rs`-style byte-teeth gate over the codec) runs green, exactly as
`gmeow:gmnCorrNormalToGts` is discharged by the narrow-waist superset gate. GMN's declared rate
(`gmeow:gmnDeclaredRate`) is the loss ledger's first rate–fidelity instance: a target whose ledger
row carries the token rate the encoding is contracted to, now alongside a zero structural-drop set.

### Wikidata lexemes — the authority surface

Lexemes, senses, and sign systems align to Wikidata L-ids, sense ids, and QIDs as
`gmeow:TermEquivalence` records — authority links naming alignments, with the GMEOW term
remaining the definition. Live-verification of ids follows the repository's established
curl-validation discipline for QIDs.

## The docs trees — a projection this slice re-types, not re-builds

The multilingual documentation trees are already generated projections of `gmeow.gts`. This
charter adds no second docs pipeline; it types what the existing one produces — each non-English
page a `lang:Rendering` and its pairing with the English peer a `lang:Translation` with
per-unit judgments ([`LANG-TRANSLATION.md`](LANG-TRANSLATION.md)) — so translation loss lands in
the same ledger as every other projection loss. The executable-docs bloat boundary (exec assets
gated to the English tree) becomes a *declared* asymmetry of the translation records rather than
an undocumented pipeline fact.

## Loss-ledger placement

Every target above lands one row per emission in the projection loss ledger with: target, fragment
projected, `logic:preservationKind`, enumerated unsupported constructs, and the round-trip verdict
where exactness is claimed. Adding a target to this charter is a `LEDGER_TARGETS`-class change
with the known drift profile (projection-report snapshots, mappings-union, conformance ledger
rows) and is wired at implementation time through the same sites as every other projection.
