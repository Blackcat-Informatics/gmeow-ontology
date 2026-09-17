<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Pipeline Spine — Carrier, Terminal, and Fanout

> **Genre.** A normative architecture spec for the regeneration pipeline. The
> declarative present tense is normative — "the carrier *is* the sole transport"
> means "a conforming realization makes it so" — not a claim that every line of the
> current build already conforms. The conformance gate (§7) is what establishes it.
>
> **Audience.** Anyone working on `crates/pipeline`, the GTS terminal, or any stage
> that produces a committed artifact under `generated/`.
>
> **Scope.** How data moves from the authored slices to the shipped `gmeow.gts` and
> back out to the flat consumer tree. It does not redefine the ontology, the `logic:`
> core, or the GTS file format — only the dataflow that assembles and unpacks them.

## 1. Thesis

The build has one spine and one exit. Authored slices flow through a chain of
stages that each **contribute to a single in-memory carrier**; exactly one
**terminal** stage presents that carrier as the `gmeow.gts` bundle; and a separate,
post-pipeline **fanout** projects the individual files back out of the bundle.

```text
slices/  ─▶  stage₁ … stageₙ  ─▶  terminal  ─▶  gmeow.gts  ─▶  fanout  ─▶  generated/
            (each ATTACHES to        (carrier ▶                (gmeow.gts ▶
             the one carrier)          bundle)                  flat files)
```

Two directions, exactly as the project's standing dataflow demands: everything is
authored **from the slices**, and everything ships **to `gmeow.gts`**. The bundle
and the `gmeow` CLI are the deliverables; every other artifact is a view of the
bundle. This is Principle 4 (one canonical source; everything else a generated
projection) applied to the build itself, and it is the project's *maximal
information flow* maximum made structural: information is carried from source to
bundle at full fidelity, and trimmed only at the exit a consumer asks for.

## 2. The carrier is the spine

There is **one** internal transport: an in-memory bundle value (the
`PipelineBundle` carrier, defined by the `purrdf` substrate) threaded through the
whole run. It holds:

- the **dataset** — every named graph the build accumulates (authored default,
  statement layer, import closure, alignments, the reasoned closure, diagnostics,
  documentation, provenance, …);
- a content-addressed **blob store** — opaque byte payloads (archives, rendered
  text trees, serialized side formats) keyed by digest;
- the **provenance** sidecar — per-quad attribution.

The carrier is the spine for the whole run: every stage hands the next stage a
richer carrier, never a serialized file and never a second parallel assembly. In
particular the GTS form is **exit-only** — the bundle is serialized to `gmeow.gts`
once, by the terminal, and never re-parsed back into the pipeline as transport.
There is no `dataset → gts → dataset` round-trip inside the spine.

**Typed publication identity.** A typed attachment binds its named graph's
canonical digest and its complete native payload commitment. Downstream action
keys and release receipts retain both. A graph projection can omit fields that
remain authoritative in the native program; its digest alone is therefore
insufficient to identify that program. Persistent publication verifies each
handle's governed projection contract, and restoration authenticates the complete
native payload, graph binding and product commitment before admitting a hit.

## 3. The stage contract

A stage is a function from carrier to carrier. It **reads** what upstream stages
contributed and **attaches** its own contribution — one or more named graphs and/or
blobs. Four rules bind every stage:

1. **No out-of-band writes.** A stage never writes a file under `generated/`, never
   reads one back as input, and never opens a side channel to a later stage. Its
   only output is what it attaches to the carrier. (Reading authored *source* —
   `slices/`, `dsl/`, `imports/` — is how stages begin; that is input, not
   transport.)
2. **Transform once (the razor).** Each transformation from one form to another
   happens at most once per run. A closure, a projection, or a rendering is computed
   a single time and attached; downstream consumers read the attached result rather
   than recomputing it. Reasoning in particular runs **once**, materializing the
   reasoned closure as a carrier graph that every consumer — the bundle, the
   committed closure artifact, the documentation — reads from.
