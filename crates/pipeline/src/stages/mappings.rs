// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `mappings` stage (P3): compile the alignment artifacts.
//!
//! All mapping artifact families are Rust-owned and wired directly here:
//!   * **SSSOM / FnO / EDOAL / SPARQL CONSTRUCT** → the oxigraph-free
//!     `gmeow-logic-compile` correspondence lowerings, driven by
//!     [`correspondence_lower::lower_all`]. EDOAL + SPARQL lower from one shared get-leg
//!     model, so the `spec-drift` invariant is gone by construction. SSSOM/EDOAL are
//!     content-equivalent to the historical emitter; SPARQL/FnO use a deterministic cell
//!     order (content-equal to the historical hash order). Outputs:
//!     `generated/mappings/*.sssom.tsv`, `generated/projections/functions.fno.ttl`,
//!     `generated/projections/*.edoal.ttl`, `generated/queries/*.rq`.
//!   * **Standpoint projections** → `purrdf::slice::emit_standpoint_sets(root, &vocab)` — the
//!     seven hand-authored `standpoint-*.rq` (six peer-model re-expressions:
//!     Standpoint-OWL 2, CRMinf, PROV-O, Web Annotation, schema.org Claim, BBC
//!     News; plus the legacy-modality projection), fixed template-coded SPARQL
//!     with no DSL input → `generated/queries/standpoint-*.rq`.
//!   * **DSL stats** → `purrdf::slice::emit_dsl_stats(root, &vocab)` — the committed,
//!     drift-gated counts summary (equivalences / functions / mapping_sets /
//!     projections / cells_by_set) → `generated/mappings/dsl-stats.json`.
//!
//! Every output is owned by the registered Rust generator and drift-gated from canonical
//! mapping sources.

use std::collections::BTreeMap;
use std::path::Path;

use crate::mapping_purity::lint_dsl_mapping_purity;
use gmeow_errors::{Finding, Location, Report, Severity};
use gmeow_logic_compile::ir::{Correspondence, DischargeVerdict};
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;
use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, project_correspondence,
};
use gmeow_logic_compile::projections::report::{ReportHeader, build_projection_report_from};
use purrdf::RdfSeverity;
use purrdf::slice::prefix_emit::{emit_core_prefixes, emit_jsonld_context};
use purrdf::slice::{
    CLAIM_VIEW_FILE, emit_claim_view, emit_list_functions, emit_standpoint_sets,
    lint_prefix_consistency,
};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::compile_logic::{
    LOGIC_PROJECTIONS_CHANNEL, LogicProjectionsChannel, PROJECTION_REPORT_PATH,
};
use crate::stages::correspondence_lower;

/// Directory (logical-path prefix) of the SSSOM TSV sets.
pub const SSSOM_DIR: &str = "generated/mappings";
/// Committed logical path of the FnO transform catalog.
pub const FNO_PATH: &str = "generated/projections/functions.fno.ttl";
/// Committed logical path of the EmotionML XML projection of the affect vocabulary.
pub const EMOTIONML_PATH: &str = "generated/projections/gmeow-affect.emotionml.xml";
/// Directory (logical-path prefix) of the EDOAL alignment Turtle files.
pub const EDOAL_DIR: &str = "generated/projections";
/// Directory (logical-path prefix) of the SPARQL CONSTRUCT projection queries
/// (also home to the seven standpoint `standpoint-*.rq` projections).
pub const QUERIES_DIR: &str = "generated/queries";
/// Committed logical path of the DSL surface-count summary.
pub const DSL_STATS_PATH: &str = "generated/mappings/dsl-stats.json";

/// Pipeline-native replacement for `purrdf::slice::emit_dsl_stats`: the external counter keyed
/// on the DELETED `gmeow:TermEquivalence` subjects (now zero), so the equivalence count + the
/// per-set breakdown come from the canonical native reader (`equivalence_cells`) over the SAME
/// merged DSL store (`dsl/mappings/**` + the slice `Mapping` artifacts). Functions, mapping
/// sets, and projections are still authored `gmeow:` nodes, counted as before. The JSON layout
/// is byte-identical to the retired emitter (`json.dumps(indent=1, sort_keys=True) + "\n"`).
fn emit_dsl_stats_native(
    root: &std::path::Path,
    catalog: Option<&purrdf::slice::SliceCatalog>,
    vocab: &purrdf::slice::SliceVocab,
) -> Result<String, gmeow_errors::Diag> {
    use gmeow_logic_compile::ingest::DslView;
    use gmeow_logic_compile::projections::sssom::equivalence_cells;
    use std::collections::{BTreeMap, BTreeSet};

    let stage_err = |m: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("dsl-stats emission failed: {m}"),
        })
    };
    let store =
        correspondence_lower::merge_dsl(root, catalog).map_err(|e| stage_err(e.to_string()))?;
    let view = DslView::new(store.as_ref());

    // cells_by_set + equivalences: every native alignment cell keyed by its sssomFile.
    let mut cells_by_set: BTreeMap<String, u64> = BTreeMap::new();
    let mut equivalences: u64 = 0;
    for cell in equivalence_cells(&view).map_err(|e| stage_err(e.to_string()))? {
        equivalences += 1;
        *cells_by_set.entry(cell.sssom_file).or_insert(0) += 1;
    }

    let functions = view.subjects_of_type(&vocab.projection_function()).len() as u64;
    let projections = view.subjects_of_type(&vocab.projection_mapping()).len() as u64;

    // mapping_sets: DISTINCT sssomFile target files (two MappingSet nodes on the same file
    // collapse to one), matching the retired emitter.
    let mut mapping_set_files: BTreeSet<String> = BTreeSet::new();
    for set_iri in view.subjects_of_type(&vocab.mapping_set()) {
        let file = view
            .object_literal(&set_iri, &vocab.sssom_file())
            .ok_or_else(|| stage_err(format!("mapping set {set_iri} missing sssomFile")))?;
        mapping_set_files.insert(file);
    }
    let mapping_sets = mapping_set_files.len() as u64;

    Ok(render_dsl_stats(
        &cells_by_set,
        equivalences,
        functions,
        mapping_sets,
        projections,
    ))
}

/// Render the DSL stats dict as `json.dumps(stats, indent=1, sort_keys=True) + "\n"` — the exact
/// byte layout the retired `purrdf::slice::emit_dsl_stats` produced.
fn render_dsl_stats(
    cells_by_set: &std::collections::BTreeMap<String, u64>,
    equivalences: u64,
    functions: u64,
    mapping_sets: u64,
    projections: u64,
) -> String {
    use std::fmt::Write as _;
    fn json_string(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        }
        out.push('"');
        out
    }
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(" \"cells_by_set\": {");
    if cells_by_set.is_empty() {
        out.push('}');
    } else {
        out.push('\n');
        let mut first = true;
        for (file, count) in cells_by_set {
            if !first {
                out.push_str(",\n");
            }
            first = false;
            let _ = write!(out, "  {}: {count}", json_string(file));
        }
        out.push_str("\n }");
    }
    out.push_str(",\n");
    let _ = writeln!(out, " \"equivalences\": {equivalences},");
    let _ = writeln!(out, " \"functions\": {functions},");
    let _ = writeln!(out, " \"mapping_sets\": {mapping_sets},");
    let _ = writeln!(out, " \"projections\": {projections}");
    out.push_str("}\n");
    out
}
/// Committed logical path of the importable named prefix set (§2).
pub const CORE_PREFIXES_PATH: &str = "generated/projections/core-prefixes.ttl";
/// Committed logical path of the JSON-LD `@context` (§2; replaces the
/// retired Python `jsonld_context.py` builder).
pub const JSONLD_CONTEXT_PATH: &str = "generated/context.jsonld";
/// Committed logical path of the first-class RDF list functions (§5).
pub const LIST_FUNCTIONS_PATH: &str = "generated/projections/list-functions.fno.ttl";
/// Committed logical path of the shape-grounding certificate ledger: one entry per
/// `logic:formalizes` record on the projected constraint surfaces
/// (`generated/shapes/constraint-shapes.ttl` + `generated/shapes/procedural-constraints.ttl`),
/// each carrying a preservation judgment RE-DERIVED this run by the certify/oracle
/// machinery ([`gmeow_validate::shape_grounding`]) — the loss-ledger doctrine applied to
/// the shape migration: "equivalence was proven" is a committed machine-checked fact,
/// not transient console output. Emitted as EXACTLY the canonical fold so it rides as an
/// RDF-fanout named graph (like the projection report).
pub const SHAPE_GROUNDING_LEDGER_PATH: &str = "generated/logic/shape-grounding-ledger.ttl";

const LEGACY_MAPPING_SOURCE_BANNER: &str = "from mapping-dsl/";
const CANONICAL_MAPPING_SOURCE_BANNER: &str = "from canonical mapping sources";

/// The mapping artifacts plus the per-correspondence loss ledger the SSSOM/FnO/EDOAL/
/// SPARQL lowerings produced (the residue set the projection report serializes).
pub struct CompiledMappings {
    /// Every emitted artifact, by logical path.
    pub artifacts: BTreeMap<String, Vec<u8>>,
    /// The per-correspondence loss ledger across all four dialects PLUS the live
    /// `lang:TranslationUnit` corpus rows (one per unit + one per language roll-up). The rows
    /// carry only identity/judgment; their drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The single loss store every correspondence dialect, the EmotionML emitter, and every
    /// `lang:` corpus interned their per-row drops into (unioned, keyed by target focus). The
    /// mappings stage unions it with the compile-logic loss store so the FINAL projection report
    /// reads every row's residue back from ONE substrate ledger.
    pub loss: LossLedger,
    /// The live translation-corpus N-Triples graph (`graph/lang-translation-corpus`):
    /// every `.po` catalog pair typed as a `lang:TranslationUnit` carrying a
    /// `logic:Correspondence` with an honestly-computed preservation judgment. Carried
    /// as a named graph by [`MappingsStage::run`], excluded from the reasoned EDB exactly
    /// like the projection-ledger graph.
    pub lang_translation_corpus: Vec<u8>,
    /// The total prose-lift corpus N-Triples graph (`graph/lang-form-corpus`): every
    /// distinct `@x-gmeow-english` source literal interned as a raw `lang:SurfaceForm`
    /// carrying its `logic:candidateSourceHash` and an exact surface-round-trip
    /// `logic:Correspondence`. Carried as a named graph by [`MappingsStage::run`], excluded
    /// from the reasoned EDB exactly like the translation-corpus graph.
    pub lang_form_corpus: Vec<u8>,
    /// The `lang:` projection corpus N-Triples graph (`graph/lang-projection-corpus`):
    /// one `lang:ProjectionEmission` per (source, target) — the honest per-emission
    /// preservation judgment of every lowering to an external linguistic ecosystem
    /// (OntoLex-Lemon, CoNLL-U, EBNF, ABNF) plus the lifted `lang:Grammar` structure it
    /// projects. Carried as a named graph by [`MappingsStage::run`], excluded from the
    /// reasoned EDB exactly like the other `lang:` corpus graphs.
    pub lang_projection_corpus: Vec<u8>,
    /// The compositional-lowering corpus N-Triples graph (`graph/lang-lowering-corpus`): the
    /// flagship quantified-SVO sentence lowered — one declared stage at a time — to its
    /// first-order `lang:CompositionalLowering` formula, each `lang:LoweringStage` carrying its
    /// `logic:preservationKind`. Carried as a named graph by [`MappingsStage::run`], excluded
    /// from the reasoned EDB exactly like the other `lang:` corpus graphs.
    pub lang_lowering_corpus: Vec<u8>,
    /// The docs-rendering corpus N-Triples graph (`graph/lang-docs-rendering-corpus`): the
    /// `.po`-derived documentation language trees re-typed as `lang:Rendering`
    /// (`lang:renderingDocsPage`) per non-English page, a `lang:Translation` per (page,
    /// language) pairing rolling up the page's `lang:TranslationUnit`s with a DERIVED
    /// document judgment, and the exec-docs English-only boundary recorded as a declared
    /// `lang:translationGap`. Carried as a named graph by [`MappingsStage::run`], excluded
    /// from the reasoned EDB exactly like the other `lang:` corpus graphs.
    pub lang_docs_rendering_corpus: Vec<u8>,
    /// The per-slice terminology-glossary N-Triples graph (`graph/lang-glossary-corpus`):
    /// every reviewed `.po` pair folded into a `gmeow:Glossary` of `gmeow:GlossaryEntry`
    /// records (term, source, rendering, sense anchor, and the `gmeow:glossaryUnit` join to
    /// its `lang:TranslationUnit`). Carried as a named graph by [`MappingsStage::run`],
    /// excluded from the reasoned EDB exactly like the other `lang:` corpus graphs.
    pub lang_glossary_corpus: Vec<u8>,
    /// The correspondence-laws N-Triples graph (`graph/correspondence-laws`): every authored
    /// `logic:Correspondence` re-projected with the EXECUTED lens-law discharge verdicts
    /// attached. Each per-`gmeow:ProjectionMapping` binding correspondence whose
    /// binding emits a put leg has its OWN get/put CONSTRUCT round-trip run through the native
    /// engine; the resulting `logic:LawClaim`s (SectionLaw / PutGet, `ObligationDischarged` on
    /// a clean lens) are attached and projected here. A binding with no put leg (Unsupported,
    /// e.g. `mapSiocTopic`) carries no discharged law. Carried as a named graph by
    /// [`MappingsStage::run`], excluded from the reasoned EDB like the other corpus graphs.
    pub correspondence_laws_corpus: Vec<u8>,
}

