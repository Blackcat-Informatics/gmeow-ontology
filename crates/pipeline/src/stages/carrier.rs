// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The structured multi-named-graph snapshot assembly (fold-parity gate).
//!
//! This re-cuts `src/gmeow_tools/gts_gen.py::build_snapshot_bytes` natively: the
//! committed `generated/dist/gmeow.gts` is NOT everything-in-the-default-graph —
//! it is a STRUCTURED snapshot whose default graph carries the AUTHORED ontology
//! only (`ontology/gmeow.ttl` + slice `module.ttl`, NO imports/mappings/reason),
//! with the import closure, self-description metadata, SSSOM alignment axioms,
//! the RDF 1.2 statement layer, the slice-analysis graph, the verify attestation,
//! and the documentation projection each riding their own named graph, plus the
//! RDF 1.2 reifier/annotation tables and the content-addressed blob channel.
//!
//! It assembles a [`purrdf::gts_compose::SnapshotBuilder`] directly, routing each
//! source into its named graph.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use purrdf::RdfDatasetBuilder;
#[cfg(test)]
use purrdf::gts_compose::emit_gts;
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};
use purrdf::provenance::DatasetProvenance;
use purrdf::{
    RdfLiteral, RdfQuad, RdfTerm, RdfTriple, SerializeGraph, flat_rdf_quads_from_dataset,
    parse_dataset, serialize_dataset,
};
#[cfg(test)]
use rayon::prelude::*;

use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::statements::RDF12_PATH;

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The committed logical path of the serialized GTS bundle — the single artifact
/// this stage produces and every fold-reading leaf (and the sink) consumes.
pub const SNAPSHOT_PATH: &str = "generated/dist/gmeow.gts";

/// The named-graph IRIs (mirror `config.GTS_GRAPH_*`).
const GRAPH_IMPORTS: &str = "https://blackcatinformatics.ca/gmeow/graph/imports";
const GRAPH_METADATA: &str = "https://blackcatinformatics.ca/gmeow/graph/metadata";
pub(crate) const GRAPH_ALIGNMENTS: &str = "https://blackcatinformatics.ca/gmeow/graph/alignments";
pub(crate) const GRAPH_STATEMENTS: &str = "https://blackcatinformatics.ca/gmeow/graph/statements";
const GRAPH_VERIFY: &str = "https://blackcatinformatics.ca/gmeow/graph/verify";
const GRAPH_SLICE_ANALYSIS: &str = "https://blackcatinformatics.ca/gmeow/graph/slice-analysis";
/// The per-slice quality-assessment corpus: every slice scored against the
/// ontology-resident rubric, projected as `gmeow:QualityAssessment` observations (each
/// per-axis grade a scalar reading + a categorical tier, plus a roll-up meet tier) by
/// [`gmeow_slice_quality::assessment_nquads`]. Attached at the parallel DAG root
/// (`build_self_description_dataset`, a per-slice sweep over the authored slices — the
/// natural sibling of `graph/slice-analysis`) and read back by the presenter via
/// `source_load_graph`. Folded as its own queryable named graph so a repo-free consumer
/// reads every slice's quality grades straight out of `gmeow.gts` (the issue's headline
/// dogfooding deliverable). Excluded from the reasoned object-level EDB exactly like
/// `graph/slice-analysis` (it asserts a self-description corpus, not object-level axioms —
/// the reasoned EDB reads named graphs by IRI, so this one never pollutes it).
pub(crate) const GRAPH_QUALITY_ASSESSMENT: &str =
    "https://blackcatinformatics.ca/gmeow/graph/quality-assessment";
/// The committed on-disk projection of the quality-assessment corpus (PIPELINE_SPINE §5:
/// RDF travels as RDF, so the `gmeow:QualityAssessment` triples are reconstructible from
/// `gmeow.gts` as a flat `generated/` file, not only as a bundle-internal named graph).
/// Its `graph/fanout/<path>` reconstruction graph carries the SAME triples as
/// [`GRAPH_QUALITY_ASSESSMENT`]; the base graph serves the queryable bundle graph, the
/// fanout copy serves the superset gate / fanout writer (the correspondence-laws corpus
/// follows the same twin-graph pattern).
pub(crate) const QUALITY_ASSESSMENT_PATH: &str = "generated/quality/gmeow.quality-assessment.nt";
/// Internal byte-artifact lane member emitted by `stage-source-load`: the repo-wide
/// slice-quality diagnostics report rendered as self-contained HTML from the SAME scoring
/// pass that emits [`QUALITY_ASSESSMENT_PATH`]'s graph. It uses the `pipeline/` prefix so
/// regenerate/check never treat it as a committed flat artifact; it exists only to let the
/// terminal snapshot embed the report in the ontology-docs archive.
pub(crate) const SLICE_QUALITY_REPORT_HTML_ARTIFACT: &str = "pipeline/slice-quality-report.html";
/// Bundle-relative docs path exported by `gmeow export-docs`.
#[cfg(test)]
const SLICE_QUALITY_DOC_PATH: &str = "slice-quality/index.html";
pub(crate) const GRAPH_DOCUMENTATION: &str =
    "https://blackcatinformatics.ca/gmeow/graph/documentation";
pub(crate) const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
/// The by-reference blob `representation` under which a diagnostics producer
/// (`stage-validate` / `stage-compile-logic` / `stage-reason`) carries its FORWARD-projected
/// `Vec<gmeow_errors::DiagNode>` (raw JSON) on its product bundle — the SINGLE source
/// the run-level `DiagLedger` folds. It rides the standard content-store + lookaside
/// blob lane, so the per-stage cache persists/replays it verbatim; a cache-hit product
/// re-serves the identical nodes. Re-exported from the reader-side definition in
/// [`crate::bundle_blobs`] so the producer and reader share ONE constant — the label
/// cannot drift (a drifted label would silently read back an empty node set).
pub(crate) use crate::bundle_blobs::REP_DIAG_NODES;
/// The archive `representation` under which the mdbook `src/` source tree rides as a bundle
/// blob (the `docs-book` archive). Re-exported from the reader-side definition in
/// [`crate::bundle_blobs`] so the producer and reader share ONE constant — the label cannot
/// drift (a drifted label would silently read back an empty archive).
#[cfg(test)]
pub(crate) use crate::bundle_blobs::REP_DOCS_BOOK;
/// The archive `representation` under which the print documentation projection (the
/// byte-reproducible `gmeow.pdf` + its deterministic `gmeow.typ` source) rides as a bundle
/// blob (the `docs-print` archive). Re-exported from the reader-side definition in
/// [`crate::bundle_blobs`] so the producer and reader share ONE constant.
#[cfg(test)]
pub(crate) use crate::bundle_blobs::REP_DOCS_PRINT;
/// The by-reference blob `representation` under which `stage-source-load` carries its
/// authored subject→source-position [`SpanIndex`](crate::ingest::SpanIndex) (raw JSON)
/// on its product bundle — the SINGLE source of the source spans the diagnostics
/// consumers (`stage-validate` / `stage-compile-logic`) lift onto their findings. It
/// rides the standard content-store + lookaside blob lane, so the per-stage cache
/// persists/replays it verbatim. It is STRIPPED from the source-load product once the
/// last span-table consumer has run (drop-after-last-consumer), so it never reaches the
/// carrier assembly and does NOT ship in `gmeow.gts`. Re-exported from the reader-side
/// definition in [`crate::bundle_blobs`] so the producer and reader share ONE constant.
pub(crate) use crate::bundle_blobs::REP_SPAN_TABLE;
/// The native↔external-corpus reasoning-divergence Findings, folded as their own
/// queryable named graph so a repo-free consumer reads every coverage divergence
/// (native-incomplete `DlGap` / native-disagrees `CorpusOnly`) against the W3C
/// published expected verdicts without re-grading the corpus. Sibling of
/// `graph/diagnostics` (correctness evidence, not validation/lint findings).
pub(crate) const GRAPH_CONFORMANCE: &str = "https://blackcatinformatics.ca/gmeow/graph/conformance";
/// The compiler's projection-report loss ledger, folded as its own queryable named
/// graph so a repo-free consumer reads every projection's preservation kind and
/// structural lossy drops without re-running the compiler.
pub(crate) const GRAPH_PROJECTION_LEDGER: &str =
    "https://blackcatinformatics.ca/gmeow/graph/projection-ledger";
/// The live `lang:TranslationUnit` corpus: every multilingual `.po` catalog pair typed
/// as a first-class crossing carrying a `logic:Correspondence` with an honestly-computed
/// preservation judgment (Principle 15 consumer wiring). Folded as its own queryable
/// named graph so a repo-free consumer reads what each translation loses against the
/// English canon without re-parsing the `.po` catalogs. Excluded from the reasoned
/// object-level EDB exactly like `graph/projection-ledger` (it asserts a
/// self-description corpus, not object-level axioms).
pub(crate) const GRAPH_LANG_TRANSLATION_CORPUS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/lang-translation-corpus";
/// The total prose-lift corpus: every distinct `@x-gmeow-english` source literal interned
/// as a raw `lang:SurfaceForm` carrying its `logic:candidateSourceHash` and an exact
/// surface-round-trip `logic:Correspondence` (Gate 1: total prose lift). Folded as its own
/// queryable named graph so a repo-free consumer reaches every source-prose surface without
/// re-parsing the slice Turtle. Excluded from the reasoned object-level EDB exactly like
/// `graph/lang-translation-corpus` (it asserts a self-description corpus, not object-level
/// axioms).
pub(crate) const GRAPH_LANG_FORM_CORPUS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/lang-form-corpus";
/// The `lang:` projection corpus: one `lang:ProjectionEmission` per (source, target) —
/// the honest per-emission preservation judgment of every lowering to an external
/// linguistic ecosystem (OntoLex-Lemon, CoNLL-U, EBNF, ABNF) plus the lifted `lang:Grammar`
/// structure it projects. Folded as its own queryable named graph so a repo-free consumer
/// reads what each projection loses without re-running the projection registry. Excluded
/// from the reasoned object-level EDB exactly like `graph/lang-form-corpus` (it asserts a
/// self-description corpus, not object-level axioms).
pub(crate) const GRAPH_LANG_PROJECTION_CORPUS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/lang-projection-corpus";
/// The compositional-lowering corpus: the flagship quantified-SVO sentence "every cat chases a
/// mouse" lowered — one declared stage at a time — to its first-order
/// `lang:CompositionalLowering` formula `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))`, each
/// `lang:LoweringStage` carrying its `logic:preservationKind`. Folded as its own queryable named
/// graph so a repo-free consumer reaches the compositional lowering without re-running the native
/// bridge. Excluded from the reasoned object-level EDB exactly like `graph/lang-projection-corpus`
/// (it asserts a self-description corpus, not object-level axioms — `gts_compose` folds only the
/// default graph, so this named graph never pollutes the composed EDB).
pub(crate) const GRAPH_LANG_LOWERING_CORPUS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/lang-lowering-corpus";
/// The docs-rendering corpus: the `.po`-derived documentation language trees re-typed as
/// reified crossings — one `lang:Rendering` (`lang:renderingDocsPage`) per non-English page,
/// one `lang:Translation` per (page, language) pairing rolling up the page's live
/// `lang:TranslationUnit`s with a DERIVED document judgment, and the exec-docs English-only
/// asset boundary recorded as a declared `lang:translationGap`. Folded as its own queryable
/// named graph so a repo-free consumer reads what the documentation translation loses without
/// re-rendering the site. Excluded from the reasoned object-level EDB exactly like
/// `graph/lang-projection-corpus` (it asserts a self-description corpus, not object-level
/// axioms).
pub(crate) const GRAPH_LANG_DOCS_RENDERING_CORPUS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/lang-docs-rendering-corpus";
/// The docs-format grounding corpus: the four documentation output formats (site, mdbook,
/// print PDF, term snippets) typed as lossy projections of one shared documentation body-set.
/// Carries a `logic:Correspondence` per composition-DAG leg (with the derived
/// weakest-dominates preservation join per format), a `gmeow:NotationProjectionProfile` per
/// format enumerating the capabilities it represents / declares lost, and a
/// `gmeow:contentDigest` self-description of the packed `docs-book` / `docs-print` blobs.
/// Assembled at carrier time — the only point the packed blobs' byte digests exist. Folded
/// as its own queryable named graph, excluded from the reasoned object-level EDB exactly like
/// `graph/lang-docs-rendering-corpus` (it asserts a self-description corpus, not object-level
/// axioms).
#[cfg(test)]
#[allow(dead_code)]
pub(crate) const GRAPH_DOCS_FORMAT_RENDERING: &str =
    "https://blackcatinformatics.ca/gmeow/graph/docs-format-rendering";
/// The correspondence-laws corpus: every authored `logic:Correspondence` re-projected with
/// the EXECUTED lens-law discharge verdicts attached — one `logic:LawClaim`
/// (`logic:lawClaimed` / `logic:lawDischargeVerdict` / `logic:lawDischargeCondition`) per law
/// the correspondence's rung permits, discharged by running its OWN per-binding get/put
/// CONSTRUCT round-trip through the native engine. Folded as its own queryable
/// named graph so a repo-free consumer reads which alignments provably round-trip
/// (`ObligationDischarged`) without re-running the engine. Excluded from the reasoned
/// object-level EDB exactly like `graph/projection-ledger` (it asserts a self-description /
/// provenance corpus, not object-level axioms).
pub(crate) const GRAPH_CORRESPONDENCE_LAWS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws";
/// The committed on-disk projection of the correspondence-laws corpus (PIPELINE_SPINE §5:
/// RDF travels as RDF, so the discharged `logic:SectionLaw` claims are reconstructible from
/// `gmeow.gts` as a flat `generated/` file, not only as a bundle-internal named graph). Its
/// `graph/fanout/<path>` reconstruction graph carries the SAME triples as
/// `GRAPH_CORRESPONDENCE_LAWS`; the base graph serves the up-projection gates, the fanout copy
/// serves the superset gate / fanout writer (the diagnostics `.nq` follow the same twin-graph
/// pattern).
pub(crate) const CORRESPONDENCE_LAWS_PATH: &str = "generated/logic/gmeow.correspondence-laws.nt";
/// The authored default graph (root ontology + slice modules + translations + guide
/// anchors, NO imports) carried as a named graph on the `stage-source-load` product so
/// the presenter reads it instead of re-loading the sources. It is an INTERNAL transport
/// graph: the presenter re-roots it into the carrier's DEFAULT graph, so it never appears
/// as a named graph in the emitted bundle (never a committed-file reconstruction rep).
pub(crate) const GRAPH_AUTHORED_DEFAULT: &str =
    "https://blackcatinformatics.ca/gmeow/graph/authored-default";

/// The seven `math:` producer graphs, one per native producer entrypoint — five bound to the
/// flagship-acceptance manifest's `gmeow:FlagshipScenario` individuals, plus
/// `probability-model` ([`gmeow_math::producers::probability_model_seam`]), the probability
/// layer's live `logic:probabilityModel` seam producer, and `pvalue-tri-slice`
/// ([`gmeow_math::producers::pvalue_tri_slice`]), the signature `lang:` → `logic:` → `math:`
/// p-value round-trip (both NOT flagship-bound; the manifest's "five, not adjectives"
/// depth-bar contract stays exactly five). The `stage-math-producers`
/// stage RUNS each `gmeow_math::producers::*` function and parses its deterministic `.turtle`
/// into the matching named graph here; the snapshot presenter reads each back via
/// `producer_graph` and folds it into `gmeow.gts` (Design A — the producer output ships in
/// the bundle, the shippable deliverable). Bundle-internal, like the `lang:` corpus graphs:
/// excluded from the reasoned object-level EDB (`gts_compose` folds only the default graph)
/// and NOT a `generated/` file, so they map to no committed path — the superset gate's orphan
/// sweep only considers `graph/fanout/…` / `graph/projections/…` reps. The array order pins
/// the producer→graph pairing shared by the stage and the presenter.
pub(crate) const MATH_PRODUCER_GRAPHS: [&str; 7] = [
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/e8-weyl",
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/additive-he",
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/proof-ingest",
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/r-bridge",
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/pca-residual",
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/probability-model",
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/pvalue-tri-slice",
];
const REP_SHACL_SARIF: &str = "gmeow:report/shacl/sarif";
const REP_SHACL_FINDINGS: &str = "gmeow:report/shacl/findings";

/// The media type carried on the typed SHACL validation-shape sidecar blob. The gts
/// decode side (`purrdf::gts::lookaside_from_graph` → `lookaside_kind_from_metadata`)
/// classifies a blob whose media type contains `shacl` into
/// [`purrdf::RdfLookasideKind::Shacl`], so a repo-free consumer reads the SHACL
/// validation surface under its typed kind without re-running the compiler. The bytes
/// are SHACL-in-Turtle; the `profile=shacl` parameter carries the domain hint the
/// classifier keys on while keeping the base type honest.
const VALIDATION_SHACL_MEDIA_TYPE: &str = "text/turtle;profile=shacl";
/// The media type carried on the typed ShEx validation-shape sidecar blob. Contains
/// `shex`, so the gts decode classifies the blob into
/// [`purrdf::RdfLookasideKind::Shex`] (`text/shex` is the ShExC compact-syntax type).
const VALIDATION_SHEX_MEDIA_TYPE: &str = "text/shex";

/// Gather the by-reference archive blobs + the SHACL report
/// blobs from `upstream`, and serialize the ALREADY-ASSEMBLED `carrier` into the
/// terminal `gmeow.gts` package — the SINGLE serialization the terminal gts sink
/// performs. The carrier is taken off the snapshot product's bundle,
/// never re-assembled (the razor: transform transport→form at most once per pipeline).
///
/// Mirrors the former snapshot-stage blob gathering exactly: REP_AXIOMS/schemas from
/// the in-memory `stage-compile-logic` / `stage-export-json-schema` products (one-pass
/// freshness), the reasoning reports from `stage-reason`, and the SHACL SARIF/findings
/// from `stage-validate`. Every missing artifact HARD-fails (no-optionality).
pub(crate) fn serialize_carrier_snapshot(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
    carrier: &purrdf::RdfDataset,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    serialize_carrier_snapshot_without_docs(root, upstream, carrier)
}

/// Assemble the terminal GTS from the logical carrier and non-document runtime
/// lookasides. Documentation projections are deliberately external artifacts:
/// ontology-docs, mdbook, print, OKF, JSON-LD, and YAML-LD are regenerated by
/// `make docs` and never embedded in `gmeow.gts`.
#[cfg(test)]
pub(crate) fn serialize_carrier_snapshot_with_docs_model(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
    carrier: &purrdf::RdfDataset,
    _docs_model: &gmeow_docs::model::DocsModel,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    serialize_carrier_snapshot_without_docs(root, upstream, carrier)
}

fn serialize_carrier_snapshot_without_docs(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
    carrier: &purrdf::RdfDataset,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    // THIS run's freshly-emitted JSON Schema + OpenAPI bytes (from the in-memory
    // product, not the on-disk files which are not written until phase 1 returns).
    let schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema gmeow.schema.json artifact"))?
        .to_vec();
    let openapi_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::OPENAPI_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema gmeow.openapi.json artifact"))?
        .to_vec();
    // THIS run's two hand-authored self-describing schemas (the term `Card` shape +
    // the `validate_local` envelope shape), from the SAME product so they never lag a
    // regenerate — folded into REP_SCHEMAS alongside the SHACL-derived pair.
    let card_schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::CARD_SCHEMA_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema card.schema.json artifact"))?
        .to_vec();
    let finding_schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::FINDING_SCHEMA_PATH))
        .ok_or_else(|| {
            stage_err("missing stage-export-json-schema validate-finding.schema.json artifact")
        })?
        .to_vec();
    // THIS run's compiled axiom surface (REP_AXIOMS), from the stage-compile-logic
    // product so it never lags a regenerate.
    let compile_artifacts = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product"))?
        .artifacts();
    // THIS run's compiled SSSOM surface (REP_MAPPINGS), from the stage-mappings product
    // so the archive never lags a mapping-source edit: the committed generated/mappings/
    // files are not flushed until phase 1 returns, so reading them from disk here would
    // tar the STALE committed set and a mapping edit could never reach the bundle without
    // a manual disk write. Sourced from the product exactly as schemas / axioms are.
    let mappings_artifacts = upstream
        .get("stage-mappings")
        .ok_or_else(|| stage_err("missing stage-mappings product"))?
        .artifacts();
    // THIS run's generated shape surfaces (REP_SHAPES members), from the producing
    // export leaves' products so the archive never lags a competency/frame edit:
    // the committed generated/shapes/*.ttl are projected back from the bundle by the
    // fanout, so a stale disk read here would freeze them forever (the exact trap the
    // validation-shapes.ttl override documents). Hard-fail if absent (no-optionality).
    let result_shapes_ttl = upstream
        .get("stage-export-result-shapes")
        .and_then(|p| p.artifact(crate::stages::result_shapes::RESULT_SHAPES_PATH))
        .ok_or_else(|| stage_err("missing stage-export-result-shapes result-shapes.ttl artifact"))?
        .to_vec();
    let frame_shapes_ttl = upstream
        .get("stage-export-frame-shapes")
        .and_then(|p| p.artifact(crate::stages::frame_shapes::FRAME_SHAPES_PATH))
        .ok_or_else(|| stage_err("missing stage-export-frame-shapes frame-shapes.ttl artifact"))?
        .to_vec();
    // THIS run's constraint-shapes surface (the SHACL projection of the logic: FOL axioms),
    // folded into REP_SHAPES from the fresh product for the SAME reason as result/frame
    // shapes: the committed generated/shapes/constraint-shapes.ttl is projected back from the
    // bundle by the fanout, and on a first run it does not exist on disk at all, so only the
    // fresh product can carry it (H8).
    let constraint_shapes_ttl = upstream
        .get("stage-export-constraint-shapes")
        .and_then(|p| p.artifact(crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH))
        .ok_or_else(|| {
            stage_err("missing stage-export-constraint-shapes constraint-shapes.ttl artifact")
        })?
        .to_vec();
    // THIS run's freshly-rendered Pydantic model package (REP_MODELS_PYTHON),
    // sourced from the stage-export-pydantic product so the bundle blob never lags a
    // regenerate: the committed packages/python/gmeow_models/* are not flushed until
    // phase 1 returns, so a disk read here would tar the stale committed tree.
    let models_python_artifacts = upstream
        .get("stage-export-pydantic")
        .ok_or_else(|| stage_err("missing stage-export-pydantic product"))?
        .artifacts();
    let mut blobs = build_archive_blobs(
        root,
        &SchemaSurfaces {
            schema: &schema_json,
            openapi: &openapi_json,
            card: &card_schema_json,
            finding: &finding_schema_json,
        },
        &compile_artifacts,
        &mappings_artifacts,
        &ShapeSurfaces {
            result: &result_shapes_ttl,
            frame: &frame_shapes_ttl,
            constraint: &constraint_shapes_ttl,
        },
        &models_python_artifacts,
    )?;
    // Slice guides are documentation projections too. They remain canonical
    // source inputs for `make docs`, but do not ride the logical bundle.
    // The document-scale surface blobs: every `@x-gmeow-english` source literal whose
    // byte-length crosses the document-scale threshold rides here by content-addressed
    // reference (the `lang:surfaceBlob "blake3:<hex>"` anchor the lang-form corpus emits),
    // so a document-scale surface never inlines payload bytes in the graph. Recomputed
    // from `root` exactly like the guide blobs (its own freshly-discovered catalog, so the
    // set stays a pure function of the sources); empty until a source literal crosses the
    // threshold.
    let lang_form_catalog = purrdf::slice::SliceCatalog::discover(
        &root.join("slices"),
        crate::gmeow_ns::gmeow_slice_vocab(),
    )
    .map_err(|e| stage_err(&format!("lang-form slice catalog: {e}")))?;
    blobs.extend(crate::stages::lang_form::build_surface_blobs(Some(
        &lang_form_catalog,
    ))?);
    blobs.push(build_reasoning_blob(upstream)?);
    // The opaque-fanout archive: every non-RDF generated/ fanout output, recomputed
    // from THIS run's carrier (superset law — RDF rides as named graphs, not here).
    blobs.push(build_fanout_opaque_blob(carrier, upstream)?);
    // The typed Shacl/Shex validation-shape sidecars: the SAME validation surfaces the
    // REP_GENERATED archive carries (one source — `validation_shape_surfaces`), ALSO folded
    // as content-addressed blobs whose media type classifies each on decode into its typed
    // lookaside kind, so a repo-free consumer reads the validation surface under
    // RdfLookasideKind::Shacl / Shex without re-running the compiler (LOGIC-VALIDATION.md).
    blobs.extend(build_validation_shape_typed_blobs(upstream)?);

    let shacl_json = upstream
        .get("stage-validate")
        .and_then(|p| p.artifact(crate::stages::validate::SHACL_JSON_PATH))
        .ok_or_else(|| stage_err("missing validate-stage SHACL diagnostics JSON"))?
        .to_vec();
    let shacl_sarif = upstream
        .get("stage-validate")
        .and_then(|p| p.artifact(crate::stages::validate::SHACL_SARIF_PATH))
        .ok_or_else(|| stage_err("missing validate-stage SHACL diagnostics SARIF"))?
        .to_vec();
    let report_blobs = vec![
        BlobRow {
            data: shacl_sarif,
            media_type: "application/sarif+json".to_string(),
            rep: REP_SHACL_SARIF.to_string(),
        },
        BlobRow {
            data: shacl_json,
            media_type: "application/json".to_string(),
            rep: REP_SHACL_FINDINGS.to_string(),
        },
    ];
    serialize_snapshot(carrier, &[], blobs, report_blobs)
}

/// Hard-fail if any documented class/property/individual term would link to an OKF
/// document the OKF bundle does not emit. The docs term surface
/// (`gmeow_docs::model::DocsModel::terms`) and the OKF term surface
/// (`crate::stages::export::collect_term_surface`) are collected by different paths, so
/// this enforces "no dangling OKF link" — a missing document is a HARD FAIL, never a
/// silent dangling reference. Reuses the renderer's own `okf_doc_reference` (the exact
/// path the site links) and the OKF stage's `doc_relpath` (the exact path it emits), so
/// the two can never diverge in scheme; this gate only checks existence.
#[cfg(test)]
fn assert_okf_docs_cover_documented_terms(
    carrier: &purrdf::RdfDataset,
    model: &gmeow_docs::model::DocsModel,
) -> Result<(), gmeow_errors::Diag> {
    let (_, _, terms) = crate::stages::export::collect_term_surface(carrier)?;
    let emitted: std::collections::BTreeSet<String> =
        terms.iter().map(crate::stages::okf::doc_relpath).collect();
    let links: Vec<Option<String>> = model
        .terms
        .iter()
        .map(gmeow_docs::okf_doc_reference)
        .collect();
    let missing = okf_link_targets_missing_from(&emitted, &links);
    if !missing.is_empty() {
        let mut curies: Vec<String> = missing
            .into_iter()
            .map(|i| model.terms[i].curie.clone())
            .collect();
        curies.sort();
        return Err(stage_err(&format!(
            "OKF projection is missing documents for {} documented term(s), which would \
             ship as dangling links: {}",
            curies.len(),
            curies.join(", ")
        )));
    }
    Ok(())
}

/// The pure set-comparison the OKF-coverage gate delegates to: given the bundle-relative
/// paths the OKF projection actually emits and the ordered list of link targets the docs
/// site would generate (`None` for categories the OKF bundle deliberately skips), return
/// the indices of `links` whose target the OKF bundle does not emit. Kept as a standalone
/// function so the hard-fail logic itself is directly unit-testable, independent of a
/// live `DocsModel`/carrier fixture.
#[cfg(test)]
fn okf_link_targets_missing_from(
    emitted: &std::collections::BTreeSet<String>,
    links: &[Option<String>],
) -> Vec<usize> {
    links
        .iter()
        .enumerate()
        .filter_map(|(i, link)| {
            let link = link.as_ref()?;
            let relpath = link.strip_prefix("gmeow-okf/").unwrap_or(link);
            if emitted.contains(relpath) {
                None
            } else {
                Some(i)
            }
        })
        .collect()
}

/// Assemble the FULL snapshot carrier: every named graph parsed into ONE native
/// `RdfDataset` and unioned once. The carried logic / relational-core / correspondence
/// / reasoning graphs ride in from the upstream producers' carriers (no re-derivation),
/// while only logic / relational-core enter the object-level reasoning EDB;
/// the snapshot-owned graphs (authored default, statement layer, imports, metadata,
/// alignments, slice-analysis, verify, documentation, diagnostics, conformance,
/// projection-ledger, provenance) are parsed and re-rooted here. This carrier is the
/// single internal transport — it is BOTH serialized to gts and carried as the snapshot
/// product's bundle, so the snapshot is assembled ONCE.
/// Every authored source [`build_self_description_dataset`] reads, so the
/// `stage-source-load` cache busts when any of them changes (cache soundness — a stale
/// self-description graph would ship a stale bundle). Over-covers rather than under: the
/// authored ontology + modules + imports (base / provenance), the self-description
/// metadata, the slice manifests (slice-analysis), the full SHACL shape surface (verify),
/// and the docs sources (translations + guides folded into the authored default). The
/// generated SSSOM alignments (`generated/mappings/`) are a produced artifact, not an
/// authored source, so they are covered by the producing stage's own cache, not here.
pub(crate) fn self_description_source_files(
    root: &Path,
) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut files = crate::stages::source_load::authored_files(root)?;
    files.extend(crate::stages::source_load::manifest_files(root)?);
    files.extend(crate::stages::docs_render::docs_source_files(root)?);
    let metadata = root.join("metadata").join("gmeow-self.ttl");
    if metadata.is_file() {
        files.push(metadata);
    }
    files.extend(list_files(&root.join("shapes"), "ttl")?);
    // The `generated/shapes/*.ttl` are NOT declared here: they are produced projections,
    // not authored sources source-load reads at run(), and each is covered by its own
    // producing stage's cache (frame/result/constraint-shapes + compile-logic). Reading
    // `generated/` from disk to cache-key a stage is the stale-disk-fold class this change
    // retires — freshness rides the consumes chain, not a disk enumeration here.
    files.extend(slice_named_files(root, "shapes.ttl")?);
    // The quality-assessment graph is built here by scoring every slice, so the cache must
    // bust when ANY scored input changes (rubric module, each slice's manifest / module /
    // examples / tests). `gmeow_slice_quality::scored_source_files` is the single authority
    // for what the scorer reads — sharing it keeps the cache key and the score set from
    // drifting (a stale scored input would ship a stale assessment in gmeow.gts).
    files.extend(gmeow_slice_quality::scored_source_files(root)?);
    files.sort();
    files.dedup();
    Ok(files)
}

/// Build the self-description named graphs from the authored sources — the graphs the
/// presenter used to re-load and re-canonicalize on the serial snapshot node. The
/// `stage-source-load` stage attaches this to its product so the LOAD and CANONICALIZE
/// happen ONCE, at the parallel DAG root, and the presenter merely reads and folds them
/// (PIPELINE_SPINE §3.2/§4 — the terminal assembles nothing).
///
/// The returned dataset carries, each in its final named graph: the authored default
/// ([`GRAPH_AUTHORED_DEFAULT`], re-rooted into the carrier's default graph by the
/// presenter), the import closure ([`GRAPH_IMPORTS`]), self-description metadata
/// ([`GRAPH_METADATA`]), the slice-analysis graph ([`GRAPH_SLICE_ANALYSIS`]), the native
/// verify attestation ([`GRAPH_VERIFY`], over the authored ∪ imports EDB), and the
/// occurrence-based provenance projection
/// ([`crate::stages::provenance_graph::GRAPH_PROVENANCE`]). Byte-identical to the former
/// in-snapshot construction — the SAME loaders and canonicalizers, relocated verbatim.
///
/// The SSSOM alignment axioms ([`GRAPH_ALIGNMENTS`]) are NO LONGER built here: they are a
/// projection of the compiled SSSOM, so `stage-mappings` builds that graph from its fresh
/// product (via [`alignment_nquads_from_artifacts`]) and the presenter reads it back through
/// `producer_graph`; it remains outside object-level reasoning. Building it here would re-read the stale committed
/// `generated/mappings/*.sssom.tsv` off disk (the stale-disk-fold class).
#[cfg(test)]
pub(crate) fn build_self_description_dataset(
    root: &Path,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let quality = gmeow_slice_quality::assessment_artifacts(root)
        .map_err(|e| stage_err(&format!("quality-assessment sweep: {e}")))?;
    build_self_description_dataset_with_quality(root, &quality.nquads)
}

/// Build the self-description named graphs with a caller-supplied slice-quality graph.
/// `stage-source-load` uses this after scoring once so the same pass can also publish the
/// diagnostics HTML; tests keep a wrapper that scores and calls this helper directly.
pub(crate) fn build_self_description_dataset_with_quality(
    root: &Path,
    quality_assessment: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let authored = load_authored_default(root)?;
    let authored_canon = canonicalize_nq(&authored, "base")?;
    reject_quoted_triples(&parse_nq(authored_canon.as_bytes())?, "<default>")?;
    // The authored default rides its own named graph (re-rooted to default by the
    // presenter); base ∪ imports is the EDB the verify attestation runs over.
    let base = parse_dataset(authored_canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("base parse: {e}")))?;

    let imports = load_imports(root)?;
    let metadata = load_metadata(root)?;
    let slice_analysis = build_slice_analysis(root, &authored)?;
    let verify_attestation = {
        let imports_ds = parse_dataset(&imports, "text/turtle", None)
            .map_err(|e| stage_err(&format!("verify imports parse: {e}")))?;
        let edb = purrdf::RdfDataset::union(&[base.as_ref(), imports_ds.as_ref()]);
        run_verify_attestation(&edb)?
    };
    let provenance_nt = build_provenance_projection(root)?;
    // graph/quality-assessment — every slice scored against the ontology-resident rubric,
    // projected as `gmeow:QualityAssessment` observations. A per-slice sweep over the
    // authored slices (the natural sibling of the slice-analysis graph), built ONCE here
    // and read back by the presenter via `source_load_graph`. The producer re-emits the
    // per-slice `graph/slice-quality` N-Quads; `parse_into_graph` re-roots them into the
    // carrier's `graph/quality-assessment` label.
    let datasets: Vec<std::sync::Arc<purrdf::RdfDataset>> = vec![
        rooted_in_graph(&base, GRAPH_AUTHORED_DEFAULT)?,
        parse_into_graph(&imports, "application/n-quads", GRAPH_IMPORTS)?,
        parse_into_graph(&metadata, "application/n-quads", GRAPH_METADATA)?,
        parse_into_graph(&slice_analysis, "application/n-quads", GRAPH_SLICE_ANALYSIS)?,
        parse_into_graph(
            quality_assessment.as_bytes(),
            "application/n-quads",
            GRAPH_QUALITY_ASSESSMENT,
        )?,
        parse_into_graph(&verify_attestation, "application/n-quads", GRAPH_VERIFY)?,
        parse_into_graph(
            provenance_nt.as_bytes(),
            "application/n-triples",
            crate::stages::provenance_graph::GRAPH_PROVENANCE,
        )?,
    ];
    let refs: Vec<&purrdf::RdfDataset> = datasets.iter().map(|d| d.as_ref()).collect();
    Ok(std::sync::Arc::new(purrdf::RdfDataset::union(&refs)))
}

