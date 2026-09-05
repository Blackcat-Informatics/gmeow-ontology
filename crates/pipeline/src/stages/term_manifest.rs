// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `term-content-manifest` export leaf: a materialized, machine-readable
//! content-address of every documented vocabulary term, generated (never
//! hand-authored) from the authored ontology.
//!
//! For each term (a `gmeow:`/`logic:`-namespaced class, property, or named
//! individual) the manifest records `gmeow:definitionDigest` — the RDFC-1.0
//! canonical blake3 digest of the term's concise bounded description with the
//! per-term provenance predicates excluded, so recording provenance never
//! perturbs the digest that generated it — plus `gmeow:addedInVersion` (the
//! release the term was first seen in) and a reified `gmeow:hasChangelogEntry`
//! record for every release whose digest differs from the explicit, tracked
//! previous-release authority. The result rides the bundle as the
//! `graph/fanout/catalog/term-content-manifest.nq` named graph and is fanned out
//! to the materialized `generated/catalog/term-content-manifest.nq`, which the
//! superset gate reconstructs byte-for-byte from that graph.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as IoWrite;
use std::path::Path;

use purrdf::gts::writer::digest_string;
use purrdf::slice::rdf_query::Dataset;
use purrdf::{RdfQuad, RdfTerm};
use serde::{Deserialize, Serialize};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::module_files;

/// Materialized logical path of the generated term content manifest.
pub const TERM_MANIFEST_RDF_PATH: &str = "generated/catalog/term-content-manifest.nq";

/// Tracked previous-release authority for computed per-term change history.
///
/// Unlike [`TERM_MANIFEST_RDF_PATH`], this is authored release evidence: ordinary
/// synchronization never rewrites it. Only the explicit maintainer refresh producer
/// advances the boundary after a release has been accepted.
pub const TERM_RELEASE_AUTHORITY_PATH: &str = "metadata/releases/term-content-authority.json";

const TERM_RELEASE_AUTHORITY_SCHEMA: &str = "gmeow.term-content-authority.v1";

/// The RDF-fanout named graph the manifest rides in (auto-derived from the committed
/// path by [`crate::stages::superset::rdf_fanout_graph_iri`]); it is ALSO the
/// 4th-column label of the committed `.nq`, so the fold reconstructs it exactly.
pub const TERM_MANIFEST_GRAPH_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/term-content-manifest.nq";

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
/// The ontology IRI (the `gmeow:` namespace without its trailing slash) — the
/// subject carrying `owl:versionInfo`.
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The per-term provenance predicates: they are recorded ABOUT a term's history
/// and are excluded from the term's own definition digest, so writing provenance
/// never perturbs the digest that produced it.
const DEFINITION_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/definitionDigest";
const VERSION_FINGERPRINT: &str = "https://blackcatinformatics.ca/gmeow/versionFingerprint";
const ADDED_IN_VERSION: &str = "https://blackcatinformatics.ca/gmeow/addedInVersion";
const HAS_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/hasChangelogEntry";
const ENTRY_VERSION: &str = "https://blackcatinformatics.ca/gmeow/entryVersion";
const ENTRY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/entryNote";
const TERM_STABILITY: &str = "https://blackcatinformatics.ca/gmeow/termStability";
const CHANGELOG_ENTRY_TYPE: &str = "https://blackcatinformatics.ca/gmeow/ChangelogEntry";

/// The prose stamped on a computed changelog entry (a digest divergence).
const CHANGE_NOTE: &str = "Definition changed";

/// The predicates excluded from a term's definition digest.
const PROVENANCE_PREDICATES: [&str; 7] = [
    DEFINITION_DIGEST,
    VERSION_FINGERPRINT,
    ADDED_IN_VERSION,
    HAS_CHANGELOG_ENTRY,
    ENTRY_VERSION,
    ENTRY_NOTE,
    TERM_STABILITY,
];

/// The `rdf:type` values that make a subject a documented term. This is a SUPERSET
/// of the docs model's `category_for_type` selection (which additionally documents
/// `rdfs:Datatype` terms such as `gmeow:markdown`), so every documented term is
/// guaranteed a manifest entry.
const TERM_TYPE_IRIS: [&str; 13] = [
    gmeow_ns::LOGIC_CLASS,
    gmeow_ns::OWL_CLASS,
    gmeow_ns::LOGIC_OBJECT_PROPERTY,
    gmeow_ns::OWL_OBJECT_PROPERTY,
    gmeow_ns::LOGIC_DATATYPE_PROPERTY,
    gmeow_ns::OWL_DATATYPE_PROPERTY,
    gmeow_ns::LOGIC_ANNOTATION_PROPERTY,
    gmeow_ns::OWL_ANNOTATION_PROPERTY,
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
    "http://www.w3.org/2000/01/rdf-schema#Class",
    gmeow_ns::LOGIC_NAMED_INDIVIDUAL,
    gmeow_ns::OWL_NAMED_INDIVIDUAL,
    "http://www.w3.org/2000/01/rdf-schema#Datatype",
];