3. **Declare contributions.** A stage names the graphs and blobs it contributes, so
   the dependency order is derivable and the superset gate (§7) can map every output
   to its producer.

4. **Read only what you declared.** The scheduler hands a stage exactly the products
   of the ids in its `dataflowConsumes`. There is no ambient access to a sibling's or
   an ancestor's carrier; an undeclared read is not "unsupported", it is
   *unreachable*.

Stage kinds (source-load, transform, reason, validate, docs-render) differ only in
*what* they contribute and *what they read*, never in *how* they deliver it: all
deliver by attaching to the carrier.

`stage-parse-sources` owns the original native root, module and import document
parses. Its typed source catalog retains exact source receipts, document bases,
roles, scopes and positions, plus one bounded composite view and one shared
aggregate materialization. A small generated receipt graph binds the catalog;
the original datasets are ephemeral and cannot enter the persistent stage cache.

After that boundary, `stage-source-load` and `stage-compile-logic` are independent
consumers. Source-load alone publishes source-origin provenance and the selected
carrier partitions. The catalog compiles the complete admitted root, module and
import selection once, before carrier graph projection. Concurrent consumers
share one immutable compiled theory, retaining the prepared source, its actual
source-term mapping, program, original diagnostics and owner emission evidence.
This invocation-local value expires with its consumers; it never enters a
cumulative carrier cache. Diagnostic meta-rules select their actual source-owner
emissions rather than matching provenance strings, and every selected rule must
be present. Correspondence projection enforces the original rejected-owner/leg
dispositions and the already evaluated native law gates, without a second
correspondence extraction or execution. Selected OPT and worked-example products
augment a separate projection program while preserving authored collections;
their example facts do not enter the source theory. No duplicate full-corpus
compiler transport graph is built or cached.

The compiler publishes its shared program and mandatory report inputs as one typed
`CompiledLogic` handle. The report retains compact projection judgments, complete
source-attributed loss witnesses and compiler-owned counts; serialized projection
bodies belong only to their required output artifacts. Mappings borrows these
inputs directly and owns the final correspondence counts. Report rendering indexes
the selected ledgers once by target, retaining borrowed evidence for that invocation.
A program-only snapshot cannot satisfy this report-bearing input contract. The
affine example likewise travels on its native correspondence handle without a
duplicate serialized program channel.

Compiler and validation diagnostics each publish the renderer's final normalized,
meta-enriched `Arc<Report>` through a native `Diagnostics` handle. The validation
record seal is applied before that value becomes immutable. The closed owner set
contains only these two producers; the snapshot shares both report Arcs and the
original producers release at their existing last consumers. In-DAG docs borrow
the producer reports, while post-DAG measurement selects the retained snapshot
explicitly. Both use one borrowed fold, preserving every documented-term and
logical-location join, slice attribution and exact constraint-code help link.
Neither consumer reparses report JSON or reconstructs rich findings from the
loss ledger or diagnostic RDF. Missing native identity is a hard failure.

Report handles authenticate every nested report field and the backing diagnostic
graph together. That graph also contains gate and meta conclusions, so its RDF
projection does not replace the complete reports as semantic input. Empty valid
reports retain an explicit diagnostic graph declaration. Action persistence uses
mandatory CBOR for the full publication, including report-only evidence and
metadata; those bytes enter the normal cache receipt and hydration accounting.
Required JSON, SARIF, HTML and RDF artifacts remain terminal presentations, and
the run ledger retains its established pre-meta finding projection. This removes
transport reparsing; it still retains two rich reports alongside terminal bytes.

Mapping source admission also publishes one immutable `CorrespondenceAnalysis`.
SSSOM, EDOAL, SPARQL and FnO borrow its exact alignment cells, patterns and bindings;
FnO derives its signature model from those patterns without another RDF parser.
Alignment lookup retains the authored morphism qualifiers, so two declarations
about the same assertion cannot overwrite each other's overclaim checks. Source
statistics share this admission and preserve per-file memberships even when the
canonical program deduplicates a correspondence. This analysis lives only for the
lowering invocation; it neither adds a persistent carrier nor certifies a rewrite.

