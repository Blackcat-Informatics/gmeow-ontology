// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native GMEOW self-description loader (port of `gmeow_tools.self_desc`).
//!
//! Parses `metadata/gmeow-self.ttl` into a [`SelfDescription`] and marshals it,
//! plus the deposit-runtime configuration, into the [`crate::crossref`] deposit
//! inputs the native CrossRef XML generator consumes. The XML / lint rendering
//! itself stays in [`crate::crossref`]; this module supplies only the graph →
//! struct extraction and the deposit-input assembly that used to live in Python.
//!
//! # Deposit configuration
//!
//! The registrant-presentation constants and the curated alignment-target table
//! were carried in the Python `config` module. Under the RUST-FIRST posture the
//! canonical copy lives here ([`deposit_config`]); the deposit-input assembly
//! reads it directly so the native `crossref` command needs no Python surface.

use std::path::{Path, PathBuf};

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};
use regex::Regex;
use std::sync::OnceLock;

use crate::crossref::{
    AlignmentTargetInput, ConfigInput, ContributorInput, DepositInput, LintInput,
    SelfDescriptionInput,
};

// ─────────────────────────────────────────────────────────────────────────────
// Namespaces (self-description vocabulary)
// ─────────────────────────────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const FOAF: &str = "http://xmlns.com/foaf/0.1/";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";

/// The Work IRI (the abstract intellectual creation, FRBR spine root).
const WORK_IRI: &str = "https://blackcatinformatics.ca/gmeow";

/// ORCID is the recognised person-authority scheme for CrossRef contributors.
const ORCID_PREFIX: &str = "https://orcid.org/";
/// Wikidata entity IRIs can identify organizations in CrossRef institution metadata.
const WIKIDATA_PREFIX: &str = "http://www.wikidata.org/entity/";

fn foaf(local: &str) -> String {
    format!("{FOAF}{local}")
}
fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}
fn dcterms(local: &str) -> String {
    format!("{DCTERMS}{local}")
}

static DOI_RE: OnceLock<Regex> = OnceLock::new();
static EMAIL_RE: OnceLock<Regex> = OnceLock::new();

/// Minimal DOI shape — `10.{registrant}/{suffix}`, anchored at the start
/// (mirrors Python's `re.match`).
fn doi_re() -> &'static Regex {
    DOI_RE.get_or_init(|| Regex::new(r"^10\.[^/\s]+/\S+").expect("valid DOI regex"))
}

/// Minimal e-mail shape sanity check.
fn email_re() -> &'static Regex {
    EMAIL_RE.get_or_init(|| Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").expect("valid email regex"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Data model
// ─────────────────────────────────────────────────────────────────────────────

/// A credited author of the work, projected to a CrossRef contributor.
///
/// `kind` is `"organization"` or `"person"`; `orcid` is the ORCID URL for
/// persons (`None` otherwise). `sequence` is the CrossRef order (`"first"` for
/// the lead, `"additional"` after).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contributor {
    pub kind: String,
    pub name: String,
    pub orcid: Option<String>,
    pub sequence: String,
    pub role: String,
}

impl Contributor {
    /// The given name(s) of a person — everything but the final token.
    pub fn given_name(&self) -> String {
        match self.name.trim().rsplit_once(' ') {
            Some((head, _)) => head.to_string(),
            None => String::new(),
        }
    }

    /// The surname of a person — the final whitespace-delimited token.
    pub fn surname(&self) -> String {
        let trimmed = self.name.trim();
        match trimmed.rsplit_once(' ') {
            Some((_, tail)) => tail.to_string(),
            None => trimmed.to_string(),
        }
    }
}

/// GMEOW self-description metadata extracted from `gmeow-self.ttl`.
///
/// Two DOIs are modelled, mirroring the FRBR spine: the concept DOI denotes the
/// Work (always-latest citation anchor), the optional version DOI denotes the
/// Manifestation (this immutable release). Concept-only (`version_doi is None`)
/// is a first-class supported state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfDescription {
    pub title: String,
    pub version: String,
    pub release_date: String,
    pub concept_doi: String,
    pub version_doi: Option<String>,
    pub version_iri: String,
    pub depositor_name: String,
    pub depositor_email: String,
    pub registrant: String,
    pub registrant_wikidata: Option<String>,
    pub license_uri: String,
    pub homepage: String,
    pub description: String,
    pub repo_url: String,
    pub contributors: Vec<Contributor>,
}

impl SelfDescription {
    /// The preferred citable DOI: the version DOI if minted, else the concept DOI.
    pub fn doi(&self) -> &str {
        self.version_doi.as_deref().unwrap_or(&self.concept_doi)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Graph helpers
// ─────────────────────────────────────────────────────────────────────────────

fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The subjects that carry `predicate`, in dataset order.
fn subjects_with(ds: &RdfDataset, predicate: &str) -> Vec<TermId> {
    let Some(pred_id) = iri_id(ds, predicate) else {
        return Vec::new();
    };
    ds.quads_for_pattern(None, Some(pred_id), None, GraphMatch::Any)
        .map(|q| q.s)
        .collect()
}

/// The first literal object of `subject predicate`, if any.
fn opt_lit(ds: &RdfDataset, subject: TermId, predicate: &str) -> Option<String> {
    let pred_id = iri_id(ds, predicate)?;
    ds.quads_for_pattern(Some(subject), Some(pred_id), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_string()),
            _ => None,
        })
}

