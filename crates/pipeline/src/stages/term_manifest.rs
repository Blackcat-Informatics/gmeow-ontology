// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `term-content-manifest` export leaf: a committed, machine-readable
//! content-address of every documented vocabulary term, generated (never
//! hand-authored) from the authored ontology.
//!
//! For each term (a `gmeow:`/`logic:`-namespaced class, property, or named
//! individual) the manifest records `gmeow:definitionDigest` — the RDFC-1.0
//! canonical blake3 digest of the term's concise bounded description with the
//! per-term provenance predicates excluded, so recording provenance never
//! perturbs the digest that generated it — plus `gmeow:addedInVersion` (the
//! release the term was first seen in) and a reified `gmeow:hasChangelogEntry`
//! record for every release whose digest differs from the prior committed
//! manifest. The result rides the bundle as the
//! `graph/fanout/catalog/term-content-manifest.nq` named graph and is fanned out
//! to the committed `generated/catalog/term-content-manifest.nq`, which the
//! superset gate reconstructs byte-for-byte from that graph.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use purrdf::gts::writer::digest_string;
use purrdf::slice::rdf_query::{Dataset, Object, Subject};
use purrdf::{RdfQuad, RdfTerm};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::module_files;

/// Committed logical path of the generated term content manifest.
pub const TERM_MANIFEST_RDF_PATH: &str = "generated/catalog/term-content-manifest.nq";

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
        acc.add_turtle(&bytes, &path.display().to_string())
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

/// The prior committed manifest's record for one term.
struct PriorTerm {
    digest: String,
    first_seen: String,
    changed_versions: Vec<String>,
}

/// Read the prior committed manifest at `<root>/generated/catalog/term-content-manifest.nq`.
/// `None` when the file is absent (bootstrap); `Some(map)` (term IRI → record) when
/// present. A present-but-unparsable manifest, or a term missing its digest /
/// first-seen version, is a hard fault (a regenerated tree always carries a
/// well-formed manifest).
fn read_prior_manifest(
    root: &Path,
) -> Result<Option<BTreeMap<String, PriorTerm>>, gmeow_errors::Diag> {
    // GENERATED-READ-OK: this reads the PRIOR committed manifest as deliberate prior-state
    // input to compute the monotonic changelog (first-seen versions + changed entries). It is
    // not a stale-disk-fold: the term-manifest stage is the sole producer of this file and needs
    // its previous value to preserve history — there is no upstream product carrying prior state.
    // On a clean tree the read equals the stage's own last output, so regenerate is a fixed point.
    let path = root.join(TERM_MANIFEST_RDF_PATH);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let parsed =
        Dataset::parse(&bytes, "application/n-quads", "prior term manifest").map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?;
    // Drop the named-graph label so the default-graph query surface applies.
    let flat: Vec<RdfQuad> = parsed
        .inner()
        .owned_quads()
        .map(|mut q| {
            q.graph_name = None;
            q
        })
        .collect();
    let dataset = Dataset::from_owned_quads(&flat).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })?;

    // Enumerate the manifest's terms: every named subject carrying a digest.
    let mut term_iris: BTreeSet<String> = BTreeSet::new();
    dataset.for_each_quad(|subject, predicate, _object, _graph| {
        if predicate == DEFINITION_DIGEST
            && let Subject::Named(iri) = subject
        {
            term_iris.insert(iri);
        }
    });

    let mut out: BTreeMap<String, PriorTerm> = BTreeMap::new();
    for term in term_iris {
        let digest = dataset
            .object_literal(&term, DEFINITION_DIGEST)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: e.to_string(),
                })
            })?
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!(
                        "prior manifest term {term} carries no gmeow:definitionDigest"
                    ),
                })
            })?;
        let first_seen = dataset
            .object_literal(&term, ADDED_IN_VERSION)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: e.to_string(),
                })
            })?
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("prior manifest term {term} carries no gmeow:addedInVersion"),
                })
            })?;
        let mut changed_versions: Vec<String> = Vec::new();
        for entry in dataset.objects(&term, HAS_CHANGELOG_ENTRY).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })? {
            let node = match entry {
                Object::Blank(label) => Subject::Blank(label),
                Object::Named(iri) => Subject::Named(iri),
                _ => continue,
            };
            for value in dataset
                .objects_of_subject(&node, ENTRY_VERSION)
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Parse {
                        message: e.to_string(),
                    })
                })?
            {
                if let Object::Literal { value, .. } = value {
                    changed_versions.push(value);
                }
            }
        }
        changed_versions.sort();
        changed_versions.dedup();
        out.insert(
            term,
            PriorTerm {
                digest,
                first_seen,
                changed_versions,
            },
        );
    }
    Ok(Some(out))
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

