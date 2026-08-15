// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage: the sole serialization exit — the gts
//! narrow waist.
//!
//! Exactly one Sink per pipeline. The STRUCTURED multi-named-graph `dist`
//! snapshot is ASSEMBLED upstream by [`crate::stages::carrier::SnapshotStage`]
//! (fold-isomorphic to the committed `generated/dist/gmeow.gts`, the parity
//! gate). This sink consumes that one `stage-snapshot` product and re-emits its
//! `gmeow.gts` bytes as the sink artifact — the single, well-defined disk-write
//! the `run_full` orchestration performs. Splitting the assembly (a Transform)
//! from the serialization exit (this Sink) is what lets every fold-reading export
//! leaf consume THIS run's freshly-composed fold rather than the stale committed
//! file (the single-pass invariant).

use std::collections::BTreeMap;

use crate::node::{SINK_CAPABILITY, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::SNAPSHOT_PATH;

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = SNAPSHOT_PATH;

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {
    consumes: Vec<String>,
    capabilities: Vec<String>,
}

impl GtsSinkStage {
    /// Construct the sink. It consumes the assembled carrier (`stage-snapshot`), the
    /// already-folded by-reference TAR archives (`stage-archive-blobs`), and the blob
    /// sources it staples itself: the in-memory reasoning / SHACL-report products and the
    /// opaque `generated/` fanout members read off their producing export leaves.
    /// It holds [`SINK_CAPABILITY`] — the sole serialization exit the loader requires
    /// exactly one stage to hold (mirrored by the slice
    /// `gmeow:stage-gts-sink gmeow:hasCapability gmeow:sinkCapability`).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-snapshot".to_string(),
                // THIS run's eleven by-reference TAR archives (mappings / cells / queries
                // / tests / schemas / shapes / axioms / models-python / lang-projections
                // / statements / yaml-ld), folded once by
                // their own producer — the terminal reads them off that product and
                // re-folds nothing (PIPELINE_SPINE §3.2/§4). The edge also orders the
                // sink after every archive-member producer transitively, so the
                // JSON-Schema / Pydantic / generated-shape leaves need no direct edge.
                "stage-archive-blobs".to_string(),
                // THIS run's SEVEN trained zstd dictionaries and their
                // gmeow:CompressionDictionaryRealization records. The terminal is the one
                // point where the whole frame set exists, so it pins the dictionaries in
                // the pack's in-band "dct" map and seals one gmeow:MediumEnvelope per
                // frame it authors.
                //
                // Seven, not eight: there is no gmeow-math-v1. A dictionary primes a
                // FRAME, and every math: named graph is unioned into the snapshot
                // payload — one frame, already primed in full by gmeow-core-v1, and
                // gmeow:payloadSchemaDictionary is maxQualifiedCardinality 1, so a second
                // dictionary on that frame is not merely unhelpful but unrepresentable.
                // No mathematical byte family exists to give one instead: the archive
                // fold takes dsl/mappings/**, the per-slice mappings/ and tests/ trees,
                // and the shape surfaces — slices/grounding/math/** reaches the bundle
                // ONLY as parsed RDF in the fold. So the mathematical content is fully
                // dictionary-compressed, by gmeow-core-v1, and nothing is lost.
                "stage-medium-dictionaries".to_string(),
                // The executable-docs "try it" surface reasons over the object-level EDB,
                // whose authored / imports / alignments graphs ride on the source-load
                // product (read, not re-loaded from disk).
                "stage-source-load".to_string(),
                "stage-compile-logic".to_string(),
                "stage-mappings".to_string(),
                "stage-reason".to_string(),
                // NOT `stage-statements`. The terminal used to staple the statement
                // layer's two byte-decorated projections into the generated-opaque
                // archive; they ride `statements-archive` now, folded by
                // `stage-archive-blobs` off that same product, so the sink reads nothing
                // from it and an edge nothing reads is removed rather than left standing.
                // The ORDERING it used to carry is unchanged: `stage-archive-blobs`
                // consumes `stage-statements`, and the sink consumes that.
                "stage-validate".to_string(),
                // The opaque fanout members ride in from their producing export leaves
                // (each rendered once, in the leaf); `collect_fanout_opaque_members` reads them
                // off these products instead of re-rendering from disk (§3.2/§4).
                "stage-export-agreement".to_string(),
                "stage-export-apache".to_string(),
                "stage-export-bench".to_string(),
                "stage-export-cost-ledger".to_string(),
                "stage-export-evals".to_string(),
                // The OntoLex vartrans terminology lowering: an RDF fanout named graph
                // folded from this run's fresh export-leaf product. (Its two NON-RDF
                // siblings — the glossary table and the TBX termbase — ride
                // `lang-projections-archive`, folded by `stage-archive-blobs`, not here.)
                "stage-export-glossary".to_string(),
                "stage-export-matrix".to_string(),
                "stage-export-metadata".to_string(),
                "stage-export-references".to_string(),
                "stage-export-research-objects".to_string(),
                // The LinkML/TypeScript/GraphQL developer schema surfaces: co-derived
                // from the same fresh shape compilation as json-schema/pydantic, folded
                // into REP_GENERATED from THIS run's fresh product (never re-derived
                // from the in-memory carrier — schemas is no longer carrier-projectable).
                "stage-export-schemas".to_string(),
                // The two slice-quality floor TSVs (P17 projection of the ontology
                // gmeow:AxisFloorCommitment / gmeow:SliceTierFloor individuals): opaque
                // REP_GENERATED fanout members read off this leaf's product.
                "stage-export-governance-floors".to_string(),
                // The two projection-vocabulary ratchet TSVs (P17 projection of the
                // ontology gmeow:ProjectionCeilingCommitment / gmeow:ProjectionVocabulary
                // individuals): opaque REP_GENERATED fanout members read off this leaf's
                // product.
                "stage-export-projection-ceilings".to_string(),
            ],
            capabilities: vec![SINK_CAPABILITY.to_string()],
        }
    }
}