/// Compile all five mapping families (SSSOM + FnO + EDOAL + SPARQL + standpoint
/// projections) plus the DSL surface-count summary from `root`, returning
/// `{logical_path → bytes}`. The mappings stage is now complete.
pub fn compile_mappings(root: &Path) -> Result<CompiledMappings, gmeow_errors::Diag> {
    let vocab = gmeow_ns::gmeow_slice_vocab();
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // Discover the slice catalog ONCE, here, and share the single in-memory instance across
    // every source-slice consumer in this stage: the correspondence lowerings (Module +
    // Mapping merges) AND the total prose-lift corpus (every `@x-gmeow-english` literal,
    // all roles). Its artifact bytes are resident, so the `slices/` tree is walked once per
    // run — the total-lift universe is a projection of this composed source, never a second
    // independent disk read. `None` only when there is no `slices/` tree.
    let slices_dir = root.join("slices");
    let catalog = if slices_dir.is_dir() {
        Some(
            purrdf::slice::SliceCatalog::discover(&slices_dir, gmeow_ns::gmeow_slice_vocab())
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                        stage: "stage-mappings".to_string(),
                        message: format!("slice catalog discovery: {e}"),
                    })
                })?,
        )
    } else {
        None
    };

    // Prefix-consistency gate (§2): no authored source may shadow a registry
    // prefix with a foreign namespace — a shadow desynchronizes authored CURIEs from
    // the registry-driven shortener. Hard-fail before emitting any artifact
    // (no-optionality); this makes update / strict sync / `make check`
    // all reject a shadow.
    let prefix_problems = lint_prefix_consistency(root, &vocab).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("prefix-consistency lint failed: {e}"),
        })
    })?;
    if let Some(first) = prefix_problems.first() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!(
                "prefix-consistency: {} registry-prefix shadow(s); first: {}",
                prefix_problems.len(),
                first.message
            ),
        }));
    }

    // DSL mapping-purity gate: alignment linkage flows from slices. A native
    // alignment cell authored under `dsl/mappings/` is a linkage restatement in the
    // wrong place — it must live in the slice that defines its subject term.
    // Hard-fail before emitting any artifact (no-optionality); this makes update /
    // strict sync / `make check` reject a stray cell.
    let purity_problems = lint_dsl_mapping_purity(root).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("dsl mapping-purity gate failed: {e}"),
        })
    })?;
    if let Some(first) = purity_problems.first() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!(
                "dsl-linkage-purity: {} dsl/mappings file(s) author alignment linkage that must \
                 live in slices; first: {}",
                purity_problems.len(),
                first.message
            ),
        }));
    }

    // Consumer down-projection inventory gate: the authored `gmeow:ProjectionProfile`
    // rows must EQUAL the `dsl/mappings/projections/` tree, and each profile must still
    // declare at least its committed cell floor. Several profile files bind the same
    // `gmeow:profile` name and fold into ONE generated query, so no generated-artifact
    // inventory can see a deleted or hollowed-out consumer surface — this authored
    // second source can. Hard-fail before any artifact is emitted (no-optionality).
    crate::projection_profiles::check_projection_profile_inventory(root)?;

    // The four alignment dialects are now produced by the oxigraph-free
    // `gmeow-logic-compile` correspondence lowerings: SSSOM (1:1 lattice band), FnO
    // (transform functions), EDOAL + SPARQL-CONSTRUCT (one shared get leg, so
    // `spec-drift` is gone by construction). One native parse of the DSL + ontology
    // sources drives all four.
    let aligned = correspondence_lower::lower_all(root, catalog.as_ref()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("correspondence lowering failed: {e}"),
        })
    })?;

    // Closed target-catalog gate + per-family ratchet, read from the ontology-resident
    // `gmeow:CatalogFamily` registry (never a Rust list): every grounding correspondence's
    // `logic:targetEndpoint` must fall in exactly one REGISTERED external catalog family,
    // and every family's measured count must hold at or above its
    // `gmeow:catalogTargetMinimum`. Admitting a new external surface is therefore an
    // ontology edit, and losing rows from an admitted one is red rather than silent.
    {
        let families = crate::catalog_families::load_catalog_families(root)?;
        let targets: Vec<(&str, &str)> = aligned
            .correspondences
            .correspondences
            .iter()
            .filter(|c| c.grounding)
            .filter_map(|c| {
                c.target_endpoint
                    .as_deref()
                    .map(|target| (c.iri.as_str(), target))
            })
            .collect();
        let measured = crate::catalog_families::check_target_catalogs(
            &families,
            targets,
            "lowered grounding correspondences",
        )?;

        // Residue-ratchet carve-out gate. A family with no single grounding-slice owner
        // carries no `gmeow:ProjectionVocabulary`, so correspondences onto it enter no
        // residue count, no per-slice ceiling and no monotonicity ratchet. That absence
        // is legitimate but it is not free: it must be a REGISTERED, rationale-bearing,
        // COUNTED row, or the carve-out widens one correspondence at a time with nothing
        // ever measuring it. The guarded set is read from the ontology-resident
        // `gmeow:ProjectionVocabulary` registry (the rubric slice's `module.ttl`, already
        // in this stage's cache key via `module_files`) — never a Rust list, exactly as
        // the family registry is not one.
        let guarded_namespaces: std::collections::BTreeSet<String> =
            gmeow_slice_quality::load_repo_rubric(root)
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                        stage: "stage-mappings".to_string(),
                        message: format!(
                            "residue-ratchet carve-out gate: cannot load the guarded \
                             gmeow:ProjectionVocabulary registry: {e}"
                        ),
                    })
                })?
                .floors
                .vocabularies
                .iter()
                .flat_map(|vocab| vocab.namespaces.iter().cloned())
                .collect();
        let exemptions = crate::catalog_families::load_residue_exemptions(root)?;
        crate::catalog_families::check_residue_exemptions(
            &families,
            &exemptions,
            &guarded_namespaces,
            &measured,
            "lowered grounding correspondences",
        )?;
    }

    // Executed lens-law discharge: for every authored correspondence,
    // run its OWN per-binding get/put CONSTRUCT round-trip through the native engine, attach
    // the resulting `logic:LawClaim`s, and project the law-bearing set to a named graph. This
    // reads `aligned` before its dialect maps are moved out below. HARD-fails on any refuted
    // law (AC2) — an executed round-trip that does not hold is a real overclaim.
    let correspondence_laws_corpus = discharge_correspondence_laws(&aligned)?;
    for (filename, tsv) in aligned.sssom {
        artifacts.insert(format!("{SSSOM_DIR}/{filename}"), tsv.into_bytes());
    }
    artifacts.insert(FNO_PATH.to_string(), canon_fanout_ttl(&aligned.fno)?);
    for (filename, ttl) in aligned.edoal {
        artifacts.insert(format!("{EDOAL_DIR}/{filename}"), ttl.into_bytes());
    }
    for (filename, rq) in aligned.sparql {
        artifacts.insert(format!("{QUERIES_DIR}/{filename}"), rq.into_bytes());
    }
    // The inverse ingest leg: each `<profile>.put.rq` SPARQL CONSTRUCT emitted alongside
    // its forward `.rq`. ml-schema authors the ingest-claim terms today, so this writes the
    // ml-schema put leg and automatically tracks the emitter (the sole authority for the set).
    for (filename, put) in aligned.sparql_put {
        artifacts.insert(format!("{QUERIES_DIR}/{filename}"), put.into_bytes());
    }
    // The EmotionML XML projection of the affect category + dimension vocabularies. Its
    // many-to-one collapse row already rides in `aligned.ledger` (folded into the union
    // projection-report below), so writing the document is all that remains here.
    artifacts.insert(EMOTIONML_PATH.to_string(), aligned.emotionml.into_bytes());
    let mut ledger = aligned.ledger;
    // The single loss store: start from the correspondence dialects' + EmotionML's unioned
    // store, then fold every `lang:` corpus's store in below (each keyed by target focus, so
    // the union is byte-identical to a single fold).
    let mut loss = aligned.loss;

    // Live `lang:TranslationUnit` corpus (Principle 15 consumer wiring): type every
    // multilingual `.po` catalog pair as a first-class crossing carrying a
    // `logic:Correspondence` with an honestly-computed preservation judgment, and fold
    // its per-unit + per-document rows into the loss ledger. The RDF graph is carried as
    // a named graph by the stage `run` below (never a `generated/` file).
    let lang_corpus = crate::stages::lang_translation::build_corpus(root)?;
    ledger.extend(lang_corpus.ledger);
    loss.union(&lang_corpus.loss);
    let lang_translation_corpus = lang_corpus.ntriples;

    // Total prose lift (Gate 1): type every distinct `@x-gmeow-english` source literal as a
    // raw `lang:SurfaceForm` carrying its prose-hash and an exact surface-round-trip
    // `logic:Correspondence`, and fold the single honest corpus row into the loss ledger.
    // The RDF graph rides as a named graph by the stage `run` below (never a `generated/`
    // file), excluded from the reasoned EDB exactly like the translation corpus.
    let form_corpus = crate::stages::lang_form::build_corpus(catalog.as_ref())?;
    ledger.extend(form_corpus.ledger);
    loss.union(&form_corpus.loss);
    let lang_form_corpus = form_corpus.ntriples;

    // `lang:` projection corpus (the projection contract): lower the canonical `lang:`
    // model out to the external linguistic ecosystems through the correspondence-carrying
    // registry, fold every emission's honest preservation judgment into the loss ledger,
    // write the generated external artifacts, and carry the `lang:ProjectionEmission`
    // records as a named graph by the stage `run` below (never a `generated/` file).
    let projection_corpus = crate::stages::lang_projection::build_corpus(catalog.as_ref())?;
    ledger.extend(projection_corpus.ledger);
    loss.union(&projection_corpus.loss);
    let mut lang_projection_corpus = projection_corpus.ntriples;
    for (path, bytes) in projection_corpus.artifacts {
        artifacts.insert(path, bytes);
    }

    // Glossary interop lowerings (Principle 17): the OntoLex vartrans:translation + TBX
    // (ISO-30042) lowerings of the reviewed glossary crossings. The rendered byte artifacts
    // ride stage-export-glossary (REP_GENERATED); HERE we fold each target's honest
    // `lang:ProjectionEmission` record into graph/lang-projection-corpus (alongside the
    // sibling `lang:` emissions) and its lossy `SoundUnderApproximation` row into the loss
    // ledger. Both are lossy — every dropped construct is enumerated, so honest-lossy passes
    // the overclaim floor and silent-lossy reds the build.
    let glossary_lowering = crate::stages::lang_glossary::build_lowering_corpus(root)?;
    ledger.extend(glossary_lowering.ledger);
    loss.union(&glossary_lowering.loss);
    lang_projection_corpus.extend_from_slice(&glossary_lowering.emission_ntriples);

    // Compositional-lowering corpus (the "a sentence to a formula, compositionally" flagship):
    // lower the authored flagship quantified-SVO sentence to its first-order formula through the
    // native Montagovian lowering, fold the one honest exact ledger row, and carry the
    // `lang:CompositionalLowering` graph as a named graph by the stage `run` below (never a
    // `generated/` file), excluded from the reasoned EDB exactly like the other `lang:` corpora.
    let lowering_corpus = crate::stages::lang_lowering::build_corpus()?;
    ledger.extend(lowering_corpus.ledger);
    loss.union(&lowering_corpus.loss);
    let lang_lowering_corpus = lowering_corpus.ntriples;

    // Docs-tree re-typing (Principle 15 consumer wiring): re-type the EXISTING `.po`-derived
    // documentation language trees — one `lang:Rendering` (`lang:renderingDocsPage`) per
    // non-English page, one `lang:Translation` per (page, language) pairing that
    // `lang:rollsUpFrom` the page's live `lang:TranslationUnit`s with a DERIVED document
    // judgment, and the exec-docs English-only boundary as a declared `lang:translationGap` —
    // and fold its honest per-page + per-boundary rows into the loss ledger. The RDF graph
    // rides as a named graph by the stage `run` below (never a `generated/` file).
    let docs_rendering_corpus = crate::stages::lang_docs_rendering::build_corpus(root)?;
    ledger.extend(docs_rendering_corpus.ledger);
    loss.union(&docs_rendering_corpus.loss);
    let lang_docs_rendering_corpus = docs_rendering_corpus.ntriples;

    // The per-slice terminology glossary (term-grain of the translation corpus): every
    // reviewed `.po` pair folded into a `gmeow:Glossary`. Rides as a named graph by the
    // stage `run` below (never a `generated/` file). A pure derivation of the catalogs.
    let lang_glossary_corpus = crate::stages::lang_glossary::build_corpus(root)?.ntriples;

    // Docs-format grounding loss (A9/F2): fold the four documentation output formats'
    // (site / mdbook / print PDF / snippets) dropped-capability rows into the single loss
    // ledger, mirroring the lang: corpora. Blob-free — a pure function of
    // `gmeow_docs::formats`, the SAME table the print PDF's loss appendix reads, so the
    // appendix ↔ ledger join holds by construction. The RDF grounding graph (which
    // additionally content-addresses the packed docs blobs) rides in the carrier stage,
    // the only point those blob digests exist.
    crate::stages::docs_format_rendering::fold_docs_format_loss(&mut ledger, &mut loss);

    // Governance-floors projection loss (P17): the two slice-quality floor TSVs are a
    // sound under-approximation of the ontology-resident gmeow:AxisFloorCommitment /
    // gmeow:SliceTierFloor individuals — every emitted row is entailed, and the dropped
    // reifier identity + annotation coat are recorded as residue, never silently dropped.
    // Blob-free (a pure structural judgment), folded exactly like the docs-format corpus.
    crate::stages::governance_floors::fold_governance_floors_loss(&mut ledger, &mut loss);

    // Projection-ceilings projection loss (P17): the two projection-vocabulary ratchet
    // TSVs are a sound under-approximation of the ontology-resident
    // gmeow:ProjectionCeilingCommitment / gmeow:ProjectionVocabulary individuals — every
    // emitted row is entailed, and the dropped reifier identity + annotation coat are
    // recorded as residue, never silently dropped. Blob-free, folded exactly like the
    // governance-floors corpus.
    crate::stages::projection_ceilings::fold_projection_ceilings_loss(&mut ledger, &mut loss);

    // Standpoint projections — the seven fixed `standpoint-*.rq` queries (template-coded;
    // no DSL input).
    let standpoint = emit_standpoint_sets(root, &vocab).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("standpoint emission failed: {e}"),
        })
    })?;
    for (filename, rq) in standpoint {
        artifacts.insert(
            format!("{QUERIES_DIR}/{filename}"),
            normalize_mapping_source_banner(rq).into_bytes(),
        );
    }

    // Observation union view — the internal gmeow→gmeow `observation-claim-view.rq`
    // CONSTRUCT that materialises the legacy Observation / StandpointClaim query
    // surface from the canonical ClaimToken layer (no DSL input).
    artifacts.insert(
        format!("{QUERIES_DIR}/{CLAIM_VIEW_FILE}"),
        normalize_mapping_source_banner(emit_claim_view(&vocab)).into_bytes(),
    );

    // DSL surface-count summary — the committed, drift-gated counts JSON. Reimplemented
    // pipeline-side (the external emit_dsl_stats counted the deleted gmeow:TermEquivalence
    // subjects): the equivalence count now comes from the canonical native reader.
    let dsl_stats = emit_dsl_stats_native(root, catalog.as_ref(), &vocab)?;
    artifacts.insert(DSL_STATS_PATH.to_string(), dsl_stats.into_bytes());

    // Prefix-set projections (§2) — both derived from the single
    // PREFIX_REGISTRY authority: the importable `gmeow:CorePrefixes` SHACL set
    // and the JSON-LD `@context`. Deterministic by construction (const-derived),
    // so they ride the `generated/` drift gate and fold into `gmeow.gts` exactly
    // like the FnO catalog, with no new pipeline stage. `emit_core_prefixes`
    // never emits `skos:definition`/`rdfs:isDefinedBy`/`gmeow:graphBoxRole` for
    // the T-Box `owl:Ontology` header it mints — complete exactly those three.
    artifacts.insert(
        CORE_PREFIXES_PATH.to_string(),
        canon_fanout_ttl(&complete_core_prefixes_abox(
            &emit_core_prefixes(&vocab),
            &vocab,
        ))?,
    );
    artifacts.insert(
        JSONLD_CONTEXT_PATH.to_string(),
        emit_jsonld_context(&vocab).into_bytes(),
    );

    // First-class RDF list functions (§5) — six FnO primitives backed by the
    // reasoning layer's recursive rdf:List resolution. Fixed content, deterministic;
    // folds into gmeow.gts like the FnO catalog. `emit_list_functions` (like the
    // FnO catalog's `to_quads`) never emits `rdfs:isDefinedBy`/`gmeow:graphBoxRole`
    // for its A-Box `fno:Function`/`fno:Output`/`fno:Parameter` individuals —
    // complete them the same way `logic-compile`'s `fno.rs` does.
    artifacts.insert(
        LIST_FUNCTIONS_PATH.to_string(),
        canon_fanout_ttl(&complete_list_functions_abox(
            &emit_list_functions(&vocab),
            &vocab,
        ))?,
    );

    Ok(CompiledMappings {
        artifacts,
        ledger,
        loss,
        lang_translation_corpus,
        lang_form_corpus,
        lang_projection_corpus,
        lang_lowering_corpus,
        lang_docs_rendering_corpus,
        lang_glossary_corpus,
        correspondence_laws_corpus,
    })
}

