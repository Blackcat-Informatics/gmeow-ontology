// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The caller for the oxigraph-free correspondence lowerings.
//!
//! The SSSOM / FnO / EDOAL / SPARQL alignment artifacts are now produced by the
//! wasm-clean `gmeow-logic-compile` correspondence lowerings, not by the historical
//! oxigraph-coupled `gmeow-slice` emitters. This module is the file-reading edge: it
//! natively parses the DSL + ontology + metadata sources into `RdfDataset`s (via the
//! oxigraph-free `gmeow-rdf` codecs) and drives the four lowerings. EDOAL + SPARQL
//! lower from one shared get-leg model, so the historical `spec-drift` invariant is
//! gone by construction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed;
use gmeow_logic_compile::projections::{ProjectionResult, edoal, emotionml, fno, sparql, sssom};
use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::slice::{ArtifactRole, SliceCatalog, SliceError};
use purrdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, TermRef, TermValue, parse_dataset};

const GM_VERSION_FINGERPRINT: &str = "https://blackcatinformatics.ca/gmeow/versionFingerprint";
const GM_DATE_PUBLISHED: &str = "https://blackcatinformatics.ca/gmeow/datePublished";

/// The canonical schadenfreude derived-intensity observation whose geometry the worked
/// `<emotion>` envelope emits. This is a SHIPPED base-graph A-Box individual (it lives in
/// `slices/core/affect/module.ttl`, not `examples/`), so it rides the in-memory ontology
/// carrier the whole correspondence lane already folds — the projection reads it from the
/// carrier, never from disk. Its computed geometry is pinned in
/// `worked_envelope_projects_the_schadenfreude_example`, so any drift in its appraisal
/// vector or the metric Gram reds that test.
const SF_INTENSITY_IRI: &str = "https://blackcatinformatics.ca/gmeow/schadenfreudeIntensity";

/// All four alignment dialects' outputs, keyed by bare file name within each
/// generated directory.
pub struct CorrespondenceArtifacts {
    /// `<name>.sssom.tsv` → TSV.
    pub sssom: BTreeMap<String, String>,
    /// The single FnO catalog N-Triples text.
    pub fno: String,
    /// `<profile>.edoal.ttl` → Turtle.
    pub edoal: BTreeMap<String, String>,
    /// `<profile>.rq` → SPARQL CONSTRUCT.
    pub sparql: BTreeMap<String, String>,
    /// `<profile>.put.rq` → SPARQL CONSTRUCT (the inverse ingest leg). ml-schema authors
    /// the ingest-claim terms today, so the map carries one entry; the emitter is the sole
    /// authority for the set, so the write loop and parity gates derive their count from
    /// this map's length.
    pub sparql_put: BTreeMap<String, String>,
    /// The `gmeow-affect.emotionml.xml` document — the many-to-one EmotionML XML lowering
    /// of the affect category + dimension vocabularies. Its loss-ledger row (the collapse
    /// record) rides in `ledger` alongside the four alignment dialects.
    pub emotionml: String,
    /// The per-correspondence loss ledger aggregated across all four dialects — one
    /// `ProjectionResult` per correspondence per dialect that drops something. The
    /// mappings stage unions this with the logic projection rows and serializes the
    /// final `generated/logic/projection-report.ttl` (the loss ledger is the residue
    /// set, per LOGIC-CORRESPONDENCE.md).
    pub ledger: Vec<ProjectionResult>,
    /// The single loss store every dialect (SSSOM/FnO/EDOAL/SPARQL get+put) and the EmotionML
    /// emitter interned their per-correspondence drops into, unioned across all five (keyed by
    /// target focus). The mappings stage unions it with the compile-logic loss store so the
    /// FINAL projection report reads every row's residue back from ONE substrate ledger.
    pub loss: LossLedger,
    /// The typed `logic:Correspondence` set materialized from the SAME `dsl/mappings/`
    /// cells the four dialects lower: one node per native alignment
    /// cell and one per `gmeow:ProjectionMapping` per-profile binding. This is the carried
    /// program the mappings stage threads onto the bundle so `LogicProgram.correspondences`
    /// is no longer reconstructed ad hoc downstream. The four dialect
    /// lowerings CONSUME this materialized set's typed `(relation, morphism class, morphism
    /// kind)` for their overclaim gate / ledger path (via the by-natural-key lookup the
    /// transpiler builds alongside the program) instead of re-deriving the relation inline —
    /// the materialized set is the single source of truth. The four rendered artifacts'
    /// bytes are unchanged (the renderers emit the authored predicate/relation token).
    pub correspondences: CorrespondenceProgram,
    /// Per-binding get/put CONSTRUCT fragments, keyed by `(cell IRI, profile)` — the
    /// single-cell slice of each per-profile query (get fragment) plus its inverse (`Some`
    /// only when the binding emits a put leg). The mappings stage joins this against
    /// [`correspondence_profiles`](Self::correspondence_profiles) + each correspondence's
    /// `get_leg` (= its cell IRI) to discharge that ONE correspondence's lens law in
    /// isolation. Pure strings (no engine ran in logic-compile — F2).
    pub sparql_fragments: BTreeMap<(String, String), (String, Option<String>)>,
    /// Correspondence IRI → profile for every `gmeow:ProjectionMapping` binding
    /// correspondence (absent for native alignment cells, which are not
    /// profile-scoped). The join key the mappings stage uses to find a correspondence's own
    /// `(cell IRI, profile)` fragment pair in [`sparql_fragments`](Self::sparql_fragments).
    pub correspondence_profiles: BTreeMap<String, String>,
}

