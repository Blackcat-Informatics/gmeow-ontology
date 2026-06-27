// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Slice catalog: manifest-based discovery, typed artifact inventory,
//! and content-addressed IDs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxigraph::model::{GraphNameRef, Term};
use oxigraph::store::Store;
use sha2::{Digest, Sha256};

use gmeow_rdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral};

use crate::artifact::{ArtifactRecord, ArtifactRole};
use crate::error::SliceError;

// ── Namespace constants ───────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const DCTERMS_TITLE: &str = "http://purl.org/dc/terms/title";
const DCTERMS_CREATOR: &str = "http://purl.org/dc/terms/creator";
const DCTERMS_IDENTIFIER: &str = "http://purl.org/dc/terms/identifier";
const GMEOW_SLICE_TIER: &str = "https://blackcatinformatics.ca/gmeow/sliceTier";
const GMEOW_SLICE_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/sliceConsumer";
const GMEOW_SLICE_PROFILE: &str = "https://blackcatinformatics.ca/gmeow/sliceProfile";
const GMEOW_SLICE_DEPENDS_ON: &str = "https://blackcatinformatics.ca/gmeow/sliceDependsOn";
const GMEOW_SLICE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";

/// The tier of a slice in the GMEOW taxonomy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SliceTier {
    Core,
    Extension,
    Domain,
    Unknown(String),
}

impl SliceTier {
    fn from_iri(iri: &str) -> Self {
        let base = GMEOW;
        match iri.strip_prefix(base) {
            Some("tierCore") => SliceTier::Core,
            Some("tierExtension") => SliceTier::Extension,
            Some("tierDomain") => SliceTier::Domain,
            _ => SliceTier::Unknown(iri.to_string()),
        }
    }
}

/// A parsed view of the mandatory `manifest.ttl` fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestView {
    /// The IRI of the slice resource (`a gmeow:Slice`).
    pub slice_iri: String,
    /// `rdfs:label` (first, in any language).
    pub label: Option<String>,
    /// `dcterms:title` (first, in any language).
    pub title: Option<String>,
    /// `dcterms:creator` values.
    pub creators: Vec<String>,
    /// `dcterms:identifier` (e.g. DOI).
    pub identifier: Option<String>,
    /// `gmeow:sliceTier`.
    pub tier: Option<SliceTier>,
    /// `gmeow:sliceConsumer` values.
    pub consumers: Vec<String>,
    /// `gmeow:sliceProfile` values — the named profiles this slice declares
    /// membership in (e.g. `claims`, `memory`, `narrative`). Profile-document
    /// generation lives in the pipeline; this view exposes the raw declarations
    /// so docs consumers can compute per-term profile membership (#1026).
    pub profiles: Vec<String>,
    /// `gmeow:sliceDependsOn` values — the slice IRIs this slice depends on.
    /// A named profile's membership is the closure of its declared members over
    /// this relation (#330); docs reuse it for the same closure (#1026).
    pub depends_on: Vec<String>,
}

/// A fully-loaded slice record: manifest view, manifest IR dataset, and artifact
/// inventory.
pub struct SliceRecord {
    /// The parsed manifest fields.
    pub manifest: ManifestView,
    /// The manifest's full RDF graph as a frozen IR dataset — lossless round-trip.
    pub manifest_graph: Arc<RdfDataset>,
    /// All artifacts discovered under the slice directory.
    pub artifacts: Vec<ArtifactRecord>,
    /// The on-disk slice directory the record was loaded from. Retained so the
    /// authoritative `manifest.ttl` path is known without any scan/substring
    /// match (#820 G8: the discover walk already knows it).
    pub slice_dir: PathBuf,
}

impl SliceRecord {
    /// The on-disk path to this slice's `manifest.ttl`.
    pub fn manifest_path(&self) -> PathBuf {
        self.slice_dir.join("manifest.ttl")
    }

    /// Find an artifact by role and logical path.
    pub fn find_artifact(&self, role: &ArtifactRole, path: &str) -> Option<&ArtifactRecord> {
        self.artifacts
            .iter()
            .find(|a| &a.role == role && a.logical_path == path)
    }

    /// Find an artifact by its raw SHA-256 digest.
    pub fn find_by_digest(&self, digest: &str) -> Option<&ArtifactRecord> {
        self.artifacts.iter().find(|a| a.raw_digest == digest)
    }
}