/// Load the root ontology + every slice module into one frozen dataset (NO imports),
/// the source the digests are computed over. Mirrors
/// `constraint_catalog::load_authored_no_imports`.
fn load_authored_no_imports(root: &Path) -> Result<Dataset, gmeow_errors::Diag> {
    let mut acc = purrdf::slice::rdf_query::DatasetAccumulator::new();
    let mut files = vec![root.join("ontology").join("gmeow.ttl")];
    files.extend(module_files(root)?);
    for path in files {
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        acc.add_turtle(&bytes, None, &path.display().to_string())
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: e.to_string(),
                })
            })?;
    }
    acc.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })
}

/// The authored ontology `owl:versionInfo` — a hard requirement, never defaulted.
fn release_version(dataset: &Dataset) -> Result<String, gmeow_errors::Diag> {
    dataset
        .object_literal(ONTOLOGY_IRI, OWL_VERSION_INFO)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("authored ontology {ONTOLOGY_IRI} has no owl:versionInfo"),
            })
        })
}

/// Every documented-term IRI: a `gmeow:`/`logic:`-namespaced subject typed as one
/// of [`TERM_TYPE_IRIS`], sorted + deduped.
fn documented_terms(dataset: &Dataset) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let mut terms: BTreeSet<String> = BTreeSet::new();
    for type_iri in TERM_TYPE_IRIS {
        for subject in dataset.subjects_of_type(type_iri).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })? {
            if subject.starts_with(GMEOW) || subject.starts_with(LOGIC) {
                terms.insert(subject);
            }
        }
    }
    Ok(terms)
}

/// The interning key for a node in subject position (the two kinds a CBD walks).
fn subject_key(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Iri(iri) => Some(format!("I:{iri}")),
        RdfTerm::BlankNode(label) => Some(format!("B:{label}")),
        _ => None,
    }
}

/// The RDFC-1.0-canonical blake3 digest of a term's concise bounded description:
/// a BFS from the named term collecting every quad with the current node as
/// subject and recursing ONLY into blank-node objects, with every provenance
/// predicate excluded. `quads` is the authored default-graph quad set and `index`
/// maps a [`subject_key`] to the quad indices it is the subject of.
fn definition_digest(
    quads: &[RdfQuad],
    index: &BTreeMap<String, Vec<usize>>,
    term_iri: &str,
) -> Result<String, gmeow_errors::Diag> {
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = vec![format!("I:{term_iri}")];
    let mut cbd: Vec<RdfQuad> = Vec::new();
    while let Some(node) = frontier.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        let Some(indices) = index.get(&node) else {
            continue;
        };
        for &i in indices {
            let quad = &quads[i];
            if PROVENANCE_PREDICATES.contains(&quad.predicate.as_str()) {
                continue;
            }
            // Recurse only into blank-node objects (never into named objects), so
            // the CBD stops at the term's own defining structure.
            if let RdfTerm::BlankNode(label) = &quad.object {
                frontier.push(format!("B:{label}"));
            }
            // Strip the graph label: the digest is over the flat defining triples.
            cbd.push(RdfQuad::new(
                quad.subject.clone(),
                quad.predicate.clone(),
                quad.object.clone(),
            ));
        }
    }
    let sub = Dataset::from_owned_quads(&cbd).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })?;
    let canon = sub.canonical_nquads_flat().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })?;
    Ok(digest_string(canon.as_bytes()))
}

/// The previous release's durable record for one term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PriorTerm {
    digest: String,
    first_seen: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    changed_versions: Vec<String>,
}

/// Versioned envelope around the previous-release term records. Keeping the
/// ontology release in-band makes the semantic boundary explicit and reviewable;
/// the path alone is never interpreted as authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReleaseAuthority {
    schema: String,
    release: String,
    terms: BTreeMap<String, PriorTerm>,
}

fn manifest_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Parse {
        message: message.into(),
    })
}

