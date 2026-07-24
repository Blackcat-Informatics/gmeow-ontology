// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `source_load` stage (P3): parse the authored ontology sources into
//! one in-memory base graph.
//!
//! This is the root of the build DAG. It loads `ontology/gmeow.ttl`, every
//! `slices/<group>/<name>/module.ttl`, and every `imports/*.ttl` into a single
//! native [`purrdf::RdfDataset`] — the RDF 1.1 base graph assembled directly from
//! canonical sources. The dataset is the
//! frozen carrier downstream stages union and project from, with the N-Quads byte
//! lane published alongside so the pre-carrier byte readers parse it from memory
//! instead of re-reading `gmeow.gts` from disk per generator (the bottleneck
//! this removes). Every parse routes through the native
//! `purrdf::parse_dataset` codecs and merges via `RdfDataset::union`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use std::collections::HashMap;

use purrdf::provenance::{DatasetProvenance, OriginKind};
use purrdf::{
    QuadHandle, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, RdfTriple, SerializeGraph,
    flat_rdf_quads_from_dataset, parse_dataset, serialize_dataset,
};

use gmeow_logic_compile::ir::ANNOTATION_LIFT_PREDS;

use crate::node::{SOURCE_ORIGIN, Stage, StageInput, StageOutput, StageProduct};

/// The `OriginKind` an authored file contributes, by its repo-relative role:
/// `ontology/gmeow.ttl` is the [`OriginKind::RootOntology`], every `imports/*.ttl`
/// is an [`OriginKind::Import`], and every slice `module.ttl` is an
/// [`OriginKind::Source`]. The classification is a pure function of the path, so the
/// provenance attribution is reproducible (no-optionality — every authored file maps
/// to a concrete kind, never an unknown).
fn authored_origin_kind(root: &Path, path: &Path) -> OriginKind {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    if rel_str == "ontology/gmeow.ttl" {
        OriginKind::RootOntology
    } else if rel_str.starts_with("imports/") {
        OriginKind::Import
    } else {
        OriginKind::Source
    }
}

/// Build the per-quad provenance sidecar for the authored base graph (C9).
///
/// Every authored file (`ontology/gmeow.ttl`, every slice `module.ttl`, every
/// `imports/*.ttl`) is registered as one compilation [`unit`](DatasetProvenance::register_unit)
/// — by its repo-relative path, with the path-derived [`OriginKind`] — and one
/// [`artifact`](DatasetProvenance::register_artifact) under that same path. Each quad the
/// file contributes is recorded as one [`AssertionOccurrence`](purrdf::provenance::AssertionOccurrence)
/// keyed by a content-deduplicated [`QuadHandle`]: two files asserting the same triple
/// collapse to ONE handle but TWO occurrences (the set-valued S0.3 invariant). Blank-node
/// labels are standardized per file (the same FNV scope the load store uses), so a
/// structurally-distinct blank axiom in two files keeps two handles.
///
/// Returns `(provenance, expected_handles)` where `expected_handles` is every distinct
/// handle minted — the coverage set [`check_provenance`](purrdf::provenance::check_provenance)
/// asserts is fully attributed. An UNATTRIBUTED authored quad is impossible by
/// construction (every quad is recorded as it is seen); the gate is the hard-fail proof.
pub fn attributed_base_provenance(
    root: &Path,
) -> Result<(DatasetProvenance, Vec<QuadHandle>), gmeow_errors::Diag> {
    let mut prov = DatasetProvenance::new();
    // Content key (the per-file-scoped native quad, location stripped so two identical
    // triples on different source lines collapse exactly as the old oxigraph quad key
    // did) → its deduplicated handle. Two files asserting an identical triple share the
    // handle but record distinct occurrences (the set-valued S0.3 invariant).
    let mut handle_of: HashMap<RdfQuad, QuadHandle> = HashMap::new();
    let mut next: u32 = 0;

    for path in authored_files(root)? {
        let bytes = std::fs::read(&path)?;
        let scope = path.display().to_string();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let kind = authored_origin_kind(root, &path);
        let unit = prov.register_unit(rel.clone(), kind);
        let artifact = prov.register_artifact(rel);

        let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("syntax error in {scope}: {e}"),
            })
        })?;
        // SCOPE blank labels by the source path: each authored file is a distinct RDF
        // document whose anonymous blanks restart per parse, so a structurally-distinct
        // blank axiom in two files must keep two handles. The native flat un-fold mirrors
        // the old `flat_oxigraph_quads_from_dataset_scoped` exactly (same FNV prefix), and
        // the location is stripped so the dedup key is the pure `(s, p, o, g)` content.
        let prefix = blank_scope_prefix(&scope);
        let quads = flat_rdf_quads_from_dataset(&dataset);
        for quad in quads {
            let key = rescope_quad_blanks_keyless(&quad, &prefix);
            let handle = *handle_of.entry(key).or_insert_with(|| {
                let h = QuadHandle::from_index(next);
                next += 1;
                h
            });
            prov.record_occurrence(handle, unit, artifact, None);
        }
    }

    let mut expected: Vec<QuadHandle> = handle_of.into_values().collect();
    expected.sort_unstable_by_key(|h| h.index());
    Ok((prov, expected))
}

