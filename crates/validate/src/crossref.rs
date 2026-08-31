// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! CrossRef deposit-XML generation in Rust.
//!
//! Reproduces the byte-identical output of the Python
//! `crossref.build_deposit_xml` / `crossref.lint_deposit` pair so the
//! generation hot-path no longer depends on `xml.etree.ElementTree`.
//!
//! # Marshalling boundary
//!
//! The caller (Python) serialises a [`DepositInput`] struct as JSON and passes
//! it as a `&str`. The JSON carries everything the generator needs:
//!
//! * `self_description` — all fields from `SelfDescription` (contributors,
//!   DOIs, version, dates, …).
//! * `config` — runtime constants that come from `config.py`
//!   (`ontology_iri`, `dataset_slug`, `deposit_format`, …) plus the full
//!   `alignment_targets` list.
//!
//! The lint path additionally receives the CITATION.cff text and the
//! ontology Turtle text as plain strings so the Rust side can do the
//! file-content checks without `Path`/IO.
//!
//! # XML serialisation contract
//!
//! The output is **byte-identical** to Python's `ET.tostring(root,
//! encoding="unicode", xml_declaration=True)` after `ET.indent(root)`.
//! Concretely:
//!
//! * XML declaration: `<?xml version='1.0' encoding='utf-8'?>` (single quotes).
//! * Root element namespace declarations appear in registration order:
//!   `xmlns="CR_NS"`, `xmlns:ai="AI_NS"`, `xmlns:rel="REL_NS"`, `xmlns:xsi=`.
//! * `xsi:schemaLocation` and `version` follow the namespace attrs.
//! * `ET.indent(root)` inserts two-space indentation with `\n` between nodes.
//! * Empty (leaf) elements: `<tag attr="val" />` (space before `/>` — Python's
//!   default inserts it).
//! * Comment nodes: `<!-- text -->`.
//! * Text content escapes `&` → `&amp;` and `<` → `&lt;`.
//! * Attribute values additionally escape `"` → `&quot;`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

// ─────────────────────────────────────────────────────────────────────────────
// Namespace constants (must match crossref.py exactly)
// ─────────────────────────────────────────────────────────────────────────────

const CR_NS: &str = "http://www.crossref.org/schema/5.4.0";
const AI_NS: &str = "http://www.crossref.org/AccessIndicators.xsd";
const REL_NS: &str = "http://www.crossref.org/relations.xsd";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
const PLACEHOLDER_DOI_PREFIX: &str = "10.XXXXX";

/// Format a string value in Python's ``!r`` style (single-quoted).
///
/// Python's ``f"{value!r}"`` produces ``'some value'`` (single quotes) for
/// plain ASCII strings. Rust's ``{:?}`` produces ``"some value"`` (double
/// quotes). This helper produces the Python-compatible form so that lint
/// messages are byte-identical to those emitted by the Python oracle.
fn py_repr(s: &str) -> String {
    // Escape backslashes and single quotes; Python repr leaves double quotes bare.
    let escaped = s.replace('\\', "\\\\").replace("'", "\\'");
    format!("'{escaped}'")
}

const SERIALIZATIONS: &[(&str, &str, &str)] = &[
    ("ttl", "Turtle", "text/turtle"),
    ("rdf", "RDF/XML", "application/rdf+xml"),
    ("nt", "N-Triples", "application/n-triples"),
    ("jsonld", "JSON-LD", "application/ld+json"),
    (
        "gts",
        "GTS content-addressed package",
        "application/cbor-seq",
    ),
];