Compiler carrier assembly places the existing canonical, relational-core and
correspondence datasets through one composite view and materializes the final
carrier once. Routing moves ordinary statements, reifier bindings, annotations and
graph declarations together. The old graph has no remaining ownership or empty
declaration; an IRI used as a provenance value remains that value. Independent
input blank scopes remain distinct. These producer inputs carry locations in their
RDF and report payloads; unexpected physical row-location attachments fail admission
before publication. Diagnostics retain their selected graph placement.

`stage-validate` declares the same native catalog as an input. It validates that
complete root/module/import dataset directly and reuses the shared compilation
for its gate and meta-rule folds. The authored-only presentation graph remains a
different selection; neither it nor the byte transport defines this input.
The validation-only substrate role is a borrowed subject/graph selection imported
through PurRDF, preserving native annotations and reifier bindings. Its statements
join the validation data without a Turtle/N-Triples/N-Quads intermediate; the rest
of the provenance graph stays outside the validation target set.
Each prepared selection retains a structural source graph before extraction or
lowering. Typed owner claims, formula/term components, mathematical backing,
module/import links and standpoint references remain distinct. Source documents
keep their exact byte/base/role receipts and original-to-canonical node bindings,
including each contributor to a deduplicated named law and separate anonymous
anchors. Native reifier and annotation tables retain their own source positions
and graph identity, including quoted statements, without a flattening round trip.
Native remapping uses a bounded per-document lookup cache; saturation
recomputes the same binding. This invocation-local structural inventory is not a
complete law-coverage certificate or permission to assert an imported context.
The catalog inventories inputs; a source/import role or transport graph is never
implicit permission to assert its contents in every logical context. Tests consume
the downstream producer-selected persistent receipts and cannot rebuild a catalog
or corpus on a cache miss.

Compiler graph reads stream ordinary statements, native reifier bindings and
annotations through one default-graph boundary. Ordinary patterns use PurRDF's
indexes; subject-bound native probes use its sorted side-table runs. An unbound
native probe streams the side tables. Exact indexed membership deduplicates a
statement stored in multiple physical carriers without an accumulating seen set.
No whole-source owned quad vector or second dataset is created for these passes.
Named-graph metadata cannot supply default-graph scope, and a bare quoted
proposition cannot become an asserted axiom. Unsupported nested scoped terms
receive a lowering error instead of being stringified or dropped.
Invalid confidence, unknown modality, proposition-valued scope coordinates and
unlowered coordinate multiplicity withhold the whole claim or rule with an error.
Separate reifiers preserve coexisting standpoints; table order cannot pick a winner.

The prepared compiler can retain emission evidence bound to that exact source.
Ordinary and annotation axioms carry their native statement coordinates; scoped
axioms and class-expression expansions carry their original structural roots.
Horn routing retains each formula root, and axiom deduplication unions all these
origins. Values and source anchors are sorted together once, so canonical IR
positions come from actual lowering rather than matching generated identifiers.
Every declared formula has an explicit outcome: emitted axiom, emitted formula,
read under an owner, malformed with a diagnostic reference, or outside the selected
default graph. A shared malformed subtree records every affected root even when
the diagnostic itself is shared. These invocation-local records do not certify
complete source coverage, owner execution or optimization laws; class-expression
roots alone do not account for every consumed restriction or list statement.

The rule, contract, path-shape, constraint/sugar, reasoning-program and
correspondence readers also capture their original owner at invocation. Emitted
values keep their owner and diagnostic references through canonical ordering;
rejected owners retain their own errors or a reference to the shared formula
failure. An emitted contract carrying an admission error remains visibly invalid
for execution. Named-graph owners have explicit default-graph exclusions.
Correspondences and their referenced transaction legs borrow PurRDF's native
indexes directly, including annotation rows. There is no separately materialized
subject/predicate string index to rebuild for the legs. Shared leg references are
read once, and a leg without a supported selected path program has an explicit
unlowered diagnostic. Anonymous correspondence identities are recorded as
unlowered by the current named-IR reader, not as invalid correspondence semantics.
Nested owner/component consumption and complete native
admission still require their own evidence; these root records do not supply it.