/// A stable (FNV-1a) blank-node label prefix for a source document — the native twin
/// of `purrdf::oxigraph::flat_oxigraph_quads_from_dataset_scoped`'s scoping, kept
/// byte-identical so the per-file provenance handle partition (and thus the committed
/// `graph/provenance` projection) is preserved across the oxigraph removal.
/// Deterministic across processes and stages — the same `scope_key` always yields the
/// same prefix.
fn blank_scope_prefix(scope_key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in scope_key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("g{hash:016x}")
}

/// Rescope every blank-node label in `term` with `prefix`, recursing into quoted triples.
fn rescope_term_blanks(term: &RdfTerm, prefix: &str) -> RdfTerm {
    match term {
        RdfTerm::BlankNode(label) => RdfTerm::blank_node(format!("{prefix}{label}")),
        RdfTerm::Triple(triple) => RdfTerm::triple(RdfTriple::new(
            rescope_term_blanks(&triple.subject, prefix),
            triple.predicate.clone(),
            rescope_term_blanks(&triple.object, prefix),
        )),
        other => other.clone(),
    }
}

/// Build the location-free, blank-scoped dedup key for one native quad. The location is
/// dropped (the old oxigraph `Quad` key carried none) so two identical triples collapse to
/// ONE handle, and blank labels are prefixed by the per-source scope so distinct blanks
/// across files stay distinct.
fn rescope_quad_blanks_keyless(quad: &RdfQuad, prefix: &str) -> RdfQuad {
    let mut key = RdfQuad::new(
        rescope_term_blanks(&quad.subject, prefix),
        quad.predicate.clone(),
        rescope_term_blanks(&quad.object, prefix),
    );
    key.graph_name = quad
        .graph_name
        .as_ref()
        .map(|g| rescope_term_blanks(g, prefix));
    key
}

/// Logical path of the published base graph (N-Quads, in-memory dataflow).
pub const BASE_GRAPH_PATH: &str = "pipeline/base-graph.nq";

/// Build the authored subject→source-position [`SpanIndex`](crate::ingest::SpanIndex)
/// under the FIXED span policy: emit spans for the [`OriginKind::RootOntology`] and
/// [`OriginKind::Source`] files (the root ontology + slice modules) and SUPPRESS the
/// [`OriginKind::Import`] files (`imports/*.ttl`). This is a pure function of the path —
/// the same [`authored_origin_kind`] classification the provenance sidecar uses, with no
/// knob. Each file is ingested THROUGH the swappable [`SourceAdapter`](crate::ingest::SourceAdapter)
/// (today `purrdf`), so source position comes from ingestion, never a re-scan; the
/// per-file contributions merge into one index, each file's path interned once.
pub fn build_source_span_index(
    root: &Path,
) -> Result<crate::ingest::SpanIndex, gmeow_errors::Diag> {
    use crate::ingest::SourceAdapter;
    let adapter = crate::ingest::PurrdfAdapter;
    let mut index = crate::ingest::SpanIndex::new();
    for path in authored_files(root)? {
        // Fixed policy: RootOntology + Source contribute spans; Import is suppressed.
        if matches!(authored_origin_kind(root, &path), OriginKind::Import) {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let ingested = adapter.ingest(&rel, "text/turtle", &bytes)?;
        index.merge(ingested.spans.into_index());
    }
    Ok(index)
}

/// Load `ontology/gmeow.ttl` + all slice modules + all imports into one frozen dataset.
///
/// Each authored file is parsed standalone (its anonymous blanks `_:gts_<counter>`
/// restart at 0 per parse), and the per-file datasets are merged via
/// [`RdfDataset::union`], which standardizes blank scopes apart per input (the native
/// twin of the old per-source FNV blank-prefix ingest) so two files' identically-labelled
/// anonymous blanks stay disjoint. The union canonicalizes on freeze, so the result is
/// order-independent.
pub fn load_authored_dataset(root: &Path) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::new();
    for path in authored_files(root)? {
        let bytes = std::fs::read(&path)?;
        let scope = path.display().to_string();
        let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("syntax error in {scope}: {e}"),
            })
        })?;
        parsed.push(dataset);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(|d| d.as_ref()).collect();
    Ok(Arc::new(RdfDataset::union(&refs)))
}