/// The slice catalog: a collection of discovered and loaded slice records.
pub struct SliceCatalog {
    records: Vec<SliceRecord>,
}

impl SliceCatalog {
    /// Recursively discover all slice directories under `root` (directories
    /// containing a `manifest.ttl`) and load each one.
    pub fn discover(root: &Path) -> Result<Self, SliceError> {
        let dirs = find_slice_dirs(root)?;
        let mut records = Vec::new();
        for dir in dirs {
            records.push(Self::from_slice_dir(&dir)?);
        }
        Ok(Self { records })
    }

    /// Load a single slice from `dir` (which must contain `manifest.ttl`).
    pub fn from_slice_dir(dir: &Path) -> Result<SliceRecord, SliceError> {
        let manifest_path = dir.join("manifest.ttl");
        let manifest_bytes = std::fs::read(&manifest_path).map_err(SliceError::Io)?;

        // Parse Turtle into an oxigraph store (lenient: accepts @x-gmeow-* lang tags).
        let store = parse_turtle_to_store(&manifest_bytes, &manifest_path)?;

        // Extract manifest view from the store.
        let manifest = extract_manifest_view(&store)?;

        // Build a frozen RdfDataset from the store for lossless round-trip.
        let manifest_graph = build_ir_dataset(&store)?;

        // Discover all artifacts.
        let artifacts = discover_artifacts(dir)?;

        Ok(SliceRecord {
            manifest,
            manifest_graph,
            artifacts,
            slice_dir: dir.to_path_buf(),
        })
    }

    /// Returns all slice records.
    pub fn records(&self) -> &[SliceRecord] {
        &self.records
    }

    /// Look up a slice record by its IRI.
    pub fn get(&self, iri: &str) -> Option<&SliceRecord> {
        self.records.iter().find(|r| r.manifest.slice_iri == iri)
    }
}

// ── Turtle parsing ────────────────────────────────────────────────────────────

fn parse_turtle_to_store(bytes: &[u8], path: &Path) -> Result<Store, SliceError> {
    crate::rdf_text::turtle_bytes_to_store(bytes, &path.display().to_string())
}

// ── Manifest extraction ───────────────────────────────────────────────────────

fn extract_manifest_view(store: &Store) -> Result<ManifestView, SliceError> {
    // Find the slice IRI: subject of `a gmeow:Slice`.
    let slice_iri = find_slice_iri(store)?;

    let mut label: Option<String> = None;
    let mut title: Option<String> = None;
    let mut creators: Vec<String> = Vec::new();
    let mut identifier: Option<String> = None;
    let mut tier: Option<SliceTier> = None;
    let mut consumers: Vec<String> = Vec::new();
    let mut profiles: Vec<String> = Vec::new();
    let mut depends_on: Vec<String> = Vec::new();

    let subject = oxigraph::model::NamedNode::new(&slice_iri)
        .map_err(|e| SliceError::InvalidManifest(format!("invalid slice IRI: {e}")))?;

    for quad in store.quads_for_pattern(
        Some(subject.as_ref().into()),
        None,
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        let pred = quad.predicate.as_str();
        match pred {
            p if p == RDFS_LABEL => {
                if label.is_none() {
                    label = Some(literal_value(&quad.object));
                }
            }
            p if p == DCTERMS_TITLE => {
                if title.is_none() {
                    title = Some(literal_value(&quad.object));
                }
            }
            p if p == DCTERMS_CREATOR => {
                creators.push(literal_value(&quad.object));
            }
            p if p == DCTERMS_IDENTIFIER => {
                if identifier.is_none() {
                    identifier = Some(literal_value(&quad.object));
                }
            }
            p if p == GMEOW_SLICE_TIER => {
                if tier.is_none() {
                    if let Term::NamedNode(nn) = &quad.object {
                        tier = Some(SliceTier::from_iri(nn.as_str()));
                    }
                }
            }
            p if p == GMEOW_SLICE_CONSUMER => {
                consumers.push(literal_value(&quad.object));
            }
            p if p == GMEOW_SLICE_PROFILE => {
                profiles.push(literal_value(&quad.object));
            }
            p if p == GMEOW_SLICE_DEPENDS_ON => {
                if let Term::NamedNode(nn) = &quad.object {
                    depends_on.push(nn.as_str().to_string());
                }
            }
            _ => {}
        }
    }

    // Deterministic order — `quads_for_pattern` iteration order is not stable.
    profiles.sort_unstable();
    profiles.dedup();
    depends_on.sort_unstable();
    depends_on.dedup();

    Ok(ManifestView {
        slice_iri,
        label,
        title,
        creators,
        identifier,
        tier,
        consumers,
        profiles,
        depends_on,
    })
}