impl Default for GtsSinkStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GtsSinkStage {
    fn id(&self) -> &str {
        "stage-gts-sink"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    fn impl_version(&self) -> &str {
        // v4: the opaque fanout members (references / bench / apache / matrix / eval +
        // research-object sidecars / metadata) ride in from their producing export leaves;
        // `collect_fanout_opaque_members` reads them off those products instead of re-rendering
        // from disk, and dsl-stats / context ride off the already-consumed
        // stage-mappings product (§3.2 transform-once, §4 pure terminal).
        // v5: REP_SHAPES' generated members (result-shapes.ttl + frame-shapes.ttl)
        // are folded from the consumed export-leaf products instead of a stale
        // disk read, matching the validation-shapes.ttl freshness rule.
        // v6: the by-reference TAR archives are no longer folded here at all —
        // they are READ off the `stage-archive-blobs` product (the fold moved to its
        // own stage so the archives exist mid-DAG).
        // v7: the generated-opaque archive SHEDS four members — the statement layer's
        // two byte projections and the two non-RDF terminology surfaces — which now ride
        // `statements-archive` / `lang-projections-archive`. The emitted bytes and the
        // emitted `opaque` fanout manifest both change, so the key moves.
        "gts_sink.v7-statements-and-terminology-off-the-opaque-archive"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // The terminal gts ARCHIVE writer: serialize THIS run's carrier
        // into the single `gmeow.gts` package. GTS is exit-only — produced HERE and
        // nowhere else; every internal export leaf reads the carrier dataset off the
        // snapshot product's bundle, never these bytes. The carrier is taken off the
        // bundle (no re-assembly — the razor: transform transport→form once); the
        // by-reference TAR archives are READ off the `stage-archive-blobs` product and
        // stapled alongside it, never re-folded here.
        let carrier = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        let gts = crate::stages::carrier::serialize_carrier_snapshot(
            input.root,
            input.upstream,
            carrier.as_ref(),
            &crate::medium::registry::MediumSelection::Authored,
        )?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(GTS_PATH.to_string(), gts);
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn sink_serializes_the_snapshot_carrier_with_blob_inputs() {
        // Build the minimal upstream product set the sink requires: a snapshot
        // carrier plus by-reference blob sources. Full real-DAG coverage lives in
        // the end-to-end pipeline test; this unit test pins the sink's fail-closed
        // artifact wiring without paying for reasoning and snapshot assembly.
        let root = repo_root();
        // The carrier folds the AUTHORED gts slice, because the terminal is now a
        // medium-aware emitter: it reads the rep→medium assignment, the declared
        // dictionaries, and the two declared media off the carrier it is serializing.
        // A carrier with four triples and no medium axis would make the sink refuse
        // (gmeow:MediumUnknownSchema) — correctly, since every emitted rep must
        // resolve — so the fixture carries the declarations the production carrier
        // always carries.
        let mut carrier_ttl = std::fs::read_to_string(root.join("slices/core/gts/module.ttl"))
            .expect("the gts slice is readable");
        carrier_ttl.push_str(
            "<https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/description> \"test bundle\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             <https://example.org/s> <https://example.org/p> <https://example.org/o> .\n",
        );
        let carrier = purrdf::parse_dataset(carrier_ttl.as_bytes(), "text/turtle", None)
            .expect("minimal carrier dataset");
        let medium = crate::stages::medium_dictionaries::test_product_over(carrier.as_ref())
            .expect("the fixture medium product trains over the declared dictionaries");
        let snapshot =
            StageProduct::from_artifacts_over("stage-snapshot", carrier, BTreeMap::new());

        // A minimal source-load product: the executable-docs "try it" EDB reads the
        // authored / imports / alignments graphs off it (empty here — this unit test
        // pins the sink's fail-closed wiring, not a real reasoned closure).
        let mut source_load_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        source_load_artifacts.insert(
            crate::stages::carrier::SLICE_QUALITY_REPORT_HTML_ARTIFACT.to_string(),
            b"<!doctype html><title>slice-quality</title>\n".to_vec(),
        );
        let source_load = StageProduct::from_artifacts_over(
            "stage-source-load",
            purrdf::parse_dataset(b"", "application/n-quads", None)
                .expect("empty source-load dataset"),
            source_load_artifacts,
        );

        let mut compile_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for path in [
            "generated/owl/gmeow-dl.ttl",
            "generated/owl/gmeow-el.ttl",
            "generated/logic/gmeow.logic.rdf12.ttl",
            "generated/datalog/gmeow.dl",
            // The SHACL-AF rule (computation) surface the generated-fanout archive pulls
            // from the compile-logic product (design/LOGIC-SHACL-AF.md).
            crate::stages::compile_logic::SHACL_AF_PATH,
            // The validation-shape surfaces (SHACL Core + ShEx) — the OPT/ADL constraints axis.
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
            crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH,
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            crate::stages::compile_logic::N3_PATH,
            crate::stages::compile_logic::CLIF_PATH,
            crate::stages::compile_logic::CGIF_PATH,
            crate::stages::compile_logic::XCL_PATH,
            crate::stages::compile_logic::GUFO_PATH,
            crate::stages::compile_logic::RELATIONAL_CORE_PATH,
            crate::stages::compile_logic::CORRESPONDENCE_PATH,
            crate::stages::compile_logic::DIAG_JSON_PATH,
            crate::stages::compile_logic::DIAG_SARIF_PATH,
            crate::stages::compile_logic::DIAG_HTML_PATH,
            crate::stages::compile_logic::DIAG_RDF_PATH,
        ] {
            compile_artifacts.insert(path.to_string(), Vec::new());
        }
        let compile = StageProduct::from_artifacts("stage-compile-logic", compile_artifacts);

        let mut json_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        json_artifacts.insert(
            crate::stages::json_schema::JSON_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        json_artifacts.insert(
            crate::stages::json_schema::OPENAPI_PATH.to_string(),
            b"{}".to_vec(),
        );
        json_artifacts.insert(
            crate::stages::json_schema::CARD_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        json_artifacts.insert(
            crate::stages::json_schema::FINDING_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        let json_schema = StageProduct::from_artifacts("stage-export-json-schema", json_artifacts);

        let mut mapping_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        mapping_artifacts.insert(
            crate::stages::compile_logic::PROJECTION_REPORT_PATH.to_string(),
            b"@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<https://example.org/projection-report> a owl:Ontology .\n".to_vec(),
        );
        // REP_MAPPINGS folds the SSSOM surface from this product (fail-closed: an
        // empty match is a hard error, so the minimal set carries one file).
        mapping_artifacts.insert(
            "generated/mappings/gmeow-test.sssom.tsv".to_string(),
            b"# minimal sssom\n".to_vec(),
        );
        // REP_QUERIES folds the generated SPARQL surface from this product (fail-closed,
        // same rule as SSSOM: a `.rq` edit must reach the bundle in one regenerate).
        mapping_artifacts.insert(
            "generated/queries/gmeow-test.rq".to_string(),
            b"# minimal query\n".to_vec(),
        );
        // REP_LANG_PROJECTIONS folds the `generated/projections/lang/**` deliverables from
        // this SAME product (fail-closed, same rule again): they no longer ride the
        // generated-opaque archive, because a rep is the unit a dictionary primes and
        // these external-format deliverables are a family a consumer extracts on its
        // own. Two members under different sub-trees, so the fixture exercises the
        // nested repo-relative member keying rather than a single flat file.
        mapping_artifacts.insert(
            "generated/projections/lang/ebnf/gmeow-test.ebnf".to_string(),
            b"(* minimal grammar *)\n".to_vec(),
        );
        mapping_artifacts.insert(
            "generated/projections/lang/gmn1/v1/gmeow-test.gmn".to_string(),
            b"# minimal gmn\n".to_vec(),
        );
        // The fanout presenter reads dsl-stats + the JSON-LD context + the EmotionML XML
        // projection off this product.
        mapping_artifacts.insert(
            crate::stages::mappings::DSL_STATS_PATH.to_string(),
            b"{}".to_vec(),
        );
        mapping_artifacts.insert(
            crate::stages::mappings::JSONLD_CONTEXT_PATH.to_string(),
            b"{}".to_vec(),
        );
        mapping_artifacts.insert(
            crate::stages::mappings::EMOTIONML_PATH.to_string(),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emotionml/>\n".to_vec(),
        );
        let mappings = StageProduct::from_artifacts("stage-mappings", mapping_artifacts);

        let mut reason_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        reason_artifacts.insert(
            crate::stages::reason::CLOSURE_PATH.to_string(),
            b"# closure".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            b"# explanations".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::LEDGER_PATH.to_string(),
            b"# ledger".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::PERF_LEDGER_PATH.to_string(),
            b"# perf".to_vec(),
        );
        let reason = StageProduct::from_artifacts("stage-reason", reason_artifacts);

        let mut statement_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        statement_artifacts.insert(
            crate::stages::statements::OWL_PATH.to_string(),
            b"# statements owl".to_vec(),
        );
        statement_artifacts.insert(
            crate::stages::statements::RDF12_PATH.to_string(),
            b"# statements rdf12".to_vec(),
        );
        // The claim corpus's JSON-LD-family surface on its INTERNAL lane — what the
        // `yaml-ld-archive` fold tars. Non-empty so that fold clears its fail-closed
        // guard; the bytes themselves are immaterial to the sink wiring this pins.
        statement_artifacts.insert(
            crate::stages::statements::RDF12_JSONLD_PATH.to_string(),
            br#"{"@context":{},"@graph":[]}"#.to_vec(),
        );
        statement_artifacts.insert(
            crate::stages::statements::RDF12_YAMLLD_PATH.to_string(),
            b"'@context': {}\n".to_vec(),
        );
        let statements = StageProduct::from_artifacts("stage-statements", statement_artifacts);

        let mut validate_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        validate_artifacts.insert(
            crate::stages::validate::SHACL_JSON_PATH.to_string(),
            b"{}".to_vec(),
        );
        validate_artifacts.insert(
            crate::stages::validate::SHACL_SARIF_PATH.to_string(),
            b"{}".to_vec(),
        );
        validate_artifacts.insert(
            crate::stages::validate::SHACL_HTML_PATH.to_string(),
            b"".to_vec(),
        );
        let validate = StageProduct::from_artifacts("stage-validate", validate_artifacts);

        // The generated shape surfaces are required products (fail-closed):
        // REP_SHAPES folds them from the in-memory products, never a disk read.
        let mut result_shapes_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        result_shapes_artifacts.insert(
            crate::stages::result_shapes::RESULT_SHAPES_PATH.to_string(),
            b"# result shapes".to_vec(),
        );
        let result_shapes =
            StageProduct::from_artifacts("stage-export-result-shapes", result_shapes_artifacts);
        let mut frame_shapes_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        frame_shapes_artifacts.insert(
            crate::stages::frame_shapes::FRAME_SHAPES_PATH.to_string(),
            b"# frame shapes".to_vec(),
        );
        let frame_shapes =
            StageProduct::from_artifacts("stage-export-frame-shapes", frame_shapes_artifacts);
        let mut constraint_shapes_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        constraint_shapes_artifacts.insert(
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH.to_string(),
            b"# constraint shapes".to_vec(),
        );
        let constraint_shapes = StageProduct::from_artifacts(
            "stage-export-constraint-shapes",
            constraint_shapes_artifacts,
        );

        // The opaque-fanout export leaves: the presenter reads their rendered members off
        // these products (empty here — this unit test pins the sink's fail-closed wiring,
        // not a real fanout; the superset gate is exercised end-to-end in fanout_parity).
        let export_leaves: Vec<StageProduct> = [
            "stage-export-agreement",
            "stage-export-bench",
            "stage-export-cost-ledger",
            "stage-export-apache",
            "stage-export-matrix",
            "stage-export-evals",
            "stage-export-research-objects",
            "stage-export-metadata",
            "stage-export-governance-floors",
            "stage-export-projection-ceilings",
            // The LinkML/TypeScript/GraphQL developer schema surfaces: read off this
            // leaf's product (empty here — this unit test pins the sink's fail-closed
            // wiring, not the real schema render).
            "stage-export-schemas",
        ]
        .into_iter()
        .map(|id| StageProduct::from_artifacts(id, BTreeMap::new()))
        .collect();

        // The references export leaf carries THIS run's generated `references.bib`, which
        // `build_docs_print_blob` folds into the print PDF's bibliography — a minimal valid
        // BibTeX database so the print-blob wiring is exercised fail-closed.
        let mut references_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        references_artifacts.insert(
            crate::stages::references::BIB_PATH.to_string(),
            b"@article{gmeow2026,\n  title = {The GMEOW Ontology},\n  author = {Audley, Patrick},\n  year = {2026},\n}\n".to_vec(),
        );
        let references =
            StageProduct::from_artifacts("stage-export-references", references_artifacts);

        // The glossary export leaf is NOT empty like its neighbours: two of its three
        // surfaces are members of `lang-projections-archive`, whose fold is fail-closed on
        // each of them by name. A minimal non-empty pair therefore exercises that wiring
        // rather than tripping it. (`.vartrans.ttl` is deliberately absent: it is RDF and
        // rides its own named graph, which this unit test does not assemble.)
        let mut glossary_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        glossary_artifacts.insert(
            crate::stages::lang_glossary::GLOSSARY_TABLE_PATH.to_string(),
            b"<!-- GENERATED -->\n| term | gloss |\n".to_vec(),
        );
        glossary_artifacts.insert(
            crate::stages::lang_glossary::GLOSSARY_TBX_PATH.to_string(),
            b"<?xml version=\"1.0\"?><martif/>\n".to_vec(),
        );
        let glossary = StageProduct::from_artifacts("stage-export-glossary", glossary_artifacts);

        // The Pydantic model package product: a minimal non-empty member so the
        // models-python blob fold clears its fail-closed guard (the carrier folds
        // this run's fresh package, never a stale disk read).
        let mut pydantic_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        pydantic_artifacts.insert(
            format!(
                "{}gmeow_models/__init__.py",
                crate::stages::pydantic::PACKAGE_DISK_PREFIX
            ),
            b"# gmeow_models\n".to_vec(),
        );
        let pydantic = StageProduct::from_artifacts("stage-export-pydantic", pydantic_artifacts);

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-export-references".to_string(), references);
        upstream.insert("stage-export-glossary".to_string(), glossary);
        upstream.insert("stage-export-pydantic".to_string(), pydantic);
        upstream.insert("stage-compile-logic".to_string(), compile);
        upstream.insert("stage-export-json-schema".to_string(), json_schema);
        upstream.insert("stage-mappings".to_string(), mappings);
        upstream.insert("stage-reason".to_string(), reason);
        upstream.insert("stage-snapshot".to_string(), snapshot);
        upstream.insert(
            crate::stages::medium_dictionaries::STAGE_ID.to_string(),
            medium,
        );
        upstream.insert("stage-source-load".to_string(), source_load);
        upstream.insert("stage-statements".to_string(), statements);
        upstream.insert("stage-validate".to_string(), validate);
        upstream.insert("stage-export-result-shapes".to_string(), result_shapes);
        upstream.insert("stage-export-frame-shapes".to_string(), frame_shapes);
        upstream.insert(
            "stage-export-constraint-shapes".to_string(),
            constraint_shapes,
        );
        for product in export_leaves {
            upstream.insert(product.stage_id.clone(), product);
        }
        // The by-reference TAR archives arrive as their OWN stage's product now: fold
        // them over this same fixture upstream and insert the result, exactly as the real
        // DAG does. The sink then READS them (never re-folds), so this also pins that the
        // sink's fail-closed archive wiring is the product read, not an inline build.
        let archives = crate::stages::archive_blobs::ArchiveBlobsStage::new()
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("the archive-blobs stage folds the fixture archives");
        upstream.insert(
            crate::stages::archive_blobs::STAGE_ID.to_string(),
            archives.product,
        );
        // Drive the PRODUCTION serializer over the fixture carrier under the authored
        // medium — the same door and the same selection `run()` takes, so this test's
        // subject (the sink's fail-closed blob/serialization wiring) is exercised on the
        // shipped path rather than on a sibling one. The docs corpus is deliberately not
        // discovered here: documentation projections are external artifacts the terminal
        // never embeds, so the fixture pays nothing for them. OKF coverage is a separate
        // gate, exercised by `okf_link_targets_missing_from` (populated links) and by the
        // end-to-end pipeline test over the real full carrier — which is also what
        // exercises the thin `run()` wrapper around this call.
        let carrier =
            crate::stages::carrier::snapshot_dataset(&upstream).expect("snapshot carrier");
        let emitted = crate::stages::carrier::serialize_carrier_snapshot(
            &root,
            &upstream,
            carrier.as_ref(),
            &crate::medium::registry::MediumSelection::Authored,
        )
        .expect("sink serializes the carrier");
        assert!(
            emitted.len() > 1024,
            "GTS bundle implausibly small: {} bytes",
            emitted.len()
        );

        // Round-trips through the kernel GTS importer (the bundle is well-formed).
        let folded = purrdf::import_gts_events(&emitted).expect("import_gts_events");

        // ── the MEDIUM the terminal emitted through ──
        // The fast twin of the whole-bundle gate (`tests/medium_bundle.rs`): the same
        // invariants, over a fixture small enough to iterate on. The whole-DAG gate is
        // what proves them on the SHIPPED artifact; this one keeps the sink's medium
        // wiring covered in the focused lane the sink's other contracts live in.
        let pinned = gmeow_gts_profile::segment_dictionaries(&emitted)
            .expect("the emitted bundle's header reads back");
        assert_eq!(
            pinned.len(),
            6,
            "the pack pins every declared dictionary in band; got {:?}",
            pinned.keys().collect::<Vec<_>>()
        );
        // The lang-projection deliverables ride their OWN primed rep, not the
        // generated-opaque archive: the fixture's two `generated/projections/lang/**`
        // members must reach the bundle as a `lang-projections-archive` frame rather
        // than being folded into the general archive.
        assert!(
            purrdf::gts::lookaside_from_graph(
                &purrdf::gts::read_graph(&emitted, true).expect("the blob lane reads")
            )
            .blobs
            .iter()
            .any(|record| record.representation.as_deref() == Some("lang-projections-archive")),
            "the emitted pack carries no lang-projections-archive frame"
        );

        let payload = purrdf::flat_rdf_quads_from_dataset(folded.dataset.as_ref());
        let registry_graph = Some(purrdf::RdfTerm::iri(crate::medium::MEDIUM_REGISTRY_GRAPH));
        let typed = |class: &str| -> std::collections::BTreeSet<String> {
            let object =
                purrdf::RdfTerm::iri(format!("https://blackcatinformatics.ca/gmeow/{class}"));
            payload
                .iter()
                .filter(|q| {
                    q.graph_name == registry_graph
                        && q.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                        && q.object == object
                })
                .filter_map(|q| match &q.subject {
                    purrdf::RdfTerm::Iri(iri) => Some(iri.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            typed("CompressionDictionaryRealization").len(),
            6,
            "one realization per declared dictionary, IN graph/medium-registry"
        );
        let envelopes = typed("MediumEnvelope");
        let payload_frames = purrdf::gts::wire::iter_items(&emitted)
            .0
            .iter()
            .filter(|(_, item)| match item {
                ciborium::value::Value::Map(entries) => {
                    purrdf::gts::wire::map_get(entries, "gts").is_none()
                        && purrdf::gts::wire::map_get(entries, "d").is_some()
                }
                _ => false,
            })
            .count();
        assert!(!envelopes.is_empty());
        assert_eq!(
            envelopes.len(),
            payload_frames,
            "one gmeow:MediumEnvelope per payload-bearing frame"
        );

        // The snapshot envelope's stratified digest, recomputed from the emitted
        // payload: the stratum is the payload MINUS the envelope subgraph, and the
        // content digest is `snapshot_content_id()` over exactly that region.
        let stratum = crate::stages::carrier::snapshot_stratum_quads(folded.dataset.as_ref());
        let envelope_quads = crate::stages::carrier::medium_envelope_quads(folded.dataset.as_ref());
        assert!(!stratum.is_empty() && !envelope_quads.is_empty());
        assert_eq!(stratum.len() + envelope_quads.len(), payload.len());
        let strata_digest = crate::medium::blake3_digest(
            crate::stages::carrier::snapshot_stratum_nquads(folded.dataset.as_ref())
                .expect("the stratum canonicalizes")
                .as_bytes(),
        );
        let snapshot_envelope = envelopes
            .iter()
            .find(|subject| {
                payload.iter().any(|q| {
                    q.subject == purrdf::RdfTerm::iri(subject.as_str())
                        && q.predicate
                            == "https://blackcatinformatics.ca/gmeow/envelopeDigestStratum"
                        && q.object
                            == purrdf::RdfTerm::iri(
                                "https://blackcatinformatics.ca/gmeow/\
                                 stratumPayloadExcludingMediumEnvelope",
                            )
                })
            })
            .expect("the snapshot frame carries a stratified envelope");
        let literal = |subject: &str, predicate: &str| -> String {
            payload
                .iter()
                .find(|q| {
                    q.subject == purrdf::RdfTerm::iri(subject)
                        && q.predicate
                            == format!("https://blackcatinformatics.ca/gmeow/{predicate}")
                })
                .and_then(|q| match &q.object {
                    purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("<{subject}> carries no gmeow:{predicate}"))
        };
        assert_eq!(
            literal(snapshot_envelope, "strataDigest"),
            strata_digest,
            "the snapshot envelope's strata digest commits to its declared stratum"
        );
        // The content digest is `snapshot_content_id()` VERBATIM, and the frame identity
        // is derived from it — so a digest that were not the payload's own id would
        // address a frame nothing describes. That derivation is what makes the reuse
        // checkable from the artifact; the payload's CBOR itself cannot be re-derived by
        // a reader, because folding the graph back re-interns its blank nodes (which is
        // exactly why the checkable commitment is the blank-node-canonical STRATUM).
        let content_digest = literal(snapshot_envelope, "contentDigest");
        assert!(
            crate::medium::is_canonical_digest(&content_digest),
            "the snapshot content digest must be canonical: {content_digest}"
        );
        assert_eq!(
            payload
                .iter()
                .find(
                    |q| q.subject == purrdf::RdfTerm::iri(snapshot_envelope.as_str())
                        && q.predicate
                            == "https://blackcatinformatics.ca/gmeow/envelopePayloadFrame"
                )
                .map(|q| q.object.clone()),
            Some(purrdf::RdfTerm::iri(
                crate::stages::medium_dictionaries::frame_iri(
                    crate::medium::SNAPSHOT_WIRE_REP,
                    &content_digest,
                )
            )),
            "the snapshot frame identity is derived from the content digest the envelope \
             carries, so the digest is the payload's own id rather than a free value"
        );
        assert_ne!(
            literal(snapshot_envelope, "contentDigest"),
            literal(snapshot_envelope, "strataDigest"),
            "the stratum digest is an addition to the witness, not a rename of it"
        );
    }
}