#[cfg(test)]
#[allow(dead_code)]
fn slice_quality_report_html(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<&[u8], gmeow_errors::Diag> {
    let bytes = upstream
        .get("stage-source-load")
        .and_then(|p| p.artifact(SLICE_QUALITY_REPORT_HTML_ARTIFACT))
        .ok_or_else(|| {
            stage_err(&format!(
                "missing stage-source-load {SLICE_QUALITY_REPORT_HTML_ARTIFACT} artifact"
            ))
        })?;
    if bytes.is_empty() {
        return Err(stage_err(&format!(
            "stage-source-load {SLICE_QUALITY_REPORT_HTML_ARTIFACT} artifact is empty"
        )));
    }
    Ok(bytes)
}

/// The `stage-source-load` product's carrier dataset (the authored base default graph
/// plus the self-description named graphs it attaches). HARD-fails if the edge is missing.
fn source_load_dataset(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    Ok(upstream
        .get("stage-source-load")
        .ok_or_else(|| {
            stage_err("missing stage-source-load product for the self-description graphs")
        })?
        .bundle()
        .dataset_arc())
}

/// Read one self-description graph off the `stage-source-load` product, re-rooted into
/// `graph_iri` (the presenter's read half of [`build_self_description_dataset`]). The
/// producer attached it canonical and rooted; this is a pure projection — no load, no
/// canonicalize.
fn source_load_graph(
    upstream: &BTreeMap<String, StageProduct>,
    graph_iri: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    rooted_in_graph(
        &source_load_dataset(upstream)?.project_named_graph(graph_iri),
        graph_iri,
    )
}

/// Read a first-class carrier named graph off its PRODUCER's attached dataset, re-rooted
/// into `graph_iri` (PIPELINE_SPINE §4 — the presenter is a pure keyed fold: it projects
/// the producer's already-parsed named graph, never re-parses the producer's byte
/// artifact). The producer attached it via `parse_into_graph`; this is the read half —
/// no parse. A missing producer HARD-fails (no-optionality).
///
/// `pub(crate)` (not module-private) because `docs_render`'s carrier-lane digests
/// (e.g. the per-term projection-loss join, B2) read a producer's named graph the
/// same way `assemble_carrier` does — one read helper, not a duplicate.
pub(crate) fn producer_graph(
    upstream: &BTreeMap<String, StageProduct>,
    stage: &str,
    graph_iri: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let product = upstream.get(stage).ok_or_else(|| {
        stage_err(&format!(
            "missing {stage} product for the <{graph_iri}> graph"
        ))
    })?;
    rooted_in_graph(
        &product.bundle().dataset().project_named_graph(graph_iri),
        graph_iri,
    )
}

fn assemble_carrier(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    // ── the self-description graphs ride in from stage-source-load's carrier ────
    // The presenter no longer loads or canonicalizes any source: the authored default,
    // imports, metadata, alignments, slice-analysis, verify attestation, and provenance
    // were all built ONCE at the parallel DAG root (`build_self_description_dataset`) and
    // are read here as pure projections. The authored default rides its own named graph;
    // re-rooting it with `project_named_graph` lands it back in the carrier's DEFAULT
    // graph (label dropped).
    let base = std::sync::Arc::new(
        source_load_dataset(upstream)?.project_named_graph(GRAPH_AUTHORED_DEFAULT),
    );
    // ── the first-class carrier graphs ride in from their producers' datasets ───
    // Each is read off the PRODUCER's attached named graph (a pure keyed fold), NOT
    // re-parsed from the producer's byte artifact (PIPELINE_SPINE §4 — the presenter
    // parses nothing). The statement layer is the producer's dataset (the parse of the
    // RDF-1.2 artifact, carried default-graph-only) re-rooted here — the same quads the
    // former `parse_into_graph(&rdf12, ...)` produced, off the already-attached dataset
    // instead of re-parsing the byte artifact. `stage-statements` carries only its default
    // graph (`gts_compose` folds it WHOLE, so it must not carry a named-graph copy), so
    // re-rooting the whole dataset is exactly the former per-parse named-graph fold.
    let statements = rooted_in_graph(
        upstream
            .get("stage-statements")
            .ok_or_else(|| stage_err("missing stage-statements product for the statement layer"))?
            .bundle()
            .dataset(),
        GRAPH_STATEMENTS,
    )?;
    let documentation = producer_graph(upstream, "stage-docs-render", GRAPH_DOCUMENTATION)?;
    // graph/diagnostics ← SHACL diagnostics (stage-validate) ∪ logic-compile diagnostics
    // (stage-compile-logic) ∪ chase certificates (stage-reason), each read off its
    // producer's attached graph and unioned here.
    let diagnostics = purrdf::RdfDataset::union(&[
        producer_graph(upstream, "stage-validate", GRAPH_DIAGNOSTICS)?.as_ref(),
        producer_graph(upstream, "stage-compile-logic", GRAPH_DIAGNOSTICS)?.as_ref(),
        producer_graph(upstream, "stage-reason", GRAPH_DIAGNOSTICS)?.as_ref(),
    ]);
    let conformance = producer_graph(upstream, "stage-conformance", GRAPH_CONFORMANCE)?;
    let projection_ledger = producer_graph(upstream, "stage-mappings", GRAPH_PROJECTION_LEDGER)?;
    let lang_translation_corpus =
        producer_graph(upstream, "stage-mappings", GRAPH_LANG_TRANSLATION_CORPUS)?;
    let lang_form_corpus = producer_graph(upstream, "stage-mappings", GRAPH_LANG_FORM_CORPUS)?;
    let lang_projection_corpus =
        producer_graph(upstream, "stage-mappings", GRAPH_LANG_PROJECTION_CORPUS)?;
    let lang_lowering_corpus =
        producer_graph(upstream, "stage-mappings", GRAPH_LANG_LOWERING_CORPUS)?;
    let lang_docs_rendering_corpus =
        producer_graph(upstream, "stage-mappings", GRAPH_LANG_DOCS_RENDERING_CORPUS)?;
    let correspondence_laws =
        producer_graph(upstream, "stage-mappings", GRAPH_CORRESPONDENCE_LAWS)?;
    // The on-disk projection of the correspondence-laws corpus: the SAME triples re-rooted into
    // their `graph/fanout/<path>` reconstruction graph so the superset gate folds them to
    // `generated/logic/gmeow.correspondence-laws.nt` (PIPELINE_SPINE §5 — RDF travels as RDF, so
    // the discharged `logic:SectionLaw` claims land in `generated/` too). The base
    // `graph/correspondence-laws` copy still serves the up-projection gates (single corpus, two
    // reconstruction roles — the diagnostics `.nq` follow the same twin-graph pattern).
    let correspondence_laws_fanout = {
        let iri = crate::stages::superset::rdf_fanout_graph_iri(CORRESPONDENCE_LAWS_PATH)
            .ok_or_else(|| stage_err("correspondence-laws fanout path is not an RDF path"))?;
        rooted_in_graph(correspondence_laws.as_ref(), &iri)?
    };
    // graph/quality-assessment — the per-slice `gmeow:QualityAssessment` corpus, read off
    // the stage-source-load product's attached graph (a pure keyed fold, PIPELINE_SPINE §4;
    // it was scored + attached ONCE at the DAG root). The base graph ships as a queryable
    // bundle graph; its fanout twin re-roots the SAME triples into their
    // `graph/fanout/<path>` reconstruction graph so the superset gate folds them to
    // `generated/quality/gmeow.quality-assessment.nt` (RDF travels as RDF — the assessment
    // lands in `generated/` too, not only as a bundle-internal named graph).
    let quality_assessment = source_load_graph(upstream, GRAPH_QUALITY_ASSESSMENT)?;
    let quality_assessment_fanout = {
        let iri = crate::stages::superset::rdf_fanout_graph_iri(QUALITY_ASSESSMENT_PATH)
            .ok_or_else(|| stage_err("quality-assessment fanout path is not an RDF path"))?;
        rooted_in_graph(quality_assessment.as_ref(), &iri)?
    };

    // ── the carried graphs ride in from the producers' carriers ────────────────
    let reason = upstream
        .get("stage-reason")
        .ok_or_else(|| stage_err("missing stage-reason product for the Reasoning handle"))?;
    let reasoning_iri = gmeow_logic::result_rdf::GRAPH_REASONING;

    // ── route every snapshot-owned source into its named graph, then union all ──
    let mut datasets: Vec<std::sync::Arc<purrdf::RdfDataset>> = vec![
        base,
        statements,
        // The self-description graphs are read (not re-loaded) off stage-source-load.
        source_load_graph(upstream, GRAPH_IMPORTS)?,
        source_load_graph(upstream, GRAPH_METADATA)?,
        // graph/alignments is a projection of the compiled SSSOM, so it rides off the
        // fresh stage-mappings product (not source-load's stale disk read).
        producer_graph(upstream, "stage-mappings", GRAPH_ALIGNMENTS)?,
        source_load_graph(upstream, GRAPH_SLICE_ANALYSIS)?,
        source_load_graph(upstream, GRAPH_VERIFY)?,
        source_load_graph(upstream, crate::stages::provenance_graph::GRAPH_PROVENANCE)?,
        documentation,
        std::sync::Arc::new(diagnostics),
        projection_ledger,
        lang_translation_corpus,
        lang_form_corpus,
        lang_projection_corpus,
        lang_lowering_corpus,
        lang_docs_rendering_corpus,
        correspondence_laws,
        correspondence_laws_fanout,
        quality_assessment,
        quality_assessment_fanout,
    ];
    // graph/math-producers/<name> — the seven `math:` producers' (five flagship producers,
    // the probability-model seam producer, and the p-value tri-slice producer) deterministic
    // RDF graphs, each read off the
    // `stage-math-producers` product's attached named graph (a pure keyed fold,
    // PIPELINE_SPINE §4) and folded into gmeow.gts (Design A — the producer output ships in
    // the bundle). Bundle-internal, like the `lang:` corpus graphs: they carry no
    // committed `generated/` file, so they map to no reconstruction rep (no orphan) and stay
    // OUT of the reasoned EDB (`gts_compose` folds only the default graph).
    for graph_iri in MATH_PRODUCER_GRAPHS {
        datasets.push(producer_graph(upstream, "stage-math-producers", graph_iri)?);
    }
    datasets.extend(compile_logic_carrier_graphs(upstream)?);
    datasets.push(rooted_in_graph(
        &reason.bundle().dataset().project_named_graph(reasoning_iri),
        reasoning_iri,
    )?);
    // graph/conformance is folded only when non-empty (an all-agree corpus has none).
    if conformance.quad_count() != 0 {
        datasets.push(conformance);
    }
    // graph/projections/<name>.edoal ← each committed EDOAL projection, one named
    // graph per file. EDOAL renders through the canonical-Turtle serializer, so the
    // fold of its named graph reproduces the committed bytes exactly (superset law).
    let mappings = upstream
        .get("stage-mappings")
        .ok_or_else(|| stage_err("missing stage-mappings product for projection graphs"))?;
    for (path, bytes) in mappings.artifacts() {
        if let Some(iri) = crate::stages::superset::edoal_projection_graph_iri(&path) {
            datasets.push(parse_into_graph(&bytes, "text/turtle", &iri)?);
        } else if crate::stages::superset::is_rdf_fanout_class(&path) {
            // The non-EDOAL RDF projections (core-prefixes / functions.fno /
            // list-functions), now emitted canonically by the mappings stage.
            if let Some(iri) = crate::stages::superset::rdf_fanout_graph_iri(&path) {
                datasets.push(parse_into_graph(&bytes, "text/turtle", &iri)?);
            }
        }
    }
    // graph/fanout/<path> ← every other RDF generated/ file, one named graph per file,
    // recomputed from THIS run's source. Each producing stage emits the committed file
    // as the canonical fold of these triples, so the superset gate reconstructs them
    // byte-for-byte (PIPELINE_SPINE §5; RDF travels as RDF, never a blob).
    for (path, bytes) in rdf_fanout_members(upstream)? {
        let iri = crate::stages::superset::rdf_fanout_graph_iri(&path)
            .ok_or_else(|| stage_err(&format!("non-RDF path in rdf_fanout_members: {path}")))?;
        let media_type = if path.ends_with(".nt") {
            "application/n-triples"
        } else if path.ends_with(".nq") {
            "application/n-quads"
        } else {
            "text/turtle"
        };
        datasets.push(parse_into_graph(&bytes, media_type, &iri)?);
    }
    let refs: Vec<&purrdf::RdfDataset> = datasets.iter().map(|d| d.as_ref()).collect();
    let composed = std::sync::Arc::new(purrdf::RdfDataset::union(&refs));

    // Reasoner-derived gate verdicts (logic:ruleGateFatalVerdict). The reason stage's
    // object-level EDB EXCLUDES the report graphs by construction, so reason_all never
    // materializes gmeow:findingGateVerdict for the shipped findings — an up-set finding
    // (Error / blocking category / Binding) rides the bundle missing its verdict and
    // gmeow:GateFatalUpsetShape fires under validate-gts. Run the AUTHORED rule (via the
    // native chase, never the Rust gate() morphism) over the COMPLETE composed bundle —
    // where every finding-bearing graph AND the rule + gmeow:categoryBlocking wiring are
    // assembled — and fold the derived verdicts into graph/diagnostics, so the shipped
    // gmeow.gts carries the ontology's entailment and the SHACL up-set shape agrees. The
    // rule + wiring are read from the authored stage-source-load base graph, never re-typed.
    let composed_final = if let Some(source_bytes) = upstream
        .get("stage-source-load")
        .and_then(|p| p.artifact(crate::stages::source_load::BASE_GRAPH_PATH))
        && let Some(gate) = crate::stages::gate_verdict::GateProgram::from_source(source_bytes)
    {
        let composed_nq = purrdf::canonical_flat_nquads(composed.as_ref())
            .map_err(|e| stage_err(&format!("serialize composed bundle for gate verdict: {e}")))?;
        let verdict_nq = gate
            .derived_verdict_nquads(&composed_nq, GRAPH_DIAGNOSTICS)
            .map_err(|e| stage_err(&format!("derive gate verdicts over the bundle: {e}")))?;
        if verdict_nq.is_empty() {
            composed
        } else {
            let verdicts = parse_dataset(verdict_nq.as_bytes(), "application/n-quads", None)
                .map_err(|e| stage_err(&format!("parse derived gate verdicts: {e}")))?;
            std::sync::Arc::new(purrdf::RdfDataset::union(&[
                composed.as_ref(),
                verdicts.as_ref(),
            ]))
        }
    } else {
        composed
    };

    // Fold the SCOPED COHERENCE CERTIFICATE into the terminal bundle (graph/attestations),
    // so EVERY gmeow.gts carries a budget-free, proof-carrying coherence attestation the
    // consumer reads directly — never recomputed per call (R6). The certificate is
    // computed ONCE here, over the fully-composed carrier, reusing stage-reason's single
    // reasoning pass (no second reason) and the ONE `build_coherence_outcome` construction
    // the release lane also uses.
    fold_coherence_certificate(composed_final, upstream)
}

/// The INJECTED issue timestamp the terminal coherence certificate carries. The
/// regenerate pipeline never samples a clock (the bundle must be byte-stable run to run —
/// the parity gate), so the certificate's `logic:checkIssuedAt` is pinned to the Unix
/// epoch, the SAME reproducible-build sentinel the packed-archive members zero their mtime
/// to. It is an injected constant, not a degraded fallback: a build-time coherence proof
/// over content-addressed bytes has no meaningful wall-clock instant.
const COHERENCE_ISSUED_AT: &str = "1970-01-01T00:00:00Z";

/// Fold the scoped coherence certificate over the fully-composed carrier into its
/// `graph/attestations` named graph, reusing `stage-reason`'s single reasoning pass.
///
/// The certificate's bundle identity is the content digest of the composed carrier BEFORE
/// the certificate graph is added (the certificate cannot hash the bytes it becomes part
/// of — the same pre-fold-hash discipline the release lane uses). The
/// [`crate::stages::release::build_coherence_outcome`] construction is shared, fed the
/// reused reasoning result and this carrier's identity. A REFUSED outcome (a forbidden
/// integrity violation) HARD-FAILS: an incoherent bundle must never ship a coherence
/// artifact (no-optionality / fail-closed). A certificate or the strictly-weaker
/// attestation folds as `graph/attestations` — always present, so the consumer read tool
/// never needs a recompute path.
fn fold_coherence_certificate(
    composed: std::sync::Arc<purrdf::RdfDataset>,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    // Reuse stage-reason's single reasoning pass (the typed Reasoning handle), never a
    // second reason over the carrier — the razor (PIPELINE_SPINE §3.2): reason once.
    let reason = upstream
        .get("stage-reason")
        .ok_or_else(|| stage_err("missing stage-reason product for the coherence certificate"))?;
    let reason_entry = reason
        .bundle()
        .handle(gmeow_logic::result_rdf::GRAPH_REASONING)
        .ok_or_else(|| stage_err("stage-reason product carries no Reasoning handle"))?;
    let crate::bundle::PipelineHandle::Reasoning(result) = &reason_entry.payload else {
        return Err(stage_err(
            "stage-reason handle for graph/reasoning is not the Reasoning arm",
        ));
    };

    // Bundle identity = digest of the composed carrier's canonical N-Quads, pinned with the
    // SAME digest primitive as the axiom-set hashes (so the certificate's bundle_hash and
    // axiom_hashes are computed under one deterministic primitive).
    let composed_nq = purrdf::canonical_flat_nquads(composed.as_ref()).map_err(|e| {
        stage_err(&format!(
            "serialize composed carrier for the coherence certificate: {e}"
        ))
    })?;
    let bundle_hash = purrdf::gts::writer::digest_string(composed_nq.as_bytes());

    let outcome = crate::stages::release::build_coherence_outcome(
        composed.as_ref(),
        result.as_ref(),
        bundle_hash,
        COHERENCE_ISSUED_AT,
    )?;
    if outcome.is_refused() {
        return Err(stage_err(
            "coherence certificate: the assembled bundle carries a forbidden integrity \
             violation; an incoherent bundle must not ship a coherence artifact",
        ));
    }
    let nquads = outcome.to_nquads(crate::stages::release::GRAPH_ATTESTATIONS);
    // A non-refused outcome always serializes (a certificate or the weaker attestation);
    // an empty projection here would mean the gate above failed to catch a refusal.
    if nquads.is_empty() {
        return Err(stage_err(
            "coherence certificate: a non-refused outcome projected no quads (internal invariant)",
        ));
    }
    let cert = parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("parse coherence certificate quads: {e}")))?;
    Ok(std::sync::Arc::new(purrdf::RdfDataset::union(&[
        composed.as_ref(),
        cert.as_ref(),
    ])))
}

/// Every remaining RDF `generated/` file (committed-path → bytes) that rides as an
/// RDF-fanout named graph, recomputed from THIS run's source. Each is the canonical
/// fold the producing stage also emits as its committed file. Grows class-by-class.
fn rdf_fanout_members(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // profiles — `owl:Ontology` import closures, canonical Turtle. Read off the
    // stage-export-profiles product (rendered once, in that leaf; keyed by full path).
    for (path, bytes) in producer_artifacts("stage-export-profiles", upstream)? {
        out.insert(path, bytes);
    }
    // research-objects — the RO-crate `.ttl` re-serializations + the `lillith.dcat.ttl`
    // CONSTRUCT (opaque JSON/HTML/XML members ride the opaque fanout blob). Read off the
    // stage-export-research-objects product; keep only the RDF members.
    for (path, bytes) in producer_artifacts("stage-export-research-objects", upstream)? {
        if is_rdf_member(&path) {
            out.insert(path, bytes);
        }
    }
    // evals scores.ttl — the meta-claim Assessments (opaque MD/JSON ride the blob). Read
    // off the stage-export-evals product; keep only the RDF members.
    for (path, bytes) in producer_artifacts("stage-export-evals", upstream)? {
        if is_rdf_member(&path) {
            out.insert(path, bytes);
        }
    }
    // gufo.ttl — the gUFO bridge projection, from the sink-consumed compile-logic
    // product (already emitted canonically).
    let gufo = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::GUFO_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/foundation/gufo.ttl in stage-compile-logic"))?;
    out.insert(crate::stages::compile_logic::GUFO_PATH.to_string(), gufo);

    // projection-report loss ledger (mappings) + the relational-core / correspondence
    // N-Triples programs (compile-logic), each emitted canonically by its stage.
    for (stage, path) in [
        (
            "stage-mappings",
            crate::stages::compile_logic::PROJECTION_REPORT_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::RELATIONAL_CORE_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::CORRESPONDENCE_PATH,
        ),
        // diagnostics `.nq` — one named graph per file (validate SHACL ∪ compile-logic),
        // each re-rooted into its own fanout container; the gate restamps back to the
        // shared `graph/diagnostics` label on fold.
        ("stage-validate", crate::stages::validate::SHACL_RDF_PATH),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_RDF_PATH,
        ),
        // The generated constraint catalog `.nq` — its own fanout named graph,
        // reconstructed byte-for-byte by the superset gate.
        (
            "stage-constraint-catalog",
            crate::stages::constraint_catalog::CONSTRAINT_CATALOG_RDF_PATH,
        ),
        // The generated term content manifest `.nq` — its own fanout named graph,
        // reconstructed byte-for-byte by the superset gate.
        (
            "stage-term-manifest",
            crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH,
        ),
    ] {
        let bytes = upstream
            .get(stage)
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| stage_err(&format!("missing {path} in {stage} for RDF fanout")))?;
        out.insert(path.to_string(), bytes);
    }
    Ok(out)
}

/// Assemble the OBJECT-LEVEL reasoned EDB: the authored default graph plus the
/// statement / import / alignment / logic / relational-core named graphs, in the
/// EXACT graph layout [`assemble_carrier`] uses (so the reasoned closure's worlds
/// match the bundle's). The shipped `graph/correspondence` graph stays meta-level and
/// is deliberately absent: its source/target endpoints describe mappings rather than
/// ontology axioms.
///
/// The meta/report graphs (metadata, slice-analysis, verify, documentation,
/// diagnostics, conformance, projection-ledger, provenance) are EXCLUDED: they assert
/// no object-level axioms, so they contribute zero inferences — reasoning over the
/// full fold vs this projection is isomorphic up to renaming of the content-addressed
/// Skolem witnesses. Excluding them makes the closure (and its witness IRIs) a
/// function of the ontology alone, not of its self-description. This is the single
/// EDB the sole `stage-reason` pass reasons over; it depends only on the
/// `stage-statements`, `stage-compile-logic`, `stage-source-load` products (the authored
/// / imports self-description graphs) — never on mapping/correspondence projections or
/// the snapshot, so reasoning need not wait on either. `stage-reason` consumes exactly
/// those three producers (see `run.rs`).
pub(crate) fn assemble_object_level_edb(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    // The authored default and imports are read (not re-loaded) off stage-source-load —
    // the same self-description graphs the presenter folds — so the reasoned closure's
    // worlds match the bundle's by construction, with ONE load. Mapping and
    // correspondence graphs are shipped by the presenter but stay meta-level, so no
    // external endpoint IRI can be mistaken for an authored object-level construct.
    let base = std::sync::Arc::new(
        source_load_dataset(upstream)?.project_named_graph(GRAPH_AUTHORED_DEFAULT),
    );
    let rdf12 = upstream
        .get("stage-statements")
        .and_then(|p| p.artifact(RDF12_PATH))
        .ok_or_else(|| stage_err("missing statements RDF 1.2 artifact"))?
        .to_vec();

    let mut datasets: Vec<std::sync::Arc<purrdf::RdfDataset>> = vec![
        base,
        parse_into_graph(&rdf12, "text/turtle", GRAPH_STATEMENTS)?,
        source_load_graph(upstream, GRAPH_IMPORTS)?,
    ];
    datasets.extend(compile_logic_object_graphs(upstream)?);
    let refs: Vec<&purrdf::RdfDataset> = datasets.iter().map(|d| d.as_ref()).collect();
    Ok(std::sync::Arc::new(purrdf::RdfDataset::union(&refs)))
}

/// Project a shipped snapshot back to the exact object-level EDB admitted by
/// [`assemble_object_level_edb`]. The authored default graph remains default-world;
/// statement/import/logic/relational-core worlds retain their graph names. Every
/// mapping, correspondence, report, documentation, and fanout graph is excluded.
///
/// This is the single snapshot-reader boundary used by the maintainer reasoning CLI.
/// Keeping it beside the producer-side assembly prevents `--fresh` and `reason-gate`
/// from accidentally reasoning over more of the shipped ontology than the pipeline
/// authority did.
pub fn snapshot_reasoning_edb(
    snapshot: &purrdf::RdfDataset,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    fn admitted_graph(graph: &Option<RdfTerm>) -> bool {
        match graph {
            None => true,
            Some(RdfTerm::Iri(iri)) => {
                iri == GRAPH_STATEMENTS
                    || iri == GRAPH_IMPORTS
                    || crate::stages::compile_logic::OBJECT_LEVEL_GRAPHS.contains(&iri.as_str())
            }
            Some(_) => false,
        }
    }

    let mut builder = RdfDatasetBuilder::new();
    for quad in snapshot.owned_quads() {
        if admitted_graph(&quad.graph_name) {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in snapshot.owned_reifiers() {
        if admitted_graph(&reifier.graph) {
            builder.push_owned_reifier(&reifier);
        }
    }
    for annotation in snapshot.owned_annotations() {
        if admitted_graph(&annotation.graph) {
            builder.push_owned_annotation(&annotation);
        }
    }
    builder
        .freeze()
        .map_err(|e| stage_err(&format!("freeze snapshot object-level reasoning EDB: {e}")))
}

#[cfg(test)]
mod reasoning_edb_projection_tests {
    use super::*;

    #[test]
    fn shipped_correspondence_and_alignment_targets_never_enter_reasoning() {
        let trig = format!(
            "@prefix ex: <https://example.test/> .\n\
             ex:authored ex:p ex:o .\n\
             GRAPH <{GRAPH_STATEMENTS}> {{ ex:statement ex:p ex:o . }}\n\
             GRAPH <{GRAPH_IMPORTS}> {{ ex:imported ex:p ex:o . }}\n\
             GRAPH <{logic}> {{ ex:logic ex:p ex:o . }}\n\
             GRAPH <{relational}> {{ ex:relational ex:p ex:o . }}\n\
             GRAPH <{GRAPH_ALIGNMENTS}> {{ ex:map ex:target <http://www.w3.org/2002/07/owl#maxCardinality> . }}\n\
             GRAPH <{correspondence}> {{ ex:corr ex:target <http://www.w3.org/2002/07/owl#InverseFunctionalProperty> . }}\n\
             GRAPH <{reasoning}> {{ ex:result ex:p ex:o . }}\n",
            logic = crate::stages::compile_logic::GRAPH_LOGIC,
            relational = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE,
            correspondence = crate::stages::compile_logic::GRAPH_CORRESPONDENCE,
            reasoning = gmeow_logic::result_rdf::GRAPH_REASONING,
        );
        let snapshot = parse_dataset(trig.as_bytes(), "application/trig", None)
            .expect("parse snapshot-shaped fixture");
        let edb = snapshot_reasoning_edb(snapshot.as_ref()).expect("project reasoning EDB");

        assert_eq!(
            edb.quad_count(),
            5,
            "default plus four admitted reasoning worlds"
        );
        let graph_iris: std::collections::BTreeSet<String> = edb
            .owned_quads()
            .filter_map(|quad| match quad.graph_name {
                Some(RdfTerm::Iri(iri)) => Some(iri),
                _ => None,
            })
            .collect();
        assert!(!graph_iris.contains(GRAPH_ALIGNMENTS));
        assert!(!graph_iris.contains(crate::stages::compile_logic::GRAPH_CORRESPONDENCE));
        assert!(!graph_iris.contains(gmeow_logic::result_rdf::GRAPH_REASONING));

        let coverage = gmeow_logic::reason::dl::scan_coverage(edb.as_ref())
            .expect("scan projected EDB coverage");
        assert!(
            coverage.unsupported.is_empty(),
            "meta-level target references must not become DL coverage gaps: {:?}",
            coverage.unsupported
        );
    }
}

/// Project the selected compile-logic named graphs off the stage product and re-root
/// each into its carrier graph. The caller chooses the complete shipped set or the
/// strictly object-level reasoning subset; keeping that distinction explicit prevents
/// the correspondence meta-formula envelope from leaking into closure.
fn compile_logic_graphs(
    upstream: &BTreeMap<String, StageProduct>,
    graph_iris: &[&str],
) -> Result<Vec<std::sync::Arc<purrdf::RdfDataset>>, gmeow_errors::Diag> {
    let compile = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product"))?;
    graph_iris
        .iter()
        .map(|iri| rooted_in_graph(&compile.bundle().dataset().project_named_graph(iri), iri))
        .collect()
}

/// Every compile-logic graph shipped by [`assemble_carrier`], including the
/// meta-level correspondence program and its digest-pinned handle backing graph.
fn compile_logic_carrier_graphs(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<std::sync::Arc<purrdf::RdfDataset>>, gmeow_errors::Diag> {
    compile_logic_graphs(upstream, &crate::stages::compile_logic::CARRIER_GRAPHS)
}

/// Only the compile-logic graphs admitted to object-level reasoning.
fn compile_logic_object_graphs(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<std::sync::Arc<purrdf::RdfDataset>>, gmeow_errors::Diag> {
    compile_logic_graphs(upstream, &crate::stages::compile_logic::OBJECT_LEVEL_GRAPHS)
}

/// Serialize the fully-assembled carrier to the `dist`-profile `gmeow.gts` bytes: fold
/// the carrier into the snapshot frame (native ingestion), staple the JSON-LD-star /
/// OKF / caller blobs, and emit. The SOLE serialization of the snapshot.
fn serialize_snapshot(
    carrier: &purrdf::RdfDataset,
    extra_graphs: &[std::sync::Arc<purrdf::RdfDataset>],
    blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut builder = SnapshotBuilder::new();
    builder
        .add_dataset(carrier)
        .map_err(|e| stage_err(&format!("fold carrier into snapshot: {e}")))?;
    // Carrier-time named graphs (e.g. the docs-format grounding, which content-addresses
    // the packed docs blobs built in this stage) fold in alongside the assembled carrier.
    for graph in extra_graphs {
        builder
            .add_dataset(graph)
            .map_err(|e| stage_err(&format!("fold carrier-time named graph into snapshot: {e}")))?;
    }
    crate::gts_profile::emit_gmeow_gts(&builder, blobs, report_blobs, None, None, None)
        .map_err(|e| stage_err(&format!("emit_gts: {e}")))
}

/// Parse `bytes` natively and re-root every quad into `graph_iri` (see
/// [`rooted_in_graph`]).
pub(crate) fn parse_into_graph(
    bytes: &[u8],
    media_type: &str,
    graph_iri: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let parsed = parse_dataset(bytes, media_type, None)
        .map_err(|e| stage_err(&format!("parse <{graph_iri}>: {e}")))?;
    rooted_in_graph(&parsed, graph_iri)
}

/// Borrow the upstream `stage-snapshot` product (or HARD-fail if a leaf forgot to
/// declare the consumes edge — fail-closed, no-optionality).
fn snapshot_product(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<&StageProduct, gmeow_errors::Diag> {
    upstream
        .get("stage-snapshot")
        .ok_or_else(|| stage_err("missing stage-snapshot product"))
}

/// THIS run's terminal carrier dataset, read DIRECTLY off the `stage-snapshot`
/// product's bundle. The single internal transport: every export leaf reads
/// the carrier here instead of re-parsing the `gmeow.gts` bytes — GTS is exit-only,
/// produced by the terminal writer (`gts_sink`), never an internal transport.
///
/// The returned `Arc<RdfDataset>` shares the immutable carrier (no copy, no re-parse);
/// the carrier already holds every named graph the snapshot folds (authored default,
/// statements, imports, metadata, alignments, slice-analysis, verify, documentation,
/// diagnostics, conformance, projection-ledger, provenance, logic, relational-core,
/// correspondence, reasoning).
pub(crate) fn snapshot_dataset(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    Ok(snapshot_product(upstream)?.bundle().dataset_arc())
}

// ── Archive blobs (regression fix) ──────────────────────────────────────────────
//
// The pre-pipeline generator folded five TAR archives into `gmeow.gts` —
// `mappings-archive` / `cells-archive` / `queries-archive` / `tests-archive` /
// `schemas-archive` —
// that the wheel-mode consumer loaders read back (`gmeow_tools.bundle`:
// `bundled_sssom` / `bundled_cells` / `bundled_queries` / `bundled_tests`). The
// pipeline cutover dropped the WRITER (only the reader survived, orphaned),
// so a repo-free `gmeow.gts` lost its lift maps / cells / queries / test specs and
// every wheel-mode consumer (up-projection, docs-from-bundle, export) broke. This
// restores the writer as a dep-free, byte-deterministic USTAR codec (sorted
// members, zeroed mtime/uid/gid, mode 0644) so the composed snapshot stays
// fold-stable. Member-name conventions MIRROR the reader: mappings/queries use the
// bare filename; cells/tests preserve the repo-relative path (so
// `bundled_cells_under(prefix)` can route by directory).

/// tar of the OPAQUE (non-RDF) fanout files under `generated/` that no dedicated
/// rep already carries — the byte-exact members are recomputed from THIS run's
/// carrier (carrier-reading leaves) or source (source-reading leaves) inside the
/// snapshot and keyed repo-relative. RDF outputs (`.ttl`/`.nt`/`.nq`) are NEVER
/// here: they ride as named graphs so the superset law reconstructs them as folds.
const REP_GENERATED: &str = "generated-opaque-archive";

const REP_MAPPINGS: &str = "mappings-archive";
const REP_CELLS: &str = "cells-archive";
const REP_QUERIES: &str = "queries-archive";
const REP_TESTS: &str = "tests-archive";
/// tar of the SHACL-derived JSON Schema + OpenAPI, member = bare filename.
const REP_SCHEMAS: &str = "schemas-archive";
/// tar of the generated Pydantic model package, member = package-relative path
/// (`gmeow_models/...`). Re-exported from the reader-side definition in
/// [`crate::bundle_blobs`] so producer and reader share ONE constant (a drifted
/// label would silently fold/read an empty package).
pub(crate) use crate::bundle_blobs::REP_MODELS_PYTHON;
/// tar of the JSON-LD-star + YAML-LD-star serializations.
#[cfg(test)]
#[allow(dead_code)]
const REP_YAMLLD: &str = "yaml-ld-archive";
/// tar of the Rust-rendered OKF bundle, member = `gmeow-okf/...`.
#[cfg(test)]
const REP_OKF: &str = "okf-export";
/// The full rendered ontology-docs static site. The rep MUST equal the
/// string the runtime consumer (`create_docs._unpack_doc_archive`) looks up —
/// `"ontology-docs"`, NOT an `-archive` variant — so `gmeow export-docs` finds it.
#[cfg(test)]
const REP_ONTOLOGY_DOCS: &str = "ontology-docs";
/// tar of the FULL SHACL shape surface, member = repo-relative path:
/// every `shapes/*.ttl` (incl. the 4 DSL/manifest lints the consumer's DSL phases
/// need) + every `generated/shapes/*.ttl` (P11 frame shapes) + every per-slice
/// `slices/<g>/<n>/shapes.ttl`. The full surface — NOT the validator's filtered
/// union — so a repo-free `gmeow validate` can re-derive both the data-graph
/// union and the DSL phases. The Python reader (`bundle.bundled_shapes`) MUST use
/// this exact rep string.
const REP_SHAPES: &str = "shapes-archive";
/// tar of the compiled logic/DL projection surface, member = repo-relative
/// path: the small committed projections in [`AXIOM_FILES`]. NOT the big reasoning
/// OUTPUTS (inferred-closure / reasoning-explanations / dl-el-crosscheck-report),
/// which ride other channels. The Python reader (`bundle.bundled_axioms`) MUST use
/// this exact rep string.
const REP_AXIOMS: &str = "axioms-archive";
/// The compiled logic/DL projection files folded as [`REP_AXIOMS`]: the
/// small, committed, drift-gated projections a repo-free consumer needs. The
/// big reasoning outputs are deliberately excluded. Order is canonical for the
/// fail-closed scan; the archive re-sorts members by key for determinism.
const AXIOM_FILES: [&str; 4] = [
    "generated/owl/gmeow-dl.ttl",
    "generated/owl/gmeow-el.ttl",
    "generated/logic/gmeow.logic.rdf12.ttl",
    "generated/datalog/gmeow.dl",
];
/// tar of the native reasoner's REPORT artifacts: the entailment
/// explanations + the DL/EL cross-check ledger over THIS run's reasoned closure. The
/// closure itself already rides the bundle GRAPH (gts-compose folds `stage-reason`'s
/// closure); the reports are deliberately kept OUT of the ontology graph, so this
/// blob channel is how a repo-free consumer reads WHY each entailment holds and the
/// DL/EL agreement ledger WITHOUT re-running the engine (maximal information flow).
/// The Python reader (`bundle.bundled_reasoning`) MUST use this exact rep string.
const REP_REASONING: &str = "reasoning-archive";
const ARCHIVE_MEDIA_TYPE: &str = "application/x-tar";

/// THIS run's three generated SHACL shape surfaces, folded into REP_SHAPES from the
/// producing export leaves' products (never a stale disk read). Grouped into named
/// fields so the three same-typed `&[u8]` cannot be transposed at the call site.
struct ShapeSurfaces<'a> {
    result: &'a [u8],
    frame: &'a [u8],
    constraint: &'a [u8],
}

/// The four JSON Schema surfaces folded into REP_SCHEMAS, all sourced from THIS
/// run's `stage-export-json-schema` product. Grouped into named fields (like
/// [`ShapeSurfaces`]) so the same-typed `&[u8]` cannot be transposed at the call
/// site: the two SHACL-derived documents (`schema` = `gmeow.schema.json`, `openapi`
/// = `gmeow.openapi.json`) and the two hand-authored self-describing schemas (`card`
/// = `card.schema.json`, `finding` = `validate-finding.schema.json`).
struct SchemaSurfaces<'a> {
    schema: &'a [u8],
    openapi: &'a [u8],
    card: &'a [u8],
    finding: &'a [u8],
}

/// Build the bundle archive blobs from the repo tree: mappings, cells, queries,
/// tests, schemas, the SHACL shape surface, and the compiled logic/DL axiom
/// surface. The SHACL-derived JSON Schema + OpenAPI bytes are passed in from
/// THIS run's `stage-export-json-schema` product (not re-read from disk) so a single
/// regenerate folds the fresh schema — the committed `generated/schemas/*.json` are
/// not flushed until phase 1 returns.
fn build_archive_blobs(
    root: &Path,
    schema_surfaces: &SchemaSurfaces<'_>,
    axiom_artifacts: &BTreeMap<String, Vec<u8>>,
    mappings_artifacts: &BTreeMap<String, Vec<u8>>,
    shape_surfaces: &ShapeSurfaces<'_>,
    models_python_artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    // mappings: member = bare filename, sourced from THIS run's stage-mappings product
    // (not re-read from disk) so a mapping-source edit folds into the bundle in one
    // regenerate — the committed generated/mappings/*.sssom.tsv are not written until
    // phase 1 returns, so a disk read here would tar the stale committed set.
    let mappings =
        members_basename_from_artifacts(mappings_artifacts, "generated/mappings/", ".sssom.tsv");
    // Fail closed, mirroring the axioms guard below: an empty match means the
    // stage-mappings product keyed its SSSOM under an unexpected prefix (or emitted
    // none), which would silently fold an EMPTY mappings archive into the bundle. A
    // missing required surface is a hard error, never a degraded fallback.
    if mappings.is_empty() {
        return Err(stage_err(
            "no generated/mappings/*.sssom.tsv artifacts in the stage-mappings product — \
             the mappings archive would fold empty (fail-closed)",
        ));
    }
    // queries: member = bare filename, sourced from THIS run's stage-mappings product
    // (not re-read from disk) so a generated-query edit folds into the bundle in one
    // regenerate — the committed generated/queries/*.rq are not written until phase 1
    // returns, so a disk read here would tar the stale committed set (the same
    // stale-disk-fold trap the mappings archive above avoids). `stage-mappings` is
    // already consumed by the sink, so no new consumes edge is required.
    let queries = members_basename_from_artifacts(mappings_artifacts, "generated/queries/", ".rq");
    // Fail closed, mirroring the mappings guard above: an empty match means the
    // stage-mappings product keyed its `.rq` under an unexpected prefix (or emitted
    // none), which would silently fold an EMPTY queries archive. A missing required
    // surface is a hard error, never a degraded fallback.
    if queries.is_empty() {
        return Err(stage_err(
            "no generated/queries/*.rq artifacts in the stage-mappings product — \
             the queries archive would fold empty (fail-closed)",
        ));
    }
    // schemas: the SHACL-derived JSON Schema + OpenAPI, member = bare
    // filename, taken from the in-memory stage product so the bundle never lags the
    // committed files by a regenerate. Bare-filename member names
    // (`gmeow.schema.json` / `gmeow.openapi.json`), so the fold is stable.
    let schemas = vec![
        (
            "gmeow.schema.json".to_string(),
            schema_surfaces.schema.to_vec(),
        ),
        (
            "gmeow.openapi.json".to_string(),
            schema_surfaces.openapi.to_vec(),
        ),
        (
            "card.schema.json".to_string(),
            schema_surfaces.card.to_vec(),
        ),
        (
            "validate-finding.schema.json".to_string(),
            schema_surfaces.finding.to_vec(),
        ),
    ];
    // cells: equivalences + projections + slice mappings, member = repo-relative path.
    let mut cells: Vec<(String, Vec<u8>)> = Vec::new();
    cells.extend(members_relpath(
        root,
        &list_files(&root.join("dsl/mappings/equivalences"), "ttl")?,
    )?);
    cells.extend(members_relpath(
        root,
        &list_files(&root.join("dsl/mappings/projections"), "ttl")?,
    )?);
    cells.extend(members_relpath(root, &slice_files(root, "mappings")?)?);
    cells.sort_by(|a, b| a.0.cmp(&b.0));
    // tests: slices/*/*/tests/*.ttl (non-recursive), member = repo-relative path.
    let mut tests = members_relpath(root, &slice_files(root, "tests")?)?;
    tests.sort_by(|a, b| a.0.cmp(&b.0));
    // shapes: the FULL SHACL surface, member = repo-relative path —
    // shapes/*.ttl (authored source) + the four generated/shapes/*.ttl members
    // (product-sourced below, P11 fail-closed) + slices/<g>/<n>/shapes.ttl. Carried
    // whole so a repo-free `gmeow validate` can reassemble both the data-graph union
    // and the DSL phases. The `generated/shapes/*.ttl` members are NEVER read off disk:
    // every one is a produced projection whose committed file the fanout rewrites from
    // the bundle, so a disk read would freeze the last-committed bytes forever (the
    // stale-disk-fold class). They are folded from THIS run's consumed products instead.
    let mut shapes: Vec<(String, Vec<u8>)> =
        members_relpath(root, &list_files(&root.join("shapes"), "ttl")?)?;
    shapes.extend(members_relpath(
        root,
        &slice_named_files(root, "shapes.ttl")?,
    )?);
    // The four generated/shapes/*.ttl members, each product-sourced (no disk enumeration):
    //   validation-shapes.ttl ← stage-compile-logic (OPT axis + OWL-restriction derivation)
    //   result-shapes.ttl     ← stage-export-result-shapes (ResultShape SHACL projection)
    //   frame-shapes.ttl      ← stage-export-frame-shapes (P11 frame relativity)
    //   constraint-shapes.ttl ← stage-export-constraint-shapes (logic: FOL-axiom projection)
    // Each MUST exist in its product (no-optionality, fail-closed): validation-shapes is
    // pulled from `axiom_artifacts` with a hard error on absence; result/frame/constraint
    // arrive as `shape_surfaces` fields already hard-failed at the call site in
    // `serialize_carrier_snapshot`. constraint-shapes does not exist on disk on a first
    // run at all, so only the fresh product can carry it (H8) — the very reason a disk
    // enumeration was wrong. This replaces the P11 "fail-closed if none" disk guard.
    let validation_shapes = axiom_artifacts
        .get(crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH)
        .ok_or_else(|| {
            stage_err(
                "carrier: stage-compile-logic produced no validation-shapes.ttl product; refusing \
                 to carry a stale on-disk read (P11 enforcement, fail-closed)",
            )
        })?;
    // The procedural-constraint SHACL surface (every logic:Constraint → sh:SPARQLConstraint) is
    // ALSO produced by stage-compile-logic, so it folds from the same product — header-only until
    // constraints are authored, and (like constraint-shapes.ttl) it does not exist on disk on a
    // first run at all, so only the fresh product can carry it (fail-closed, no stale disk read).
    let procedural_constraints = axiom_artifacts
        .get(crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH)
        .ok_or_else(|| {
            stage_err(
                "carrier: stage-compile-logic produced no procedural-constraints.ttl product; \
                 refusing to carry a stale on-disk read (P11 enforcement, fail-closed)",
            )
        })?;
    for (rel, fresh_bytes) in [
        (
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
            validation_shapes.as_slice(),
        ),
        (
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            procedural_constraints.as_slice(),
        ),
        (
            crate::stages::result_shapes::RESULT_SHAPES_PATH,
            shape_surfaces.result,
        ),
        (
            crate::stages::frame_shapes::FRAME_SHAPES_PATH,
            shape_surfaces.frame,
        ),
        (
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
            shape_surfaces.constraint,
        ),
    ] {
        if let Some(entry) = shapes.iter_mut().find(|(k, _)| k == rel) {
            entry.1 = fresh_bytes.to_vec();
        } else {
            shapes.push((rel.to_string(), fresh_bytes.to_vec()));
        }
    }
    shapes.sort_by(|a, b| a.0.cmp(&b.0));
    // axioms: the compiled logic/DL projection surface, member = repo-relative path.
    // Sourced from THIS run's `stage-compile-logic` product (not re-read from disk) so
    // a single regenerate folds the fresh projections — the committed files are not
    // flushed until phase 1 returns. Each MUST exist (no-optionality, fail-closed): a
    // partial archive would silently break the consumer.
    let mut axioms: Vec<(String, Vec<u8>)> = Vec::with_capacity(AXIOM_FILES.len());
    for rel in AXIOM_FILES {
        let bytes = axiom_artifacts.get(rel).ok_or_else(|| {
            stage_err(&format!(
                "missing axiom artifact {rel} in the stage-compile-logic product (fail-closed)"
            ))
        })?;
        axioms.push((rel.to_string(), bytes.clone()));
    }
    axioms.sort_by(|a, b| a.0.cmp(&b.0));
    // models-python: the generated Pydantic package, member = package-relative path
    // (`gmeow_models/...`, the on-disk `packages/python/` prefix stripped). Sourced
    // from THIS run's stage-export-pydantic product so the bundle blob, the on-disk
    // wheel source tree, and `gmeow export-docs --format pydantic` are the SAME bytes
    // (four-way identity). Fail closed: an empty package would silently ship a bundle
    // without the documentation surface.
    let mut models_python: Vec<(String, Vec<u8>)> = models_python_artifacts
        .iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix(crate::stages::pydantic::PACKAGE_DISK_PREFIX)
                .map(|member| (member.to_string(), bytes.clone()))
        })
        .collect();
    models_python.sort_by(|a, b| a.0.cmp(&b.0));
    if models_python.is_empty() {
        return Err(stage_err(
            "no packages/python/gmeow_models/* artifacts in the stage-export-pydantic product — \
             the models-python archive would fold empty (fail-closed)",
        ));
    }
    Ok(vec![
        archive_blob(REP_MAPPINGS, &mappings)?,
        archive_blob(REP_CELLS, &cells)?,
        archive_blob(REP_QUERIES, &queries)?,
        archive_blob(REP_TESTS, &tests)?,
        archive_blob(REP_SCHEMAS, &schemas)?,
        archive_blob(REP_SHAPES, &shapes)?,
        archive_blob(REP_AXIOMS, &axioms)?,
        archive_blob(REP_MODELS_PYTHON, &models_python)?,
    ])
}

/// The full committed-path → bytes artifact map a producing `stage` attached to the
/// carrier. The presenter reads a leaf's rendered output off its product here rather
/// than re-rendering it from disk — the render happens ONCE, in the producing stage
/// (PIPELINE_SPINE §3.2 the transform-once razor; §4 the terminal recomputes no view).
/// A missing product HARD-fails (no-optionality): the stage MUST be declared upstream.
fn producer_artifacts(
    stage: &str,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Ok(upstream
        .get(stage)
        .ok_or_else(|| stage_err(&format!("missing {stage} product for the fanout presenter")))?
        .artifacts())
}

/// One byte-artifact `path` off a producing `stage`'s product (see [`producer_artifacts`]).
fn producer_artifact(
    stage: &str,
    path: &str,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    upstream
        .get(stage)
        .and_then(|p| p.artifact(path))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            stage_err(&format!(
                "missing {path} in {stage} for the fanout presenter"
            ))
        })
}