/// Lower every alignment dialect from the sources under `root`, reading slice artifacts
/// from the SHARED in-memory `catalog` the mappings stage discovered ONCE (the same
/// instance the total prose-lift corpus draws its `@x-gmeow-english` universe from), so the
/// `slices/` tree is walked once per pipeline run — never re-scanned per consumer. `catalog`
/// is `None` only when there is no `slices/` tree (the DSL/ontology merges then fold just
/// `dsl/mappings/` + `ontology/gmeow.ttl`).
pub fn lower_all(
    root: &Path,
    catalog: Option<&SliceCatalog>,
) -> Result<CorrespondenceArtifacts, SliceError> {
    let dsl = merge_dsl(root, catalog)?;
    let onto = merge_ontology(root, catalog)?;
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&onto);
    let (version, release_date) = read_self_metadata(root)?;

    // Materialize the typed logic:Correspondence set + its by-natural-key lookup from the
    // DSL cells FIRST: the lookup is the single source of truth the four
    // dialect lowerings CONSUME for their overclaim gate / ledger path, so it
    // must exist before they run. The four RENDERED artifacts are unchanged (they still
    // emit the authored predicate/relation token verbatim).
    let (correspondences, lookup) = transpile_correspondences_indexed(&dsl_view, &onto_view)
        .map_err(|e| SliceError::Parse(e.to_string()))?;

    let sssom = sssom::lower_sssom(&dsl_view, &version, &release_date, &lookup)
        .map_err(|e| SliceError::Parse(e.to_string()))?;
    let fno =
        fno::lower_fno(&dsl_view, &onto_view).map_err(|e| SliceError::Parse(e.to_string()))?;
    let edoal = edoal::lower_edoal(&dsl_view, &onto_view, &lookup)
        .map_err(|e| SliceError::Parse(e.to_string()))?;
    let sparql = sparql::lower_sparql(&dsl_view, &onto_view, &lookup)
        .map_err(|e| SliceError::Parse(e.to_string()))?;

    // The EmotionML XML lowering enumerates the affect category (gmeow:EmotionType) and
    // dimension (gmeow:AppraisalDimension / gmeow:CoreAffectDimension) vocabularies straight
    // out of the merged ontology view — a many-to-one, lossy-by-construction emitter that
    // needs no external RDF namespace. Its collapse record joins the shared loss ledger.
    // The worked <emotion> envelope PROJECTS the canonical schadenfreude worked instance: its
    // overall intensity + per-dimension values are COMPUTED by gmeow-affect from the SHIPPED
    // base-graph A-Box (the same in-memory `onto` carrier the lane already folds) over
    // gmeow:coreAffectGram — never a fabricated constant and never a disk re-read. A missing
    // observation or a compute failure is a HARD FAIL here (no fallback literal).
    let worked = compute_worked_envelope(&onto).map_err(|e| SliceError::Parse(e.to_string()))?;
    let emotionml = emotionml::lower_emotionml(&onto_view, &worked);

    // Aggregate the per-correspondence ledger across all four dialects plus the EmotionML
    // collapse row. Each dialect already attributes its residue to the dropping (get) leg.
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    ledger.extend(sssom.ledger);
    ledger.extend(fno.ledger);
    ledger.extend(edoal.ledger);
    ledger.extend(sparql.ledger);
    ledger.extend(emotionml.ledger);

    // Union every dialect's + the EmotionML emitter's loss store into ONE (content-addressed,
    // idempotent), so the mappings stage reads each correspondence row's residue back from a
    // single substrate ledger, byte-identically to a single fold.
    let mut loss = LossLedger::new();
    loss.union(&sssom.loss);
    loss.union(&fno.loss);
    loss.union(&edoal.loss);
    loss.union(&sparql.loss);
    loss.union(&emotionml.loss);

    Ok(CorrespondenceArtifacts {
        sssom: sssom.sets,
        fno: fno.catalog,
        edoal: edoal.alignments,
        sparql: sparql.queries,
        // The inverse ingest leg rides on the same lowering; ml-schema authors the
        // ingest-claim terms today, so the map carries one entry now and grows automatically
        // as more slices author claims; the map is the sole authority for the `.put.rq` set.
        sparql_put: sparql.put_queries,
        emotionml: emotionml.document,
        ledger,
        loss,
        correspondences,
        // The per-binding get/put fragments + the corr→profile join key: the mappings stage
        // discharges each correspondence's OWN lens law from these (never the per-profile
        // UNION, which is the wrong unit).
        sparql_fragments: sparql.fragments,
        correspondence_profiles: lookup.binding_profiles().clone(),
    })
}