/// Predicates carrying NO input the `stage-compile-logic` augmentation readers key on —
/// documentation (labels/definitions/notes), alignment/mapping assertions, foreign-domain
/// example vocabulary, or bibliographic/provenance/catalogue metadata. Removing every triple
/// on one of these predicates from the corpus compile-logic consumes is therefore SOUND: the
/// five readers (`derive_validation_shapes`, `extract_all_constraints`,
/// `extract_correspondences`, `extract_leg_programs`, `MetaProgram::from_source_dataset`) read
/// ONLY the OWL/RDFS/XSD restriction + facet vocabulary, the `logic:` Constraint/Formula/sugar/
/// closure/Correspondence/leg vocabulary, the `gmeow:enforcesFailureClass`/`DiagnosticMetaRule`/
/// `categoryPolarity` diagnostic-meta wiring, the `gm:` leg paths, and `rdfs:comment`
/// (caveats) — never a predicate denylisted here. The
/// `logic_compile_input_subgraph_preserves_reader_output` guard proves reader-output identity
/// over the REAL corpus, so this denylist can only ever be a strict subset of the never-read
/// predicates: a mistaken addition of a genuinely-read predicate makes that guard RED.
///
/// `rdfs:comment` is DELIBERATELY absent — it IS read (`read_caveats`), so stripping it
/// would silently drop authored caveats. The read namespaces (`rdf:`, `rdfs:` subClassOf/
/// domain/range/comment, `owl:`, `xsd:`, `logic:`, `gmeow:`/`gm:`) are NEVER denylisted.
///
/// These are the EXACT-IRI members; whole namespace-prefix families (SKOS, SSSOM, gUFO,
/// schema.org, Wikidata, FOAF, and the bibliographic-metadata families) are matched by prefix
/// in [`LOGIC_COMPILE_INPUT_DENYLIST_PREFIXES`].
pub const LOGIC_COMPILE_INPUT_DENYLIST: &[&str] = &[
    // RDFS presentational/navigational documentation predicates. NOT rdfs:label — that is now
    // LIFTED into a NodeKind::Annotation axiom (`ANNOTATION_LIFT_PREDS`), so it is READ and must
    // survive (the annotation-exception early-return in `predicate_is_logic_compile_denylisted`
    // keeps it out of the strip). NOT rdfs:comment either — read by `read_caveats` AND lifted.
    // Only these genuinely-inert navigational RDFS predicates are stripped. (SKOS annotations are
    // un-denied by the same exception; the SKOS *mapping* surface stays denied by the prefix
    // family below.)
    "http://www.w3.org/2000/01/rdf-schema#seeAlso",
    "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
];

