// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The file-reading edge for the oxigraph-free correspondence soundness pass.
//!
//! The seven correspondence-stack semantic checks (the five alignment checks + the two
//! FnO back-end checks — including the sole native enforcer of Constitution Principle 5,
//! the equivalence-collapse gate) now live in the wasm-clean
//! [`gmeow_logic_compile::projections::correspondence_soundness`] module, which operates
//! over already-parsed [`DslView`]s. This module is the file-reading + Turtle-parsing edge
//! (exactly like [`super::correspondence_lower`] is for the four dialect lowerings): it
//! reads the committed corpus under `root`, parses every source into an `RdfDataset` via
//! the native oxigraph-free codecs, and drives the pure pass.
//!
//! Inputs sourced oxigraph-free:
//!
//! * **ontology** — `ontology/gmeow.ttl` ⊕ every slice `Module` artifact (the same merge
//!   [`super::correspondence_lower::lower_all`] performs).
//! * **SSSOM mappings** — the committed `generated/mappings/*.sssom.tsv` (the SAME source
//!   the retired alignment lint read, for exact parity), parsed via
//!   [`gmeow_logic_compile::projections::correspondence_soundness::parse_sssom_tsv`].
//! * **target axioms** — per referenced prefix: the vendored snapshot
//!   `imports/targets/<prefix>.ttl` ⊕ the fixture `tests/fixtures/target_axioms/<prefix>.ttl`
//!   ⊕ (when `allow_network`) a live network fetch of the canonical source document,
//!   filtered to the minimal structural axiom subset in the target namespace.
//! * **FnO catalog** — `generated/projections/functions.fno.ttl` ⊕ `transforms.fno.ttl`
//!   (falling back to `dsl/mappings/transforms.fno.ttl`).
//! * **EDOAL** — every committed `generated/projections/*.edoal.ttl`, sorted by path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::correspondence_soundness::{
    self as soundness, Mapping, ProjectionDiagnostic, expand_curie, parse_sssom_tsv, prefix_of,
};
use gmeow_logic_compile::projections::get_leg::{self, ProjectionCell};
use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::slice::{ArtifactRole, SliceCatalog, SliceError};
use purrdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, TermRef, parse_dataset};

const GMEOW_PREFIX: &str = "gmeow:";

/// The grounding namespace. Alignment cells keyed on a `logic:` property are checked
/// exactly like `gmeow:`-keyed ones: when a domain term is superseded by a grounding
/// term, its alignments are RE-KEYED onto the grounding spine rather than dropped, and
/// scoping the check to `gmeow:` subjects would silently stop checking them at precisely
/// the moment they moved. The `is_property` guard below still applies, so this admits
/// only cells whose subject is a declared object/datatype property.
const LOGIC_PREFIX: &str = "logic:";

const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const SCHEMA_INVERSE_OF: &str = "https://schema.org/inverseOf";
const SCHEMA_DOMAIN_INCLUDES: &str = "https://schema.org/domainIncludes";
const SCHEMA_RANGE_INCLUDES: &str = "https://schema.org/rangeIncludes";

const OWL_PROPERTY_TYPES: &[&str] = soundness::OWL_PROPERTY_TYPES;

const FNO_FUNCTIONS_FILE: &str = "functions.fno.ttl";
const FNO_TRANSFORMS_FILE: &str = "transforms.fno.ttl";

/// A fetchable target-vocabulary source document.
struct TargetSource {
    prefix: &'static str,
    url: &'static str,
    media_type: &'static str,
}

/// Canonical source documents per target prefix (mirrors the retired Python
/// `target_axioms.TARGET_SOURCES`); fetched only when `allow_network=true` and never
/// committed.
const TARGET_SOURCES: &[TargetSource] = &[
    TargetSource {
        prefix: "org",
        url: "https://www.w3.org/ns/org.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "foaf",
        url: "http://xmlns.com/foaf/spec/index.rdf",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "vcard",
        url: "https://www.w3.org/2006/vcard/ns.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "prov",
        url: "https://www.w3.org/ns/prov-o.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "time",
        url: "https://www.w3.org/2006/time.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "geo",
        url: "https://opengeospatial.github.io/ogc-geosparql/geosparql11/geo.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "schema",
        url: "https://schema.org/version/latest/schemaorg-current-https.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "bfo",
        url: "http://purl.obolibrary.org/obo/bfo.owl",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "ontolex",
        url: "https://www.w3.org/ns/lemon/ontolex.owl",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "lime",
        url: "https://www.w3.org/ns/lemon/lime.owl",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "jams",
        url: "http://w3id.org/polifonia/ontology/jams/",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "pon",
        url: "https://w3id.org/polifonia/ontology/ontology-network/",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "chord",
        url: "http://purl.org/ontology/chord/",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "bot",
        url: "https://w3id.org/bot/bot.ttl",
        media_type: "text/turtle",
    },
];