// ─────────────────────────────────────────────────────────────────────────────
// Serialisation types (JSON in from Python)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
pub struct ContributorInput {
    pub kind: String,
    pub name: String,
    pub orcid: Option<String>,
    pub sequence: String,
    pub role: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct AlignmentTargetInput {
    /// The dict key (sorted to produce alphabetical ordering).
    pub key: String,
    /// Display name, e.g. "gUFO".
    pub name: String,
    /// Base namespace URI.
    pub namespace: String,
    /// "upper" | "schema" | "concept_scheme"
    pub kind: String,
    /// Optional DOI; when present used as `identifier-type="doi"`.
    pub doi: Option<String>,
    /// The resolved related_identifier (doi if Some(doi), else namespace).
    pub related_identifier: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SelfDescriptionInput {
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
    pub contributors: Vec<ContributorInput>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ConfigInput {
    pub ontology_iri: String,
    pub dataset_slug: String,
    pub deposit_format: String,
    pub registrant_place: String,
    pub registrant_acronym: String,
    pub crossmark_enabled: bool,
    pub crossmark_policy_doi: String,
    pub alignment_targets: Vec<AlignmentTargetInput>,
}

#[derive(Deserialize, Serialize)]
pub struct DepositInput {
    pub self_description: SelfDescriptionInput,
    pub config: ConfigInput,
}

#[derive(Deserialize, Serialize)]
pub struct LintInput {
    pub self_description: SelfDescriptionInput,
    pub config: ConfigInput,
    /// Contents of CITATION.cff (None if file does not exist).
    pub citation_cff: Option<String>,
    /// Contents of ontology/gmeow.ttl (None if file does not exist).
    pub ontology_ttl: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// XML helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Escape text content: `&` → `&amp;`, `<` → `&lt;`.
fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

/// Escape attribute value: additionally `"` → `&quot;`.
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

// ─────────────────────────────────────────────────────────────────────────────
// Virtual DOM
//
// We build a tree then serialise it with Python-compatible indentation.
// ─────────────────────────────────────────────────────────────────────────────

enum Node {
    Elem {
        ns: &'static str,
        local: String,
        attrs: Vec<(String, String)>,
        /// Text content of the element (exclusive with children).
        text: Option<String>,
        children: Vec<Node>,
    },
    Comment(String),
}

impl Node {
    fn elem(ns: &'static str, local: &str) -> Self {
        Node::Elem {
            ns,
            local: local.to_string(),
            attrs: vec![],
            text: None,
            children: vec![],
        }
    }

    fn with_text(mut self, t: &str) -> Self {
        if let Node::Elem { ref mut text, .. } = self {
            *text = Some(t.to_string());
        }
        self
    }

    fn with_attr(mut self, name: &str, value: &str) -> Self {
        if let Node::Elem { ref mut attrs, .. } = self {
            attrs.push((name.to_string(), value.to_string()));
        }
        self
    }

    fn push(mut self, child: Node) -> Self {
        if let Node::Elem {
            ref mut children, ..
        } = self
        {
            children.push(child);
        }
        self
    }

    fn push_mut(&mut self, child: Node) {
        if let Node::Elem { children, .. } = self {
            children.push(child);
        }
    }
}

/// Map a namespace URI to its XML prefix.
fn ns_prefix(ns: &str) -> &str {
    match ns {
        s if s == CR_NS => "", // default namespace — no prefix
        s if s == AI_NS => "ai",
        s if s == REL_NS => "rel",
        _ => "",
    }
}

/// Serialise a node at the given indentation depth.
fn write_node(node: &Node, w: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    match node {
        Node::Comment(text) => {
            w.push_str(&indent);
            w.push_str("<!--");
            w.push_str(text);
            w.push_str("-->");
        }
        Node::Elem {
            ns,
            local,
            attrs,
            text,
            children,
        } => {
            let prefix = ns_prefix(ns);
            let tag = if prefix.is_empty() {
                local.clone()
            } else {
                format!("{prefix}:{local}")
            };

            w.push_str(&indent);
            w.push('<');
            w.push_str(&tag);
            for (k, v) in attrs {
                w.push(' ');
                w.push_str(k);
                w.push_str("=\"");
                w.push_str(&esc_attr(v));
                w.push('"');
            }

            let has_children = !children.is_empty();
            let has_text = text.as_ref().is_some_and(|s| !s.is_empty());

            if !has_children && !has_text {
                // Empty element: Python writes `<tag ... />` with a space.
                w.push_str(" />");
            } else {
                w.push('>');
                if has_children {
                    for child in children {
                        w.push('\n');
                        write_node(child, w, depth + 1);
                    }
                    w.push('\n');
                    w.push_str(&indent);
                } else {
                    // text only
                    w.push_str(&esc_text(text.as_deref().unwrap_or("")));
                }
                w.push_str("</");
                w.push_str(&tag);
                w.push('>');
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Convenience constructors
// ─────────────────────────────────────────────────────────────────────────────

fn cr(local: &str) -> Node {
    Node::elem(CR_NS, local)
}
fn ai_node(local: &str) -> Node {
    Node::elem(AI_NS, local)
}
fn rel_node(local: &str) -> Node {
    Node::elem(REL_NS, local)
}

// ─────────────────────────────────────────────────────────────────────────────
// Small helpers
// ─────────────────────────────────────────────────────────────────────────────

fn doi_suffix(doi: &str) -> &str {
    doi.find('/').map(|p| &doi[p + 1..]).unwrap_or(doi)
}

fn crossref_pid_uri(s: &str) -> String {
    s.replace(
        "http://www.wikidata.org/entity/",
        "https://www.wikidata.org/entity/",
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Element builders (mirroring crossref.py's `_add_*` functions)
// ─────────────────────────────────────────────────────────────────────────────

fn build_contributors(contributors: &[ContributorInput]) -> Node {
    let mut node = cr("contributors");
    for c in contributors {
        if c.kind == "organization" {
            let org = cr("organization")
                .with_text(&c.name)
                .with_attr("sequence", &c.sequence)
                .with_attr("contributor_role", &c.role);
            node.push_mut(org);
            continue;
        }
        let mut person = cr("person_name")
            .with_attr("sequence", &c.sequence)
            .with_attr("contributor_role", &c.role);
        // given_name = everything before the last space
        let (given, surname) = if let Some(pos) = c.name.rfind(' ') {
            (&c.name[..pos], &c.name[pos + 1..])
        } else {
            ("", c.name.as_str())
        };
        if !given.is_empty() {
            person.push_mut(cr("given_name").with_text(given));
        }
        person.push_mut(cr("surname").with_text(surname));
        if let Some(ref orcid) = c.orcid {
            person.push_mut(cr("ORCID").with_text(orcid));
        }
        node.push_mut(person);
    }
    node
}

/// Validate that `date` is an `xsd:date`-shaped `YYYY-MM-DD` string (the only shape
/// `build_date` can split into year/month/day). Returns a precise error otherwise so
/// the deposit-XML builder hard-fails instead of panicking on an out-of-range index.
fn validate_iso_date(date: &str) -> gmeow_errors::Result<()> {
    let parts: Vec<&str> = date.splitn(3, '-').collect();
    let well_formed = parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()));
    if well_formed {
        Ok(())
    } else {
        Err(gmeow_errors::Diag::of_kind(crate::error::Crossref {
            detail: format!(
                "release_date {date:?} is not a well-formed YYYY-MM-DD date; \
             the Crossref deposit requires an xsd:date-shaped release_date"
            ),
        }))
    }
}

/// Returns a `database_date` wrapper containing `date_name` with month/day/year.
///
/// Callers MUST have validated `iso_date` via [`validate_iso_date`] first (the
/// deposit-XML builder does); the defensive fallbacks here keep this total even if a
/// future caller forgets.
fn build_date(date_name: &str, iso_date: &str) -> Node {
    let parts: Vec<&str> = iso_date.splitn(3, '-').collect();
    let year = parts.first().copied().unwrap_or_default();
    let month = parts.get(1).copied().unwrap_or_default();
    let day = parts.get(2).copied().unwrap_or_default();
    let date_elem = cr(date_name)
        .with_attr("media_type", "online")
        .push(cr("month").with_text(month))
        .push(cr("day").with_text(day))
        .push(cr("year").with_text(year));
    cr("database_date").push(date_elem)
}

fn build_publisher(name: &str, place: &str) -> Node {
    let mut node = cr("publisher");
    node.push_mut(cr("publisher_name").with_text(name));
    if !place.is_empty() {
        node.push_mut(cr("publisher_place").with_text(place));
    }
    node
}

fn build_institution(name: &str, acronym: &str, place: &str, identifiers: &[(&str, &str)]) -> Node {
    let mut node = cr("institution");
    node.push_mut(cr("institution_name").with_text(name));
    for (id_type, value) in identifiers {
        let converted = crossref_pid_uri(value);
        node.push_mut(
            cr("institution_id")
                .with_text(&converted)
                .with_attr("type", id_type),
        );
    }
    if !acronym.is_empty() {
        node.push_mut(cr("institution_acronym").with_text(acronym));
    }
    if !place.is_empty() {
        node.push_mut(cr("institution_place").with_text(place));
    }
    node
}

fn build_publisher_item(
    item_numbers: &[(&str, &str)],
    identifiers: &[(&str, &str)],
) -> Option<Node> {
    if item_numbers.is_empty() && identifiers.is_empty() {
        return None;
    }
    let mut node = cr("publisher_item");
    for (number_type, value) in item_numbers {
        node.push_mut(
            cr("item_number")
                .with_text(value)
                .with_attr("item_number_type", number_type),
        );
    }
    for (id_type, value) in identifiers {
        node.push_mut(
            cr("identifier")
                .with_text(value)
                .with_attr("id_type", id_type),
        );
    }
    Some(node)
}

fn build_version_info(version: &str, description: &str) -> Node {
    let mut node = cr("version_info");
    node.push_mut(cr("version").with_text(version));
    if !description.is_empty() {
        node.push_mut(cr("description").with_text(description));
    }
    node
}

/// Build the `ai:program name="AccessIndicators"` block.
fn build_access(license_url: &str, start_date: &str) -> Option<Node> {
    if license_url.is_empty() {
        return None;
    }
    let mut prog = ai_node("program").with_attr("name", "AccessIndicators");
    prog.push_mut(ai_node("free_to_read").with_attr("start_date", start_date));
    for applies_to in ["vor", "tdm"] {
        prog.push_mut(
            ai_node("license_ref")
                .with_text(license_url)
                .with_attr("start_date", start_date)
                .with_attr("applies_to", applies_to),
        );
    }
    Some(prog)
}

fn build_crossmark(policy_doi: &str, license_url: &str, start_date: &str) -> Node {
    let mut crossmark = cr("crossmark");
    crossmark.push_mut(cr("crossmark_version").with_text("1"));
    crossmark.push_mut(cr("crossmark_policy").with_text(policy_doi));
    if !license_url.is_empty() {
        let mut custom_metadata = cr("custom_metadata");
        if let Some(access) = build_access(license_url, start_date) {
            custom_metadata.push_mut(access);
        }
        crossmark.push_mut(custom_metadata);
    }
    crossmark
}

struct Relation {
    kind: &'static str,
    rel_type: String,
    identifier_type: &'static str,
    target: String,
    description: String,
}

fn build_relations(relations: &[Relation]) -> Option<Node> {
    if relations.is_empty() {
        return None;
    }
    let mut program = rel_node("program").with_attr("name", "relations");
    for r in relations {
        let mut item = rel_node("related_item");
        if !r.description.is_empty() {
            item.push_mut(rel_node("description").with_text(&r.description));
        }
        item.push_mut(
            rel_node(r.kind)
                .with_text(&r.target)
                .with_attr("relationship-type", &r.rel_type)
                .with_attr("identifier-type", r.identifier_type),
        );
        program.push_mut(item);
    }
    Some(program)
}

struct TdmResource {
    url: String,
    mime_type: &'static str,
}

fn build_tdm_resources(resources: &[TdmResource]) -> Option<Node> {
    if resources.is_empty() {
        return None;
    }
    let mut collection = cr("collection").with_attr("property", "text-mining");
    for r in resources {
        let item = cr("item").push(
            cr("resource")
                .with_text(&r.url)
                .with_attr("mime_type", r.mime_type)
                .with_attr("content_version", "vor"),
        );
        collection.push_mut(item);
    }
    Some(collection)
}

struct Citation {
    key: String,
    doi: Option<String>,
    unstructured: String,
}

fn build_citation_list(citations: &[Citation]) -> Option<Node> {
    if citations.is_empty() {
        return None;
    }
    let mut list = cr("citation_list");
    for c in citations {
        let mut node = cr("citation")
            .with_attr("key", &c.key)
            .with_attr("type", "web_resource");
        if let Some(ref d) = c.doi {
            node.push_mut(cr("doi").with_text(d));
        }
        node.push_mut(cr("unstructured_citation").with_text(&c.unstructured));
        list.push_mut(node);
    }
    Some(list)
}

// ─────────────────────────────────────────────────────────────────────────────
// Relation / resource / citation projections
// ─────────────────────────────────────────────────────────────────────────────

fn format_relations(base_iri: &str) -> Vec<Relation> {
    SERIALIZATIONS
        .iter()
        .map(|(ext, label, _)| Relation {
            kind: "intra_work_relation",
            rel_type: "hasFormat".to_string(),
            identifier_type: "uri",
            target: format!("{base_iri}.{ext}"),
            description: format!("{label} serialization of the ontology."),
        })
        .collect()
}

fn tdm_resources_for(base_iri: &str) -> Vec<TdmResource> {
    SERIALIZATIONS
        .iter()
        .map(|(ext, _, mime)| TdmResource {
            url: format!("{base_iri}.{ext}"),
            mime_type: mime,
        })
        .collect()
}

fn alignment_relations(targets: &[AlignmentTargetInput]) -> Vec<Relation> {
    let mut sorted: Vec<&AlignmentTargetInput> = targets.iter().collect();
    sorted.sort_by_key(|t| t.key.as_str());
    sorted
        .into_iter()
        .map(|t| Relation {
            kind: "inter_work_relation",
            rel_type: if t.kind == "upper" {
                "isDerivedFrom".to_string()
            } else {
                "references".to_string()
            },
            identifier_type: if t.doi.is_some() { "doi" } else { "uri" },
            target: t.related_identifier.clone(),
            description: format!("GMEOW aligns to {} by reference.", t.name),
        })
        .collect()
}

fn alignment_citations(targets: &[AlignmentTargetInput]) -> Vec<Citation> {
    let mut sorted: Vec<&AlignmentTargetInput> = targets.iter().collect();
    sorted.sort_by_key(|t| t.key.as_str());
    sorted
        .into_iter()
        .map(|t| Citation {
            key: format!("ref-{}", t.key),
            doi: t.doi.clone(),
            unstructured: format!("{}. {}.", t.name, t.related_identifier),
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Dataset builder
// ─────────────────────────────────────────────────────────────────────────────

/// Build one `<dataset dataset_type="record">` element.
///
/// Element order matches `_add_dataset` in `crossref.py`:
/// 1. contributors
/// 2. titles
/// 3. publication_date (database_date)
/// 4. update_date (database_date)
/// 5. publisher_item
/// 6. description
/// 7. format
/// 8. crossmark / ai:program
/// 9. rel:program
/// 10. version_info
/// 11. [comment] if component_seam
/// 12. doi_data (doi, resource, collection)
/// 13. citation_list
#[allow(clippy::too_many_arguments)]
fn build_dataset(
    sd: &SelfDescriptionInput,
    config: &ConfigInput,
    doi: &str,
    resource: &str,
    title: &str,
    relations: Vec<Relation>,
    tdm: Vec<TdmResource>,
    citations: Vec<Citation>,
    component_seam: bool,
    crossmark_policy: Option<&str>,
) -> Node {
    let mut dataset = cr("dataset").with_attr("dataset_type", "record");

    dataset.push_mut(build_contributors(&sd.contributors));

    let mut titles_node = cr("titles");
    titles_node.push_mut(cr("title").with_text(title));
    dataset.push_mut(titles_node);

    dataset.push_mut(build_date("publication_date", &sd.release_date));
    dataset.push_mut(build_date("update_date", &sd.release_date));

    if let Some(pi) = build_publisher_item(
        &[
            ("doi-suffix", doi_suffix(doi)),
            ("site", &config.dataset_slug),
        ],
        &[("other", resource)],
    ) {
        dataset.push_mut(pi);
    }
    dataset.push_mut(cr("description").with_text(&sd.description));
    dataset.push_mut(cr("format").with_text(&config.deposit_format));

    if let Some(policy) = crossmark_policy {
        dataset.push_mut(build_crossmark(policy, &sd.license_uri, &sd.release_date));
    } else if let Some(access) = build_access(&sd.license_uri, &sd.release_date) {
        dataset.push_mut(access);
    }

    if let Some(rel_prog) = build_relations(&relations) {
        dataset.push_mut(rel_prog);
    }

    dataset.push_mut(build_version_info(
        &sd.version,
        &format!("Release {} of the GMEOW ontology.", sd.version),
    ));

    if component_seam {
        dataset.push_mut(Node::Comment(
            " profile sub-DOI seam: future <component_list> with \
             <component parent_relation=\"isPartOf\"> per profile "
                .to_string(),
        ));
    }

    let mut doi_data = cr("doi_data");
    doi_data.push_mut(cr("doi").with_text(doi));
    doi_data.push_mut(cr("resource").with_text(resource));
    if let Some(coll) = build_tdm_resources(&tdm) {
        doi_data.push_mut(coll);
    }
    dataset.push_mut(doi_data);

    if let Some(cit) = build_citation_list(&citations) {
        dataset.push_mut(cit);
    }

    dataset
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Build the CrossRef deposit XML.
///
/// `json` is a JSON-serialised [`DepositInput`]. `timestamp` is a
/// `YYYYMMDDHHMMSS` string; `batch_id` is the unique submission id.
///
/// Returns a string byte-identical to the Python `build_deposit_xml`.
pub fn build_deposit_xml(
    json: &str,
    timestamp: &str,
    batch_id: &str,
) -> gmeow_errors::Result<String> {
    let input: DepositInput = serde_json::from_str(json).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: e.to_string(),
        })
    })?;
    let sd = &input.self_description;
    let config = &input.config;

    // `release_date` feeds every `build_date` element (publication_date /
    // update_date). Validate its `YYYY-MM-DD` shape ONCE here — where the function
    // already returns a `Result` — so a malformed date surfaces as a validation
    // error rather than panicking deep in node construction (no-optionality/hard-fail).
    validate_iso_date(&sd.release_date)?;

    let crossmark_policy: Option<String> = if config.crossmark_enabled {
        if config.crossmark_policy_doi.trim().is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::SelfDescription {
                detail: "CROSSMARK_POLICY_DOI must be non-empty when CROSSMARK_ENABLED is True."
                    .to_string(),
            }));
        }
        Some(config.crossmark_policy_doi.clone())
    } else {
        None
    };

    let has_version = sd.version_doi.is_some();

    // Concept relations
    let mut concept_relations = format_relations(&config.ontology_iri);
    if !sd.repo_url.is_empty() {
        concept_relations.push(Relation {
            kind: "inter_work_relation",
            rel_type: "isSupplementedBy".to_string(),
            identifier_type: "uri",
            target: sd.repo_url.clone(),
            description: "Source repository for the GMEOW ontology.".to_string(),
        });
    }
    if has_version && let Some(ref vdoi) = sd.version_doi {
        concept_relations.push(Relation {
            kind: "intra_work_relation",
            rel_type: "hasVersion".to_string(),
            identifier_type: "doi",
            target: vdoi.clone(),
            description: format!("Immutable version DOI for release {}.", sd.version),
        });
    }
    concept_relations.extend(alignment_relations(&config.alignment_targets));

    let concept_tdm = tdm_resources_for(&config.ontology_iri);
    let all_citations = alignment_citations(&config.alignment_targets);

    // Concept dataset
    let concept_dataset = build_dataset(
        sd,
        config,
        &sd.concept_doi,
        &config.ontology_iri,
        &format!("{} (concept)", sd.title),
        concept_relations,
        concept_tdm,
        all_citations,
        false,
        crossmark_policy.as_deref(),
    );

    // Version dataset (optional)
    let version_dataset: Option<Node> = if has_version {
        if let Some(ref vdoi) = sd.version_doi {
            let mut version_relations = format_relations(&sd.version_iri);
            version_relations.push(Relation {
                kind: "intra_work_relation",
                rel_type: "isVersionOf".to_string(),
                identifier_type: "doi",
                target: sd.concept_doi.clone(),
                description: "Concept DOI for the always-latest GMEOW ontology.".to_string(),
            });
            let version_tdm = tdm_resources_for(&sd.version_iri);
            let version_citations = alignment_citations(&config.alignment_targets);
            Some(build_dataset(
                sd,
                config,
                vdoi,
                &sd.version_iri,
                &format!("{} (version {})", sd.title, sd.version),
                version_relations,
                version_tdm,
                version_citations,
                true,
                crossmark_policy.as_deref(),
            ))
        } else {
            None
        }
    } else {
        None
    };

    // database_metadata
    let mut db_meta = cr("database_metadata").with_attr("language", "en");
    db_meta.push_mut(build_contributors(&sd.contributors));
    let mut db_titles = cr("titles");
    db_titles.push_mut(cr("title").with_text(&sd.title));
    db_meta.push_mut(db_titles);
    if !sd.description.is_empty() {
        db_meta.push_mut(cr("description").with_text(&sd.description));
    }
    db_meta.push_mut(build_date("publication_date", &sd.release_date));
    db_meta.push_mut(build_date("update_date", &sd.release_date));
    db_meta.push_mut(build_publisher(&sd.registrant, &config.registrant_place));

    // institution identifiers: wikidata only when present
    let inst_ids: Vec<(&str, &str)> = if let Some(ref wikidata) = sd.registrant_wikidata {
        vec![("wikidata", wikidata.as_str())]
    } else {
        vec![]
    };
    db_meta.push_mut(build_institution(
        &sd.registrant,
        &config.registrant_acronym,
        &config.registrant_place,
        &inst_ids,
    ));
    if let Some(pi) = build_publisher_item(&[("site", &config.dataset_slug)], &[]) {
        db_meta.push_mut(pi);
    }
    db_meta.push_mut(build_version_info(
        &sd.version,
        &format!("Release {} of the GMEOW ontology.", sd.version),
    ));

    // Assemble body / database
    let mut database = cr("database");
    database.push_mut(db_meta);
    database.push_mut(concept_dataset);
    if let Some(vds) = version_dataset {
        database.push_mut(vds);
    }
    let mut body = cr("body");
    body.push_mut(database);

    // head
    let mut head = cr("head");
    head.push_mut(cr("doi_batch_id").with_text(batch_id));
    head.push_mut(cr("timestamp").with_text(timestamp));
    let mut depositor = cr("depositor");
    depositor.push_mut(cr("depositor_name").with_text(&sd.depositor_name));
    depositor.push_mut(cr("email_address").with_text(&sd.depositor_email));
    head.push_mut(depositor);
    head.push_mut(cr("registrant").with_text(&sd.registrant));

    // Serialise — root element is written manually for exact namespace ordering.
    let schema_location = format!("{CR_NS} https://www.crossref.org/schemas/crossref5.4.0.xsd");
    let mut w = String::with_capacity(64 * 1024);
    w.push_str("<?xml version='1.0' encoding='utf-8'?>\n");
    w.push_str("<doi_batch");
    w.push_str(" xmlns=\"");
    w.push_str(CR_NS);
    w.push('"');
    w.push_str(" xmlns:ai=\"");
    w.push_str(AI_NS);
    w.push('"');
    w.push_str(" xmlns:rel=\"");
    w.push_str(REL_NS);
    w.push('"');
    w.push_str(" xmlns:xsi=\"");
    w.push_str(XSI_NS);
    w.push('"');
    w.push_str(" xsi:schemaLocation=\"");
    w.push_str(&esc_attr(&schema_location));
    w.push('"');
    w.push_str(" version=\"5.4.0\">");
    // head and body as children at depth 1
    w.push('\n');
    write_node(&head, &mut w, 1);
    w.push('\n');
    write_node(&body, &mut w, 1);
    w.push_str("\n</doi_batch>");

    Ok(w)
}

// ─────────────────────────────────────────────────────────────────────────────
// doi-lint
// ─────────────────────────────────────────────────────────────────────────────

/// Return DOI consistency problems, or `[]` if the deposit is sound.
///
/// `json` is a JSON-serialised [`LintInput`].  The Python caller reads the
/// CITATION.cff and ontology files and passes their text in, so this function
/// never does I/O.
pub fn lint_deposit(json: &str) -> gmeow_errors::Result<Vec<String>> {
    let input: LintInput = serde_json::from_str(json).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: e.to_string(),
        })
    })?;
    let sd = &input.self_description;
    let config = &input.config;
    let mut problems: Vec<String> = vec![];

    // (a) placeholder checks
    if sd.concept_doi.contains(PLACEHOLDER_DOI_PREFIX) {
        problems.push(format!(
            "concept DOI is still a placeholder: {}",
            py_repr(&sd.concept_doi)
        ));
    }
    if let Some(ref vdoi) = sd.version_doi
        && vdoi.contains(PLACEHOLDER_DOI_PREFIX)
    {
        problems.push(format!(
            "version DOI is still a placeholder: {}",
            py_repr(vdoi)
        ));
    }

    if let Some(ref cff) = input.citation_cff {
        if cff.contains(PLACEHOLDER_DOI_PREFIX) {
            problems.push("CITATION.cff still contains a placeholder DOI".to_string());
        }
        if !cff.contains(&sd.concept_doi) {
            problems.push(format!(
                "CITATION.cff does not reference the concept DOI {}",
                py_repr(&sd.concept_doi)
            ));
        }
    }

    if let Some(ref ontology) = input.ontology_ttl {
        if ontology.contains(PLACEHOLDER_DOI_PREFIX) {
            problems.push("ontology/gmeow.ttl still contains a placeholder DOI".to_string());
        }
        if !ontology.contains(&sd.concept_doi) {
            problems.push(format!(
                "ontology/gmeow.ttl does not carry the concept DOI {}",
                py_repr(&sd.concept_doi)
            ));
        }
    }

    // (c) concept resource must not be version-pinned
    let last_seg = config.ontology_iri.rsplit('/').next().unwrap_or("");
    if last_seg.starts_with(|c: char| c.is_ascii_digit()) {
        problems.push(format!(
            "concept resource IRI looks version-pinned: {}",
            py_repr(&config.ontology_iri)
        ));
    }
    if sd.version_doi.is_some() && !sd.version_iri.starts_with(&config.ontology_iri) {
        problems.push(format!(
            "version IRI {} is not under the concept IRI",
            py_repr(&sd.version_iri)
        ));
    }

    // (e) maximal-schema invariants
    if sd.license_uri.is_empty() {
        problems.push("self-description carries no dcterms:license for ai:program".to_string());
    }
    if sd.contributors.is_empty() {
        problems.push("self-description carries no author contributors".to_string());
    }
    if sd.registrant_wikidata.is_none() {
        problems.push("self-description carries no Wikidata authority for registrant".to_string());
    }

    // (b) round-trip: render the deposit natively from the input metadata and parse pairs.
    let deposit_input = DepositInput {
        self_description: sd.clone(),
        config: config.clone(),
    };
    let deposit_json = serde_json::to_string(&deposit_input).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Serialize {
            detail: e.to_string(),
        })
    })?;
    let xml: String = build_deposit_xml(&deposit_json, "00000000000000", "lint-roundtrip")?;

    let pairs = extract_doi_resource_pairs(&xml);
    let mut expected: BTreeSet<(String, String)> = BTreeSet::new();
    expected.insert((sd.concept_doi.clone(), config.ontology_iri.clone()));
    if let Some(ref vdoi) = sd.version_doi {
        expected.insert((vdoi.clone(), sd.version_iri.clone()));
    }
    let pairs_set: BTreeSet<(String, String)> = pairs.into_iter().collect();
    if pairs_set != expected {
        let pairs_sorted: Vec<_> = pairs_set.into_iter().collect();
        let expected_sorted: Vec<_> = expected.into_iter().collect();
        problems.push(format!(
            "deposit (doi, resource) pairs {pairs_sorted:?} do not match self-description {expected_sorted:?}"
        ));
    }

    // TDM collection
    if !xml.contains("property=\"text-mining\"") {
        problems.push("deposit carries no text-mining URL collection".to_string());
    }

    // Citation list
    if !xml.contains("<citation_list>") {
        problems.push("deposit carries no citation_list references".to_string());
    }

    // Duplicate citation keys per dataset
    check_duplicate_keys(&xml, &mut problems);

    // Citation business-rule checks (doi or unstructured_citation required;
    // no partial journal-article citations)
    check_citation_business_rules(&xml, &mut problems);

    Ok(problems)
}