fn find_slice_iri(store: &Store) -> Result<String, SliceError> {
    let rdf_type = oxigraph::model::NamedNode::new(RDF_TYPE)
        .map_err(|e| SliceError::InvalidManifest(format!("invalid rdf:type IRI: {e}")))?;
    let gmeow_slice = oxigraph::model::NamedNode::new(GMEOW_SLICE_CLASS)
        .map_err(|e| SliceError::InvalidManifest(format!("invalid gmeow:Slice IRI: {e}")))?;

    let mut subjects: Vec<String> = Vec::new();

    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(gmeow_slice.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let oxigraph::model::NamedOrBlankNode::NamedNode(nn) = &quad.subject {
            subjects.push(nn.as_str().to_string());
        }
    }

    match subjects.len() {
        0 => Err(SliceError::InvalidManifest(
            "no `a gmeow:Slice` triple found in manifest.ttl".to_string(),
        )),
        1 => Ok(subjects.remove(0)),
        _ => {
            subjects.sort();
            Err(SliceError::InvalidManifest(format!(
                "manifest.ttl declares {} `a gmeow:Slice` subjects (must be exactly one): {}",
                subjects.len(),
                subjects.join(", ")
            )))
        }
    }
}

fn literal_value(term: &Term) -> String {
    match term {
        Term::Literal(lit) => lit.value().to_string(),
        Term::NamedNode(nn) => nn.as_str().to_string(),
        Term::BlankNode(bn) => format!("_:{}", bn.as_str()),
        Term::Triple(_) => "<triple>".to_string(),
    }
}

// ── IR dataset builder ────────────────────────────────────────────────────────

fn build_ir_dataset(store: &Store) -> Result<Arc<RdfDataset>, SliceError> {
    use gmeow_rdf::BlankScope;

    let mut builder = RdfDatasetBuilder::new();

    for quad_result in store.quads_for_pattern(None, None, None, None) {
        let quad = quad_result.map_err(|e| SliceError::Parse(format!("store iter error: {e}")))?;

        let s = match &quad.subject {
            oxigraph::model::NamedOrBlankNode::NamedNode(nn) => {
                builder.intern_iri(nn.as_str().to_string())
            }
            oxigraph::model::NamedOrBlankNode::BlankNode(bn) => {
                builder.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT)
            }
        };

        let p = builder.intern_iri(quad.predicate.as_str().to_string());

        let o = ox_term_to_id(&mut builder, &quad.object)?;

        let g = match &quad.graph_name {
            oxigraph::model::GraphName::NamedNode(nn) => {
                Some(builder.intern_iri(nn.as_str().to_string()))
            }
            oxigraph::model::GraphName::BlankNode(bn) => {
                Some(builder.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT))
            }
            oxigraph::model::GraphName::DefaultGraph => None,
        };

        builder.push_quad(s, p, o, g);
    }

    builder
        .freeze()
        .map_err(|e| SliceError::Parse(format!("IR dataset freeze failed: {e}")))
}

fn ox_term_to_id(
    builder: &mut RdfDatasetBuilder,
    term: &oxigraph::model::Term,
) -> Result<gmeow_rdf::TermId, SliceError> {
    use gmeow_rdf::BlankScope;
    use oxigraph::model::{BaseDirection, Term};

    match term {
        Term::NamedNode(nn) => Ok(builder.intern_iri(nn.as_str().to_string())),
        Term::BlankNode(bn) => {
            Ok(builder.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT))
        }
        Term::Literal(lit) => {
            let direction = lit.direction().map(|d| match d {
                BaseDirection::Ltr => gmeow_rdf::RdfTextDirection::Ltr,
                BaseDirection::Rtl => gmeow_rdf::RdfTextDirection::Rtl,
            });
            Ok(builder.intern_literal(RdfLiteral {
                lexical_form: lit.value().to_string(),
                datatype: Some(lit.datatype().as_str().to_string()),
                language: lit.language().map(str::to_string),
                direction,
            }))
        }
        Term::Triple(triple) => {
            let inner_s = match &triple.subject {
                oxigraph::model::NamedOrBlankNode::NamedNode(nn) => {
                    builder.intern_iri(nn.as_str().to_string())
                }
                oxigraph::model::NamedOrBlankNode::BlankNode(bn) => {
                    builder.intern_blank(bn.as_str().to_string(), BlankScope::DEFAULT)
                }
            };
            let inner_p = builder.intern_iri(triple.predicate.as_str().to_string());
            let inner_o = ox_term_to_id(builder, &triple.object)?;
            Ok(builder.intern_triple(inner_s, inner_p, inner_o))
        }
    }
}