/// The required first literal object of `subject predicate` (else an error).
fn lit(ds: &RdfDataset, subject: TermId, predicate: &str, label: &str) -> Result<String, String> {
    opt_lit(ds, subject, predicate)
        .ok_or_else(|| format!("No literal found for {label} <{predicate}>"))
}

/// The first IRI object of `subject predicate`, if any.
fn opt_iri(ds: &RdfDataset, subject: TermId, predicate: &str) -> Option<String> {
    let pred_id = iri_id(ds, predicate)?;
    ds.quads_for_pattern(Some(subject), Some(pred_id), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(n) => Some(n.to_string()),
            _ => None,
        })
}

/// Every IRI object of `subject predicate`, in dataset order.
fn iri_objects(ds: &RdfDataset, subject: TermId, predicate: &str) -> Vec<String> {
    let Some(pred_id) = iri_id(ds, predicate) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subject), Some(pred_id), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(n) => Some(n.to_string()),
            _ => None,
        })
        .collect()
}

/// The rdf:type IRIs asserted for `subject`.
fn types_of(ds: &RdfDataset, subject: TermId) -> Vec<String> {
    iri_objects(ds, subject, RDF_TYPE)
}

/// The display name of a contributor agent (foaf:name / gmeow:name / rdfs:label).
fn agent_name(ds: &RdfDataset, agent: TermId) -> String {
    for prop in [foaf("name"), gmeow("name"), RDFS_LABEL.to_string()] {
        if let Some(name) = opt_lit(ds, agent, &prop) {
            return name;
        }
    }
    String::new()
}

/// Author contributions to the work, ordered organizations-first then persons,
/// each alphabetised by name; the overall lead carries `sequence = "first"`.
fn load_contributors(ds: &RdfDataset, work: TermId) -> Vec<Contributor> {
    let mut orgs: Vec<Contributor> = Vec::new();
    let mut persons: Vec<Contributor> = Vec::new();

    let Some(type_id) = iri_id(ds, RDF_TYPE) else {
        return Vec::new();
    };
    let Some(contribution_id) = iri_id(ds, &gmeow("Contribution")) else {
        return Vec::new();
    };
    let target_pred = iri_id(ds, &gmeow("contributionTarget"));
    let role_pred = iri_id(ds, &gmeow("contributionRole"));
    let role_author = iri_id(ds, &gmeow("roleAuthor"));
    let contributor_pred = iri_id(ds, &gmeow("contributor"));

    for q in ds.quads_for_pattern(None, Some(type_id), Some(contribution_id), GraphMatch::Any) {
        let contrib = q.s;

        // contributionTarget must be the work.
        let targets_work = target_pred
            .map(|p| {
                ds.quads_for_pattern(Some(contrib), Some(p), Some(work), GraphMatch::Any)
                    .next()
                    .is_some()
            })
            .unwrap_or(false);
        if !targets_work {
            continue;
        }
        // contributionRole must be roleAuthor.
        let is_author = match (role_pred, role_author) {
            (Some(p), Some(o)) => ds
                .quads_for_pattern(Some(contrib), Some(p), Some(o), GraphMatch::Any)
                .next()
                .is_some(),
            _ => false,
        };
        if !is_author {
            continue;
        }
        let Some(agent) = contributor_pred.and_then(|p| {
            ds.quads_for_pattern(Some(contrib), Some(p), None, GraphMatch::Any)
                .find_map(|qq| match ds.resolve(qq.o) {
                    TermRef::Iri(_) => Some(qq.o),
                    _ => None,
                })
        }) else {
            continue;
        };
        let name = agent_name(ds, agent);
        if name.is_empty() {
            continue;
        }
        let types = types_of(ds, agent);
        if types.contains(&foaf("Organization")) || types.contains(&gmeow("Organization")) {
            orgs.push(Contributor {
                kind: "organization".to_string(),
                name,
                orcid: None,
                sequence: "first".to_string(),
                role: "author".to_string(),
            });
        } else if types.contains(&gmeow("Person")) {
            let orcid = iri_objects(ds, agent, &gmeow("authorityLink"))
                .into_iter()
                .find(|o| o.starts_with(ORCID_PREFIX));
            persons.push(Contributor {
                kind: "person".to_string(),
                name,
                orcid,
                sequence: "additional".to_string(),
                role: "author".to_string(),
            });
        }
    }

    orgs.sort_by(|a, b| a.name.cmp(&b.name));
    persons.sort_by(|a, b| a.name.cmp(&b.name));
    let mut ordered: Vec<Contributor> = orgs;
    ordered.extend(persons);
    for (i, c) in ordered.iter_mut().enumerate() {
        c.sequence = if i == 0 { "first" } else { "additional" }.to_string();
    }
    ordered
}