// ─────────────────────────────────────────────────────────────────────────────
// XML parsing helpers (string scanning, no full parser needed)
// ─────────────────────────────────────────────────────────────────────────────

fn extract_doi_resource_pairs(xml: &str) -> Vec<(String, String)> {
    let mut pairs = vec![];
    let mut rest = xml;
    while let Some(start) = rest.find("<doi_data>") {
        rest = &rest[start + "<doi_data>".len()..];
        if let Some(end) = rest.find("</doi_data>") {
            let block = &rest[..end];
            let doi = extract_tag_text(block, "doi");
            let resource = extract_tag_text(block, "resource");
            if let (Some(d), Some(r)) = (doi, resource) {
                pairs.push((d, r));
            }
            rest = &rest[end + "</doi_data>".len()..];
        } else {
            break;
        }
    }
    pairs
}

fn extract_tag_text(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&close)?;
    Some(s[start..start + end].to_string())
}

fn check_duplicate_keys(xml: &str, problems: &mut Vec<String>) {
    let mut rest = xml;
    let mut ds_index = 1usize;
    while let Some(ds_start) = rest.find("<dataset ") {
        rest = &rest[ds_start..];
        if let Some(ds_end) = rest.find("</dataset>") {
            let block = &rest[..ds_end + "</dataset>".len()];

            // Extract DOI for error message
            let doi_in_block = block.find("<doi_data>").and_then(|p| {
                let doi_block = &block[p + "<doi_data>".len()..];
                extract_tag_text(doi_block, "doi")
            });

            if let Some(cl_start) = block.find("<citation_list>") {
                let cl_rest = &block[cl_start + "<citation_list>".len()..];
                if let Some(cl_end) = cl_rest.find("</citation_list>") {
                    let cl_block = &cl_rest[..cl_end];
                    let mut keys: Vec<String> = vec![];
                    let mut scan = cl_block;
                    while let Some(kp) = scan.find("key=\"") {
                        let after = &scan[kp + 5..];
                        if let Some(qe) = after.find('"') {
                            keys.push(after[..qe].to_string());
                        }
                        scan = &scan[kp + 5..];
                    }
                    let unique: HashSet<_> = keys.iter().cloned().collect();
                    if keys.len() != unique.len() {
                        let location = doi_in_block
                            .map(|d| format!("dataset DOI {d}"))
                            .unwrap_or_else(|| format!("dataset #{ds_index}"));
                        problems.push(format!(
                            "deposit citation_list for {location} contains duplicate citation keys"
                        ));
                    }
                }
            }

            rest = &rest[ds_end + "</dataset>".len()..];
            ds_index += 1;
        } else {
            break;
        }
    }
}

