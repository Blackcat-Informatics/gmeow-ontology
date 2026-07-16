<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — Runtime and Ingestion

> The **runtime charter** of the GMEOW Language design set: ingestion as projection run backwards,
> the content-addressed form interning that keeps the ABox tractable, the external-NLP-engine
> handoff, and the Rust-first implementation posture with its acceptance gates. The structures
> being built are defined in [`LANG-FORMS.md`](LANG-FORMS.md) and
> [`LANG-MEANING.md`](LANG-MEANING.md); the outbound surfaces this charter runs backwards are in
> [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's gates — not a claim that any implementation
> already realizes X except as those gates demonstrate.

## Ingestion is projection run backwards

Every inbound surface is a parser-compiler that **lifts** external material into the canonical
form AST and meaning records, in the same architecture as the `math:` bridges (R → `math:`,
ONNX → `math:`) and the `logic:` dialect importers (CLIF/CGIF/XCL). One rule governs all of them:

> **Lift fully, or hard-fail with a typed diagnostic.** An ingester never drops content silently
> and never approximates structure it cannot represent. What cannot be lifted is either held
> honestly as `lang:UnanalyzedProse` (when the input is prose and that is its truthful status) or
> rejected with a diagnostic naming the unliftable construct (when the input claims structure the
> lifter cannot carry). Epistemic shape is preserved in both directions — imposed structure is
> fabrication, discarded structure is loss.

The inbound surfaces, ordered by how much structure they claim:

- **Plain text** lifts to `lang:SurfaceForm` individuals — `lang:UnanalyzedProse` unless and until
  an interpretation act analyzes them. This is deliberately cheap: *every* string GMEOW already
  holds can be lifted to this stratum without any NLP, which is what makes flagship 2 (GMEOW
  reading its own prose) incremental rather than big-bang.
- **CoNLL-U** lifts token rows, FEATS, and dependency relations into word forms, morph features,
  and composed forms with slot roles — the UD projection run backwards. A CoNLL-U file that
  violates the UD spec is rejected, not repaired.
- **Lexicons** (OntoLex-Lemon, Wiktionary-derived extracts after license review) lift to lexemes,
  word forms, and senses, with the source recorded as the vantage holding the sense inventory —
  a lexicon is somebody's claim about a language, and it stays that.
- **Grammar files** (EBNF/ABNF) lift to `lang:Grammar`/`lang:GrammarRule` objects — the grammar
  projection run backwards, and the round-trip partner of its emission
  ([`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md)).
- **GMN** ([`LANG-GMN.md`](LANG-GMN.md)) is the LLM-facing case of the same doctrine: LLM-emitted
  GMN is lifted by the GMN parser — parse (the LL(1) table), alias-expand (against the pinned
  `gmeow:gmnDictionaryVersion`), typecheck, prover, canonical store — the GMN projection run
  backwards. A record the lift rejects is answered with an `@err` in-dialect failure record naming
  its `lang:Gmn*` class, so the failure returns over the same channel as the claim; nothing is
  repaired silently, and nothing enters the canon unproven.
- **GMEOW's own serializations** are the self-hosting case: the Turtle and GTS grammars, lifted
  once as grammar objects, make every parse of the repository's own files an interpretation act
  over a formal sign system — flagship 4. This re-types the existing native parse/emit seams; it
  does not add a second parser stack beside `native_codecs`.

Authoring converges on the same AST: slice authors write forms in the slice DSL/TTL (the
authored surface), ingesters lift external files, and both meet in one canonical representation —
there is no "ingested form" type distinct from an "authored form" type.

## Content-addressed form interning

Text is high-volume and highly repetitive; a naive lift of every docs page would flood the ABox
with near-duplicate nodes. The runtime therefore interns forms exactly as `math:` interns
expressions:

- Every `lang:Form` carries a **content key** computed over its structural content — sign system,
  stratum, features, slot structure with constituent keys — per the identity rule of
  [`LANG-FORMS.md`](LANG-FORMS.md): surfaces, encodings, and renderings are excluded by
  construction.
- Structurally identical forms are one node; realization links fan out from it. The key
  computation is the Rust validator's job, deterministic and enforced at fold time, with the
  derived-`Ord`-versus-lexical-sort trap explicitly out of scope for key ordering (content keys
  sort lexically, never by enum declaration order).
- **Surface forms intern by (text, script, encoding, normalization)** — byte identity plus its
  declared frame — which is what makes the prose-hash discipline (`candidateSourceHash`) a lookup
  rather than a recomputation.
- Large texts follow the **blob by-reference doctrine**: a document-scale surface form holds a
  `blob_id` reference and origin, never inline payload bytes.

## The external-engine handoff

Parsers, taggers, and MT systems are **oracles that produce claims, never authorities that
produce facts** — the logic-stack retirement doctrine applied from day one rather than retrofitted:

- An external run (a UD parser, a lemmatizer, an MT engine) is a `gmeow:Activity` with the engine,
  version, and configuration as provenance.
- Its output enters as `lang:InterpretationAct` results — readings and candidate denotations held
  from the *engine's* vantage with the engine's confidence, per the process/result/claim
  separation of [`LANG-MEANING.md`](LANG-MEANING.md). Two engines that disagree produce
  co-resident readings; the disagreement is data.
- No engine output is folded into the canon as unattributed structure. Promotion from
  engine-claimed reading to slice-asserted analysis is an explicit editorial act with its own
  provenance.
- Engines are invoked through a declared handoff seam (the solver-profile pattern of
  `math:`/`logic:`), not linked into the reasoning core; a missing engine is a hard fail of the
  lane that needs it, never a silent skip.

## Rust-first posture

Core work is Rust, per the standing constraints; Python appears nowhere in this design:

- **`lang-form`** (working name, peer of `math-ast`): the form AST, content keys, interning,
  normalization, and the analyzed/unanalyzed bookkeeping.
- **`lang-bridge`** (working name): the ingesters (CoNLL-U, EBNF/ABNF, OntoLex, plain text) and
  the emitters of [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md), sharing the lift/emit skeleton
  with the existing dialect and codec crates rather than duplicating it.
- Meaning lowering into the `logic:` IR reuses the `logic:` crates' Formula/Term types directly —
  the bridge is a function into an existing IR, not a parallel one.
- Pipeline integration follows the carrier doctrine: lifted forms travel as named graphs in the
  in-memory `PipelineBundle`, and every generated artifact is a projection of `gmeow.gts`. New
  stages wire through the standard three lockstep sites, and exhaustive corpus sweeps use
  explicit `maint-` lanes while focused contracts stay on the default lane.

## Acceptance gates

The runtime is accepted when, and only when, the gates below pass — each a concrete check, not a
demo:

1. **Total lift of the repository's prose.** Every `@x-gmeow-english` literal in the bundle is
   reachable as a `lang:SurfaceForm` (unanalyzed at minimum) with a stable content key, and the
   prose-hash discipline resolves through it. (Flagship 2, minimal form.)
2. **CoNLL-U round-trip.** A real, ring-fenced + fully-attributed vendored UD treebank fragment
   (CC BY-SA 4.0, cleared for vendoring by the native license CATEGORY keyed off its descriptor,
   not a path) lifts and re-emits byte-identically through the production `ConlluBridge` ON-GATE —
   the same round-trip surface the per-reading projection stage enforces on every shipped
   `.conllu`. The retraction law `serialize∘parse = id` is grounded as the carried
   `logic:Correspondence`/`SectionLaw` and property-tested over a deterministic grammar-edge
   mutation generator (permuted FEATS order, injected/removed `SpaceAfter=No`, a multiword-token
   range, an empty node, and a populated enhanced `DEPS`): every well-formed mutant round-trips
   byte-exact, every ill-formed mutant hard-fails. A CoNLL-U file that violates the UD spec is
   rejected, not repaired.
3. **Grammar round-trip.** The Turtle and GTS grammars lift from EBNF, re-emit, and re-lift to
   isomorphic grammar objects — `ExactPreservation` demonstrated, not asserted. (Flagship 4,
   minimal form.)
4. **Compositional fragment.** The flagship-1 sentence class (quantified subject–verb–object)
   lowers compositionally to `logic:` formulas that the native reasoner consumes, with per-stage
   preservation records present.
5. **Ambiguity survives the pipeline.** A fixture with co-resident readings enters, folds,
   projects, and returns with both readings and their vantages intact — no stage collapses it.
   (Flagship 5, minimal form.)
6. **Determinism.** The three `lang:` corpus producers — the compositional-lowering corpus, the
   prose-lift corpus, and the projection corpus (its graph AND every per-reading `.conllu` and
   sibling external artifact) — produce byte-identical output across repeated regeneration, the
   project-wide determinism bar inherited without exception, discharged ON-GATE by execution rather
   than fixture existence. Two legs prove it: an in-process two-run byte replay asserts each
   producer emits identical bytes across two runs, and the drift lane (`run_full(RunMode::Check)`,
   the strict-sync host) re-derives every committed artifact fresh-process — the per-reading
   `.conllu` files by exact bytes, the corpus graphs through the bundle superset/fold gate. The
   same-process leg is no false green: every corpus payload is asserted to be in the SORTED, DEDUPED
   canonical order the shared N-Triples emitter produces, a property that is a pure function of the
   line set and independent of any hash-map iteration seed — so byte identity in one process holds
   across fresh processes, which the drift lane then confirms.
