// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Measured, deterministic documentation-distribution sizing.
//!
//! [`measure_docs_designs`] renders every shipped documentation / serialization
//! format through the SAME production renderers `gmeow-dev sync` uses, frames
//! each rendered format through the single mandated GTS authorship profile
//! (`crate::gts_profile::emit_gmeow_gts`), and totals the real byte counts for
//! three candidate external-distribution designs:
//!
//! * **Design A** (external + manifest): every format written to disk as plain
//!   files, plus a small release-manifest allowance.
//! * **Design B** (sidecar `.gts`): a docs-only GTS snapshot carrying every
//!   format as a framed blob and no object-level ontology graph.
//! * **Design C** (opt-in embedded profile): an ANALYTICAL proxy — a real
//!   without-docs carrier snapshot (the same bytes `gmeow.gts` ships today)
//!   plus the L12-framed cost of adding every doc format to it. No shippable
//!   embed path exists (re-embedding docs in `gmeow.gts` is FORBIDDEN by
//!   project directive — PIPELINE_SPINE.md); Design C is a measured proxy for
//!   "what would it cost if we did", never a real carrier round-trip.
//!
//! Every byte count is MEASURED (not estimated): each format is actually
//! rendered and actually framed through the mandated zstd-rsyncable L12
//! profile, so re-running this module reproduces the same numbers as long as
//! the sources are unchanged (determinism — no timestamps, no HashMap
//! iteration order, sorted output).
//!
//! Obtaining a REAL without-docs `gmeow.gts` carrier (Design C) and the REAL
//! per-format upstream artifacts (the compiled axioms, the JSON Schema /
//! OpenAPI pair, the Pydantic model package, …) requires the full pipeline
//! DAG's non-docs stages to have actually run — this codebase is a strict
//! single-pass executor with no partial/scoped DAG execution (`crate::run`'s
//! own doc comments: "the WHOLE DAG ... runs in ONE scheduler pass"), and
//! `crate::stages::carrier::serialize_carrier_snapshot` fails closed on any
//! missing upstream artifact rather than falling back to a stale disk read
//! (the "stale-disk-fold" class every stage in this crate explicitly refuses
//! to reintroduce). So this module runs the real DAG once, entirely in
//! memory (`crate::scheduler::run` with an uncached [`crate::scheduler::RunContext`]),
//! and reads every artifact it needs off the resulting in-memory
//! [`crate::node::StageProduct`] map — never off disk, never re-derived by
//! hand. Update-mode disk reconciliation is skipped entirely, so a
//! `docs-measure` run never writes to the working tree.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_errors::Diag;
use purrdf::RdfDataset;
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};

use crate::error::DocsMeasure as DocsMeasureError;
use crate::node::StageProduct;
use crate::stages::compile_logic::{CANONICAL_RDF12_PATH, DATALOG_PATH, OWL_DL_PATH, OWL_EL_PATH};
use crate::stages::references::BIB_PATH;

fn err(message: impl Into<String>) -> Diag {
    Diag::of_kind(DocsMeasureError {
        message: message.into(),
    })
}

/// The compiled logic/DL axiom listing the print renderer takes as its
/// bibliography-adjacent input — the same four paths `serialize_carrier_snapshot`
/// folds into `REP_AXIOMS` (`crate::stages::carrier::AXIOM_FILES`, private to that
/// module, mirrored here so this module has no dependency on it).
const AXIOM_FILES: [&str; 4] = [OWL_DL_PATH, OWL_EL_PATH, CANONICAL_RDF12_PATH, DATALOG_PATH];

/// A fixed nominal allowance for the Design A release manifest (the DCAT catalog
/// instance + per-format checksums the external distribution ships alongside the
/// rendered trees — produced for real by `crate::docs_distribution`). The manifest
/// is KB-scale and negligible against every measured format (the smallest is
/// `pydantic` at ~2.6 MB), so this size comparison carries it as a single explicit,
/// greppable nominal constant rather than folding it silently into `design_a_bytes`
/// with no accounting; measuring the exact manifest bytes would not move the total.
const DESIGN_A_MANIFEST_ALLOWANCE_BYTES: u64 = 4096;