// ── Artifact discovery ────────────────────────────────────────────────────────

fn discover_artifacts(dir: &Path) -> Result<Vec<ArtifactRecord>, SliceError> {
    let mut artifacts = Vec::new();
    collect_artifacts(dir, dir, &mut artifacts)?;
    artifacts.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
    Ok(artifacts)
}

fn collect_artifacts(
    root: &Path,
    current: &Path,
    out: &mut Vec<ArtifactRecord>,
) -> Result<(), SliceError> {
    for entry in std::fs::read_dir(current).map_err(SliceError::Io)? {
        let entry = entry.map_err(SliceError::Io)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(SliceError::Io)?;

        if file_type.is_dir() {
            collect_artifacts(root, &path, out)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        // Compute relative logical path.
        let rel = path.strip_prefix(root).map_err(|_| {
            SliceError::InvalidPath(format!("path not under root: {}", path.display()))
        })?;

        // Validate: no absolute components, no `..`.
        for component in rel.components() {
            use std::path::Component;
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                other => {
                    return Err(SliceError::InvalidPath(format!(
                        "unsafe path component {other:?} in {}",
                        rel.display()
                    )));
                }
            }
        }

        let logical_path = rel.to_string_lossy().to_string();
        let content = std::fs::read(&path).map_err(SliceError::Io)?;
        let raw_digest = hex_sha256(&content);

        let role = classify_role(&logical_path);
        let media_type = infer_media_type(&logical_path);

        // For RDF files, compute semantic digest via sorted N-Triples.
        let semantic_digest = if is_rdf_file(&logical_path) {
            compute_semantic_digest(&content, &path).ok()
        } else {
            None
        };

        out.push(ArtifactRecord {
            role,
            logical_path,
            media_type,
            raw_digest,
            semantic_digest,
            content,
        });
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn parse_rdf_to_store(bytes: &[u8], path: &Path) -> Result<Store, SliceError> {
    let media_type = crate::rdf_text::media_type_for_path(path);
    crate::rdf_text::rdf_bytes_to_store(bytes, media_type, &path.display().to_string())
}

fn compute_semantic_digest(bytes: &[u8], path: &Path) -> Result<String, SliceError> {
    let store = parse_rdf_to_store(bytes, path)?;

    // Canonicalize blank-node labels BEFORE digesting. Parsing assigns blank-node IDs
    // non-deterministically, so a plain sorted-N-Triples digest would differ
    // run-to-run for any artifact that uses blank nodes (e.g. SHACL property shapes,
    // OWL restrictions). Canonicalization makes the *semantic* digest a stable
    // function of the graph's identity (RFC #820 §12 — semantic, path-independent,
    // deterministic keys).
    //
    // Native full RDFC-1.0 (#910), replacing oxrdf's non-spec `Unstable` algorithm:
    // RDFC-1.0 is the W3C-standard canonical form, so the digest is now stable across
    // versions/builds (the `Unstable` algorithm was only canonical *within* one
    // build). `canonical_nquads` already sorts + deduplicates the canonical N-Quads.
    let quads: Vec<oxigraph::model::Quad> = store
        .quads_for_pattern(None, None, None, None)
        .collect::<Result<_, _>>()
        .map_err(|e| SliceError::Parse(format!("store iter error: {e}")))?;
    let canonical = gmeow_rdf::canonical_nquads(quads.iter())
        .map_err(|e| SliceError::Parse(format!("canonicalization error: {e}")))?;
    Ok(hex_sha256(canonical.as_bytes()))
}

fn is_rdf_file(path: &str) -> bool {
    path.ends_with(".ttl") || path.ends_with(".nt") || path.ends_with(".nq")
}

fn classify_role(path: &str) -> ArtifactRole {
    let name = path.split('/').next_back().unwrap_or(path);
    // Top-level well-known files.
    match name {
        "manifest.ttl" => return ArtifactRole::Manifest,
        "module.ttl" => return ArtifactRole::Module,
        "shapes.ttl" => return ArtifactRole::Shapes,
        "docs.md" => return ArtifactRole::Documentation,
        "CITATION.cff" => return ArtifactRole::Citation,
        _ => {}
    }
    // Directory-based classification.
    if path.starts_with("mappings/") {
        return ArtifactRole::Mapping;
    }
    if path.starts_with("queries/competency/") {
        return ArtifactRole::CompetencyQuery;
    }
    if path.starts_with("queries/verify/") {
        return ArtifactRole::VerifyQuery;
    }
    if path.starts_with("tests/counter-examples/") {
        return ArtifactRole::CounterExample;
    }
    if path.starts_with("tests/") {
        return ArtifactRole::TestDsl;
    }
    if path.starts_with("examples/") {
        return ArtifactRole::Example;
    }
    if path.starts_with("i18n/") {
        return ArtifactRole::TranslationCatalog;
    }
    ArtifactRole::Other(path.to_string())
}

fn infer_media_type(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "ttl" => "text/turtle",
        "nt" => "application/n-triples",
        "nq" => "application/n-quads",
        "sparql" | "rq" => "application/sparql-query",
        "md" => "text/markdown",
        "yaml" | "yml" => "application/yaml",
        "json" => "application/json",
        "cff" => "application/yaml",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ── Recursive slice-dir discovery ─────────────────────────────────────────────

fn find_slice_dirs(root: &Path) -> Result<Vec<PathBuf>, SliceError> {
    let mut dirs = Vec::new();
    find_slice_dirs_inner(root, &mut dirs)?;
    dirs.sort();
    Ok(dirs)
}

fn find_slice_dirs_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SliceError> {
    let manifest = dir.join("manifest.ttl");
    if manifest.exists() {
        out.push(dir.to_path_buf());
        // Don't recurse into a slice dir — slices are not nested.
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(SliceError::Io)? {
        let entry = entry.map_err(SliceError::Io)?;
        if entry.file_type().map_err(SliceError::Io)?.is_dir() {
            find_slice_dirs_inner(&entry.path(), out)?;
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Fix 1: a manifest declaring two `a gmeow:Slice` subjects must hard-fail
    /// with `SliceError::InvalidManifest` naming both subjects.
    #[test]
    fn find_slice_iri_rejects_multiple_slice_subjects() {
        let ttl = r#"
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

<https://example.org/slice/alpha> a gmeow:Slice .
<https://example.org/slice/beta>  a gmeow:Slice .
"#;
        let path = Path::new("manifest.ttl");
        let store =
            parse_turtle_to_store(ttl.as_bytes(), path).expect("should parse without error");
        let result = find_slice_iri(&store);
        match result {
            Err(SliceError::InvalidManifest(msg)) => {
                assert!(
                    msg.contains("https://example.org/slice/alpha"),
                    "error message must name first subject, got: {msg}"
                );
                assert!(
                    msg.contains("https://example.org/slice/beta"),
                    "error message must name second subject, got: {msg}"
                );
                assert!(
                    msg.contains('2') || msg.contains("two"),
                    "error message must state the count, got: {msg}"
                );
            }
            other => panic!("expected InvalidManifest error, got: {other:?}"),
        }
    }

    /// Fix 2: an N-Triples artifact produces a correct, non-empty semantic
    /// digest that equals the digest of the same triples written as Turtle.
    #[test]
    fn compute_semantic_digest_handles_nt_artifacts() {
        // The same single triple expressed in two formats.
        let turtle_bytes = br#"
@prefix ex: <https://example.org/> .
ex:subject ex:predicate ex:object .
"#;
        let nt_bytes = b"<https://example.org/subject> <https://example.org/predicate> <https://example.org/object> .\n";

        let ttl_path = Path::new("data.ttl");
        let nt_path = Path::new("data.nt");

        let digest_ttl = compute_semantic_digest(turtle_bytes, ttl_path)
            .expect("Turtle semantic digest must not fail");
        let digest_nt = compute_semantic_digest(nt_bytes, nt_path)
            .expect("N-Triples semantic digest must not fail");

        assert!(
            !digest_ttl.is_empty(),
            "Turtle semantic digest must not be empty"
        );
        assert!(
            !digest_nt.is_empty(),
            "N-Triples semantic digest must not be empty"
        );
        assert_eq!(
            digest_ttl, digest_nt,
            "Turtle and N-Triples digests for identical triples must match"
        );
    }
}