Selected correspondence scalar reads distinguish an absent field from a present
wrong-kind, conflicting or undecodable value. Enum fields require a recognized
canonical `logic:` value, including optional determinacy, preservation and law
conditions. Quantitative axes retain their exact RDF numeric literals alongside
validated native numeric values. Their lexical decoder and the mnemomorphic
boolean decoder come from PurRDF; numeric value equality never erases authored
literal identity.
No malformed number becomes an absent axis, and no unrecognized boolean becomes
false. Multivalue readers require every declared member, including law, recovery,
caveat and program-registry references. Physical duplicate rows remain one RDF
value rather than a scalar conflict.

Named textual caveats belong to their `Correspondence` IR owner. Every
`rdfs:comment` is retained as a complete PurRDF literal: lexical form, datatype,
language and RDF 1.2 base direction. Multiple comments are a canonical RDF set;
no language or comment wins over another. Literal shape checks use PurRDF's
component validator, and canonical ordering compares borrowed native fields.
The serialized cache representation includes every component and rejects
incoherent language/datatype/direction combinations and unknown directions.
Bare-source
compilation and wrapper rehydration use the same reader; an unreadable caveat
rejects its owner with a diagnostic. Program assembly, put derivation, law
attachment and Common Logic projections retain that typed payload. CLIF and
CGIF metadata literals carry direction in their language token; XCL retains
the native RDF carrier. Caveat identity, comment counts and every literal
component are length-framed in the owner's content key. No program-level
caveat registry or per-correspondence scan of an unrelated side collection is
needed. This literal representation does not implement formula-valued caveats.

The selected path-program reader requires constructor fields, member items and
list tails to agree. A malformed tail cannot truncate a sequence, and conflicting
or unsupported constructors cannot become a bare predicate step. Constructor and
list cycles fail explicitly; the existing depth and length limits remain bounded
errors. Public leg-registry extraction and the producer propagate any selected
leg failure instead of publishing the remaining programs as a complete registry.

Formula assertions, constraints, reasoning-program clauses and correspondence
recovery transforms share one reconstruction session per selected dataset. Its
memo uses native source-term identity together with modal world and depth, with
an 8 MiB retained-payload limit and a 4096-entry limit. Eviction recomputes the
same formula; it cannot omit a check. Modal binders use a deterministic namespace
disjoint from authored variables, and sort declarations remain occurrence-scoped
through modal bodies. Only assertion roots retain owned trees in the formula
extractor; intermediate reuse belongs to the bounded session. Source ownership,
admission and execution evidence are separate from this derived-value memo.

The mapping adapter borrows native reifier identities from the same source dataset.
It probes each reifier's indexed statement rows and allocates only the selected
correspondence fields; unrelated propositions do not become owned intermediate
records. Scalar field kind and multiplicity are checked before lowering. Ordinary
and native annotation carriers share the same default-graph read boundary.
The adapter's value type is PurRDF's complete `TermValue`, including scoped blanks,
directional literals and nested propositions. Expression constants retain that
value until the target renderer admits them or records unsupported residue. A
source-scoped blank in a proposition requires an explicit target query binding.

