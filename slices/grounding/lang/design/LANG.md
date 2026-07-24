<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — Vision and Doctrine

> The **manifesto** of the GMEOW Language design set; it carries the vision, doctrine, and lineage
> of the third grounding layer. The form calculus, denotation layer, translation-and-rendering
> layer, and projection contract live in the sibling documents below. Where this document states a
> thesis once, the siblings make it precise — repetition is replaced by cross-reference on purpose.
> The cross-slice contract binding this slice to its co-foundational peers (`logic:`, `math:`) —
> the seam registry, shared disciplines, and acceptance bar — is
> [`docs/GROUNDING.md`](../../../../docs/GROUNDING.md).

## The document set

| Document | Genre | Contents |
|---|---|---|
| `LANG.md` (this) | manifesto | vision, doctrine, lineage; the position of the slice as the third grounding layer against `logic:` and `math:`; the grafting posture toward the existing `language`, `notation`, `names`, and `coreference` slices |
| [`LANG-FORMS.md`](LANG-FORMS.md) | form core | the sign-system reference layer (sign systems, varieties, scripts, orthographies, grammars); the typed form AST (surface forms, lexemes, word forms, morphs, indexed form slots; form identity independent of encoding; the analyzed/unanalyzed discipline); grammars as first-class objects |
| [`LANG-MEANING.md`](LANG-MEANING.md) | meaning layer | sense, reference, and the Frege discipline; the reified denotation record and its kinds; compositional denotation into `logic:` (the one-way bridge); interpretation as a vantage-held act; ambiguity as co-resident readings; deixis, indexicality, and speech acts |
| [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md) | translation & rendering | rendering as the reified content→form relation (the general theory behind `math:ExpressionRendering` and every docs/serialization surface); paraphrase; translation as `logic:Correspondence` between sign systems with a preservation judgment; the multilingual documentation trees as the first live corpus |
| [`LANG-GMN.md`](LANG-GMN.md) | dialect charter | GMN, the GMEOW Model Notation — the engineered, token-compact dialect ladder (GMN-0 the narrow-waist normal form; GMN-1 the model surface; GMN-2 lossy cognitive compaction); the rate–fidelity contract over the LLM token channel; the record-initial sigil table, record and tabular forms, the in-band header, the encoding and versioning policy, and the eight-field envelope contract |
| [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md) | projection contract | the generated lossy lowerings — OntoLex-Lemon, WordNet/ILI, Universal Dependencies/CoNLL-U, ISO 24617 SemAF, AMR, NIF/Web Annotation, TEI, BCP-47/ISO 639/ISO 15924, EBNF/ABNF grammar surfaces, GMN — each carrying a preservation judgment in the loss ledger |
| [`LANG-RUNTIME.md`](LANG-RUNTIME.md) | runtime & ingestion | ingestion as projection run backwards (parser-compilers lifting text, CoNLL-U, lexicons, and grammar files into the canonical form AST); content-addressed form interning against ABox density; the NLP-engine handoff with results returned as interpretation observations; the Rust-first implementation posture and acceptance gates |
| [`LANG-CONFORMANCE.md`](LANG-CONFORMANCE.md) | conformance | the gate matrix — every hard rule mapped to its enforcing gate (OWL axiom / SHACL Core / SHACL-SPARQL / source-lint / Rust validator / competency query / projection test) and a named failure class; the reuse of the `logic:` `preservationKind` vocabulary; the positive/negative fixture corpus |
| [`LANG-REFERENCES.md`](LANG-REFERENCES.md) | references appendix | the classified survey of external standards, ontologies, formalisms, and engines — each tagged subsume/project/link/reference × license × kind; the primary anchors (OntoLex-Lemon, Universal Dependencies, Unicode/CLDR, Wikidata lexemes); and the original surface GMEOW authors where no external ontology exists |

> **Reading this design set.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's shapes, competency queries, and the
> projection loss ledger. It is not a claim that any particular implementation already realizes X
> except as those gates demonstrate. The sibling documents named above are companion charters,
> written to the same voice; together with this manifesto they constitute the complete language
> design set.

## The thesis