/// Predicate-IRI namespace PREFIXES whose EVERY predicate is inert to the compile-logic
/// augmentation readers — matched by predicate-IRI `starts_with`, so the whole family is
/// denylisted without enumerating each term. Four groups, all provably never read (the
/// `logic_compile_input_subgraph_preserves_reader_output` guard REDs on any family that is):
///   * Documentation / annotation: SKOS (`skos:`). The prefix denies the SKOS mapping surface
///     (closeMatch/exactMatch/relatedMatch/broadMatch/…) — pure alignment surface the
///     compile-logic augmentation readers never consult. EXCEPTION: the four SKOS *annotation*
///     predicates (skos:definition/prefLabel/altLabel/scopeNote) are un-denied by the
///     `ANNOTATION_LIFT_PREDS` early-return in `predicate_is_logic_compile_denylisted` — they
///     are LIFTED into NodeKind::Annotation axioms and so ARE read (the annotation reader in the
///     `logic_compile_input_subgraph_preserves_reader_output` guard REDs if they are re-denied).
///   * Alignment/mapping provenance: SSSOM `semapv:` mapping-justification vocabulary.
///   * Foreign-domain example / correspondence-target vocabularies: gUFO (`gufo:`),
///     schema.org, FOAF (`foaf:`), and Wikidata (`wd:`/`wdt:`/`wikibase:`) — used only as
///     example/target data, never as a predicate any reader walks.
///   * Bibliographic / provenance / catalogue metadata: Dublin Core Terms (`dcterms:`), legacy
///     Dublin Core Elements (`dc:`), DCMI Type (`dcmitype:`), PROV-O (`prov:`), PAV (`pav:`),
///     VANN (`vann:`), VoID (`void:`), DCAT (`dcat:`), DQV (`dqv:`), CiTO (`cito:`), BIBO
///     (`bibo:`), PREMIS, CIDOC-CRM (`crm:`), ORG (`org:`), vCard (`vcard:`), Time (`time:`),
///     and Web Annotation (`oa:`).
///
/// None overlaps a read namespace (`rdf:`/`rdfs:`/`owl:`/`xsd:`/`logic:`/`gmeow:`/`gm:`).
pub const LOGIC_COMPILE_INPUT_DENYLIST_PREFIXES: &[&str] = &[
    // Documentation / annotation + the SKOS mapping surface.
    "http://www.w3.org/2004/02/skos/core#",
    // SSSOM mapping-justification provenance.
    "https://w3id.org/semapv/vocab/",
    // Foreign-domain example / correspondence-target vocabularies.
    "http://purl.org/nemo/gufo#",
    "https://schema.org/",
    "http://xmlns.com/foaf/0.1/",
    "http://www.wikidata.org/entity/",
    "http://www.wikidata.org/prop/direct/",
    "http://wikiba.se/ontology#",
    // Bibliographic / provenance / catalogue metadata families.
    "http://purl.org/dc/terms/",
    "http://purl.org/dc/elements/1.1/",
    "http://purl.org/dc/dcmitype/",
    "http://www.w3.org/ns/prov#",
    "http://purl.org/pav/",
    "http://purl.org/vocab/vann/",
    "http://rdfs.org/ns/void#",
    "http://www.w3.org/ns/dcat#",
    "http://www.w3.org/ns/dqv#",
    "http://purl.org/spar/cito/",
    "http://purl.org/ontology/bibo/",
    "http://www.loc.gov/premis/rdf/v3/",
    "http://www.cidoc-crm.org/cidoc-crm/",
    "http://www.w3.org/ns/org#",
    "http://www.w3.org/2006/vcard/ns#",
    "http://www.w3.org/2006/time#",
    "http://www.w3.org/ns/oa#",
];

/// Whether a quad's PREDICATE IRI is a pure-documentation predicate the compile-logic
/// readers never consult — an exact [`LOGIC_COMPILE_INPUT_DENYLIST`] member or a
/// [`LOGIC_COMPILE_INPUT_DENYLIST_PREFIXES`] namespace member.
fn predicate_is_logic_compile_denylisted(predicate: &str) -> bool {
    // The six RDFS/SKOS annotation predicates are LIFTED into NodeKind::Annotation axioms
    // (`isSupersetOf` SKOS/RDFS), so they are READ and must reach the compiler — surgically
    // un-denied BEFORE the prefix scan. This exception un-denies exactly
    // skos:definition/prefLabel/altLabel/scopeNote (leaving the rest of the `skos:` prefix —
    // notably the skos:*Match mapping surface — denied) and rdfs:label (rdfs:comment is already
    // off the exact denylist). The `logic_compile_input_subgraph_preserves_reader_output`
    // annotation reader REDs if any of these is silently re-denied.
    if ANNOTATION_LIFT_PREDS.contains(&predicate) {
        return false;
    }
    LOGIC_COMPILE_INPUT_DENYLIST.contains(&predicate)
        || LOGIC_COMPILE_INPUT_DENYLIST_PREFIXES
            .iter()
            .any(|ns| predicate.starts_with(ns))
}