**The carrier's lifetime is bounded (drop-after-last-carrier-consumer).** Every
`dataflowConsumes` edge remains the authored scheduling, action-key, and artifact
dependency. The executable `carrier_consumes()` subset says which of those edges
also reads the live transport lanes. The loader requires that subset to be unique
and contained in `dataflowConsumes`; it cannot create a shadow DAG. A producer's
carrier is therefore *provably dead* once its last declared carrier reader has run,
or immediately after production when every later reader is artifact-only. The
whole-repository build **releases** it at that point — the dataset, typed handles,
blob records, provenance, and internal `pipeline/`-prefixed byte artifacts are freed
— keeping only the committed byte artifacts later consumers and the post-run
reconcile still read, plus the product's `digest` verbatim. Peak residency is then
the live carrier frontier plus the run's outputs, not the **sum** of every stage's
cumulative carrier snapshot over the DAG; the latter grows with both the corpus and
the stage count, and is a build that dies as the ontology grows rather than one that
scales with it.

The release is invisible: it never changes a produced byte, and the run's
order-independent `combined_digest` is identical with or without it. A stage that
reaches for a carrier through an artifact-only edge, or an out-of-band whole-run
reader that reaches for an unretained intermediate, HARD-fails on the released marker
rather than treating the empty residue as data. A post-run proof names the exact
intermediate carriers it retains in the run context; `RetainAll` remains an explicit
diagnostic profile. These profiles change only residency, never produced bytes.
On glibc Linux, the scheduler also requests allocator reclamation whenever ownership
of the declarative serialization-buffer resource passes to the next carrier-scale
serializer, and at the topological barrier immediately before the unique terminal.
That returns pages already freed by cold export, mapping, and dictionary waves instead
of carrying arena slack into the next whole-document buffer; on other allocators it is
a no-op, and on every platform it changes only residency, never the live value graph or
output.

### 3.1 The grounding-generator contract

A grounding slice (`logic:`, `lang:`, `math:`) does not only *declare* vocabulary —
it computes things: closures, corpora, projections, executed lifts. Those computed
artifacts reach consumers the same way every other stage output does, and the
uniformity is the point: **a grounding slice's computed artifact is a carrier graph
attached by a registered stage, never a file the slice writes and something else
reads back.** Rule 1 of the stage contract admits no grounding exception.

Three stages instantiate the contract today, and they differ only in what they
compute — never in how it travels:

| Stage | Computes | Attaches |
| --- | --- | --- |
| `stage-compile-logic` | the `logic:` IR and its typed views | `GRAPH_LOGIC`, `GRAPH_RELATIONAL_CORE`, `GRAPH_CORRESPONDENCE`, `GRAPH_DIAGNOSTICS` |
| `stage-reason` | the reasoned closure (once — the razor) | `GRAPH_REASONING`, `GRAPH_EXAMPLES`, `GRAPH_DIAGNOSTICS` |
| `stage-math-producers` | the `math:` producer outputs, including executed R / ONNX / TSTP lifts | `GRAPH_EXAMPLES` |

Diagnostic reports project directly into native datasets for rule execution and
carrier publication. The same finding projection supplies the terminal text
renderer, preserving grade coordinates, exact location integers, antecedent
identities and quad-provenance links. The shared diagnostics renderer carries its
native findings plus authored gate and meta-rule conclusions alongside JSON,
SARIF, HTML and canonical RDF artifacts. Compilation and validation consume that
dataset directly; neither reads its RDF artifact back. Meta-findings append into
the final builder, and an empty derivation reuses the immutable input publication.
The reason stage appends chase diagnostics into its final carrier builder.
Minted cross-node conflict witnesses use the shared A-Box annotation contract,
including their required carrier-language label and definition. These changes
remove transport conversions; they neither replace the authored rules nor grant
composition or optimization certificates.

The reason stage projects each derived axiom once into its native carrier and
required Turtle artifact. Proof reifiers, source receipts, world evidence and
math-expression identity edges share that traversal. The native carrier is frozen
once after its reasoning and diagnostic graphs are attached; the closure is never
parsed back from Turtle or frozen as an intermediate dataset. Resolved witness
heads also flow directly into native diagnostics and are reused for certificate
links. Anonymous proof nodes occupy a scope disjoint from source blanks, including
quoted and composite-literal occurrences. The completed carrier is shared by the
stage product. Stage timing metadata reports removed closure-reparse bytes and
witness wire bytes separately from required artifact bytes.