GMEOW must be able to talk about **saying** with the same claim, provenance, and preservation rigor
it applies to what is said. Every artifact GMEOW holds arrives and leaves as language — prose
definitions, labels, competency questions, documentation trees, serialization formats, notations,
names — and in ordinary practice and in most ontologies all of that is reduced to bare strings with
a language tag. That reduction discards exactly the structure that makes meaning checkable: which
sign system the string belongs to, what structured form the string realizes, what the form denotes,
under which interpretation, held by whom, and at what loss when it is translated or re-rendered.

The language slice supplies **semiotic domain objects** — sign systems, structured forms,
denotations, interpretations, renderings, and translations — that the native `logic:` reasoning
layer consumes and that every other slice's textual surface grounds in. Its core commitment:

> A string is not a form. A form is not its meaning. A sense is not a referent. A language is not a
> tag. A translation is not an equivalence. Every meaning-bearing artifact that carries inferential
> weight is structurally represented, system-scoped, denotation-linked, vantage-held, and
> projection-audited.

This is the project's own doctrine applied to language: author the maximal, explicit, factored form
in the canon, then project lossily and visibly to every consumer — OntoLex-Lemon for lexica,
Universal Dependencies for morphosyntax, SemAF/AMR for meaning annotation, TEI for documents,
BCP-47 for language identification, EBNF for grammars. None of those becomes a second source of
truth; each is a generated view with a recorded preservation judgment.

## The three grounding layers

GMEOW has three co-foundational grounding layers, and language is the third of them. `logic:`
grounds **reasoning** — truth, inference, proof, modality — as a Turing-complete computational
substrate built on a relational core. `math:` grounds **quantity and structure** — number, space,
operation, measure, dimension — as the canonical structural substrate. `lang:` grounds **meaning
and expression** — sign, form, denotation, interpretation, translation — as the canonical semiotic
substrate. None reduces to the others, and almost every real artifact needs all three: a claim is a
linguistic act (`lang:`) carrying logical content (`logic:`) that is often about quantities
(`math:`). "*The p-value was 0.03*" is a sentence realizing a form that denotes a formula that
references a framed measure.

| | `logic:` — logical grounding | `math:` — mathematical grounding | `lang:` — semiotic grounding |
|---|---|---|---|
| **Grounds** | reasoning, truth, inference, proof | quantity, structure, measure, dimension | meaning, expression, reference, interpretation |
| **Core** | relational (predicates, rules, resolution) | structural (objects, operations, morphisms) | semiotic (sign, form, denotation, interpretation act) |
| **Character** | Turing-complete computational substrate | canonical structural/quantitative substrate | canonical expressive/denotational substrate |
| **Canonical IR** | the full-FOL typed IR | the expression AST + object/structure model | the form AST + reified denotation records |
| **Projects out to** | OWL, Datalog, SHACL, Prolog, N3, gUFO | MathML, OpenMath, RDF Data Cube, QUDT, STATO | OntoLex-Lemon, WordNet/ILI, UD/CoNLL-U, SemAF, AMR, TEI, BCP-47, EBNF |
| **Dogfoods** | GMEOW's axioms and foundation ground in it | GMEOW's quantities, counts, dimensions, probabilities ground in it | GMEOW's labels, prose, docs trees, notations, and serialization grammars ground in it |