fn normalize_mapping_source_banner(query: String) -> String {
    query.replace(
        LEGACY_MAPPING_SOURCE_BANNER,
        CANONICAL_MAPPING_SOURCE_BANNER,
    )
}

/// Discharge each authored correspondence's lens law by EXECUTION and project the
/// law-bearing set to N-Triples.
///
/// For each `logic:Correspondence` minted from a `gmeow:ProjectionMapping` binding, join its
/// `(cell IRI = get_leg, profile)` against the per-binding SPARQL fragments and — when the
/// binding emits BOTH a get and a put CONSTRUCT — run
/// [`crate::correspondence_law::discharge_laws`] over that ONE correspondence's OWN fragment
/// pair (the per-profile UNION query is the wrong unit — it mixes recoverable and
/// non-recoverable branches, so `put∘get` always drops). The returned `LawClaimIr`s are
/// attached (merged with any authored ingest claims) via [`Correspondence::new`], which
/// sorts + dedups them.
///
/// A correspondence whose binding emits NO put fragment (Unsupported — e.g. `mapSiocTopic`),
/// or a native alignment cell (no profile), is left untouched: it carries no
/// discharged section law, which is exactly the intended exclusion (AC3). A non-injective
/// rung yields no claim (`discharge_laws` returns empty), so it too passes through.
///
/// AC2 hard-fail: after attaching, any `ObligationViolated` verdict is a REAL overclaim (the
/// executed round-trip does not hold) and HARD-fails the stage — never shipped.
fn discharge_correspondence_laws(
    aligned: &correspondence_lower::CorrespondenceArtifacts,
) -> gmeow_errors::Result<Vec<u8>> {
    let stage_err = |message: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message,
        })
    };

    let mut rebuilt: Vec<Correspondence> = Vec::new();
    for corr in &aligned.correspondences.correspondences {
        // Only a per-profile binding correspondence knows its profile (a native alignment
        // cell is absent from the map); its `get_leg` is the pattern-bearing cell IRI.
        let fragment_pair = match (
            aligned.correspondence_profiles.get(&corr.iri),
            corr.get_leg.as_deref(),
        ) {
            (Some(profile), Some(cell_iri)) => aligned
                .sparql_fragments
                .get(&(cell_iri.to_owned(), profile.clone())),
            _ => None,
        };
        // No fragment pair, or a get fragment with no put leg (Unsupported binding): leave
        // the correspondence untouched — it claims no executed law (AC3).
        let Some((get_rq, Some(put_rq))) = fragment_pair.map(|(g, p)| (g, p.as_ref())) else {
            rebuilt.push(corr.clone());
            continue;
        };
        let claims = crate::correspondence_law::discharge_laws(get_rq, put_rq, corr.morphism_class);
        if claims.is_empty() {
            // A non-injective rung permits no section/put-get law: nothing to attach.
            rebuilt.push(corr.clone());
            continue;
        }
        let mut merged = corr.law_claims.clone();
        merged.extend(claims);
        let mut law_bearing = Correspondence::new(
            corr.iri.clone(),
            corr.relation,
            corr.morphism_class,
            corr.morphism_kind,
            corr.mnemomorphic,
            corr.determinacy,
            corr.get_leg.clone(),
            corr.put_leg.clone(),
            merged,
            corr.confidence,
            corr.evidence_strength,
            corr.weight,
            corr.probability,
            corr.according_to.clone(),
            // Rebuild: carry the authored per-correspondence preservation judgment forward.
            corr.preservation,
        )
        .map_err(|e| stage_err(format!("law-bearing correspondence <{}>: {e}", corr.iri)))?;
        if let (Some(source), Some(target)) = (&corr.source_endpoint, &corr.target_endpoint) {
            law_bearing = law_bearing
                .with_endpoints(source.clone(), target.clone())
                .map_err(|e| {
                    stage_err(format!(
                        "law-bearing correspondence <{}> endpoints: {e}",
                        corr.iri
                    ))
                })?;
        }
        if corr.grounding {
            law_bearing = law_bearing.as_grounding();
        }
        law_bearing = law_bearing
            .with_recovery_cases(corr.recovery_cases.clone())
            .map_err(|e| {
                stage_err(format!(
                    "law-bearing correspondence <{}> recovery cases: {e}",
                    corr.iri
                ))
            })?;
        rebuilt.push(law_bearing);
    }

    // AC2 hard-fail: an executed lens-law refutation is a real overclaim — never shipped.
    for corr in &rebuilt {
        for claim in &corr.law_claims {
            if claim.verdict == DischargeVerdict::ObligationViolated {
                return Err(stage_err(format!(
                    "correspondence <{}> refuted lens law logic:{} (ObligationViolated): the \
                     executed put∘get round-trip over its own get/put CONSTRUCT does not hold — \
                     a real overclaim, not something to suppress",
                    corr.iri,
                    claim.law.as_str(),
                )));
            }
        }
    }

    // Re-project the now-law-bearing correspondence set (reusing the existing
    // `logic:hasLawClaim` emission). Deterministic: `project_correspondence` sorts + dedups.
    let program = CorrespondenceProgram::new(
        rebuilt,
        aligned.correspondences.caveats.clone(),
        aligned.correspondences.preservation,
    )
    .with_leg_programs(aligned.correspondences.leg_programs.clone());
    Ok(project_correspondence(&program).into_bytes())
}