/// Fold the opaque (non-RDF, not-already-carried) members of one leaf's output into
/// the fanout member map.
fn take_opaque(members: &mut BTreeMap<String, Vec<u8>>, arts: BTreeMap<String, Vec<u8>>) {
    for (path, bytes) in arts {
        if !is_rdf_member(&path) && !opaque_already_carried(&path) {
            members.insert(path, bytes);
        }
    }
}

/// Whether an archive blob `rep` can contain committed `generated/` reconstruction
/// targets, so the superset gate decodes it. Excludes the source archives
/// (cells/tests carry `dsl/`+`slices/`), the `reason/` reports, the per-slice
/// guides, and the large `ontology-docs`/`okf`/`yaml-ld` payloads — none back a
/// `generated/` file, and the docs/okf archives are large enough to trip the zstd
/// decode safety bound. One authority, consulted by the gate.
pub(crate) fn archive_rep_carries_generated(rep: &str) -> bool {
    matches!(
        rep,
        REP_MAPPINGS | REP_QUERIES | REP_SCHEMAS | REP_AXIOMS | REP_SHAPES | REP_GENERATED
    )
}

/// The committed repo-relative path an archive member reconstructs — the inverse of
/// this stage's member-naming conventions, so the superset gate and the fanout
/// projection resolve a blob member to its `generated/` path without guessing. The
/// basename-keyed reps (`REP_MAPPINGS`/`REP_QUERIES`/`REP_SCHEMAS`, keyed by bare
/// filename in their single directory via `members_basename_from_artifacts`) get their directory
/// prefix restored here; the repo-relative reps (`REP_AXIOMS`/`REP_SHAPES`/
/// `REP_GENERATED`, keyed by `members_relpath`) pass through unchanged. One authority:
/// carrier.rs owns both the forward member naming and this inverse. Returns `None`
/// for a rep that carries no committed `generated/` file (mirrors
/// [`archive_rep_carries_generated`]).
pub(crate) fn committed_path_for_archive_member(rep: &str, member: &str) -> Option<String> {
    match rep {
        REP_MAPPINGS => Some(format!("generated/mappings/{member}")),
        REP_QUERIES => Some(format!("generated/queries/{member}")),
        REP_SCHEMAS => Some(format!("generated/schemas/{member}")),
        // Already repo-relative (`generated/...` or source `shapes/`/`slices/`).
        REP_AXIOMS | REP_SHAPES | REP_GENERATED => Some(member.to_string()),
        _ => None,
    }
}

/// Whether `path` is an RDF text artifact (carried as a NAMED GRAPH, never a blob).
fn is_rdf_member(path: &str) -> bool {
    path.ends_with(".ttl") || path.ends_with(".nt") || path.ends_with(".nq")
}

/// Whether an opaque `generated/` file is already carried by another rep, so the
/// fanout archive must not double-carry it (keeps the superset reverse sweep clean).
fn opaque_already_carried(path: &str) -> bool {
    path.ends_with(".sssom.tsv")                        // REP_MAPPINGS
        || path.ends_with(".rq")                        // REP_QUERIES
        || path == "generated/schemas/gmeow.schema.json"  // REP_SCHEMAS
        || path == "generated/schemas/gmeow.openapi.json" // REP_SCHEMAS
        || path == "generated/schemas/card.schema.json"   // REP_SCHEMAS
        || path == "generated/schemas/validate-finding.schema.json" // REP_SCHEMAS
        || path == "generated/datalog/gmeow.dl" // REP_AXIOMS
}

/// The two generated validation-shape surfaces (SHACL Core Turtle + ShEx compact),
/// read ONCE off THIS run's `stage-compile-logic` product — the SINGLE source both the
/// `REP_GENERATED` archive fold (committed-file reconstruction) and the typed
/// `Shacl`/`Shex` consumer sidecars ([`build_validation_shape_typed_blobs`]) draw from,
/// so the two channels can never drift from one another or from the committed files.
/// Hard-fails if either surface is absent from the product (no-optionality, fail-closed).
fn validation_shape_surfaces(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<(Vec<u8>, Vec<u8>), gmeow_errors::Diag> {
    let shacl = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            stage_err("missing generated/shapes/validation-shapes.ttl in stage-compile-logic")
        })?;
    let shex = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            stage_err("missing generated/shapes/validation-shapes.shex in stage-compile-logic")
        })?;
    Ok((shacl, shex))
}

/// The typed `Shacl`/`Shex` consumer sidecars for the validation surface: the two
/// generated validation-shape surfaces carried as content-addressed blobs whose media
/// type classifies each on gts decode into its typed lookaside kind
/// ([`purrdf::RdfLookasideKind::Shacl`] / [`purrdf::RdfLookasideKind::Shex`], via
/// `purrdf::gts::lookaside_from_graph`), so a repo-free consumer reads the validation
/// surface under its typed kind without re-running the compiler (LOGIC-VALIDATION.md).
///
/// ADDITIVE: the SAME bytes also ride the `REP_GENERATED` archive (the committed-file
/// reconstruction the fanout/superset gate depends on); both channels draw from
/// [`validation_shape_surfaces`] (ONE source, no drift). The `rep` carries the committed
/// path so a consumer recovers the surface's logical path from the blob metadata. The
/// two blobs carry distinct reps, so the blob-frame sort in `emit_gts` is deterministic.
fn build_validation_shape_typed_blobs(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    let (shacl, shex) = validation_shape_surfaces(upstream)?;
    Ok(vec![
        BlobRow {
            data: shacl,
            media_type: VALIDATION_SHACL_MEDIA_TYPE.to_string(),
            rep: crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH.to_string(),
        },
        BlobRow {
            data: shex,
            media_type: VALIDATION_SHEX_MEDIA_TYPE.to_string(),
            rep: crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH.to_string(),
        },
    ])
}

/// Build the generated-fanout archive [`REP_GENERATED`]: the byte-exact `generated/`
/// fanout members that ride as opaque byte projections (as opposed to named-graph
/// folds). Each rides in from a sink-consumed stage product — either projected from
/// THIS run's carrier dataset (lpg / schemas) or read off its producing export leaf's
/// product (the render ran once, in the leaf; the presenter never re-renders from
/// disk). Byte-decorated RDF reports whose committed form carries generated comments /
/// section markers ride here rather than as canonical graph folds. The bytes are
/// byte-identical to the committed files, which the superset gate proves.
fn build_fanout_opaque_blob(
    carrier: &purrdf::RdfDataset,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // carrier-reading leaves (project from THIS run's carrier dataset — §8 permits a
    // side format produced from the in-memory carrier, never from disk).
    take_opaque(
        &mut members,
        crate::stages::lpg::render_from_dataset(carrier)?,
    );
    take_opaque(
        &mut members,
        crate::stages::schemas::render_schemas_from_dataset(carrier)?,
    );

    // source-reading export leaves: read their ALREADY-rendered output off the producing
    // stage's product (the render ran once, in the leaf), never re-rendered from disk.
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-references", upstream)?,
    );
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-bench", upstream)?,
    );
    // The deterministic engine-cost ledger: projected once in stage-export-cost-ledger
    // from the committed cost baseline; read off its product, never re-rendered from disk.
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-cost-ledger", upstream)?,
    );
    // The external-corpus agreement matrix: projected once in stage-export-agreement
    // from the single grade's attached tallies; read off its product, never re-rendered.
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-agreement", upstream)?,
    );
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-apache", upstream)?,
    );
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-matrix", upstream)?,
    );
    // The two slice-quality floor TSVs (P17 projection of the ontology-resident
    // gmeow:AxisFloorCommitment / gmeow:SliceTierFloor individuals): projected once in
    // stage-export-governance-floors from the rubric slice; read off its product, never
    // re-rendered from disk. Non-RDF, so they ride here as opaque byte members.
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-governance-floors", upstream)?,
    );
    // The two projection-vocabulary ratchet TSVs (P17 projection of the ontology-resident
    // gmeow:ProjectionCeilingCommitment / gmeow:ProjectionVocabulary individuals):
    // projected once in stage-export-projection-ceilings from the rubric slice; read off
    // its product, never re-rendered from disk. Non-RDF, so they ride here as opaque
    // byte members.
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-projection-ceilings", upstream)?,
    );

    // evals + research-objects: the OPAQUE members only (their `.ttl`/`.dcat.ttl` ride
    // as named graphs). Read off the producing leaf's product — byte-identical.
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-evals", upstream)?,
    );
    take_opaque(
        &mut members,
        producer_artifacts("stage-export-research-objects", upstream)?,
    );

    // diagnostics sidecars (`.json`/`.sarif`/`.html`) ride in from the sink-consumed
    // validate + compile-logic products; the `.nq` graphs ride as named graphs.
    for (stage, path) in [
        ("stage-validate", crate::stages::validate::SHACL_JSON_PATH),
        ("stage-validate", crate::stages::validate::SHACL_SARIF_PATH),
        ("stage-validate", crate::stages::validate::SHACL_HTML_PATH),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_JSON_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_SARIF_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::DIAG_HTML_PATH,
        ),
    ] {
        let bytes = upstream
            .get(stage)
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| stage_err(&format!("missing {path} in {stage} for fanout archive")))?;
        members.insert(path.to_string(), bytes);
    }

    // Byte-decorated RDF artifacts: these are valid RDF, but their committed files
    // intentionally include generated comments / section markers that are not graph
    // data and therefore cannot be recovered from a canonical named-graph fold.
    // Carry their committed byte projections here while the queryable semantics keep
    // riding the first-class statement / reasoning / metadata graph lanes.
    for (stage, path) in [
        ("stage-reason", crate::stages::reason::CLOSURE_PATH),
        ("stage-reason", crate::stages::reason::EXPLANATIONS_PATH),
        ("stage-reason", crate::stages::reason::LEDGER_PATH),
        ("stage-reason", crate::stages::reason::PERF_LEDGER_PATH),
    ] {
        let bytes = upstream
            .get(stage)
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| {
                stage_err(&format!(
                    "missing byte-decorated RDF artifact {path} in {stage}"
                ))
            })?;
        members.insert(path.to_string(), bytes);
    }

    // The statement-layer OWL + RDF-1.2 byte projections are byte-decorated (generated
    // banners), so they cannot reconstruct from a canonical named-graph fold — carry
    // their committed byte projections here. Read off the sink-consumed stage-statements
    // product (the compile ran once, in that stage), never recomputed from disk.
    members.insert(
        crate::stages::statements::OWL_PATH.to_string(),
        producer_artifact(
            "stage-statements",
            crate::stages::statements::OWL_PATH,
            upstream,
        )?,
    );
    members.insert(
        crate::stages::statements::RDF12_PATH.to_string(),
        producer_artifact(
            "stage-statements",
            crate::stages::statements::RDF12_PATH,
            upstream,
        )?,
    );
    // metadata (void.ttl + dcat.ttl) — byte-decorated, carried as byte projections; read
    // off the stage-export-metadata product (rendered once from the same snapshot carrier).
    members.extend(producer_artifacts("stage-export-metadata", upstream)?);
    members.insert(
        crate::stages::yaml_ld::PRESERVATION_PATH.to_string(),
        crate::stages::yaml_ld::preservation_ledger().into_bytes(),
    );

    // loss matrices: deterministic, code-derived (verified by tests, not stage-built),
    // recomputed verbatim — the committed files equal the function output exactly.
    members.insert(
        "generated/rdf-loss-matrix.json".to_string(),
        purrdf::loss_matrix_json().into_bytes(),
    );
    members.insert(
        "generated/transcode-loss-matrix.json".to_string(),
        crate::transcode::transcode_loss_matrix_json().into_bytes(),
    );
    members.insert(
        "generated/transcode-matrix.json".to_string(),
        crate::transcode::transcode_matrix_json().into_bytes(),
    );

    // n3 rides in from the sink-consumed stage-compile-logic product (no recompute).
    let n3 = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::N3_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/n3/gmeow.n3 in stage-compile-logic"))?;
    members.insert(crate::stages::compile_logic::N3_PATH.to_string(), n3);

    // CLIF rides in from the sink-consumed stage-compile-logic product. It is a
    // non-RDF text projection whose committed form carries generated `;;` comments and
    // `;; ===` section markers, so it cannot reconstruct from a canonical named-graph
    // fold; it is carried here as a committed byte projection (byte-identical to the file).
    let clif = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::CLIF_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/cl/gmeow.clif in stage-compile-logic"))?;
    members.insert(crate::stages::compile_logic::CLIF_PATH.to_string(), clif);

    // CGIF (the conceptual-graph dialect) rides in from the same sink-consumed
    // stage-compile-logic product. It is a non-RDF text projection whose committed form carries
    // generated `/* … */` comments and section markers, so it cannot reconstruct from a
    // canonical named-graph fold; it is carried here as a committed byte projection.
    let cgif = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::CGIF_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/cl/gmeow.cgif in stage-compile-logic"))?;
    members.insert(crate::stages::compile_logic::CGIF_PATH.to_string(), cgif);

    // XCL (the XML dialect) rides in from the same sink-consumed stage-compile-logic
    // product. It is a non-RDF text projection whose committed form carries an XML
    // declaration and generated `<!-- … -->` comments, so it cannot reconstruct from a
    // canonical named-graph fold; it is carried here as a committed byte projection.
    let xcl = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::XCL_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| stage_err("missing generated/cl/gmeow.xcl in stage-compile-logic"))?;
    members.insert(crate::stages::compile_logic::XCL_PATH.to_string(), xcl);

    // The SHACL-AF rule (computation) surface rides in from the same sink-consumed
    // compile-logic product (byte-decorated Turtle with a GENERATED banner — not a plain
    // canonical fold), as a committed byte projection.
    let shacl_af = upstream
        .get("stage-compile-logic")
        .and_then(|p| p.artifact(crate::stages::compile_logic::SHACL_AF_PATH))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            stage_err("missing generated/shacl-af/gmeow.shacl-af.ttl in stage-compile-logic")
        })?;
    members.insert(
        crate::stages::compile_logic::SHACL_AF_PATH.to_string(),
        shacl_af,
    );

    // The validation-shape surfaces (SHACL Core + ShEx) — the OPT/ADL constraint axis lifted
    // to logic:ValidationShape and projected — ride in from the same compile-logic product,
    // read through [`validation_shape_surfaces`] so these archive members and the typed
    // Shacl/Shex consumer sidecars ([`build_validation_shape_typed_blobs`]) share ONE source.
    let (validation_shacl, validation_shex) = validation_shape_surfaces(upstream)?;
    members.insert(
        crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH.to_string(),
        validation_shacl,
    );
    members.insert(
        crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH.to_string(),
        validation_shex,
    );

    // context.jsonld + dsl-stats ride in from the sink-consumed stage-mappings product
    // (rendered once by that stage), never recomputed here.
    members.insert(
        crate::stages::mappings::JSONLD_CONTEXT_PATH.to_string(),
        producer_artifact(
            "stage-mappings",
            crate::stages::mappings::JSONLD_CONTEXT_PATH,
            upstream,
        )?,
    );
    members.insert(
        crate::stages::mappings::DSL_STATS_PATH.to_string(),
        producer_artifact(
            "stage-mappings",
            crate::stages::mappings::DSL_STATS_PATH,
            upstream,
        )?,
    );

    // The EmotionML XML projection of the affect vocabulary rides in from the sink-consumed
    // stage-mappings product (rendered once by that stage). It is a non-RDF (XML) projection,
    // so it cannot reconstruct from a canonical named-graph fold; carried here as a committed
    // byte projection so a single regenerate folds the fresh document into gmeow.gts (never a
    // stale disk read — the committed file is not flushed until phase 1 returns).
    members.insert(
        crate::stages::mappings::EMOTIONML_PATH.to_string(),
        producer_artifact(
            "stage-mappings",
            crate::stages::mappings::EMOTIONML_PATH,
            upstream,
        )?,
    );

    // The `lang:` projection deliverables under `generated/projections/lang/` are STANDALONE
    // external-format files a consumer reads — the EBNF/ABNF grammar files, the TEI XML, the
    // Web-Annotation JSON-LD, AND the RDF side formats (a NIF `.nt` stand-off annotation, the
    // `bcp47-tags.ttl` tag set). None of them reconstructs from a canonical named-graph fold:
    // the RDF ones are lowerings a consumer reads as files, not reasoned graphs (their
    // `lang:ProjectionEmission` semantics ride the `graph/lang-projection-corpus` named graph
    // independently). So every lang-projection artifact — RDF-extension or not — is carried as
    // a committed byte projection, read off the sink-consumed stage-mappings product (rendered
    // once by that stage) via a keyed fold over the in-memory product, never a disk walk.
    for (path, bytes) in producer_artifacts("stage-mappings", upstream)? {
        if path.starts_with(&format!(
            "{}/",
            crate::stages::lang_projection::LANG_PROJECTION_DIR
        )) {
            members.insert(path, bytes);
        }
    }

    let mut members: Vec<(String, Vec<u8>)> = members.into_iter().collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    archive_blob(REP_GENERATED, &members)
}

/// Fold the native reasoner's explanation + DL/EL cross-check ledger REPORTS into a
/// deterministic [`REP_REASONING`] archive blob. Sourced from
/// `stage-reason`'s in-memory product (a `stage-snapshot` consumes-edge), so the fold
/// is ONE-PASS: no disk read, no dependency on the post-snapshot `stage-export-logic`
/// leaf, no convergence lag. The reasoned closure is NOT re-bundled here — it already
/// rides the bundle graph via `gts-compose`. Each artifact MUST exist (no-optionality,
/// fail-closed): a partial archive would silently strip the reasoning reports.
fn build_reasoning_blob(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let get = |path: &str| -> Result<Vec<u8>, gmeow_errors::Diag> {
        upstream
            .get("stage-reason")
            .and_then(|p| p.artifact(path))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| stage_err(&format!("missing stage-reason artifact {path}")))
    };
    // Bundle-relative keys under `reason/` — deliberately NOT `generated/logic/…`, so
    // a consumer never mistakes these bundle-consistent reports (over the early-composed
    // closure that rides the graph) for the full-fold committed `generated/logic/` files
    // owned by `stage-export-logic`.
    let members = vec![
        (
            "reason/reasoning-explanations.rdf12.ttl".to_string(),
            get(crate::stages::reason::EXPLANATIONS_PATH)?,
        ),
        (
            "reason/dl-el-crosscheck-report.ttl".to_string(),
            get(crate::stages::reason::LEDGER_PATH)?,
        ),
        (
            "reason/perf-ledger.ttl".to_string(),
            get(crate::stages::reason::PERF_LEDGER_PATH)?,
        ),
    ];
    archive_blob(REP_REASONING, &members)
}

/// Pack the JSON-LD-star + YAML-LD-star serializations into a deterministic tar
/// archive blob. Member names mirror the `dist/` logical paths.
#[cfg(test)]
#[allow(dead_code)]
fn build_yaml_ld_blob(jsonld: &[u8], yamlld: &[u8]) -> Result<BlobRow, gmeow_errors::Diag> {
    let members = vec![
        ("gmeow.jsonld".to_string(), jsonld.to_vec()),
        ("gmeow.yamlld".to_string(), yamlld.to_vec()),
    ];
    archive_blob(REP_YAMLLD, &members)
}

/// Build the YAML-LD archive by serializing the carrier dataset in-memory — the
/// SAME native carrier every fold-reading export leaf consumes (no gts round-trip).
#[cfg(test)]
#[allow(dead_code)]
fn build_yaml_ld_blob_from_dataset(
    carrier: &purrdf::RdfDataset,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let jsonld = crate::stages::yaml_ld::serialize_graph(carrier)?;
    let yamlld = crate::stages::yaml_ld::serialize_graph_yaml(carrier, None)?;
    build_yaml_ld_blob(jsonld.as_bytes(), yamlld.as_bytes())
}

/// Build the OKF archive from the carrier dataset — the SAME native carrier the
/// fold-reading export leaves consume, avoiding a `stage-snapshot` ↔
/// `stage-export-okf` DAG cycle (no gts round-trip).
///
/// The public reader (`gmeow_tools.bundle.bundled_okf`) expects members relative to
/// the bundle root (`gmeow-okf/classes/Foo.md`), while the export leaf product is a
/// disk artifact under `dist/`. Strip only that leading `dist/` boundary and hard-fail
/// if a renderer path escapes it.
#[cfg(test)]
fn build_okf_blob_from_dataset(
    carrier: &purrdf::RdfDataset,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let (title, version, terms) = crate::stages::export::collect_term_surface(carrier)?;
    let artifacts = crate::stages::okf::render_okf(&title, &version, &terms)?;
    let mut members: Vec<(String, Vec<u8>)> = Vec::with_capacity(artifacts.len());
    for (path, bytes) in artifacts {
        let member = path
            .strip_prefix("dist/")
            .ok_or_else(|| stage_err(&format!("OKF export path is not under dist/: {path}")))?;
        members.push((member.to_string(), bytes));
    }
    archive_blob(REP_OKF, &members)
}

/// Render the full ontology-docs static site and pack it into the single
/// `ontology-docs` archive blob — the producer half of repo-free
/// `gmeow export-docs`.
///
/// The rust doc generator (`gmeow_docs::render_site_lang`) emits a complete site
/// (`index.md`/`index.html` per page, `assets/gmeow.css`, SVG diagrams,
/// `search-index.json`, `llms.txt`/`llms-full.txt`, alias redirects) as a deterministic
/// `BTreeMap<path, bytes>`. We render it once per available language and prefix
/// every member with that language's INTERNAL tag (`x-gmeow-english`,
/// `x-gmeow-<lang>`, …) — the exact `{tag}/` prefix `_unpack_doc_archive` filters
/// on (`resolve_doc_language` returns these internal tags). The prefix comes from
/// `Translations::internal_tag`, never the carrier key or a hardcoded string, so a
/// new `.po` catalog is picked up with the correct tag automatically.
// The docs model (with the reasoning verdict already attached by the caller) + the
// executable-docs data are passed in — both are shared with `build_executable_docs_data`
// so the model is discovered and the reasoner run once per snapshot.
#[cfg(test)]
fn build_docs_archive(
    root: &Path,
    model: &gmeow_docs::model::DocsModel,
    exec: &gmeow_docs::ExecutableDocsData,
    slice_quality_html: &[u8],
) -> Result<BlobRow, gmeow_errors::Diag> {
    let catalog = purrdf::slice::SliceCatalog::discover(
        &root.join("slices"),
        crate::gmeow_ns::gmeow_slice_vocab(),
    )
    .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);

    // Render each language's full site in parallel: the per-language renders are
    // independent pure functions of the shared read-only model + executable data, and
    // this is the dominant cost of the snapshot stage (which sits on the build DAG's
    // serial critical path). Results are collected then sorted by member path, so the
    // archive is byte-identical regardless of completion order.
    let langs = gmeow_docs::available_languages(&translations);
    // The purrdf graph diagrams (thousands of per-term / per-slice SVGs) are
    // language-invariant and dominate the render cost — render them ONCE and share the
    // identical bytes across every language tree, rather than re-rendering per locale.
    let diagrams = gmeow_docs::render_purrdf_diagrams(model);
    let mut members: Vec<(String, Vec<u8>)> = langs
        .par_iter()
        .flat_map_iter(|lang| {
            let site =
                gmeow_docs::render_site_lang_exec_with_diagrams(model, lang, exec, &diagrams);
            let prefix = translations.internal_tag(lang);
            site.files
                .into_iter()
                .map(move |(path, bytes)| (format!("{prefix}/{path}"), bytes))
        })
        .collect();
    for lang in &langs {
        let prefix = translations.internal_tag(lang);
        let member = format!("{prefix}/{SLICE_QUALITY_DOC_PATH}");
        if members.iter().any(|(path, _)| path == &member) {
            return Err(stage_err(&format!(
                "ontology-docs renderer already emitted reserved slice-quality report path {member}"
            )));
        }
        members.push((member, slice_quality_html.to_vec()));
    }
    members.sort_by(|a, b| a.0.cmp(&b.0));
    archive_blob(REP_ONTOLOGY_DOCS, &members)
}

/// Render the mdbook `src/` source tree and pack it into the single `docs-book` archive blob
/// — the producer half of the mdbook documentation projection.
///
/// [`gmeow_docs::mdbook::render_book`] emits a flat, un-prefixed [`gmeow_docs::render::Site`]
/// (`book.toml`, `SUMMARY.md`, `src/<page>/index.md`). We render ONLY the English carrier and
/// prefix every member with English's INTERNAL tag (`x-gmeow-english/…`), taken from
/// `Translations::internal_tag` exactly as [`build_docs_archive`] does, so the archive member
/// scheme matches the ontology-docs archive and a docs consumer selects the same way.
#[cfg(test)]
fn build_docs_book_archive(
    root: &Path,
    model: &gmeow_docs::model::DocsModel,
    exec: &gmeow_docs::ExecutableDocsData,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let catalog = purrdf::slice::SliceCatalog::discover(
        &root.join("slices"),
        crate::gmeow_ns::gmeow_slice_vocab(),
    )
    .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);
    let prefix = translations.internal_tag(gmeow_docs::i18n::ENGLISH);

    let site = gmeow_docs::mdbook::render_book(model, exec);
    let mut members: Vec<(String, Vec<u8>)> = site
        .files
        .into_iter()
        .map(|(path, bytes)| (format!("{prefix}/{path}"), bytes))
        .collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    archive_blob(REP_DOCS_BOOK, &members)
}