/// One documentation or serialization format's measured footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatMeasurement {
    /// The format's stable, lower-kebab name (`site`, `mdbook`, `print`,
    /// `snippets`, `pydantic`, `okf`, `yaml-ld`).
    pub format_name: String,
    /// The distribution family the format belongs to: `docs` for the
    /// executable-documentation fanout, `serialization` for the OKF /
    /// YAML-LD serialization-family projections.
    pub family: String,
    /// The sum of the format's rendered file bytes, exactly as they would be
    /// written to an external distribution tree (no archive/frame overhead).
    pub uncompressed_bytes: u64,
    /// The incremental byte cost of framing this format's rendered tree
    /// (packed as one deterministic USTAR archive, matching how every
    /// multi-file documentation projection already rides inside `gmeow.gts`)
    /// through the mandated zstd-rsyncable level-12 GTS profile: the size of
    /// a GTS snapshot carrying only this format's blob, minus the size of an
    /// otherwise-identical empty snapshot.
    pub l12_bytes: u64,
}

/// The measured byte totals for the three candidate external-distribution
/// designs, plus the per-format breakdown each design total is computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsMeasurements {
    /// Every measured format, sorted by [`FormatMeasurement::format_name`].
    pub formats: Vec<FormatMeasurement>,
    /// Design A (external tree + manifest): Σ per-format uncompressed bytes
    /// + [`DESIGN_A_MANIFEST_ALLOWANCE_BYTES`].
    pub design_a_bytes: u64,
    /// Design B (sidecar `.gts`): the byte size of a docs-only GTS snapshot
    /// framing every format's blob together, with no object-level RDF graph.
    pub design_b_bytes: u64,
    /// Design C (opt-in embedded profile, an ANALYTICAL proxy — see the
    /// module doc comment): the real without-docs carrier snapshot size
    /// (`crate::stages::carrier::serialize_carrier_snapshot`) plus
    /// Σ per-format `l12_bytes`.
    pub design_c_bytes: u64,
}

/// Render every shipped documentation / serialization format through the real
/// production renderers, frame each through the mandated GTS profile, and
/// total the three external-distribution designs. See the module doc comment for
/// why this runs the full pipeline DAG once, in memory, with no disk writes.
pub fn measure_docs_designs(root: &Path) -> Result<DocsMeasurements, Diag> {
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let products = run_pipeline_products(root, jobs)?;
    let carrier = crate::stages::carrier::snapshot_dataset(&products)
        .map_err(|e| err(format!("assemble the snapshot carrier: {e}")))?;

    let rendered = render_every_format(root, &products, carrier.as_ref())?;

    let empty_builder = SnapshotBuilder::new();
    let baseline_len = crate::gts_profile::emit_gmeow_gts(
        &empty_builder,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
    )
    .map_err(|e| err(format!("frame the baseline (empty) GTS snapshot: {e}")))?
    .len() as u64;

    let mut formats = Vec::with_capacity(rendered.len());
    let mut doc_blob_rows = Vec::with_capacity(rendered.len());
    let mut design_a_total = 0u64;
    for rendered_format in &rendered {
        let framed_len = crate::gts_profile::emit_gmeow_gts(
            &empty_builder,
            vec![rendered_format.blob_row()],
            Vec::new(),
            None,
            None,
            None,
        )
        .map_err(|e| {
            err(format!(
                "frame the {} format's GTS blob: {e}",
                rendered_format.format_name
            ))
        })?
        .len() as u64;
        let l12_bytes = framed_len.saturating_sub(baseline_len);
        design_a_total += rendered_format.uncompressed_bytes;
        formats.push(FormatMeasurement {
            format_name: rendered_format.format_name.clone(),
            family: rendered_format.family.clone(),
            uncompressed_bytes: rendered_format.uncompressed_bytes,
            l12_bytes,
        });
        doc_blob_rows.push(rendered_format.blob_row());
    }
    formats.sort_by(|a, b| a.format_name.cmp(&b.format_name));

    let design_a_bytes = design_a_total + DESIGN_A_MANIFEST_ALLOWANCE_BYTES;

    let design_b_bytes = crate::gts_profile::emit_gmeow_gts(
        &empty_builder,
        doc_blob_rows,
        Vec::new(),
        None,
        None,
        None,
    )
    .map_err(|e| {
        err(format!(
            "frame the Design B docs-only sidecar snapshot: {e}"
        ))
    })?
    .len() as u64;

    let without_docs_bytes =
        crate::stages::carrier::serialize_carrier_snapshot(root, &products, carrier.as_ref())
            .map_err(|e| err(format!("serialize the without-docs carrier snapshot: {e}")))?
            .len() as u64;
    let l12_doc_sum: u64 = formats.iter().map(|f| f.l12_bytes).sum();
    let design_c_bytes = without_docs_bytes + l12_doc_sum;

    Ok(DocsMeasurements {
        formats,
        design_a_bytes,
        design_b_bytes,
        design_c_bytes,
    })
}