/// The discharged-`logic:SectionLaw` cell IRIs computed directly from in-memory cell TTLs + the
/// ontology N-Triples — the string-fed twin of the mappings stage's
/// `discharge_correspondence_laws`
/// (crate::stages::mappings). It reuses the SAME `transpile_correspondences_indexed` +
/// [`sparql::lower_sparql`] + [`crate::correspondence_law::discharge_laws`] pipeline, so it yields
/// the identical authorization set the bundle folds into `graph/correspondence-laws` — never a
/// second copy of the discharge algorithm.
///
/// This exists for callers that supply the projection cells and ontology, but not
/// the folded verdict graph, so they can recompute the SAME verdicts rather than
/// hard-failing for want of the folded set. The native `gmeow` / `gmeow-dev`
/// up-projection path consumes the FOLDED verdict from the bundle
/// ([`crate::projections::discharged_section_cells_from_bundle`]) instead.
pub fn discharged_section_cells_from_cells(
    projection_ttls: &[String],
    ontology_nt: &str,
) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    use gmeow_logic_compile::ir::{CorrespondenceLaw, DischargeVerdict};

    let up = |message: String| gmeow_errors::Diag::of_kind(crate::error::UpProjection { message });
    let mut dsl_b = RdfDatasetBuilder::new();
    for ttl in projection_ttls {
        let ds = parse_dataset(ttl.as_bytes(), NativeRdfFormat::Turtle.media_type(), None)
            .map_err(|e| up(format!("parse projection cell: {e}")))?;
        dsl_b.push_dataset(&ds);
    }
    let dsl = dsl_b.freeze().map_err(|e| up(e.to_string()))?;
    let onto = parse_dataset(ontology_nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| up(format!("parse ontology: {e}")))?;
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&onto);
    let (correspondences, lookup) =
        transpile_correspondences_indexed(&dsl_view, &onto_view).map_err(|e| up(e.to_string()))?;
    let sparql =
        sparql::lower_sparql(&dsl_view, &onto_view, &lookup).map_err(|e| up(e.to_string()))?;
    let profiles = lookup.binding_profiles();

    let mut cells = std::collections::BTreeSet::new();
    for corr in &correspondences.correspondences {
        let (Some(profile), Some(cell_iri)) = (profiles.get(&corr.iri), corr.get_leg.as_deref())
        else {
            continue;
        };
        let Some((get_rq, Some(put_rq))) = sparql
            .fragments
            .get(&(cell_iri.to_owned(), profile.clone()))
            .map(|(g, p)| (g, p.as_ref()))
        else {
            continue;
        };
        for claim in crate::correspondence_law::discharge_laws(get_rq, put_rq, corr.morphism_class)
        {
            if claim.law == CorrespondenceLaw::SectionLaw
                && claim.verdict == DischargeVerdict::ObligationDischarged
            {
                cells.insert(cell_iri.to_owned());
            }
        }
    }
    Ok(cells)
}