/// Render the deterministic Typst source, compile the byte-reproducible print PDF, and pack
/// both into the single `docs-print` archive blob — the producer half of the print
/// documentation projection.
///
/// The renderer reads THIS run's compiled logic/DL axiom surface ([`AXIOM_FILES`], sourced
/// from the `stage-compile-logic` product exactly as [`build_archive_blobs`]'s REP_AXIOMS
/// fold does — never a stale disk read) as its axiom-listing input, and the bibliography
/// database from the `stage-export-references` product. The loss appendix reads the shared
/// per-format capability table. Both `gmeow.pdf` and `gmeow.typ` ride under English's internal
/// tag (`x-gmeow-english/…`) so the member scheme matches the sibling docs archives.
#[cfg(test)]
fn build_docs_print_blob(
    model: &gmeow_docs::model::DocsModel,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<(BlobRow, String), gmeow_errors::Diag> {
    let bib = producer_artifact(
        "stage-export-references",
        crate::stages::references::BIB_PATH,
        upstream,
    )?;
    // The axiom-listing input: THIS run's compiled logic/DL projections, keyed by their
    // repo-relative path, pulled from the stage-compile-logic product (fail-closed on absence,
    // mirroring `build_archive_blobs`' REP_AXIOMS guard — a partial listing would silently ship
    // an incomplete PDF).
    let axiom_artifacts = producer_artifacts("stage-compile-logic", upstream)?;
    let mut axioms: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for rel in AXIOM_FILES {
        let bytes = axiom_artifacts.get(rel).ok_or_else(|| {
            stage_err(&format!(
                "missing axiom artifact {rel} in the stage-compile-logic product for the print PDF (fail-closed)"
            ))
        })?;
        axioms.insert(rel.to_string(), bytes.clone());
    }
    let losses: Vec<gmeow_docs::formats::FormatCapabilities> = [
        gmeow_docs::formats::DocFormat::Site,
        gmeow_docs::formats::DocFormat::Mdbook,
        gmeow_docs::formats::DocFormat::Pdf,
        gmeow_docs::formats::DocFormat::Snippets,
    ]
    .into_iter()
    .map(gmeow_docs::formats::format_capabilities)
    .collect();

    let typ = docs_print::render_typ(model, &axioms, &bib, &losses);
    let pdf = docs_print::compile_pdf(&typ, &bib)?;
    // The raw `gmeow.pdf` byte digest — BEFORE it is packed into the docs-print tar —
    // so the docs-format grounding graph (F4) can attest the PDF itself, not just the
    // archive that carries it. Computed here, the ONLY point the un-tarred bytes exist.
    let pdf_digest = purrdf::gts::writer::digest_string(&pdf);

    let prefix = model.translations.internal_tag(gmeow_docs::i18n::ENGLISH);
    let members = vec![
        (format!("{prefix}/gmeow.pdf"), pdf),
        (format!("{prefix}/gmeow.typ"), typ.into_bytes()),
    ];
    Ok((archive_blob(REP_DOCS_PRINT, &members)?, pdf_digest))
}

/// Compute the build-time [`gmeow_docs::ExecutableDocsData`] the "live" docs surfaces
/// need — from the carrier, the authored ontology, and the on-disk worked examples.
///
/// - **Try it:** reason over `(authored default-world ontology ∪ all example ABoxes)` and
///   slice the resulting closure by each example's own subjects, diffed (witness-
///   insensitively) against `stage-reason`'s committed base closure and the example's
///   asserted triples. Inferences not attributable to a single example go to a
///   `cross_example` bucket — never silently dropped. This does NOT re-derive the full
///   ontology closure: that runs ONCE, in stage-reason (reason-once, project-many); the
///   examples can only propagate through the authored default-world axioms (the calculus
///   is same-world and imports ride named worlds), so the small seed is sufficient.
/// - **Playground asset:** `documentation graph ∪ reasoned ontology closure`, TriG.
#[cfg(test)]
fn build_executable_docs_data(
    upstream: &BTreeMap<String, StageProduct>,
    carrier: &purrdf::RdfDataset,
    model: &gmeow_docs::model::DocsModel,
) -> Result<gmeow_docs::ExecutableDocsData, gmeow_errors::Diag> {
    // The reasoning seed for the "try it" hypothetical is the AUTHORED default-world
    // ontology ALONE — NOT the full object-level EDB. Reason-once, project-many
    // (PIPELINE_SPINE §3.2/§8): the expensive full-EDB closure is computed exactly ONCE, in
    // stage-reason. The try-it must not re-derive it. It can't: worked examples parse into
    // the DEFAULT world and the EL/RL calculus is same-world (`?w`), while imports /
    // statements / alignments / logic ride NAMED worlds — so an example can ONLY propagate
    // through the authored default-world axioms. Reasoning that small seed (instead of the
    // full EDB with its import bulk) yields byte-identical attributed inferences at a
    // fraction of the cost (verified against the full EDB over the real ontology).
    let base_seed = std::sync::Arc::new(
        source_load_dataset(upstream)?.project_named_graph(GRAPH_AUTHORED_DEFAULT),
    );
    // The base ontology closure stage-reason already committed: the core subtracts it
    // (witness-insensitively) so only EXAMPLE-INDUCED inferences remain — reuse, not a
    // second authority.
    let base_bytes = upstream
        .get("stage-reason")
        .and_then(|p| p.artifact(crate::stages::reason::CLOSURE_PATH))
        .ok_or_else(|| stage_err("missing stage-reason inferred-closure artifact"))?;
    // Lower the discovered docs model to the reason-and-attribute core's plain inputs, so
    // the core is exercisable over a fixed fixture without a full pipeline product map.
    let sources: Vec<ExampleSource> = model
        .examples
        .iter()
        .map(|ex| ExampleSource {
            slice: ex.slice.clone(),
            logical_path: ex.logical_path.clone(),
            text: ex.text.clone(),
        })
        .collect();
    let mut data = executable_docs_from_sources(base_seed.as_ref(), base_bytes, &sources, carrier)?;
    // B3: the per-term entailment "why" panels, parsed from `stage-reason`'s materialized
    // `reasoning-explanations` proof skeletons (reason-once — this reads the SAME product
    // the CLOSURE_PATH fetch above already reads, a different artifact key on the identical
    // upstream product, never a second reasoning pass). Joined against every documented
    // term's IRI, so a term with no matching derivation is honestly absent from the map.
    let term_iris: std::collections::BTreeSet<String> =
        model.terms.iter().map(|t| t.iri.clone()).collect();
    data.term_entailments = term_entailments_from_upstream(upstream, &term_iris)?;
    Ok(data)
}

/// One reasoner-derivation's raw shape, accumulated per blank-node subject while
/// walking the explanations Turtle (see [`term_entailments_from_explanations`]).
#[derive(Default)]
struct RawDerivation {
    /// Whether an `rdf:type gmeow:Derivation` triple was seen for this subject — a
    /// defensive check so a stray blank node in the explanations graph (there should
    /// be none) can never masquerade as a derivation.
    is_derivation: bool,
    /// The `gmeow:concludes` quoted-triple object, if seen.
    concludes: Option<RdfTriple>,
    /// Every `gmeow:hasPremise` quoted-triple object seen (zero or more).
    premises: Vec<RdfTriple>,
    /// The `gmeow:viaRule` object IRI, if seen.
    via_rule: Option<String>,
}

/// Whether any documented term IRI appears in `triple`'s subject, predicate, or
/// object position, added to `out` (a term appearing twice in one triple is recorded
/// once — `out` is a set).
fn collect_term_matches(
    triple: &RdfTriple,
    term_iris: &std::collections::BTreeSet<String>,
    out: &mut std::collections::BTreeSet<String>,
) {
    if let RdfTerm::Iri(iri) = &triple.subject
        && term_iris.contains(iri)
    {
        out.insert(iri.clone());
    }
    if term_iris.contains(&triple.predicate) {
        out.insert(triple.predicate.clone());
    }
    if let RdfTerm::Iri(iri) = &triple.object
        && term_iris.contains(iri)
    {
        out.insert(iri.clone());
    }
}

/// Fetch `stage-reason`'s materialized `reasoning-explanations` artifact off the
/// upstream product and fold it into the B3 per-term entailment digest. HARD-fails if
/// `stage-reason` (or its explanations artifact) is absent — the pipeline path never
/// falls back to an empty digest silently; only the model-only
/// `ExecutableDocsData::default()` seam is allowed to be empty (F-2).
pub(crate) fn term_entailments_from_upstream(
    upstream: &BTreeMap<String, StageProduct>,
    term_iris: &std::collections::BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<gmeow_docs::Entailment>>, gmeow_errors::Diag> {
    let bytes = upstream
        .get("stage-reason")
        .and_then(|p| p.artifact(crate::stages::reason::EXPLANATIONS_PATH))
        .ok_or_else(|| {
            stage_err(&format!(
                "missing stage-reason artifact {} for term entailments",
                crate::stages::reason::EXPLANATIONS_PATH
            ))
        })?;
    term_entailments_from_explanations(bytes, term_iris)
}

/// Parse `stage-reason`'s materialized `reasoning-explanations` proof skeletons
/// (RDF 1.2 Turtle; see `crate::stages::reason::EXPLANATIONS_PATH` and
/// `gmeow_logic::reason::artifacts::build_explanations_ttl`) into the B3 per-term
/// entailment digest.
///
/// For every `gmeow:Derivation`, any IRI in `term_iris` that appears in the subject,
/// predicate, or object position of EITHER the `gmeow:concludes` conclusion OR any
/// `gmeow:hasPremise` premise gets that derivation's rule + a display of its
/// conclusion + displays of its premises appended to its panel. Pure function of the
/// bytes + the term-IRI set — independently testable without a pipeline product map
/// (mirrors [`executable_docs_from_sources`]'s fixture-only core).
fn term_entailments_from_explanations(
    explanations_bytes: &[u8],
    term_iris: &std::collections::BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<gmeow_docs::Entailment>>, gmeow_errors::Diag> {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let dataset = parse_dataset(explanations_bytes, "text/turtle", None).map_err(|e| {
        stage_err(&format!(
            "parse reasoning-explanations for term entailments: {e}"
        ))
    })?;

    // `gmeow_logic::reason::artifacts::build_explanations_ttl` writes each
    // `gmeow:concludes` / `gmeow:hasPremise` object via `emit_term(&RdfTerm::triple(..))`,
    // which serializes the RDF 1.2 triple-term shorthand `<< <s> <p> <o> >>` — the
    // BARE (non-parenthesized) form. Per the RDF 1.2 Turtle grammar, a bare `<<...>>`
    // used as the object of any predicate OTHER than `rdf:reifies` is the REIFYING-
    // TRIPLE production, not a triple-term value: the parser mints a fresh reifier
    // (here, a blank node) IN THAT POSITION and records the actual triple as a
    // reifier binding — `dataset.owned_reifiers()` — rather than inline as
    // `RdfTerm::Triple` on the base quad. So `q.object` here is the reifier (a
    // blank node), and the real conclusion/premise triple is looked up from this
    // map. (The canonical parenthesized form `<<( s p o )>>` — used by hand-built
    // fixtures/tests — parses directly to `RdfTerm::Triple` and is honored as a
    // fallback below, so both forms resolve identically.)
    let reifier_triples: std::collections::HashMap<RdfTerm, RdfTriple> = dataset
        .owned_reifiers()
        .map(|r| (r.reifier, r.statement))
        .collect();
    let resolve_triple = |term: &RdfTerm| -> Option<RdfTriple> {
        match term {
            RdfTerm::Triple(t) => Some((**t).clone()),
            other => reifier_triples.get(other).cloned(),
        }
    };

    let derivation_ty = format!("{GMEOW_NS}Derivation");
    let concludes_p = format!("{GMEOW_NS}concludes");
    let has_premise_p = format!("{GMEOW_NS}hasPremise");
    let via_rule_p = format!("{GMEOW_NS}viaRule");

    let mut raw: BTreeMap<String, RawDerivation> = BTreeMap::new();
    for q in dataset.owned_quads() {
        let RdfTerm::BlankNode(label) = &q.subject else {
            continue;
        };
        let entry = raw.entry(label.clone()).or_default();
        if q.predicate == RDF_TYPE {
            if let RdfTerm::Iri(iri) = &q.object
                && *iri == derivation_ty
            {
                entry.is_derivation = true;
            }
        } else if q.predicate == concludes_p {
            if let Some(triple) = resolve_triple(&q.object) {
                entry.concludes = Some(triple);
            }
        } else if q.predicate == has_premise_p {
            if let Some(triple) = resolve_triple(&q.object) {
                entry.premises.push(triple);
            }
        } else if q.predicate == via_rule_p
            && let RdfTerm::Iri(iri) = &q.object
        {
            entry.via_rule = Some(iri.clone());
        }
    }

    let mut term_entailments: BTreeMap<String, Vec<gmeow_docs::Entailment>> = BTreeMap::new();
    for derivation in raw.into_values() {
        if !derivation.is_derivation {
            continue;
        }
        let Some(concludes) = &derivation.concludes else {
            continue;
        };
        let mut matched: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        collect_term_matches(concludes, term_iris, &mut matched);
        for premise in &derivation.premises {
            collect_term_matches(premise, term_iris, &mut matched);
        }
        if matched.is_empty() {
            continue;
        }
        let rule = derivation
            .via_rule
            .as_deref()
            .map(compact_iri)
            .unwrap_or_default();
        let conclusion =
            triple_display(&concludes.subject, &concludes.predicate, &concludes.object);
        let mut premises: Vec<String> = derivation
            .premises
            .iter()
            .map(|p| triple_display(&p.subject, &p.predicate, &p.object))
            .collect();
        premises.sort();
        premises.dedup();
        let entailment = gmeow_docs::Entailment {
            rule,
            conclusion,
            premises,
        };
        for term_iri in matched {
            term_entailments
                .entry(term_iri)
                .or_default()
                .push(entailment.clone());
        }
    }
    for entries in term_entailments.values_mut() {
        entries.sort_by(|a, b| {
            a.conclusion
                .cmp(&b.conclusion)
                .then_with(|| a.rule.cmp(&b.rule))
                .then_with(|| a.premises.cmp(&b.premises))
        });
        entries.dedup();
    }
    Ok(term_entailments)
}

/// One worked example's authored source — its slice IRI, logical path (extension drives
/// the parse dispatch), and raw text. The reason-and-attribute core takes these instead of
/// the whole discovered [`gmeow_docs::model::DocsModel`] so it is exercisable over a fixed
/// fixture without a full pipeline product map.
#[cfg(test)]
pub(crate) struct ExampleSource {
    pub slice: String,
    pub logical_path: String,
    pub text: String,
}

/// The reason-and-attribute core of the executable "try it" docs (see
/// [`build_executable_docs_data`] for how the pipeline gathers the inputs).
///
/// Reason over `(reason_seed ∪ every example ABox)`, subtract the committed `base_closure`
/// (witness-insensitively) and each example's own assertions, and attribute every
/// remaining (example-induced) inference to the example that owns its subject. Inferences
/// with no owning example subject (shared / Skolem witnesses) go to the `cross_example`
/// bucket — never silently dropped. The playground asset is
/// `documentation graph ∪ base_closure`, TriG.
///
/// `reason_seed` is the authored default-world ontology (not the full object-level EDB):
/// the examples can only propagate through the same-world authored axioms, so this small
/// seed reproduces the full-EDB attribution exactly without re-deriving the base closure.
#[cfg(test)]
pub(crate) fn executable_docs_from_sources(
    reason_seed: &purrdf::RdfDataset,
    base_closure_bytes: &[u8],
    examples: &[ExampleSource],
    carrier: &purrdf::RdfDataset,
) -> Result<gmeow_docs::ExecutableDocsData, gmeow_errors::Diag> {
    use std::collections::{BTreeMap as StdBTreeMap, BTreeSet, HashSet};

    // Parse every worked example's ABox; remember its subjects + asserted display lines.
    struct ExampleAbox {
        key: String,
        subjects: BTreeSet<String>,
        asserted: Vec<String>,
        dataset: std::sync::Arc<purrdf::RdfDataset>,
    }
    let mut parsed: Vec<ExampleAbox> = Vec::new();
    for ex in examples {
        let ds = parse_example(&ex.logical_path, &ex.text)?;
        let mut subjects = BTreeSet::new();
        let mut asserted = Vec::new();
        for q in ds.owned_quads() {
            if let RdfTerm::Iri(iri) = &q.subject {
                subjects.insert(iri.clone());
            }
            asserted.push(format_triple(&q));
        }
        asserted.sort();
        asserted.dedup();
        parsed.push(ExampleAbox {
            key: gmeow_docs::example_key(&ex.slice, &ex.logical_path),
            subjects,
            asserted,
            dataset: ds,
        });
    }

    // Reason over (reason_seed ∪ every example ABox). push_dataset standardizes blanks
    // apart per merged dataset, so example blanks never collide.
    let mut union = RdfDatasetBuilder::new();
    union.push_dataset(reason_seed);
    for ex in &parsed {
        union.push_dataset(ex.dataset.as_ref());
    }
    let union_ds = union
        .freeze()
        .map_err(|e| stage_err(&format!("freeze try-it union EDB: {e}")))?;
    let reasoned = crate::stages::reason::reason_over_dataset(union_ds.as_ref())?;
    let union_closure = parse_dataset(reasoned.closure.as_bytes(), "text/turtle", None)
        .map_err(|e| stage_err(&format!("try-it union closure parse: {e}")))?;

    // The base ontology-only closure (already committed by the reason stage): subtract
    // it so only EXAMPLE-INDUCED inferences remain (reuse, not a second authority).
    //
    // Witness-insensitive: a Skolem witness edge (an `X ⊑ ∃r.C` restriction materialized
    // as `X ⊑ <skolem>`) carries a content-addressed IRI that depends on the reasoning
    // context, so raw-IRI matching would leak the ontology-level edge into `cross_example`.
    // Normalizing the witness object lets an ontology edge cancel against the base
    // regardless of context, while example-SUBJECT facts (absent from the base) are kept.
    let witness_norm = |line: &str| -> String {
        line.split(' ')
            .map(|t| {
                if t.starts_with("_:") || t.contains("blackcatinformatics.ca/gmeow/skolem/") {
                    "<skolem>".to_string()
                } else {
                    t.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let base_closure = parse_dataset(base_closure_bytes, "text/turtle", None)
        .map_err(|e| stage_err(&format!("base closure parse: {e}")))?;
    let base_set: HashSet<String> = base_closure
        .owned_quads()
        .map(|q| witness_norm(&format_triple(&q)))
        .collect();
    let asserted_set: HashSet<String> = parsed
        .iter()
        .flat_map(|e| e.asserted.iter().cloned())
        .collect();

    // Map each example subject to its owning example key. A subject named by exactly one
    // example maps to `Some(key)`; a subject shared by 2+ examples is ambiguous (we cannot
    // tell which example induced a given inference on it) and is recorded as `None` so it
    // routes to `cross_example` rather than being silently misattributed to whichever
    // example happened to insert last.
    let mut subject_to_example: StdBTreeMap<String, Option<String>> = StdBTreeMap::new();
    for ex in &parsed {
        for s in &ex.subjects {
            subject_to_example
                .entry(s.clone())
                .and_modify(|owner| *owner = None)
                .or_insert_with(|| Some(ex.key.clone()));
        }
    }

    // Attribute each example-induced inference to its example, else the cross bucket.
    let mut per_example: StdBTreeMap<String, Vec<String>> = StdBTreeMap::new();
    let mut cross_example: Vec<String> = Vec::new();
    for q in union_closure.owned_quads() {
        let line = format_triple(&q);
        if base_set.contains(&witness_norm(&line)) || asserted_set.contains(&line) {
            continue; // ontology-only inference or the example's own assertion.
        }
        let subject_iri = match &q.subject {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        };
        // Unknown subject or an ambiguous (multi-example) subject both fall through to
        // `cross_example`; only an unambiguous single-owner subject attributes directly.
        match subject_iri
            .and_then(|s| subject_to_example.get(&s).cloned())
            .flatten()
        {
            Some(key) => per_example.entry(key).or_default().push(line),
            None => cross_example.push(line),
        }
    }
    cross_example.sort();
    cross_example.dedup();

    // Assemble the per-example asserted-vs-inferred diffs.
    let mut example_inferences: StdBTreeMap<String, gmeow_docs::InferenceDiff> = StdBTreeMap::new();
    for ex in &parsed {
        let mut inferred = per_example.remove(&ex.key).unwrap_or_default();
        inferred.sort();
        inferred.dedup();
        let diff = gmeow_docs::InferenceDiff {
            asserted: ex.asserted.clone(),
            inferred,
        };
        if !diff.is_empty() {
            example_inferences.insert(ex.key.clone(), diff);
        }
    }

    // The playground asset: documentation graph ∪ the reasoned ontology closure,
    // serialized to TriG. This is the "documentation + reasoned ontology" surface the
    // playground queries AND the substrate term/slice `DESCRIBE` export reads — the
    // closure carries the told-and-inferred ontology. The raw multi-graph EDB (with
    // external imports and alignments) is deliberately EXCLUDED to keep the bundled
    // asset bounded.
    let playground_trig = build_playground_trig(carrier, &base_closure)?;

    Ok(gmeow_docs::ExecutableDocsData {
        example_inferences,
        cross_example,
        playground_trig,
        // `term_entailments` is NOT this core's concern (it needs the discovered
        // term-IRI set, not just the reduced reasoning seed): `build_executable_docs_data`
        // fills it in afterward via `term_entailments_from_upstream`, so this fixture-only
        // core stays exercisable without a full docs model.
        ..Default::default()
    })
}

/// Parse one worked example into a dataset, dispatching on its file extension —
/// examples are authored in Turtle, but also JSON-LD-star and YAML-LD-star.
#[cfg(test)]
fn parse_example(
    logical_path: &str,
    text: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let ext = logical_path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let media = match ext.as_str() {
        "ttl" | "turtle" => "text/turtle",
        "nt" | "ntriples" => "application/n-triples",
        "nq" | "nquads" => "application/n-quads",
        "trig" => "application/trig",
        "rdf" | "xml" => "application/rdf+xml",
        "jsonld" => {
            return purrdf::native_codecs::jsonld::parse_jsonld(text.as_bytes())
                .map_err(|e| stage_err(&format!("example jsonld parse {logical_path}: {e}")));
        }
        "yamlld" | "yaml" | "yml" => {
            let json = purrdf::native_codecs::jsonld::yamlld_to_jsonld(text.as_bytes())
                .map_err(|e| stage_err(&format!("example yamlld convert {logical_path}: {e}")))?;
            return purrdf::native_codecs::jsonld::parse_jsonld(json.as_bytes())
                .map_err(|e| stage_err(&format!("example yamlld parse {logical_path}: {e}")));
        }
        other => {
            return Err(stage_err(&format!(
                "example {logical_path}: unsupported format .{other}"
            )));
        }
    };
    parse_dataset(text.as_bytes(), media, None)
        .map_err(|e| stage_err(&format!("example parse {logical_path}: {e}")))
}

/// Lift ONLY the chase-invented witness-derivation subgraph out of a `graph/diagnostics`
/// projection and route each lifted quad into `into` (the reasoning graph). A Skolem null
/// the reasoned closure already carries as an object can then be decomposed into its firing
/// rule + existential ordinal + frontier binding. Content-addressed skolem IRIs are lifted
/// 1:1 (never normalized), so a playground query can pin an exact null. Only the
/// `gmeow:InventedWitness` typings and their minting head-quad reifiers cross over —
/// `graph/diagnostics` findings stay out. `owned_quads()` iterates deterministically, so the
/// lifted set is byte-stable; an empty diagnostics graph / empty witness set adds nothing.
pub(crate) fn lift_witness_subgraph(
    diag: &purrdf::RdfDataset,
    into: &RdfTerm,
    builder: &mut RdfDatasetBuilder,
) {
    use std::collections::BTreeSet;
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
    let invented_witness = format!("{GMEOW_NS}InventedWitness");
    // (1) The witness IRI set W: every subject typed `gmeow:InventedWitness`.
    let mut witnesses: BTreeSet<String> = BTreeSet::new();
    for q in diag.owned_quads() {
        if q.predicate == RDF_TYPE
            && let RdfTerm::Iri(s) = &q.subject
            && let RdfTerm::Iri(o) = &q.object
            && *o == invented_witness
        {
            witnesses.insert(s.clone());
        }
    }
    if witnesses.is_empty() {
        return;
    }
    // (3) The reifier IRI set R: every subject `r` with `(r, rdf:object, w)`, w ∈ W.
    let mut reifiers: BTreeSet<String> = BTreeSet::new();
    for q in diag.owned_quads() {
        if q.predicate == RDF_OBJECT
            && let RdfTerm::Iri(r) = &q.subject
            && let RdfTerm::Iri(o) = &q.object
            && witnesses.contains(o)
        {
            reifiers.insert(r.clone());
        }
    }
    // (2) + (4) Every quad whose subject ∈ W ∪ R, routed into `into`.
    for q in diag.owned_quads() {
        if let RdfTerm::Iri(s) = &q.subject
            && (witnesses.contains(s) || reifiers.contains(s))
        {
            let mut routed = q.clone();
            routed.graph_name = Some(into.clone());
            builder.push_owned_quad(&routed);
        }
    }
}

/// Build the offline SPARQL-playground TriG asset from a committed GTS bundle's OWN named
/// graphs — the production surface `gmeow-dev export-docs` ships. It projects the bundle's
/// `graph/documentation` (routed back into the documentation graph), its reasoned
/// `graph/reasoning` closure (routed into the reasoning graph), and the chase-invented-null
/// witness subgraph lifted out of `graph/diagnostics` into the reasoning graph (findings stay
/// out). Deterministic; an empty diagnostics/witness set adds nothing.
pub fn playground_trig_from_bundle(
    bundle: &purrdf::RdfDataset,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut pg = RdfDatasetBuilder::new();
    // The documentation graph, routed back into its named graph.
    let docs_iri = RdfTerm::Iri(GRAPH_DOCUMENTATION.to_owned());
    for q in bundle
        .project_named_graph(GRAPH_DOCUMENTATION)
        .owned_quads()
    {
        let mut routed = q.clone();
        routed.graph_name = Some(docs_iri.clone());
        pg.push_owned_quad(&routed);
    }
    // The reasoned closure, routed into the reasoning graph.
    let reasoning_iri = RdfTerm::Iri(gmeow_logic::result_rdf::GRAPH_REASONING.to_owned());
    for q in bundle
        .project_named_graph(gmeow_logic::result_rdf::GRAPH_REASONING)
        .owned_quads()
    {
        let mut routed = q.clone();
        routed.graph_name = Some(reasoning_iri.clone());
        pg.push_owned_quad(&routed);
    }
    // The chase-invented witness-derivation subgraph, lifted from `graph/diagnostics` into
    // the reasoning graph so the "explain a witness" affordance can decompose an exact null.
    let diag_graph = bundle.project_named_graph(GRAPH_DIAGNOSTICS);
    lift_witness_subgraph(&diag_graph, &reasoning_iri, &mut pg);

    let pg_ds = pg
        .freeze()
        .map_err(|e| stage_err(&format!("freeze playground dataset: {e}")))?;
    serialize_dataset(&pg_ds, "application/trig", SerializeGraph::Dataset)
        .map_err(|e| stage_err(&format!("serialize playground TriG: {e}")))
}

/// Serialize `documentation graph ∪ reasoned closure` to TriG — the self-contained
/// asset the offline SPARQL playground queries and the export `DESCRIBE` reads.
#[cfg(test)]
fn build_playground_trig(
    carrier: &purrdf::RdfDataset,
    base_closure: &purrdf::RdfDataset,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut pg = RdfDatasetBuilder::new();
    // The documentation graph, routed back into its named graph.
    let docs_graph = carrier.project_named_graph(GRAPH_DOCUMENTATION);
    let docs_iri = RdfTerm::Iri(GRAPH_DOCUMENTATION.to_owned());
    for q in docs_graph.owned_quads() {
        let mut routed = q.clone();
        routed.graph_name = Some(docs_iri.clone());
        pg.push_owned_quad(&routed);
    }
    // The reasoned closure, routed into the reasoning graph.
    let reasoning_iri = RdfTerm::Iri(gmeow_logic::result_rdf::GRAPH_REASONING.to_owned());
    for q in base_closure.owned_quads() {
        let mut routed = q.clone();
        routed.graph_name = Some(reasoning_iri.clone());
        pg.push_owned_quad(&routed);
    }
    // The chase-invented witness-derivation subgraph, lifted out of `graph/diagnostics`
    // and routed into the reasoning graph — so a Skolem null the closure already carries
    // as an object can be decomposed into its firing rule + existential ordinal + frontier
    // binding. Content-addressed skolem IRIs are lifted 1:1 (never normalized), so a
    // playground query can pin an exact null. Only the `gmeow:InventedWitness` typings and
    // their minting head-quad reifiers are lifted; findings stay out of the playground.
    let diag_graph = carrier.project_named_graph(GRAPH_DIAGNOSTICS);
    lift_witness_subgraph(&diag_graph, &reasoning_iri, &mut pg);
    let pg_ds = pg
        .freeze()
        .map_err(|e| stage_err(&format!("freeze playground dataset: {e}")))?;
    serialize_dataset(&pg_ds, "application/trig", SerializeGraph::Dataset)
        .map_err(|e| stage_err(&format!("serialize playground TriG: {e}")))
}

/// Format an owned quad's `(s, p, o)` as a compact, deterministic display line for the
/// "try it" asserted-vs-inferred surfaces. The graph is dropped (these are triples).
#[cfg(test)]
fn format_triple(q: &RdfQuad) -> String {
    triple_display(&q.subject, &q.predicate, &q.object)
}

/// The shared `s p o` compact display form (CURIE-compacted subject/predicate/object)
/// underlying [`format_triple`] and the B3 entailment displays
/// ([`term_entailments_from_explanations`]) — a `gmeow:Derivation`'s `gmeow:concludes`
/// / `gmeow:hasPremise` quoted triple has the identical `(subject, predicate, object)`
/// shape as an owned quad, so both render through this one function.
fn triple_display(subject: &RdfTerm, predicate: &str, object: &RdfTerm) -> String {
    format!(
        "{} {} {}",
        term_display(subject),
        compact_iri(predicate),
        term_display(object)
    )
}

// The canonical prefix registry (generated from the ontology's prefix config,
// longest-namespace-first). Shared verbatim with the LPG/JSON-LD projections rather
// than hand-maintaining a second, divergent table for the try-it display lines.
include!("lpg_prefixes.rs");

/// A compact CURIE for a full IRI, or `<iri>` when no known prefix matches. Drawing
/// from the full canonical registry means every external ontology GMEOW links to
/// compacts on the try-it surface, not just the handful of core namespaces.
fn compact_iri(iri: &str) -> String {
    // `PREFIXES_BY_LEN` is longest-namespace-first, so the first namespace the IRI
    // starts with is the most specific prefix.
    for (prefix, ns) in PREFIXES_BY_LEN {
        if let Some(local) = iri.strip_prefix(ns) {
            // Only compact when the local part is a simple name (no slash), so a
            // nested-path IRI is never mangled into a misleading CURIE.
            if !local.contains('/') {
                return format!("{prefix}:{local}");
            }
        }
    }
    format!("<{iri}>")
}

/// A compact display form for a term (IRI as CURIE, literal with datatype/lang, blank).
fn term_display(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => compact_iri(iri),
        RdfTerm::BlankNode(label) => format!("_:{label}"),
        RdfTerm::Literal(lit) => format_literal(lit),
        RdfTerm::Triple(t) => format!(
            "<< {} {} {} >>",
            term_display(&t.subject),
            compact_iri(&t.predicate),
            term_display(&t.object)
        ),
    }
}

/// A Turtle-ish display form for a literal.
fn format_literal(lit: &RdfLiteral) -> String {
    let lex = lit.lexical_form.replace('"', "\\\"");
    if let Some(lang) = &lit.language {
        return format!("\"{lex}\"@{lang}");
    }
    match &lit.datatype {
        Some(dt) if dt != "http://www.w3.org/2001/XMLSchema#string" => {
            format!("\"{lex}\"^^{}", compact_iri(dt))
        }
        _ => format!("\"{lex}\""),
    }
}

/// Every `*.<ext>` directly under `dir`, sorted by path (empty if the dir is absent).
fn list_files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(stage_err(&format!("read_dir {}: {e}", dir.display()))),
    };
    let dot = format!(".{ext}");
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| stage_err(&format!("read_dir entry under {}: {e}", dir.display())))?
            .path();
        if path.is_file() && path.to_string_lossy().ends_with(&dot) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Every `slices/<group>/<name>/<sub>/*.ttl` (non-recursive past `<sub>/`), sorted.
fn slice_files(root: &Path, sub: &str) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let slices = root.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    let groups = match std::fs::read_dir(&slices) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(stage_err(&format!("read_dir {}: {e}", slices.display()))),
    };
    for group in groups {
        let gpath = group
            .map_err(|e| stage_err(&format!("slices group: {e}")))?
            .path();
        if !gpath.is_dir() {
            continue;
        }
        let names = std::fs::read_dir(&gpath)
            .map_err(|e| stage_err(&format!("read_dir {}: {e}", gpath.display())))?;
        for name in names {
            let npath = name
                .map_err(|e| stage_err(&format!("slices name: {e}")))?
                .path();
            if npath.is_dir() {
                out.extend(list_files(&npath.join(sub), "ttl")?);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Every `slices/<group>/<name>/<file>` (a single well-known FILE directly under
/// each slice dir, e.g. `shapes.ttl`), sorted. Unlike [`slice_files`] — which globs
/// a `<sub>/*.ttl` *directory* — this targets one named file per slice. Mirrors the
/// shacl crate's private `shape_union::slice_shape_files` walk (re-implemented here
/// because it is not `pub`, the same way [`slice_files`] duplicates a walk). A read
/// error HARD-FAILS so a slice subtree is never silently dropped.
fn slice_named_files(root: &Path, file: &str) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let slices = root.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    let groups = match std::fs::read_dir(&slices) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(stage_err(&format!("read_dir {}: {e}", slices.display()))),
    };
    for group in groups {
        let gpath = group
            .map_err(|e| stage_err(&format!("slices group: {e}")))?
            .path();
        if !gpath.is_dir() {
            continue;
        }
        let names = std::fs::read_dir(&gpath)
            .map_err(|e| stage_err(&format!("read_dir {}: {e}", gpath.display())))?;
        for name in names {
            let npath = name
                .map_err(|e| stage_err(&format!("slices name: {e}")))?
                .path();
            if npath.is_dir() {
                let candidate = npath.join(file);
                if candidate.is_file() {
                    out.push(candidate);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `(filename, bytes)` members — the file's bare name (mappings / queries).
///
/// A read error HARD-FAILS rather than silently dropping the file: an incomplete
/// archive would silently break the wheel-mode consumers (no-optionality, the
/// no-silent-caps doctrine — the same as [`members_relpath`]).
/// `(filename, bytes)` archive members sourced from a STAGE PRODUCT's in-memory
/// artifacts (not disk): every artifact whose path is under `dir` and ends with
/// `suffix`, keyed by bare filename, sorted. Used for the mappings archive so the
/// bundle carries THIS run's freshly-compiled SSSOM rather than the stale committed
/// files (which are not flushed to disk until phase 1 returns).
fn members_basename_from_artifacts(
    artifacts: &BTreeMap<String, Vec<u8>>,
    dir: &str,
    suffix: &str,
) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = artifacts
        .iter()
        .filter(|(path, _)| path.starts_with(dir) && path.ends_with(suffix))
        .map(|(path, bytes)| {
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            (name, bytes.clone())
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `(repo-relative-path, bytes)` members — the path under `root` (cells / tests).
fn members_relpath(
    root: &Path,
    files: &[PathBuf],
) -> Result<Vec<(String, Vec<u8>)>, gmeow_errors::Diag> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(files.len());
    for p in files {
        let rel = p
            .strip_prefix(root)
            .map_err(|_| stage_err(&format!("path {} not under root", p.display())))?
            .to_string_lossy()
            .replace('\\', "/");
        let data =
            std::fs::read(p).map_err(|e| stage_err(&format!("read {}: {e}", p.display())))?;
        out.push((rel, data));
    }
    Ok(out)
}

/// One archive blob: a deterministic USTAR tar over `members`, tagged with `rep`.
fn archive_blob(rep: &str, members: &[(String, Vec<u8>)]) -> Result<BlobRow, gmeow_errors::Diag> {
    Ok(BlobRow {
        data: purrdf::ustar::write_archive(members).map_err(|e| stage_err(&e))?,
        media_type: ARCHIVE_MEDIA_TYPE.to_string(),
        rep: rep.to_string(),
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `stage-snapshot` Transform stage: assembles the fully-accumulated
/// multi-named-graph `dist` carrier as an in-memory value and attaches it to its
/// product — it serializes NOTHING (no bytes on the product). The split from the
/// sink lets every fold-reading export leaf consume THIS run's freshly-assembled
/// carrier rather than re-reading the committed file from disk; the sole
/// [`crate::stages::gts_sink::GtsSinkStage`] then presents that carrier as the
/// `generated/dist/gmeow.gts` bytes — the single serialization boundary (the
/// narrow-waist invariant: one Sink, the sole byte writer).
pub struct SnapshotStage {
    consumes: Vec<String>,
}

impl SnapshotStage {
    /// Construct the snapshot stage. It reads the RDF 1.2 statement layer
    /// (`stage-statements`) and the documentation projection (`stage-docs-render`)
    /// products to assemble the structured snapshot, plus `stage-gts-compose` /
    /// `stage-reason` for the composed-fold / reasoned-closure wiring.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                // The logic compiler's in-memory product: the projection-report loss
                // ledger folds into the bundle as its own named graph and the compile
                // findings union into the diagnostics graph (no disk re-read).
                "stage-compile-logic".to_string(),
                // The external-corpus divergence Findings (graph/conformance):
                // the conformance stage grades the committed corpus's native frozen
                // verdict against the published external verdict and emits the
                // divergences as a gmeow:Finding N-Quads product folded here.
                "stage-conformance".to_string(),
                // The generated constraint catalog `.nq`, folded as the
                // graph/fanout/catalog/constraint-catalog.nq named graph.
                "stage-constraint-catalog".to_string(),
                "stage-docs-render".to_string(),
                // The RDF fanout members ride in from their producing export leaves (the
                // render ran once, in the leaf): profiles / evals scores.ttl / research-
                // object graphs. `rdf_fanout_members` reads them off these products.
                "stage-export-evals".to_string(),
                "stage-export-profiles".to_string(),
                "stage-export-research-objects".to_string(),
                // The mappings product carries the FINAL projection-report loss ledger
                // (logic rows ∪ correspondence rows), folded into graph/projection-ledger.
                "stage-mappings".to_string(),
                // The seven math producer graphs (five flagship producers plus the
                // probability-model seam producer and the p-value tri-slice producer), folded
                // into gmeow.gts as their own bundle-internal named graphs (Design A — the
                // producer output ships).
                "stage-math-producers".to_string(),
                // The SHACL→JSON-Schema export leaf: its in-memory product
                // carries THIS run's freshly-emitted gmeow.schema.json / .openapi.json
                // bytes, which `build_archive_blobs` folds into the `schemas-archive`
                // blob. Without this edge the snapshot would re-read the (previous-run)
                // committed schema from disk and lag a regenerate behind (the bytes
                // are only flushed to disk AFTER phase 1 returns — run.rs:242-254).
                "stage-export-json-schema".to_string(),
                "stage-gts-compose".to_string(),
                "stage-reason".to_string(),
                // The self-description named graphs (authored default / imports / metadata
                // / alignments / slice-analysis / verify / provenance) are attached by
                // stage-source-load; the presenter reads them off this product instead of
                // re-loading + re-canonicalizing the sources (PIPELINE_SPINE §3.2/§4).
                "stage-source-load".to_string(),
                "stage-statements".to_string(),
                // The generated term content manifest `.nq`, folded as the
                // graph/fanout/catalog/term-content-manifest.nq named graph.
                "stage-term-manifest".to_string(),
                "stage-validate".to_string(),
            ],
        }
    }
}

impl Default for SnapshotStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for SnapshotStage {
    fn id(&self) -> &str {
        "stage-snapshot"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn cache_policy(&self) -> CachePolicy {
        // Snapshot is a cumulative carrier boundary. Restoring its packed cache
        // requires hydrating the entire aggregate dataset and all typed handles,
        // which measured slower than assembling it from live upstream products.
        // The sync-level manifest owns the zero-work warm path instead.
        CachePolicy::Recompute
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
        // v5: fold the `schemas-archive` from the in-memory
        // `stage-export-json-schema` product (THIS run's fresh bytes) instead of
        // re-reading the committed `generated/schemas/*.json` from disk —
        // a single regenerate now folds the fresh schema. v4: render+tar+embed the
        // full ontology-docs site as the `ontology-docs` blob. v3 added the
        // mappings/cells/queries/tests archive blobs + per-slice docs guide blobs.
        // v7 folds both the JSON-LD-star/YAML-LD-star archive and the
        // DAG-native SHACL diagnostics graph/report blobs. v8 folds
        // the Rust-rendered OKF archive into gmeow.gts. v9 folds the full
        // SHACL shape surface (REP_SHAPES) and the compiled logic/DL axiom surface
        // (REP_AXIOMS) so a repo-free `gmeow validate` is self-sufficient.
        // v10 wires the orphaned REP_REASONING reader to a writer: folds the native
        // reasoner's explanation + DL/EL cross-check ledger reports from stage-reason's
        // in-memory product routed through the first-class-output rail. v11 folds the compiler's projection-report
        // loss ledger as the projection-ledger named graph, unions the logic-compile
        // diagnostics into the diagnostics graph, and sources REP_AXIOMS from the
        // in-memory stage-compile-logic product (single-pass freshness).
        // v12 folds the external-corpus divergence Findings (graph/conformance) from
        // the stage-conformance product, when any committed case diverges from its
        // published expected verdict. v13 folds the dogfooded occurrence-based
        // provenance projection (graph/provenance) — the public compilation-unit +
        // per-lane carrier manifest (no runtime ids, S0.5) — and gates that every
        // authored quad carries ≥1 stage-origin. v14 folds the byte-exact
        // generated metadata, statement, reasoning, and preservation projections into
        // REP_GENERATED so the superset gate can reconstruct every committed
        // generated file without re-reading disk.
        // v15 embeds the executable-docs surfaces in the ontology-docs site — the
        // offline SPARQL playground asset and the reasoner "try it"
        // asserted-vs-inferred diffs (a docs-only side computation that never folds
        // into the reasoned production graphs) — and folds the CLIF projection
        // (generated/cl/gmeow.clif) into REP_GENERATED as a committed byte projection
        // (a non-RDF text dialect with generated comments / section markers) and the
        // executable-docs surfaces (offline SPARQL playground + reasoner "try it" diffs).
        // v16: the presenter reads the self-description graphs off stage-source-load (no
        // in-snapshot source load or canonicalize) — PIPELINE_SPINE §3.2/§4.
        // v17: the RDF fanout members (profiles / evals scores / research-object graphs)
        // ride in from their producing export leaves — `rdf_fanout_members` reads them
        // off those products instead of re-rendering from disk (§3.2 transform-once).
        // v18 additionally consumes stage-constraint-catalog and folds its generated
        // `.nq` as the graph/fanout/catalog/constraint-catalog.nq named graph.
        // v19 sources REP_SHAPES' generated members (result-shapes.ttl +
        // frame-shapes.ttl) from the consumed export-leaf products instead of the
        // stale disk read, matching the validation-shapes.ttl freshness rule — a
        // new competency ResultShape now reaches the bundle (and the fanout) in one
        // regenerate.
        // v20 embeds the repo-wide slice-quality HTML report, produced by stage-source-load
        // from the same sweep as graph/quality-assessment, into the ontology-docs archive.
        // v21 folds two NEW documentation-projection blobs: the mdbook `src/` source tree
        // (REP_DOCS_BOOK, English-tagged) and the print documentation projection
        // (REP_DOCS_PRINT — the byte-reproducible gmeow.pdf + its deterministic gmeow.typ),
        // built concurrently with the ontology-docs site render so the PDF compile overlaps
        // the per-language renders; the print blob additionally consumes stage-export-references
        // for the bibliography.
        // v22 additionally consumes stage-math-producers and folds its five math flagship
        // producer graphs into gmeow.gts as bundle-internal named graphs (Design A).
        // v23: the embedded ontology-docs site now carries conformance Do/Don't fixtures
        // (`DocsModel::fixtures`, joined to each slice's `tests/example-conformance.ttl`
        // binding) — a new `Page::FixtureIndex` page and a per-term "Conformance examples"
        // section, so the rendered site bytes change shape for an unchanged model schema.
        // v24: `DocCompetency` grows the resolved `query_text` / `exact_rows` /
        // `expected_row_count` / structured `expected_rows` surface (T2) — a new
        // `Page::CompetencyIndex` page renders the full copy-paste-runnable SPARQL
        // question set, so the rendered site bytes change shape again.
        // v25 removes every derived documentation/presentation payload from the
        // logical bundle: site, mdbook, print, slice guides, OKF, JSON-LD, and
        // YAML-LD are regenerated externally by `make docs`.
        // v26 additionally folds `stage-math-producers`' SIXTH graph
        // (graph/math-producers/probability-model, `gmeow_math::producers::
        // probability_model_seam`) — the probability layer's live
        // `logic:probabilityModel` A-box crossing triple now ships inside
        // `gmeow.gts` itself (Design A), not only in the illustrative
        // `examples/probability.ttl` fixture validated on disk.
        // v27 additionally folds `stage-math-producers`' SEVENTH graph
        // (graph/math-producers/pvalue-tri-slice, `gmeow_math::producers::
        // pvalue_tri_slice`) — the charter's signature lang: -> logic: -> math:
        // round-trip ("the p-value was 0.03" grounded as a lang:SurfaceForm
        // denoting a logic:Formula that predicates over a well-framed math:PValue)
        // now ships inside `gmeow.gts` itself (Design A), not only in the
        // illustrative `examples/pvalue-tri-slice.ttl` fixture validated on disk.
        "snapshot.v27-pvalue-tri-slice-producer"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        let mut files = Vec::new();
        // REP_SHAPES folds the AUTHORED shape surface (`shapes/*.ttl` +
        // `slices/<g>/<n>/shapes.ttl`) off disk in `build_archive_blobs` (authored
        // sources, allowed) — declare them so an authored-shape edit busts this stage and
        // the sink re-folds the bundle (cache soundness). The GENERATED shape members
        // (validation/result/frame/constraint-shapes) are NO LONGER read from disk: the
        // sink product-sources them from the consumed export-leaf + compile-logic products,
        // whose digests already cover a shape-source edit, so `generated/shapes` is no
        // longer declared here (the stale-disk-fold class this change retires). Likewise
        // REP_AXIOMS is sourced from the `stage-compile-logic` product, so the AXIOM_FILES
        // are not declared here either.
        files.extend(list_files(&root.join("shapes"), "ttl")?);
        files.extend(slice_named_files(root, "shapes.ttl")?);
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Assemble the terminal carrier ONCE — the single native RdfDataset that every
        // export leaf (N-Quads/TriG/JSON-LD/OKF/LPG/metadata/logic AND the gts export)
        // reads off this product's bundle. NOTHING is serialized here: GTS is exit-only,
        // presented as bytes by the sole terminal writer (`gts_sink::GtsSinkStage`),
        // never by this stage — the snapshot product carries the carrier, no byte lane.
        let carrier = assemble_carrier(input.upstream)?;
        let bundle = build_snapshot_bundle(carrier, input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id(),
            std::sync::Arc::new(bundle),
        )))
    }
}

/// Build the snapshot product bundle: the fully-assembled carrier dataset — this stage
/// attaches NO byte artifacts (the sole terminal `gts_sink` serializes the carrier to
/// `gmeow.gts`) — whose `graph/logic` and `graph/reasoning` named graphs are the
/// canonical projections of the compiled program and the typed reasoning result, with
/// the upstream typed [`PipelineHandle::Logic`] and
/// [`PipelineHandle::Reasoning`](crate::bundle::PipelineHandle::Reasoning) re-pinned to
/// those graphs' canonical digests.
///
/// Each handle's payload is taken from its upstream product's handle (never
/// re-compiled / re-run); the backing graph is re-derived from the SAME projection the
/// snapshot folded, so each pinned digest is a pure function of that projection alone.
/// A missing handle or a digest mismatch HARD-fails (no-optionality, fail-closed).
fn build_snapshot_bundle(
    carrier: std::sync::Arc<purrdf::RdfDataset>,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<purrdf::PipelineBundle<crate::bundle::PipelineHandle>, gmeow_errors::Diag> {
    // ── the Logic handle payload + its backing graph/logic projection ────────────
    let compile = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product for the Logic handle"))?;
    let entry = compile
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_LOGIC)
        .ok_or_else(|| stage_err("stage-compile-logic product carries no Logic handle"))?;
    let crate::bundle::PipelineHandle::Logic(program) = &entry.payload else {
        return Err(stage_err(
            "stage-compile-logic handle for graph/logic is not the Logic arm",
        ));
    };
    let program = program.clone();

    // ── the RelationalCore handle payload + its backing graph/relational-core ─────
    let rc_entry = compile
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_RELATIONAL_CORE)
        .ok_or_else(|| stage_err("stage-compile-logic product carries no RelationalCore handle"))?;
    let crate::bundle::PipelineHandle::RelationalCore(rc_program) = &rc_entry.payload else {
        return Err(stage_err(
            "stage-compile-logic handle for graph/relational-core is not the RelationalCore arm",
        ));
    };
    let rc_program = rc_program.clone();

    // ── the Correspondence handle payload + its backing graph/correspondence ──────
    let corr_entry = compile
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_CORRESPONDENCE)
        .ok_or_else(|| stage_err("stage-compile-logic product carries no Correspondence handle"))?;
    let crate::bundle::PipelineHandle::Correspondence(corr_program) = &corr_entry.payload else {
        return Err(stage_err(
            "stage-compile-logic handle for graph/correspondence is not the Correspondence arm",
        ));
    };
    let corr_program = corr_program.clone();

    // ── the Reasoning handle payload + its backing graph/reasoning projection ─────
    let reason = upstream
        .get("stage-reason")
        .ok_or_else(|| stage_err("missing stage-reason product for the Reasoning handle"))?;
    let reason_entry = reason
        .bundle()
        .handle(gmeow_logic::result_rdf::GRAPH_REASONING)
        .ok_or_else(|| stage_err("stage-reason product carries no Reasoning handle"))?;
    let crate::bundle::PipelineHandle::Reasoning(result) = &reason_entry.payload else {
        return Err(stage_err(
            "stage-reason handle for graph/reasoning is not the Reasoning arm",
        ));
    };
    let result = result.clone();

    // The bundle's dataset IS the assembled snapshot carrier (the whole snapshot, the same
    // value serialized to gmeow.gts) — never a second, partial assembly. The carried
    // graph/logic + relational-core + correspondence + reasoning are already folded into
    // it, so each typed handle re-pins to ITS graph in the carrier (digest hard-checked).
    // The snapshot product carries the carrier ALONE — no byte artifacts (only the
    // terminal `gts_sink` emits bytes), so the artifact map is empty by construction.
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        carrier,
        BTreeMap::new(),
        DatasetProvenance::new(),
    );
    let pinned_logic = bundle.graph_digest(crate::stages::compile_logic::GRAPH_LOGIC);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_LOGIC,
            crate::bundle::PipelineHandle::Logic(program),
            pinned_logic,
        )
        .map_err(|e| stage_err(&format!("re-pin Logic handle on snapshot product: {e}")))?;
    let pinned_reasoning = bundle.graph_digest(gmeow_logic::result_rdf::GRAPH_REASONING);
    bundle
        .pin_handle(
            gmeow_logic::result_rdf::GRAPH_REASONING,
            crate::bundle::PipelineHandle::Reasoning(result),
            pinned_reasoning,
        )
        .map_err(|e| stage_err(&format!("re-pin Reasoning handle on snapshot product: {e}")))?;
    let pinned_rc = bundle.graph_digest(crate::stages::compile_logic::GRAPH_RELATIONAL_CORE);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_RELATIONAL_CORE,
            crate::bundle::PipelineHandle::RelationalCore(rc_program),
            pinned_rc,
        )
        .map_err(|e| {
            stage_err(&format!(
                "re-pin RelationalCore handle on snapshot product: {e}"
            ))
        })?;
    let pinned_corr = bundle.graph_digest(crate::stages::compile_logic::GRAPH_CORRESPONDENCE);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_CORRESPONDENCE,
            crate::bundle::PipelineHandle::Correspondence(corr_program),
            pinned_corr,
        )
        .map_err(|e| {
            stage_err(&format!(
                "re-pin Correspondence handle on snapshot product: {e}"
            ))
        })?;
    Ok(bundle)
}

/// Build the per-quad provenance sidecar for the authored base graph, GATE it
/// (every authored quad must carry ≥1 occurrence — an unattributed quad is a HARD
/// FAIL, no-optionality), and project its PUBLIC projection into the deterministic
/// `graph/provenance` N-Triples. Only public unit names/IRIs + kinds +
/// artifact paths reach the projection — NO runtime `UnitId` / `ArtifactId` /
/// `OriginSetId` (S0.5). The fixed carrier-lane manifest + the realized process
/// vocab (`gmeow:Procedure` / `gmeow:ProcedureStep` / `gmeow:Execution`) round it out.
fn build_provenance_projection(root: &Path) -> Result<String, gmeow_errors::Diag> {
    let (prov, expected) = crate::stages::source_load::attributed_base_provenance(root)?;
    // The hard-fail gate: every authored quad has ≥1 stage-origin occurrence and every
    // occurrence references a registered unit + artifact. A violation aborts the build.
    purrdf::provenance::check_provenance(&prov, &expected).map_err(|errors| {
        stage_err(&format!(
            "provenance gate: {} authored quad(s) unattributed or mis-attributed: {}",
            errors.len(),
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })?;
    Ok(crate::stages::provenance_graph::project_provenance_graph(
        &prov.public_projection(),
    ))
}

/// Re-root every quad of `src` into the named graph `graph_iri` (preserving the
/// graph-less reifier/annotation side-tables), so a carrier subgraph projected via
/// [`RdfDataset::project_named_graph`](purrdf::RdfDataset::project_named_graph) —
/// which strips the graph name to the default graph — folds back into ITS named graph,
/// never the authored default graph.
pub(crate) fn rooted_in_graph(
    src: &purrdf::RdfDataset,
    graph_iri: &str,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    use purrdf::{RdfDatasetBuilder, RdfTerm};
    let graph = RdfTerm::Iri(graph_iri.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in src.owned_quads() {
        quad.graph_name = Some(graph.clone());
        builder.push_owned_quad(&quad);
    }
    for reifier in src.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in src.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder
        .freeze()
        .map_err(|e| stage_err(&format!("re-root carrier graph <{graph_iri}>: {e}")))
}

// ── default graph (authored ontology, NO imports) ───────────────────────────────

/// The localizable predicates shared with `gmeow-docs` i18n compilation: the
/// vocabulary surface a slice `.po` catalog may translate. Full IRIs.
const LOCALIZABLE_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2000/01/rdf-schema#comment",
    "http://www.w3.org/2004/02/skos/core#definition",
    "http://www.w3.org/2004/02/skos/core#scopeNote",
    "http://www.w3.org/2004/02/skos/core#example",
    "http://www.w3.org/2004/02/skos/core#prefLabel",
    "http://www.w3.org/2004/02/skos/core#altLabel",
    "http://www.w3.org/2004/02/skos/core#note",
    "http://purl.org/dc/terms/title",
    "http://purl.org/dc/terms/description",
    "https://blackcatinformatics.ca/gmeow/name",
    "https://blackcatinformatics.ca/gmeow/title",
    "https://blackcatinformatics.ca/gmeow/description",
    "https://blackcatinformatics.ca/gmeow/fullName",
];

/// Load `ontology/gmeow.ttl` + every slice `module.ttl` into one store, merge the
/// slice `.po` translations onto its localizable literals, and return canonical
/// N-Quads. This is `load_merged_graph(include_imports=False)` followed by
/// `merge_terms(graph, po_paths)` — the committed default graph is multilingual.
fn load_authored_default(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let onto = root.join("ontology").join("gmeow.ttl");
    // The root ontology is REQUIRED — the authored default graph is meaningless
    // without it. A missing `ontology/gmeow.ttl` HARD-fails rather than silently
    // assembling a partial default graph (no-optionality).
    if !onto.is_file() {
        return Err(stage_err(&format!(
            "required root ontology {} is missing",
            onto.display()
        )));
    }
    // Root ontology + every slice `module.ttl`, each parsed into its OWN dataset so
    // its blank labels live in an independent scope, then merged via the native
    // standardize-apart `RdfDataset::union`. This REPLACES the per-file `f{scope}_`
    // string-prefixing (`ingest_turtle_scoped`) and the oxigraph `Store` accumulation:
    // the union's per-input `BlankScope` keeps structurally-distinct blank-node axioms
    // (two `owl:AllDisjointClasses` lists) disjoint, the very distinctness the build
    // relies on. Imports are EXCLUDED — they ride `graph/imports` (`load_imports`).
    let mut sources: Vec<Vec<u8>> = Vec::new();
    sources.push(std::fs::read(&onto)?);
    for module in crate::stages::source_load::module_files(root)? {
        sources.push(std::fs::read(&module)?);
    }
    let base = union_turtle_datasets(&sources)?;

    // The merged default graph as a flat native quad list (the union's standardized
    // blank labels), onto which multilingual translations are folded natively.
    // Documentation guide digests are external projection metadata and do not enter
    // the logical bundle.
    let mut quads = flat_rdf_quads_from_dataset(&base);
    merge_translations(root, &mut quads)?;

    let dataset = purrdf::dataset_from_quads(&quads)
        .map_err(|e| stage_err(&format!("authored default graph freeze: {e}")))?;
    dataset_to_nquads(&dataset)
}

/// Merge the slice `.po` translations into the default-graph quad list, mirroring
/// `merge_terms`: for every base-graph localizable literal `(iri, predicate)`, add a
/// translated literal `(iri, predicate, "msgstr"@<internal-tag>)` for each language
/// that translates it. The translation index + the BCP-47 → `x-gmeow-*` tag map come
/// from the native `gmeow_docs::Translations` (the same catalog the docs render).
///
/// The scan is over the pre-translation base quads (a snapshot taken before any
/// additions), so a translated literal is never itself re-scanned — matching the
/// original `quads_for_pattern` view of the base store.
fn merge_translations(root: &Path, quads: &mut Vec<RdfQuad>) -> Result<(), gmeow_errors::Diag> {
    let catalog = purrdf::slice::SliceCatalog::discover(
        &root.join("slices"),
        crate::gmeow_ns::gmeow_slice_vocab(),
    )
    .map_err(|e| stage_err(&format!("slice catalog: {e}")))?;
    let translations = gmeow_docs::Translations::from_catalog(&catalog);
    let langs: Vec<String> = translations.languages().to_vec();
    if langs.is_empty() {
        return Ok(());
    }
    let localizable: std::collections::BTreeSet<&str> =
        LOCALIZABLE_PREDICATES.iter().copied().collect();

    // The base-graph localizable literals: `(subject_iri, predicate_iri)` whose object
    // is a literal (the allowed-keys set of merge_terms). Scanned over the base quads
    // only; additions are appended afterwards.
    let mut additions: Vec<RdfQuad> = Vec::new();
    for quad in quads.iter() {
        let pred = quad.predicate.as_str();
        if !localizable.contains(pred) {
            continue;
        }
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        if !matches!(&quad.object, RdfTerm::Literal(_)) {
            continue;
        }
        for lang in &langs {
            if let Some(msgstr) = translations.lookup(subject, pred, lang) {
                let tag = translations.internal_tag(lang);
                additions.push(RdfQuad::new(
                    RdfTerm::iri(subject.clone()),
                    pred.to_owned(),
                    RdfTerm::literal(RdfLiteral::language_tagged(msgstr, tag)),
                ));
            }
        }
    }
    quads.extend(additions);
    Ok(())
}

// ── imports (graph/imports) ─────────────────────────────────────────────────────

fn load_imports(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let dir = root.join("imports");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "ttl") {
            files.push(path);
        }
    }
    files.sort();
    // Each import file is its own blank scope; merge via the standardize-apart union
    // (the native replacement for the per-file Store accumulation).
    let sources: Vec<Vec<u8>> = files.iter().map(std::fs::read).collect::<Result<_, _>>()?;
    dataset_to_nquads(&union_turtle_datasets(&sources)?)
}

// ── metadata (graph/metadata) ───────────────────────────────────────────────────

fn load_metadata(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let path = root.join("metadata").join("gmeow-self.ttl");
    turtle_to_nquads(&std::fs::read(&path)?)
}

// ── slice-analysis (graph/slice-analysis) ───────────────────────────────────────

/// Build the `gmeow:graph/slice-analysis` graph via the native ownership
/// analyzer — the Rust twin of `gts_gen.build_slice_analysis_graph`. The analyzer
/// reads AUTHORED slices only; `authored_nq` (the authored base graph as text)
/// feeds the emitter's self-attestation guard.
fn build_slice_analysis(root: &Path, authored_nq: &[u8]) -> Result<Vec<u8>, gmeow_errors::Diag> {
    use purrdf::slice::{
        OwnershipAnalyzer, OwnershipStatus, SliceCatalog, ToolchainContext, emit_analysis_graph,
    };

    let slices_dir = root.join("slices");
    let catalog = SliceCatalog::discover(&slices_dir, crate::gmeow_ns::gmeow_slice_vocab())
        .map_err(|e| stage_err(&format!("slice catalog discover: {e}")))?;
    let report = OwnershipAnalyzer::new(&catalog)
        .analyze()
        .map_err(|e| stage_err(&format!("ownership analysis: {e}")))?;

    // tier map + every authored artifact raw digest (mirror PyOwnershipAnalyzer).
    let mut tier_of: std::collections::HashMap<purrdf::slice::SliceIri, u8> =
        std::collections::HashMap::new();
    let mut raw_digests: Vec<String> = Vec::new();
    for record in catalog.records() {
        tier_of.insert(
            record.manifest.slice_iri.clone(),
            tier_priority(record.manifest.tier.as_ref()),
        );
        for artifact in &record.artifacts {
            raw_digests.push(artifact.raw_digest.clone());
        }
    }
    raw_digests.sort_unstable();
    let digests: Vec<&str> = raw_digests.iter().map(String::as_str).collect();

    let term_count_of = |slice: &purrdf::slice::SliceIri| -> usize {
        report
            .ownership
            .values()
            .filter(|o| {
                matches!(o.status, OwnershipStatus::Validated) && &o.declared_owner == slice
            })
            .count()
    };
    let tier_lookup =
        |slice: &purrdf::slice::SliceIri| -> u8 { tier_of.get(slice).copied().unwrap_or(2) };

    let version = ontology_version(authored_nq)?;
    let toolchain = ToolchainContext::new(&version, "dist");
    // The self-attestation guard rejects the analysis graph being fed as its own INPUT
    // (a mention of graph/slice-analysis as CONTENT). The pipeline slice's
    // gmeow:attachesGraph declarations name every carrier graph a stage attaches —
    // including graph/slice-analysis — as a benign object of a declaration triple, not as
    // analysis-graph content. Drop exactly those declaration QUADS (matched on the parsed
    // PREDICATE term, not a text substring) from the guard text so a legitimate attach
    // declaration does not false-trigger the guard while a real content mention of the
    // string elsewhere (a literal, a comment, a non-predicate position) is preserved
    // faithfully; the guard's real purpose (catching the analysis graph re-consumed as
    // input) is intact, and the analysis itself reads `report.edges` + digests, never
    // this text.
    let attaches_graph_pred = format!("{GMEOW_NS}attachesGraph");
    let authored_text: String = parse_nq(authored_nq)?
        .iter()
        .filter(|quad| quad.predicate != attaches_graph_pred)
        .map(|quad| format!("{} <{}> {} .", quad.subject, quad.predicate, quad.object))
        .collect::<Vec<_>>()
        .join("\n");
    let graph = emit_analysis_graph(
        &crate::gmeow_ns::gmeow_slice_vocab(),
        &report.edges,
        &authored_text,
        &digests,
        &toolchain,
        tier_lookup,
        term_count_of,
    )
    .map_err(|e| stage_err(&format!("slice-analysis emit: {e}")))?;

    // The emitter returns a Turtle body; normalize to N-Quads natively so the builder
    // ingests it the same way as every other named-graph source.
    turtle_to_nquads(graph.turtle_body.as_bytes())
}

fn tier_priority(tier: Option<&purrdf::slice::SliceTier>) -> u8 {
    use purrdf::slice::SliceTier;
    match tier {
        Some(SliceTier::Core) => 0,
        Some(SliceTier::Extension) => 1,
        Some(SliceTier::Domain) | Some(SliceTier::Unknown(_)) | None => 2,
    }
}

/// The authored ontology `owl:versionInfo` (a hard requirement — never defaulted).
fn ontology_version(authored_nq: &[u8]) -> Result<String, gmeow_errors::Diag> {
    let onto = GMEOW_NS.trim_end_matches('/');
    let version_info = "http://www.w3.org/2002/07/owl#versionInfo";
    for quad in parse_nq(authored_nq)? {
        if let RdfTerm::Iri(subject) = &quad.subject
            && subject == onto
            && quad.predicate.as_str() == version_info
            && let RdfTerm::Literal(l) = &quad.object
        {
            return Ok(l.lexical_form.clone());
        }
    }
    Err(stage_err(&format!(
        "authored ontology {GMEOW_NS} has no owl:versionInfo"
    )))
}

// ── alignments (graph/alignments) ───────────────────────────────────────────────

/// Build the SSSOM alignment-axiom graph: one `(subject, predicate, object)`
/// triple per SSSOM data row with CURIEs expanded through the per-file
/// `# curie_map:` header, deduplicated. Sourced from THIS run's `stage-mappings`
/// product artifacts (`generated/mappings/*.sssom.tsv`), NOT the committed disk files:
/// the alignment graph is a projection of the freshly-compiled SSSOM, so reading disk
/// here would carry the last-committed mappings forever (the stale-disk-fold class).
/// The mappings stage builds `graph/alignments` from this helper and unions it into its
/// product; the presenter and the reasoning EDB read it back via `producer_graph`.
pub(crate) fn alignment_nquads_from_artifacts(
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    // BTreeMap iterates by key (repo-relative path), so the `generated/mappings/*.sssom.tsv`
    // are visited in the same sorted order the former disk `read_dir(...).sort()` produced.
    let mut quads: Vec<RdfQuad> = Vec::new();
    for (path, bytes) in artifacts {
        if !(path.starts_with("generated/mappings/") && path.ends_with(".sssom.tsv")) {
            continue;
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|e| stage_err(&format!("sssom {path} is not utf-8: {e}")))?;
        for (s, p, o) in alignment_rows(text)? {
            quads.push(RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)));
        }
    }
    let dataset = purrdf::dataset_from_quads(&quads)
        .map_err(|e| stage_err(&format!("alignment graph freeze: {e}")))?;
    dataset_to_nquads(&dataset)
}

/// Parse one SSSOM TSV into `(subject_iri, predicate_iri, object_iri)` rows,
/// expanding CURIEs through the file's `# curie_map:` header block.
fn alignment_rows(text: &str) -> Result<Vec<(String, String, String)>, gmeow_errors::Diag> {
    let mut curie_map: BTreeMap<String, String> = BTreeMap::new();
    let mut in_curie_map = false;
    let mut header: Option<Vec<String>> = None;
    let mut rows: Vec<(String, String, String)> = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            let trimmed = rest.trim();
            if trimmed == "curie_map:" {
                in_curie_map = true;
                continue;
            }
            if in_curie_map {
                // `#   prefix: namespace` — two leading spaces then `prefix: ns`.
                if let Some((prefix, ns)) = trimmed.split_once(':') {
                    // Only treat as a curie-map entry if it looks like `name: uri`.
                    let prefix = prefix.trim();
                    let ns = ns.trim();
                    if !prefix.is_empty() && (ns.contains("://") || ns.starts_with("urn:")) {
                        curie_map.insert(prefix.to_string(), ns.to_string());
                        continue;
                    }
                }
                // A non-curie header line ends the curie_map block.
                in_curie_map = false;
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<String> = line.split('\t').map(str::to_string).collect();
        if header.is_none() {
            header = Some(cols);
            continue;
        }
        let head = header.as_ref().unwrap();
        let get = |name: &str| -> Option<&str> {
            head.iter()
                .position(|h| h == name)
                .and_then(|i| cols.get(i).map(String::as_str))
        };
        let (Some(s), Some(p), Some(o)) =
            (get("subject_id"), get("predicate_id"), get("object_id"))
        else {
            continue;
        };
        if s.is_empty() || p.is_empty() || o.is_empty() {
            continue;
        }
        rows.push((
            expand_curie(s, &curie_map)?,
            expand_curie(p, &curie_map)?,
            expand_curie(o, &curie_map)?,
        ));
    }
    Ok(rows)
}

/// Expand a `prefix:local` CURIE through `curie_map` (an already-absolute IRI
/// passes through). Mirrors `mappings.expand_curie`.
fn expand_curie(
    curie: &str,
    curie_map: &BTreeMap<String, String>,
) -> Result<String, gmeow_errors::Diag> {
    if curie.starts_with("http://") || curie.starts_with("https://") || curie.starts_with("urn:") {
        return Ok(curie.to_string());
    }
    if let Some((prefix, local)) = curie.split_once(':')
        && let Some(ns) = curie_map.get(prefix)
    {
        return Ok(format!("{ns}{local}"));
    }
    Err(stage_err(&format!("unresolvable CURIE {curie:?}")))
}

// ── verify attestation (graph/verify) ───────────────────────────────────────────

/// Run the native verify lane over `edb` and build the attestation graph as
/// N-Quads. Mirrors `gts_gen.build_verify_attestation_graph` exactly (the same
/// `gmeow:QualityAssessment` vocabulary, one per query).
///
/// The query set is compile-time-embedded (`gmeow_logic::verify::
/// embedded_verify_queries`) rather than walked off disk: `queries/verify/` and
/// `slices/**/queries/verify/` are baked into the `gmeow-logic` binary by its
/// `build.rs`, sorted by stem.
fn run_verify_attestation(edb: &purrdf::RdfDataset) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let pairs = gmeow_logic::verify::embedded_verify_queries();

    let report = gmeow_logic::verify::verify(edb, &pairs)
        .map_err(|e| stage_err(&format!("native verify: {e}")))?;

    // The failed set: stems whose finding is an error coded `verify.<stem>`.
    let mut failed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for finding in &report.findings {
        if matches!(finding.severity, gmeow_errors::Severity::Error)
            && finding.code.starts_with("verify.")
        {
            failed.insert(finding.code["verify.".len()..].to_string());
        }
    }

    let attestation = emit_verify_attestation(&pairs, &failed);
    turtle_to_nquads(attestation.as_bytes())
}

/// Emit the verify-attestation Turtle (pure, deterministic). One
/// `gmeow:QualityAssessment` per query; mirrors `build_verify_attestation_graph`.
fn emit_verify_attestation(
    query_paths: &[(String, String)],
    failed: &std::collections::BTreeSet<String>,
) -> String {
    use std::fmt::Write;
    let mut body = String::new();
    writeln!(body, "@prefix gmeow: <{GMEOW_NS}> .").unwrap();
    writeln!(body, "@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .").unwrap();
    writeln!(
        body,
        "@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> ."
    )
    .unwrap();
    writeln!(body).unwrap();

    let ontology_iri = GMEOW_NS.trim_end_matches('/');
    // The verify activity and per-query assessments are generated A-Box
    // instance data folded into the bundle's `graph/verify`, not vocabulary
    // surface: each typed subject carries a human label, its named-graph
    // provenance anchor, and the assertional `gmeow:boxABox` role so the bundle
    // satisfies the assertional-tier validation contract (no `skos:definition`).
    writeln!(
        body,
        "<{GMEOW_NS}activity/native-verify> a <{GMEOW_NS}Activity> ;"
    )
    .unwrap();
    writeln!(body, "    rdfs:label \"Native verify activity\" ;").unwrap();
    writeln!(body, "    rdfs:isDefinedBy <{GMEOW_NS}graph/verify> ;").unwrap();
    writeln!(body, "    gmeow:graphBoxRole gmeow:boxABox ;").unwrap();
    writeln!(
        body,
        "    <{GMEOW_NS}wasAssociatedWith> <{GMEOW_NS}agent/native-verify> ."
    )
    .unwrap();
    writeln!(body).unwrap();

    for (name, _sparql) in query_paths {
        let stem = query_stem(name);
        let passed = !failed.contains(stem);
        writeln!(body, "<{GMEOW_NS}verify-attestation/{stem}>").unwrap();
        writeln!(body, "    a <{GMEOW_NS}QualityAssessment> ;").unwrap();
        writeln!(body, "    rdfs:label \"Verify attestation: {stem}\" ;").unwrap();
        writeln!(body, "    rdfs:isDefinedBy <{GMEOW_NS}graph/verify> ;").unwrap();
        writeln!(body, "    gmeow:graphBoxRole gmeow:boxABox ;").unwrap();
        writeln!(body, "    <{GMEOW_NS}assessedEntity> <{ontology_iri}> ;").unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}qualityDimension> <{GMEOW_NS}qualityDimensionLogicalConsistency> ;"
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}observationResult> \"{}\"^^xsd:boolean ;",
            if passed { "true" } else { "false" }
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}wasDerivedFrom> <{GMEOW_NS}verify-query/{stem}> ;"
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}wasGeneratedBy> <{GMEOW_NS}activity/native-verify> ."
        )
        .unwrap();
        writeln!(body).unwrap();
    }
    body
}