/// The A→B authorization set computed straight from `root`: the `gmeow:ProjectionMapping` cell
/// IRIs whose EXECUTED lens-law discharge carried an `ObligationDischarged` `logic:SectionLaw`.
///
/// This drives the SAME [`discharge_correspondence_laws`] the mappings stage folds into
/// `graph/correspondence-laws`, so the set the up-projection executor consumes agrees with the
/// shipped bundle by construction (single source of truth). The consumer that reads the folded
/// bundle graph ([`crate::projections::discharged_section_cells_from_bundle`]) yields the identical
/// set; this root-recompute path is for the acceptance harness, which reads fresh `root` inputs.
pub fn discharged_section_cells_from_root(
    root: &Path,
) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    let slices_dir = root.join("slices");
    let catalog = if slices_dir.is_dir() {
        Some(
            purrdf::slice::SliceCatalog::discover(&slices_dir, gmeow_ns::gmeow_slice_vocab())
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                        stage: "stage-mappings".to_string(),
                        message: format!("slice catalog discovery: {e}"),
                    })
                })?,
        )
    } else {
        None
    };
    let aligned = correspondence_lower::lower_all(root, catalog.as_ref()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("correspondence lowering failed: {e}"),
        })
    })?;
    let corr_laws = discharge_correspondence_laws(&aligned)?;
    let nt = String::from_utf8(corr_laws).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("correspondence-laws graph is not UTF-8: {e}"),
        })
    })?;
    crate::up_projection_gates::discharged_section_cells_from_corpus(&nt).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("extract discharged section cells: {e}"),
        })
    })
}

/// Emit a Turtle RDF projection as EXACTLY the canonical fold (shared prefix
/// authority, no banner) so it rides as an RDF-fanout named graph and the superset
/// gate reconstructs it byte-for-byte.
fn canon_fanout_ttl(body: &str) -> Result<Vec<u8>, gmeow_errors::Diag> {
    canon_fanout_ttl_bytes(body.as_bytes())
}

fn canon_fanout_ttl_bytes(body: &[u8]) -> Result<Vec<u8>, gmeow_errors::Diag> {
    purrdf::turtle_normalize::canonical_turtle(body, &crate::stages::superset::rdf_prefixes())
        .map(String::into_bytes)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-mappings".to_string(),
                message: format!("canonicalize RDF projection: {e}"),
            })
        })
}

/// Complete `gmeow:CorePrefixes`' A-Box structural-annotation gap:
/// `purrdf::slice::prefix_emit::emit_core_prefixes` mints the importable SHACL
/// prefix set as `a owl:Ontology` with an `rdfs:label`/`rdfs:comment` (bare `@en`,
/// retagged to the `x-gmeow-english` carrier here by [`retag_core_prefixes_english`])
/// but NEVER a `skos:definition`, `rdfs:isDefinedBy`, or `gmeow:graphBoxRole`. `CorePrefixes` is a T-Box document (an ontology header
/// describing a declared prefix set), never an assertional individual, so it
/// carries [`gmeow_errors::abox::BOX_TBOX`] rather than the A-Box
/// [`gmeow_errors::abox::BOX_ABOX`] every other completion in this module uses.
/// Appended as full-IRI Turtle triples so `canon_fanout_ttl` folds them into the
/// same canonicalization pass as `body`.
/// Retag the bare public `@en` tag `emit_core_prefixes` mints on the
/// `gmeow:CorePrefixes` `rdfs:label` / `rdfs:comment` to the `x-gmeow-english`
/// carrier tag. `CorePrefixes` is an INTERNAL importable SHACL prefix set — not a
/// Principle-17 external lowering — so its GMEOW-authored English prose owes the
/// carrier tag like every other internal term (only its `skos:definition`, minted
/// below, already carries it). Keyed on the label/comment predicate so no other
/// literal is touched; the object literal value never itself contains `"@en`.
fn retag_core_prefixes_english(body: &str) -> String {
    body.lines()
        .map(|line| {
            let annotation = line.contains("#label>")
                || line.contains("#comment>")
                || line.contains("rdfs:label")
                || line.contains("rdfs:comment");
            if annotation {
                line.replace(
                    "\"@en",
                    &format!("\"@{}", gmeow_errors::abox::X_GMEOW_ENGLISH),
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn complete_core_prefixes_abox(body: &str, vocab: &purrdf::SliceVocab) -> String {
    let body = retag_core_prefixes_english(body);
    let subject = vocab.core_prefixes_iri();
    const DEFINITION: &str = "The importable SHACL prefix set (`sh:declare`) covering every \
        namespace prefix in the mapping-DSL prefix registry; reference it from a SHACL shape \
        via `sh:prefixes gmeow:CorePrefixes` instead of redeclaring prefixes per shape.";
    format!(
        "{body}\n\
         <{subject}> <{definition_pred}> \"{definition}\"@{carrier} .\n\
         <{subject}> <{is_defined_by_pred}> <{ontology_iri}> .\n\
         <{subject}> <{graph_box_role_pred}> <{box_tbox}> .\n",
        definition_pred = gmeow_errors::abox::SKOS_DEFINITION,
        definition = gmeow_errors::render::nq_escape(DEFINITION),
        carrier = gmeow_errors::abox::X_GMEOW_ENGLISH,
        is_defined_by_pred = gmeow_errors::abox::RDFS_IS_DEFINED_BY,
        ontology_iri = vocab.ontology_iri(),
        graph_box_role_pred = gmeow_errors::abox::GRAPH_BOX_ROLE,
        box_tbox = gmeow_errors::abox::BOX_TBOX,
    )
}

/// Complete the first-class RDF list-functions catalog's A-Box structural-
/// annotation gap: `purrdf::slice::list_functions::emit_list_functions` routes
/// through the SAME `purrdf::fno::to_quads` serializer `logic-compile`'s FnO
/// catalog uses, so it inherits the identical gap — `rdfs:isDefinedBy`/
/// `gmeow:graphBoxRole` are NEVER emitted for any subject. Unlike the FnO
/// catalog, every `fno:Function`/`fno:Output`/`fno:Parameter` individual here
/// already carries a real `rdfs:label`/`skos:definition` from the catalog model
/// (`list_functions_catalog` populates both fields for all three), so only the
/// two structural predicates need completing. Re-derives the catalog MODEL
/// (public, deterministic, side-effect-free) purely to enumerate the subject
/// IRIs — `body`'s serialized text (with its own retag/tag choices) is passed
/// through unmodified, only appended to, so this never diverges from what
/// `emit_list_functions` actually emitted.
fn complete_list_functions_abox(body: &str, vocab: &purrdf::SliceVocab) -> String {
    let catalog = purrdf::slice::list_functions::list_functions_catalog(vocab);
    let mut subjects: Vec<&str> = Vec::new();
    for func in &catalog.functions {
        subjects.push(&func.iri);
        subjects.push(&func.output.iri);
    }
    for param in &catalog.params {
        subjects.push(&param.iri);
    }
    let mut out = body.to_owned();
    for subject in subjects {
        out.push_str(&format!(
            "<{subject}> <{is_defined_by_pred}> <{graph}> .\n\
             <{subject}> <{graph_box_role_pred}> <{box_abox}> .\n",
            is_defined_by_pred = gmeow_errors::abox::RDFS_IS_DEFINED_BY,
            graph = catalog.document_iri,
            graph_box_role_pred = gmeow_errors::abox::GRAPH_BOX_ROLE,
            box_abox = gmeow_errors::abox::BOX_ABOX,
        ));
    }
    out
}

/// Assemble the FINAL `generated/logic/projection-report.ttl` over the UNION of the
/// logic projection rows (handed over from compile-logic via [`LogicProjectionsChannel`])
/// and the correspondence-calculus loss ledger.  This is the ONE place the committed
/// report is serialized; it funnels through the single `build_projection_report_from`
/// routine, so the seven whole-program logic rows stay byte-identical — only the added
/// correspondence rows differ.
fn build_union_report(
    header: ReportHeader,
    channel: &LogicProjectionsChannel,
    correspondence_ledger: &[ProjectionResult],
    correspondence_loss: &LossLedger,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut rows: Vec<ProjectionResult> = channel.projections.clone();
    rows.extend(correspondence_ledger.iter().cloned());
    // Reconstruct the compile-logic loss store from the nodes carried across the JSON channel,
    // then union the correspondence + lang loss store in. Every row above (logic projections +
    // correspondence/lang rows) reads its residue back from this ONE store, byte-identically to
    // the pre-split single-ledger serialization.
    let mut loss = LossLedger::from_nodes(channel.loss_nodes.clone());
    loss.union(correspondence_loss);
    let report = build_projection_report_from(header, &rows, &loss).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message: format!("projection-report assembly: {e}"),
        })
    })?;
    Ok(report.into_bytes())
}

/// Assemble the shape-grounding certificate ledger over THIS run's projected constraint
/// surfaces: `generated/shapes/procedural-constraints.ttl` (off the consumed
/// `stage-compile-logic` product) and `generated/shapes/constraint-shapes.ttl` (off the
/// consumed `stage-export-constraint-shapes` product). For every `logic:formalizes`
/// record the shared machinery ([`gmeow_validate::shape_grounding`]) RE-DERIVES the
/// preservation judgment — the oracle read, the executable-SHACL parse, and the
/// lift/certify round-trip all run afresh each regenerate — and the ledger is emitted as
/// EXACTLY the canonical fold so it rides as an RDF-fanout named graph (superset gate).
///
/// Hard-fail semantics (no-optionality): a missing surface, an underivable record, or an
/// empty record scan (the surfaces are never record-free) is a stage error.
fn build_shape_grounding_ledger(
    upstream: &BTreeMap<String, crate::node::StageProduct>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let stage_err = |message: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message,
        })
    };
    let surface = |stage: &str,
                   path: &str|
     -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
        let bytes = upstream
            .get(stage)
            .and_then(|p| p.artifact(path))
            .ok_or_else(|| {
                stage_err(format!(
                    "shape-grounding ledger: missing {path} in the {stage} product \
                     (fail-closed, no stale disk read)"
                ))
            })?;
        purrdf::parse_dataset(bytes, "text/turtle", None)
            .map_err(|e| stage_err(format!("shape-grounding ledger: parse {path}: {e}")))
    };
    // Deterministic surface order: the FOL-constraint surface, then the
    // procedural-constraint surface (the certificates are re-sorted by record IRI, so
    // the order only scopes the duplicate-record ambiguity check).
    let surfaces = vec![
        surface(
            "stage-export-constraint-shapes",
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
        )?,
        surface(
            "stage-compile-logic",
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
        )?,
    ];
    let certs = gmeow_validate::shape_grounding::derive_grounding_certificates(&surfaces)
        .map_err(|e| stage_err(format!("shape-grounding ledger: {e}")))?;
    // Fail closed: both surfaces carry logic:formalizes records by construction, so an
    // empty certificate set means the record scan silently missed them.
    if certs.is_empty() {
        return Err(stage_err(
            "shape-grounding ledger: the projected constraint surfaces yielded ZERO \
             logic:formalizes records — the record scan missed the surfaces (fail-closed)"
                .to_string(),
        ));
    }
    canon_fanout_ttl(&gmeow_validate::shape_grounding::render_grounding_ledger(
        &certs,
    ))
}

