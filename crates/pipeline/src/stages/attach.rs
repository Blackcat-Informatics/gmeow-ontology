// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single Rust-side authority for each stage's ATTACH declaration — the named
//! graphs and blob-representation lanes a stage contributes to the carrier as its
//! delta (PIPELINE_SPINE §3 rule 3: "Declare contributions").
//!
//! Each attaching stage's [`Stage::attaches_graphs`](crate::node::Stage::attaches_graphs)
//! / [`Stage::attaches_blob_reps`](crate::node::Stage::attaches_blob_reps) returns a
//! slice from this one table (keyed by stage id), and
//! [`crate::run::full_spec`] fills the `StageSpec` attach fields from the SAME bound
//! impls — so the Rust side has a single definition. The slice `module.ttl` mirrors
//! it with `gmeow:attachesGraph` / `gmeow:attachesBlobRep` triples, and the dogfooding
//! parity gate (`tests/dag_dogfood.rs`, via `bind`) proves the two never diverge
//! (Rust/RDF agreement, HARD-fail on mismatch).
//!
//! **A stage's attach set = its DELTA** — the named graphs / content-identified blob-rep
//! records present in its OUTPUT product bundle but NOT in its effective INPUT. Named
//! graphs honor typed `consumed_entities`; blob records use `(representation, content
//! digest)` identity across every consumed upstream product, so several diagnostics
//! producers may each attach distinct content under the shared `diagnostics:nodes` lane.
//! The scheduler recomputes that delta at run time and HARD-fails
//! ([`crate::error::AttachDrift`]) if it diverges from this declaration in either
//! direction. The sets below were captured empirically from a full production run so the
//! global bidirectional check has zero false positives.

use std::collections::BTreeMap;
use std::sync::LazyLock;

/// One stage's declared attach set (sorted, deduplicated).
pub(crate) struct StageAttach {
    /// The named-graph IRIs the stage attaches (its graph delta).
    pub graphs: Vec<String>,
    /// The blob-representation lane labels the stage attaches (its blob-rep delta).
    pub blob_reps: Vec<String>,
}

/// The attach table, keyed by stage id. Stages absent here attach nothing on either
/// tracked lane (the default `&[]`).
static ATTACH_TABLE: LazyLock<BTreeMap<&'static str, StageAttach>> = LazyLock::new(build_table);

/// The named-graph IRIs `stage_id` attaches, sorted (empty for a non-attaching stage).
pub(crate) fn graphs(stage_id: &str) -> &'static [String] {
    ATTACH_TABLE
        .get(stage_id)
        .map(|a| a.graphs.as_slice())
        .unwrap_or(&[])
}

/// The blob-representation lane labels `stage_id` attaches, sorted (empty for a
/// non-attaching stage).
pub(crate) fn blob_reps(stage_id: &str) -> &'static [String] {
    ATTACH_TABLE
        .get(stage_id)
        .map(|a| a.blob_reps.as_slice())
        .unwrap_or(&[])
}

fn entry(
    t: &mut BTreeMap<&'static str, StageAttach>,
    id: &'static str,
    graphs: &[&str],
    blob_reps: &[&str],
) {
    let mut g: Vec<String> = graphs.iter().map(|s| (*s).to_string()).collect();
    g.sort();
    g.dedup();
    let mut b: Vec<String> = blob_reps.iter().map(|s| (*s).to_string()).collect();
    b.sort();
    b.dedup();
    t.insert(
        id,
        StageAttach {
            graphs: g,
            blob_reps: b,
        },
    );
}