/// Build the N-Quads document for the term content manifest (every quad in
/// [`TERM_MANIFEST_GRAPH_IRI`]). Deterministic: terms iterate sorted, change
/// versions sorted, and the whole document is re-sorted + deduped before it is
/// parsed and canonicalized.
fn build_manifest_nquads(root: &Path, dataset: &Dataset) -> Result<String, gmeow_errors::Diag> {
    let release = release_version(dataset)?;
    let terms = documented_terms(dataset)?;
    let prior = read_prior_manifest(root)?;

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

    let mut out = String::new();
    let mut entry_counter: usize = 0;
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

        // Resolve first-seen version + the change versions from the prior manifest.
        let (first_seen, mut changed): (String, Vec<String>) = match &prior {
            // Bootstrap: no prior manifest. First-seen is the authored version or the
            // release; no term is "changed" this run.
            None => (
                authored_added.clone().unwrap_or_else(|| release.clone()),
                Vec::new(),
            ),
            Some(map) => match map.get(term) {
                // Carried term: keep its recorded first-seen; record a change at the
                // release if its digest diverged from the prior digest.
                Some(prior_term) => {
                    let mut changed = prior_term.changed_versions.clone();
                    if digest != prior_term.digest {
                        changed.push(release.clone());
                    }
                    (prior_term.first_seen.clone(), changed)
                }
                // A term new since the prior manifest: first-seen is the authored
                // version or the release; not a "change".
                None => (
                    authored_added.clone().unwrap_or_else(|| release.clone()),
                    Vec::new(),
                ),
            },
        };
        changed.sort();
        changed.dedup();

        quad_str(&mut out, term, DEFINITION_DIGEST, &digest);
        quad_str(&mut out, term, ADDED_IN_VERSION, &first_seen);
        for version in &changed {
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
    Ok(sorted)
}

/// Render the committed term-content-manifest bytes: build the N-Quads, parse them,
/// and re-serialize as RDFC-1.0 canonical N-Quads (the SAME fold the superset gate
/// reconstructs from the carrier graph, so `file == fold` holds by construction).
pub fn render_term_manifest(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let dataset = load_authored_no_imports(root)?;
    let nq = build_manifest_nquads(root, &dataset)?;
    let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("parse term-content-manifest N-Quads: {e}"),
        })
    })?;
    crate::stages::superset::canonical_ntriples(&ds).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("canonicalize term-content-manifest: {e}"),
        })
    })
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
        "term_manifest.v2-canonical-type-markers"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The digests are computed over the authored default graph (root ontology +
        // slice modules); declare them so a vocabulary edit that changes a term's
        // definition busts the cache. The committed manifest is the prior-state input
        // to change detection, so declare it too (a manifest edit re-runs the stage) —
        // but it is ALSO this stage's own output, hence absent on the one-shot
        // bootstrap build that first mints it; declare it only when present.
        let mut files = vec![root.join("ontology").join("gmeow.ttl")];
        files.extend(module_files(root)?);
        // GENERATED-READ-OK: cache declaration for the deliberate prior-state read in
        // read_prior_manifest (see its justification) — the prior manifest is a genuine input
        // to the monotonic changelog, not a stale projection folded as fresh.
        let prior = root.join(TERM_MANIFEST_RDF_PATH);
        if prior.is_file() {
            files.push(prior);
        }
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
"#,
            "synthetic term manifest graph",
        )
        .expect("parse synthetic term manifest graph")
    }

    fn digest_for(turtle: &[u8], term: &str) -> String {
        let dataset = Dataset::parse_turtle(turtle, "synthetic digest graph")
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
        let root = tempfile::tempdir().expect("temporary manifest root");
        let text = build_manifest_nquads(root.path(), &dataset).expect("build synthetic manifest");
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
}