/// The SOUND (denylist) narrowing of the corpus `stage-compile-logic` reads: `base` with
/// every quad whose predicate is a pure-documentation predicate
/// ([`predicate_is_logic_compile_denylisted`]) REMOVED, and every other quad — including
/// the bnode-connected OWL restriction / `logic:` formula / correspondence structures —
/// preserved verbatim. The denylist predicates only ever appear on named-subject
/// documentation triples (a label/definition/note on a class or property), NEVER inside a
/// restriction/formula/leg bnode tree, so filtering by predicate leaves every structure the
/// readers walk intact. The RDF-1.2 statement layer (reifiers/annotations) is carried across
/// verbatim (mirroring [`crate::stages::carrier::rooted_in_graph`]).
///
/// `stage-source-load` publishes this as its `graph/logic-compile-inputs` named graph so
/// compile-logic reads a NARROWED, typed entity instead of re-parsing the whole corpus, and
/// a documentation-only edit no longer busts the compiler's cache. Reader-output identity
/// against the full corpus is proved by the
/// `logic_compile_input_subgraph_preserves_reader_output` guard.
pub fn logic_compile_input_subgraph(
    base: &RdfDataset,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in base.owned_quads() {
        if predicate_is_logic_compile_denylisted(&quad.predicate) {
            continue;
        }
        builder.push_owned_quad(&quad);
    }
    // Carry the RDF-1.2 statement layer across verbatim — the denylist is a triple-level
    // predicate filter, so the reifier/annotation side tables ride untouched.
    for reifier in base.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in base.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-source-load".to_string(),
            message: format!("freeze logic-compile-inputs subgraph: {e}"),
        })
    })
}

/// The sorted authored Turtle files that form the base graph (the hidden-input
/// closure `source_load` declares so the cache key cannot go stale).
pub fn authored_files(root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut files: Vec<PathBuf> = Vec::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.exists() {
        files.push(onto);
    }
    files.extend(module_files(root)?);
    files.extend(ttl_files_in(&root.join("imports"))?);
    files.sort();
    Ok(files)
}

/// Every `slices/<group>/<name>/module.ttl`.
pub fn module_files(root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut out = Vec::new();
    let slices = root.join("slices");
    for group in sorted_dirs(&slices)? {
        for slice_dir in sorted_dirs(&group)? {
            let module = slice_dir.join("module.ttl");
            if module.is_file() {
                out.push(module);
            }
        }
    }
    Ok(out)
}

/// Every `slices/<group>/<name>/manifest.ttl` (the sibling of each `module.ttl`),
/// for export leaves whose cache key must reflect the slice manifests they read
/// directly from disk (catalog, profiles, matrix — `gmeow:sliceProfile` /
/// `sliceTier` / `sliceDependsOn` live in the manifest, NOT the composed fold).
pub fn manifest_files(root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut out = Vec::new();
    for module in module_files(root)? {
        let manifest = module.with_file_name("manifest.ttl");
        if manifest.is_file() {
            out.push(manifest);
        }
    }
    Ok(out)
}

/// Every `slices/<group>/<name>/manifest.ttl`, INCLUDING slices that have no
/// `module.ttl` — the profile-tier pure-selection slices that mint nothing
/// (Principle 16) and so carry only a manifest declaring their `sliceDependsOn`
/// selection. `manifest_files` is deliberately module-gated (a slice that loads
/// nothing has no composed fold); the profiles stage uses THIS to discover the
/// selection-only profile slices whose dependency closure it emits.
pub fn all_manifest_files(root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut out = Vec::new();
    let slices = root.join("slices");
    for group in sorted_dirs(&slices)? {
        for slice_dir in sorted_dirs(&group)? {
            let manifest = slice_dir.join("manifest.ttl");
            if manifest.is_file() {
                out.push(manifest);
            }
        }
    }
    Ok(out)
}