/// Whether `date` is a strict `YYYY-MM-DD` calendar date (like `%Y-%m-%d`).
fn is_iso_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    if !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    let (y, m, d): (i64, u32, u32) = match (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
        (Ok(y), Ok(m), Ok(d)) => (y, m, d),
        _ => return false,
    };
    if !(1..=12).contains(&m) || d < 1 {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ][(m - 1) as usize];
    d <= dim
}

// ─────────────────────────────────────────────────────────────────────────────
// Public loading API (mirrors self_desc.load_self_description*)
// ─────────────────────────────────────────────────────────────────────────────

/// The default self-description file for a repository root
/// (`<root>/metadata/gmeow-self.ttl`).
pub fn default_self_desc_path(root: &Path) -> PathBuf {
    root.join("metadata").join("gmeow-self.ttl")
}

/// Parse a self-description Turtle file into structured metadata.
///
/// # Errors
///
/// Returns `Err(message)` if the file cannot be read, the Turtle fails to parse,
/// or required metadata is missing / malformed.
pub fn load_self_description(path: &Path) -> Result<SelfDescription, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| format!("{}: does not parse: {e}", path.display()))?;
    load_self_description_from_dataset(&ds)
}

/// Extract structured self-description metadata from a parsed RDF dataset.
///
/// # Errors
///
/// Returns `Err(message)` if required metadata is missing or malformed.
pub fn load_self_description_from_dataset(ds: &RdfDataset) -> Result<SelfDescription, String> {
    let work = iri_id(ds, WORK_IRI)
        .ok_or_else(|| format!("No work subject <{WORK_IRI}> found in self-description"))?;

    // The Manifestation is discovered dynamically as any URI subject carrying
    // gmeow:versionFingerprint (never the Work URI).
    let manifestation = subjects_with(ds, &gmeow("versionFingerprint"))
        .into_iter()
        .find(|s| matches!(ds.resolve(*s), TermRef::Iri(_)))
        .ok_or_else(|| {
            "No manifestation with gmeow:versionFingerprint found in self-description".to_string()
        })?;
    let TermRef::Iri(manifestation_iri) = ds.resolve(manifestation) else {
        return Err("manifestation is not an IRI".to_string());
    };
    let version_iri = manifestation_iri.to_string();

    let title = lit(ds, work, RDFS_LABEL, "work rdfs:label")?;
    let version = lit(
        ds,
        manifestation,
        &gmeow("versionFingerprint"),
        "manifestation gmeow:versionFingerprint",
    )?;
    let release_date = lit(
        ds,
        manifestation,
        &gmeow("datePublished"),
        "manifestation gmeow:datePublished",
    )?;
    let concept_doi = lit(ds, work, &dcterms("identifier"), "work dcterms:identifier")?;
    let version_doi = opt_lit(ds, manifestation, &dcterms("identifier"));
    let license_uri = opt_iri(ds, work, &dcterms("license")).unwrap_or_default();
    let homepage = opt_iri(ds, work, &foaf("homepage")).unwrap_or_default();

    if !doi_re().is_match(&concept_doi) {
        return Err(format!(
            "Invalid concept DOI format in self-description: {concept_doi:?}"
        ));
    }
    if let Some(vd) = &version_doi {
        if !doi_re().is_match(vd) {
            return Err(format!(
                "Invalid version DOI format in self-description: {vd:?}"
            ));
        }
        if vd == &concept_doi {
            return Err(format!(
                "Version DOI must differ from the concept DOI (both are {concept_doi:?}); \
                 they resolve to distinct resources."
            ));
        }
    }
    if !is_iso_date(&release_date) {
        return Err(format!(
            "Invalid release_date format in self-description (expected YYYY-MM-DD): {release_date:?}"
        ));
    }

    let publisher = opt_iri_term(ds, manifestation, &dcterms("publisher")).ok_or_else(|| {
        format!(
            "No dcterms:publisher found for manifestation <{version_iri}>; \
             publisher metadata is required for CrossRef deposits and other outputs."
        )
    })?;

    let depositor_name = opt_lit(ds, publisher, &foaf("name")).unwrap_or_default();
    let depositor_email = ds
        .term_id_by_value(&TermValue::iri(foaf("mbox")))
        .and_then(|p| {
            ds.quads_for_pattern(Some(publisher), Some(p), None, GraphMatch::Any)
                .find_map(|q| match ds.resolve(q.o) {
                    TermRef::Iri(n) => Some(n.trim_start_matches("mailto:").to_string()),
                    _ => None,
                })
        })
        .unwrap_or_default();
    let registrant = depositor_name.clone();

    let mut wikidata_links: Vec<String> = iri_objects(ds, publisher, &gmeow("authorityLink"))
        .into_iter()
        .filter(|o| o.starts_with(WIKIDATA_PREFIX))
        .collect();
    wikidata_links.sort();
    wikidata_links.dedup();
    if wikidata_links.len() > 1 {
        let TermRef::Iri(pub_iri) = ds.resolve(publisher) else {
            unreachable!("publisher resolved as IRI above")
        };
        return Err(format!(
            "Publisher <{pub_iri}> has multiple Wikidata authority links: {wikidata_links:?}. \
             Expected exactly one."
        ));
    }
    let registrant_wikidata = wikidata_links.into_iter().next();

    if depositor_name.is_empty() || depositor_email.is_empty() {
        let TermRef::Iri(pub_iri) = ds.resolve(publisher) else {
            unreachable!("publisher resolved as IRI above")
        };
        return Err(format!(
            "Publisher <{pub_iri}> must have foaf:name and foaf:mbox; \
             depositor metadata is required for CrossRef deposits."
        ));
    }
    if !email_re().is_match(&depositor_email) {
        return Err(format!(
            "Invalid depositor_email format in self-description: {depositor_email:?}"
        ));
    }

    let description = opt_lit(ds, work, SKOS_DEFINITION).unwrap_or_default();

    let repo_url = ds
        .term_id_by_value(&TermValue::iri(gmeow("webUrl")))
        .and_then(|p| {
            ds.quads_for_pattern(None, Some(p), None, GraphMatch::Any)
                .find_map(|q| match ds.resolve(q.o) {
                    TermRef::Iri(n) => Some(n.to_string()),
                    TermRef::Literal { lexical, .. } => Some(lexical.to_string()),
                    _ => None,
                })
        })
        .unwrap_or_default();

    let contributors = load_contributors(ds, work);
    if contributors.is_empty() {
        return Err(
            "No author Contribution found targeting the work; at least one contributor is \
             required for CrossRef deposits."
                .to_string(),
        );
    }

    Ok(SelfDescription {
        title,
        version,
        release_date,
        concept_doi,
        version_doi,
        version_iri,
        depositor_name,
        depositor_email,
        registrant,
        registrant_wikidata,
        license_uri,
        homepage,
        description,
        repo_url,
        contributors,
    })
}