fn validate_release_authority(authority: &ReleaseAuthority) -> Result<(), gmeow_errors::Diag> {
    if authority.schema != TERM_RELEASE_AUTHORITY_SCHEMA {
        return Err(manifest_error(format!(
            "{} declares schema {:?}, but this build reads only {:?}",
            TERM_RELEASE_AUTHORITY_PATH, authority.schema, TERM_RELEASE_AUTHORITY_SCHEMA
        )));
    }
    ordered_release(&authority.release, "release authority")?;
    if authority.terms.is_empty() {
        return Err(manifest_error(format!(
            "{} carries no term records; an empty previous-release authority is not a release",
            TERM_RELEASE_AUTHORITY_PATH
        )));
    }
    for (term, record) in &authority.terms {
        if !(term.starts_with(GMEOW) || term.starts_with(LOGIC)) {
            return Err(manifest_error(format!(
                "{} carries out-of-scope term IRI {term:?}",
                TERM_RELEASE_AUTHORITY_PATH
            )));
        }
        let digest_hex = record.digest.strip_prefix("blake3:").unwrap_or_default();
        if digest_hex.len() != 64
            || !digest_hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(manifest_error(format!(
                "{} term {term} carries non-canonical definition digest {:?}",
                TERM_RELEASE_AUTHORITY_PATH, record.digest
            )));
        }
        if record.first_seen.trim().is_empty() {
            return Err(manifest_error(format!(
                "{} term {term} carries no first-seen release",
                TERM_RELEASE_AUTHORITY_PATH
            )));
        }
        if record
            .changed_versions
            .iter()
            .any(|version| version.trim().is_empty())
            || !record
                .changed_versions
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        {
            return Err(manifest_error(format!(
                "{} term {term} carries empty, duplicate, or unsorted changed releases {:?}",
                TERM_RELEASE_AUTHORITY_PATH, record.changed_versions
            )));
        }
    }
    Ok(())
}

fn ordered_release(value: &str, source: &str) -> Result<semver::Version, gmeow_errors::Diag> {
    semver::Version::parse(value).map_err(|error| {
        manifest_error(format!(
            "{source} release identity {value:?} is not semantic-version authority: {error}"
        ))
    })
}

/// Bind a tracked authority to the ontology release it precedes. An authority from
/// the future is a hard fault; equality is the normal state immediately after a
/// release boundary, while a greater current release is the only state permitted to
/// advance the authority.
fn release_boundary_order(
    dataset: &Dataset,
    authority: &ReleaseAuthority,
) -> Result<Ordering, gmeow_errors::Diag> {
    let current_text = release_version(dataset)?;
    let current = ordered_release(&current_text, "authored ontology")?;
    let accepted = ordered_release(&authority.release, "release authority")?;
    let order = current.cmp(&accepted);
    if order == Ordering::Less {
        return Err(manifest_error(format!(
            "authored ontology release {current} precedes tracked release authority {}; refusing \
             to compare current terms with future evidence",
            authority.release
        )));
    }
    Ok(order)
}

/// Read the required, tracked previous-release authority. Missing, malformed, or
/// non-canonical evidence is a hard fault: falling back to the ignored generated
/// output would make documentation depend on one worktree's synchronization history.
fn read_release_authority(root: &Path) -> Result<ReleaseAuthority, gmeow_errors::Diag> {
    let path = root.join(TERM_RELEASE_AUTHORITY_PATH);
    let bytes = std::fs::read(&path).map_err(|error| {
        manifest_error(format!(
            "cannot read required previous-release authority {}: {error}; run the explicit \
             maintainer release-authority producer at an accepted release boundary",
            path.display()
        ))
    })?;
    let authority: ReleaseAuthority = serde_json::from_slice(&bytes).map_err(|error| {
        manifest_error(format!(
            "cannot parse previous-release authority {}: {error}",
            path.display()
        ))
    })?;
    validate_release_authority(&authority)?;
    Ok(authority)
}

/// Escape a string to a valid N-Triples quoted-literal body (without the surrounding
/// quotes). Mirrors `constraint_catalog::escape_literal`.
fn escape_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn quad_str(out: &mut String, s: &str, p: &str, lit: &str) {
    writeln!(
        out,
        "<{s}> <{p}> \"{}\" <{TERM_MANIFEST_GRAPH_IRI}> .",
        escape_literal(lit)
    )
    .expect("write to String");
}

fn quad_blank_object(out: &mut String, s: &str, p: &str, blank: &str) {
    writeln!(out, "<{s}> <{p}> _:{blank} <{TERM_MANIFEST_GRAPH_IRI}> .").expect("write to String");
}

fn blank_type(out: &mut String, blank: &str, type_iri: &str) {
    writeln!(
        out,
        "_:{blank} <{RDF_TYPE}> <{type_iri}> <{TERM_MANIFEST_GRAPH_IRI}> ."
    )
    .expect("write to String");
}