fn query_stem(name: &str) -> &str {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .strip_suffix(".rq")
        .unwrap_or(name)
}

// ── small helpers ───────────────────────────────────────────────────────────────

/// Canonicalize N-Quads bytes and route them into `graph_name` on `builder` — the
/// oxigraph-ingestion path the byte-golden tests use to author fixture snapshots.
/// Production assembly now goes through the native carrier ([`assemble_carrier`]).
#[cfg(test)]
fn add_named(
    builder: &mut SnapshotBuilder,
    nq_bytes: &[u8],
    graph_name: &str,
    scope: &str,
) -> Result<(), gmeow_errors::Diag> {
    let canon = canonicalize_nq(nq_bytes, scope)?;
    let quads = parse_nq(canon.as_bytes())?;
    reject_quoted_triples(&quads, graph_name)?;
    let dataset = parse_dataset(canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("add_named parse: {e}")))?;
    builder
        .add_dataset_scoped(&dataset, Some(graph_name), Some(scope))
        .map_err(|e| stage_err(&e))?;
    Ok(())
}

/// Ingest a default-graph N-Quads fixture under a blank scope (test-only): canonicalize
/// → native parse → `add_dataset_scoped`, the carrier-test analogue of [`add_named`] for
/// the default graph (no graph name).
#[cfg(test)]
fn add_base_nq(
    builder: &mut SnapshotBuilder,
    nq_bytes: &[u8],
    scope: &str,
) -> Result<(), gmeow_errors::Diag> {
    let canon = canonicalize_nq(nq_bytes, scope)?;
    let quads = parse_nq(canon.as_bytes())?;
    reject_quoted_triples(&quads, "default")?;
    let dataset = parse_dataset(canon.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err(&format!("add_base_nq parse: {e}")))?;
    builder
        .add_dataset_scoped(&dataset, None, Some(scope))
        .map_err(|e| stage_err(&e))?;
    Ok(())
}

/// A plain RDF-1.1 N-Quads fixture must carry no quoted-triple (`<<>>`) object: the
/// RDF-1.2 statement layer rides the dataset's reifier/annotation side-tables (which
/// `add_dataset` folds), never a base quoted-triple object. A quoted triple here would
/// be a real defect — HARD-fail rather than let it shrink the fold (no-optionality /
/// no silent data loss).
fn reject_quoted_triples(quads: &[RdfQuad], graph_name: &str) -> Result<(), gmeow_errors::Diag> {
    if quads.iter().any(|q| matches!(q.object, RdfTerm::Triple(_))) {
        return Err(stage_err(&format!(
            "graph {graph_name} carries a quoted-triple (<<>>) object that the base-quad fold \
             would not represent; the RDF-1.2 statement layer must ride the reifier/annotation \
             tables, not a base quad"
        )));
    }
    Ok(())
}

/// Canonicalize a graph's blank-node labels under RDFC-1.0, returning N-Quads.
/// Mirrors `compile_gts`'s `to_canonical_graph` before each `add_graph`.
fn canonicalize_nq(nq_bytes: &[u8], _scope: &str) -> Result<String, gmeow_errors::Diag> {
    // Native full RDFC-1.0, replacing oxrdf `Dataset::canonicalize`.
    // `canonical_flat_nquads` parses → flattens the RDF 1.2 statement overlay → RDFC-1.0
    // canonicalizes, byte-identical to the prior oxigraph `canonicalize_quads` flat path
    // (conformant SHA-256 RDFC-1.0, identical blank labeling). The returned N-Quads lines
    // are already `.`-terminated and bytewise-sorted.
    let dataset = parse_dataset(nq_bytes, "application/n-quads", None)
        .map_err(|e| stage_err(&format!("canonicalize parse: {e}")))?;
    purrdf::canonical_flat_nquads(&dataset).map_err(|e| stage_err(&format!("canonicalize: {e}")))
}

fn parse_nq(bytes: &[u8]) -> Result<Vec<RdfQuad>, gmeow_errors::Diag> {
    parse_rdf(bytes, "application/n-quads")
}

/// Parse RDF text of `media_type` into a flat native quad list via the native codecs.
/// The IR fold + [`flat_rdf_quads_from_dataset`] un-fold are exact inverses (set-equal
/// to the original parse), so the RDF 1.2 statement layer's `rdf:reifies`/annotation
/// rows are re-materialized for `add_rdf12`'s own fold.
fn parse_rdf(bytes: &[u8], media_type: &str) -> Result<Vec<RdfQuad>, gmeow_errors::Diag> {
    let dataset =
        parse_dataset(bytes, media_type, None).map_err(|e| stage_err(&format!("parse: {e}")))?;
    Ok(flat_rdf_quads_from_dataset(&dataset))
}

/// Parse one Turtle source's bytes into a frozen [`RdfDataset`] via the native
/// codec. The IR fold standardizes blank labels per-dataset, so each parse is an
/// independent blank-node scope — [`RdfDataset::union`] keeps those scopes disjoint.
fn parse_turtle_dataset(
    bytes: &[u8],
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    parse_dataset(bytes, "text/turtle", None).map_err(|e| stage_err(&format!("parse: {e}")))
}

/// Serialize a frozen [`RdfDataset`] to N-Quads, the same byte form every named-graph
/// source flows through before [`add_named`] re-canonicalizes it. Fully native: no
/// oxigraph `Store`, no oxigraph quad detour.
///
/// CRITICAL: the typed-literal lexical forms are canonicalized to the W3C-canonical XSD
/// mapping (`0.90` → `0.9`, `1.0` → `1.0`, `415.0` → `415.0`, `+00:00` → `Z`) via
/// [`purrdf::xsd::parse_by_iri`] + [`purrdf::xsd::XsdValue::canonical_lexical`]. The native
/// codecs PRESERVE raw lexical forms on a faithful round-trip, so without this normalize
/// the committed canonical bundle (and every artifact re-derived from it) would drift.
/// Byte-compatibility with oxigraph's literal value-space is NOT a goal — correct native
/// XSD-canonical output is. The canonicalization recurses into quoted-triple (RDF 1.2)
/// objects, and a malformed typed literal HARD-fails (no-optionality).
///
/// The mapped quads / reifier bindings / annotations are re-interned through a fresh
/// [`purrdf::RdfDatasetBuilder`] (carrying the full RDF 1.2 statement layer), so the
/// whole pass stays on the native kernel — no transient oxigraph `Store`.
fn dataset_to_nquads(dataset: &purrdf::RdfDataset) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        builder.push_owned_quad(&canonicalize_quad_xsd(quad)?);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&canonicalize_reifier_xsd(reifier)?);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&canonicalize_annotation_xsd(annotation)?);
    }
    let normalized = builder
        .freeze()
        .map_err(|e| stage_err(&format!("literal-canonical freeze: {e}")))?;
    serialize_dataset(
        normalized.as_ref(),
        "application/n-quads",
        SerializeGraph::Dataset,
    )
    .map_err(|e| stage_err(&format!("serialize: {e}")))
}

/// Canonicalize every typed-literal lexical form in an owned [`purrdf::RdfQuad`] to
/// the W3C XSD canonical mapping via gmeow-xsd, recursing into quoted-triple terms.
fn canonicalize_quad_xsd(mut quad: purrdf::RdfQuad) -> Result<purrdf::RdfQuad, gmeow_errors::Diag> {
    canonicalize_term_xsd(&mut quad.subject)?;
    canonicalize_term_xsd(&mut quad.object)?;
    if let Some(graph_name) = quad.graph_name.as_mut() {
        canonicalize_term_xsd(graph_name)?;
    }
    Ok(quad)
}

/// As [`canonicalize_quad_xsd`] for an owned RDF 1.2 reifier binding.
fn canonicalize_reifier_xsd(
    mut reifier: purrdf::RdfReifier,
) -> Result<purrdf::RdfReifier, gmeow_errors::Diag> {
    canonicalize_triple_xsd(&mut reifier.statement)?;
    canonicalize_term_xsd(&mut reifier.reifier)?;
    Ok(reifier)
}

/// As [`canonicalize_quad_xsd`] for an owned RDF 1.2 statement annotation.
fn canonicalize_annotation_xsd(
    mut annotation: purrdf::RdfAnnotation,
) -> Result<purrdf::RdfAnnotation, gmeow_errors::Diag> {
    canonicalize_term_xsd(&mut annotation.reifier)?;
    canonicalize_term_xsd(&mut annotation.object)?;
    Ok(annotation)
}

/// Recurse a single owned [`purrdf::RdfTriple`], canonicalizing its term literals.
fn canonicalize_triple_xsd(triple: &mut purrdf::RdfTriple) -> Result<(), gmeow_errors::Diag> {
    canonicalize_term_xsd(&mut triple.subject)?;
    canonicalize_term_xsd(&mut triple.object)?;
    Ok(())
}

/// Canonicalize a single owned [`purrdf::RdfTerm`] in place: a typed literal with a
/// recognized XSD datatype is rewritten to its W3C-canonical lexical form, a
/// quoted-triple term recurses, and every other term (IRI, blank node, language-tagged
/// literal, `xsd:string`/unrecognized-datatype literal) is left VERBATIM.
///
/// A malformed lexical for a RECOGNIZED XSD datatype HARD-fails (`Err` from
/// `parse_by_iri`): an authored ontology should never carry one, so surface it
/// (no-optionality) rather than silently passing it through.
fn canonicalize_term_xsd(term: &mut purrdf::RdfTerm) -> Result<(), gmeow_errors::Diag> {
    match term {
        purrdf::RdfTerm::Literal(literal) => {
            // A language tag (rdf:langString) has no numeric value space — verbatim.
            if literal.language.is_some() {
                return Ok(());
            }
            if let Some(datatype_iri) = literal.datatype.as_deref() {
                match purrdf::xsd::parse_by_iri(&literal.lexical_form, datatype_iri) {
                    // Recognized XSD datatype → rewrite to the canonical lexical form.
                    Ok(Some(value)) => literal.lexical_form = value.canonical_lexical(),
                    // Unrecognized datatype IRI → leave the lexical form VERBATIM.
                    Ok(None) => {}
                    // Malformed lexical for a recognized XSD datatype → HARD-fail.
                    Err(e) => {
                        return Err(stage_err(&format!(
                            "malformed typed literal {:?}^^<{datatype_iri}> in the authored ontology: {e:?}",
                            literal.lexical_form
                        )));
                    }
                }
            }
            // A literal with no datatype (xsd:string) has no numeric value space —
            // verbatim.
            Ok(())
        }
        purrdf::RdfTerm::Triple(triple) => canonicalize_triple_xsd(triple),
        purrdf::RdfTerm::Iri(_) | purrdf::RdfTerm::BlankNode(_) => Ok(()),
    }
}

/// Parse a single Turtle source and serialize it straight to N-Quads (no `Store`).
/// The native equivalent of the old `Store::new()+ingest_turtle+store_to_nquads`
/// trio for single-file named-graph sources (metadata, slice-analysis).
pub(crate) fn turtle_to_nquads(bytes: &[u8]) -> Result<Vec<u8>, gmeow_errors::Diag> {
    dataset_to_nquads(parse_turtle_dataset(bytes)?.as_ref())
}

/// The standardize-apart union of several Turtle sources into ONE default-graph
/// dataset. Each source is parsed independently (its own blank scope) and merged via
/// [`RdfDataset::union`], whose per-input `BlankScope` keeps structurally-distinct
/// blank-node axioms (e.g. two `owl:AllDisjointClasses` lists) disjoint — the native
/// replacement for the removed `ingest_turtle_scoped` string-prefix scoping.
fn union_turtle_datasets(sources: &[Vec<u8>]) -> Result<purrdf::RdfDataset, gmeow_errors::Diag> {
    let owned: Vec<std::sync::Arc<purrdf::RdfDataset>> = sources
        .iter()
        .map(|bytes| parse_turtle_dataset(bytes))
        .collect::<Result<_, _>>()?;
    let refs: Vec<&purrdf::RdfDataset> = owned.iter().map(AsRef::as_ref).collect();
    Ok(purrdf::RdfDataset::union(&refs))
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-gts-sink".to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
mod xsd_canon_tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTriple};

    const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
    const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

    /// Serialize a single-quad dataset through `dataset_to_nquads` and return the
    /// canonical N-Quads as a string.
    fn nquads_of(quad: RdfQuad) -> String {
        let mut b = RdfDatasetBuilder::new();
        b.push_owned_quad(&quad);
        let ds = b.freeze().expect("freeze");
        String::from_utf8(dataset_to_nquads(ds.as_ref()).expect("nquads")).expect("utf8")
    }

    fn typed_quad(lexical: &str, datatype: &str) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::typed(lexical, datatype)),
        )
    }

    /// A recognized XSD datatype is rewritten to its W3C-canonical lexical form —
    /// `415.0`→`415.0`, `0.90`→`0.9`, `+00:00`→`Z` (correct native output; oxigraph
    /// byte-parity is NOT a goal).
    #[test]
    fn recognized_xsd_literal_is_canonicalized() {
        for (lex, datatype, expected) in [
            ("0.90", XSD_DECIMAL, "0.9"),
            // XSD 1.1 canonical decimal drops the trailing `.0` for whole values.
            ("415.0", XSD_DECIMAL, "415"),
            ("-200.0", XSD_DECIMAL, "-200"),
            (
                "2024-06-01T10:00:00+00:00",
                XSD_DATETIME,
                "2024-06-01T10:00:00Z",
            ),
        ] {
            let nq = nquads_of(typed_quad(lex, datatype));
            assert!(
                nq.contains(&format!("\"{expected}\"^^<{datatype}>")),
                "{lex}^^<{datatype}> must canonicalize to {expected}; got:\n{nq}"
            );
        }
    }

    /// A language-tagged literal passes through VERBATIM (rdf:langString has no
    /// numeric value space).
    #[test]
    fn language_tagged_literal_is_verbatim() {
        let nq = nquads_of(RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::language_tagged("hallo", "de")),
        ));
        assert!(
            nq.contains("\"hallo\"@de"),
            "lang literal verbatim; got:\n{nq}"
        );
    }

    /// An unrecognized-datatype literal passes through VERBATIM (parse_by_iri →
    /// Ok(None)): `0.90` keeps its trailing zero under a custom datatype.
    #[test]
    fn unknown_datatype_literal_is_verbatim() {
        let custom = "https://example.org/myType";
        let nq = nquads_of(typed_quad("0.90", custom));
        assert!(
            nq.contains(&format!("\"0.90\"^^<{custom}>")),
            "unknown-datatype literal keeps its raw lexical form; got:\n{nq}"
        );
    }

    /// A plain `xsd:string`-no-datatype literal passes through VERBATIM.
    #[test]
    fn plain_string_literal_is_verbatim() {
        let nq = nquads_of(RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::simple("0.90")),
        ));
        assert!(nq.contains("\"0.90\""), "plain string verbatim; got:\n{nq}");
    }

    /// A malformed lexical for a RECOGNIZED XSD datatype HARD-fails (no-optionality):
    /// an authored ontology should never carry one, so surface it.
    #[test]
    fn malformed_recognized_literal_hard_fails() {
        let mut b = RdfDatasetBuilder::new();
        b.push_owned_quad(&typed_quad("not-a-decimal", XSD_DECIMAL));
        let ds = b.freeze().expect("freeze");
        let err = dataset_to_nquads(ds.as_ref())
            .expect_err("a malformed xsd:decimal must hard-fail, not pass through");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("malformed typed literal"),
            "error must name the malformed typed literal; got: {msg}"
        );
    }

    /// A literal nested inside a quoted-triple (RDF 1.2 `<< s p o >>`) object is
    /// canonicalized too (the recursion contract): `xsd:decimal` `0.90`→`0.9`.
    #[test]
    fn quoted_triple_object_literal_is_canonicalized() {
        let inner = RdfTriple::new(
            RdfTerm::iri("https://example.org/qs"),
            "https://example.org/qp",
            RdfTerm::literal(RdfLiteral::typed("0.90", XSD_DECIMAL)),
        );
        let nq = nquads_of(RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::triple(inner),
        ));
        assert!(
            nq.contains(&format!("\"0.9\"^^<{XSD_DECIMAL}>")),
            "the literal inside a quoted triple must canonicalize 0.90→0.9; got:\n{nq}"
        );
        assert!(
            !nq.contains("\"0.90\""),
            "the raw 0.90 form must not survive inside the quoted triple; got:\n{nq}"
        );
    }
}