/// Compute the FINAL report header correspondence/uplift counts. This is the SINGLE owner
/// of `correspondenceCount` / `lawfulUpliftCount` / `claimedUpliftCount`: it composes the
/// curated affine-gate BASE compile-logic ships on the channel (`base_correspondence_count`
/// / `base_lawful_uplift_count`) with the gate-derived 591-term up-projection audit. The
/// `correspondenceCount` becomes the curated affine-gate cell PLUS every audited external
/// term; `lawfulUpliftCount` the base lawful count PLUS the proved tier (round-trip
/// verified); `claimedUpliftCount` the claimed tier (alignment-asserted, not proved; base 0).
/// The audit headline thus becomes a gate-verdict ledger in the canonical loss ledger, not a
/// heuristic bucket count.
///
/// The incoming `header`'s count fields ride the channel as 0 (compile-logic no longer writes
/// them), so the composition is a clean assignment (`=`) that documents mappings owns the
/// value; the base arrives explicitly via the two `base_*` parameters.
///
/// Inputs are gathered the same way the `gmeow up-projection-audit` CLI does: the freshly
/// generated SSSOM (in-memory), the authored projection cells under `root`, and the vendored
/// coverage corpus (`tests/fixtures/coverage/external/{bii,paudley}.ttl`). The corpus is fixed
/// real RDF, so the folded counts are deterministic and ride the `generated/` drift gate.
fn fold_up_projection_audit(
    root: &Path,
    artifacts: &BTreeMap<String, Vec<u8>>,
    mut header: ReportHeader,
    base_correspondence_count: usize,
    base_lawful_uplift_count: usize,
) -> Result<ReportHeader, gmeow_errors::Diag> {
    let stage_err = |message: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_string(),
            message,
        })
    };

    // The freshly-generated SSSOM (never the on-disk copy, which may be stale this run).
    let sssom_texts: Vec<String> = artifacts
        .iter()
        .filter(|(path, _)| path.starts_with(SSSOM_DIR) && path.ends_with(".sssom.tsv"))
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .collect();

    // The authored projection cells: dsl/mappings/projections/*.ttl + slices/**/mappings/*.ttl.
    let mut projection_paths: Vec<std::path::PathBuf> = Vec::new();
    collect_files_recursive(
        &root.join("dsl").join("mappings").join("projections"),
        &mut projection_paths,
    )?;
    let mut slice_files: Vec<std::path::PathBuf> = Vec::new();
    collect_files_recursive(&root.join("slices"), &mut slice_files)?;
    projection_paths.extend(
        slice_files
            .into_iter()
            .filter(|p| p.components().any(|c| c.as_os_str() == "mappings")),
    );
    projection_paths.retain(|p| p.extension().is_some_and(|e| e == "ttl"));
    projection_paths.sort();
    projection_paths.dedup();
    let projection_ttls: Vec<String> = projection_paths
        .iter()
        .map(|p| std::fs::read_to_string(p).map_err(|e| stage_err(format!("read {p:?}: {e}"))))
        .collect::<Result<_, _>>()?;

    // The vendored coverage corpus, converted Turtle→N-Triples natively.
    let mut corpus_nts: Vec<(String, String)> = Vec::new();
    for name in ["bii", "paudley"] {
        let path = root
            .join("tests")
            .join("fixtures")
            .join("coverage")
            .join("external")
            .join(format!("{name}.ttl"));
        let ttl =
            std::fs::read_to_string(&path).map_err(|e| stage_err(format!("read {path:?}: {e}")))?;
        let nt = crate::up_projection_corpus::ttl_to_nt(&ttl)
            .map_err(|e| stage_err(format!("corpus {name} ttl→nt: {e}")))?;
        corpus_nts.push((name.to_string(), nt));
    }

    let ledger =
        crate::up_projection_gates::gate_derived_audit(&sssom_texts, &projection_ttls, &corpus_nts)
            .map_err(|e| stage_err(format!("gate-derived up-projection audit: {e}")))?;

    // Single-owner composition: mappings COMPUTES the final counts as base + audit. The
    // incoming header carries 0 in these fields (compile-logic ships only the axiom/rule/
    // profile counts it owns), so `=` and `+=` are arithmetically identical here — `=`
    // documents that mappings is the sole writer of the final value.
    header.correspondence_count = base_correspondence_count + ledger.total();
    header.lawful_uplift_count = base_lawful_uplift_count + ledger.totals.proved;
    header.claimed_uplift_count = ledger.totals.claimed;
    Ok(header)
}

/// Compile mappings and fold their diagnostics into the native report.
///
/// This is the Rust-owned implementation behind the Python feedback surface:
/// Python remains the CLI/interface, while compilation, SSSOM validation, and
/// cross-layer projection linting all run through native Rust authorities.
pub fn compile_diagnostics_report(root: &Path) -> Report {
    let mut report = Report::new("mapping-compile");
    let artifacts = match compile_mappings(root) {
        Ok(compiled) => compiled.artifacts,
        Err(err) => {
            add_dsl_error(&mut report, err.to_string());
            return report;
        }
    };

    for (path, bytes) in artifacts
        .iter()
        .filter(|(path, _)| path.ends_with(".sssom.tsv"))
    {
        fold_sssom_findings(&mut report, path, bytes);
    }

    // The seven correspondence-stack soundness checks (the five alignment checks + the two
    // FnO back-end checks, incl. the sole native enforcer of Constitution Principle 5) run
    // through the oxigraph-free native pass
    // (`stages::correspondence_soundness::lint_correspondence_soundness`).
    match crate::stages::correspondence_soundness::lint_correspondence_soundness(root, false) {
        Ok(problems) => {
            for problem in problems {
                let mut finding = Finding::new(
                    match problem.severity.as_str() {
                        "ERROR" => Severity::Error,
                        "WARNING" => Severity::Warning,
                        "INFO" => Severity::Info,
                        _ => Severity::Warning,
                    },
                    format!("mapping-compile.{}", problem.check),
                    problem.message,
                )
                .with_tool("mapping-compile");
                if let Some(instance) = problem.instance {
                    finding.add_location(Location::new(None, None, None, Some(instance)));
                }
                report.add_finding(finding);
            }
        }
        Err(err) => {
            report.add_finding(
                Finding::new(
                    Severity::Warning,
                    "mapping-compile.projection-lint-skipped",
                    format!("projection lint findings not surfaced: {err}"),
                )
                .with_tool("mapping-compile"),
            );
        }
    }

    report
}

fn add_dsl_error(report: &mut Report, message: String) {
    report.add_finding(
        Finding::new(Severity::Error, "mapping-compile.dsl-error", message)
            .with_tool("mapping-compile"),
    );
}

fn fold_sssom_findings(report: &mut Report, path: &str, bytes: &[u8]) {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            report.add_finding(sssom_finding(
                path,
                None,
                format!("SSSOM artifact is not UTF-8: {err}"),
                "parse",
                "Utf8",
            ));
            return;
        }
    };

    let set = match purrdf::sssom::parse_tsv(text) {
        Ok(set) => set,
        Err(diag) => {
            report.add_finding(sssom_finding(path, None, diag.message, "parse", diag.code));
            return;
        }
    };

    // Structural SSSOM parse failures returned above are already folded into the
    // report; semantic validation diagnostics use the closed RDF severity enum.
    for diag in purrdf::sssom::validate(&set) {
        if diag.severity == RdfSeverity::Error {
            report.add_finding(sssom_finding(
                path,
                diag.instance,
                diag.message,
                diag.check,
                diag.code,
            ));
        }
    }
}

fn sssom_finding(
    path: &str,
    instance: Option<String>,
    message: String,
    check: impl Into<String>,
    code: impl Into<String>,
) -> Finding {
    let mut finding = Finding::new(Severity::Error, "mapping-compile.sssom", message)
        .with_tool("mapping-compile");
    let location = Location::new(Some(path.to_owned()), None, None, instance);
    finding.add_location(location);
    finding.detail = Some(format!("check={} code={}", check.into(), code.into()));
    finding
}

/// Recursively collect every regular file under `dir` into `out` (fail-fast on a
/// `read_dir` entry error — a transient FS error must surface, not silently drop
/// a mapping source). A missing directory yields nothing.
fn collect_files_recursive(
    dir: &Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), gmeow_errors::Diag> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files_recursive(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `mappings` pipeline stage — complete: all five mapping families (SSSOM +
/// FnO + EDOAL + SPARQL + standpoint projections) plus the DSL surface-count summary,
/// and the FINAL projection-report loss ledger (logic rows ∪ correspondence rows).
pub struct MappingsStage {
    consumes: Vec<String>,
}

impl MappingsStage {
    /// Construct the stage. It consumes the compile-logic product to obtain the logic
    /// projection rows + report-header counts it unions with the correspondence ledger
    /// when assembling the final `generated/logic/projection-report.ttl` (plus the
    /// procedural-constraint surface the shape-grounding ledger re-certifies), and the
    /// constraint-shapes export leaf for the FOL-constraint surface of the same ledger.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-export-constraint-shapes".to_string(),
            ],
        }
    }
}