fn blank_str(out: &mut String, blank: &str, p: &str, lit: &str) {
    writeln!(
        out,
        "_:{blank} <{p}> \"{}\" <{TERM_MANIFEST_GRAPH_IRI}> .",
        escape_literal(lit)
    )
    .expect("write to String");
}

/// Resolve the current manifest records against one explicit release authority.
/// The authority is immutable for the duration of an ordinary synchronization;
/// the newly rendered manifest is never fed back into this function.
fn resolve_term_records(
    dataset: &Dataset,
    prior: &BTreeMap<String, PriorTerm>,
) -> Result<BTreeMap<String, PriorTerm>, gmeow_errors::Diag> {
    let release = release_version(dataset)?;
    let terms = documented_terms(dataset)?;

    // The authored default-graph quad set + a subject-keyed index, shared across
    // every term's CBD walk (built once).
    let quads: Vec<RdfQuad> = dataset
        .inner()
        .owned_quads()
        .filter(|q| q.graph_name.is_none())
        .collect();
    let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, quad) in quads.iter().enumerate() {
        if let Some(key) = subject_key(&quad.subject) {
            index.entry(key).or_default().push(i);
        }
    }

    let mut records = BTreeMap::new();
    for term in &terms {
        let digest = definition_digest(&quads, &index, term)?;

        // The term's authored `gmeow:addedInVersion`, if any (the seed for a term's
        // first-seen version when there is no prior record).
        let authored_added = dataset
            .object_literal(term, ADDED_IN_VERSION)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: e.to_string(),
                })
            })?;

        // Resolve first-seen version + the change versions from the explicit
        // previous-release authority. A term absent from that release is new, not
        // changed. Bootstrap is reachable only through the one-time maintainer
        // producer; ordinary stage execution requires a non-empty authority file.
        let (first_seen, mut changed): (String, Vec<String>) = match prior.get(term) {
            Some(prior_term) => {
                let mut changed = prior_term.changed_versions.clone();
                if digest != prior_term.digest {
                    changed.push(release.clone());
                }
                (prior_term.first_seen.clone(), changed)
            }
            None => (
                authored_added.clone().unwrap_or_else(|| release.clone()),
                Vec::new(),
            ),
        };
        changed.sort();
        changed.dedup();

        records.insert(
            term.clone(),
            PriorTerm {
                digest,
                first_seen,
                changed_versions: changed,
            },
        );
    }
    Ok(records)
}

/// Build the N-Quads document for already-resolved term records (every quad in
/// [`TERM_MANIFEST_GRAPH_IRI`]). Deterministic: terms iterate sorted, change
/// versions are authority-canonical, and the whole document is re-sorted + deduped
/// before it is parsed and canonicalized.
fn manifest_nquads(records: &BTreeMap<String, PriorTerm>) -> String {
    let mut out = String::new();
    let mut entry_counter: usize = 0;
    for (term, record) in records {
        quad_str(&mut out, term, DEFINITION_DIGEST, &record.digest);
        quad_str(&mut out, term, ADDED_IN_VERSION, &record.first_seen);
        for version in &record.changed_versions {
            let blank = format!("clentry{entry_counter}");
            entry_counter += 1;
            quad_blank_object(&mut out, term, HAS_CHANGELOG_ENTRY, &blank);
            blank_type(&mut out, &blank, CHANGELOG_ENTRY_TYPE);
            blank_str(&mut out, &blank, ENTRY_VERSION, version);
            blank_str(&mut out, &blank, ENTRY_NOTE, CHANGE_NOTE);
        }
    }

    // Byte-stable regardless of emission order (canonicalization re-sorts anyway,
    // but keeping the intermediate deterministic makes the parse input stable).
    let mut lines: Vec<&str> = out.lines().collect();
    lines.sort_unstable();
    lines.dedup();
    let mut sorted = lines.join("\n");
    sorted.push('\n');
    sorted
}

fn canonical_manifest_bytes(nq: &str) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| manifest_error(format!("parse term-content-manifest N-Quads: {e}")))?;
    crate::stages::superset::canonical_ntriples(&ds)
        .map_err(|e| manifest_error(format!("canonicalize term-content-manifest: {e}")))
}

fn render_with_authority(
    dataset: &Dataset,
    prior: &BTreeMap<String, PriorTerm>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let records = resolve_term_records(dataset, prior)?;
    canonical_manifest_bytes(&manifest_nquads(&records))
}

/// Render the materialized term-content-manifest bytes against the tracked
/// previous-release authority, then canonicalize them through the SAME fold the
/// superset gate uses (`file == fold` by construction).
pub fn render_term_manifest(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let dataset = load_authored_no_imports(root)?;
    let authority = read_release_authority(root)?;
    release_boundary_order(&dataset, &authority)?;
    render_with_authority(&dataset, &authority.terms)
}