/// Parse Turtle bytes into a frozen dataset (native lenient codec).
fn parse_turtle(bytes: &[u8], context: &str) -> Result<Arc<RdfDataset>, SliceError> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None)
        .map_err(|e| SliceError::Parse(format!("{context}: {e}")))
}

/// Merge the ontology source set (`ontology/gmeow.ttl` ⊕ sorted slice `Module` artifacts) —
/// the same merge `correspondence_lower::merge_ontology` performs.
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
    if let Some(catalog) = catalog {
        let mut artifacts: Vec<(PathBuf, &[u8])> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role == ArtifactRole::Module {
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
    }
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// Recursively collect every `.ttl` file under `dir` (mirrors
/// `correspondence_lower::collect_ttl_files`; duplicated here since this stage reads its
/// own committed corpus independently, exactly like the four dialect lowerings do).
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

/// Merge the mapping-DSL source set (`dsl/mappings/**/*.ttl` ⊕ sorted slice `Mapping`
/// artifacts) — the same merge `correspondence_lower::merge_dsl` performs. This is the
/// carrier of every `gmeow:ProjectionMapping` cell the EDOAL lowering renders from; the
/// entity2 template-coherence check (`check_edoal_entity_kind`'s check B) re-parses it via
/// [`get_leg::projections`] to correlate a committed EDOAL cell to its authoring template.
fn merge_dsl(root: &Path, catalog: Option<&SliceCatalog>) -> Result<Arc<RdfDataset>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let mut files = Vec::new();
    collect_ttl_files(&root.join("dsl").join("mappings"), &mut files)?;
    files.sort();
    for path in &files {
        let bytes = std::fs::read(path).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, &path.display().to_string())?;
        b.push_dataset(&ds);
    }
    if let Some(catalog) = catalog {
        let mut artifacts: Vec<(PathBuf, &[u8])> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role == ArtifactRole::Mapping {
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
    }
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// Load every SSSOM mapping row from `generated/mappings/*.sssom.tsv` (sorted by path,
/// the same source + order the retired `load_sssom_mappings` read).
fn load_sssom_mappings(root: &Path) -> Result<Vec<Mapping>, SliceError> {
    // GENERATED-READ-OK: audit lane. `lint_correspondence_soundness` is NOT a pipeline
    // produce-stage — its only callers are the dev-CLI feedback surfaces
    // (`compile_diagnostics_report` → `py.rs`, the `scoreboards` audit, `py.rs` directly).
    // Its job is to AUDIT the committed `generated/mappings/*.sssom.tsv`, so reading them
    // off disk is correct-by-design, not the stale-disk-fold class (which is a produce
    // stage folding stale bytes into the bundle). None of its output reaches gmeow.gts.
    let mappings_dir = root.join("generated").join("mappings");
    if !mappings_dir.is_dir() {
        return Err(SliceError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "missing SSSOM mappings directory {}",
                mappings_dir.display()
            ),
        )));
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&mappings_dir).map_err(SliceError::Io)? {
        let path = entry.map_err(SliceError::Io)?.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".sssom.tsv"))
        {
            files.push(path);
        }
    }
    files.sort();
    let mut mappings: Vec<Mapping> = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).map_err(SliceError::Io)?;
        let rows = parse_sssom_tsv(&text)
            .map_err(|e| SliceError::Parse(format!("{}: {e}", path.display())))?;
        mappings.extend(rows);
    }
    Ok(mappings)
}