impl Default for MappingsStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for MappingsStage {
    fn id(&self) -> &str {
        "stage-mappings"
    }
    fn consumes(&self) -> &[String] {
        // Reads dsl/mappings + slice mapping cells from the root, the compile-logic
        // product (the logic projection rows + header counts for the FINAL projection
        // report, plus the procedural-constraint surface the shape-grounding ledger
        // re-certifies), AND the constraint-shapes export leaf (the FOL-constraint
        // surface of the same ledger — THIS run's bytes, never a stale disk read).
        &self.consumes
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v12: attach the substrate SBOM — the reconciliation A-Box
        // projected through the compiled spdx.rq into graph/substrate-sbom. Bump busts the
        // stage cache so the new named graph is emitted on cached inputs.
        // v11: added the shape-grounding certificate ledger
        // (generated/logic/shape-grounding-ledger.ttl) — every logic:formalizes record's
        // preservation judgment re-derived per regenerate over the fresh constraint
        // surfaces. Bump busts the stage cache so the ledger is emitted on cached inputs.
        "mappings.v12-substrate-sbom"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // Raw source read: the alignment artifacts compile from the `dsl/mappings/`
        // tree plus the per-slice mapping cells in the slice modules — none of which
        // any upstream product reflects. The vendored coverage corpus
        // (tests/fixtures/coverage/external/*.ttl) is also a raw source input that
        // feeds the committed audit ledger. Declare them ALL so any edit busts the
        // cache. `consumes() == []` (the leaf reads sources, not upstream products).
        let mut files = Vec::new();
        collect_files_recursive(&root.join("dsl").join("mappings"), &mut files)?;
        files.extend(crate::stages::source_load::module_files(root)?);
        // Several slice source surfaces are read DIRECTLY by this stage yet are reflected by no
        // upstream product, so each must bust the cache on edit:
        //   - slices/**/mappings/*.ttl — the Mapping cells compile_mappings
        //     (correspondence_lower::lower_all) and fold_up_projection_audit merge; module_files
        //     only declares each slice's module.ttl.
        //   - slices/**/grammars/*.ebnf and slices/**/examples/*.ttl — the grammar SOURCE surfaces
        //     and the lang: example A-boxes the lang: projection corpus (build_corpus) lowers FROM
        //     (OntoLex / CoNLL-U / TEI / NIF / SemAF / BCP-47); without these an edit to a
        //     projected grammar or example serves a stale projection past the drift gate.
        //   - slices/**/*.po — the documentation-language catalogs the docs re-typing reads.
        let mut slice_files: Vec<std::path::PathBuf> = Vec::new();
        collect_files_recursive(&root.join("slices"), &mut slice_files)?;
        files.extend(slice_files.into_iter().filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str());
            let in_dir = |name: &str| p.components().any(|c| c.as_os_str() == name);
            (ext == Some("ttl") && in_dir("mappings"))
                || ext == Some("ebnf")
                || (ext == Some("ttl") && in_dir("examples"))
                || ext == Some("po")
        }));
        for name in ["bii", "paudley"] {
            files.push(
                root.join("tests")
                    .join("fixtures")
                    .join("coverage")
                    .join("external")
                    .join(format!("{name}.ttl")),
            );
        }
        // The substrate SBOM projection reads the substrate
        // reconciliation A-Box's build INPUTS directly (manifests, lockfile, shipped
        // SUBSTRATE.txt stamps, prose), none of which any upstream product reflects — so
        // each must bust this stage's cache on edit, else a substrate pin change would
        // serve a stale SBOM.
        files.extend(crate::stages::substrate_graph::substrate_input_paths(root));
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let compiled = compile_mappings(input.root)?;
        let mut artifacts = compiled.artifacts;

        // Assemble the FINAL committed projection report over the UNION of the logic
        // projection rows (from compile-logic's in-memory channel) and the
        // correspondence loss ledger. Serialized ONCE through the single routine, so
        // the logic rows stay byte-identical and only correspondence rows are added.
        let channel_bytes = input
            .upstream
            .get("stage-compile-logic")
            .and_then(|p| p.artifact(LOGIC_PROJECTIONS_CHANNEL))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!(
                        "missing compile-logic projection channel ({LOGIC_PROJECTIONS_CHANNEL})"
                    ),
                })
            })?;
        let channel: LogicProjectionsChannel =
            serde_json::from_slice(channel_bytes).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-mappings".to_string(),
                    message: format!("decode logic-projections channel: {e}"),
                })
            })?;
        // Fold the gate-derived 591-term up-projection audit into the curated-cell header
        // counts, so the committed loss ledger carries the gate-verdict liftability statistic
        // . Then canonicalize the report TTL so `projection-report.ttl` is carried as
        // the fold of its named graph (superset gate), not an opaque byte lane.
        let header = fold_up_projection_audit(
            input.root,
            &artifacts,
            channel.header,
            channel.base_correspondence_count,
            channel.base_lawful_uplift_count,
        )?;
        let report = build_union_report(header, &channel, &compiled.ledger, &compiled.loss)?;
        artifacts.insert(
            PROJECTION_REPORT_PATH.to_string(),
            canon_fanout_ttl_bytes(&report)?,
        );

        // The shape-grounding certificate ledger: RE-DERIVE every `logic:formalizes`
        // record's preservation judgment against THIS run's projected constraint
        // surfaces — the procedural-constraint surface off the consumed compile-logic
        // product and the FOL-constraint surface off the consumed constraint-shapes
        // export leaf (never a stale disk read; PIPELINE_SPINE §3). A record whose
        // judgment cannot be derived is a stage error, never a skipped entry.
        artifacts.insert(
            SHAPE_GROUNDING_LEDGER_PATH.to_string(),
            build_shape_grounding_ledger(input.upstream)?,
        );

        // Carry the union of the RDF outputs (`.ttl` / `.nq` / `.nt` — the alignment
        // axioms / projections this stage contributes to compose) as the bundle's
        // frozen dataset; the non-RDF outputs (`.json`, `.jsonld`, `.tsv`, `.rq`) stay
        // byte-lane only. Each RDF artifact is parsed and the per-input datasets are
        // unioned (standardize-apart per input), so `gts_compose` folds in this one
        // dataset instead of re-parsing each byte artifact.
        // The default-graph union of the RDF outputs (the alignment axioms `gts_compose`
        // folds default-graph-only) PLUS the projection-report loss ledger re-rooted into
        // the carrier's `graph/projection-ledger` named graph, so the presenter reads the
        // ledger as a pure keyed fold (PIPELINE_SPINE §4) instead of re-parsing the byte
        // artifact. `gts_compose` folds only the default graph, so the named ledger graph
        // never pollutes the composed EDB.
        let rdf_dataset = mappings_rdf_dataset(&artifacts)?;
        let report_ttl = artifacts.get(PROJECTION_REPORT_PATH).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("mappings run omitted {PROJECTION_REPORT_PATH}"),
            })
        })?;
        let ledger_graph = crate::stages::carrier::parse_into_graph(
            &crate::stages::carrier::turtle_to_nquads(report_ttl)?,
            "application/n-quads",
            crate::stages::carrier::GRAPH_PROJECTION_LEDGER,
        )?;
        // graph/alignments — the SSSOM alignment axioms (one triple per data row, CURIEs
        // expanded), built from THIS run's freshly-compiled `generated/mappings/*.sssom.tsv`
        // product artifacts and carried as a named graph so the presenter reads it via
        // `producer_graph` (PIPELINE_SPINE §4) instead of source-load re-reading the stale
        // committed SSSOM off disk (the stale-disk-fold class). It stays OUT of the
        // reasoned EDB: SSSOM is a generated view of meta-level correspondence, not an
        // object-level axiom source.
        let alignments_graph = crate::stages::carrier::parse_into_graph(
            &crate::stages::carrier::alignment_nquads_from_artifacts(&artifacts)?,
            "application/n-quads",
            crate::stages::carrier::GRAPH_ALIGNMENTS,
        )?;
        // graph/lang-translation-corpus — the live `lang:TranslationUnit` corpus (every
        // `.po` catalog pair typed as a crossing carrying a `logic:Correspondence`),
        // carried as a named graph so the presenter reads it via `producer_graph`. Like
        // the projection-ledger and alignments graphs it stays OUT of the reasoned EDB
        // (`gts_compose` folds only the default graph, so this named graph never pollutes
        // the composed object-level EDB).
        let lang_translation_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_translation_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_TRANSLATION_CORPUS,
        )?;
        // graph/lang-form-corpus — the total prose lift (Gate 1): every distinct
        // `@x-gmeow-english` source literal typed as a raw `lang:SurfaceForm`. Carried as a
        // named graph so the presenter reads it via `producer_graph`; like the
        // translation-corpus graph it stays OUT of the reasoned EDB (`gts_compose` folds only
        // the default graph).
        let lang_form_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_form_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_FORM_CORPUS,
        )?;
        // graph/lang-projection-corpus — the `lang:ProjectionEmission` records (every
        // lowering to an external linguistic ecosystem) plus the lifted `lang:Grammar`
        // structure. Carried as a named graph so the presenter reads it via
        // `producer_graph`; like the other `lang:` corpus graphs it stays OUT of the
        // reasoned EDB (`gts_compose` folds only the default graph).
        let lang_projection_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_projection_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_PROJECTION_CORPUS,
        )?;
        // graph/lang-lowering-corpus — the flagship quantified-SVO sentence lowered to its
        // compositional first-order `lang:CompositionalLowering` formula, one
        // `lang:LoweringStage` per lowering step. Carried as a named graph so the presenter reads
        // it via `producer_graph`; like the other `lang:` corpus graphs it stays OUT of the
        // reasoned EDB (`gts_compose` folds only the default graph).
        let lang_lowering_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_lowering_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_LOWERING_CORPUS,
        )?;
        // graph/lang-docs-rendering-corpus — the `.po`-derived documentation language trees
        // re-typed as `lang:Rendering` / `lang:Translation` crossings plus the exec-docs
        // English-only boundary gap. Carried as a named graph so the presenter reads it via
        // `producer_graph`; like the other `lang:` corpus graphs it stays OUT of the reasoned
        // EDB (`gts_compose` folds only the default graph).
        let lang_docs_rendering_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_docs_rendering_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_DOCS_RENDERING_CORPUS,
        )?;
        // graph/lang-glossary-corpus — the per-slice terminology glossary derived from the
        // reviewed `.po` pairs. Carried as a named graph so the presenter reads it via
        // `producer_graph`; like the other `lang:` corpus graphs it stays OUT of the reasoned
        // EDB (`gts_compose` folds only the default graph).
        let lang_glossary_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_glossary_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_GLOSSARY_CORPUS,
        )?;
        // graph/correspondence-laws — every authored `logic:Correspondence` re-projected with
        // its EXECUTED lens-law discharge verdicts. Carried as a named graph so
        // the presenter reads it via `producer_graph`; like the other corpus graphs it stays
        // OUT of the reasoned EDB (`gts_compose` folds only the default graph, so this named
        // graph never pollutes the composed object-level EDB — the verdicts are
        // presenter/provenance RDF, not reasoned facts).
        let correspondence_laws_graph = crate::stages::carrier::parse_into_graph(
            &compiled.correspondence_laws_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_CORRESPONDENCE_LAWS,
        )?;
        // graph/substrate-sbom — the substrate reconciliation A-Box
        // projected through THIS run's compiled `spdx.rq` into pure SPDX (one spdx:Package
        // per engine/library + `contains` relationships). It rides its own bundle-internal
        // named graph so the presenter reads it via `producer_graph`; it flattens into the
        // shipped bundle's base graph so `gmeow project --profile spdx` returns the substrate
        // packages. Like the other corpus graphs it stays OUT of the reasoned EDB
        // (`gts_compose` folds only the default graph). `spdx.rq` is THIS run's compiled
        // artifact (never a stale disk read); the source A-Box reads only build INPUTS
        // (`substrate_input_paths`), so the fold is non-self-referential.
        let spdx_rq_path = format!("{QUERIES_DIR}/spdx.rq");
        let spdx_rq = artifacts.get(&spdx_rq_path).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("mappings run omitted the compiled {spdx_rq_path}"),
            })
        })?;
        let spdx_rq = std::str::from_utf8(spdx_rq).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("compiled {spdx_rq_path} is not valid UTF-8: {e}"),
            })
        })?;
        let substrate_sbom_graph = crate::stages::carrier::parse_into_graph(
            crate::stages::substrate_graph::build_substrate_sbom_projection(input.root, spdx_rq)?
                .as_bytes(),
            "application/n-triples",
            crate::stages::carrier::GRAPH_SUBSTRATE_SBOM,
        )?;
        let dataset = std::sync::Arc::new(purrdf::RdfDataset::union(&[
            rdf_dataset.as_ref(),
            ledger_graph.as_ref(),
            alignments_graph.as_ref(),
            lang_translation_graph.as_ref(),
            lang_form_graph.as_ref(),
            lang_projection_graph.as_ref(),
            lang_lowering_graph.as_ref(),
            lang_docs_rendering_graph.as_ref(),
            lang_glossary_graph.as_ref(),
            correspondence_laws_graph.as_ref(),
            substrate_sbom_graph.as_ref(),
        ]));
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            artifacts,
        )))
    }
}