/// Advance the tracked previous-release authority at an explicit maintainer release
/// boundary. The candidate manifest is computed against the old authority, then is
/// proved to be a fixed point when used as the new authority before any byte is
/// written. `bootstrap` is a one-time repository initialization and refuses to
/// overwrite an existing authority.
pub fn refresh_release_authority(
    root: &Path,
    bootstrap: bool,
) -> Result<(String, usize, bool), gmeow_errors::Diag> {
    let path = root.join(TERM_RELEASE_AUTHORITY_PATH);
    if bootstrap && path.exists() {
        return Err(manifest_error(format!(
            "refusing to bootstrap over existing release authority {}",
            path.display()
        )));
    }

    let dataset = load_authored_no_imports(root)?;
    let existing = if bootstrap {
        None
    } else {
        Some(read_release_authority(root)?)
    };
    let boundary_order = existing
        .as_ref()
        .map(|authority| release_boundary_order(&dataset, authority))
        .transpose()?;
    let prior = existing
        .as_ref()
        .map_or_else(BTreeMap::new, |authority| authority.terms.clone());
    let records = resolve_term_records(&dataset, &prior)?;
    let candidate = canonical_manifest_bytes(&manifest_nquads(&records))?;
    let fixed_point = render_with_authority(&dataset, &records)?;
    if fixed_point != candidate {
        return Err(manifest_error(
            "term release-authority refresh is not a fixed point; refusing to write evidence",
        ));
    }

    let release = release_version(&dataset)?;
    if boundary_order == Some(Ordering::Equal) {
        let authority = existing.expect("equal boundary has an existing authority");
        if records != authority.terms {
            return Err(manifest_error(format!(
                "refusing to overwrite release authority {} at unchanged ontology release \
                 {release}: term records moved without a new accepted release identity",
                path.display()
            )));
        }
        return Ok((release, records.len(), false));
    }
    let authority = ReleaseAuthority {
        schema: TERM_RELEASE_AUTHORITY_SCHEMA.to_string(),
        release: release.clone(),
        terms: records,
    };
    validate_release_authority(&authority)?;
    // Compact JSON is the canonical tracked encoding. The authority carries one
    // record per documented term, so pretty-print indentation alone can push the
    // evidence over repository file-size policy without adding information.
    let mut bytes = serde_json::to_vec(&authority)
        .map_err(|error| manifest_error(format!("serialize term release authority: {error}")))?;
    bytes.push(b'\n');
    let parent = path.parent().ok_or_else(|| {
        manifest_error(format!(
            "release authority path {} has no parent",
            path.display()
        ))
    })?;
    std::fs::create_dir_all(parent)?;
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    pending.write_all(&bytes)?;
    pending.as_file_mut().sync_all()?;
    pending.persist(&path).map_err(|error| {
        manifest_error(format!(
            "atomically publish release authority {}: {}",
            path.display(),
            error.error
        ))
    })?;
    Ok((release, authority.terms.len(), true))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `term-content-manifest` export-leaf stage.
pub struct TermManifestStage {
    consumes: Vec<String>,
}

impl TermManifestStage {
    /// Construct the stage. It consumes `stage-reason` so it runs after the reasoned
    /// closure is available (mirroring the constraint-catalog leaf's placement).
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-reason".to_string()],
        }
    }
}