/// Compute the EmotionML worked-`<emotion>` envelope's values by projecting the canonical
/// schadenfreude worked instance carried on the in-memory `onto` dataset (which already folds
/// the affect `module.ttl` — its metric Gram, axis indices, and the shipped schadenfreude
/// A-Box). It computes the metric-tensor geometry via `gmeow-affect` and maps its intensity +
/// per-axis unit-clamp values to the emitter's plain data. Every emitted number is COMPUTED —
/// the emitter is forbidden a hand-typed constant. A missing observation or a compute failure
/// is a HARD FAIL (`Err`), never a fallback literal.
fn compute_worked_envelope(onto: &RdfDataset) -> gmeow_errors::Result<emotionml::WorkedEnvelope> {
    let geometry = schadenfreude_geometry(onto)?;
    Ok(emotionml::WorkedEnvelope {
        intensity: geometry.intensity,
        dimensions: geometry
            .normalized
            .into_iter()
            .map(|axis| (axis.dimension, axis.value))
            .collect(),
    })
}

/// The schadenfreude affect-intensity geometry, computed over the in-memory ontology carrier
/// `onto` (the folded `ontology/gmeow.ttl` + every slice `module.ttl`, so it carries both the
/// metric Gram and the shipped schadenfreude appraisal vector). The carrier is snapshotted
/// through the same GTS path the `gmeow affect` CLI uses, so this compute is byte-identical to
/// the shipped CLI's — and it reads NOTHING from disk (the projection is a true projection of
/// the carrier, per PIPELINE_SPINE).
fn schadenfreude_geometry(onto: &RdfDataset) -> gmeow_errors::Result<gmeow_affect::Geometry> {
    use purrdf::gts_compose::SnapshotBuilder;

    let transform =
        |message: String| gmeow_errors::Diag::of_kind(crate::error::Transform { message });
    let mut builder = SnapshotBuilder::new();
    builder
        .add_dataset(onto)
        .map_err(|e| transform(format!("add ontology carrier: {e}")))?;
    let gts =
        crate::gts_profile::emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
            .map_err(|e| transform(format!("emit affect fixture gts: {e}")))?;
    gmeow_affect::geometry_from_gts_bytes(&gts, Some(SF_INTENSITY_IRI))
        .map_err(|e| transform(e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| {
            transform(format!(
                "no affect geometry computed for {SF_INTENSITY_IRI}"
            ))
        })
}

fn parse_turtle(bytes: &[u8], context: &str) -> Result<Arc<RdfDataset>, SliceError> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None)
        .map_err(|e| SliceError::Parse(format!("{context}: {e}")))
}

fn collect_ttl_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SliceError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(SliceError::Io)? {
        let path = entry.map_err(SliceError::Io)?.path();
        if path.is_dir() {
            collect_ttl_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
    Ok(())
}

fn merge_slice_artifacts(
    catalog: Option<&SliceCatalog>,
    role: ArtifactRole,
    b: &mut RdfDatasetBuilder,
) -> Result<(), SliceError> {
    let Some(catalog) = catalog else {
        return Ok(());
    };
    // Borrow the artifact bytes (no clone): the catalog outlives this merge.
    let mut artifacts: Vec<(PathBuf, &[u8])> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == role {
                artifacts.push((
                    record.slice_dir.join(&artifact.logical_path),
                    &artifact.content,
                ));
            }
        }
    }
    artifacts.sort_by(|a, c| a.0.cmp(&c.0));
    for (path, bytes) in &artifacts {
        let ds = parse_turtle(bytes, &path.display().to_string())?;
        b.push_dataset(&ds);
    }
    Ok(())
}