Four properties bind a grounding generator, all inherited rather than special:

1. **Determinism over a committed input.** The artifact is a pure function of authored
   source plus, where a lift executes a real front-end, a committed artifact embedded
   at compile time. `stage-math-producers` runs the shipped `gmeow_math_lift`
   entrypoint — the same one the `gmeow` CLI calls — over a real committed script, so
   the bundle carries the output of the actual parser rather than an imitation of it.
2. **Attached, not written.** The output is one or more named graphs on the carrier.
   A grounding artifact that appears under `generated/` is a *projection* of the
   bundle produced by fanout (§6), never the transport that got it there.
3. **Computed once.** The razor applies unchanged: a closure or a lift is computed a
   single time and read from the carrier by every downstream consumer.
4. **Diagnostics ride the same rail.** A grounding generator's findings attach to
   `GRAPH_DIAGNOSTICS` alongside every other stage's, so a grounding failure is a
   carrier-borne `gmeow:Finding` rather than a private error channel.

The consequence for a new grounding capability is that there is nothing to design
about its transport: register a stage, attach graphs, and it is in the bundle, in the
superset law, and under the conformance gate automatically. The open question for any
grounding slice is only *what to compute* — never *how to ship it*.

## 4. One terminal

Exactly **one** stage writes bytes. The terminal takes the fully-accumulated
carrier and **presents** it as the `gmeow.gts` bundle. It assembles nothing: it does
not load sources, union datasets, re-canonicalize graphs, or recompute any view —
those are the stages' work, already in the carrier. The terminal is the sole
serialization boundary in the build, and the bundle it emits is the single
content-addressed, signable artifact the project ships (Principles 14, 16).

A build with two writers — one that serializes and another that re-emits — has two
terminals, and is non-conforming. Presentation and writing are one stage.

## 5. The superset law

> **`gmeow.gts` is a superset of every build output.** For every committed artifact
> `o` under `generated/`, `o` is byte-reconstructible from `gmeow.gts` alone — either
> as a fold of one of its named graphs or as an extraction of one of its inline
> blobs.

This is not an aspiration; it is the definition of correctness for the spine. The
project is *a superset by design* and pursues *maximal utility*: the bundle is the
one place that holds everything, so the bundle must in fact hold everything. An
artifact that exists on disk but is **not** reconstructible from the bundle is a
defect — the build produced something the canonical deliverable does not carry, and
the one-direction-to-`gmeow.gts` dataflow is broken. When a stage's output is not in
the bundle, the fix is to make the stage **attach** it (§3), not to special-case the
file.

The law follows directly from Principle 4 (the bundle is the canonical source, the
flat files its projections) and Principle 5 (maximal superset). It also makes the
bundle *self-sufficient*: a consumer with only `gmeow.gts` can reconstruct every
view without the repository (Principle 13).

## 6. Fanout

Reconstructing the flat consumer tree is a **separate phase that runs after the
pipeline ends**. Fanout is pure projection: it reads `gmeow.gts` and writes files,
performing **no** computation, reasoning, or assembly. Because each extraction is
independent and reads the same immutable bundle, fanout is embarrassingly parallel
and is driven as ordinary build targets.

Every committed output is therefore the meeting of two halves:

- a **carrier contribution** — the producing stage attaches the bytes (or the graph
  they fold from) to the bundle during the pipeline; and
- a **fanout extraction** — the post-pipeline phase projects those bytes back to
  their path under `generated/`.

Fanout is built on the existing bundle-introspection surface — the `gmeow export`
consumer views, and the GTS structural verbs (`fold` for named graphs, `extract`
for a blob by digest, `unpack` for a files-profile archive). No output requires a
bespoke generator at fanout time; an output that cannot be produced by extraction
alone signals a §5 violation upstream, not a need for computation downstream.

### 6.1 Worked instance — the GMN-1 ecosystem projections