impl Default for TermManifestStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for TermManifestStage {
    fn id(&self) -> &str {
        "stage-term-manifest"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "term_manifest.v3-explicit-release-authority"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The digests are computed over the authored default graph (root ontology +
        // slice modules); declare them so a vocabulary edit that changes a term's
        // definition busts the cache. The tracked release authority is the ONLY prior
        // state: the materialized generated manifest is this stage's output and must
        // never be listed as its own semantic input.
        let mut files = vec![root.join("ontology").join("gmeow.ttl")];
        files.extend(module_files(root)?);
        let authority = root.join(TERM_RELEASE_AUTHORITY_PATH);
        if !authority.is_file() {
            return Err(manifest_error(format!(
                "required previous-release authority {} is absent",
                authority.display()
            )));
        }
        files.push(authority);
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let bytes = render_term_manifest(input.root)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(TERM_MANIFEST_RDF_PATH.to_string(), bytes);
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("canonical repository root")
    }

    fn synthetic_manifest_dataset() -> Dataset {
        Dataset::parse_turtle(
            br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<https://blackcatinformatics.ca/gmeow> owl:versionInfo "9.9.9" .
gmeow:SyntheticClass a owl:Class ; rdfs:label "Synthetic class" .
logic:syntheticProperty a owl:ObjectProperty ; rdfs:label "Synthetic property" .
gmeow:CanonicalClass a logic:Class ; rdfs:label "Canonical class" .
logic:canonicalObjectProperty a logic:ObjectProperty ; rdfs:label "Canonical object property" .
gmeow:canonicalDatatypeProperty a logic:DatatypeProperty ; rdfs:label "Canonical datatype property" .
gmeow:canonicalAnnotationProperty a logic:AnnotationProperty ; rdfs:label "Canonical annotation property" .
gmeow:canonicalIndividual a logic:NamedIndividual ; rdfs:label "Canonical individual" .
"#, None,
            "synthetic term manifest graph")
        .expect("parse synthetic term manifest graph")
    }

    fn digest_for(turtle: &[u8], term: &str) -> String {
        let dataset = Dataset::parse_turtle(turtle, None, "synthetic digest graph")
            .expect("parse synthetic digest graph");
        let quads: Vec<RdfQuad> = dataset
            .inner()
            .owned_quads()
            .filter(|quad| quad.graph_name.is_none())
            .collect();
        let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (position, quad) in quads.iter().enumerate() {
            if let Some(key) = subject_key(&quad.subject) {
                index.entry(key).or_default().push(position);
            }
        }
        definition_digest(&quads, &index, term).expect("definition digest")
    }

    #[test]
    fn manifest_fanout_iri_is_auto_derived() {
        // The declared graph IRI must equal what the superset helper derives from the
        // committed path, so the fold reconstructs the committed 4th column.
        assert_eq!(
            crate::stages::superset::rdf_fanout_graph_iri(TERM_MANIFEST_RDF_PATH).as_deref(),
            Some(TERM_MANIFEST_GRAPH_IRI)
        );
    }

    #[test]
    fn every_gmeow_typed_term_gets_a_digest() {
        let dataset = synthetic_manifest_dataset();
        let terms = documented_terms(&dataset).expect("documented terms");
        assert_eq!(terms.len(), 7, "the synthetic graph declares seven terms");
        let records =
            resolve_term_records(&dataset, &BTreeMap::new()).expect("resolve synthetic manifest");
        let text = manifest_nquads(&records);
        for term in &terms {
            assert!(
                text.contains(&format!("<{term}> <{DEFINITION_DIGEST}>")),
                "missing definition digest for term {term}"
            );
        }
        // Every quad carries the fanout 4th column and a blake3 digest is present.
        assert!(text.contains(TERM_MANIFEST_GRAPH_IRI));
        assert!(text.contains("blake3:"));
    }

    #[test]
    fn digest_excludes_provenance_predicates() {
        // A term's digest is over its defining triples only; adding a provenance
        // predicate (e.g. addedInVersion) must not change it.
        let term = "https://blackcatinformatics.ca/gmeow/SyntheticTerm";
        let base = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:SyntheticTerm a owl:Class ; rdfs:label "Stable definition" .
"#;
        let with_provenance = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:SyntheticTerm a owl:Class ;
    rdfs:label "Stable definition" ;
    gmeow:addedInVersion "9.9.9" ;
    gmeow:definitionDigest "blake3:prior" .
"#;
        let without = digest_for(base, term);
        let with = digest_for(with_provenance, term);
        assert_eq!(
            without, with,
            "provenance must not perturb definition identity"
        );
        assert!(without.starts_with("blake3:"));
    }

    #[test]
    fn generated_manifest_is_not_a_stage_input() {
        let root = repo_root();
        let files = TermManifestStage::new()
            .input_files(&root)
            .expect("term-manifest inputs");
        assert!(
            files.contains(&root.join(TERM_RELEASE_AUTHORITY_PATH)),
            "the tracked previous-release authority must salt the stage"
        );
        assert!(
            !files.contains(&root.join(TERM_MANIFEST_RDF_PATH)),
            "the stage's ignored generated output must never feed its semantic history"
        );
    }

    fn one_term_dataset(release: &str, label: &str, authored_note: Option<&str>) -> Dataset {
        let authored = authored_note.map_or_else(String::new, |note| {
            format!(
                r#"
    gmeow:hasChangelogEntry [
        a gmeow:ChangelogEntry ;
        gmeow:entryVersion "1.0.0" ;
        gmeow:entryNote "{note}" ] ;"#
            )
        });
        let turtle = format!(
            r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<https://blackcatinformatics.ca/gmeow> owl:versionInfo "{release}" .
gmeow:BoundaryTerm a owl:Class ;
    rdfs:label "{label}" ;{authored}
    gmeow:addedInVersion "1.0.0" .
"#
        );
        Dataset::parse_turtle(turtle.as_bytes(), None, "release-boundary term graph")
            .expect("parse release-boundary term graph")
    }

    #[test]
    fn real_definition_change_persists_on_the_second_fixed_point() {
        let before = one_term_dataset("1.0.0", "Before release", None);
        let authority =
            resolve_term_records(&before, &BTreeMap::new()).expect("seed previous release");
        let after = one_term_dataset("1.1.0", "After release", None);

        let first = render_with_authority(&after, &authority).expect("first post-change render");
        let second = render_with_authority(&after, &authority).expect("warm fixed-point render");
        assert_eq!(
            first, second,
            "the materialized first output must not become the next run's semantic prior"
        );
        let text = String::from_utf8(first).expect("manifest is UTF-8 N-Quads");
        assert!(text.contains(HAS_CHANGELOG_ENTRY), "{text}");
        assert!(
            text.contains(&format!("<{ENTRY_VERSION}> \"1.1.0\"")),
            "{text}"
        );
        assert_eq!(
            text.matches(CHANGE_NOTE).count(),
            1,
            "one real changed term produces one persistent computed entry"
        );
    }

    fn write_release_source(root: &Path, release: &str, label: &str) {
        let ontology = root.join("ontology");
        std::fs::create_dir_all(&ontology).expect("ontology directory");
        let turtle = format!(
            r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<https://blackcatinformatics.ca/gmeow> owl:versionInfo "{release}" .
gmeow:BoundaryTerm a owl:Class ; rdfs:label "{label}" .
"#
        );
        std::fs::write(ontology.join("gmeow.ttl"), turtle).expect("write ontology source");
    }

    #[test]
    fn release_authority_is_ordered_and_same_release_rewrites_fail_closed() {
        let guard = tempfile::tempdir().expect("release authority repo");
        let root = guard.path();
        write_release_source(root, "1.0.0", "Accepted definition");
        let (release, terms, wrote) =
            refresh_release_authority(root, true).expect("bootstrap accepted release");
        assert_eq!((release.as_str(), terms, wrote), ("1.0.0", 1, true));
        let path = root.join(TERM_RELEASE_AUTHORITY_PATH);
        let accepted = std::fs::read(&path).expect("accepted authority bytes");
        assert_eq!(
            accepted.iter().filter(|byte| **byte == b'\n').count(),
            1,
            "tracked authority uses one compact JSON record plus its terminal newline"
        );

        let accepted_authority = read_release_authority(root).expect("read accepted authority");
        let older = one_term_dataset("0.9.0", "Older source", None);
        let error = release_boundary_order(&older, &accepted_authority)
            .expect_err("an ontology older than its authority must fail closed")
            .to_string();
        assert!(error.contains("future evidence"), "{error}");

        write_release_source(root, "1.0.0", "Unreleased rewrite");
        let error = refresh_release_authority(root, false)
            .expect_err("same-release content must not move accepted authority")
            .to_string();
        assert!(error.contains("unchanged ontology release"), "{error}");
        assert_eq!(
            std::fs::read(&path).expect("authority after refusal"),
            accepted,
            "a refused refresh preserves every accepted byte"
        );

        write_release_source(root, "1.1.0", "Accepted next definition");
        let (release, terms, wrote) =
            refresh_release_authority(root, false).expect("advance at newer release");
        assert_eq!((release.as_str(), terms, wrote), ("1.1.0", 1, true));
        let advanced = std::fs::read(&path).expect("advanced authority bytes");
        assert_ne!(
            advanced, accepted,
            "a new release advances authority identity"
        );
        let (_, _, wrote) =
            refresh_release_authority(root, false).expect("same-release fixed-point no-op");
        assert!(
            !wrote,
            "an identical same-release refresh must not rewrite bytes"
        );
        assert_eq!(
            std::fs::read(&path).expect("authority after no-op"),
            advanced,
            "same-release no-op preserves authority identity"
        );
    }

    #[test]
    fn fresh_and_warm_fixed_points_render_byte_identical_term_docs() {
        let guard = tempfile::tempdir().expect("synthetic clean worktree");
        let root = guard.path();
        let before = one_term_dataset("1.0.0", "Before release", None);
        let prior = resolve_term_records(&before, &BTreeMap::new())
            .expect("accepted synthetic previous release");
        let authority = ReleaseAuthority {
            schema: TERM_RELEASE_AUTHORITY_SCHEMA.to_string(),
            release: "1.0.0".to_string(),
            terms: prior.clone(),
        };
        validate_release_authority(&authority).expect("valid synthetic release authority");
        let authority_path = root.join(TERM_RELEASE_AUTHORITY_PATH);
        std::fs::create_dir_all(authority_path.parent().expect("authority parent"))
            .expect("authority directory");
        std::fs::write(
            &authority_path,
            serde_json::to_vec_pretty(&authority).expect("serialize synthetic authority"),
        )
        .expect("write synthetic authority");
        write_release_source(root, "1.1.0", "After release");

        // The synthetic repository has a real tracked authority and authored source,
        // while generated/ is deliberately absent until first materialization.
        let materialized = root.join(TERM_MANIFEST_RDF_PATH);
        assert!(
            !materialized.exists(),
            "fresh synthetic worktree starts with no generated manifest"
        );

        // gmeow-test-input: synthetic-only
        let fresh = render_term_manifest(root).expect("fresh manifest render");
        std::fs::create_dir_all(materialized.parent().expect("manifest parent"))
            .expect("first materialization directory");
        std::fs::write(&materialized, &fresh).expect("materialize first manifest output");
        // gmeow-test-input: synthetic-only
        let warm = render_term_manifest(root).expect("warm manifest render");
        assert_eq!(fresh, warm, "manifest bytes must be fixed-point stable");
        assert_eq!(
            std::fs::read(&materialized).expect("read first materialization"),
            fresh,
            "warm rendering must neither read back nor rewrite the first generated output"
        );

        let after = one_term_dataset("1.1.0", "After release", None);
        let fresh_records = resolve_term_records(&after, &prior).expect("fresh resolved records");
        let warm_records = resolve_term_records(&after, &prior).expect("warm resolved records");
        assert_eq!(
            canonical_manifest_bytes(&manifest_nquads(&fresh_records))
                .expect("fresh canonical manifest"),
            fresh
        );
        assert_eq!(
            canonical_manifest_bytes(&manifest_nquads(&warm_records))
                .expect("warm canonical manifest"),
            warm
        );

        fn docs_pages(records: &BTreeMap<String, PriorTerm>) -> (String, String) {
            let term_iri = format!("{GMEOW}BoundaryTerm");
            let record = records.get(&term_iri).expect("BoundaryTerm record");
            let mut changelog = vec![gmeow_docs::model::DocChangelogEntry {
                version: "1.1.0".to_string(),
                note: Some("Authored release note.".to_string()),
                source: gmeow_docs::model::DocChangelogSource::Authored,
            }];
            changelog.extend(record.changed_versions.iter().map(|version| {
                gmeow_docs::model::DocChangelogEntry {
                    version: version.clone(),
                    note: Some(CHANGE_NOTE.to_string()),
                    source: gmeow_docs::model::DocChangelogSource::Computed,
                }
            }));
            let term = gmeow_docs::model::DocTerm {
                iri: term_iri,
                curie: "gmeow:BoundaryTerm".to_string(),
                label: Some("After release".to_string()),
                category: gmeow_docs::model::DocTermCategory::Class,
                content_digest: record.digest.clone(),
                added_in_version: Some(record.first_seen.clone()),
                changelog,
                ..Default::default()
            };
            let slug = gmeow_docs::slug::term_slug(&term);
            let model = gmeow_docs::DocsModel {
                title: "Synthetic lifecycle".to_string(),
                version: "1.1.0".to_string(),
                terms: vec![term],
                ..Default::default()
            };
            (
                gmeow_docs::to_markdown(&model, &gmeow_docs::Page::Term(slug)),
                gmeow_docs::to_markdown(&model, &gmeow_docs::Page::Changelog),
            )
        }

        let (fresh_page, fresh_changelog) = docs_pages(&fresh_records);
        let (warm_page, warm_changelog) = docs_pages(&warm_records);
        assert_eq!(
            fresh_page.as_bytes(),
            warm_page.as_bytes(),
            "term documentation must be byte-identical before and after materialization"
        );
        assert_eq!(
            fresh_changelog.as_bytes(),
            warm_changelog.as_bytes(),
            "global changelog must be byte-identical before and after materialization"
        );
        for (surface, markdown) in [
            ("term page", &fresh_page),
            ("global changelog", &fresh_changelog),
        ] {
            let authored = markdown.find("(authored)").unwrap_or_else(|| {
                panic!("authored changelog identity must be visible on the {surface}")
            });
            let computed = markdown.find("(computed)").unwrap_or_else(|| {
                panic!("computed changelog identity must be visible on the {surface}")
            });
            assert!(
                authored < computed,
                "same-version authored and computed entries must remain distinct and deterministic \
                 on the {surface}"
            );
        }
    }
}