fn ttl_files_in(dir: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    // Same NotFound-is-empty contract as `sorted_dirs`: an absent directory yields an
    // empty listing, any other IO error hard-fails.
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "ttl") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    // An absent directory is not an error — the caller iterating an optional tree gets
    // an empty listing (no manual existence pre-check needed). Any OTHER IO error (e.g.
    // permission denied) and per-entry errors still fail-fast: a transient FS error must
    // surface, not silently drop a slice group/dir (no-optionality).
    let mut out: Vec<PathBuf> = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Serialize a frozen [`RdfDataset`] to the deterministic N-Quads byte form (full
/// RDF 1.2 statement layer, lines sorted bytewise ascending, trailing newline). This
/// is the single dataset → N-Quads projection the pipeline's in-memory dataflow speaks;
/// the `gts_compose` stage projects the composed UNION dataset through it for its byte
/// lane.
pub fn dataset_to_sorted_nquads(
    dataset: &purrdf::RdfDataset,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let buf = serialize_dataset(dataset, "application/n-quads", SerializeGraph::Dataset).map_err(
        |e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("serialize failed: {e}"),
            })
        },
    )?;
    // Sort lines for determinism (serializer iteration order is not guaranteed).
    let text = String::from_utf8(buf).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("non-utf8 n-quads: {e}"),
        })
    })?;
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    lines.sort_unstable();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

/// Parse the published base-graph N-Quads artifact back into a frozen dataset (the
/// in-memory hand-off downstream stages use instead of re-reading from disk).
pub fn parse_base_graph(bytes: &[u8]) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    parse_dataset(bytes, "application/n-quads", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("base-graph parse: {e}"),
        })
    })
}

/// Parse RDF text `bytes` of `media_type` into a fresh frozen [`RdfDataset`] via the
/// native codecs, preserving named graphs. `context` labels parse errors.
pub fn rdf_bytes_to_dataset(
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    parse_dataset(bytes, media_type, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("syntax error in {context}: {e}"),
        })
    })
}