#[cfg(test)]
mod ustar_tests {
    use super::*;

    /// The GNU long-name sentinel used in wire-format assertions.
    const LONGLINK_NAME: &str = "././@LongLink";

    /// Decode `(name, bytes)` members from a USTAR archive via the shared codec.
    fn parse(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
        purrdf::ustar::read_archive(raw).unwrap()
    }

    #[test]
    fn long_member_name_round_trips_via_longlink() {
        let long = format!(
            "x-gmeow-english/terms/classes/gmeow-{}.html",
            "A".repeat(90)
        );
        assert!(long.len() > 100, "fixture must exceed the 100-byte field");
        let members = vec![
            (long.clone(), b"<html>long</html>".to_vec()),
            ("x-gmeow-english/index.html".to_string(), b"idx".to_vec()),
        ];
        let raw = purrdf::ustar::write_archive(&members).expect("archive");
        let got = parse(&raw);
        assert_eq!(got, members, "GNU LongLink path must round-trip exactly");

        // The first record on the wire is the 'L' LongLink, then the real header
        // whose name field is the 100-byte truncation of the long path.
        assert_eq!(raw[156], b'L', "first record is a LongLink");
        assert_eq!(&raw[0..LONGLINK_NAME.len()], LONGLINK_NAME.as_bytes());
    }

    #[test]
    fn short_names_emit_no_longlink_and_stay_plain_ustar() {
        let members = vec![
            ("mappings/a.sssom.tsv".to_string(), b"x".to_vec()),
            ("slices/core/x/tests/t.ttl".to_string(), vec![0u8; 600]),
        ];
        let raw = purrdf::ustar::write_archive(&members).expect("archive");
        // No member name overflows 100 bytes, so NO 'L' record may appear: the
        // four existing consumer archives must stay byte-identical (fold-stable).
        assert!(
            !raw.chunks(512).any(|c| c.len() == 512 && c[156] == b'L'),
            "short-name archive must not emit a LongLink record"
        );
        // The first header carries the full name inline (typeflag '0', ustar magic).
        assert_eq!(raw[156], b'0');
        assert_eq!(&raw[257..263], b"ustar\0");
        assert_eq!(&raw[263..265], b"00");
        assert_eq!(parse(&raw), members);
    }

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Empty schema surfaces for the blob-archive unit tests, which assert the
    /// REP_AXIOMS / mappings / queries / shapes channels and do not read the schema
    /// bytes (production sources them from the `stage-export-json-schema` product).
    fn empty_schemas() -> SchemaSurfaces<'static> {
        SchemaSurfaces {
            schema: b"",
            openapi: b"",
            card: b"",
            finding: b"",
        }
    }

    /// A minimal non-empty stage-export-pydantic product for the blob-archive unit
    /// tests: one package member under the on-disk prefix, so `build_archive_blobs`
    /// clears its models-python fail-closed guard.
    fn sample_models_python() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([(
            format!(
                "{}gmeow_models/__init__.py",
                crate::stages::pydantic::PACKAGE_DISK_PREFIX
            ),
            b"# gmeow_models\n".to_vec(),
        )])
    }

    /// Mirror the committed `generated/mappings/*.sssom.tsv` AND `generated/queries/*.rq`
    /// into an artifact map keyed by repo-relative path — the stand-in for the
    /// stage-mappings product in blob-archive unit tests (production sources both the
    /// SSSOM surface and the SPARQL query surface from the in-memory product).
    fn mappings_artifacts_from_disk(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        for p in list_files(&root.join("generated/mappings"), "sssom.tsv").unwrap_or_default() {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.insert(
                format!("generated/mappings/{name}"),
                std::fs::read(&p).unwrap_or_else(|_| panic!("read {}", p.display())),
            );
        }
        for p in list_files(&root.join("generated/queries"), "rq").unwrap_or_default() {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.insert(
                format!("generated/queries/{name}"),
                std::fs::read(&p).unwrap_or_else(|_| panic!("read {}", p.display())),
            );
        }
        out
    }

    /// The committed ResultShape SHACL projection — the stand-in for the
    /// stage-export-result-shapes product in blob-archive unit tests (production
    /// sources these from the in-memory product).
    fn fresh_result_shapes_from_disk(root: &Path) -> Vec<u8> {
        let rel = crate::stages::result_shapes::RESULT_SHAPES_PATH;
        std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
    }

    /// The committed P11 frame shapes — the stand-in for the
    /// stage-export-frame-shapes product in blob-archive unit tests.
    fn fresh_frame_shapes_from_disk(root: &Path) -> Vec<u8> {
        let rel = crate::stages::frame_shapes::FRAME_SHAPES_PATH;
        std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
    }

    // The committed logic: FOL-axiom SHACL projection — the stand-in for the
    // stage-export-constraint-shapes product in blob-archive unit tests.
    fn fresh_constraint_shapes_from_disk(root: &Path) -> Vec<u8> {
        let rel = crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH;
        std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
    }

    #[test]
    fn build_docs_archive_packs_the_rendered_site() {
        // The archive packing is exercised with a model-only render (empty executable
        // data): the reasoned "try it" / playground surfaces need a full pipeline run,
        // covered by the regenerate gate, not this structural packing test.
        let root = repo_root();
        let model = gmeow_docs::model::DocsModel::discover(&root).expect("docs model");
        let slice_quality_html = b"<!doctype html><title>slice-quality</title>\n";
        let blob = build_docs_archive(
            &root,
            &model,
            &gmeow_docs::ExecutableDocsData::default(),
            slice_quality_html,
        )
        .expect("docs archive");
        assert_eq!(blob.rep, REP_ONTOLOGY_DOCS);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the site archive must carry members");

        // Every member is under an INTERNAL `x-gmeow-*/` tag (English carrier plus
        // any translation language) — exactly the `{tag}/` prefix
        // `_unpack_doc_archive` filters on, NOT the carrier key (`english/`).
        assert!(
            members.iter().all(|(n, _)| n.starts_with("x-gmeow-")),
            "every member must carry an internal-tag prefix, got e.g. {:?}",
            members.iter().map(|(n, _)| n).take(3).collect::<Vec<_>>()
        );
        assert!(
            members
                .iter()
                .any(|(n, _)| n == "x-gmeow-english/index.html"),
            "the English landing page must be present"
        );
        let report = members
            .iter()
            .find(|(n, _)| n == "x-gmeow-english/slice-quality/index.html")
            .expect("slice-quality HTML report must be embedded in English docs");
        assert_eq!(
            report.1.as_slice(),
            slice_quality_html,
            "slice-quality report bytes must ride unchanged in the docs archive"
        );
        // The site carries its structural assets (deterministic, language-keyed).
        for asset in [
            "assets/gmeow.css",
            "search-index.json",
            "llms.txt",
            "llms-full.txt",
        ] {
            let want = format!("x-gmeow-english/{asset}");
            assert!(
                members.iter().any(|(n, _)| n == &want),
                "expected site asset {want}"
            );
        }
        // The per-term card surface: at least one `card.md` file must
        // be present in the archive under the English carrier tag.
        let card_md_present = members
            .iter()
            .any(|(n, _)| n.starts_with("x-gmeow-english/terms/") && n.ends_with("/card.md"));
        assert!(
            card_md_present,
            "expected at least one x-gmeow-english/terms/<slug>/card.md in the docs archive"
        );
        // Member names CAN exceed the 100-byte USTAR field (LongLink-covered).
        // Today's longest stays under it, so LongLink is a defensive net rather
        // than currently-triggered — `long_member_name_round_trips_via_longlink`
        // is the dedicated proof. The longest member name must stay a valid tar
        // member (non-empty archive), asserted rather than merely logged.
        let max_len = members.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
        assert!(
            max_len > 0,
            "the docs archive must carry at least one named member"
        );
    }

    /// The four axiom projections + validation-shapes, mirrored off the committed tree — the
    /// stand-in for the `stage-compile-logic` product in blob-archive unit tests.
    fn axiom_artifacts_from_disk(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut axiom_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            axiom_artifacts.insert(
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}")),
            );
        }
        let vs_rel = crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH;
        axiom_artifacts.insert(
            vs_rel.to_string(),
            std::fs::read(root.join(vs_rel)).unwrap_or_else(|_| panic!("read {vs_rel}")),
        );
        // The procedural-constraints.ttl product is required (fail-closed) for the same reason:
        // mirror the committed header-only file, as the production stage-compile-logic emits it.
        let pc_rel = crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH;
        axiom_artifacts.insert(
            pc_rel.to_string(),
            std::fs::read(root.join(pc_rel)).unwrap_or_else(|_| panic!("read {pc_rel}")),
        );
        axiom_artifacts
    }

    // DOCUMENTED SWEEP — the four single-file generated edit kinds each reach the bundle
    // product-sourced in ONE fold, so a single `regenerate` is a fixed point for
    // `check-generated` regardless of which one was edited:
    //   - generated query    → stage-mappings product          → REP_QUERIES  (members_basename_from_artifacts)
    //   - generated SSSOM map → stage-mappings product          → REP_MAPPINGS (members_basename_from_artifacts)
    //   - frame-shape source  → stage-export-frame-shapes prod  → REP_SHAPES   (ShapeSurfaces.frame)
    //   - competency test     → result-shapes projection        → stage-export-result-shapes prod → REP_SHAPES (ShapeSurfaces.result)
    // Shared invariant proven by the probes below: every archived `generated/` member is
    // sourced from an in-memory stage PRODUCT, never a disk read. Were any fold still a
    // `list_files(generated/…)` disk read (the stale-disk-fold bug), a product-only probe
    // could never reach the bundle and `regenerate`/`check-generated` would disagree forever.
    /// FIXED-POINT PROOF: a change to the `stage-mappings`
    /// product's generated SPARQL surface reaches the bundle in ONE fold. REP_QUERIES is
    /// product-sourced (`members_basename_from_artifacts`), not a disk read, so a query that
    /// exists ONLY in the in-memory product — never on disk — MUST appear in the archive. Were
    /// the fold still a `list_files(generated/queries)` disk read (the stale-disk-fold bug),
    /// the product-only probe could never reach the bundle and `regenerate`/`check-generated`
    /// would disagree forever. This encodes the "edit a generated query → one-pass fixed point"
    /// property directly at the fold, complementing the structural repo-static guard.
    #[test]
    fn a_query_present_only_in_the_mappings_product_reaches_the_bundle_in_one_fold() {
        let root = repo_root();
        let axiom_artifacts = axiom_artifacts_from_disk(&root);
        let shapes = ShapeSurfaces {
            result: &fresh_result_shapes_from_disk(&root),
            frame: &fresh_frame_shapes_from_disk(&root),
            constraint: &fresh_constraint_shapes_from_disk(&root),
        };

        // A probe query that exists ONLY in the product — it is NOT committed under
        // generated/queries/, so a disk read could never surface it.
        const PROBE_NAME: &str = "zzz-fixed-point-probe.rq";
        let probe_rel = format!("generated/queries/{PROBE_NAME}");
        assert!(
            !root.join(&probe_rel).exists(),
            "the probe must not exist on disk, or the test proves nothing"
        );
        let probe_bytes = b"# fixed-point probe: product-only generated query\n".to_vec();

        let mut mappings = mappings_artifacts_from_disk(&root);
        mappings.insert(probe_rel.clone(), probe_bytes.clone());

        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings,
            &shapes,
            &sample_models_python(),
        )
        .expect("archive blobs");
        let queries = blobs
            .iter()
            .find(|b| b.rep == REP_QUERIES)
            .expect("REP_QUERIES blob present");
        let members = parse(&queries.data);
        let probe = members
            .iter()
            .find(|(n, _)| n == PROBE_NAME)
            .expect("product-only probe query MUST reach REP_QUERIES (fold is product-sourced)");
        assert_eq!(
            probe.1, probe_bytes,
            "the folded probe bytes must be the product bytes, not a disk read"
        );

        // Fail-closed: an empty query surface in the product is a hard error, never a silent
        // fallback to a stale disk read.
        let mut no_queries = mappings_artifacts_from_disk(&root);
        no_queries.retain(|k, _| !k.starts_with("generated/queries/"));
        let err = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &no_queries,
            &shapes,
            &sample_models_python(),
        )
        .expect_err("empty queries product must fail closed");
        assert!(
            format!("{err:?}").contains("queries archive would fold empty"),
            "unexpected error: {err:?}"
        );
    }

    /// FIXED-POINT PROOF: a change to the `stage-mappings` product's generated SSSOM
    /// surface reaches the bundle in ONE fold. REP_MAPPINGS is product-sourced
    /// (`members_basename_from_artifacts`), an exact mirror of REP_QUERIES, so a mapping
    /// that exists ONLY in the in-memory product — never on disk — MUST appear in the
    /// archive. A stale disk read would leave the product-only probe stranded and make
    /// `regenerate`/`check-generated` disagree forever.
    #[test]
    fn a_mapping_present_only_in_the_stage_mappings_product_reaches_the_bundle_in_one_fold() {
        let root = repo_root();
        let axiom_artifacts = axiom_artifacts_from_disk(&root);
        let shapes = ShapeSurfaces {
            result: &fresh_result_shapes_from_disk(&root),
            frame: &fresh_frame_shapes_from_disk(&root),
            constraint: &fresh_constraint_shapes_from_disk(&root),
        };

        // A probe mapping that exists ONLY in the product — it is NOT committed under
        // generated/mappings/, so a disk read could never surface it.
        const PROBE_NAME: &str = "zzz-fixed-point-probe.sssom.tsv";
        let probe_rel = format!("generated/mappings/{PROBE_NAME}");
        assert!(
            !root.join(&probe_rel).exists(),
            "the probe must not exist on disk, or the test proves nothing"
        );
        let probe_bytes = b"# fixed-point probe: product-only SSSOM mapping\n".to_vec();

        let mut mappings = mappings_artifacts_from_disk(&root);
        mappings.insert(probe_rel.clone(), probe_bytes.clone());

        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings,
            &shapes,
            &sample_models_python(),
        )
        .expect("archive blobs");
        let archive = blobs
            .iter()
            .find(|b| b.rep == REP_MAPPINGS)
            .expect("REP_MAPPINGS blob present");
        let members = parse(&archive.data);
        let probe = members
            .iter()
            .find(|(n, _)| n == PROBE_NAME)
            .expect("product-only probe mapping MUST reach REP_MAPPINGS (fold is product-sourced)");
        assert_eq!(
            probe.1, probe_bytes,
            "the folded probe bytes must be the product bytes, not a disk read"
        );

        // Fail-closed: an empty mappings surface in the product is a hard error, never a
        // silent fallback to a stale disk read.
        let mut no_mappings = mappings_artifacts_from_disk(&root);
        no_mappings.retain(|k, _| !k.starts_with("generated/mappings/"));
        let err = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &no_mappings,
            &shapes,
            &sample_models_python(),
        )
        .expect_err("empty mappings product must fail closed");
        assert!(
            format!("{err:?}").contains("mappings archive would fold empty"),
            "unexpected error: {err:?}"
        );
    }

    /// FIXED-POINT PROOF: the frame-shape source, the competency test (which flows through
    /// the result-shapes ResultShape projection), and the constraint-shape source each reach
    /// the bundle in ONE fold. REP_SHAPES folds the `ShapeSurfaces { result, frame,
    /// constraint }` product BYTES — never a disk read — into members named by the full
    /// repo-relative projection paths, so a product-only surface that differs from the
    /// committed file MUST appear verbatim in the archive.
    #[test]
    fn product_only_shape_surfaces_reach_the_bundle_in_one_fold() {
        let root = repo_root();

        // Three distinct product-only surfaces, each differing from its committed file, so a
        // match in the archive proves the fold used the PRODUCT bytes, not a disk read.
        let result_probe = b"# fixed-point probe: product-only result-shapes surface\n".to_vec();
        let frame_probe = b"# fixed-point probe: product-only frame-shapes surface\n".to_vec();
        let constraint_probe =
            b"# fixed-point probe: product-only constraint-shapes surface\n".to_vec();
        assert_ne!(
            result_probe,
            fresh_result_shapes_from_disk(&root),
            "the result probe must differ from disk, or the test proves nothing"
        );
        assert_ne!(
            frame_probe,
            fresh_frame_shapes_from_disk(&root),
            "the frame probe must differ from disk, or the test proves nothing"
        );
        assert_ne!(
            constraint_probe,
            fresh_constraint_shapes_from_disk(&root),
            "the constraint probe must differ from disk, or the test proves nothing"
        );

        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts_from_disk(&root),
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &result_probe,
                frame: &frame_probe,
                constraint: &constraint_probe,
            },
            &sample_models_python(),
        )
        .expect("archive blobs");
        let archive = blobs
            .iter()
            .find(|b| b.rep == REP_SHAPES)
            .expect("REP_SHAPES blob present");
        let members = parse(&archive.data);

        for (path, probe) in [
            (
                crate::stages::result_shapes::RESULT_SHAPES_PATH,
                &result_probe,
            ),
            (crate::stages::frame_shapes::FRAME_SHAPES_PATH, &frame_probe),
            (
                crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
                &constraint_probe,
            ),
        ] {
            let member = members.iter().find(|(n, _)| n == path).unwrap_or_else(|| {
                panic!(
                    "a product-only shape surface MUST reach REP_SHAPES from the product, \
                     not a disk read: {path}"
                )
            });
            assert_eq!(
                &member.1, probe,
                "the folded {path} bytes must be the product bytes, not a disk read"
            );
        }
    }

    #[test]
    fn build_archive_blobs_folds_the_shapes_surface() {
        let root = repo_root();
        // schema/openapi bytes are irrelevant to the shapes blob; pass empty. The axiom
        // surface is irrelevant here too, but it must be present (fail-closed), so mirror
        // the committed projections into the artifact map.
        let mut axiom_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            axiom_artifacts.insert(
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}")),
            );
        }
        // The validation-shapes.ttl product is required (fail-closed): mirror the committed
        // file, as the production stage-compile-logic always emits it.
        let vs_rel = crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH;
        axiom_artifacts.insert(
            vs_rel.to_string(),
            std::fs::read(root.join(vs_rel)).unwrap_or_else(|_| panic!("read {vs_rel}")),
        );
        // The procedural-constraints.ttl product is required (fail-closed) for the same reason:
        // mirror the committed header-only file, as the production stage-compile-logic emits it.
        let pc_rel = crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH;
        axiom_artifacts.insert(
            pc_rel.to_string(),
            std::fs::read(root.join(pc_rel)).unwrap_or_else(|_| panic!("read {pc_rel}")),
        );
        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &fresh_result_shapes_from_disk(&root),
                frame: &fresh_frame_shapes_from_disk(&root),
                constraint: &fresh_constraint_shapes_from_disk(&root),
            },
            &sample_models_python(),
        )
        .expect("archive blobs");
        let blob = blobs
            .iter()
            .find(|b| b.rep == REP_SHAPES)
            .expect("REP_SHAPES blob present");
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the shape surface must carry members");
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();

        // Base hand-authored shape + the generated frame shape (P11) + ≥1 per-slice.
        assert!(names.contains(&"shapes/gmeow-shapes.ttl"));
        assert!(names.contains(&"generated/shapes/frame-shapes.ttl"));
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("slices/") && n.ends_with("/shapes.ttl")),
            "at least one per-slice shapes.ttl must be folded"
        );
        // The FULL surface carries the 4 DSL/manifest lints (the validator filters
        // them OUT of its data-graph union, but the consumer's DSL phases need them).
        for dsl in [
            "shapes/mapping-dsl-shapes.ttl",
            "shapes/statement-dsl-shapes.ttl",
            "shapes/test-dsl-shapes.ttl",
            "shapes/slice-manifest-shapes.ttl",
        ] {
            assert!(
                names.contains(&dsl),
                "DSL lint {dsl} must be in the FULL shape surface"
            );
        }
        // Member count == on-disk count (no silent drops).
        let on_disk = list_files(&root.join("shapes"), "ttl").unwrap().len()
            + list_files(&root.join("generated/shapes"), "ttl")
                .unwrap()
                .len()
            + slice_named_files(&root, "shapes.ttl").unwrap().len();
        assert_eq!(
            members.len(),
            on_disk,
            "every shape file must be folded exactly once"
        );
        // The slice-shape subset matches an independent on-disk enumeration — pins
        // `slice_named_files` against drift from the shacl crate's private walk.
        let folded_slices: std::collections::BTreeSet<&str> = names
            .iter()
            .copied()
            .filter(|n| n.starts_with("slices/") && n.ends_with("/shapes.ttl"))
            .collect();
        let disk_slices: std::collections::BTreeSet<String> =
            slice_named_files(&root, "shapes.ttl")
                .unwrap()
                .iter()
                .map(|p| {
                    p.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
        let disk_slices_ref: std::collections::BTreeSet<&str> =
            disk_slices.iter().map(String::as_str).collect();
        assert_eq!(folded_slices, disk_slices_ref);
        // Keys sorted (deterministic fold).
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "shape members must be sorted by key");
    }

    #[test]
    fn build_archive_blobs_folds_the_axiom_surface() {
        let root = repo_root();
        // The axiom surface is now sourced from the stage-compile-logic product; mirror
        // that here by reading the committed projections into the artifact map.
        let mut axiom_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            axiom_artifacts.insert(
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}")),
            );
        }
        // The validation-shapes.ttl product is required (fail-closed): mirror the committed
        // file, as the production stage-compile-logic always emits it.
        let vs_rel = crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH;
        axiom_artifacts.insert(
            vs_rel.to_string(),
            std::fs::read(root.join(vs_rel)).unwrap_or_else(|_| panic!("read {vs_rel}")),
        );
        // The procedural-constraints.ttl product is required (fail-closed) for the same reason:
        // mirror the committed header-only file, as the production stage-compile-logic emits it.
        let pc_rel = crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH;
        axiom_artifacts.insert(
            pc_rel.to_string(),
            std::fs::read(root.join(pc_rel)).unwrap_or_else(|_| panic!("read {pc_rel}")),
        );
        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &fresh_result_shapes_from_disk(&root),
                frame: &fresh_frame_shapes_from_disk(&root),
                constraint: &fresh_constraint_shapes_from_disk(&root),
            },
            &sample_models_python(),
        )
        .expect("archive blobs");
        let blob = blobs
            .iter()
            .find(|b| b.rep == REP_AXIOMS)
            .expect("REP_AXIOMS blob present");
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        let names: std::collections::BTreeSet<&str> =
            members.iter().map(|(n, _)| n.as_str()).collect();
        // Exactly the 5 compiled projections — no more, no less.
        let want: std::collections::BTreeSet<&str> = AXIOM_FILES.iter().copied().collect();
        assert_eq!(
            names, want,
            "REP_AXIOMS must carry exactly the 5 projection files"
        );
        // The big reasoning OUTPUTS ride other channels — never in REP_AXIOMS.
        for big in [
            "generated/logic/inferred-closure.rdf12.ttl",
            "generated/logic/reasoning-explanations.rdf12.ttl",
            "generated/logic/dl-el-crosscheck-report.ttl",
        ] {
            assert!(!names.contains(big), "{big} must NOT be in REP_AXIOMS");
        }
        // Determinism: rebuild and assert byte-equality.
        let again = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &fresh_result_shapes_from_disk(&root),
                frame: &fresh_frame_shapes_from_disk(&root),
                constraint: &fresh_constraint_shapes_from_disk(&root),
            },
            &sample_models_python(),
        )
        .expect("archive blobs");
        let blob2 = again.iter().find(|b| b.rep == REP_AXIOMS).unwrap();
        assert_eq!(
            blob.data, blob2.data,
            "REP_AXIOMS must be byte-deterministic"
        );
    }

    #[test]
    fn build_reasoning_blob_folds_the_report_artifacts() {
        // Construct a fake stage-reason product with the two report artifacts (avoids
        // running the reasoner); proves the wiring (rep, keys, fail-closed).
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            b"# explanations".to_vec(),
        );
        artifacts.insert(
            crate::stages::reason::LEDGER_PATH.to_string(),
            b"# ledger".to_vec(),
        );
        artifacts.insert(
            crate::stages::reason::PERF_LEDGER_PATH.to_string(),
            b"# perf ledger".to_vec(),
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", artifacts),
        );
        let blob = build_reasoning_blob(&upstream).expect("reasoning blob");
        assert_eq!(blob.rep, REP_REASONING);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        let names: std::collections::BTreeSet<&str> =
            members.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "reason/dl-el-crosscheck-report.ttl",
                "reason/perf-ledger.ttl",
                "reason/reasoning-explanations.rdf12.ttl"
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<&str>>(),
            "REP_REASONING carries the report artifacts under bundle-relative keys"
        );
        // Missing artifact HARD-fails (no-optionality, fail-closed).
        let empty: BTreeMap<String, StageProduct> = BTreeMap::new();
        assert!(
            build_reasoning_blob(&empty).is_err(),
            "a missing stage-reason product must fail closed"
        );
    }

    #[test]
    fn build_okf_archive_packs_the_rust_rendered_bundle() {
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).expect("committed gts");
        let dataset = purrdf::import_gts_events(&gts)
            .expect("import committed gts")
            .dataset;
        let blob = build_okf_blob_from_dataset(dataset.as_ref()).expect("okf archive");
        assert_eq!(blob.rep, REP_OKF);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the OKF archive must carry members");
        assert!(
            members.iter().all(|(n, _)| n.starts_with("gmeow-okf/")),
            "every OKF member must be bundle-relative under gmeow-okf/, got e.g. {:?}",
            members.iter().map(|(n, _)| n).take(3).collect::<Vec<_>>()
        );
        assert!(
            members.iter().any(|(n, _)| n == "gmeow-okf/index.md"),
            "root OKF index must be present"
        );
        for required_dir in ["classes", "properties", "individuals"] {
            let prefix = format!("gmeow-okf/{required_dir}/");
            assert!(
                members
                    .iter()
                    .any(|(n, _)| n.starts_with(&prefix) && !n.ends_with("/index.md")),
                "expected at least one OKF document under {prefix}"
            );
        }
        let root_index = members
            .iter()
            .find(|(n, _)| n == "gmeow-okf/index.md")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .expect("root index bytes");
        assert!(
            root_index.contains("LOSSY projection"),
            "root OKF index must declare projection loss"
        );

        let blob2 = build_okf_blob_from_dataset(dataset.as_ref()).expect("second okf archive");
        assert_eq!(blob.data, blob2.data, "OKF archive must be deterministic");
    }

    #[test]
    fn okf_docs_cover_every_documented_term_on_the_committed_ontology() {
        // Happy path: the real committed ontology must not ship a single dangling
        // OKF link — every documented class/property/individual term the docs site
        // would link to has a corresponding document in the OKF projection.
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).expect("committed gts");
        let dataset = purrdf::import_gts_events(&gts)
            .expect("import committed gts")
            .dataset;
        let model = gmeow_docs::model::DocsModel::discover(&root).expect("docs model");
        assert_okf_docs_cover_documented_terms(dataset.as_ref(), &model)
            .expect("committed ontology must not have dangling OKF links");
    }

    #[test]
    fn okf_link_targets_missing_from_flags_only_the_absent_target() {
        // Pure-logic test of the hard-fail comparison itself: prove it does not
        // silently accept a link whose target the OKF bundle never emits, and does
        // not false-positive on a link whose target IS emitted.
        let emitted: std::collections::BTreeSet<String> =
            ["classes/Present.md".to_string()].into_iter().collect();
        let links = vec![
            Some("gmeow-okf/classes/Present.md".to_string()),
            Some("gmeow-okf/classes/Absent.md".to_string()),
            None, // e.g. a Datatype/Other term the OKF bundle deliberately skips
        ];
        let missing = okf_link_targets_missing_from(&emitted, &links);
        assert_eq!(
            missing,
            vec![1],
            "only the link whose target is absent from the emitted set must be flagged"
        );
    }

    #[test]
    fn header_checksum_is_valid() {
        // Build a minimal archive and inspect the first 512-byte header.
        let members = vec![("x-gmeow-english/index.html".to_string(), vec![0u8; 42])];
        let raw = purrdf::ustar::write_archive(&members).expect("archive");
        let h: &[u8] = &raw[..512];
        // The stored checksum equals the sum of all bytes with the checksum field
        // taken as spaces — the canonical USTAR self-check.
        let stored = usize::from_str_radix(
            std::str::from_utf8(&h[148..154])
                .unwrap()
                .trim_matches('\0')
                .trim(),
            8,
        )
        .unwrap();
        let mut probe = [0u8; 512];
        probe.copy_from_slice(h);
        for b in &mut probe[148..156] {
            *b = b' ';
        }
        let computed: usize = probe.iter().map(|&b| b as usize).sum();
        assert_eq!(stored, computed);
    }

    // ── docs-book / docs-print blob wiring (fresh-build, no committed-bundle dep) ──

    /// A small, deterministic docs model (one slice, three terms, one competency, one
    /// linkage) — the SAME shape the `docs-print` integration suite uses. It stays
    /// small so unit tests isolate the renderer; full-catalog render/compile belongs
    /// to the regenerate gate.
    fn small_docs_model() -> gmeow_docs::model::DocsModel {
        use gmeow_docs::model::{
            DocCompetency, DocLinkage, DocSlice, DocTerm, DocTermCategory, DocsModel,
            ReasoningVerdict,
        };
        let slice_iri = "https://blackcatinformatics.ca/gmeow/slice/demo".to_string();
        let mk = |iri: &str, curie: &str, label: &str, def: &str, cat: DocTermCategory| DocTerm {
            iri: iri.to_string(),
            curie: curie.to_string(),
            label: Some(label.to_string()),
            definition: Some(def.to_string()),
            category: cat,
            owner_slice: slice_iri.clone(),
            ..Default::default()
        };
        let demo_slice = DocSlice {
            iri: slice_iri.clone(),
            label: Some("Demo".to_string()),
            title: Some("Demo slice".to_string()),
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
            artifacts: Vec::new(),
            has_thesis_sentence: false,
            realized_state_complete: false,
        };
        let competency = DocCompetency {
            iri: "https://blackcatinformatics.ca/gmeow/cq/demo".to_string(),
            rationale: Some("Can a demo Foo be found?".to_string()),
            query_file: Some("demo.rq".to_string()),
            exercises: vec!["https://blackcatinformatics.ca/gmeow/Foo".to_string()],
            owner_slice: slice_iri.clone(),
            ..Default::default()
        };
        let linkage = DocLinkage {
            mapping_set: None,
            subject: "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
            subject_curie: "gmeow:Foo".to_string(),
            predicate: "http://www.w3.org/2004/02/skos/core#closeMatch".to_string(),
            object: "http://purl.org/nemo/gufo#Object".to_string(),
            justification: None,
            confidence: Some(0.9),
            owner_slice: slice_iri.clone(),
        };
        DocsModel {
            title: "GMEOW Demo Documentation".to_string(),
            version: "test-1".to_string(),
            slices: vec![demo_slice],
            terms: vec![
                mk(
                    "https://blackcatinformatics.ca/gmeow/Foo",
                    "gmeow:Foo",
                    "Foo",
                    "A foundational demonstration class.",
                    DocTermCategory::Class,
                ),
                mk(
                    "https://blackcatinformatics.ca/gmeow/hasValue",
                    "gmeow:hasValue",
                    "hasValue",
                    "Relates a Foo to a value.",
                    DocTermCategory::Property,
                ),
                mk(
                    "https://blackcatinformatics.ca/gmeow/Baz",
                    "gmeow:Baz",
                    "Baz",
                    "An individual of the demo.",
                    DocTermCategory::Individual,
                ),
            ],
            competencies: vec![competency],
            linkages: vec![linkage],
            reasoning: Some(ReasoningVerdict {
                is_consistent: true,
                unsatisfiable: Default::default(),
            }),
            ..Default::default()
        }
    }

    /// A minimal valid BibTeX database, the stand-in for the `stage-export-references`
    /// product's `references.bib` in the print-blob tests.
    fn fixture_bib() -> Vec<u8> {
        b"@article{gmeow2026,\n  title = {The GMEOW Ontology},\n  author = {Audley, Patrick},\n  year = {2026},\n  journal = {Journal of Ontology},\n}\n".to_vec()
    }

    /// A synthetic upstream product map carrying the two products `build_docs_print_blob`
    /// reads: `stage-export-references` (the bibliography) and `stage-compile-logic` (the
    /// axiom listings). Each axiom file carries small synthetic bytes — the PDF lists them
    /// verbatim, so their content need not be the real projection for a wiring test.
    fn print_upstream() -> BTreeMap<String, StageProduct> {
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let mut refs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        refs.insert(
            crate::stages::references::BIB_PATH.to_string(),
            fixture_bib(),
        );
        upstream.insert(
            "stage-export-references".to_string(),
            StageProduct::from_artifacts("stage-export-references", refs),
        );
        let mut logic: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            logic.insert(
                rel.to_string(),
                format!("% axiom listing for {rel}\n").into_bytes(),
            );
        }
        upstream.insert(
            "stage-compile-logic".to_string(),
            StageProduct::from_artifacts("stage-compile-logic", logic),
        );
        upstream
    }

    #[test]
    fn build_docs_book_archive_packs_the_mdbook_tree() {
        let root = repo_root();
        let model = small_docs_model();
        let exec = gmeow_docs::ExecutableDocsData::default();

        let blob = build_docs_book_archive(&root, &model, &exec).expect("docs-book archive");
        assert_eq!(blob.rep, REP_DOCS_BOOK);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

        let members = parse(&blob.data);
        // Every member rides under the English internal tag, and the two mdbook anchor
        // files are present.
        assert!(
            members
                .iter()
                .all(|(n, _)| n.starts_with("x-gmeow-english/")),
            "every book member must carry the English internal-tag prefix, got e.g. {:?}",
            members.iter().map(|(n, _)| n).take(3).collect::<Vec<_>>()
        );
        assert!(
            members
                .iter()
                .any(|(n, _)| n == "x-gmeow-english/book.toml"),
            "the mdbook book.toml must be present"
        );
        assert!(
            members
                .iter()
                .any(|(n, _)| n == "x-gmeow-english/src/SUMMARY.md"),
            "the mdbook SUMMARY.md must be present"
        );

        // Byte-stability: a second build folds byte-identical archive bytes.
        let again = build_docs_book_archive(&root, &model, &exec).expect("docs-book archive again");
        assert_eq!(
            blob.data, again.data,
            "the docs-book archive must be byte-deterministic"
        );
    }

    #[test]
    fn build_docs_print_blob_packs_pdf_and_typ() {
        let model = small_docs_model();
        let upstream = print_upstream();

        let (blob, pdf_digest) = build_docs_print_blob(&model, &upstream).expect("docs-print blob");
        assert_eq!(blob.rep, REP_DOCS_PRINT);
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

        let members: BTreeMap<String, Vec<u8>> = parse(&blob.data).into_iter().collect();
        let pdf = members
            .get("x-gmeow-english/gmeow.pdf")
            .expect("the print PDF member must be present");
        assert!(
            pdf.starts_with(b"%PDF"),
            "the print member must be a real PDF (starts with %PDF)"
        );
        assert_eq!(
            pdf_digest,
            purrdf::gts::writer::digest_string(pdf),
            "the returned pdf digest must be the raw PDF's blake3, not the archive's"
        );
        let typ = members
            .get("x-gmeow-english/gmeow.typ")
            .expect("the Typst source member must be present");
        assert!(
            !typ.is_empty(),
            "the Typst source member must carry the rendered source"
        );

        // Byte-stability: a second build folds byte-identical archive bytes (the Typst
        // source is pure and the PDF compile is byte-reproducible).
        let (again, again_digest) =
            build_docs_print_blob(&model, &upstream).expect("docs-print blob again");
        assert_eq!(
            blob.data, again.data,
            "the docs-print archive must be byte-deterministic"
        );
        assert_eq!(
            pdf_digest, again_digest,
            "the raw pdf digest must be byte-deterministic too"
        );
    }

    /// The `application/pdf` attestation the SHIPPED bundle carries (F4) must bind the
    /// RAW `gmeow.pdf` bytes, not the tar that packs them. Recompute the raw PDF blake3
    /// straight from the docs-print blob (untar, find the `gmeow.pdf` member, digest it)
    /// and assert it EQUALS the `gmeow:contentDigest` the docs-format corpus emits on the
    /// `application/pdf` attestation artifact — proving the binding is real and non-DARK
    /// on the committed-bundle production path (the exact path `make regenerate` runs).
    #[test]
    fn shipped_pdf_attestation_binds_the_raw_pdf_bytes() {
        let model = small_docs_model();
        let upstream = print_upstream();

        // The producer path: the same blob + raw-PDF digest the carrier threads.
        let (print_blob, print_pdf_digest) =
            build_docs_print_blob(&model, &upstream).expect("docs-print blob");

        // The consumer path: untar the shipped blob, find gmeow.pdf, digest the RAW bytes.
        let members: BTreeMap<String, Vec<u8>> = parse(&print_blob.data).into_iter().collect();
        let pdf = members
            .get("x-gmeow-english/gmeow.pdf")
            .expect("the docs-print blob must carry gmeow.pdf");
        let recomputed = purrdf::gts::writer::digest_string(pdf);
        assert_eq!(
            recomputed, print_pdf_digest,
            "the threaded raw-PDF digest must equal the blake3 of the shipped gmeow.pdf"
        );

        // The corpus emits that digest on an application/pdf AttestationArtifact — the
        // literal that lands in the committed bundle. HARD-FAIL if the binding drifts.
        let corpus = crate::stages::docs_format_rendering::build_docs_format_corpus(
            "blake3:0000000000000000000000000000000000000000000000000000000000000000",
            "blake3:1111111111111111111111111111111111111111111111111111111111111111",
            &print_pdf_digest,
        );
        let nt = String::from_utf8(corpus.ntriples).expect("utf8 n-triples");
        let pdf_blob = "http://example.org/docs-format/blob/docs-print-pdf";
        let media = format!(
            "<{pdf_blob}> <https://blackcatinformatics.ca/gmeow/artifactMediaType> \"application/pdf\" ."
        );
        let digest = format!(
            "<{pdf_blob}> <https://blackcatinformatics.ca/gmeow/contentDigest> \"{recomputed}\" ."
        );
        assert!(
            nt.contains(&media),
            "the corpus must mint an application/pdf attestation artifact"
        );
        assert!(
            nt.contains(&digest),
            "the application/pdf attestation must carry the RAW gmeow.pdf blake3, got:\n{nt}"
        );
    }

    #[test]
    fn docs_book_and_print_resolve_via_bundle_round_trip() {
        let root = repo_root();
        let model = small_docs_model();
        let exec = gmeow_docs::ExecutableDocsData::default();
        let upstream = print_upstream();

        let book_blob = build_docs_book_archive(&root, &model, &exec).expect("docs-book archive");
        let (print_blob, _print_pdf_digest) =
            build_docs_print_blob(&model, &upstream).expect("docs-print blob");

        // Fold a minimal snapshot carrying exactly the two new blobs (plus a well-formed
        // base graph) through the SAME emit path the carrier uses, then read them back
        // through the repo-free `Bundle` reader — the producer↔reader wiring end-to-end.
        let mut builder = SnapshotBuilder::new();
        add_base_nq(
            &mut builder,
            b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
            "base",
        )
        .expect("fold base graph");
        let gts = emit_gts(
            &builder,
            "dist",
            Some(vec!["zstd-rsyncable".to_string()]),
            vec![book_blob, print_blob],
            Vec::new(),
            None,
            None,
            None,
            purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit snapshot");

        let bundle =
            crate::bundle_blobs::Bundle::from_snapshot(&gts).expect("fold the minimal snapshot");
        let book = bundle.docs_book().expect("docs_book resolves");
        assert!(
            book.contains_key("x-gmeow-english/book.toml")
                && book.contains_key("x-gmeow-english/src/SUMMARY.md"),
            "docs_book() must resolve the mdbook anchor members; got {:?}",
            book.keys().take(4).collect::<Vec<_>>()
        );
        let print = bundle.docs_print().expect("docs_print resolves");
        assert!(
            print
                .get("x-gmeow-english/gmeow.pdf")
                .is_some_and(|b| b.starts_with(b"%PDF")),
            "docs_print() must resolve the PDF member as a real PDF"
        );
        assert!(
            print.contains_key("x-gmeow-english/gmeow.typ"),
            "docs_print() must resolve the Typst source member"
        );
    }
}

