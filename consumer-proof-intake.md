# Complete-canon consumer proof: source intake

This records source observations for the still-active 1768/1770 objective while
the existing combined producer build runs. It is not an independently reviewed
implementation plan, a new worktree, a PR, or a completion verdict. The full issue
1770 contract is in raw/issue1770-current.json. No scope is cut.

## Observed caller and representation contracts

1. gmeow-cli/src/lib.rs routes Commands::Prove directly to commands::prove.
   commands/prove.rs::load_theory reads caller-selected files, constructs and
   materializes CompositeDatasetView, then calls PreparedLogicSource::new and
   into_compiled. It does not establish either complete embedded-canon consumption
   or producer-precompiled-base reuse. Existing tiny external-prover protocol
   controls demonstrate the bounded protocol, not those missing behaviors.
2. logic-compile/frontend/emission.rs explicitly binds CompiledTheory and its
   AxiomSource, FormulaLowering and OwnerLowering traces to one immutable prepared
   dataset. Their native TermIds are invocation-local and intentionally are not
   durable serialized identities. Deriving serialization over that container
   would not implement authenticated portable proof loading.
3. LogicProgram already has a serialized native IR. SourceAdmission records
   portable unit IDs, canonical source anchors, exact documents, source/program/
   selection identities and every disposition. Its Formula/Rule indices refer to
   the exact program whose identity is recorded; composition must preserve or
   explicitly rebase those references rather than concatenate independently
   canonicalized collections blindly.
4. tptp/writer.rs already has the single classical exporter, deterministic
   predicate/constant encoding, FofSentence source-unit sets and a projection
   digest. Sentence bodies are strings; they are not a second native formula AST.
   Native request execution must share lowered IR with this exporter, not parse
   those strings back into a second logical representation.
5. The existing FofSemanticProfile only admits the selected default graph and
   flat classical scope. Contextual claims, selected unsupported owners,
   uncertified class-expression axioms and procedural identity guards are actual
   blockers in source_admission.rs. Removing those refusals or relabeling the
   profile is not the missing semantic implementation. The historical 327 count
   has not been measured on the current candidate and is not current evidence.
6. scripts/test-one-file-prover.sh currently checks copied-executable operation
   with a tiny caller theory and a stub prover, plus GMN asset loading. Its
   consistent verdict alone cannot establish complete-canon admission, actual
   native proof, a valid grounding diagnostic, or prepared-canon reuse. It denies
   access to its checkout during execution; run that acceptance only after the
   producer and other checkout-consuming jobs have finished.

## Required implementation boundaries

The producer must publish one complete typed prepared-canon product from its
existing shared source compilation. Attach it to the in-memory carrier, retain
portable source-unit/provenance/loss/contract accounting, bind the native payload
and profile, and carry it in gmeow.gts. The flat artifact must be fanout of that
bundle, not a second compilation or an out-of-band source file. Follow the
existing typed-publication restoration and source-hash contracts; no duplicate
full-corpus transport dataset or persisted unvalidated TermIds.

Semantic admission must give every selected canonical unit a sound typed
disposition. Class expressions need certified lowering; standpoint/modal claims
need their actual context semantics; selected owners and procedural guards need
their execution contract; the probabilistic profile needs its declared model.
An explicit supported proof profile can make only the sound exclusions allowed
by the issue. It must never become a hidden module selector or a silently smaller
canon. Preserve the original corpus and every semantic acceptance criterion.

The public library request API and installed CLI must compose typed premises and
goal with that immutable authenticated base in memory. Preserve blank-node scope
across separately prepared inputs, literal datatype/language/direction identity,
formula/rule index ownership and source provenance. Native execution and external
FOF export must share one lowering and symbol contract. Ordinary-domain data
requiring a correspondence must fail actionably when no certified grounding law
is supplied; it cannot produce a misleading successful proof.

## Required demonstrations, all still open

- Produce the entire selected canon with zero admission blockers for the admitted
  proof profile; retain a complete disposition inventory and exact identities.
- Exercise the documented navigation obligation through both the compact request
  API and canonical carrier, requiring the same intended verdict and evidence.
  Its ordinary-domain form must fail for the intended grounding reason.
- Execute the staged self-contained binary from a blinded working directory with
  source/generated inputs unavailable. Verify exact embedded bundle and prepared
  canon identities in its real proof receipt, not merely its info output.
- Prove request composition uses the producer-selected native base without a
  flattened-canon recompile, serialization/reparse round trip or test-side corpus
  production. Missing, stale, corrupt and wrong-profile products must fail closed.
- Record real prepared-base load/composition elapsed time and intermediate-byte
  evidence through the operation or hosted measurement lane. Use existing
  authenticated measurements for required footprint evidence; no host probes or
  resource wrappers are authorized.
- Add copy-paste CLI and library examples matching the actual supported request
  surface. Run focused product controls and the required final qualification,
  obtain independent/current-head review, then publish and merge through ghprsq.

The current optimized producer build and certificate qualification do not prove
any of these consumer-proof demonstrations. They remain part of the full goal.

## Probability-model source adjudication

Current slices/grounding/logic/module.ttl explicitly declares FullIndependence as
a ProbabilityModel. The compiler's graph_declares_probability_model inspects the
selected default graph through the shared structural typing helper; that helper
recognizes both RDF typing and native logic:instanceOf for canonical logic
classes. Thus the old missing-model count is not evidence that the current
authored module needs another model declaration. The actual newly produced
selection must be measured before assigning a fresh blocker count or cause.

The existing positive compiler regression now runs both typing spellings and
requires one emitted contract with the probabilistic measure retained. A new
synthetic TriG control places the only model in an unselected named graph and
requires the original hard contract diagnostic, under both spellings. The
existing missing-model and broader native-structural controls are included.
All four selected tests passed in 0.018s (nextest
8561645e-6675-4c38-9788-cf45144937ba); source lint and producer-independent
all-target strict Clippy passed. Raw probability-model-native-scope logs retain
the invocations and terminal evidence.

This is structural admission, not selection or execution of an actual probability
query. The semantic contract still requires an explicit model for that operation.
There is no new declaration, assumed independence, flattened graph, changed
production parser, or full-canon proof claim in this test-only pass.