/// The referenced target prefixes: every alignment-target prefix that is the object of a
/// `gmeow:`-subject property mapping whose subject is an ontology property. Mirrors the
/// `referenced` set the retired `lint_alignment_directions` built.
fn referenced_prefixes(mappings: &[Mapping], onto: &DslView<'_>) -> BTreeSet<String> {
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for m in mappings {
        if !m.subject_id.starts_with(GMEOW_PREFIX) && !m.subject_id.starts_with(LOGIC_PREFIX) {
            continue;
        }
        let Some(prefix) = prefix_of(&m.object_id) else {
            continue;
        };
        let Some(subj_iri) = expand_curie(&m.subject_id) else {
            continue;
        };
        let is_property = onto.objects_of(&subj_iri, RDF_TYPE).into_iter().any(|t| {
            matches!(
                t.as_iri(),
                Some(OWL_OBJECT_PROPERTY) | Some(OWL_DATATYPE_PROPERTY)
            )
        });
        if !is_property {
            continue;
        }
        referenced.insert(prefix);
    }
    referenced
}

/// Load + merge the target-axiom dataset for a prefix (snapshot ⊕ fixture), or `None`
/// when neither exists.
fn load_target_axioms(root: &Path, prefix: &str) -> Result<Option<Arc<RdfDataset>>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let mut has = false;

    let snapshot = root
        .join("imports")
        .join("targets")
        .join(format!("{prefix}.ttl"));
    if snapshot.is_file() {
        let bytes = std::fs::read(&snapshot).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, &snapshot.display().to_string())?;
        b.push_dataset(&ds);
        has = true;
    }
    let fixture = root
        .join("tests")
        .join("fixtures")
        .join("target_axioms")
        .join(format!("{prefix}.ttl"));
    if fixture.is_file() {
        let bytes = std::fs::read(&fixture).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, &fixture.display().to_string())?;
        b.push_dataset(&ds);
        has = true;
    }

    if !has {
        return Ok(None);
    }
    let ds = b.freeze().map_err(|e| SliceError::Parse(e.to_string()))?;
    Ok(Some(ds))
}

/// The merged FnO catalog dataset (`functions.fno.ttl` ⊕ `transforms.fno.ttl`, the latter
/// falling back to `dsl/mappings/transforms.fno.ttl`).
fn fno_catalog(root: &Path, projections: &Path) -> Result<Arc<RdfDataset>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let functions = projections.join(FNO_FUNCTIONS_FILE);
    let bytes = std::fs::read(&functions).map_err(SliceError::Io)?;
    let functions_ds = parse_turtle(&bytes, &functions.display().to_string())?;
    b.push_dataset(&functions_ds);

    let transforms = projections.join(FNO_TRANSFORMS_FILE);
    let transforms = if transforms.is_file() {
        transforms
    } else {
        root.join("dsl").join("mappings").join(FNO_TRANSFORMS_FILE)
    };
    let bytes = std::fs::read(&transforms).map_err(SliceError::Io)?;
    let transforms_ds = parse_turtle(&bytes, &transforms.display().to_string())?;
    b.push_dataset(&transforms_ds);
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// Every committed `*.edoal.ttl` under `projections`, sorted by path.
fn edoal_datasets(projections: &Path) -> Result<Vec<(String, Arc<RdfDataset>)>, SliceError> {
    let mut files: Vec<PathBuf> = Vec::new();
    if projections.is_dir() {
        for entry in std::fs::read_dir(projections).map_err(SliceError::Io)? {
            let path = entry.map_err(SliceError::Io)?.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".edoal.ttl"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut out: Vec<(String, Arc<RdfDataset>)> = Vec::new();
    for path in &files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let bytes = std::fs::read(path).map_err(SliceError::Io)?;
        out.push((name, parse_turtle(&bytes, &path.display().to_string())?));
    }
    Ok(out)
}

/// Fetch a target vocabulary over the network, keeping only the minimal structural axiom
/// subset (domain/range/inverse + property types) in the target namespace. Mirrors the
/// retired Python `target_axioms.fetch_target_axioms`. Only reachable with
/// `allow_network=true` (the on-gate path uses `false`).
fn fetch_target_axioms(prefix: &str) -> Result<Arc<RdfDataset>, SliceError> {
    let source = TARGET_SOURCES
        .iter()
        .find(|s| s.prefix == prefix)
        .ok_or_else(|| {
            SliceError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no network source configured for target {prefix}"),
            ))
        })?;
    let namespace = gmeow_logic_compile::ingest::registry_iri(prefix).ok_or_else(|| {
        SliceError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no namespace configured for target {prefix}"),
        ))
    })?;

    let response = ureq::get(source.url)
        .header(
            "User-Agent",
            "gmeow-pipeline/0.1 (ontology correspondence-soundness validator)",
        )
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|e| {
            SliceError::Io(std::io::Error::other(format!(
                "network fetch failed for {prefix}: {e}"
            )))
        })?;
    let bytes = response
        .into_body()
        .into_with_config()
        .read_to_vec()
        .map_err(|e| SliceError::Io(std::io::Error::other(format!("read body {prefix}: {e}"))))?;

    let parsed = parse_dataset(&bytes, source.media_type, None)
        .map_err(|e| SliceError::Parse(format!("parse error fetching {prefix}: {e}")))?;

    // Keep only the structural axiom / property-type quads whose subject is in the target
    // namespace (the historical filter), rebuilt as a fresh dataset.
    let mut b = RdfDatasetBuilder::new();
    for q in parsed.quads_for_pattern(None, None, None, GraphMatch::Default) {
        let TermRef::Iri(subj) = parsed.resolve(q.s) else {
            continue;
        };
        if !subj.starts_with(namespace) {
            continue;
        }
        let TermRef::Iri(pred) = parsed.resolve(q.p) else {
            continue;
        };
        if !is_axiom_or_property_type(pred, &parsed.resolve(q.o)) {
            continue;
        }
        // Re-intern this quad into the filtered dataset (subject is an IRI; predicate an
        // IRI; object an IRI or literal — the structural axioms only carry IRI objects,
        // but a permissive re-intern keeps any object kind).
        let s = b.intern_iri(subj);
        let p = b.intern_iri(pred);
        let o = intern_object(&mut b, &parsed, q.o);
        b.push_quad(s, p, o, None);
    }
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// Re-intern an object term (IRI / literal / blank) of `ds` into the filtered builder.
fn intern_object(
    b: &mut RdfDatasetBuilder,
    ds: &RdfDataset,
    obj: purrdf::TermId,
) -> purrdf::TermId {
    match ds.resolve(obj) {
        TermRef::Iri(iri) => b.intern_iri(iri),
        TermRef::Blank { label, scope } => b.intern_blank(label, scope),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => {
            let lit = match language {
                Some(lang) => {
                    purrdf::RdfLiteral::language_tagged(lexical.to_owned(), lang.to_owned())
                }
                None => match ds.resolve(datatype) {
                    TermRef::Iri(dt) => {
                        purrdf::RdfLiteral::typed(lexical.to_owned(), dt.to_owned())
                    }
                    _ => purrdf::RdfLiteral::simple(lexical.to_owned()),
                },
            };
            b.intern_literal(lit)
        }
        TermRef::Triple { .. } => b.intern_iri("urn:gmeow:unsupported-triple-term"),
    }
}