The GMN-1 (Grounded Model Notation) ecosystem is the two-halves law at work over a
family of related outputs, every one a projection of `gmeow.gts` (§5) and none
authored on disk. Two producers contribute to the carrier:

- **The lang projection producer** (inside `stage-mappings`) attaches the
  graph-derived GMN notation surfaces. Fanout extracts them under
  `generated/projections/lang/`: the formalism grammars `ebnf/gmn.ebnf` and
  `abnf/gmn.abnf`, and — keyed by the graph-resolved dialect major under
  `gmn1/v<major>/` — the constrained-decode grammars `gbnf/gmn.gbnf` and
  `lark/gmn.lark`, the math-grounded `token-metrics.ttl` (a `gmeow:Measurement`
  7-vector with a byte-fallback compression gate), the `verbalizations.ttl`
  GMN↔controlled-NL `lang:translationCorrespondence` pairs, and the per-example
  `*.gmn` witnesses.
- **`stage-gmn-training-corpus`** — a **new registered generator stage**, the
  first dedicated `lang:` generator stage (the `lang:` sibling of
  `stage-math-producers`) — consumes `stage-compile-logic` (the typecheck/prover
  lane) and `stage-mappings` (the glyph registry), enumerates well-typed GMN terms,
  rejection-samples each through five deterministic verifiers, and attaches the
  proof-carrying corpus (plus its typed rejections) as the bundle-internal named
  graph `graph/gmn-training-corpus` (dual-carriage, exactly like
  `graph/goal-directed`).

The ~500-token GMN-1 teachability primer is not a separate file: `stage-docs-render`
folds it into the `llms.txt` / `llms-full.txt` surfaces, and the MCP server serves
the identical bytes off the bundle alone as the `gmeow://ontology/gmn1-primer`
resource. Whole-ecosystem tamper-evidence is folded into `pack_root`; the superset
gate (§7) keeps every one of these paths byte-reconstructible from the bundle.

### 6.2 Worked instance — the medium dictionaries

The shipped zstd dictionaries are the two-halves law at work over a family whose
canonical form is **not a file at all**, which is what makes it the sharper worked
instance. Two producers contribute to the carrier:

- **`stage-archive-blobs`** folds the by-reference TAR archives once. It is the
  upstream half: the archive-rep corpus selectors resolve against *its* product, so
  a dictionary is trained over the same in-memory bytes the bundle is about to
  carry rather than over a previous build's copy on disk.
- **`stage-medium-dictionaries`** — the single producer of the bundle's
  dictionaries — trains each declared `gmeow:CompressionDictionary` over its
  declared `gmeow:DictionaryCorpus` selectors, measures each into a
  `gmeow:CompressionDictionaryRealization` (content digest, byte length, zstd
  `Dictionary_ID`, measured strategy, measured target length), and attaches the
  result as the build-time named graph `graph/medium-registry`. Its trained bytes
  ride an **internal `pipeline/` byte lane**, not a committed `generated/` file.
  The terminal then reads that product to pin every dictionary in the shipped
  segment header's in-band `"dct"` map and to seal one `gmeow:MediumEnvelope` per
  emitted frame.

**The in-band bytes are the canonical form, carried exactly once.** A dictionary's
shipping channel is the segment header a consumer primes from — that is where a
runtime store obtains one without a second artifact, a network fetch, or a repo
checkout. Routing the same bytes through the generated-opaque archive as well would
carry one high-entropy blob twice: it would re-fold a blob the snapshot already
carries (Constitution §18) and feed incompressible bytes to a compressor. So the
fanout family for these paths is neither `rdf-fanout` nor `opaque` but a third one,
**`header-dict`**: the committed path is reconstructed as the *verbatim bytes of one
entry of the header's `"dct"` map*.

Fanout therefore extracts, under `generated/medium/`:

- `generated/medium/gmeow-core-v1.zdict`
- `generated/medium/gmeow-lang-ast-v1.zdict`
- `generated/medium/gmeow-logic-v1.zdict`
- `generated/medium/gmeow-memory-compact-v1.zdict`
- `generated/medium/gmeow-memory-hot-v1.zdict`
- `generated/medium/gmeow-prooftrace-v1.zdict`