/// The DSL source set (functions + cells): the sorted `dsl/mappings/**/*.ttl` tree,
/// then the sorted slice `Mapping` artifacts — the same order the historical store
/// loaded them, so collisions resolve identically.
pub(crate) fn merge_dsl(
    root: &Path,
    catalog: Option<&SliceCatalog>,
) -> Result<Arc<RdfDataset>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let mut files = Vec::new();
    collect_ttl_files(&root.join("dsl").join("mappings"), &mut files)?;
    files.sort();
    for path in &files {
        let bytes = std::fs::read(path).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, &path.display().to_string())?;
        b.push_dataset(&ds);
    }
    merge_slice_artifacts(catalog, ArtifactRole::Mapping, &mut b)?;
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// The ontology source set (`rdfs:range` / suppression vocabulary / language tags):
/// `ontology/gmeow.ttl`, then the sorted slice `Module` artifacts.
fn merge_ontology(
    root: &Path,
    catalog: Option<&SliceCatalog>,
) -> Result<Arc<RdfDataset>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.is_file() {
        let bytes = std::fs::read(&onto).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, "ontology/gmeow.ttl")?;
        b.push_dataset(&ds);
    }
    merge_slice_artifacts(catalog, ArtifactRole::Module, &mut b)?;
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// Read `(version, release_date)` from `metadata/gmeow-self.ttl` (the Manifestation is
/// the subject of `gmeow:versionFingerprint`; its `gmeow:datePublished` is the date).
fn read_self_metadata(root: &Path) -> Result<(String, String), SliceError> {
    let bytes =
        std::fs::read(root.join("metadata").join("gmeow-self.ttl")).map_err(SliceError::Io)?;
    let ds = parse_turtle(&bytes, "metadata/gmeow-self.ttl")?;
    let vfp = ds
        .term_id_by_value(&TermValue::Iri(GM_VERSION_FINGERPRINT.to_owned()))
        .ok_or_else(|| {
            SliceError::Parse("gmeow-self.ttl: no gmeow:versionFingerprint predicate".to_owned())
        })?;
    let manifestation = ds
        .quads_for_pattern(None, Some(vfp), None, GraphMatch::Default)
        .next()
        .ok_or_else(|| {
            SliceError::Parse("gmeow-self.ttl: no manifestation with versionFingerprint".to_owned())
        })?
        .s;
    let subject_iri = match ds.resolve(manifestation) {
        TermRef::Iri(iri) => iri.to_owned(),
        _ => {
            return Err(SliceError::Parse(
                "gmeow-self.ttl: versionFingerprint subject is not an IRI".to_owned(),
            ));
        }
    };
    let view = DslView::new(&ds);
    let version = view
        .object_literal(&subject_iri, GM_VERSION_FINGERPRINT)
        .ok_or_else(|| {
            SliceError::Parse("gmeow-self.ttl: missing versionFingerprint".to_owned())
        })?;
    let release_date = view
        .object_literal(&subject_iri, GM_DATE_PUBLISHED)
        .ok_or_else(|| SliceError::Parse("gmeow-self.ttl: missing datePublished".to_owned()))?;
    Ok((version, release_date))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("canonicalize repo root")
    }

    /// The EmotionML worked-envelope pin: the emitter PROJECTS the SHIPPED schadenfreude worked
    /// instance carried on the in-memory ontology dataset, so its intensity + per-dimension
    /// values are COMPUTED — never fabricated and never re-read from disk. This drives the REAL
    /// `compute_worked_envelope` over the REAL committed affect `module.ttl` (the carrier the
    /// pipeline folds) and asserts the metric-tensor outputs: intensity √(79/100) = 0.888819,
    /// valence 0.7 → 0.85, arousal 0.4 → 0.7. Retiring the shipped observation or perturbing its
    /// appraisal vector reds this test — the drift gate the invariant demands.
    #[test]
    fn worked_envelope_projects_the_schadenfreude_example() {
        let root = repo_root();
        // Build the ontology carrier from the committed affect module (the metric Gram, axis
        // indices, and the shipped schadenfreude A-Box all live there), mirroring the dataset
        // `lower_all` folds and hands to `compute_worked_envelope`.
        let module_ttl = root.join("slices/core/affect/module.ttl");
        let bytes = std::fs::read(&module_ttl).expect("read affect module.ttl");
        let onto = parse_dataset(&bytes, NativeRdfFormat::Turtle.media_type(), None)
            .expect("parse affect module.ttl");

        // The shipped observation IRI must resolve as a base-graph subject (retirement is a
        // hard fail): it is authored in module.ttl, never an excluded example overlay.
        assert!(
            onto.term_id_by_value(&TermValue::Iri(SF_INTENSITY_IRI.to_owned()))
                .is_some(),
            "shipped schadenfreude intensity observation missing from the carrier: {SF_INTENSITY_IRI}"
        );

        let worked = compute_worked_envelope(&onto).expect("compute worked envelope");

        assert_eq!(
            worked.intensity, "0.888819",
            "overall intensity is the computed metric-tensor norm √(79/100)"
        );

        let valence = "https://blackcatinformatics.ca/gmeow/dimensionValence";
        let arousal = "https://blackcatinformatics.ca/gmeow/dimensionArousal";
        assert_eq!(
            worked.dimensions,
            vec![
                (valence.to_owned(), "0.85".to_owned()),
                (arousal.to_owned(), "0.7".to_owned()),
            ],
            "per-dimension unit-clamp values are computed from the schadenfreude vector"
        );
    }
}