Both older manifestos already lean on this layer without owning it. `MATHEMATICS.md` declares that
"a formula is not a string" and that MathML is a *rendering* — but the rendering relation itself is
nobody's first-class subject. Both layers terminate in *denotation* — a `math:` expression
"denotes into" a `logic:` term; the Common Logic dialects are parse/emit surfaces of `logic:` — but
denotation is a bridge primitive used twice and grounded zero times. `LOGIC.md`'s deepest design
influence is Ithkuil, a natural language, and the projection doctrine ("maximal canon, speakable
surfaces") is a semiotic thesis about which expressions can carry which meanings at what loss. The
language slice grounds what its siblings presuppose.

> **Honesty note.** `MATHEMATICS.md` previously said "GMEOW has two co-foundational grounding
> layers." That sentence is revised by this design set: the mathematics manifesto's wording and the
> constitutional statement of the triad (CONSTITUTION.md Principle 19) are updated in the same
> change as this manifesto, not left in silent contradiction.

### The bridges and the co-foundational kernel

The layers **interlock at declared bridges, not by merger**, and every bridge is a registered seam
in the grounding contract ([`docs/GROUNDING.md`](../../../../docs/GROUNDING.md)). The three
grounding slices are co-foundational peers — declared with `gmeow:sliceCoFoundationalWith` in each
manifest — rather than rungs of an internal dependency ladder.

- **`lang:` → `logic:`.** A form *denotes* into a `logic:` term, formula, type, or proof object
  through a reified `lang:Denotation` record with a declared denotation kind and lowering
  preservation ([`LANG-MEANING.md`](LANG-MEANING.md)) — exactly parallel to the `math:` expression
  lowering. `logic:` never depends back on `lang:`: its prose and label surfaces stay ordinary
  annotations that `lang:` objects may later analyze.
- **`lang:` ↔ `math:` — two registered seams, one each way.** Formal-language-theoretic facts
  *about* `lang:` objects — the language generated by a grammar as a set of strings, automaton
  equivalence, information measures over codes — are `math:` objects that reference `lang:`
  individuals, and the `math:` rendering seam (`math:ExpressionRendering ⊑ lang:Rendering`,
  realized; see [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md)) grafts mathematical notation onto the
  general rendering theory rather than forking it. In the other direction, `lang:`'s own declared
  and measured magnitudes — GMN codebook rates and glyph token costs — ground in `math:Quantity`
  with `math:hasDimension`/`math:quantityValue` through the registered quantity seam, because a
  dimensioned magnitude is a mathematical object wherever it appears.

The symmetry is made concrete in the namespace. The grounding layer's terms live in the **`lang:`**
namespace (`https://blackcatinformatics.ca/lang/`), peer to **`logic:`**
(`https://blackcatinformatics.ca/logic/`) and **`math:`** (`https://blackcatinformatics.ca/math/`).
Terms this layer *borrows* from other slices keep their home namespace — the `observations` spine
(`gmeow:Observation`, `gmeow:vantage`), `provenance`/`events` (`gmeow:Activity`,
`gmeow:wasGeneratedBy`), and the alignment vocabulary (native `skos:*Match` alignment cells) — and the slice is
still *declared* with the `gmeow:` slice-manifest vocabulary. A worked example therefore mixes
namespaces on purpose: a `lang:` form *held via* a `gmeow:Observation` and *denoting into* a
`logic:` formula is the grounding-layer composition made visible.

## Why This Exists

The current ontology can *hold* a string — a label, a definition, a documentation page, a name —
with a language tag and provenance. That is the right substrate for prose, and the language slice
builds on it rather than around it. But holding a string is not the same as saying **what the
string is** or **what it means**, and three structural gaps follow.

First, **a string is not a form.** `"bank"` is eight bits times four; a word is a form in a sign
system — a lexeme with a part of speech, morphology, and senses, realized by written and spoken
surface forms across scripts and encodings. The same byte string realizes different forms in
different systems (`chat` in English and French), and the same form is realized by different byte
strings (NFC/NFD, transliterations, spellings). Storing only the string makes form identity
untrackable: nothing can say two labels are inflections of one lexeme, or that a rename preserved
the term and changed only its realization. The language slice supplies form identity as first-class
structure, with the byte string demoted to a `lang:SurfaceForm` — one realization among several.

Second, **a form is not its meaning, and sense is not reference.** "The morning star" and "the
evening star" share a referent and differ in sense; `bank` has one spelling and several senses; a
GMEOW definition sentence *denotes* the term it defines. Most systems collapse this triangle into
string-equality, which is why they cannot represent ambiguity, synonymy, or translation except as
noise. The language slice holds the Frege triangle — form, sense, reference — as three disjoint
kinds connected by reified, contexted records, and it must never let string identity silently stand
in for identity of sense or of referent. This is the layer's peer of the `math:` rule that a
probability is not a confidence.

Third, **interpretation is an act, not a lookup.** Assigning a meaning to a form happens from a
vantage, under a sign system and a context, by an agent or a process, with confidence and
provenance — and it can come out differently for different interpreters, which is not an error but
the epistemic shape of language. An ambiguous utterance has co-resident readings; a translation has
a recorded loss; a parse produced by an NLP engine is an observation with a method, not a fact. The
language slice makes interpretation a vantage-held act whose results are observations, in exactly
the discipline the observation spine already enforces for measurements.

## Position within GMEOW

The language slice is a **core** slice and the third grounding layer. It sits below every slice
with a textual, nominal, or notational surface — which is all of them — and above the base
identity, claim, provenance, and reasoning slices it depends on. Its dependency set is drawn
entirely from existing core slices: `kernel` and `entities` for base type vocabulary; `logic` for
denotation targets, correspondence, and preservation semantics; `observations` for claims,
interpretation results, and vantage; `evidence`, `provenance`, and `citations` for warrant, method,
and bibliographic grounding; `temporal` and `versions` for time-scoped varieties and evolving
lexica.

Three boundaries are load-bearing and stated once here.

**It does not replace `logic:`.** The `logic:` layer owns formal reasoning semantics — truth,
inference, proof, modality, and the correspondence calculus. The language slice creates **no
alternate meaning calculus**: when a form's meaning is formal, the meaning *is* a `logic:` object
(a formula, a term, a type) reached through a declared denotation record, and compositional
semantics is a lowering into the `logic:` IR, not a rival representation. What `lang:` adds is the
semiotic side `logic:` deliberately does not model: the forms themselves, their systems, and the
interpretive acts that connect form to content.

**It does not replace the `observations` spine or the prose surface.** GMEOW's `@x-gmeow-english`
prose fields, labels, and documentation remain ordinary annotations; nothing requires every string
in the repository to be lifted into a form AST. The analyzed/unanalyzed discipline
([`LANG-FORMS.md`](LANG-FORMS.md)) makes the boundary explicit: a string is either the surface of an
analyzed form, or it is explicitly *unanalyzed prose* — a recorded status, not a silent default —
and only forms that carry inferential weight (denoting forms, grammar-governed forms, translated
forms) are required to be analyzed.

**It deepens `language`, `notation`, `names`, and `coreference` rather than forking them.** Before
the graft the `language` slice held its own language classes and individuals (`gmeow:Language`,
`gmeow:FormalLanguage`, `gmeow:ProgrammingLanguage`, `gmeow:WritingSystem`,
`gmeow:TransliterationScheme`, the BCP-47 tag property); the `notation` slice held notation systems
with projection profiles and declared loss; and `names` and `coreference` held naming and
reference-resolution machinery. These are the domain surface of exactly the phenomena `lang:`
grounds. The grounding layer supplies the substrate — `lang:SignSystem` beneath `gmeow:Language` and
`gmeow:NotationSystem`, `lang:Rendering` beneath the notation projection profile, `lang:Denotation`
beneath naming and coreference — and the domain slices are grafted onto it, exactly as `math:`
reuses and deepens `notation` rather than creating a mathematical-notation twin.
Under the greenfield principle, where a grounding-layer term strictly supersedes a domain term, the
inferior term was removed in the graft, not kept as a shim.

## Slice placement, tier, and manifest

The slice is placed at `slices/grounding/lang/` — the `slices/grounding/` group is the grounding
layers' home, peer to `slices/grounding/math/` and `slices/grounding/logic` — and declares
`gmeow:tierCore` — the manifest, not the directory, is the source of tier. Core tier is the deliberate commitment: forms, denotations,
interpretations, renderings, and translations are part of the default mental model every textual
surface builds on, not an optional extension. Each grounding layer's directory name matches its
namespace prefix; the namespace is the identity, the path is human organization the build never
reads.

The manifest is authored (`manifest.ttl`) and is the sole source of slice identity and tier. Its
realized dependency set — gate-verified against the *computed* cross-slice reference graph, so it
carries exactly what `module.ttl`/`shapes.ttl` actually use — is `logic`, `kernel`, `entities`,
`provenance`, `observations`, and `events`, together with the co-foundational peerage declaration
(`gmeow:sliceCoFoundationalWith` naming `logic` and `math`, per
[`docs/GROUNDING.md`](../../../../docs/GROUNDING.md)). The `evidence`, `citations`, `temporal`, and
`versions` dependencies this manifesto once sketched were trimmed by that computed graph: the slice
reaches them through the observations and provenance spines rather than referencing their terms
directly. The deliberate **absences** remain as load-bearing as the presences: `math` appears under
peerage (the registered rendering and quantity seams), never under `sliceDependsOn`; and no
`language`/`notation`/`names`/`coreference` (those slices migrate to depend on `lang:`, never the
reverse).

## Lineage and Supersession

The language slice inherits the project's supersession doctrine: each external linguistic or
document vocabulary contributes a fragment, imposes a restraint GMEOW rejects, and becomes a
**generated, lossy projection** rather than the canonical model. GMEOW aligns to each by reference,
projects to each where a consumer needs it, and records the loss.

| External vocabulary | Contributes | Restraint we reject | How the language slice exceeds it |
|---|---|---|---|
| **OntoLex-Lemon** | the form/sense/reference split for lexica; the ontology-lexicon interface | a lexicon model with no claim, vantage, or provenance structure; senses as a frozen inventory | forms, senses, and denotations are vantage-held, contexted, provenance-carrying objects; Lemon is the lexicon projection |
| **WordNet / ILI** | synsets, lexical relations, the interlingual index | synset membership mistaken for meaning identity; no compositional semantics | senses are first-class with declared relations; synsets are alignment targets, not identity |
| **Universal Dependencies / CoNLL-U** | the de facto morphosyntax standard across 100+ languages | an annotation *format* per sentence, not an ontology; trees without meaning | the form AST holds UD-alignable morphosyntax as structure; CoNLL-U is an exchange projection and an ingestion surface |
| **ISO 24617 (SemAF) / AMR / PropBank / FrameNet** | meaning-annotation practice: roles, frames, meaning graphs | meaning graphs unanchored to a logic; fixed role granularity; English-centric bias | meaning bottoms out in `logic:` through declared denotation records; role inventories are alignment targets |
| **NIF / Web Annotation** | stand-off annotation, string anchoring | character offsets mistaken for identity | anchoring targets are surface forms of identified forms; offsets are projection detail |
| **TEI** | document and text encoding at scholarly depth | markup treated as the text's identity | document structure is held natively; TEI is a document projection |
| **BCP-47 / ISO 639 / ISO 15924 / CLDR** | language, script, and locale identification | a language reduced to a tag string | sign systems, varieties, and scripts are individuals with structure and history; tags are generated identifiers, exactly as the existing `language` slice already treats `gmeow:bcp47Tag` |
| **Unicode** | character identity, scripts, normalization forms | codepoints mistaken for characters mistaken for text | surface forms declare encoding and normalization; Unicode properties are referenced, not re-modeled |
| **EBNF / ABNF (ISO 14977, RFC 5234)** | grammar interchange | a grammar as a file format with no semantic link | grammars are first-class `lang:` objects linked to the sign systems they generate and the forms they license; EBNF is a grammar projection |
| **Wikidata (lexemes)** | QIDs and L-ids for languages, lexemes, senses | an identifier source mistaken for a definition source | Wikidata IDs are authority links that name alignments; the GMEOW term remains the definition |

The governing rule across the table: **external identifiers name alignments, not identity.** A
GMEOW sign system, lexeme, or sense may align to Wikidata, OntoLex, WordNet, or a BCP-47 tag, but
the GMEOW term is the local source of truth, and every lossy export carries a preservation record.

## Design influences — abstract syntax, the triadic sign, and Montague's program

Three influences shape the slice beyond the external vocabularies it supersedes.

**Grammatical Framework's abstract/concrete split** is the semiotic instance of the project-wide
canon/projection doctrine, discovered independently in another field: GF holds one *abstract
syntax* (the structured content) and many *concrete syntaxes* (per-language linearizations), and
translation is composition through the abstract tree. The `lang:` form AST plus the rendering and
translation layers are exactly this shape — the canonical form is the maximal structured one, and
every language-specific string is a linearization with a recorded loss. GF proves the architecture
is buildable; `lang:` adds what GF's compiler discards: claims, vantage, provenance, and the loss
ledger.

**Peirce's triadic sign** — sign, object, interpretant — is the reason interpretation is an *act*
in this design and not a static table. A denotation record that named only form and referent would
be a dyadic sign, and dyadic signs cannot represent disagreement, ambiguity, or change of meaning
over time. The reified `lang:Denotation` with its interpreting context, and the
`lang:InterpretationAct` that produces it, are the triad made structural — and they are what lets
the slice hold "this sentence meant X to the 1890 reader and means Y now" as data rather than as a
contradiction.

**Montague's program** — "there is no important theoretical difference between natural and formal
languages" — is the license for one form AST across English prose, Turtle syntax, mathematical
notation, and programming languages, rather than a natural-language module and a separate
formal-language module. The differences are real but they live in the *grammar* and *sign system*
objects, not in the ontology of form itself. Compositional denotation into `logic:` is the
Montagovian lowering, specified in [`LANG-MEANING.md`](LANG-MEANING.md) for the fragment GMEOW
needs, with the unliftable remainder held honestly as unanalyzed prose.

## The canonical layer model

The slice is one ontology unit factored into coherent internal regions. The manifesto names the
regions; the sibling charters make each precise.

- **Sign-system reference layer** — sign systems (natural, formal, notational), varieties, scripts,
  orthographies, and grammars, with authority alignments to external registries. External
  identifiers name alignments; the GMEOW term is identity. Detailed in
  [`LANG-FORMS.md`](LANG-FORMS.md).
- **Form AST layer** — surface forms, lexemes, word forms, morphs, and composed forms with indexed
  form slots; form identity independent of encoding, script, and rendering; the
  analyzed/unanalyzed discipline. Detailed in [`LANG-FORMS.md`](LANG-FORMS.md).
- **Meaning layer** — senses, referents, and the reified denotation record; denotation kinds
  (entity, `logic:` formula/term/type, `math:`-object-by-reference); compositional denotation;
  interpretation acts, readings, ambiguity, deixis, and speech acts. Detailed in
  [`LANG-MEANING.md`](LANG-MEANING.md).
- **Rendering and translation layer** — rendering as the reified content→form relation; paraphrase;
  translation as correspondence between sign systems with preservation judgments; transliteration.
  Detailed in [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md).
- **Projection and alignment surfaces** — the generated lossy lowerings to OntoLex-Lemon,
  WordNet/ILI, UD/CoNLL-U, SemAF/AMR, NIF, TEI, BCP-47, and EBNF. Detailed in
  [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md).

## The doctrine — hard fails and the loss ledger

The slice inherits the project's low/no-optionality posture: dangling sign-system references,
denotations without context, silently resolved ambiguity, and lossy unmarked translations **fail
early**. The manifesto records the doctrine; the sibling charters specify the SHACL and source-lint
gates that enforce it ([`LANG-CONFORMANCE.md`](LANG-CONFORMANCE.md)).

- **Form ≠ sense ≠ reference.** The three kinds are disjoint. String identity never stands in for
  identity of form; form identity never stands in for identity of sense; sense identity never
  stands in for identity of referent. Conflation is a typed failure, not a modeling convenience.
- **Analyzed or explicitly unanalyzed.** A string is the surface of an analyzed form, or it is
  explicitly unanalyzed prose — a recorded status, never a silent default. Forms that carry
  inferential weight (denoting, grammar-governed, or translated forms) are analyzed.
- **Every form has a system.** A form without a sign system is ill-formed, not under-annotated. A
  sign system is an individual, never a bare tag; tags are generated identifiers.
- **Denotation is declared, contexted, and kind-typed.** A denotation names its form, its target,
  its kind, and its interpreting context; formal meaning bottoms out in `logic:` objects through
  the one-way bridge.
- **Interpretation is a vantage-held act.** Interpretation results are observations with method and
  provenance. Ambiguity is held as co-resident readings with vantages and confidences; silent
  disambiguation is fabrication of certainty and fails the gate.
- **Rendering never becomes identity.** A rendering names the content object it renders; a
  translation names its source, target, and preservation judgment. Any projection that drops
  senses, morphology, denotation links, or reading multiplicity emits a machine-readable
  preservation/loss record in the loss ledger, in the same discipline `logic:` and `math:` apply.

The loss ledger is the semiotic instance of the project-wide preservation contract: "GMEOW projects
faithfully to vocabulary V" — and now also "GMEOW translates faithfully to language L" — is a
checkable claim with a recorded polarity, not a slogan.

## Flagship competency questions

The grounding layer's depth is defined by concrete grand acceptance scenarios, not by adjectives. A
conforming realization is one that can represent each of the following structurally — system-scoped,
denotation-linked, vantage-held, and projection-audited — and each stresses a different axis of the
layer. All five reduce to the same core: *forms and meaning-preserving maps, expressed across the
`lang:`/`logic:` bridge, deeply enough to be self-describing.*

1. **A sentence to a formula, compositionally.** "Every cat chases some mouse" as a form AST whose
   compositional denotation lowers, stage by declared stage, into a `logic:` first-order formula —
   morphosyntax, then Montagovian composition, then the `logic:` IR — with each stage's
   preservation recorded. The purest test of the one-way bridge, and the `lang:` peer of the
   homomorphic-encryption flagship.
2. **GMEOW reading its own prose.** Every `@x-gmeow-english` definition held as a surface form whose
   denotation record targets the term it defines, making the existing prose-hash discipline
   (`candidateSourceHash` over prose fields) a `lang:` fact rather than a pipeline convention. The
   dogfooding apex: the ontology's own documentation becomes data the ontology can reason over.
3. **The multilingual docs trees as translation.** The generated ×N-language documentation trees
   held as `lang:Translation` correspondences with per-unit preservation judgments — the loss
   ledger applied to human languages, answering "what does the French tree lose against the English
   canon?" as a query.
4. **GMEOW's serializations as grammars.** The grammar of Turtle and of GTS held as `lang:Grammar`
   objects over formal sign systems, with parse and emit as interpretation and rendering acts — so
   "ingestion is projection run backwards" is a grounded statement about first-class objects, not a
   slogan. The self-describing-serialization apex.
5. **Ambiguity held honestly.** "I saw her duck" as one form with co-resident readings — distinct
   denotation records held from vantages with confidences, never silently resolved — and a
   downstream `logic:` reasoning request that must acknowledge the ambiguity or report itself
   unevaluated. The epistemic-shape-preservation test for meaning.

These are the layer's acceptance bar. [`LANG-REFERENCES.md`](LANG-REFERENCES.md) records which
anchor externally and which GMEOW authors; the domain charters exist to make them answerable.

## Constitutional Alignment

The language slice is the project's doctrine applied to language and to the project's own voice.
The CONSTITUTION requires a maximal canonical model, maximal linking, explicit and gated
projection, and no compatibility format promoted above the canonical source. The statement layer
realizes this for facts; `logic:` realizes it for axioms and the foundation; `math:` realizes it
for quantity and structure. The language slice realizes it for *meaning-bearing form itself* —
forms as ASTs, denotation as declared records, interpretation as vantage-held acts, translation as
audited correspondence — and takes OntoLex, WordNet, UD, SemAF, TEI, BCP-47, Unicode-adjacent
registries, and the grammar formats to their correct places as documented, reproducible, lossy
projections, never second sources of truth.

## End State

The end state is not "a lexicon vocabulary, but richer." It is:

- meaning-bearing artifacts are structurally represented, system-scoped, denotation-linked,
  vantage-held, and projection-audited, with the same claim and provenance rigor GMEOW applies
  everywhere else;
- forms are canonical structured objects; byte strings, spellings, transliterations, and rendered
  documents are surface forms and projections, never canonical substitutes;
- meaning bottoms out in the `logic:` layer through declared, kind-typed denotation records, with
  sense, reference, and form held strictly distinct;
- interpretation, ambiguity, and translation carry their epistemic shape — vantages, readings,
  confidences, and losses — instead of being silently flattened;
- the `language`, `notation`, `names`, and `coreference` slices stand on the grounding layer
  rather than beside it, with superseded terms removed under the greenfield principle;
- OntoLex-Lemon, WordNet/ILI, UD/CoNLL-U, ISO 24617/AMR, NIF, TEI, BCP-47/ISO 639/ISO 15924, and
  EBNF/ABNF are generated, lossy projections and alignment surfaces, each carrying a preservation
  judgment in the loss ledger;
- projection loss — including translation loss across human languages — is visible,
  machine-readable, and tested.

This makes the language slice match the rest of the project: a maximal canonical model, maximal
linking, explicit projection, and no compatibility format — not OntoLex, not TEI, not a language
tag — promoted above the canonical source.