/// The first IRI object of `subject predicate` resolved to its [`TermId`].
fn opt_iri_term(ds: &RdfDataset, subject: TermId, predicate: &str) -> Option<TermId> {
    let pred_id = iri_id(ds, predicate)?;
    ds.quads_for_pattern(Some(subject), Some(pred_id), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(_) => Some(q.o),
            _ => None,
        })
}

/// The preferred citable GMEOW DOI (version DOI if minted, else concept).
///
/// # Errors
///
/// Propagates any [`load_self_description`] failure.
pub fn full_doi(path: &Path) -> Result<String, String> {
    Ok(load_self_description(path)?.doi().to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Deposit-runtime configuration (canonical native copy of the Python `config`
// deposit constants + curated alignment targets)
// ─────────────────────────────────────────────────────────────────────────────

/// Registrant-presentation constants and the curated alignment-target table
/// consumed by CrossRef deposit assembly.
pub mod deposit_config {
    /// Ontology IRI (the document IRI, no trailing slash).
    pub const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
    /// LOD Cloud identifier and CrossRef `<item_number>` value.
    pub const DATASET_SLUG: &str = "GMEOW";
    /// Published content formats → CrossRef `<format>`.
    pub const DEPOSIT_FORMAT: &str = "Turtle; RDF/XML; N-Triples; JSON-LD; OWL; SHACL; GTS";
    /// Registrant mailing locale → CrossRef `<publisher_place>` / `<institution_place>`.
    pub const REGISTRANT_PLACE: &str = "Spruce Grove, AB, Canada";
    /// Registrant institutional acronym → CrossRef `<institution_acronym>`.
    pub const REGISTRANT_ACRONYM: &str = "BII";
    /// Whether the deposit emits nested Crossmark AccessIndicators.
    pub const CROSSMARK_ENABLED: bool = true;
    /// Crossmark policy DOI for GMEOW concept/version records.
    pub const CROSSMARK_POLICY_DOI: &str = "10.67342/xn9qgdr5mw/v1";

    /// One curated external vocabulary GMEOW aligns to:
    /// `(key, display name, namespace, kind)`.
    ///
    /// `kind` is `"upper" | "schema" | "concept_scheme"`. None of the curated
    /// targets carries a registered DOI, so the CrossRef related identifier is
    /// always the namespace URI.
    pub const ALIGNMENT_TARGETS: &[(&str, &str, &str, &str)] = &[
        ("gufo", "gUFO", "http://purl.org/nemo/gufo#", "upper"),
        ("ontouml", "OntoUML", "https://w3id.org/ontouml#", "upper"),
        ("umbel", "UMBEL", "http://umbel.org/umbel#", "upper"),
        (
            "dolce",
            "DOLCE/DUL",
            "http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#",
            "upper",
        ),
        ("bfo", "BFO", "http://purl.obolibrary.org/obo/bfo.owl", "upper"),
        ("foaf", "FOAF", "http://xmlns.com/foaf/0.1/", "schema"),
        (
            "rel",
            "REL (Relationship)",
            "http://purl.org/vocab/relationship/",
            "schema",
        ),
        ("doap", "DOAP", "http://usefulinc.com/ns/doap#", "schema"),
        ("prov", "PROV-O", "http://www.w3.org/ns/prov#", "schema"),
        ("dqv", "W3C DQV", "http://www.w3.org/ns/dqv#", "schema"),
        ("org", "ORG", "http://www.w3.org/ns/org#", "schema"),
        ("time", "OWL-Time", "http://www.w3.org/2006/time#", "schema"),
        ("schema", "Schema.org", "https://schema.org/", "schema"),
        (
            "dcterms",
            "DCMI Metadata Terms",
            "http://purl.org/dc/terms/",
            "schema",
        ),
        ("mo", "Music Ontology", "http://purl.org/ontology/mo/", "schema"),
        ("mbz", "MusicBrainz", "https://musicbrainz.org/", "schema"),
        ("discogs", "Discogs", "https://www.discogs.com/", "schema"),
        (
            "afo",
            "Audio Feature Ontology",
            "https://w3id.org/afo/onto/1.1#",
            "schema",
        ),
        (
            "afv",
            "Audio Feature Vocabulary",
            "https://w3id.org/afo/vocab/1.1#",
            "concept_scheme",
        ),
        (
            "jams",
            "JAMS Annotation Vocabulary",
            "http://w3id.org/polifonia/ontology/jams/",
            "schema",
        ),
        (
            "pon",
            "Polifonia Ontology Network",
            "https://w3id.org/polifonia/ontology/",
            "schema",
        ),
        (
            "chord",
            "OMRAS2 Chord Ontology",
            "http://purl.org/ontology/chord/",
            "schema",
        ),
        (
            "gedcom",
            "W3C GEDCOM",
            "http://www.w3.org/2000/10/swap/pim/gedcom#",
            "schema",
        ),
        ("vcard", "vCard", "http://www.w3.org/2006/vcard/ns#", "schema"),
        (
            "geo",
            "GeoSPARQL",
            "http://www.opengis.net/ont/geosparql#",
            "schema",
        ),
        (
            "wgs84",
            "WGS84 Geo Positioning",
            "http://www.w3.org/2003/01/geo/wgs84_pos#",
            "schema",
        ),
        (
            "tgn",
            "Getty TGN",
            "http://vocab.getty.edu/tgn/",
            "concept_scheme",
        ),
        (
            "gvp",
            "Getty Vocabulary Program",
            "http://vocab.getty.edu/ontology#",
            "concept_scheme",
        ),
        ("frbr", "FRBRcore", "http://purl.org/vocab/frbr/core#", "schema"),
        ("fabio", "FaBiO", "http://purl.org/spar/fabio/", "schema"),
        (
            "lrmoo",
            "LRMoo",
            "http://iflastandards.info/ns/lrm/lrmoo/",
            "schema",
        ),
        ("bibo", "BIBO", "http://purl.org/ontology/bibo/", "schema"),
        (
            "bibframe",
            "BIBFRAME",
            "http://id.loc.gov/ontologies/bibframe/",
            "schema",
        ),
        ("sioc", "SIOC", "http://rdfs.org/sioc/ns#", "schema"),
        (
            "skos",
            "SKOS",
            "http://www.w3.org/2004/02/skos/core#",
            "concept_scheme",
        ),
        (
            "nmo",
            "Nepomuk Message Ontology",
            "http://www.semanticdesktop.org/ontologies/2007/03/22/nmo#",
            "schema",
        ),
        ("wot", "WOT Schema", "http://xmlns.com/wot/0.1/", "schema"),
        ("odrl", "ODRL 2.2", "http://www.w3.org/ns/odrl/2/", "schema"),
        ("cc", "CC REL", "http://creativecommons.org/ns#", "schema"),
        (
            "premis",
            "PREMIS 3",
            "http://www.loc.gov/premis/rdf/v3/",
            "schema",
        ),
        (
            "rstmt",
            "RightsStatements.org",
            "https://rightsstatements.org/vocab/",
            "concept_scheme",
        ),
        ("spdx", "SPDX", "http://spdx.org/rdf/terms#", "schema"),
        (
            "spdxlic",
            "SPDX License List",
            "http://spdx.org/licenses/",
            "concept_scheme",
        ),
        (
            "codemeta",
            "CodeMeta",
            "https://codemeta.github.io/terms/#",
            "schema",
        ),
        ("forgefed", "ForgeFed", "https://forgefed.org/ns#", "schema"),
        (
            "ma",
            "Ontology for Media Resources",
            "http://www.w3.org/ns/ma-ont#",
            "schema",
        ),
        (
            "gsso",
            "Gender, Sex, and Sexual Orientation Ontology",
            "http://purl.obolibrary.org/obo/GSSO_",
            "concept_scheme",
        ),
        (
            "homosaurus",
            "Homosaurus",
            "https://homosaurus.org/v4/",
            "concept_scheme",
        ),
        ("fhir", "HL7 FHIR", "http://hl7.org/fhir/", "schema"),
        (
            "bio",
            "BIO vocabulary",
            "http://purl.org/vocab/bio/0.1/",
            "schema",
        ),
        ("gedcomx", "GEDCOM X", "http://gedcomx.org/", "schema"),
        (
            "geonames",
            "GeoNames",
            "http://www.geonames.org/ontology#",
            "concept_scheme",
        ),
        (
            "wikidata",
            "Wikidata",
            "http://www.wikidata.org/entity/",
            "concept_scheme",
        ),
        ("lexvo", "Lexvo", "http://lexvo.org/id/", "concept_scheme"),
        (
            "glottolog",
            "Glottolog",
            "https://glottolog.org/resource/languoid/id/",
            "concept_scheme",
        ),
        (
            "ontolex",
            "OntoLex-Lemon",
            "http://www.w3.org/ns/lemon/ontolex#",
            "schema",
        ),
        ("lime", "LIME", "http://www.w3.org/ns/lemon/lime#", "schema"),
        ("qudt", "QUDT", "http://qudt.org/schema/qudt/", "schema"),
        ("gtfs", "GTFS", "http://vocab.gtfs.org/terms#", "schema"),
        (
            "fibo-fnd-acc-cur",
            "FIBO CurrencyAmount",
            "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/CurrencyAmount/",
            "schema",
        ),
        (
            "fibo-iso4217",
            "FIBO ISO4217 Currency Codes",
            "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/ISO4217-CurrencyCodes/",
            "schema",
        ),
        (
            "fibo-fnd-acc-ae",
            "FIBO AccountingEquity",
            "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/AccountingEquity/",
            "schema",
        ),
        (
            "fibo-fbc-fi-fi",
            "FIBO FinancialInstruments",
            "https://spec.edmcouncil.org/fibo/ontology/FBC/FinancialInstruments/FinancialInstruments/",
            "schema",
        ),
        (
            "fibo-fbc-pas-fpas",
            "FIBO FinancialProductsAndServices",
            "https://spec.edmcouncil.org/fibo/ontology/FBC/ProductsAndServices/FinancialProductsAndServices/",
            "schema",
        ),
        (
            "fibo-fnd-pas-ps",
            "FIBO ProductsAndServices",
            "https://spec.edmcouncil.org/fibo/ontology/FND/ProductsAndServices/ProductsAndServices/",
            "schema",
        ),
        (
            "brick",
            "Brick",
            "https://brickschema.org/schema/Brick#",
            "schema",
        ),
        (
            "bot",
            "BOT (Building Topology Ontology)",
            "https://w3id.org/bot#",
            "schema",
        ),
        (
            "ifc",
            "ifcOWL (IFC4)",
            "http://www.buildingsmart-tech.org/ifcOWL/IFC4#",
            "schema",
        ),
        (
            "crmsci",
            "CRMsci",
            "http://www.cidoc-crm.org/extensions/crmsci/",
            "schema",
        ),
        (
            "lvont",
            "Lexvo Ontology",
            "http://lexvo.org/ontology#",
            "schema",
        ),
        ("moat", "MOAT", "http://moat-project.org/ns#", "schema"),
        (
            "tags",
            "Tag Ontology",
            "http://www.holygoat.co.uk/owl/redwood/0.1/tags/",
            "schema",
        ),
        (
            "qb",
            "RDF Data Cube",
            "http://purl.org/linked-data/cube#",
            "schema",
        ),
        (
            "mf",
            "OGC Moving Features",
            "http://www.opengis.net/ont/movingfeatures#",
            "schema",
        ),
        (
            "faldo",
            "FALDO",
            "http://biohackathon.org/resource/faldo#",
            "schema",
        ),
        (
            "so",
            "Sequence Ontology",
            "http://purl.obolibrary.org/obo/SO_",
            "concept_scheme",
        ),
        (
            "crmarc",
            "CRMarchaeo",
            "http://www.cidoc-crm.org/crmarchaeo/",
            "schema",
        ),
        (
            "crmdig",
            "CRMdig",
            "http://www.ics.forth.gr/isl/CRMdig/",
            "schema",
        ),
        ("exif", "W3C EXIF", "http://www.w3.org/2003/12/exif/ns#", "schema"),
        (
            "iiif",
            "IIIF Presentation API",
            "http://iiif.io/api/presentation/3#",
            "schema",
        ),
        (
            "obscore",
            "IVOA ObsCore",
            "http://www.ivoa.net/rdf/ObsCore#",
            "schema",
        ),
        ("ivoa", "IVOA", "http://www.ivoa.net/rdf/", "schema"),
        (
            "bbc",
            "BBC News Ontology",
            "http://www.bbc.co.uk/ontologies/news/",
            "schema",
        ),
        (
            "iptc",
            "IPTC NewsML-G2",
            "http://iptc.org/std/NewsML-G2/",
            "schema",
        ),
        ("loinc", "LOINC", "http://loinc.org/rdf/", "schema"),
        (
            "snomed",
            "SNOMED CT",
            "http://snomed.info/id/",
            "concept_scheme",
        ),
    ];
}

/// The alignment targets, sorted by key, as CrossRef deposit inputs.
fn alignment_target_inputs() -> Vec<AlignmentTargetInput> {
    let mut targets: Vec<AlignmentTargetInput> = deposit_config::ALIGNMENT_TARGETS
        .iter()
        .map(|(key, name, namespace, kind)| AlignmentTargetInput {
            key: (*key).to_string(),
            name: (*name).to_string(),
            namespace: (*namespace).to_string(),
            kind: (*kind).to_string(),
            doi: None,
            related_identifier: (*namespace).to_string(),
        })
        .collect();
    targets.sort_by(|a, b| a.key.cmp(&b.key));
    targets
}

fn config_input() -> ConfigInput {
    ConfigInput {
        ontology_iri: deposit_config::ONTOLOGY_IRI.to_string(),
        dataset_slug: deposit_config::DATASET_SLUG.to_string(),
        deposit_format: deposit_config::DEPOSIT_FORMAT.to_string(),
        registrant_place: deposit_config::REGISTRANT_PLACE.to_string(),
        registrant_acronym: deposit_config::REGISTRANT_ACRONYM.to_string(),
        crossmark_enabled: deposit_config::CROSSMARK_ENABLED,
        crossmark_policy_doi: deposit_config::CROSSMARK_POLICY_DOI.to_string(),
        alignment_targets: alignment_target_inputs(),
    }
}

fn self_description_input(description: &SelfDescription) -> SelfDescriptionInput {
    SelfDescriptionInput {
        title: description.title.clone(),
        version: description.version.clone(),
        release_date: description.release_date.clone(),
        concept_doi: description.concept_doi.clone(),
        version_doi: description.version_doi.clone(),
        version_iri: description.version_iri.clone(),
        depositor_name: description.depositor_name.clone(),
        depositor_email: description.depositor_email.clone(),
        registrant: description.registrant.clone(),
        registrant_wikidata: description.registrant_wikidata.clone(),
        license_uri: description.license_uri.clone(),
        homepage: description.homepage.clone(),
        description: description.description.clone(),
        repo_url: description.repo_url.clone(),
        contributors: description
            .contributors
            .iter()
            .map(|c| ContributorInput {
                kind: c.kind.clone(),
                name: c.name.clone(),
                orcid: c.orcid.clone(),
                sequence: c.sequence.clone(),
                role: c.role.clone(),
            })
            .collect(),
    }
}

/// Assemble the [`DepositInput`] the native CrossRef XML generator consumes.
pub fn deposit_input(description: &SelfDescription) -> DepositInput {
    DepositInput {
        self_description: self_description_input(description),
        config: config_input(),
    }
}

/// Serialise a [`SelfDescription`] + runtime config to the deposit-input JSON
/// understood by [`crate::crossref::build_deposit_xml`].
///
/// # Errors
///
/// Returns `Err(message)` if JSON serialisation fails (never expected for the
/// plain-data inputs).
pub fn deposit_input_json(description: &SelfDescription) -> Result<String, String> {
    serde_json::to_string(&deposit_input(description)).map_err(|e| e.to_string())
}

/// Assemble the [`LintInput`] the native CrossRef linter consumes.
///
/// `citation_cff` / `ontology_ttl` are the file contents (already read by the
/// caller); `None` when a file does not exist. The linter renders its own XML,
/// so no pre-rendered XML is passed.
pub fn lint_input(
    description: &SelfDescription,
    citation_cff: Option<String>,
    ontology_ttl: Option<String>,
) -> LintInput {
    let deposit = deposit_input(description);
    LintInput {
        self_description: deposit.self_description,
        config: deposit.config,
        citation_cff,
        ontology_ttl,
    }
}

/// Serialise the full lint context to the JSON understood by
/// [`crate::crossref::lint_deposit`].
///
/// # Errors
///
/// Returns `Err(message)` if JSON serialisation fails.
pub fn lint_input_json(
    description: &SelfDescription,
    citation_cff: Option<String>,
    ontology_ttl: Option<String>,
) -> Result<String, String> {
    serde_json::to_string(&lint_input(description, citation_cff, ontology_ttl))
        .map_err(|e| e.to_string())
}

/// A `(timestamp, batch_id)` pair for a fresh CrossRef submission.
///
/// The timestamp is the current UTC time in `%Y%m%d%H%M%S`: CrossRef uses it to
/// order (re)submissions of the same DOI, and the batch id embeds it so each
/// generated deposit is uniquely identifiable.
pub fn live_stamp(description: &SelfDescription) -> (String, String) {
    let timestamp = crate::time_util::utc_compact();
    let batch_id = format!("gmeow-{}-{}", description.version, timestamp);
    (timestamp, batch_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        // crates/validate/src/self_desc.rs → repo root is three ancestors up.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf()
    }

    fn load() -> SelfDescription {
        let path = default_self_desc_path(&repo_root());
        load_self_description(&path).expect("self-description parses")
    }

    #[test]
    fn parses_core_metadata() {
        let sd = load();
        assert_eq!(sd.concept_doi, "10.67342/26w4o");
        assert_eq!(sd.version, "0.1.0");
        assert_eq!(sd.release_date, "2026-06-03");
        assert_eq!(sd.version_doi, None);
        assert_eq!(sd.doi(), "10.67342/26w4o");
        assert_eq!(sd.version_iri, "https://blackcatinformatics.ca/gmeow/0.1.0");
        assert_eq!(
            sd.license_uri,
            "https://creativecommons.org/licenses/by/4.0/"
        );
        assert_eq!(sd.homepage, "https://blackcatinformatics.ca/gmeow");
        assert_eq!(
            sd.repo_url,
            "https://github.com/Blackcat-Informatics/gmeow-ontology"
        );
        assert!(!sd.title.is_empty());
        assert!(!sd.description.is_empty());
    }

    #[test]
    fn parses_depositor_and_registrant() {
        let sd = load();
        assert_eq!(sd.depositor_name, "Blackcat Informatics® Inc.");
        assert_eq!(sd.depositor_email, "root@blackcatinformatics.ca");
        assert_eq!(sd.registrant, "Blackcat Informatics® Inc.");
        assert_eq!(
            sd.registrant_wikidata.as_deref(),
            Some("http://www.wikidata.org/entity/Q140285712")
        );
    }

    #[test]
    fn parses_contributors_orgs_first_then_persons() {
        let sd = load();
        assert_eq!(sd.contributors.len(), 2);
        let org = &sd.contributors[0];
        assert_eq!(org.kind, "organization");
        assert_eq!(org.name, "Blackcat Informatics® Inc.");
        assert_eq!(org.sequence, "first");
        assert_eq!(org.orcid, None);
        let person = &sd.contributors[1];
        assert_eq!(person.kind, "person");
        assert_eq!(person.name, "Patrick Audley");
        assert_eq!(person.sequence, "additional");
        assert_eq!(
            person.orcid.as_deref(),
            Some("https://orcid.org/0000-0003-4382-7625")
        );
        assert_eq!(person.given_name(), "Patrick");
        assert_eq!(person.surname(), "Audley");
    }

    #[test]
    fn deposit_input_carries_config_and_sorted_alignments() {
        let sd = load();
        let deposit = deposit_input(&sd);
        assert_eq!(
            deposit.config.deposit_format,
            "Turtle; RDF/XML; N-Triples; JSON-LD; OWL; SHACL; GTS"
        );
        assert!(deposit.config.crossmark_enabled);
        let keys: Vec<&str> = deposit
            .config
            .alignment_targets
            .iter()
            .map(|t| t.key.as_str())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "alignment targets must be key-sorted");
        // Every curated target is present and its related identifier is the namespace.
        assert_eq!(
            deposit.config.alignment_targets.len(),
            deposit_config::ALIGNMENT_TARGETS.len()
        );
        for target in &deposit.config.alignment_targets {
            assert_eq!(target.doi, None);
            assert_eq!(target.related_identifier, target.namespace);
        }
    }

    #[test]
    fn deposit_and_lint_json_round_trip_through_serde() {
        let sd = load();
        let deposit_json = deposit_input_json(&sd).expect("deposit json");
        let back: DepositInput = serde_json::from_str(&deposit_json).expect("valid deposit json");
        assert_eq!(back.self_description.concept_doi, "10.67342/26w4o");

        let lint_json = lint_input_json(&sd, Some("cff".into()), None).expect("lint json");
        assert!(lint_json.contains("\"citation_cff\":\"cff\""));
        assert!(lint_json.contains("\"ontology_ttl\":null"));
    }

    #[test]
    fn live_stamp_embeds_version() {
        let sd = load();
        let (timestamp, batch_id) = live_stamp(&sd);
        assert_eq!(timestamp.len(), 14);
        assert!(timestamp.bytes().all(|b| b.is_ascii_digit()));
        assert!(batch_id.starts_with("gmeow-0.1.0-"));
        assert!(batch_id.ends_with(&timestamp));
    }

    #[test]
    fn iso_date_validation() {
        assert!(is_iso_date("2026-06-03"));
        assert!(is_iso_date("2024-02-29")); // leap day
        assert!(!is_iso_date("2023-02-29")); // not a leap year
        assert!(!is_iso_date("2026-13-01"));
        assert!(!is_iso_date("2026-6-3"));
        assert!(!is_iso_date("2026/06/03"));
        assert!(!is_iso_date("not-a-date"));
    }
}