#[cfg(test)]
mod conformance_fold_tests {
    use super::*;

    /// Read every named-graph IRI present in a folded snapshot's quad table.
    fn folded_graph_names(gts: &[u8]) -> std::collections::BTreeSet<String> {
        let g = purrdf::gts::read_graph(gts, true).expect("read_graph");
        let mut names = std::collections::BTreeSet::new();
        for &(_, _, _, gname) in &g.quads {
            if let Some(gid) = gname
                && let Some(value) = g.terms.get(gid).and_then(|t| t.value.clone())
            {
                names.insert(value);
            }
        }
        names
    }

    /// A synthetic divergence Finding folds into the `graph/conformance` named
    /// graph of the emitted snapshot — the C3 fold contract. Constructed
    /// independently of the (currently all-agree) committed corpus so the assertion
    /// holds regardless of whether a real divergence exists today.
    #[test]
    fn synthetic_divergence_lands_in_graph_conformance() {
        // One CorpusOnly + one DlGap divergence, projected to gmeow:Finding N-Quads
        // in the conformance graph by the shared emitter.
        let conformance = gmeow_conformance::divergence::emit_divergence_nq(
            "w3c-owl2-el",
            &[
                gmeow_logic::reason::ExternalComparison {
                    case: "clash".to_owned(),
                    world: "https://gmeow.example/w3c-owl2-el/clash/w".to_owned(),
                    native: "consistent".to_owned(),
                    published: "inconsistent".to_owned(),
                },
                gmeow_logic::reason::ExternalComparison {
                    case: "beyond-el".to_owned(),
                    world: "https://gmeow.example/w3c-owl2-el/beyond-el/w".to_owned(),
                    native: "incomplete".to_owned(),
                    published: "consistent".to_owned(),
                },
            ],
        );
        assert!(
            !conformance.is_empty(),
            "the synthetic divergence must emit Findings"
        );

        // Fold it through the SAME add_named path the snapshot serialization uses, emit,
        // and read the bundle back.
        let mut builder = SnapshotBuilder::new();
        // A non-empty default graph so the bundle is well-formed.
        add_base_nq(
            &mut builder,
            b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
            "base",
        )
        .expect("fold base graph");
        add_named(
            &mut builder,
            conformance.as_bytes(),
            GRAPH_CONFORMANCE,
            "conformance",
        )
        .expect("fold conformance graph");

        let gts = emit_gts(
            &builder,
            "dist",
            Some(vec!["gzip".to_string()]),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit snapshot");

        let names = folded_graph_names(&gts);
        assert!(
            names.contains(GRAPH_CONFORMANCE),
            "the folded snapshot must carry the graph/conformance named graph; got {names:?}"
        );
    }

    /// An empty divergence (the all-agree corpus) is skipped — folding empty bytes
    /// must NOT add a phantom `graph/conformance` slot.
    #[test]
    fn empty_divergence_adds_no_conformance_graph() {
        let mut builder = SnapshotBuilder::new();
        add_base_nq(
            &mut builder,
            b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
            "base",
        )
        .expect("fold base graph");
        // Mirror the snapshot serialization guard: an empty graph is never add_named'd.
        let conformance: Vec<u8> = Vec::new();
        if !conformance.is_empty() {
            add_named(&mut builder, &conformance, GRAPH_CONFORMANCE, "conformance").expect("fold");
        }

        let gts = emit_gts(
            &builder,
            "dist",
            Some(vec!["gzip".to_string()]),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit snapshot");

        assert!(
            !folded_graph_names(&gts).contains(GRAPH_CONFORMANCE),
            "an all-agree corpus must not fold a phantom graph/conformance"
        );
    }
}

#[cfg(test)]
mod validation_shape_typed_lookaside_tests {
    use super::*;
    use crate::node::StageProduct;

    /// The typed Shacl/Shex validation-shape sidecars ride the REAL gmeow.gts
    /// serialize+decode: a decoded bundle exposes the SHACL surface under
    /// [`purrdf::RdfLookasideKind::Shacl`] and the ShEx surface under
    /// [`purrdf::RdfLookasideKind::Shex`], each resolving to the exact producer bytes.
    /// This is the production-surface demonstration that a repo-free consumer reads the
    /// validation surface under its typed kind (LOGIC-VALIDATION.md) without re-running
    /// the compiler — the decode path exercised is the true `emit_gts` writer +
    /// `read_graph`/`lookaside_from_graph` reader, never a hand-rolled shortcut.
    #[test]
    fn typed_shacl_shex_sidecars_round_trip_through_gmeow_gts() {
        // A minimal stage-compile-logic product carrying the two validation-shape surfaces
        // (the SINGLE source the typed sidecars and the REP_GENERATED archive both draw from).
        let shacl_bytes = b"@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
            <https://blackcatinformatics.ca/gmeow/CatShape> a sh:NodeShape .\n"
            .to_vec();
        let shex_bytes = b"PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
            gmeow:CatShape { gmeow:name . }\n"
            .to_vec();
        let mut compile_arts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        compile_arts.insert(
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH.to_string(),
            shacl_bytes.clone(),
        );
        compile_arts.insert(
            crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH.to_string(),
            shex_bytes.clone(),
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-compile-logic".to_string(),
            StageProduct::from_artifacts("stage-compile-logic", compile_arts),
        );

        // Build the typed sidecars through the PRODUCTION helper, fold them into a
        // well-formed snapshot, and emit through the REAL gts writer (`emit_gts`).
        let typed_blobs = build_validation_shape_typed_blobs(&upstream).expect("typed sidecars");
        let mut builder = SnapshotBuilder::new();
        add_base_nq(
            &mut builder,
            b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
            "base",
        )
        .expect("fold base graph");
        let gts = emit_gts(
            &builder,
            "dist",
            Some(vec!["gzip".to_string()]),
            typed_blobs,
            Vec::new(),
            None,
            None,
            None,
            purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit snapshot");

        // DECODE the emitted bytes back through the real gts reader + lookaside fold.
        let graph = purrdf::gts::read_graph(&gts, true).expect("read_graph");
        let lookaside = purrdf::gts::lookaside_from_graph(&graph);

        // Resolve the single resource of `kind` to its payload bytes via the content-store
        // (digest → bytes) join — exactly how a repo-free consumer reads a typed surface.
        let bytes_of = |kind: purrdf::RdfLookasideKind| -> Vec<u8> {
            let resource = lookaside
                .resources_of_kind(kind.clone())
                .next()
                .unwrap_or_else(|| panic!("a decoded {kind:?} resource is present"));
            let digest = resource
                .content_digest
                .as_deref()
                .expect("typed resource carries a content digest");
            let (_, entry) = graph
                .blobs
                .iter()
                .find(|(d, _)| d == digest)
                .expect("blob store carries the resource payload by digest");
            entry.decoded_vec().expect("decode blob payload")
        };

        // The typed Shacl kind decodes to the exact SHACL surface bytes.
        assert_eq!(
            bytes_of(purrdf::RdfLookasideKind::Shacl),
            shacl_bytes,
            "resources_of_kind(Shacl) yields the validation-shapes.ttl content"
        );
        // The typed Shex kind decodes to the exact ShEx surface bytes.
        assert_eq!(
            bytes_of(purrdf::RdfLookasideKind::Shex),
            shex_bytes,
            "resources_of_kind(Shex) yields the validation-shapes.shex content"
        );
    }
}

#[cfg(test)]
mod logic_graph_golden_tests {
    use super::*;
    use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram};

    const GRAPH_LOGIC: &str = crate::stages::compile_logic::GRAPH_LOGIC;

    /// A small, FIXED clean program — its canonical RDF-1.2 projection is the byte
    /// golden subject. Deliberately synthetic (not the real module) so the golden is
    /// stable and the per-graph fold is regression-pinned independent of the full
    /// gmeow.gts and independent of any logic-module edit.
    fn fixed_program() -> LogicProgram {
        let ax = |s: &str, o: &str| {
            LogicAxiom::new(
                s,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                o,
                false,
                false,
                ContextualScope::default(),
            )
            .expect("valid axiom")
        };
        LogicProgram::new(
            vec![
                ax(
                    "https://blackcatinformatics.ca/gmeow/Animal",
                    "https://blackcatinformatics.ca/logic/Kind",
                ),
                ax(
                    "https://blackcatinformatics.ca/gmeow/Cat",
                    "https://blackcatinformatics.ca/logic/Subkind",
                ),
            ],
            vec![],
            vec![],
            None,
        )
    }

    /// Read the canonical N-Quads of one named graph from an emitted snapshot,
    /// sorted — a deterministic byte surface for the per-graph golden.
    fn folded_graph_nquads(gts: &[u8], graph_iri: &str) -> String {
        let g = purrdf::gts::read_graph(gts, true).expect("read_graph");
        let mut rows: Vec<String> = Vec::new();
        for &(s, p, o, gname) in &g.quads {
            let Some(gid) = gname else { continue };
            let in_graph = g
                .terms
                .get(gid)
                .and_then(|t| t.value.clone())
                .is_some_and(|v| v == graph_iri);
            if !in_graph {
                continue;
            }
            let term = |id: usize| -> String {
                let t = &g.terms[id];
                match t.value.clone() {
                    Some(v) if v.starts_with("http") || v.starts_with("urn:") => format!("<{v}>"),
                    Some(v) => v,
                    None => format!("_:{id}"),
                }
            };
            rows.push(format!("{} {} {} .", term(s), term(p), term(o)));
        }
        rows.sort();
        rows.join("\n")
    }

    /// Byte golden: the `graph/logic` named-graph content of an emitted
    /// snapshot, over a FIXED synthetic program. Pins the per-graph fold path
    /// (canonical RDF-1.2 → N-Quads → add_named canonicalization → emit → read-back)
    /// byte-for-byte, independent of the full gmeow.gts. A second emit is asserted
    /// byte-identical (determinism).
    #[test]
    fn graph_logic_fold_byte_golden() {
        let arts = gmeow_logic_compile::projections::compile_program(
            &fixed_program(),
            &Default::default(),
        )
        .expect("compile fixed program");
        let logic_nq = turtle_to_nquads(arts.canonical_rdf12.as_bytes()).expect("turtle → nq");

        let build = || {
            let mut builder = SnapshotBuilder::new();
            add_base_nq(
                &mut builder,
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
                "base",
            )
            .expect("fold base graph");
            add_named(&mut builder, &logic_nq, GRAPH_LOGIC, "logic").expect("fold graph/logic");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_LOGIC);
        assert!(!folded.is_empty(), "graph/logic must carry the projection");
        insta::assert_snapshot!("graph_logic_fold", folded);

        // Determinism: a second build folds the SAME graph/logic content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_LOGIC),
            folded,
            "the graph/logic fold must be byte-deterministic"
        );
    }

    const GRAPH_REASONING: &str = gmeow_logic::result_rdf::GRAPH_REASONING;

    /// A FIXED synthetic reasoning result — the byte-golden subject for the
    /// `graph/reasoning` fold (deliberately synthetic so the golden is stable and
    /// independent of any reasoner output).
    fn fixed_reasoning_result() -> gmeow_logic::result::ReasoningResult {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };
        ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            PreservationClaim::exact(),
            InformationState::Supported,
            ResultProvenance::native(
                "contract:golden",
                "https://blackcatinformatics.ca/gmeow/graph/world/actual",
            ),
            ResultPayload::Empty,
        )
    }

    /// Byte golden: the `graph/reasoning` named-graph content of an emitted
    /// snapshot, over a FIXED synthetic reasoning result. Pins the per-graph fold path
    /// (project → N-Triples → add_named canonicalization → emit → read-back)
    /// byte-for-byte, independent of the full gmeow.gts. A second emit is asserted
    /// byte-identical (determinism).
    #[test]
    fn graph_reasoning_fold_byte_golden() {
        let reasoning_nt =
            gmeow_logic::result_rdf::project_reasoning_result(&fixed_reasoning_result());

        let build = || {
            let mut builder = SnapshotBuilder::new();
            add_base_nq(
                &mut builder,
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
                "base",
            )
            .expect("fold base graph");
            add_named(
                &mut builder,
                reasoning_nt.as_bytes(),
                GRAPH_REASONING,
                "reasoning",
            )
            .expect("fold graph/reasoning");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_REASONING);
        assert!(
            !folded.is_empty(),
            "graph/reasoning must carry the projection"
        );
        insta::assert_snapshot!("graph_reasoning_fold", folded);

        // Determinism: a second build folds the SAME graph/reasoning content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_REASONING),
            folded,
            "the graph/reasoning fold must be byte-deterministic"
        );
    }

    const GRAPH_RELATIONAL_CORE: &str = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;

    /// A FIXED synthetic relational-core program — the byte-golden subject for the
    /// `graph/relational-core` fold (a clean Horn program with one rule, so the golden
    /// is stable and independent of the real module).
    fn fixed_relational_core() -> gmeow_logic_compile::relational_core::RelationalCoreProgram {
        use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
        use gmeow_logic_compile::relational_core::lower_program;
        let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        let ax = |s: &str, p: &str, o: &str| {
            LogicAxiom::new(s, p, o, false, false, ContextualScope::default()).expect("axiom")
        };
        // ?x sc ?z :- ?x sc ?y, ?y sc ?z .
        let rule = LogicRule::new(
            ax("?x", sc, "?z"),
            vec![ax("?x", sc, "?y"), ax("?y", sc, "?z")],
            vec![],
            ContextualScope::default(),
        );
        let program = LogicProgram::new(
            vec![ax(
                "https://blackcatinformatics.ca/gmeow/Cat",
                sc,
                "https://blackcatinformatics.ca/gmeow/Animal",
            )],
            vec![rule],
            vec![],
            None,
        );
        lower_program(&program)
    }

    /// Byte golden: the `graph/relational-core` named-graph content of an
    /// emitted snapshot, over a FIXED synthetic relational-core program. Pins the
    /// per-graph fold path (lower → project N-Triples → add_named canonicalization →
    /// emit → read-back) byte-for-byte, independent of the full gmeow.gts. A second emit
    /// is asserted byte-identical (determinism).
    #[test]
    fn graph_relational_core_fold_byte_golden() {
        let rc_nt =
            gmeow_logic_compile::relational_core::project_relational_core(&fixed_relational_core());

        let build = || {
            let mut builder = SnapshotBuilder::new();
            add_base_nq(
                &mut builder,
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
                "base",
            )
            .expect("fold base graph");
            add_named(
                &mut builder,
                rc_nt.as_bytes(),
                GRAPH_RELATIONAL_CORE,
                "relcore",
            )
            .expect("fold graph/relational-core");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_RELATIONAL_CORE);
        assert!(
            !folded.is_empty(),
            "graph/relational-core must carry the projection"
        );
        insta::assert_snapshot!("graph_relational_core_fold", folded);

        // Determinism: a second build folds the SAME graph/relational-core content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_RELATIONAL_CORE),
            folded,
            "the graph/relational-core fold must be byte-deterministic"
        );
    }

    const GRAPH_CORRESPONDENCE: &str = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;

    /// Byte golden: the `graph/correspondence` named-graph content of an
    /// emitted snapshot, over the §14 affine-triangle worked example. Pins the per-graph
    /// fold path (construct → project N-Triples → add_named canonicalization → emit →
    /// read-back) byte-for-byte, independent of the full gmeow.gts. Also asserts the
    /// load-bearing correctness point in the folded bytes: `skos:relatedMatch` present,
    /// `skos:exactMatch` + `owl:equivalentClass` absent, the loss-ledger row present. A
    /// second emit is asserted byte-identical (determinism).
    #[test]
    fn graph_correspondence_fold_byte_golden() {
        let corr_nt = gmeow_logic_compile::projections::correspondence::project_correspondence(
            &gmeow_logic_compile::projections::correspondence::affine_triangle_worked_example(),
        );

        let build = || {
            let mut builder = SnapshotBuilder::new();
            add_base_nq(
                &mut builder,
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
                "base",
            )
            .expect("fold base graph");
            add_named(
                &mut builder,
                corr_nt.as_bytes(),
                GRAPH_CORRESPONDENCE,
                "correspondence",
            )
            .expect("fold graph/correspondence");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_CORRESPONDENCE);
        assert!(
            !folded.is_empty(),
            "graph/correspondence must carry the projection"
        );
        // The load-bearing correctness point, asserted on the FOLDED snapshot bytes —
        // checking the alignment PREDICATE position, not bare substrings (the loss-ledger
        // prose mentions the forbidden predicate names as disclosure, not as edges).
        assert!(
            folded.contains("<http://www.w3.org/2004/02/skos/core#relatedMatch>"),
            "the folded correspondence graph keeps the overlap at skos:relatedMatch:\n{folded}"
        );
        assert!(
            !folded.contains("<http://www.w3.org/2004/02/skos/core#exactMatch>"),
            "the folded correspondence graph MUST NOT emit a skos:exactMatch edge:\n{folded}"
        );
        assert!(
            !folded.contains("<http://www.w3.org/2002/07/owl#equivalentClass>"),
            "the folded correspondence graph MUST NOT emit an owl:equivalentClass edge:\n{folded}"
        );
        assert!(
            folded.contains("lossyDrop"),
            "the folded correspondence graph MUST carry the loss-ledger row:\n{folded}"
        );
        insta::assert_snapshot!("graph_correspondence_fold", folded);

        // Determinism: a second build folds the SAME graph/correspondence content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_CORRESPONDENCE),
            folded,
            "the graph/correspondence fold must be byte-deterministic"
        );
    }

    const GRAPH_PROVENANCE: &str = crate::stages::provenance_graph::GRAPH_PROVENANCE;

    /// A FIXED synthetic provenance projection — the byte-golden subject for the
    /// `graph/provenance` fold. Three units (root / source / import) so every
    /// `OriginKind` branch is exercised; deliberately synthetic so the golden is
    /// stable and independent of the real ontology (whose unit set churns).
    fn fixed_provenance_projection() -> Vec<(usize, String, String, String, Option<String>)> {
        vec![
            (
                0,
                "imports/prov.ttl".to_string(),
                "import".to_string(),
                "imports/prov.ttl".to_string(),
                None,
            ),
            (
                1,
                "ontology/gmeow.ttl".to_string(),
                "root-ontology".to_string(),
                "ontology/gmeow.ttl".to_string(),
                None,
            ),
            (
                2,
                "slices/core/epistemics/module.ttl".to_string(),
                "source".to_string(),
                "slices/core/epistemics/module.ttl".to_string(),
                None,
            ),
        ]
    }

    /// Byte golden: the `graph/provenance` named-graph content of an emitted
    /// snapshot, over a FIXED synthetic provenance projection. Pins the per-graph fold
    /// path (public projection → N-Triples → add_named canonicalization → emit →
    /// read-back) byte-for-byte, independent of the full gmeow.gts. A second emit is
    /// asserted byte-identical (determinism). The golden ALSO proves S0.5 (no runtime id).
    #[test]
    fn graph_provenance_fold_byte_golden() {
        let prov_nt = crate::stages::provenance_graph::project_provenance_graph(
            &fixed_provenance_projection(),
        );

        let build = || {
            let mut builder = SnapshotBuilder::new();
            add_base_nq(
                &mut builder,
                b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
                "base",
            )
            .expect("fold base graph");
            add_named(
                &mut builder,
                prov_nt.as_bytes(),
                GRAPH_PROVENANCE,
                "provenance",
            )
            .expect("fold graph/provenance");
            emit_gts(
                &builder,
                "dist",
                Some(vec!["gzip".to_string()]),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
                purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            )
            .expect("emit snapshot")
        };

        let gts = build();
        let folded = folded_graph_nquads(&gts, GRAPH_PROVENANCE);
        assert!(
            !folded.is_empty(),
            "graph/provenance must carry the projection"
        );
        // S0.5: the folded bytes must NOT contain any runtime id.
        assert!(
            !folded.contains("unit#"),
            "no runtime UnitId in graph/provenance"
        );
        assert!(
            !folded.contains("artifact#"),
            "no runtime ArtifactId in graph/provenance"
        );
        assert!(
            !folded.contains("origin-set#"),
            "no runtime OriginSetId in graph/provenance"
        );
        insta::assert_snapshot!("graph_provenance_fold", folded);

        // Determinism: a second build folds the SAME graph/provenance content.
        let gts2 = build();
        assert_eq!(
            folded_graph_nquads(&gts2, GRAPH_PROVENANCE),
            folded,
            "the graph/provenance fold must be byte-deterministic"
        );
    }

    /// The hard-fail attribution gate passes on the REAL ontology: every
    /// authored quad carries ≥1 stage-origin occurrence. Builds the real per-quad
    /// provenance sidecar and runs `check_provenance` over its full coverage set.
    #[test]
    fn real_ontology_every_quad_is_attributed() {
        let root = repo_root();
        let (prov, expected) =
            crate::stages::source_load::attributed_base_provenance(&root).expect("attribute");
        assert!(
            expected.len() > 5_000,
            "real authored base graph unexpectedly small: {} quads",
            expected.len()
        );
        purrdf::provenance::check_provenance(&prov, &expected)
            .expect("every authored quad must carry ≥1 stage-origin occurrence");
        // The public projection over the real ontology must carry NO runtime id.
        for (_quad, name, kind, artifact, _loc) in prov.public_projection() {
            for field in [&name, &kind, &artifact] {
                assert!(!field.contains("unit#"), "runtime UnitId leaked: {field}");
                assert!(
                    !field.contains("artifact#"),
                    "runtime ArtifactId leaked: {field}"
                );
                assert!(
                    !field.contains("origin-set#"),
                    "runtime OriginSetId leaked: {field}"
                );
            }
        }
    }

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }
}

#[cfg(test)]
mod native_assembly_tests {
    use super::*;

    /// Count the `owl:AllDisjointClasses` typed subjects (blank nodes) and the
    /// `owl:members` list-head triples in a canonical N-Quads blob.
    fn disjoint_shape(canon: &str) -> (usize, usize) {
        let all_disjoint = canon
            .lines()
            .filter(|l| {
                l.contains("<http://www.w3.org/2002/07/owl#AllDisjointClasses>")
                    && l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>")
            })
            .count();
        let members = canon
            .lines()
            .filter(|l| l.contains("<http://www.w3.org/2002/07/owl#members>"))
            .count();
        (all_disjoint, members)
    }

    /// Two distinct `owl:AllDisjointClasses` axioms authored in SEPARATE files MUST
    /// survive the native `RdfDataset::union` standardize-apart as TWO distinct blank
    /// lists — never collapsing into one. This is exactly why the removed
    /// `ingest_turtle_scoped` string-prefixed per-file blanks; the union's per-input
    /// `BlankScope` is its native replacement. Each file independently mints `_:b0`
    /// (the codecs restart blank counters per parse), so without standardize-apart the
    /// two axioms would merge into a single subject and one of the lists would vanish.
    #[test]
    fn two_all_disjoint_lists_survive_union_distinctly() {
        // Two files, each with ONE owl:AllDisjointClasses over a DIFFERENT class set,
        // both anonymous (blank-node subject + blank-node list cells).
        let file_a = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:A ex:B ex:C ) .
"#;
        let file_b = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:D ex:E ) .
"#;

        let union = union_turtle_datasets(&[file_a.to_vec(), file_b.to_vec()])
            .expect("union two disjoint files");
        let nq = dataset_to_nquads(&union).expect("union → n-quads");
        let canon = canonicalize_nq(&nq, "base").expect("canonicalize union");

        let (subjects, members) = disjoint_shape(&canon);
        assert_eq!(
            subjects, 2,
            "the union must keep TWO distinct AllDisjointClasses subjects (one per file); \
             a collapse would leave only 1.\nCanonical:\n{canon}"
        );
        assert_eq!(
            members, 2,
            "each AllDisjointClasses must keep its own owl:members list head"
        );

        // The two list contents (3-element and 2-element) must both be present — a
        // collapse would lose one set entirely. Count rdf:first cells: 3 + 2 = 5.
        let first_cells = canon
            .lines()
            .filter(|l| l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#first>"))
            .count();
        assert_eq!(
            first_cells, 5,
            "both lists (3 + 2 members) must survive distinctly; got {first_cells} rdf:first cells"
        );

        // Contrast: parsing BOTH files into ONE dataset WITHOUT standardize-apart
        // would let the two `_:b0` subjects collide. We can't easily force that here,
        // but the union path above is the production assembly — its 2-subject result
        // is the proof the native union preserves per-file distinctness.
    }

    /// The projection-ledger named graph built natively (`turtle_to_nquads`)
    /// canonicalizes to a STABLE, idempotent RDFC-1.0 N-Quads form that carries every
    /// authored triple (typed literals + the blank-node structural-drop list). This
    /// retired the prior oxigraph-`Store` cross-check: the conversion is now fully native
    /// (`turtle_to_nquads` → `canonical_flat_nquads`), so the meaningful invariant is
    /// canonical idempotence + content fidelity, not equality to a removed oxigraph path.
    #[test]
    fn projection_ledger_canonicalizes_stably() {
        // A representative projection-report fragment: typed loss-ledger entries with
        // a blank-node structural-drop list (exercises blank canonicalization).
        let report_ttl = br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
<https://blackcatinformatics.ca/gmeow/projection/okf>
    a gmeow:ProjectionLedgerEntry ;
    rdfs:label "OKF projection" ;
    gmeow:preservationKind gmeow:Lossy ;
    gmeow:droppedCount "3"^^xsd:integer ;
    gmeow:structuralDrop [ gmeow:dropKind gmeow:StatementLayer ] .
"#;

        // Native path: the C3 helper, then RDFC-1.0 canonicalize.
        let native = turtle_to_nquads(report_ttl).expect("native turtle → n-quads");
        let native_canon = canonicalize_nq(&native, "projledger").expect("canon native");

        // Idempotence: re-canonicalizing the canonical form is a fixpoint.
        let recanon = canonicalize_nq(native_canon.as_bytes(), "projledger").expect("recanon");
        assert_eq!(
            native_canon, recanon,
            "RDFC-1.0 canonicalization of the projection-ledger N-Quads must be idempotent"
        );

        // Content fidelity: every authored triple survives (typed literal + blank-node
        // structural-drop list), and a canonical blank label (`_:c14n…`) is minted.
        assert!(native_canon.contains("<https://blackcatinformatics.ca/gmeow/projection/okf>"));
        assert!(native_canon.contains("\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
        assert!(native_canon.contains("<https://blackcatinformatics.ca/gmeow/StatementLayer>"));
        assert!(
            native_canon.contains("_:c14n"),
            "the blank structural-drop node must carry a canonical RDFC-1.0 label"
        );
    }

    /// `load_authored_default` over the real repo tree produces a non-empty
    /// multilingual default graph (the union path + native translation fold),
    /// without external documentation payload references.
    #[test]
    fn authored_default_assembles_natively() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let nq = load_authored_default(&root).expect("authored default graph");
        let text = String::from_utf8(nq).expect("utf-8 n-quads");
        assert!(
            !text.trim().is_empty(),
            "the default graph must be non-empty"
        );
        // The root ontology declaration must be present (the ontology IRI has no
        // trailing slash — `…/gmeow`, distinct from the `…/gmeow/` namespace prefix).
        assert!(
            text.contains("<https://blackcatinformatics.ca/gmeow>"),
            "the authored default graph must carry the root ontology subject"
        );
        let guide_predicate = " <https://blackcatinformatics.ca/gmeow/guideBlob> ";
        assert!(
            !text.lines().any(|line| line.contains(guide_predicate)),
            "external guideBlob references must not enter the logical bundle"
        );
        // Determinism: a second assembly is byte-identical.
        let again = load_authored_default(&root).expect("authored default graph (2)");
        assert_eq!(
            text.as_bytes(),
            again.as_slice(),
            "the native authored assembly must be byte-deterministic"
        );
    }
}

/// The playground lift of the chase-invented witness-derivation subgraph
/// ([`build_playground_trig`]): the `gmeow:InventedWitness` typings + their minting
/// reifiers must cross from `graph/diagnostics` into `graph/reasoning` with their
/// content-addressed Skolem IRIs preserved 1:1 (never collapsed), while `graph/diagnostics`
/// findings stay OUT of the playground.
#[cfg(test)]
mod playground_witness_lift {
    use super::*;

    const GRAPH_REASONING: &str = gmeow_logic::result_rdf::GRAPH_REASONING;
    const SKOLEM_A: &str = "https://blackcatinformatics.ca/gmeow/skolem/aaaaaa";
    const SKOLEM_B: &str = "https://blackcatinformatics.ca/gmeow/skolem/bbbbbb";

    // A carrier carrying (in `graph/diagnostics`) two distinct invented nulls, each with
    // its existential ordinal + minting head-quad reifier, plus a finding that must NOT be
    // lifted into the playground.
    const CARRIER_TRIG: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://example.org/w/> .
@prefix skolem: <https://blackcatinformatics.ca/gmeow/skolem/> .
GRAPH <https://blackcatinformatics.ca/gmeow/graph/diagnostics> {
  ex:r1 rdf:subject ex:x1 ; rdf:predicate ex:p ; rdf:object skolem:aaaaaa ; gmeow:viaRule ex:rule1 .
  skolem:aaaaaa a gmeow:InventedWitness ; gmeow:existentialOrdinal \"0\"^^xsd:nonNegativeInteger .
  ex:r2 rdf:subject ex:x2 ; rdf:predicate ex:p ; rdf:object skolem:bbbbbb ; gmeow:viaRule ex:rule1 .
  skolem:bbbbbb a gmeow:InventedWitness ; gmeow:existentialOrdinal \"1\"^^xsd:nonNegativeInteger .
  ex:finding1 a gmeow:Finding ; gmeow:findingCode \"X001\" .
}
";
    // A trivial closure that references the first null as an object (the shape the reason
    // stage commits): the witness must land in the SAME graph as the closure that uses it.
    const CLOSURE_TTL: &str = "\
@prefix ex: <https://example.org/w/> .
@prefix skolem: <https://blackcatinformatics.ca/gmeow/skolem/> .
ex:x1 ex:p skolem:aaaaaa .
";

    #[test]
    fn witness_subgraph_survives_into_reasoning_graph() {
        let carrier =
            parse_dataset(CARRIER_TRIG.as_bytes(), "application/trig", None).expect("carrier");
        let closure = parse_dataset(CLOSURE_TTL.as_bytes(), "text/turtle", None).expect("closure");

        let trig = build_playground_trig(&carrier, &closure).expect("playground");
        let pg = parse_dataset(&trig, "application/trig", None).expect("parse playground");
        let reasoning = pg.project_named_graph(GRAPH_REASONING);

        // Collect the reasoning-graph triples as `(subject_iri, predicate, object)` — the
        // subject IRI is compared BYTE-IDENTICAL, so a collapse to `<skolem>` would fail.
        let mut triples: Vec<(String, String, RdfTerm)> = Vec::new();
        for q in reasoning.owned_quads() {
            if let RdfTerm::Iri(s) = &q.subject {
                triples.push((s.clone(), q.predicate.clone(), q.object.clone()));
            }
        }
        let has = |s: &str, p: &str, o_iri: &str| {
            triples.iter().any(|(qs, qp, qo)| {
                qs == s && qp == p && matches!(qo, RdfTerm::Iri(i) if i == o_iri)
            })
        };
        let has_lit = |s: &str, p: &str| triples.iter().any(|(qs, qp, _)| qs == s && qp == p);

        const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const VIA_RULE: &str = "https://blackcatinformatics.ca/gmeow/viaRule";
        const EX_ORD: &str = "https://blackcatinformatics.ca/gmeow/existentialOrdinal";
        const INVENTED: &str = "https://blackcatinformatics.ca/gmeow/InventedWitness";

        // Both distinct nulls survive, typed + ordinal-carrying, at their exact Skolem IRIs.
        assert!(
            has(SKOLEM_A, RDF_TYPE, INVENTED),
            "aaaaaa typed InventedWitness"
        );
        assert!(
            has(SKOLEM_B, RDF_TYPE, INVENTED),
            "bbbbbb typed InventedWitness"
        );
        assert!(has_lit(SKOLEM_A, EX_ORD), "aaaaaa existentialOrdinal");
        assert!(has_lit(SKOLEM_B, EX_ORD), "bbbbbb existentialOrdinal");
        assert_ne!(
            SKOLEM_A, SKOLEM_B,
            "the two nulls are distinct (no collision)"
        );

        // Both reifiers survive with their firing rule (the frontier/predicate too).
        assert!(
            has(
                "https://example.org/w/r1",
                VIA_RULE,
                "https://example.org/w/rule1"
            ),
            "reifier r1 viaRule survives"
        );
        assert!(
            has(
                "https://example.org/w/r2",
                VIA_RULE,
                "https://example.org/w/rule1"
            ),
            "reifier r2 viaRule survives"
        );
        assert!(
            has(
                "https://example.org/w/r1",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#object",
                SKOLEM_A
            ),
            "reifier r1 rdf:object is the exact Skolem null"
        );

        // The finding is NOT lifted — findings stay out of the playground.
        assert!(
            !triples
                .iter()
                .any(|(s, _, _)| s == "https://example.org/w/finding1"),
            "graph/diagnostics findings must not leak into the playground"
        );
        assert!(
            !has_lit("https://example.org/w/finding1", "any"),
            "no finding subject present"
        );

        // Determinism: byte-stable across runs.
        let again = build_playground_trig(&carrier, &closure).expect("playground again");
        assert_eq!(
            trig, again,
            "playground witness lift must be byte-deterministic"
        );
    }
}

/// The PRODUCTION playground builder `playground_trig_from_bundle` — the surface
/// `gmeow-dev export-docs` ships — must project the committed bundle's OWN named graphs:
/// `graph/documentation` + `graph/reasoning` carried 1:1, the chase-invented-null witness
/// subgraph lifted from `graph/diagnostics` into the reasoning graph, and NO finding leaking.
#[cfg(test)]
mod playground_from_bundle {
    use super::*;

    const GRAPH_REASONING: &str = gmeow_logic::result_rdf::GRAPH_REASONING;
    const SKOLEM_A: &str = "https://blackcatinformatics.ca/gmeow/skolem/aaaaaa";