/// Whether a quad is a structural axiom or an `rdf:type` naming a property kind.
fn is_axiom_or_property_type(pred: &str, obj: &TermRef<'_>) -> bool {
    if matches!(
        pred,
        RDFS_DOMAIN
            | RDFS_RANGE
            | OWL_INVERSE_OF
            | SCHEMA_INVERSE_OF
            | SCHEMA_DOMAIN_INCLUDES
            | SCHEMA_RANGE_INCLUDES
            | RDFS_SUB_CLASS_OF
            | OWL_EQUIVALENT_CLASS
            | OWL_EQUIVALENT_PROPERTY
            | SKOS_EXACT_MATCH
    ) {
        return true;
    }
    if pred == RDF_TYPE
        && let TermRef::Iri(o) = obj
    {
        return OWL_PROPERTY_TYPES.contains(o);
    }
    false
}

/// Owned, parsed corpus the pure soundness pass borrows. Holds the `Arc<RdfDataset>`s alive
/// so the `DslView`s built in [`Corpus::run`] stay valid.
struct Corpus {
    ontology: Arc<RdfDataset>,
    mappings: Vec<Mapping>,
    target_graphs: BTreeMap<String, Arc<RdfDataset>>,
    network_failed: BTreeMap<String, String>,
    fno: Arc<RdfDataset>,
    edoal: Vec<(String, Arc<RdfDataset>)>,
    /// Every parsed `gmeow:ProjectionMapping` cell (from `merge_dsl`'s view) — owned
    /// (no lifetime tied to the DSL dataset), the entity2 template-coherence check's
    /// correlation authority.
    cells: Vec<ProjectionCell>,
}