fn build_table() -> BTreeMap<&'static str, StageAttach> {
    // Entries use full `graph/...` IRI literals (grep-able against the module.ttl
    // `gmeow:attachesGraph` declarations and the empirical run dump).
    let mut t = BTreeMap::new();

    // stage-source-load — the authored self-description graphs + the source-span blob.
    entry(
        &mut t,
        "stage-source-load",
        &[
            "https://blackcatinformatics.ca/gmeow/graph/authored-default",
            "https://blackcatinformatics.ca/gmeow/graph/grounding-seams",
            "https://blackcatinformatics.ca/gmeow/graph/imports",
            "https://blackcatinformatics.ca/gmeow/graph/logic-compile-inputs",
            "https://blackcatinformatics.ca/gmeow/graph/metadata",
            "https://blackcatinformatics.ca/gmeow/graph/provenance",
            "https://blackcatinformatics.ca/gmeow/graph/quality-assessment",
            "https://blackcatinformatics.ca/gmeow/graph/slice-analysis",
            "https://blackcatinformatics.ca/gmeow/graph/verify",
        ],
        &["spans:source-table"],
    );

    // stage-compile-logic — the object-level logic graphs + compile diagnostics nodes.
    entry(
        &mut t,
        "stage-compile-logic",
        &[
            "https://blackcatinformatics.ca/gmeow/graph/correspondence",
            "https://blackcatinformatics.ca/gmeow/graph/diagnostics",
            "https://blackcatinformatics.ca/gmeow/graph/logic",
            "https://blackcatinformatics.ca/gmeow/graph/relational-core",
        ],
        &["diagnostics:nodes"],
    );

    // stage-goal-directed — the checked backward-engine answers + proof derivations.
    entry(
        &mut t,
        "stage-goal-directed",
        &["https://blackcatinformatics.ca/gmeow/graph/goal-directed"],
        &[],
    );

    // stage-math-producers — the five flagship producer graphs (the rBridge one being the
    // executable r-lift) plus the probability-model seam, p-value tri-slice, and exact
    // Clifford producer graphs, and the ONNX / proof lift producer graphs.
    entry(
        &mut t,
        "stage-math-producers",
        &[
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/additive-he",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/clifford-12-13",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/e8-weyl",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/onnx-lift",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/pca-residual",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/probability-model",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/proof-ingest",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/proof-lift",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/pvalue-tri-slice",
            "https://blackcatinformatics.ca/gmeow/graph/math-producers/r-lift",
        ],
        &[],
    );

    // stage-gmn-training-corpus — the enumerated + certified GMN training corpus (and the typed
    // rejections), one bundle-internal named graph.
    entry(
        &mut t,
        "stage-gmn-training-corpus",
        &["https://blackcatinformatics.ca/gmeow/graph/gmn-training-corpus"],
        &[],
    );

    // stage-slice-brief — the per-slice authoring-packet corpus (base graph); the snapshot
    // re-roots the SAME triples into their fanout twin (below).
    entry(
        &mut t,
        "stage-slice-brief",
        &["https://blackcatinformatics.ca/gmeow/graph/authoring-briefs"],
        &[],
    );

    // stage-mappings — the alignment / correspondence-laws / lang-corpus / loss-ledger graphs.
    entry(
        &mut t,
        "stage-mappings",
        &[
            "https://blackcatinformatics.ca/gmeow/graph/alignments",
            "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws",
            "https://blackcatinformatics.ca/gmeow/graph/lang-docs-rendering-corpus",
            "https://blackcatinformatics.ca/gmeow/graph/lang-form-corpus",
            "https://blackcatinformatics.ca/gmeow/graph/lang-glossary-corpus",
            "https://blackcatinformatics.ca/gmeow/graph/lang-lowering-corpus",
            "https://blackcatinformatics.ca/gmeow/graph/lang-projection-corpus",
            "https://blackcatinformatics.ca/gmeow/graph/lang-translation-corpus",
            "https://blackcatinformatics.ca/gmeow/graph/projection-ledger",
        ],
        &[],
    );

    // stage-reason — the reasoned closure plus its production chase certificates.
    entry(
        &mut t,
        "stage-reason",
        &[
            "https://blackcatinformatics.ca/gmeow/graph/diagnostics",
            "https://blackcatinformatics.ca/gmeow/graph/reasoning",
        ],
        &["diagnostics:nodes"],
    );

    // stage-validate — the SHACL diagnostics graph, the advisory dual-projection's
    // materialised ComplianceAssessment claim graph (D4), + diagnostics nodes.
    entry(
        &mut t,
        "stage-validate",
        &[
            "https://blackcatinformatics.ca/gmeow/graph/diagnostics",
            "https://blackcatinformatics.ca/gmeow/graph/norm-claims",
        ],
        &["diagnostics:nodes"],
    );

    // stage-conformance — the external-corpus divergence Findings graph.
    entry(
        &mut t,
        "stage-conformance",
        &["https://blackcatinformatics.ca/gmeow/graph/conformance"],
        &[],
    );

    // stage-docs-render — the projected documentation graph.
    entry(
        &mut t,
        "stage-docs-render",
        &["https://blackcatinformatics.ca/gmeow/graph/documentation"],
        &[],
    );

    // stage-snapshot — re-roots the per-file fanout/EDOAL byte-artifacts into their
    // reconstruction named graphs (graph/fanout/<path>, graph/projections/<stem>.edoal)
    // and the RDF-1.2 statement graph. These are the reconstruction reps the superset
    // gate folds; the snapshot presenter is where they first enter a named graph.
    entry(
        &mut t,
        "stage-snapshot",
        &[
            // The scoped coherence certificate / attestation folded over the composed
            // carrier (R6): a budget-free, proof-carrying coherence artifact the consumer
            // read tool surfaces directly.
            "https://blackcatinformatics.ca/gmeow/graph/attestations",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/briefs/authoring-packets.nt",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/constraint-catalog.nq",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/term-content-manifest.nq",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/diagnostics/logic-compile.nq",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/diagnostics/shacl.nq",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/evals/scores.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/foundation/gufo.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/logic/gmeow.correspondence-laws.nt",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/logic/gmeow.correspondence.nt",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/logic/gmeow.relational-core.nt",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/logic/projection-report.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/logic/shape-grounding-ledger.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/agent-runtime.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/claims.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/dreaming.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/full.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/memory.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/music.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/narrative.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/purremb.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/projections/core-prefixes.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/projections/functions.fno.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/projections/glossary.vartrans.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/projections/list-functions.fno.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/quality/gmeow.quality-assessment.nt",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/lillith.dcat.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/ro-crate/corpus.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/ro-crate/grounded-claim.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/ro-crate/lillith-dataset.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/ro-crate/lillith-pipeline.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/ro-crate/rubric.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/research-objects/lillith/ro-crate/scores.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/fanout/skos/gmeow-skos.ttl",
            "https://blackcatinformatics.ca/gmeow/graph/projections/activitystreams.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/bibframe.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/bibo.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/bot.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/cc.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/codemeta.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/crmarchaeo.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/dcat.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/dcterms.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/doap.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/exif.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/foaf.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/gedcom.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/geosparql.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/ical.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/iiif.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/intoto.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/iptc.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/ivoa.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/jams.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/jcal.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/loinc.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/lrmoo.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/mailmap.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/markdown.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/ml-schema.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/mo.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/oai_dc.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/odrl.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/ontolex.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/ontouml.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/org.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/owl-time.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/pon.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/prov.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/qb.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/resume.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/schema-org-schedule.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/schema-org.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/sigstore.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/sioc.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/skos.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/slsa.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/sosa.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/spdx.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/vcard.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/projections/web-annotation.edoal",
            "https://blackcatinformatics.ca/gmeow/graph/statements",
        ],
        &[],
    );

    t
}