/// One rendered format, ready to be totalled and GTS-framed. Carries the
/// packed archive bytes + a stable `rep` label rather than an owned
/// [`BlobRow`] (which does not implement `Clone`), since this format's blob
/// is framed twice — once alone (its own `l12_bytes` delta) and once
/// together with every other format (Design B).
struct RenderedFormat {
    format_name: String,
    family: String,
    /// Σ raw file bytes — the external-tree size (no archive overhead).
    uncompressed_bytes: u64,
    /// The format's tree packed as one deterministic USTAR archive, the same
    /// multi-file→single-blob convention every documentation projection
    /// already uses inside `gmeow.gts` (`crate::stages::carrier::archive_blob`).
    archive: Vec<u8>,
    /// The blob's stable representation label.
    rep: String,
}

impl RenderedFormat {
    fn blob_row(&self) -> BlobRow {
        BlobRow {
            data: self.archive.clone(),
            media_type: "application/x-tar".to_string(),
            rep: self.rep.clone(),
        }
    }
}

/// Run the full pipeline DAG once, entirely in memory, and render the seven
/// external-distribution formats (five `docs` family, two `serialization`
/// family) off its real in-memory products — never off disk.
fn render_every_format(
    root: &Path,
    products: &BTreeMap<String, StageProduct>,
    carrier: &RdfDataset,
) -> Result<Vec<RenderedFormat>, Diag> {
    let model = build_docs_model(root, products)?;

    let playground_trig = crate::stages::carrier::playground_trig_from_bundle(carrier)
        .map_err(|e| err(format!("build the playground TriG asset: {e}")))?;
    let known_term_iris: std::collections::BTreeSet<String> =
        model.terms.iter().map(|t| t.iri.clone()).collect();
    let term_entailments =
        crate::stages::carrier::term_entailments_from_upstream(products, &known_term_iris)?;
    let exec = gmeow_docs::ExecutableDocsData {
        playground_trig,
        term_entailments,
        ..Default::default()
    };

    let site = gmeow_docs::render_site_lang_exec(&model, gmeow_docs::i18n::ENGLISH, &exec).files;
    let mdbook =
        gmeow_docs::mdbook::render_book(&model, &gmeow_docs::ExecutableDocsData::default()).files;
    let print = render_print(&model, products)?;
    let snippets = source_snippets(&site)?;
    let pydantic = pydantic_docs_tree(products)?;

    let (title, version, terms) = crate::stages::export::collect_term_surface(carrier)
        .map_err(|e| err(format!("collect the OKF/serialization term surface: {e}")))?;
    let okf = crate::stages::okf::render_okf(&title, &version, &terms)
        .map_err(|e| err(format!("render the OKF bundle: {e}")))?;
    let yaml_ld = yaml_ld_tree(carrier)?;

    Ok(vec![
        rendered_format("site", "docs", &site)?,
        rendered_format("mdbook", "docs", &mdbook)?,
        rendered_format("print", "docs", &print)?,
        rendered_format("snippets", "docs", &snippets)?,
        rendered_format("pydantic", "docs", &pydantic)?,
        rendered_format("okf", "serialization", &okf)?,
        rendered_format("yaml-ld", "serialization", &yaml_ld)?,
    ])
}