impl Corpus {
    /// Build all `DslView`s and run the seven-check soundness pass.
    fn run(&self) -> Vec<ProjectionDiagnostic> {
        let onto = DslView::new(&self.ontology);
        let fno = DslView::new(&self.fno);
        let targets: BTreeMap<String, DslView<'_>> = self
            .target_graphs
            .iter()
            .map(|(p, ds)| (p.clone(), DslView::new(ds)))
            .collect();
        let edoal: Vec<(String, DslView<'_>)> = self
            .edoal
            .iter()
            .map(|(n, ds)| (n.clone(), DslView::new(ds)))
            .collect();
        let inputs = soundness::SoundnessInputs {
            ontology: &onto,
            target_graphs: &targets,
            network_failed: &self.network_failed,
            mappings: &self.mappings,
            fno: &fno,
            edoal: &edoal,
            cells: &self.cells,
        };
        soundness::run_soundness(&inputs)
    }
}

/// Read + parse the committed corpus under `root` and run the seven correspondence
/// soundness checks oxigraph-free, returning every problem as a [`ProjectionDiagnostic`].
///
/// `allow_network` mirrors the retired flag: the on-gate path passes `false` (snapshots /
/// fixtures only). With `true`, referenced prefixes still missing a snapshot/fixture are
/// fetched from their canonical source document (a failure yields the same INFO finding as
/// the retired lint).
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source (the ontology, the
/// SSSOM tables, the FnO catalog) — no degraded fallback (CONSTITUTION / no-compromises).
pub fn lint_correspondence_soundness(
    root: &Path,
    allow_network: bool,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let slices_dir = root.join("slices");
    let catalog = if slices_dir.is_dir() {
        Some(SliceCatalog::discover(
            &slices_dir,
            crate::gmeow_ns::gmeow_slice_vocab(),
        )?)
    } else {
        None
    };

    let ontology = merge_ontology(root, catalog.as_ref())?;
    let mappings = load_sssom_mappings(root)?;

    // The mapping-DSL corpus (`dsl/mappings/` ⊕ slice `Mapping` artifacts): every
    // `gmeow:ProjectionMapping` cell, parsed via the SAME shared get-leg model the EDOAL
    // lowering renders from — the entity2 template-coherence check's correlation
    // authority. Scoped so the view's borrow of `dsl` ends before `cells` (owned, no
    // lifetime) outlives it.
    let dsl = merge_dsl(root, catalog.as_ref())?;
    let cells: Vec<ProjectionCell> = {
        let dsl_view = DslView::new(&dsl);
        get_leg::projections(&dsl_view).map_err(|e| SliceError::Parse(e.to_string()))?
    };

    // Scope the ontology view so its borrow of `ontology` ends before `ontology` is moved
    // into the `Corpus` below (the view is only needed to compute the referenced prefixes).
    let referenced = {
        let onto_view = DslView::new(&ontology);
        referenced_prefixes(&mappings, &onto_view)
    };

    let mut target_graphs: BTreeMap<String, Arc<RdfDataset>> = BTreeMap::new();
    for prefix in &referenced {
        if let Some(ds) = load_target_axioms(root, prefix)? {
            target_graphs.insert(prefix.clone(), ds);
        }
    }

    // For referenced prefixes still missing snapshots/fixtures, optionally fetch live.
    let mut network_failed: BTreeMap<String, String> = BTreeMap::new();
    if allow_network {
        let missing: Vec<String> = referenced
            .iter()
            .filter(|p| !target_graphs.contains_key(*p))
            .cloned()
            .collect();
        for prefix in missing {
            if TARGET_SOURCES.iter().any(|s| s.prefix == prefix) {
                match fetch_target_axioms(&prefix) {
                    Ok(ds) => {
                        target_graphs.insert(prefix.clone(), ds);
                    }
                    Err(e) => {
                        network_failed.insert(prefix.clone(), e.to_string());
                    }
                }
            }
        }
    }

    // GENERATED-READ-OK: audit lane (see load_sssom_mappings) — this lints the committed
    // generated/projections EDOAL/FnO output; not a produce-stage read, never folds into gmeow.gts.
    let projections = root.join("generated").join("projections");
    let fno = fno_catalog(root, &projections)?;
    let edoal = edoal_datasets(&projections)?;

    let corpus = Corpus {
        ontology,
        mappings,
        target_graphs,
        network_failed,
        fno,
        edoal,
        cells,
    };
    Ok(corpus.run())
}