/// Parse every RDF artifact (`.ttl` / `.nq` / `.nt`) of the mappings byte-artifact
/// map and union them into one frozen dataset (the native contribution
/// `gts_compose` folds). Non-RDF artifacts are skipped. Inputs are unioned in
/// sorted-path order (the `BTreeMap` order) under [`RdfDataset::union`], which
/// standardizes blank scopes apart per input and canonicalizes on freeze.
fn mappings_rdf_dataset(
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let mut parsed: Vec<std::sync::Arc<purrdf::RdfDataset>> = Vec::new();
    for (path, bytes) in artifacts {
        let media_type = if path.ends_with(".nq") {
            "application/n-quads"
        } else if path.ends_with(".nt") {
            "application/n-triples"
        } else if path.ends_with(".ttl") {
            "text/turtle"
        } else {
            continue;
        };
        let ds = purrdf::parse_dataset(bytes, media_type, None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("mappings RDF parse of {path}: {e}"),
            })
        })?;
        parsed.push(ds);
    }
    let refs: Vec<&purrdf::RdfDataset> = parsed.iter().map(|a| a.as_ref()).collect();
    Ok(std::sync::Arc::new(purrdf::RdfDataset::union(&refs)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::source_load::rdf_bytes_to_dataset;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn fixture_jobs() -> usize {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    }

    fn triple_set(bytes: &[u8], media_type: &str) -> std::collections::BTreeSet<String> {
        let dataset = rdf_bytes_to_dataset(bytes, media_type, "triple_set").unwrap();
        purrdf::flat_rdf_quads_from_dataset(&dataset)
            .iter()
            .map(|q| {
                // The predicate is a bare IRI string; wrap it as an IRI term so its
                // Display renders `<iri>` — matching the old oxigraph `NamedNode` form
                // the substring assertions key on.
                let predicate = purrdf::RdfTerm::iri(q.predicate.clone());
                format!("{} {} {} .", q.subject, predicate, q.object)
            })
            .collect()
    }

    #[test]
    fn sssom_diagnostics_surface_parse_and_validation_errors() {
        let mut report = Report::new("mapping-compile");
        fold_sssom_findings(
            &mut report,
            "generated/mappings/bad.sssom.tsv",
            b"# mapping_set_id: https://example.org/missing-body\n",
        );
        let parse = report
            .findings
            .iter()
            .find(|finding| finding.detail.as_deref() == Some("check=parse code=sssom-tsv-parse"))
            .expect("parse failure finding");
        assert_eq!(parse.code, "mapping-compile.sssom");
        assert_eq!(
            parse
                .primary_location()
                .and_then(|location| location.path.as_deref()),
            Some("generated/mappings/bad.sssom.tsv")
        );

        let invalid = "\
# mapping_set_id: https://example.org/mapping\n\
# mapping_set_version: 0.1.0\n\
# license: https://creativecommons.org/licenses/by/4.0/\n\
# curie_map:\n\
#   gmeow: https://blackcatinformatics.ca/gmeow/\n\
#   skos: http://www.w3.org/2004/02/skos/core#\n\
#   semapv: https://w3id.org/semapv/vocab/\n\
subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment\n\
nope:Foo\tskos:closeMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t0.7\tmissing prefix\n";
        fold_sssom_findings(
            &mut report,
            "generated/mappings/prefix.sssom.tsv",
            invalid.as_bytes(),
        );
        let validation = report
            .findings
            .iter()
            .find(|finding| {
                finding.detail.as_deref()
                    == Some("check=PrefixMapCompleteness code=prefix validation")
            })
            .expect("validation failure finding");
        assert_eq!(validation.code, "mapping-compile.sssom");
        assert_eq!(
            validation
                .primary_location()
                .and_then(|location| location.path.as_deref()),
            Some("generated/mappings/prefix.sssom.tsv")
        );
    }

    /// Post-lang-graft: the BCP-47 projection consumers reach a language's tag THROUGH
    /// its carrier variety (the folded `gmeow:bcp47Tag` rides the variety IRI, joined via
    /// `lang:varietyOf`), the `@x-gmeow-*` retag reads `lang:carrierTag` on the variety,
    /// and the schema.org cells are re-expressed against the migrated shape
    /// (`gmeow:Language` + `lang:signSystemKind`; `lang:Orthography` binding). Computed
    /// FRESH from the DSL, so it verifies the rewiring independently of the committed
    /// (current) `.rq` bytes.
    #[test]
    fn bcp47_projection_queries_join_through_variety() {
        let root = repo_root();
        let artifacts =
            crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");
        let query = |name: &str| -> String {
            let path = format!("{QUERIES_DIR}/{name}");
            let (_, bytes) = artifacts
                .iter()
                .find(|(p, _)| p.as_str() == path.as_str())
                .unwrap_or_else(|| panic!("missing generated query {path}"));
            String::from_utf8_lossy(bytes).into_owned()
        };

        // ontolex: the name/lexical-item language tag is reached THROUGH the variety.
        let ontolex = query("ontolex.rq");
        assert!(
            ontolex.contains("lang:varietyOf ?lang") && ontolex.contains("gmeow:bcp47Tag ?langTag"),
            "ontolex must join the language tag through its variety:\n{ontolex}"
        );

        let schema = query("schema-org.rq");
        // inLanguage joins through the content language's variety.
        assert!(
            schema.contains("lang:varietyOf ?ilLang") && schema.contains("gmeow:bcp47Tag ?ilTag"),
            "schema inLanguage must join through the variety"
        );
        // The @x-gmeow-* retag reads lang:carrierTag on the variety, not the removed
        // gmeow:languageTag on the language.
        assert!(
            schema.contains("?_variety lang:varietyOf ?_lang")
                && schema.contains("?_variety lang:carrierTag ?_intTag")
                && schema.contains("?_variety gmeow:bcp47Tag ?_extTag"),
            "schema retag must reach carrierTag + bcp47Tag through the variety"
        );
        // The removed authored properties/classes are gone from the emitted queries.
        assert!(
            !schema.contains("gmeow:languageTag") && !schema.contains("gmeow:usesWritingSystem"),
            "no removed authored language properties survive in the schema query"
        );
        assert!(
            !schema.contains("a gmeow:ProgrammingLanguage"),
            "the removed gmeow:ProgrammingLanguage class must not survive"
        );
        // The migrated programming-language shape: gmeow:Language of programmingLanguageKind.
        assert!(
            schema.contains("lang:programmingLanguageKind"),
            "programming languages are gmeow:Language of lang:programmingLanguageKind"
        );
    }

    /// Load the exact authenticated mappings and upstream artifact lanes. A cache miss
    /// hard-fails; this helper cannot execute any producer.
    fn load_mappings_with_authenticated_upstream() -> (StageProduct, BTreeMap<String, StageProduct>)
    {
        let root = repo_root();
        let jobs = fixture_jobs();
        let mappings = crate::fixture::stage_artifacts(&root, jobs, "stage-mappings")
            .expect("exact mappings artifact fixture");
        let compile_logic = crate::fixture::stage_artifacts(&root, jobs, "stage-compile-logic")
            .expect("exact compile-logic artifact fixture");
        let constraint_shapes =
            crate::fixture::stage_artifacts(&root, jobs, "stage-export-constraint-shapes")
                .expect("exact constraint-shapes artifact fixture");
        (
            StageProduct::from_artifacts("stage-mappings", mappings),
            BTreeMap::from([
                (
                    "stage-compile-logic".to_string(),
                    StageProduct::from_artifacts("stage-compile-logic", compile_logic),
                ),
                (
                    "stage-export-constraint-shapes".to_string(),
                    StageProduct::from_artifacts(
                        "stage-export-constraint-shapes",
                        constraint_shapes,
                    ),
                ),
            ]),
        )
    }

    #[test]
    fn projection_report_unions_logic_and_correspondence_rows() {
        // Consume the exact producer-authenticated mapping product. The test never
        // recompiles either the mappings or compile-logic stages.
        let (out, _upstream) = load_mappings_with_authenticated_upstream();
        let report = std::str::from_utf8(out.artifact(PROJECTION_REPORT_PATH).expect("report"))
            .expect("utf8 report");

        // The report carries the correspondence rows (the whole point of the union): at
        // least one row per alignment dialect.
        for dialect in ["sssom:", "fno:", "edoal:", "sparql:"] {
            assert!(
                report.contains(&format!("/target/{dialect}")),
                "projection report missing {dialect} correspondence rows"
            );
        }

        assert!(
            report.contains("/target/owl-dl>") || report.contains("/target/datalog>"),
            "projection report must retain at least one logic projection row"
        );
    }

    /// The shape-grounding certificate ledger: one re-derived certificate per
    /// `logic:formalizes` record on THIS run's projected constraint surfaces, with the
    /// entry count EQUAL to the surfaces' record count (count-consistency — no record is
    /// skipped, none is invented), every entry carrying a re-derived
    /// `logic:preservationKind`, and the artifact emitted as its own canonical fold
    /// (structural idempotence: re-canonicalizing is a byte no-op).
    #[test]
    fn shape_grounding_ledger_covers_every_formalizes_record() {
        let (out, upstream) = load_mappings_with_authenticated_upstream();
        let ledger = std::str::from_utf8(
            out.artifact(SHAPE_GROUNDING_LEDGER_PATH)
                .expect("shape-grounding ledger artifact"),
        )
        .expect("utf8 ledger");

        // Count the formalizes records on the SAME fresh surfaces the stage consumed
        // (read off the upstream products the helper already ran — no re-run).
        let constraint_ttl = upstream["stage-export-constraint-shapes"]
            .artifact(crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH)
            .expect("constraint shapes");
        let procedural_ttl = upstream["stage-compile-logic"]
            .artifact(crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH)
            .expect("procedural constraints");
        let mut expected = 0usize;
        for bytes in [constraint_ttl, procedural_ttl] {
            let ds = purrdf::parse_dataset(bytes, "text/turtle", None).expect("surface parses");
            expected += gmeow_validate::shape_grounding::formalizes_records(&ds)
                .values()
                .map(std::collections::BTreeSet::len)
                .sum::<usize>();
        }
        assert!(expected > 0, "the surfaces must carry formalizes records");
        // Count-consistency, quad-exact: the ledger re-states EVERY surface
        // logic:formalizes record (no record skipped, none invented) and carries exactly
        // one re-derived judgment per record subject.
        let ledger_ds =
            purrdf::parse_dataset(ledger.as_bytes(), "text/turtle", None).expect("ledger parses");
        let ledger_records = gmeow_validate::shape_grounding::formalizes_records(&ledger_ds);
        assert_eq!(
            ledger_records
                .values()
                .map(std::collections::BTreeSet::len)
                .sum::<usize>(),
            expected,
            "the ledger must carry EXACTLY one certificate entry per surface \
             logic:formalizes record (count-consistency)"
        );
        assert_eq!(
            ledger.matches("logic:preservationKind logic:").count(),
            ledger_records.len(),
            "every record carries exactly one re-derived preservation judgment"
        );
        // The committed bytes ARE the canonical fold: re-canonicalizing is a byte no-op
        // (the structural idempotence guarantee — a second regenerate cannot differ).
        let recanon = purrdf::turtle_normalize::canonical_turtle(
            ledger.as_bytes(),
            &crate::stages::superset::rdf_prefixes(),
        )
        .expect("re-canonicalize");
        assert_eq!(
            recanon.as_bytes(),
            ledger.as_bytes(),
            "the ledger must be emitted as exactly its own canonical fold"
        );
    }

    #[test]
    fn fno_is_well_formed_ntriples() {
        // Wiring check: the FnO correspondence lowering produces a non-empty FnO
        // catalog that parses. (Committed-byte/iso parity is the CI strict-sync
        // gate, env-matched.)
        let root = repo_root();
        let artifacts =
            crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");
        let fno = artifacts.get(FNO_PATH).expect("fno artifact");
        let triples = triple_set(fno, "text/turtle");
        assert!(
            triples.len() > 20,
            "FnO catalog unexpectedly small: {} triples",
            triples.len()
        );
    }

    #[test]
    fn prefix_set_projections_are_emitted_and_parse() {
        // Wiring check (§2): the mappings stage emits the importable prefix
        // set + JSON-LD context, and the Turtle parses with the importable node
        // carrying the generalized sh:declare surface.
        let root = repo_root();
        let artifacts =
            crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");

        let core = artifacts
            .get(CORE_PREFIXES_PATH)
            .expect("core-prefixes artifact");
        let triples = triple_set(core, "text/turtle");
        // owl:Ontology declaration + at least one sh:declare per registry entry.
        let has_node = triples.iter().any(|t| {
            t.contains("CorePrefixes")
                && t.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                && t.contains("http://www.w3.org/2002/07/owl#Ontology")
        });
        assert!(has_node, "core-prefixes missing owl:Ontology node");
        let declares = triples
            .iter()
            .filter(|t| t.contains("http://www.w3.org/ns/shacl#prefix>"))
            .count();
        assert!(
            declares > 100,
            "expected one sh:prefix per registry entry, got {declares}"
        );

        let ctx = artifacts
            .get(JSONLD_CONTEXT_PATH)
            .expect("context.jsonld artifact");
        let text = std::str::from_utf8(ctx).expect("utf8 context");
        assert!(
            text.contains("\"@context\""),
            "context.jsonld has no @context"
        );
        assert!(text.contains("\"@vocab\""), "context.jsonld has no @vocab");
        assert!(text.ends_with("}\n}\n"), "context.jsonld malformed tail");
    }

    #[test]
    fn list_functions_are_emitted_and_parse() {
        // Wiring check (§5): the mappings stage emits the six list functions
        // as well-formed FnO Turtle (routed through the shared
        // `purrdf::fno::to_quads` serializer, §19 one-path), each typed via
        // fno:Output and fno:Function.
        let root = repo_root();
        let artifacts =
            crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");
        let lf = artifacts
            .get(LIST_FUNCTIONS_PATH)
            .expect("list-functions artifact");
        let triples = triple_set(lf, "text/turtle");
        let functions = triples
            .iter()
            .filter(|t| t.contains("https://w3id.org/function/ontology#Function"))
            .count();
        assert_eq!(functions, 6, "expected six fno:Function declarations");
        // Primitives are NOT gmeow:ProjectionFunction.
        assert!(
            !triples
                .iter()
                .any(|t| t.contains("https://blackcatinformatics.ca/gmeow/ProjectionFunction")),
            "list functions must not be gmeow:ProjectionFunction"
        );
        // Primitives bind no fno:predicate.
        assert!(
            !triples
                .iter()
                .any(|t| t.contains("<https://w3id.org/function/ontology#predicate>")),
            "list functions must bind no fno:predicate"
        );
        // Each issue-named function is present.
        for name in [
            "listLength",
            "listGet",
            "listIndexOf",
            "listSlice",
            "listConcat",
            "listContains",
        ] {
            assert!(
                triples
                    .iter()
                    .any(|t| t.contains(&format!("gmeow/{name}>"))),
                "missing function {name}"
            );
        }
    }

    /// Errors from `structural_lint_dataset` whose message names one of the four
    /// A-Box structural-annotation predicates this task completes — deliberately
    /// excludes the (separately tracked) language-tag-discipline codes, so this
    /// stays a precise "zero A-Box findings" check, not "zero findings at all".
    fn abox_annotation_errors<'a>(errors: &'a [String], subject: &str) -> Vec<&'a String> {
        const MISSING_MARKERS: [&str; 4] = [
            "is missing rdfs:label",
            "is missing skos:definition",
            "is missing rdfs:isDefinedBy",
            "is missing gmeow:graphBoxRole",
        ];
        errors
            .iter()
            .filter(|e| e.contains(subject) && MISSING_MARKERS.iter().any(|m| e.contains(m)))
            .collect()
    }

    /// Shift-left: drive the SAME native structural lint the whole-bundle SHACL
    /// validation (`make validate` / the pipeline stage-validate) runs
    /// (`gmeow_validate::lint::structural_lint_dataset`) over `complete_core_prefixes_abox`'s
    /// real output, so a missing/incorrect A-Box annotation on the minted
    /// `gmeow:CorePrefixes` T-Box header reds HERE — a fast `cargo nextest -p
    /// gmeow-pipeline` — rather than only surfacing at the next expensive
    /// whole-bundle SHACL validation. Mirrors `provenance_graph.rs`'s
    /// `minted_individuals_satisfy_the_assertional_abox_contract`.
    #[test]
    fn core_prefixes_completion_satisfies_the_structural_contract() {
        use gmeow_validate::lint::{
            LintConfig, default_annotation_predicates, structural_lint_dataset,
        };

        let vocab = gmeow_ns::gmeow_slice_vocab();
        let subject = vocab.core_prefixes_iri();
        let completed = complete_core_prefixes_abox(&emit_core_prefixes(&vocab), &vocab);
        // The real bundle supplies `gmeow:boxTBox a gmeow:GraphBoxRole` from the
        // kernel slice (`slices/vocabulary.ttl` et al. already reference
        // `gmeow:boxTBox` as a role); add it here so the role-typing check has its
        // declaration to resolve against (same pattern as `release.rs`'s
        // `minted_attestations_satisfy_the_assertional_contract`).
        let doc = format!(
            "{completed}<{}> <{}> <https://blackcatinformatics.ca/gmeow/GraphBoxRole> .\n",
            gmeow_errors::abox::BOX_TBOX,
            gmeow_errors::abox::RDF_TYPE,
        );
        let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
            .expect("parse the completed core-prefixes turtle");

        let cfg = LintConfig {
            namespace: gmeow_ns::GMEOW_NS.to_string(),
            ontology_iri: gmeow_ns::GMEOW_NS.trim_end_matches('/').to_string(),
            selector_tokens: Default::default(),
            core_slice_iris: Default::default(),
            annotation_predicates: default_annotation_predicates().into_iter().collect(),
        };
        let report = structural_lint_dataset(&ds, &cfg);
        let errors = report.errors();
        let core_prefixes_errors = abox_annotation_errors(&errors, &subject);
        assert!(
            core_prefixes_errors.is_empty(),
            "gmeow:CorePrefixes must satisfy the T-Box structural-annotation contract \
             (rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole): \
             {core_prefixes_errors:?}"
        );
        // Do NOT add gmeow:boxABox to a T-Box owl:Ontology header.
        assert!(
            !completed.contains(gmeow_errors::abox::BOX_ABOX),
            "gmeow:CorePrefixes is a T-Box header — it must never carry gmeow:boxABox"
        );
        // The label/comment `emit_core_prefixes` mints with a bare `@en` must be
        // retagged to the carrier tag — CorePrefixes is an internal ontology, not an
        // external lowering, so it owes the carrier-tag discipline.
        assert!(
            !errors
                .iter()
                .any(|e| e.contains(&subject) && e.contains("external language tag 'en'")),
            "gmeow:CorePrefixes label/comment must carry x-gmeow-english, not bare @en: {errors:?}"
        );
        assert!(
            !completed.contains("\"@en"),
            "no bare @en may survive on the completed CorePrefixes A-Box: {completed}"
        );
    }

    /// Shift-left: same pattern as `core_prefixes_completion_satisfies_the_structural_contract`,
    /// over `complete_list_functions_abox`'s real output — every A-Box
    /// `fno:Function`/`fno:Output`/`fno:Parameter` individual it mints must satisfy
    /// the full four-predicate contract.
    #[test]
    fn list_functions_completion_satisfies_the_structural_contract() {
        use gmeow_validate::lint::{LintConfig, structural_lint_dataset};

        let vocab = gmeow_ns::gmeow_slice_vocab();
        let catalog = purrdf::slice::list_functions::list_functions_catalog(&vocab);
        let completed = complete_list_functions_abox(&emit_list_functions(&vocab), &vocab);
        // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
        // kernel slice; add it here so the role-typing check has its declaration.
        let doc = format!(
            "{completed}<{}> <{}> <https://blackcatinformatics.ca/gmeow/GraphBoxRole> .\n",
            gmeow_errors::abox::BOX_ABOX,
            gmeow_errors::abox::RDF_TYPE,
        );
        let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
            .expect("parse the completed list-functions turtle");

        let cfg = LintConfig {
            namespace: gmeow_ns::GMEOW_NS.to_string(),
            ontology_iri: gmeow_ns::GMEOW_NS.trim_end_matches('/').to_string(),
            selector_tokens: Default::default(),
            core_slice_iris: Default::default(),
            annotation_predicates: Default::default(),
        };
        let report = structural_lint_dataset(&ds, &cfg);
        let errors = report.errors();

        let mut subjects: Vec<String> = Vec::new();
        for func in &catalog.functions {
            subjects.push(func.iri.clone());
            subjects.push(func.output.iri.clone());
        }
        for param in &catalog.params {
            subjects.push(param.iri.clone());
        }
        assert_eq!(
            subjects.len(),
            6 + 6 + 7,
            "expected 6 functions + 6 outputs + 7 params"
        );

        for subject in &subjects {
            let subject_errors = abox_annotation_errors(&errors, subject);
            assert!(
                subject_errors.is_empty(),
                "{subject} must satisfy the A-Box structural-annotation contract \
                 (rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole): \
                 {subject_errors:?}"
            );
        }
    }
}