fn check_citation_business_rules(xml: &str, problems: &mut Vec<String>) {
    let mut rest = xml;
    while let Some(cit_start) = rest.find("<citation ") {
        rest = &rest[cit_start..];
        if let Some(cit_end) = rest.find("</citation>") {
            let block = &rest[..cit_end + "</citation>".len()];

            // Extract this citation's key — scoped to `block`, NOT `rest`: a
            // (currently impossible) keyless citation must not borrow a LATER
            // citation's key from the remaining XML and mis-attribute the problem.
            let key = block
                .find("key=\"")
                .and_then(|kp| {
                    let after = &block[kp + 5..];
                    after.find('"').map(|qe| after[..qe].to_string())
                })
                .unwrap_or_default();

            let has_doi = block.contains("<doi>");
            let has_unstructured = block.contains("<unstructured_citation>");
            if !has_doi && !has_unstructured {
                problems.push(format!(
                    "citation {key:?} has neither a doi nor an \
                     unstructured_citation (Crossref citation business rule)"
                ));
            }

            let has_author = block.contains("<author>");
            let has_first_page = block.contains("<first_page>");
            let article_shaped =
                block.contains("<journal_title>") || block.contains("<article_title>");
            if article_shaped && !has_author && !has_first_page {
                problems.push(format!(
                    "citation {key:?} carries journal_title/article_title without \
                     an author or first_page (Crossref rejects this shape)"
                ));
            }

            rest = &rest[cit_end + "</citation>".len()..];
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod date_tests {
    use super::validate_iso_date;

    #[test]
    fn accepts_well_formed_iso_date() {
        assert!(validate_iso_date("2026-06-21").is_ok());
    }

    #[test]
    fn rejects_malformed_dates() {
        // A malformed release_date must surface as an Err, never an out-of-range panic.
        for bad in ["2026", "2026-06", "not-a-date", "2026/06/21", "26-6-1", ""] {
            let err = validate_iso_date(bad).expect_err("malformed date must be rejected");
            let message = err.message();
            assert!(message.contains("YYYY-MM-DD"), "{message}");
        }
    }
}

#[cfg(test)]
mod lint_tests {
    use super::check_duplicate_keys;

    #[test]
    fn detects_duplicate_citation_keys_in_dataset() {
        let xml = r#"
<dataset dataset_type="record">
  <doi_data>
    <doi>10.67342/26w4o</doi>
    <resource>https://example.invalid/</resource>
  </doi_data>
  <citation_list>
    <citation key="ref-dup" type="web_resource">
      <unstructured_citation>First duplicate.</unstructured_citation>
    </citation>
    <citation key="ref-dup" type="web_resource">
      <unstructured_citation>Second duplicate.</unstructured_citation>
    </citation>
  </citation_list>
</dataset>
"#;
        let mut problems: Vec<String> = vec![];
        check_duplicate_keys(xml, &mut problems);
        assert_eq!(
            problems.len(),
            1,
            "expected exactly one problem, got: {:?}",
            problems
        );
        assert!(
            problems[0].contains("duplicate citation keys"),
            "unexpected message: {}",
            problems[0]
        );
        assert!(
            problems[0].contains("10.67342/26w4o"),
            "message should name the dataset DOI: {}",
            problems[0]
        );
    }

    #[test]
    fn no_false_positive_for_unique_citation_keys() {
        let xml = r#"
<dataset dataset_type="record">
  <doi_data>
    <doi>10.67342/26w4o</doi>
    <resource>https://example.invalid/</resource>
  </doi_data>
  <citation_list>
    <citation key="ref-a" type="web_resource">
      <unstructured_citation>First.</unstructured_citation>
    </citation>
    <citation key="ref-b" type="web_resource">
      <unstructured_citation>Second.</unstructured_citation>
    </citation>
  </citation_list>
</dataset>
"#;
        let mut problems: Vec<String> = vec![];
        check_duplicate_keys(xml, &mut problems);
        assert!(
            problems.is_empty(),
            "expected no problems for unique keys, got: {:?}",
            problems
        );
    }
}