— one `header-dict` row per shipped dictionary — plus one path that is **not** a
header dictionary and travels as RDF because it *is* RDF (§5):

- `generated/medium/dictionary-effect.ttl`, the `rdf-fanout` fold of
  `graph/medium-measurement` re-rooted into its `graph/fanout` twin: the measured
  two-part code of every shipped dictionary, taken at the terminal because that is
  the one point at which the emission's whole blob frame set exists.

The gate keys the `header-dict` family on the `.zdict` suffix, so a `.ttl` under the
same prefix falls to `rdf-fanout` by construction rather than by exception.

**Superset-gate coverage (§7) is a bijection per family.** Every entry the shipped
segment header pins resolves to exactly one authored `header-dict` row, and every
authored `header-dict` row is claimed by exactly one pinned entry — a dictionary
added to the registry without its fanout row (or a row left behind after a
dictionary is retired) is a hard failure, not a smaller expectation. The
family-scoped bijection is what keeps the three families from vouching for each
other, and a separate clause hard-fails any `generated/medium/*.zdict` path that
also appears as a generated-opaque archive member, which is the one way the
"carried exactly once" law could be broken while every other assertion still held.

## 7. The conformance gate

The superset law (§5) is machine-checked, not trusted. The gate maps every committed
path under `generated/` to its carrier representative — a named graph or an inline
blob — and reconstructs it from `gmeow.gts`. A path with no representative, or whose
reconstruction does not match the committed bytes, is a hard failure: no skips, no
optional coverage, no degraded pass (the project's low/no-optionality, hard-fail
stance). The gate is the drift check that keeps the bundle honest as a superset, the
same way the existing drift gates keep the projections honest (Principle 7).

The read-only reconcile consumes stage products one at a time and handles the
declared terminal last. By the time the superset gate imports `gmeow.gts`, every
non-terminal committed-artifact store has already been compared and released. The
independent import therefore overlaps only the terminal bytes it is proving, not the
entire post-DAG artifact frontier; check and update retain the same bounded-residency
contract while exercising the same exact carrier.

## 8. Consequences and non-goals

- **Reason once, project many.** The reasoned closure is a single carrier graph. The
  committed closure file, the bundle's reasoning graph, and any documentation of
  inferred axioms are all projections of that one graph — never independent
  reasoning passes. Two artifacts that claim to be "the closure" but were reasoned
  separately are a razor (§3.2) violation, even if they happen to agree.
- **One serialization.** The carrier is serialized exactly once, by the terminal.
  Intermediate `gmeow.gts` emissions inside the pipeline are non-conforming; a side
  format a blob needs is produced from the in-memory carrier, not by emitting and
  re-parsing a temporary bundle.
- **Determinism.** Stage completion order does not affect the bundle: contributions
  fold by a stable key, so the emitted bytes are identical regardless of scheduling.
- **Not a format spec.** The on-the-wire layout of `gmeow.gts` (segments, blob
  encoding, signatures) is the GTS specification's domain, referenced from the
  README documentation map. This document governs only what the build puts *into* the
  bundle and how it comes back *out*.

## 9. Grounding

| This spec | Canon |
| --- | --- |
| Carrier is the sole transport; trim only at exit | *Maximal information flow*; Principle 4 |
| Bundle ⊇ every output; superset by design | *Maximal utility*; Principle 5 |
| Authored from slices; shipped to `gmeow.gts` | The standing one-direction dataflow |
| One terminal; one signed single-file bundle | Principles 14, 16 |
| Bundle self-sufficient for consumers | Principle 13 |
| Pin-on-attach; the conformance gate | Principle 7 |
| Flat files are projections of the bundle | Principle 4; Principle 17 (canon → views) |
| Hard-fail gate, no optionality | Low/no-optionality, hard-fail stance |