/// Discover and fully attach the documentation model exactly as the in-DAG
/// `stage-docs-render` does (`crate::stages::docs_render::render_docs_graph`),
/// sourced ENTIRELY from THIS run's in-memory upstream products — never a
/// disk read of `generated/catalog/*.nq` or `generated/schemas/*.json`
/// (the stale-disk-fold class; those files do not exist at all until a prior
/// `make regen` has materialized them, and this module never writes to disk).
/// The one on-disk-adjacent call, [`crate::stages::constraint_catalog::render_constraint_catalog`],
/// is a pure function of the AUTHORED sources (root ontology + slice
/// modules), not a read of the generated projection, so it needs no upstream
/// product and works on a cold tree.
fn build_docs_model(
    root: &Path,
    products: &BTreeMap<String, StageProduct>,
) -> Result<gmeow_docs::DocsModel, Diag> {
    let manifest_bytes = products
        .get("stage-term-manifest")
        .and_then(|p| p.artifact(crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH))
        .ok_or_else(|| err("missing stage-term-manifest product for the documentation model"))?;
    let catalog_bytes = crate::stages::constraint_catalog::render_constraint_catalog(root)
        .map_err(|e| err(format!("render the constraint catalog: {e}")))?;
    let mut model = gmeow_docs::DocsModel::discover_with_manifest_and_catalog(
        root,
        manifest_bytes,
        &catalog_bytes,
    )
    .map_err(|e| err(format!("discover the documentation model: {e}")))?;

    let verdict = crate::stages::docs_render::reasoning_verdict_from_reason(products)?;
    model.attach_reasoning(verdict);

    let known_term_iris: std::collections::BTreeSet<String> =
        model.terms.iter().map(|t| t.iri.clone()).collect();
    let diagnostics = crate::stages::docs_render::diagnostics_digest_from_upstream(
        products,
        &known_term_iris,
        &model.constraint_rules,
    )?;
    model.attach_diagnostics(diagnostics);

    let term_loss = crate::stages::docs_render::term_loss_digest_from_upstream(
        products,
        &model.shapes,
        &model.terms,
    )?;
    model.attach_term_loss(term_loss);

    let schema_digest =
        crate::stages::docs_render::schema_fragments_from_upstream(products, &model.terms)?;
    model.attach_schema_fragments(schema_digest);

    Ok(model)
}

/// Pack `tree` into one deterministic USTAR blob and total its raw bytes.
fn rendered_format(
    format_name: &str,
    family: &str,
    tree: &BTreeMap<String, Vec<u8>>,
) -> Result<RenderedFormat, Diag> {
    if tree.is_empty() {
        return Err(err(format!(
            "the {format_name} format rendered an empty tree — refusing to measure a format \
             with no real bytes (fail-closed)"
        )));
    }
    let uncompressed_bytes = tree.values().map(|bytes| bytes.len() as u64).sum();
    let members: Vec<(String, Vec<u8>)> = tree
        .iter()
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect();
    let archive = purrdf::ustar::write_archive(&members)
        .map_err(|e| err(format!("tar the {format_name} format for GTS framing: {e}")))?;
    Ok(RenderedFormat {
        format_name: format_name.to_string(),
        family: family.to_string(),
        uncompressed_bytes,
        archive,
        rep: format!("docs-measure/{format_name}"),
    })
}