    // A bundle-shaped dataset: a documentation-graph triple, a reasoning-graph closure triple,
    // and a `graph/diagnostics` witness projection (an invented null + its minting reifier +
    // a finding that must NOT be lifted). Named-graph-preserving TriG stands in for the folded
    // GTS bundle `dataset_from_gts_graph` yields in production.
    const BUNDLE_TRIG: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://example.org/w/> .
@prefix skolem: <https://blackcatinformatics.ca/gmeow/skolem/> .
GRAPH <https://blackcatinformatics.ca/gmeow/graph/documentation> {
  ex:TermA a gmeow:Class ; rdfs:label \"Term A\" .
}
GRAPH <https://blackcatinformatics.ca/gmeow/graph/reasoning> {
  ex:x1 ex:p skolem:aaaaaa .
}
GRAPH <https://blackcatinformatics.ca/gmeow/graph/diagnostics> {
  ex:r1 rdf:subject ex:x1 ; rdf:predicate ex:p ; rdf:object skolem:aaaaaa ; gmeow:viaRule ex:rule1 .
  skolem:aaaaaa a gmeow:InventedWitness ; gmeow:existentialOrdinal \"0\"^^xsd:nonNegativeInteger .
  ex:finding1 a gmeow:Finding ; gmeow:findingCode \"X001\" .
}
";

    #[test]
    fn bundle_projection_carries_docs_reasoning_and_witness_only() {
        let bundle = parse_dataset(BUNDLE_TRIG.as_bytes(), "application/trig", None)
            .expect("parse synthetic bundle");
        let trig = playground_trig_from_bundle(&bundle).expect("playground from bundle");
        let pg = parse_dataset(&trig, "application/trig", None).expect("parse playground trig");

        const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
        const VIA_RULE: &str = "https://blackcatinformatics.ca/gmeow/viaRule";
        const EX_ORD: &str = "https://blackcatinformatics.ca/gmeow/existentialOrdinal";
        const INVENTED: &str = "https://blackcatinformatics.ca/gmeow/InventedWitness";

        // The documentation triple survives in the documentation graph.
        let docs = pg.project_named_graph(GRAPH_DOCUMENTATION);
        assert!(
            docs.owned_quads().any(|q| matches!(&q.subject, RdfTerm::Iri(s) if s == "https://example.org/w/TermA")
                && q.predicate == RDF_TYPE
                && matches!(&q.object, RdfTerm::Iri(o) if o == "https://blackcatinformatics.ca/gmeow/Class")),
            "documentation-graph triple must be carried 1:1"
        );

        // The reasoning graph carries BOTH the closure triple and the lifted witness subgraph.
        let reasoning = pg.project_named_graph(GRAPH_REASONING);
        let triples: Vec<(String, String, RdfTerm)> = reasoning
            .owned_quads()
            .filter_map(|q| match &q.subject {
                RdfTerm::Iri(s) => Some((s.clone(), q.predicate.clone(), q.object.clone())),
                _ => None,
            })
            .collect();
        let has = |s: &str, p: &str, o_iri: &str| {
            triples.iter().any(|(qs, qp, qo)| {
                qs == s && qp == p && matches!(qo, RdfTerm::Iri(i) if i == o_iri)
            })
        };
        let has_lit = |s: &str, p: &str| triples.iter().any(|(qs, qp, _)| qs == s && qp == p);

        // Closure triple present.
        assert!(
            has(
                "https://example.org/w/x1",
                "https://example.org/w/p",
                SKOLEM_A
            ),
            "reasoned closure triple must be carried into the reasoning graph"
        );
        // Witness survives 1:1, typed + ordinal-carrying, at its exact Skolem IRI.
        assert!(
            has(SKOLEM_A, RDF_TYPE, INVENTED),
            "aaaaaa typed InventedWitness"
        );
        assert!(
            has_lit(SKOLEM_A, EX_ORD),
            "aaaaaa existentialOrdinal survives"
        );
        // The reifier survives with its firing rule and the exact null as its object.
        assert!(
            has(
                "https://example.org/w/r1",
                VIA_RULE,
                "https://example.org/w/rule1"
            ),
            "reifier r1 viaRule survives"
        );
        assert!(
            has("https://example.org/w/r1", RDF_OBJECT, SKOLEM_A),
            "reifier r1 rdf:object is the exact Skolem null"
        );
        // NO finding leaks anywhere in the playground.
        for g in [GRAPH_DOCUMENTATION, GRAPH_REASONING, GRAPH_DIAGNOSTICS] {
            let quads = pg.project_named_graph(g);
            assert!(
                !quads.owned_quads().any(|q| matches!(&q.subject, RdfTerm::Iri(s) if s == "https://example.org/w/finding1")),
                "graph/diagnostics findings must never leak into the playground (graph {g})"
            );
        }

        // Determinism: byte-stable across runs.
        let again = playground_trig_from_bundle(&bundle).expect("playground from bundle again");
        assert_eq!(
            trig, again,
            "playground-from-bundle must be byte-deterministic"
        );
    }

    #[test]
    fn empty_diagnostics_lifts_no_witness() {
        // A bundle with no `graph/diagnostics` witness projection adds nothing beyond the
        // documentation + reasoning graphs — the empty witness set is a no-op, not an error.
        const NO_DIAG: &str = "\
@prefix ex: <https://example.org/w/> .
GRAPH <https://blackcatinformatics.ca/gmeow/graph/documentation> {
  ex:TermA ex:p ex:o .
}
GRAPH <https://blackcatinformatics.ca/gmeow/graph/reasoning> {
  ex:x1 ex:p ex:y1 .
}
";
        let bundle =
            parse_dataset(NO_DIAG.as_bytes(), "application/trig", None).expect("parse bundle");
        let trig = playground_trig_from_bundle(&bundle).expect("playground from bundle");
        let pg = parse_dataset(&trig, "application/trig", None).expect("parse playground trig");
        let reasoning = pg.project_named_graph(GRAPH_REASONING);
        assert!(
            !reasoning
                .owned_quads()
                .any(|q| matches!(&q.object, RdfTerm::Iri(o) if o.contains("InventedWitness"))),
            "no witness may appear when diagnostics carries none"
        );
        assert_eq!(
            reasoning.owned_quads().count(),
            1,
            "only the closure triple survives"
        );
    }
}

/// Semantic golden over the executable "try it" docs core
/// ([`executable_docs_from_sources`]). This surface is otherwise UNGUARDED — the
/// superset gate excludes `REP_ONTOLOGY_DOCS`, and the fold gates compare RDF quads, not
/// the dangling docs blob — so a change to WHICH inferences are attributed to WHICH
/// example (e.g. from a future incremental-reasoning path replacing the full-union pass)
/// could pass every default gate. This test pins the *semantic content* over a fixed
/// fixture: the per-example asserted-vs-inferred attribution, the cross-example bucket,
/// and the playground asset — each Skolem/blank-normalized so it survives a witness-IRI
/// shift while still catching an attribution divergence.
#[cfg(test)]
mod docs_try_it_golden {
    use super::*;

    // A minimal TBox: a two-step subclass chain. Reasoning propagates an individual's
    // type up the chain, so an example asserting `a Dog` yields inferred `a Animal`,
    // `a LivingThing` — the canonical "try it" shape, with no existentials (hence no
    // Skolem witnesses) so the fixture is fully deterministic.
    const EDB_TTL: &str = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:Dog rdfs:subClassOf ex:Animal .
ex:Animal rdfs:subClassOf ex:LivingThing .
";
    const EX_DOG_TTL: &str = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
ex:rex a ex:Dog .
";
    const EX_CAT_TTL: &str = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
ex:felix a ex:Animal .
";
    const SLICE: &str = "https://example.org/gmeow-try-it/slice";

    /// Normalize a display line so the golden pins WHICH inferences land WHERE, not the
    /// non-stable identity of blank / content-addressed Skolem witnesses. Blank labels
    /// collapse to `_:_`; any Skolem-witness IRI collapses to `<skolem>`. (The fixture is
    /// witness-free by design, so this is a defensive no-op here — present so the golden
    /// is robust if a future reasoning path starts materializing witnesses.)
    fn norm(line: &str) -> String {
        line.split(' ')
            .map(|tok| {
                if tok.starts_with("_:") {
                    "_:_".to_string()
                } else if tok.contains("blackcatinformatics.ca/gmeow/skolem/") {
                    "<skolem>".to_string()
                } else {
                    tok.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn norm_all(lines: &[String]) -> Vec<String> {
        let mut v: Vec<String> = lines.iter().map(|l| norm(l)).collect();
        v.sort();
        v.dedup();
        v
    }

    fn compute() -> gmeow_docs::ExecutableDocsData {
        let edb = parse_dataset(EDB_TTL.as_bytes(), "text/turtle", None).expect("parse EDB");
        // The base ontology-only closure = reason(EDB), exactly as the reason stage
        // commits it — subtracted so only EXAMPLE-INDUCED inferences survive.
        let base = crate::stages::reason::reason_over_dataset(edb.as_ref()).expect("reason base");
        let sources = vec![
            ExampleSource {
                slice: SLICE.to_string(),
                logical_path: "examples/dog.ttl".to_string(),
                text: EX_DOG_TTL.to_string(),
            },
            ExampleSource {
                slice: SLICE.to_string(),
                logical_path: "examples/cat.ttl".to_string(),
                text: EX_CAT_TTL.to_string(),
            },
        ];
        // The carrier here is the bare EDB (no documentation named graph), so the
        // playground asset is exactly the base closure routed into `graph/reasoning`.
        executable_docs_from_sources(
            edb.as_ref(),
            base.closure.as_bytes(),
            &sources,
            edb.as_ref(),
        )
        .expect("executable docs")
    }

    #[test]
    fn try_it_attribution_is_semantically_pinned() {
        const NS: &str = "https://example.org/gmeow-try-it/";
        let data = compute();

        // ── Per-example attribution: each example's OWN subject carries its induced
        //    inferences; the told triple stays in `asserted`, never `inferred`. ──
        assert_eq!(
            data.example_inferences.len(),
            2,
            "both examples induce an inference"
        );
        let dog = data
            .example_inferences
            .get(&gmeow_docs::example_key(SLICE, "examples/dog.ttl"))
            .expect("dog example diff");
        assert_eq!(
            norm_all(&dog.asserted),
            vec![format!("<{NS}rex> rdf:type <{NS}Dog>")]
        );
        assert_eq!(
            norm_all(&dog.inferred),
            vec![
                format!("<{NS}rex> rdf:type <{NS}Animal>"),
                format!("<{NS}rex> rdf:type <{NS}LivingThing>"),
            ]
        );
        let cat = data
            .example_inferences
            .get(&gmeow_docs::example_key(SLICE, "examples/cat.ttl"))
            .expect("cat example diff");
        assert_eq!(
            norm_all(&cat.asserted),
            vec![format!("<{NS}felix> rdf:type <{NS}Animal>")]
        );
        assert_eq!(
            norm_all(&cat.inferred),
            vec![format!("<{NS}felix> rdf:type <{NS}LivingThing>")]
        );

        // ── No inference in this fixture is unattributable — the cross-example bucket
        //    is empty (there are no shared / Skolem-witness inferences here). ──
        assert!(
            norm_all(&data.cross_example).is_empty(),
            "no unattributable inferences: got {:?}",
            norm_all(&data.cross_example)
        );

        // ── Playground asset = documentation graph (∅ here) ∪ base closure. Pin it by
        //    its triple set (graph-dropped, normalized) so its assembly is guarded
        //    without coupling to the exact TriG serialization. ──
        let edb = parse_dataset(EDB_TTL.as_bytes(), "text/turtle", None).expect("parse EDB");
        let base = crate::stages::reason::reason_over_dataset(edb.as_ref()).expect("reason base");
        let base_ds = parse_dataset(base.closure.as_bytes(), "text/turtle", None)
            .expect("parse base closure");
        let base_triples: std::collections::BTreeSet<String> = base_ds
            .owned_quads()
            .map(|q| norm(&format_triple(&q)))
            .collect();
        let pg_ds = parse_dataset(&data.playground_trig, "application/trig", None)
            .expect("parse playground");
        let pg_triples: std::collections::BTreeSet<String> = pg_ds
            .owned_quads()
            .map(|q| norm(&format_triple(&q)))
            .collect();
        assert_eq!(
            pg_triples, base_triples,
            "playground asset must be documentation(∅) ∪ base closure"
        );

        // ── Determinism: the core is a pure function of its inputs. ──
        let again = compute();
        assert_eq!(
            data.example_inferences, again.example_inferences,
            "attribution must be deterministic"
        );
        assert_eq!(
            data.cross_example, again.cross_example,
            "cross_example must be deterministic"
        );
        assert_eq!(
            data.playground_trig, again.playground_trig,
            "playground asset must be byte-deterministic"
        );
    }

    /// The witness-insensitive subtraction: an ONTOLOGY-level Skolem-witness edge whose
    /// content-addressed IRI differs between the reduced-seed reasoning context and the
    /// committed base closure must still cancel against the base (never leak into
    /// `cross_example`), while an example-SUBJECT fact (absent from the base) survives.
    /// Without normalization the context-shifted witness IRI fails to match and pollutes
    /// the bucket — the exact divergence the full-EDB-vs-reduced-seed validation surfaced
    /// on the real ontology.
    #[test]
    fn ontology_witness_cancels_across_skolem_iri_shift() {
        // Seed: C ⊑ Mid ⊑ <skolem/aaa> — transitivity DERIVES `C ⊑ <skolem/aaa>`, a
        // non-example-subject witness edge (a cross_example candidate).
        let seed_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:C rdfs:subClassOf ex:Mid .
ex:Mid rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/skolem/aaa> .
";
        let seed = parse_dataset(seed_ttl.as_bytes(), "text/turtle", None).expect("parse seed");
        // The committed base closure carries the SAME edge under a DIFFERENT skolem IRI.
        let base_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:C rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/skolem/bbb> .
";
        let sources = vec![ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/probe.ttl".to_string(),
            text: "@prefix ex: <https://example.org/gmeow-try-it/> .\nex:x a ex:C .\n".to_string(),
        }];
        let data = executable_docs_from_sources(
            seed.as_ref(),
            base_ttl.as_bytes(),
            &sources,
            seed.as_ref(),
        )
        .expect("executable docs");

        // The ontology witness edge (C ⊑ <skolem/aaa>) is SUBTRACTED despite the base
        // carrying it under <skolem/bbb> — so it never reaches cross_example.
        assert!(
            !data
                .cross_example
                .iter()
                .any(|l| l.contains("/C>") && l.contains("skolem")),
            "context-shifted ontology witness leaked into cross_example: {:?}",
            data.cross_example
        );
        // The example subject still receives its derived type (x a C told; x a Mid derived).
        let probe = data
            .example_inferences
            .get(&gmeow_docs::example_key(SLICE, "examples/probe.ttl"))
            .expect("probe example diff");
        assert!(
            probe
                .inferred
                .iter()
                .any(|l| l.contains("/x>") && l.contains("/Mid>")),
            "example subject must still receive its derived type: {:?}",
            probe.inferred
        );
    }

    /// Two DIFFERENT worked examples naming the SAME subject IRI are ambiguous: neither
    /// example can be said to have solely induced an inference on that shared subject, so
    /// the induced inferences must route to `cross_example`, never to either example's
    /// `.inferred` (a plain last-write-wins map would misattribute them to whichever
    /// example happened to be parsed last).
    #[test]
    fn shared_subject_across_examples_routes_to_cross_example() {
        const NS: &str = "https://example.org/gmeow-try-it/";
        let edb = parse_dataset(EDB_TTL.as_bytes(), "text/turtle", None).expect("parse EDB");
        let base = crate::stages::reason::reason_over_dataset(edb.as_ref()).expect("reason base");
        let shared_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
ex:shared a ex:Dog .
";
        let sources = vec![
            ExampleSource {
                slice: SLICE.to_string(),
                logical_path: "examples/shared-a.ttl".to_string(),
                text: shared_ttl.to_string(),
            },
            ExampleSource {
                slice: SLICE.to_string(),
                logical_path: "examples/shared-b.ttl".to_string(),
                text: shared_ttl.to_string(),
            },
        ];
        let data = executable_docs_from_sources(
            edb.as_ref(),
            base.closure.as_bytes(),
            &sources,
            edb.as_ref(),
        )
        .expect("executable docs");

        // Both `ex:shared a ex:Animal` and `ex:shared a ex:LivingThing` are induced by an
        // asserted `ex:shared a ex:Dog`, but which example "owns" `ex:shared` is
        // ambiguous — they must land in cross_example, not in either example's diff.
        let expected_cross = vec![
            format!("<{NS}shared> rdf:type <{NS}Animal>"),
            format!("<{NS}shared> rdf:type <{NS}LivingThing>"),
        ];
        assert_eq!(
            norm_all(&data.cross_example),
            expected_cross,
            "ambiguous shared-subject inferences must route to cross_example: {:?}",
            norm_all(&data.cross_example)
        );

        // Neither example's diff carries the ambiguous inferences (both would be
        // present, and only their own `asserted` line, if attribution were unambiguous
        // — but a shared subject must never appear in a per-example `.inferred`).
        for key in [
            gmeow_docs::example_key(SLICE, "examples/shared-a.ttl"),
            gmeow_docs::example_key(SLICE, "examples/shared-b.ttl"),
        ] {
            if let Some(diff) = data.example_inferences.get(&key) {
                assert!(
                    diff.inferred.is_empty(),
                    "shared-subject inference must not be attributed to a single example {key}: {:?}",
                    diff.inferred
                );
            }
        }
    }

    /// `build_executable_docs_data`'s core correctness claim: reasoning the reduced seed
    /// `source_load_dataset(upstream).project_named_graph(GRAPH_AUTHORED_DEFAULT)`
    /// reproduces the attribution `assemble_object_level_edb`'s FULL object-level EDB
    /// would give, because worked examples parse into the default world and the calculus
    /// is same-world (imports/statements/alignments/logic ride NAMED worlds the examples
    /// cannot reach). This mirrors `assemble_object_level_edb`'s real shape (carrier.rs
    /// ~515-556): the authored-default content is projected OUT of its internal transport
    /// tag into the true default graph, then UNIONED with the other pipeline products,
    /// each rooted in ITS OWN named-world graph (never the default graph).
    ///
    /// The fixture is DISCRIMINATING: the full EDB's "import" named world carries an
    /// axiom (`ex:Animal rdfs:subClassOf ex:ImportedExtra`) that WOULD transitively fire
    /// on the example's own asserted type — yielding `ex:rex a ex:ImportedExtra` — if it
    /// were (wrongly) merged into the default world the examples inhabit; a control
    /// computation below reasons the SAME axioms flattened into one world to prove that.
    /// In the real (world-separated) fixture that axiom lives only in the full seed's
    /// `import` named graph, structurally absent from the reduced (authored-default
    /// projection) seed — so if the reduced-seed optimization, or the reasoner's
    /// world-scoping it relies on, were unsound, this test would see `reduced != full`
    /// or the `ImportedExtra` type leaking into an example's `.inferred`.
    #[test]
    fn reduced_seed_attribution_matches_full_edb_attribution() {
        // Mirrors what `source_load_dataset(upstream)` carries: the authored default-
        // world chain under the GRAPH_AUTHORED_DEFAULT internal transport tag.
        let raw_source_load_trig = format!(
            "@prefix ex: <https://example.org/gmeow-try-it/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             GRAPH <{GRAPH_AUTHORED_DEFAULT}> {{\n\
             \x20 ex:Dog rdfs:subClassOf ex:Animal .\n\
             \x20 ex:Animal rdfs:subClassOf ex:LivingThing .\n\
             }}\n"
        );
        let raw_source_load =
            parse_dataset(raw_source_load_trig.as_bytes(), "application/trig", None)
                .expect("parse raw source-load fixture");
        // The reduced seed: exactly `build_executable_docs_data`'s
        // `source_load_dataset(upstream)?.project_named_graph(GRAPH_AUTHORED_DEFAULT)`
        // call — the authored chain re-rooted into the true default graph.
        let reduced_seed = raw_source_load.project_named_graph(GRAPH_AUTHORED_DEFAULT);

        // A SEPARATE named "import" world (standing in for GRAPH_IMPORTS /
        // GRAPH_ALIGNMENTS / the logic graphs `assemble_object_level_edb` unions in),
        // carrying an additional superclass edge off `ex:Animal` that only a
        // world-isolation bug would let leak into the default-world reasoning the
        // examples participate in.
        let import_world_trig = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
GRAPH <https://example.org/gmeow-try-it/import> {
  ex:Animal rdfs:subClassOf ex:ImportedExtra .
}
";
        let import_world = parse_dataset(import_world_trig.as_bytes(), "application/trig", None)
            .expect("parse import-world fixture");

        // Control computation proving the fixture is DISCRIMINATING: flatten the SAME
        // authored-chain + import axioms into a single (default) world and reason over
        // `(flat_edb ∪ the dog example)` directly — no reduced/full split at all. If
        // `ex:Animal rdfs:subClassOf ex:ImportedExtra` were reachable from a default-
        // world example, this is where it would show up.
        let flat_probe_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:Dog rdfs:subClassOf ex:Animal .
ex:Animal rdfs:subClassOf ex:LivingThing .
ex:Animal rdfs:subClassOf ex:ImportedExtra .
ex:rex a ex:Dog .
";
        let flat_probe = parse_dataset(flat_probe_ttl.as_bytes(), "text/turtle", None)
            .expect("parse flat probe fixture");
        let flat_reasoned = crate::stages::reason::reason_over_dataset(flat_probe.as_ref())
            .expect("reason flat probe");
        assert!(
            flat_reasoned.closure.contains("ImportedExtra"),
            "fixture is not discriminating: merging the import axiom into the default \
             world never yields an ImportedExtra inference on the example subject: {}",
            flat_reasoned.closure
        );

        // The full object-level EDB, shaped exactly like `assemble_object_level_edb`:
        // the default-world projection UNIONED with the other named-world graphs.
        let full_edb = purrdf::RdfDataset::union(&[&reduced_seed, import_world.as_ref()]);

        // What stage-reason would commit for this full EDB: the base ontology-only
        // closure, subtracted (witness-insensitively) in both runs below. Because the
        // reasoner is world-scoped by design (PIPELINE_SPINE's same-world calculus),
        // this closure does NOT show the import axiom crossing into the default world —
        // that is exactly the invariant this test locks down, via the reduced-vs-full
        // attribution comparison below rather than via this closure alone.
        let base =
            crate::stages::reason::reason_over_dataset(&full_edb).expect("reason full-EDB base");

        let sources = vec![
            ExampleSource {
                slice: SLICE.to_string(),
                logical_path: "examples/dog.ttl".to_string(),
                text: EX_DOG_TTL.to_string(),
            },
            ExampleSource {
                slice: SLICE.to_string(),
                logical_path: "examples/cat.ttl".to_string(),
                text: EX_CAT_TTL.to_string(),
            },
        ];

        // Run 1 — production behavior: reason the REDUCED seed (the authored
        // default-world projection alone), exactly as `build_executable_docs_data` does.
        let reduced = executable_docs_from_sources(
            &reduced_seed,
            base.closure.as_bytes(),
            &sources,
            &reduced_seed,
        )
        .expect("executable docs (reduced seed)");

        // Run 2 — the ground truth: reason the FULL object-level EDB, import world and
        // all, exactly as `assemble_object_level_edb` + stage-reason would.
        let full =
            executable_docs_from_sources(&full_edb, base.closure.as_bytes(), &sources, &full_edb)
                .expect("executable docs (full EDB)");

        // The reduction must be attribution-lossless: same per-example diffs, same
        // cross-example bucket. Normalize with the module's witness-insensitive `norm_all`
        // — this fixture has no existentials so it is a no-op here, but keeps the
        // comparison robust to incidental witness IRIs.
        let reduced_keys: Vec<&String> = reduced.example_inferences.keys().collect();
        let full_keys: Vec<&String> = full.example_inferences.keys().collect();
        assert_eq!(
            reduced_keys, full_keys,
            "reduced- and full-seed runs must attribute to the same set of examples"
        );
        for key in full.example_inferences.keys() {
            let reduced_diff = reduced
                .example_inferences
                .get(key)
                .unwrap_or_else(|| panic!("reduced run missing diff for {key}"));
            let full_diff = &full.example_inferences[key];
            assert_eq!(
                norm_all(&reduced_diff.asserted),
                norm_all(&full_diff.asserted),
                "asserted lines diverged for {key}"
            );
            assert_eq!(
                norm_all(&reduced_diff.inferred),
                norm_all(&full_diff.inferred),
                "reduced-seed attribution diverged from full-EDB attribution for {key}"
            );
        }
        assert_eq!(
            norm_all(&reduced.cross_example),
            norm_all(&full.cross_example),
            "cross_example diverged between reduced- and full-seed runs"
        );

        // Neither run's example diffs pick up the import-world's `ex:ImportedExtra` edge
        // — the flat probe above proved it WOULD if the worlds merged, so its absence
        // here is the world boundary holding, not the fixture being inert.
        for diff in full
            .example_inferences
            .values()
            .chain(reduced.example_inferences.values())
        {
            assert!(
                diff.inferred.iter().all(|l| !l.contains("ImportedExtra")),
                "import-world axiom leaked into example attribution: {:?}",
                diff.inferred
            );
        }
        assert!(
            reduced
                .cross_example
                .iter()
                .chain(full.cross_example.iter())
                .all(|l| !l.contains("ImportedExtra")),
            "import-world axiom leaked into cross_example"
        );
    }
}

#[cfg(test)]
mod term_entailments_tests {
    use super::*;

    /// A hand-built `reasoning-explanations.rdf12.ttl`-shaped fixture, mirroring
    /// `gmeow_logic::reason::artifacts::build_explanations_ttl`'s ACTUAL output shape:
    /// one `gmeow:Derivation` blank node with a `gmeow:concludes` quoted triple, one
    /// `gmeow:hasPremise` quoted triple, and a `gmeow:viaRule` IRI. The quoted triples
    /// use the BARE `<< s p o >>` form — exactly what `purrdf::turtle::emit_term`
    /// serializes for a `RdfTerm::Triple` — which the RDF 1.2 Turtle grammar parses as
    /// the REIFYING-triple production (a minted reifier bound via
    /// `RdfDataset::owned_reifiers()`), not as an inline `RdfTerm::Triple` object; the
    /// join must resolve through that reifier binding.
    const EXPLANATIONS_TTL: &str = "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

[] a gmeow:Derivation ;
   gmeow:concludes << <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Animal> >> ;
   gmeow:hasPremise << <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Mammal> >> ;
   gmeow:viaRule <https://blackcatinformatics.ca/gmeow/rule/subclass-transitivity> ;
   gmeow:inferenceKind gmeow:Deduction ;
   rdfs:label \"derivation of an inferred axiom\"@en ;
   gmeow:inWorld <https://blackcatinformatics.ca/gmeow/world/default> .
";

    /// The same derivation, but with the CANONICAL parenthesized `<<( s p o )>>` triple-
    /// term form — parses directly to an inline `RdfTerm::Triple` (no reifier). Both
    /// forms must resolve to the identical entailment digest.
    const EXPLANATIONS_TTL_PARENTHESIZED: &str = "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

[] a gmeow:Derivation ;
   gmeow:concludes <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Animal> )>> ;
   gmeow:hasPremise <<( <https://blackcatinformatics.ca/gmeow/Cat> rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/Mammal> )>> ;
   gmeow:viaRule <https://blackcatinformatics.ca/gmeow/rule/subclass-transitivity> ;
   gmeow:inferenceKind gmeow:Deduction ;
   rdfs:label \"derivation of an inferred axiom\"@en ;
   gmeow:inWorld <https://blackcatinformatics.ca/gmeow/world/default> .
";

    #[test]
    fn term_entailments_from_explanations_populates_matching_term_only() {
        let mut term_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        term_iris.insert("https://blackcatinformatics.ca/gmeow/Cat".to_string());

        let digest = term_entailments_from_explanations(EXPLANATIONS_TTL.as_bytes(), &term_iris)
            .expect("parse explanations fixture");

        // `Cat` is the conclusion's subject AND the premise's subject: matched once
        // (the join is a set, never a duplicate panel entry for one derivation).
        let entries = digest
            .get("https://blackcatinformatics.ca/gmeow/Cat")
            .expect("Cat must have a populated entailment panel");
        assert_eq!(entries.len(), 1, "one derivation ⇒ one panel entry");
        let entailment = &entries[0];
        assert!(
            entailment.conclusion.contains("rdfs:subClassOf"),
            "conclusion display: {}",
            entailment.conclusion
        );
        assert!(
            entailment.conclusion.contains("Animal"),
            "conclusion display: {}",
            entailment.conclusion
        );
        assert_eq!(entailment.premises.len(), 1);
        assert!(
            entailment.premises[0].contains("Mammal"),
            "premise display: {}",
            entailment.premises[0]
        );
        assert!(
            !entailment.rule.is_empty(),
            "the firing rule must never be a fabricated empty string"
        );

        // `Animal` and `Mammal` are documented terms too — a term appearing ONLY in an
        // object/premise-object position also gets the derivation's panel (any position
        // joins), so the same derivation lands on all three matched terms.
        let mut term_iris_all = term_iris.clone();
        term_iris_all.insert("https://blackcatinformatics.ca/gmeow/Animal".to_string());
        term_iris_all.insert("https://blackcatinformatics.ca/gmeow/Mammal".to_string());
        let digest_all =
            term_entailments_from_explanations(EXPLANATIONS_TTL.as_bytes(), &term_iris_all)
                .expect("parse explanations fixture (wider term set)");
        assert!(digest_all.contains_key("https://blackcatinformatics.ca/gmeow/Animal"));
        assert!(digest_all.contains_key("https://blackcatinformatics.ca/gmeow/Mammal"));

        // A term absent from the derivation entirely gets no entry (honest absence,
        // never a fabricated empty panel).
        let mut term_iris_unrelated: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        term_iris_unrelated.insert("https://blackcatinformatics.ca/gmeow/Unrelated".to_string());
        let digest_unrelated =
            term_entailments_from_explanations(EXPLANATIONS_TTL.as_bytes(), &term_iris_unrelated)
                .expect("parse explanations fixture (unrelated term)");
        assert!(digest_unrelated.is_empty());

        // The canonical parenthesized `<<( s p o )>>` form (an inline `RdfTerm::Triple`,
        // no reifier) must resolve to the IDENTICAL digest as the bare reifying form —
        // the join is agnostic to which triple-term serialization produced the data.
        let digest_parenthesized = term_entailments_from_explanations(
            EXPLANATIONS_TTL_PARENTHESIZED.as_bytes(),
            &term_iris,
        )
        .expect("parse parenthesized explanations fixture");
        assert_eq!(
            digest, digest_parenthesized,
            "bare-reifier and parenthesized triple-term forms must join identically"
        );
    }

    #[test]
    fn term_entailments_from_upstream_joins_and_hard_fails_on_missing_artifact() {
        let mut term_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        term_iris.insert("https://blackcatinformatics.ca/gmeow/Cat".to_string());

        // Positive: a synthetic `stage-reason` StageProduct carrying the explanations
        // artifact joins exactly like the pure function above.
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            EXPLANATIONS_TTL.as_bytes().to_vec(),
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", artifacts),
        );
        let digest = term_entailments_from_upstream(&upstream, &term_iris)
            .expect("digest folds from synthetic upstream");
        assert!(digest.contains_key("https://blackcatinformatics.ca/gmeow/Cat"));

        // Missing the whole stage-reason product hard-fails (never a silent empty digest).
        assert!(
            term_entailments_from_upstream(&BTreeMap::new(), &term_iris).is_err(),
            "missing stage-reason product must hard-fail"
        );

        // A declared stage-reason product present but MISSING the explanations artifact
        // (e.g. a stale/partial product) hard-fails too — never silently treated as empty.
        let mut missing_artifact: BTreeMap<String, StageProduct> = BTreeMap::new();
        missing_artifact.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", BTreeMap::new()),
        );
        assert!(
            term_entailments_from_upstream(&missing_artifact, &term_iris).is_err(),
            "a stage-reason product missing the explanations artifact must hard-fail"
        );
    }

    /// F1 (binding, production-surface non-vacuity): `term_entailments` parsed from the
    /// REAL materialized `stage-reason` explanations over the real ontology must
    /// populate ≥1 per-term inferred-facts panel — a reasoner-derived OWL/RL closure
    /// over an ontology this size entails real subsumption/property-characteristic
    /// axioms, so the panel must never ship vacuous. Runs the real
    /// `source_load` → `statements` / `compile_logic` → `mappings` → `reason` stage
    /// chain directly (each `Stage::run` call is pure in-memory — no disk write; the
    /// committed `generated/` tree is untouched), mirroring the chaining pattern
    /// `mappings::projection_report_unions_logic_and_correspondence_rows` already uses.
    #[test]
    fn term_entailments_are_non_vacuous_on_the_real_repo() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let empty: BTreeMap<String, StageProduct> = BTreeMap::new();

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let source_load = crate::stages::source_load::SourceLoadStage::new()
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real source-load");
        upstream.insert("stage-source-load".to_string(), source_load.product);

        let statements = crate::stages::statements::StatementsStage
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real statements");
        upstream.insert("stage-statements".to_string(), statements.product);

        let compile = crate::stages::compile_logic::CompileLogicStage::new()
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real compile-logic");
        upstream.insert("stage-compile-logic".to_string(), compile.product);

        let mappings = crate::stages::mappings::MappingsStage::new()
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("real mappings");
        upstream.insert("stage-mappings".to_string(), mappings.product);

        let reason = crate::stages::reason::ReasonStage::new()
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("real reason");
        upstream.insert("stage-reason".to_string(), reason.product);

        let model =
            gmeow_docs::model::DocsModel::discover(&root).expect("real docs model discovery");
        let carrier = source_load_dataset(&upstream).expect("source-load dataset");
        let data = build_executable_docs_data(&upstream, carrier.as_ref(), &model)
            .expect("real executable docs data");

        assert!(
            !data.term_entailments.is_empty(),
            "term_entailments must be non-vacuous on the real repo (F1) — if this trips, \
             investigate whether the join logic (not merely real-data sparsity) is at fault"
        );
        let (term_iri, entailments) = data
            .term_entailments
            .iter()
            .next()
            .expect("at least one populated panel");
        assert!(
            !entailments.is_empty(),
            "panel for {term_iri} must be non-empty"
        );
        assert!(!entailments[0].conclusion.is_empty());
        assert!(!entailments[0].rule.is_empty());
    }
}

#[cfg(test)]
mod quality_assessment_tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const QUALITY_ASSESSMENT_CLASS: &str = "https://blackcatinformatics.ca/gmeow/QualityAssessment";

    /// Count `?s a gmeow:QualityAssessment` in a projected graph.
    fn count_quality_assessments(ds: &purrdf::RdfDataset) -> usize {
        ds.owned_quads()
            .filter(|q| {
                q.predicate.as_str() == RDF_TYPE
                    && matches!(&q.object, RdfTerm::Iri(o) if o == QUALITY_ASSESSMENT_CLASS)
            })
            .count()
    }

    #[test]
    fn quality_assessment_graph_rides_the_self_description_carrier_heavy_offgate() {
        // G2 dogfooding: scoring every slice attaches the `graph/quality-assessment`
        // named graph to the source-load self-description carrier, so it folds into
        // gmeow.gts (the presenter reads it back and also emits the fanout twin).
        //
        // Off-gate (`_heavy_offgate`): builds the full self-description carrier, which
        // scores all ~81 slices (~86 s) — irreducibly O(slice count), the same
        // whole-repo class as `end_to_end`/`fold_parity`. The attach↔fanout bijection
        // and N-Triples fold form stay on-gate via the fast sibling tests
        // `quality_assessment_fanout_path_is_registered_and_folds_as_ntriples` and
        // `superset::tests::quality_assessment_nt_folds_as_ntriples_via_its_own_fanout_graph`,
        // and any real drift is caught on every `make check` by `make check-generated`.
        let root = repo_root();
        let ds = build_self_description_dataset(&root).expect("self-description dataset");

        let base = ds.project_named_graph(GRAPH_QUALITY_ASSESSMENT);
        assert!(
            base.quad_count() > 0,
            "graph/quality-assessment must carry the scored slice corpus"
        );
        let n = count_quality_assessments(&base);
        assert!(
            n >= 1,
            "graph/quality-assessment must carry real gmeow:QualityAssessment triples, got {n}"
        );

        // The fanout twin (the superset gate's on-disk fold) carries the SAME triples
        // re-rooted into the `graph/fanout/<path>` reconstruction container.
        let fanout_iri = crate::stages::superset::rdf_fanout_graph_iri(QUALITY_ASSESSMENT_PATH)
            .expect("quality-assessment path is an RDF path");
        let fanout = rooted_in_graph(&base, &fanout_iri).expect("re-root into fanout container");
        assert_eq!(
            count_quality_assessments(&fanout.project_named_graph(&fanout_iri)),
            n,
            "the fanout twin must carry the same QualityAssessment triples as the base graph"
        );
    }

    #[test]
    fn quality_assessment_fanout_path_is_registered_and_folds_as_ntriples() {
        // The attach ↔ committed-path bijection the superset gate enforces: the carrier
        // attaches `graph/fanout/quality/gmeow.quality-assessment.nt` and the gate claims
        // the same committed path as an N-Triples fanout fold. A committed path with no
        // attaching stage (or vice-versa) is a wiring contradiction — this pins both legs.
        assert!(
            crate::stages::superset::is_rdf_fanout_class(QUALITY_ASSESSMENT_PATH),
            "the quality-assessment committed path must be a registered RDF-fanout class"
        );
        let iri = crate::stages::superset::rdf_fanout_graph_iri(QUALITY_ASSESSMENT_PATH)
            .expect("committed path yields a fanout graph IRI");
        assert_eq!(
            crate::stages::superset::rdf_fanout_path_for_graph_iri(&iri).as_deref(),
            Some(QUALITY_ASSESSMENT_PATH),
            "the fanout IRI must invert back to the committed path (bijection)"
        );
    }
}

#[cfg(test)]
mod coherence_certificate_tests {
    use super::*;
    use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
    use gmeow_logic::result_rdf::{GRAPH_REASONING, project_reasoning_result};
    use purrdf::RdfTerm;
    use std::sync::Arc;

    /// Wrap a reasoned result as a `stage-reason` product carrying the typed Reasoning
    /// handle pinned to `graph/reasoning` — the SAME shape `stage-reason` emits, which
    /// `fold_coherence_certificate` reuses instead of reasoning a second time.
    fn reason_product(result: &gmeow_logic::result::ReasoningResult) -> StageProduct {
        let reasoning_nt = project_reasoning_result(result);
        let reasoning_ds =
            parse_dataset(reasoning_nt.as_bytes(), "application/n-triples", None).unwrap();
        let mut b = RdfDatasetBuilder::new();
        let g = RdfTerm::Iri(GRAPH_REASONING.to_owned());
        for q in reasoning_ds.owned_quads() {
            let mut routed = q.clone();
            routed.graph_name = Some(g.clone());
            b.push_owned_quad(&routed);
        }
        let dataset = b.freeze().unwrap();
        let mut bundle =
            bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
        let pinned = bundle.graph_digest(GRAPH_REASONING);
        bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(result.clone())),
                pinned,
            )
            .unwrap();
        StageProduct::from_bundle("stage-reason", Arc::new(bundle))
    }

    /// `fold_coherence_certificate` folds a `graph/attestations` coherence artifact over the
    /// composed carrier, REUSING `stage-reason`'s single reasoning pass (never re-reasoning),
    /// so every terminal gmeow.gts carries the certificate the consumer read tool surfaces.
    #[test]
    fn fold_attaches_a_coherence_artifact_to_graph_attestations() {
        // A tiny consistent EDB → a real reasoned result (no forbidden violation).
        let edb = concat!(
            "<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> ",
            "<http://example.org/B> <http://gmeow.example/w> .\n"
        );
        let reasoned = crate::stages::reason::reason_artifacts(edb.as_bytes()).expect("reason");
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-reason".to_string(), reason_product(&reasoned.result));

        let composed = parse_dataset(edb.as_bytes(), "application/n-quads", None).unwrap();
        let folded = fold_coherence_certificate(composed, &upstream).expect("fold certificate");

        let attestations = folded.project_named_graph(crate::stages::release::GRAPH_ATTESTATIONS);
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let coherence_typed = attestations.owned_quads().any(|q| {
            q.predicate == rdf_type
                && matches!(&q.object, RdfTerm::Iri(o) if o.contains("Coherence"))
        });
        assert!(
            coherence_typed,
            "the fold must attach a typed logic:Coherence* artifact to graph/attestations"
        );
        // The certificate pins a real bundle identity + per-graph axiom digest (the tamper
        // surface), so the read tool can surface non-trivial hashes.
        let has_bundle_hash = attestations.owned_quads().any(|q| {
            q.predicate == "https://blackcatinformatics.ca/logic/bundleHash"
                && matches!(&q.object, RdfTerm::Literal(l) if !l.lexical_form.is_empty())
        });
        assert!(has_bundle_hash, "the folded certificate pins a bundle hash");

        // Deterministic: re-folding the same carrier + result is byte-identical.
        let composed2 = parse_dataset(edb.as_bytes(), "application/n-quads", None).unwrap();
        let folded2 = fold_coherence_certificate(composed2, &upstream).expect("fold again");
        let nq1 = purrdf::canonical_flat_nquads(folded.as_ref()).unwrap();
        let nq2 = purrdf::canonical_flat_nquads(folded2.as_ref()).unwrap();
        assert_eq!(nq1, nq2, "the folded certificate is deterministic");
    }
}