/// Parse Turtle `bytes` into a fresh frozen [`RdfDataset`] via the native codecs.
///
/// The native `parse_dataset` folds the RDF 1.2 statement layer; a stand-alone Turtle
/// document only ever populates the default graph. `context` labels parse errors.
pub fn turtle_bytes_to_dataset(
    bytes: &[u8],
    context: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    rdf_bytes_to_dataset(bytes, "text/turtle", context)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `source_load` pipeline stage — the authored-source loader. Holds
/// [`SOURCE_ORIGIN`], so the scheduler stamps its emitted quads' provenance origin as
/// `Source` (the kind-enum replacement: origin is read off a capability, not a tag).
pub struct SourceLoadStage {
    capabilities: Vec<String>,
}

impl SourceLoadStage {
    /// Construct the loader, declaring the [`SOURCE_ORIGIN`] capability (mirrored by
    /// the slice `gmeow:stage-source-load gmeow:hasCapability gmeow:sourceOrigin`).
    pub fn new() -> Self {
        Self {
            capabilities: vec![SOURCE_ORIGIN.to_string()],
        }
    }
}

impl Default for SourceLoadStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for SourceLoadStage {
    fn id(&self) -> &str {
        "stage-source-load"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
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
        // v2: attach the self-description named graphs (authored-default / imports /
        // metadata / alignments / slice-analysis / verify / provenance) so the presenter
        // reads them instead of re-loading + re-canonicalizing the sources on the serial
        // snapshot node (PIPELINE_SPINE §3.2/§4). The BASE_GRAPH_PATH byte lane and the
        // default-graph fold `gts_compose` takes are unchanged.
        // v3: attach the authored subject→source-position SpanIndex as the digest-pinned
        // REP_SPAN_TABLE blob (the fixed span policy — RootOntology+Source, Import
        // suppressed) so the diagnostics consumers lift source coordinates onto findings.
        // v4: score slice quality once at the DAG root, emitting both the queryable
        // quality-assessment graph and the internal HTML report artifact consumed by the
        // terminal docs archive.
        // v5: attach the `graph/logic-compile-inputs` named graph — the SOUND (denylist)
        // narrowing of the whole authored corpus compile-logic reads — so compile-logic
        // consumes it as a typed entity and a documentation-only edit no longer busts the
        // compiler's cache.
        "source_load.v5-logic-compile-inputs"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The self-description graphs read authored sources beyond the base authored
        // files: imports, self-description metadata, SSSOM alignments, slice manifests +
        // shapes (slice-analysis / verify), translation catalogs + docs guides (the
        // translated authored default). Declare them ALL so any of these busting the
        // cache re-runs the loader (cache soundness — a stale self-description graph would
        // ship a stale bundle). `build_self_description_dataset` is the single authority
        // for what is read; this mirrors its source closure.
        let mut files = crate::stages::carrier::self_description_source_files(root)?;
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let mut timings = Vec::with_capacity(7);
        // Carry the authored base graph as the bundle's DEFAULT graph (the native
        // contribution `gts_compose`'s default-graph fold unions), and keep emitting the
        // BASE_GRAPH_PATH N-Quads byte lane for the byte readers — BOTH from the base
        // dataset alone, so neither changes as the self-description graphs are added.
        let started = Instant::now();
        let base = load_authored_dataset(input.root)?;
        timings.push(crate::node::StageRunTiming::new(
            "authored-dataset",
            started.elapsed().as_millis(),
        ));
        let started = Instant::now();
        let nq = dataset_to_sorted_nquads(&base)?;
        timings.push(crate::node::StageRunTiming::new(
            "base-nquads",
            started.elapsed().as_millis(),
        ));
        // Score slice quality ONCE at the DAG root: the RDF graph rides in the
        // self-description carrier and the rendered diagnostics HTML rides as an internal
        // pipeline artifact for the terminal docs archive.
        //
        // The DocMaturity axis's constraint catalog is rendered FRESH here from THIS run's
        // authored sources (root ontology + slice modules) and handed to the sweep, never
        // read off the committed generated/catalog/constraint-catalog.nq. That file is
        // absent on a cold tree and the previous run's bytes on a warm one, and a disk read
        // would fail the whole documentation-model build, collapsing every slice's
        // DocMaturity to a vacuous 1.0 (diverging cold-vs-warm — a two-generation
        // determinism break). `render_constraint_catalog` is a PURE function of the authored
        // sources (source-load already declares them as inputs), so it needs no DAG edge to
        // the constraint-catalog stage — which cannot precede this DAG root anyway — and is
        // byte-identical to what that stage produces and the fanout writes.
        let started = Instant::now();
        let catalog_bytes =
            crate::stages::constraint_catalog::render_constraint_catalog(input.root)?;
        let quality =
            gmeow_slice_quality::assessment_artifacts_with_catalog(input.root, &catalog_bytes)
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                        stage: self.id().to_string(),
                        message: format!("quality-assessment sweep: {e}"),
                    })
                })?;
        timings.push(crate::node::StageRunTiming::new(
            "slice-quality",
            started.elapsed().as_millis(),
        ));
        timings.extend(quality.slice_timings.iter().map(|timing| {
            crate::node::StageRunTiming::new(
                format!("slice-quality/{}", timing.slice),
                timing.elapsed_ms,
            )
        }));
        // Attach the self-description named graphs alongside the base default graph — the
        // load + canonicalize the presenter used to do on the serial snapshot node, done
        // ONCE here at the parallel DAG root.
        let started = Instant::now();
        let self_desc = crate::stages::carrier::build_self_description_dataset_with_quality(
            input.root,
            base.as_ref(),
            &quality.nquads,
        )?;
        timings.push(crate::node::StageRunTiming::new(
            "self-description",
            started.elapsed().as_millis(),
        ));
        let started = Instant::now();
        let dataset = Arc::new(RdfDataset::union(&[base.as_ref(), self_desc.as_ref()]));
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), nq);
        artifacts.insert(
            crate::stages::carrier::SLICE_QUALITY_REPORT_HTML_ARTIFACT.to_string(),
            gmeow_errors::render::to_html(&quality.report).into_bytes(),
        );
        timings.push(crate::node::StageRunTiming::new(
            "carrier-union",
            started.elapsed().as_millis(),
        ));
        // Build the authored subject→source-position span index (fixed policy: RootOntology
        // + Source, Import suppressed) and attach it as the digest-pinned REP_SPAN_TABLE
        // raw-JSON blob — the SINGLE source of the source spans the diagnostics consumers
        // lift onto their findings. It rides the by-reference blob lane (cache-replayable),
        // and the scheduler strips it once the last consumer has run.
        let started = Instant::now();
        let span_index = build_source_span_index(input.root)?;
        timings.push(crate::node::StageRunTiming::new(
            "source-spans",
            started.elapsed().as_millis(),
        ));
        let started = Instant::now();
        let span_blob = serde_json::to_vec(&span_index).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("encode source-span table blob: {e}"),
            })
        })?;
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            purrdf::provenance::DatasetProvenance::new(),
            crate::stages::carrier::REP_SPAN_TABLE,
            "application/json",
            span_blob,
        );
        let product = StageProduct::from_bundle(self.id(), Arc::new(bundle));
        timings.push(crate::node::StageRunTiming::new(
            "product-assembly",
            started.elapsed().as_millis(),
        ));
        Ok(StageOutput {
            product,
            diags: Vec::new(),
            timings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn source_load_parses_the_whole_ontology() {
        let root = repo_root();
        let dataset = load_authored_dataset(&root).expect("load");
        // The merged authored graph is substantial (50+ slices); sanity-floor it.
        assert!(
            dataset.quad_count() > 5_000,
            "authored base graph unexpectedly small: {} quads",
            dataset.quad_count()
        );
        // Round-trips through the in-memory N-Quads hand-off.
        let nq = dataset_to_sorted_nquads(&dataset).expect("serialize");
        let back = parse_base_graph(&nq).expect("reparse");
        assert_eq!(dataset.quad_count(), back.quad_count());
    }

    #[test]
    fn authored_files_includes_root_and_modules() {
        let root = repo_root();
        let files = authored_files(&root).unwrap();
        assert!(files.iter().any(|p| p.ends_with("ontology/gmeow.ttl")));
        assert!(
            files
                .iter()
                .any(|p| p.ends_with("slices/core/pipeline/module.ttl"))
        );
        assert!(
            files.len() > 50,
            "expected 50+ authored files, got {}",
            files.len()
        );
    }

    /// Fixed span policy, asserted against the span table read back from the FOLDED
    /// product bundle (not just the live index): a RootOntology + Source file contribute
    /// their subjects; the imports/ (Import) file is SUPPRESSED. Mirrors
    /// `bundle_carries_the_consumer_archives` in reading through the fold accessor.
    #[test]
    fn fixed_policy_emits_source_and_root_suppresses_imports_through_the_fold() {
        use std::sync::Arc;
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        // RootOntology: ontology/gmeow.ttl.
        std::fs::create_dir_all(root.join("ontology")).unwrap();
        std::fs::write(
            root.join("ontology/gmeow.ttl"),
            "@prefix ex: <https://example.test/> .\nex:rootSubject a ex:Root .\n",
        )
        .unwrap();
        // Source: a slice module.ttl.
        std::fs::create_dir_all(root.join("slices/g/n")).unwrap();
        std::fs::write(
            root.join("slices/g/n/module.ttl"),
            "@prefix ex: <https://example.test/> .\nex:sourceSubject a ex:Thing .\n",
        )
        .unwrap();
        // Import: imports/foo.ttl — must be SUPPRESSED.
        std::fs::create_dir_all(root.join("imports")).unwrap();
        std::fs::write(
            root.join("imports/foo.ttl"),
            "@prefix ex: <https://example.test/> .\nex:importSubject a ex:Imported .\n",
        )
        .unwrap();

        // Fold the built index into a product bundle exactly as `run` does, then read it
        // back through the `span_index()` accessor (the folded product, not the live index).
        let index = build_source_span_index(root).expect("build span index");
        let blob = serde_json::to_vec(&index).expect("encode");
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            Arc::new(RdfDataset::union(&[])),
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
            crate::stages::carrier::REP_SPAN_TABLE,
            "application/json",
            blob,
        );
        let product = StageProduct::from_bundle("stage-source-load", Arc::new(bundle));
        let folded = product
            .span_index()
            .expect("read span table back from the fold");

        assert!(
            folded.lookup("https://example.test/rootSubject").is_some(),
            "RootOntology subject must be tracked"
        );
        assert!(
            folded
                .lookup("https://example.test/sourceSubject")
                .is_some(),
            "Source subject must be tracked"
        );
        assert!(
            folded
                .lookup("https://example.test/importSubject")
                .is_none(),
            "Import subject must be SUPPRESSED by the fixed policy"
        );
    }

    #[test]
    fn missing_directory_listings_are_empty_not_errors() {
        // `sorted_dirs` / `ttl_files_in` treat an absent directory as an empty listing
        // (NotFound → Ok(empty)), so the discovery helpers on a root with no `slices`/
        // `imports` tree return empty rather than erroring.
        let empty = tempfile::tempdir().unwrap();
        let root = empty.path();
        assert!(sorted_dirs(&root.join("slices")).unwrap().is_empty());
        assert!(ttl_files_in(&root.join("imports")).unwrap().is_empty());
        assert!(module_files(root).unwrap().is_empty());
        assert!(all_manifest_files(root).unwrap().is_empty());
    }
}