/// Render the deterministic Typst source, compile the byte-reproducible print
/// PDF, from THIS run's in-memory `stage-compile-logic` (axiom listing) and
/// `stage-export-references` (bibliography) products — mirrors
/// `crate::stages::carrier::build_docs_print_blob` (test-only) and
/// `dev_project::render_source_print` (disk-sourced, post-pipeline), sourced
/// from the in-memory products instead so this module never depends on a
/// prior materialized `generated/` tree.
fn render_print(
    model: &gmeow_docs::model::DocsModel,
    products: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, Diag> {
    let compile_logic = products
        .get("stage-compile-logic")
        .ok_or_else(|| err("missing stage-compile-logic product for the print renderer"))?;
    let mut axioms: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for rel in AXIOM_FILES {
        let bytes = compile_logic.artifact(rel).ok_or_else(|| {
            err(format!(
                "missing axiom artifact {rel} for the print renderer"
            ))
        })?;
        axioms.insert(rel.to_string(), bytes.to_vec());
    }
    let bib = products
        .get("stage-export-references")
        .and_then(|p| p.artifact(BIB_PATH))
        .ok_or_else(|| err("missing stage-export-references bibliography for the print renderer"))?
        .to_vec();
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
    let pdf = docs_print::compile_pdf(&typ, &bib)
        .map_err(|e| err(format!("compile the print PDF: {e}")))?;
    Ok(BTreeMap::from([
        ("gmeow.pdf".to_string(), pdf),
        ("gmeow.typ".to_string(), typ.into_bytes()),
    ]))
}

/// The README written at the root of the `--format snippets` export tree.
/// Mirrors `dev_project::SNIPPETS_README` verbatim (fixed text, deterministic
/// from source) — duplicated rather than shared because the constant lives in
/// the downstream `gmeow-dev-cli` crate, which depends on this one, not the
/// reverse.
const SNIPPETS_README: &str = "\
# GMEOW documentation snippets

This directory is the **offline, agent-ingestible projection** of the GMEOW bundle
documentation. It contains one prompt-ready Markdown card per vocabulary term at
`terms/<slug>.md`. Each card is self-contained plain Markdown (metadata,
definition, and usage advice) with no site chrome and no cross-page links, so it
can be dropped straight into a prompt or a retrieval index without further
rendering.

## How to consume it

- Read a single term card directly: `terms/<slug>.md`, where `<slug>` is the
  lower-cased local name of the term.
- Ingest the whole corpus: concatenate or index every `terms/*.md` file; the set
  is complete (one card per documented term) and deterministically named.
- Regenerate the corpus from canonical sources with
  `gmeow-dev sync --mode update --outputs docs`.

The cards here are the same per-term surface the published documentation renders;
this projection simply flattens them for offline agent use.
";

/// Derive the `--format snippets` export tree from the rendered site's term
/// cards — mirrors `dev_project::source_snippets` exactly.
fn source_snippets(site: &BTreeMap<String, Vec<u8>>) -> Result<BTreeMap<String, Vec<u8>>, Diag> {
    let mut snippets = site
        .iter()
        .filter_map(|(path, bytes)| {
            let rest = path.strip_prefix("terms/")?;
            let slug = rest.strip_suffix("/card.md")?;
            Some((format!("terms/{slug}.md"), bytes.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    if snippets.is_empty() {
        return Err(err(
            "source documentation render produced no term-card snippets",
        ));
    }
    snippets.insert("README.md".to_string(), SNIPPETS_README.as_bytes().to_vec());
    Ok(snippets)
}

/// The rendered Pydantic model package — THIS run's in-memory
/// `stage-export-pydantic` product artifacts, never a disk read (that
/// producer's committed-tree sibling, `pydantic::render_models_python_package`,
/// is documented as "the standalone `make regen SYNC_OUTPUTS=docs` entry" that
/// requires an already-materialized `generated/shapes` tree; this module has
/// the fresher, in-memory bytes already, from the same run that fed Design C).
fn pydantic_docs_tree(
    products: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, Diag> {
    let product = products
        .get("stage-export-pydantic")
        .ok_or_else(|| err("missing stage-export-pydantic product for the pydantic docs format"))?;
    Ok(product.artifacts())
}

/// The WHOLE-CARRIER JSON-LD-star + YAML-LD-star serialization-family tree, keyed by
/// the `dist/` basenames `make build` writes.
///
/// Deliberately NOT the bundle's `yaml-ld-archive` frame, which carries the CLAIM
/// CORPUS's projection under different member names
/// ([`crate::bundle_blobs::YAMLLD_JSONLD_MEMBER`]): this module measures the
/// distribution family a consumer downloads, and that family is the whole carrier.
fn yaml_ld_tree(carrier: &RdfDataset) -> Result<BTreeMap<String, Vec<u8>>, Diag> {
    let jsonld = crate::stages::yaml_ld::serialize_graph(carrier)
        .map_err(|e| err(format!("serialize the JSON-LD-star document: {e}")))?;
    let yamlld = crate::stages::yaml_ld::serialize_graph_yaml(carrier, None)
        .map_err(|e| err(format!("serialize the YAML-LD-star document: {e}")))?;
    Ok(BTreeMap::from([
        ("gmeow.jsonld".to_string(), jsonld.into_bytes()),
        ("gmeow.yamlld".to_string(), yamlld.into_bytes()),
    ]))
}

/// Run the full pipeline DAG once, entirely in memory (no per-stage cache
/// I/O, no disk writes — mirrors `crate::run::run_full_scoped_with_progress`'s
/// DAG-execution half exactly, stopping short of its post-run disk
/// reconciliation loop, which this module never needs or wants to trigger).
fn run_pipeline_products(root: &Path, jobs: usize) -> Result<BTreeMap<String, StageProduct>, Diag> {
    let spec = crate::run::full_spec();
    let graph = spec.validate()?;
    let registry = crate::registry::default_registry();
    let bound = crate::loader::bind(&spec, &graph, &registry)?;
    let mut ctx = crate::scheduler::RunContext::open_uncached(root, jobs);
    let result = crate::scheduler::run(&graph, &bound, &mut ctx)?;
    Ok(result.products)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("workspace root")
    }

    /// The measurement is a pure function of the sources: two independent runs
    /// over the same tree must agree on every byte count, and every design
    /// total must be internally consistent with the per-format breakdown.
    #[test]
    #[ignore = "runs the full pipeline DAG twice; expensive, exercised on demand"]
    fn measurement_is_deterministic_across_two_runs() {
        let root = repo_root();
        let first = measure_docs_designs(&root).expect("first measurement");
        let second = measure_docs_designs(&root).expect("second measurement");
        assert_eq!(first, second);
        assert!(!first.formats.is_empty());
        let mut sorted_names: Vec<&str> = first
            .formats
            .iter()
            .map(|f| f.format_name.as_str())
            .collect();
        let mut expected = sorted_names.clone();
        expected.sort_unstable();
        assert_eq!(sorted_names, expected, "formats must be sorted by name");
        sorted_names.dedup();
        assert_eq!(
            sorted_names.len(),
            first.formats.len(),
            "format names must be unique"
        );
        let manifest_only = first.design_a_bytes
            - first
                .formats
                .iter()
                .map(|f| f.uncompressed_bytes)
                .sum::<u64>();
        assert_eq!(manifest_only, DESIGN_A_MANIFEST_ALLOWANCE_BYTES);
    }

    /// Cheap, DAG-free coverage of the tar-packing helper: the uncompressed
    /// total is the sum of the raw file bytes (no archive overhead folded in),
    /// and the archive itself carries real, non-empty bytes.
    #[test]
    fn rendered_format_sums_raw_bytes_and_packs_a_real_archive() {
        let mut tree = BTreeMap::new();
        tree.insert("a.md".to_string(), b"hello".to_vec());
        tree.insert("b/c.md".to_string(), b"world!!".to_vec());
        let format = rendered_format("demo", "docs", &tree).expect("pack a real tree");
        assert_eq!(format.format_name, "demo");
        assert_eq!(format.family, "docs");
        assert_eq!(format.uncompressed_bytes, 5 + 7);
        assert!(!format.archive.is_empty());
        assert_eq!(format.rep, "docs-measure/demo");
    }

    /// A format that renders no files is a hard failure, never a silently
    /// zero-sized measurement (no-optionality).
    #[test]
    fn rendered_format_fails_closed_on_an_empty_tree() {
        let tree: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let result = rendered_format("empty", "docs", &tree);
        assert!(result.is_err());
    }

    /// The blob-row constructor is used twice per format (its own delta, then
    /// again inside the combined Design B snapshot); both calls must yield the
    /// exact same bytes so the two GTS framings are directly comparable.
    #[test]
    fn blob_row_is_stable_across_repeated_calls() {
        let mut tree = BTreeMap::new();
        tree.insert("x.md".to_string(), b"stable".to_vec());
        let format = rendered_format("stable", "serialization", &tree).expect("pack a real tree");
        let first = format.blob_row();
        let second = format.blob_row();
        assert_eq!(first.data, second.data);
        assert_eq!(first.media_type, second.media_type);
        assert_eq!(first.rep, second.rep);
    }
}
