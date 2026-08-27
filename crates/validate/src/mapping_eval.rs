// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Wikidata and Dublin Core mapping evaluators.
//!
//! The public command surface remains Python (`gmeow-dev`), but the evaluator
//! authority lives here: QID/PID syntax, mapping-IRI namespace misuse, offline
//! Wikidata/DC coverage reports, and the maintainer-only live Wikidata existence
//! check.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gmeow_errors::{Finding, Location, Report, Severity};
use purrdf::{DatasetView, GraphMatch, TermRef, TermValue};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::model::{owl, rdf};
use crate::store;

const WD_NS: &str = "http://www.wikidata.org/entity/";
const WDT_NS: &str = "http://www.wikidata.org/prop/direct/";
const WD_HTTPS_NS: &str = "https://www.wikidata.org/entity/";
const WDT_HTTPS_NS: &str = "https://www.wikidata.org/prop/direct/";
const DCTERMS_NS: &str = "http://purl.org/dc/terms/";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const DCMITYPE_NS: &str = "http://purl.org/dc/dcmitype/";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
const WIKIDATA_API: &str = "https://www.wikidata.org/w/api.php";
const WIKIDATA_MAX_IDS_PER_REQUEST: usize = 50;
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

const EXPECTED_DC: &[&str] = &[
    "title",
    "creator",
    "contributor",
    "subject",
    "description",
    "publisher",
    "date",
    "type",
    "format",
    "identifier",
    "source",
    "language",
    "relation",
    "coverage",
    "rights",
];

const EXPECTED_DCTERMS: &[&str] = &[
    "title",
    "creator",
    "contributor",
    "subject",
    "description",
    "publisher",
    "date",
    "type",
    "format",
    "identifier",
    "source",
    "language",
    "relation",
    "coverage",
    "rights",
    "created",
    "modified",
    "issued",
    "valid",
    "available",
    "dateAccepted",
    "dateCopyrighted",
    "dateSubmitted",
    "abstract",
    "tableOfContents",
    "references",
    "isReferencedBy",
    "requires",
    "isRequiredBy",
    "replaces",
    "isReplacedBy",
    "hasPart",
    "isPartOf",
    "hasVersion",
    "isVersionOf",
    "conformsTo",
    "license",
    "rightsHolder",
    "accessRights",
    "spatial",
    "temporal",
    "bibliographicCitation",
    "extent",
    "medium",
    "audience",
    "provenance",
];

const EXPECTED_DCMITYPE: &[&str] = &[
    "Collection",
    "Dataset",
    "Event",
    "Image",
    "MovingImage",
    "PhysicalObject",
    "Service",
    "Software",
    "Sound",
    "StillImage",
    "Text",
    "InteractiveResource",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceMisuse {
    WdPropShouldBeWdt,
    WdtItemShouldBeWd,
    HttpsUrlShouldBeCurie,
    BadSyntax,
}

impl NamespaceMisuse {
    pub fn as_str(self) -> &'static str {
        match self {
            NamespaceMisuse::WdPropShouldBeWdt => "wd-prop-should-be-wdt",
            NamespaceMisuse::WdtItemShouldBeWd => "wdt-item-should-be-wd",
            NamespaceMisuse::HttpsUrlShouldBeCurie => "https-url-should-be-curie",
            NamespaceMisuse::BadSyntax => "bad-syntax",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misuse {
    pub local_id: String,
    pub kind: NamespaceMisuse,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyntaxReport {
    pub valid: Vec<String>,
    pub invalid: Vec<String>,
    pub misuses: Vec<Misuse>,
}

impl SyntaxReport {
    pub fn ok(&self) -> bool {
        self.invalid.is_empty() && self.misuses.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistenceStatus {
    Ok,
    Missing,
    Redirect,
    BadSyntax,
}

impl ExistenceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ExistenceStatus::Ok => "ok",
            ExistenceStatus::Missing => "missing",
            ExistenceStatus::Redirect => "redirect",
            ExistenceStatus::BadSyntax => "bad-syntax",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MappingRow {
    subject_id: String,
    predicate_id: String,
    object_id: String,
    object_label: String,
    confidence: Option<f64>,
    source_stem: String,
    subject_iri: String,
    object_iri: String,
}

#[derive(Debug, Default)]
struct OntologyTerms {
    classes: BTreeSet<String>,
    properties: BTreeSet<String>,
    individuals: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WikidataCoverageReport {
    pub total_classes: usize,
    pub total_properties: usize,
    pub total_individuals: usize,
    pub mapped_classes: BTreeSet<String>,
    pub mapped_properties: BTreeSet<String>,
    pub mapped_individuals: BTreeSet<String>,
    pub all_classes: BTreeSet<String>,
    pub all_properties: BTreeSet<String>,
    pub all_individuals: BTreeSet<String>,
    pub domain_counts: BTreeMap<String, DomainCounts>,
    pub predicate_counts: BTreeMap<String, usize>,
    pub low_confidence: Vec<MappingFinding>,
    pub missing_labels: Vec<MissingLabel>,
    pub threshold: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DcCoverageReport {
    pub total_dcterms: usize,
    pub total_dc: usize,
    pub total_dcmitype: usize,
    pub mapped_dcterms: BTreeSet<String>,
    pub mapped_dc: BTreeSet<String>,
    pub mapped_dcmitype: BTreeSet<String>,
    pub domain_counts: BTreeMap<String, DomainCounts>,
    pub predicate_counts: BTreeMap<String, usize>,
    pub low_confidence: Vec<MappingFinding>,
    pub fallback_confidences: usize,
    pub threshold: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DomainCounts {
    pub total: usize,
    #[serde(rename = "exactMatch")]
    pub exact_match: usize,
    #[serde(rename = "closeMatch")]
    pub close_match: usize,
    #[serde(rename = "relatedMatch")]
    pub related_match: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingFinding {
    pub subject: String,
    pub object: String,
    pub predicate: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingLabel {
    pub subject: String,
    pub object: String,
}

fn is_valid_qid(identifier: &str) -> bool {
    let Some(rest) = identifier.strip_prefix('Q') else {
        return false;
    };
    valid_non_zero_integer(rest)
}

fn is_valid_pid(identifier: &str) -> bool {
    let Some(rest) = identifier.strip_prefix('P') else {
        return false;
    };
    valid_non_zero_integer(rest)
}

fn valid_non_zero_integer(rest: &str) -> bool {
    !rest.is_empty() && !rest.starts_with('0') && rest.as_bytes().iter().all(u8::is_ascii_digit)
}

pub fn is_valid_id(identifier: &str) -> bool {
    is_valid_qid(identifier) || is_valid_pid(identifier)
}

/// A Wikidata **lexeme** id: `L` followed by a non-zero, non-leading-zero integer (`L7`,
/// `L1570700`). A malformed lexeme id (`L`, `L0`, `L01`, `L7x`) is rejected exactly like a
/// malformed QID.
fn is_valid_lexeme_id(identifier: &str) -> bool {
    let Some(rest) = identifier.strip_prefix('L') else {
        return false;
    };
    valid_non_zero_integer(rest)
}

/// A Wikidata **sense** id: a valid lexeme id, then `-S`, then a non-zero sense ordinal
/// (`L7-S1`). A QID or PID with a sense suffix (`Q42-S3`) is rejected — a sense hangs off a
/// lexeme, never an item or property.
fn is_valid_sense_id(identifier: &str) -> bool {
    let Some((lexeme, sense)) = identifier.split_once("-S") else {
        return false;
    };
    is_valid_lexeme_id(lexeme) && valid_non_zero_integer(sense)
}

/// Whether `identifier` is a well-formed Wikidata **entity** id of any kind an alignment object
/// may name: an item (`Q…`), a property (`P…`), a lexeme (`L…`), or a sense (`L…-S…`). This is
/// the SYNTAX gate, distinct from `is_valid_id` (the item/property queryability filter for the
/// live existence check).
pub fn is_valid_entity_id(identifier: &str) -> bool {
    is_valid_id(identifier) || is_valid_lexeme_id(identifier) || is_valid_sense_id(identifier)
}

pub fn local_name(iri: &str) -> Option<&str> {
    iri.strip_prefix(WD_NS)
}

pub fn local_name_wdt(iri: &str) -> Option<&str> {
    iri.strip_prefix(WDT_NS)
}

pub fn check_syntax(identifiers: &[String]) -> SyntaxReport {
    let mut report = SyntaxReport::default();
    for identifier in identifiers {
        if is_valid_entity_id(identifier) {
            report.valid.push(identifier.clone());
        } else {
            report.invalid.push(identifier.clone());
        }
    }
    report
}

pub fn check_syntax_iri(iri: &str, in_object_position: bool) -> Vec<Misuse> {
    if let Some(local) = iri.strip_prefix(WD_HTTPS_NS) {
        if is_valid_entity_id(local) {
            return vec![Misuse {
                local_id: local.to_owned(),
                kind: NamespaceMisuse::HttpsUrlShouldBeCurie,
                message: format!("{iri} should be written as wd:{local}"),
            }];
        }
        return vec![Misuse {
            local_id: local.to_owned(),
            kind: NamespaceMisuse::BadSyntax,
            message: format!("malformed identifier in HTTPS URL: {iri}"),
        }];
    }

    if let Some(local) = iri.strip_prefix(WDT_HTTPS_NS) {
        if is_valid_id(local) {
            return vec![Misuse {
                local_id: local.to_owned(),
                kind: NamespaceMisuse::HttpsUrlShouldBeCurie,
                message: format!("{iri} should be written as wdt:{local}"),
            }];
        }
        return vec![Misuse {
            local_id: local.to_owned(),
            kind: NamespaceMisuse::BadSyntax,
            message: format!("malformed identifier in HTTPS URL: {iri}"),
        }];
    }

    if let Some(local) = iri.strip_prefix(WD_NS) {
        if !is_valid_entity_id(local) {
            return vec![Misuse {
                local_id: local.to_owned(),
                kind: NamespaceMisuse::BadSyntax,
                message: format!("malformed wd: identifier: {local}"),
            }];
        }
        if local.starts_with('P') && !in_object_position {
            return vec![Misuse {
                local_id: local.to_owned(),
                kind: NamespaceMisuse::WdPropShouldBeWdt,
                message: format!(
                    "wd:{local} is a property entity; use wdt:{local} for direct-claim property mappings"
                ),
            }];
        }
        return Vec::new();
    }

    if let Some(local) = iri.strip_prefix(WDT_NS)
        && !is_valid_pid(local)
    {
        let (kind, message) = if is_valid_qid(local) {
            (
                NamespaceMisuse::WdtItemShouldBeWd,
                format!("wdt:{local} is an item ID; use wd:{local} for item mappings"),
            )
        } else {
            (
                NamespaceMisuse::BadSyntax,
                format!("malformed wdt: identifier: {local}"),
            )
        };
        return vec![Misuse {
            local_id: local.to_owned(),
            kind,
            message,
        }];
    }

    Vec::new()
}

pub fn wikidata_mapping_syntax(mappings_dir: &Path) -> gmeow_errors::Result<SyntaxReport> {
    let rows = load_mapping_rows(mappings_dir)?;
    let mut report = check_syntax(&collect_wikidata_ids_from_rows(&rows));
    for row in rows {
        report
            .misuses
            .extend(check_syntax_iri(&row.object_iri, true));
    }
    Ok(report)
}

pub fn wikidata_diagnostics(mappings_dir: &Path) -> gmeow_errors::Result<Report> {
    let syntax = wikidata_mapping_syntax(mappings_dir)?;
    let mut report = Report::new("wikidata");
    for identifier in syntax.invalid {
        let mut finding = Finding::new(
            Severity::Error,
            crate::codes::WIKIDATA_QID_SYNTAX,
            format!("invalid Wikidata identifier: {identifier}"),
        );
        finding.add_location(Location::new(None, None, None, Some(identifier)));
        report.add_finding(finding);
    }
    for misuse in syntax.misuses {
        let mut finding = Finding::new(
            Severity::Error,
            crate::codes::WIKIDATA_NAMESPACE_MISUSE,
            misuse.message,
        );
        finding.tags.push(misuse.kind.as_str().to_owned());
        finding.add_location(Location::new(None, None, None, Some(misuse.local_id)));
        report.add_finding(finding);
    }
    Ok(report)
}

pub fn wikidata_coverage(
    root: &Path,
    mappings_dir: &Path,
    threshold: f64,
) -> gmeow_errors::Result<WikidataCoverageReport> {
    let rows = load_mapping_rows(mappings_dir)?;
    let wd_rows: Vec<_> = rows
        .into_iter()
        .filter(|row| row.object_iri.starts_with(WD_NS) || row.object_iri.starts_with(WDT_NS))
        .collect();
    let terms = collect_ontology_terms(root)?;
    let mut report = WikidataCoverageReport {
        total_classes: terms.classes.len(),
        total_properties: terms.properties.len(),
        total_individuals: terms.individuals.len(),
        mapped_classes: BTreeSet::new(),
        mapped_properties: BTreeSet::new(),
        mapped_individuals: BTreeSet::new(),
        all_classes: terms.classes,
        all_properties: terms.properties,
        all_individuals: terms.individuals,
        domain_counts: BTreeMap::new(),
        predicate_counts: BTreeMap::new(),
        low_confidence: Vec::new(),
        missing_labels: Vec::new(),
        threshold,
    };

    for row in &wd_rows {
        if report.all_classes.contains(&row.subject_iri) {
            report.mapped_classes.insert(row.subject_iri.clone());
        } else if report.all_properties.contains(&row.subject_iri) {
            report.mapped_properties.insert(row.subject_iri.clone());
        } else if report.all_individuals.contains(&row.subject_iri) {
            report.mapped_individuals.insert(row.subject_iri.clone());
        }

        let confidence = row.confidence.unwrap_or(1.0);
        if confidence < threshold {
            report.low_confidence.push(MappingFinding {
                subject: row.subject_id.clone(),
                object: row.object_id.clone(),
                predicate: row.predicate_id.clone(),
                confidence,
            });
        }
        if row.object_label.trim().is_empty() {
            report.missing_labels.push(MissingLabel {
                subject: row.subject_id.clone(),
                object: row.object_id.clone(),
            });
        }

        *report
            .predicate_counts
            .entry(row.predicate_id.clone())
            .or_insert(0) += 1;
        add_domain_count(&mut report.domain_counts, row);
    }

    Ok(report)
}

pub fn dc_coverage(mappings_dir: &Path, threshold: f64) -> gmeow_errors::Result<DcCoverageReport> {
    let rows = load_mapping_rows(mappings_dir)?;
    let expected_dcterms = expected_set(DCTERMS_NS, EXPECTED_DCTERMS);
    let expected_dc = expected_set(DC_NS, EXPECTED_DC);
    let expected_dcmitype = expected_set(DCMITYPE_NS, EXPECTED_DCMITYPE);
    let dc_rows: Vec<_> = rows
        .into_iter()
        .filter(|row| dc_namespace(&row.object_iri).is_some())
        .collect();
    let mut report = DcCoverageReport {
        total_dcterms: expected_dcterms.len(),
        total_dc: expected_dc.len(),
        total_dcmitype: expected_dcmitype.len(),
        mapped_dcterms: BTreeSet::new(),
        mapped_dc: BTreeSet::new(),
        mapped_dcmitype: BTreeSet::new(),
        domain_counts: BTreeMap::new(),
        predicate_counts: BTreeMap::new(),
        low_confidence: Vec::new(),
        fallback_confidences: 0,
        threshold,
    };

    for row in &dc_rows {
        match expected_dc_namespace(
            &row.object_iri,
            &expected_dcterms,
            &expected_dc,
            &expected_dcmitype,
        ) {
            Some("dcterms") => {
                report.mapped_dcterms.insert(row.object_iri.clone());
            }
            Some("dc") => {
                report.mapped_dc.insert(row.object_iri.clone());
            }
            Some("dcmitype") => {
                report.mapped_dcmitype.insert(row.object_iri.clone());
            }
            _ => {}
        }

        let (confidence, fallback) = match row.confidence {
            Some(value) => (value, false),
            None => (0.0, true),
        };
        if fallback {
            report.fallback_confidences += 1;
        }
        if confidence < threshold {
            report.low_confidence.push(MappingFinding {
                subject: row.subject_id.clone(),
                object: row.object_id.clone(),
                predicate: row.predicate_id.clone(),
                confidence,
            });
        }

        *report
            .predicate_counts
            .entry(row.predicate_id.clone())
            .or_insert(0) += 1;
        add_domain_count(&mut report.domain_counts, row);
    }

    Ok(report)
}

pub fn render_wikidata_coverage(report: &WikidataCoverageReport, json_mode: bool) -> String {
    if json_mode {
        return serde_json::to_string_pretty(&serde_json::json!({
            "totals": {
                "classes": report.total_classes,
                "properties": report.total_properties,
                "individuals": report.total_individuals,
            },
            "mapped": {
                "classes": report.mapped_classes.len(),
                "properties": report.mapped_properties.len(),
                "individuals": report.mapped_individuals.len(),
            },
            "coverage": {
                "classes": round4(report.class_coverage()),
                "properties": round4(report.property_coverage()),
                "individuals": round4(report.individual_coverage()),
            },
            "domains": report.domain_counts,
            "predicates": report.predicate_counts,
            "low_confidence": report.low_confidence,
            "missing_labels": report.missing_labels,
            "gaps": {
                "classes": report.gap_classes(),
                "properties": report.gap_properties(),
                "individuals": report.gap_individuals(),
            },
        }))
        .expect("coverage report serializes");
    }

    let mut lines = Vec::new();
    lines.push("Wikidata Mapping Coverage".to_owned());
    lines.push("========================================".to_owned());
    lines.push(String::new());
    lines.push(format!(
        "classes      {:>4} / {:<4} ({:.0}%)",
        report.mapped_classes.len(),
        report.total_classes,
        report.class_coverage() * 100.0
    ));
    lines.push(format!(
        "properties   {:>4} / {:<4} ({:.0}%)",
        report.mapped_properties.len(),
        report.total_properties,
        report.property_coverage() * 100.0
    ));
    lines.push(format!(
        "individuals  {:>4} / {:<4} ({:.0}%)",
        report.mapped_individuals.len(),
        report.total_individuals,
        report.individual_coverage() * 100.0
    ));
    render_domain_and_predicates(&mut lines, &report.domain_counts, &report.predicate_counts);
    if !report.low_confidence.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Low confidence (< {}) - {} mappings",
            report.threshold,
            report.low_confidence.len()
        ));
        lines.push("--------------------".to_owned());
        for item in &report.low_confidence {
            lines.push(format!(
                "  {} -> {} ({}, {})",
                item.subject, item.object, item.predicate, item.confidence
            ));
        }
    }
    if !report.missing_labels.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Missing objectLabel - {} mappings",
            report.missing_labels.len()
        ));
        lines.push("--------------------".to_owned());
        for item in &report.missing_labels {
            lines.push(format!("  {} -> {}", item.subject, item.object));
        }
    }
    lines.join("\n")
}

pub fn render_dc_coverage(report: &DcCoverageReport, json_mode: bool) -> String {
    if json_mode {
        return serde_json::to_string_pretty(&serde_json::json!({
            "totals": {
                "dcterms": report.total_dcterms,
                "dc": report.total_dc,
                "dcmitype": report.total_dcmitype,
            },
            "mapped": {
                "dcterms": report.mapped_dcterms.len(),
                "dc": report.mapped_dc.len(),
                "dcmitype": report.mapped_dcmitype.len(),
            },
            "fallback_confidences": report.fallback_confidences,
            "coverage": {
                "dcterms": round4(report.dcterms_coverage()),
                "dcmitype": round4(report.dcmitype_coverage()),
            },
            "domains": report.domain_counts,
            "predicates": report.predicate_counts,
            "low_confidence": report.low_confidence,
            "gaps": {
                "dcterms": report.gap_dcterms(),
                "dcmitype": report.gap_dcmitype(),
            },
        }))
        .expect("coverage report serializes");
    }

    let mut lines = Vec::new();
    lines.push("Dublin Core Mapping Coverage".to_owned());
    lines.push("========================================".to_owned());
    lines.push(String::new());
    lines.push(format!(
        "dcterms      {:>4} / {:<4} ({:.0}%)",
        report.mapped_dcterms.len(),
        report.total_dcterms,
        report.dcterms_coverage() * 100.0
    ));
    lines.push(format!(
        "dc           {:>4} / {:<4} (derived dumb-down)",
        report.mapped_dc.len(),
        report.total_dc
    ));
    lines.push(format!(
        "dcmitype     {:>4} / {:<4} ({:.0}%)",
        report.mapped_dcmitype.len(),
        report.total_dcmitype,
        report.dcmitype_coverage() * 100.0
    ));
    render_domain_and_predicates(&mut lines, &report.domain_counts, &report.predicate_counts);
    if report.fallback_confidences > 0 {
        lines.push(String::new());
        lines.push(format!(
            "Fallback confidences (treated as 0.0): {}",
            report.fallback_confidences
        ));
    }
    if !report.low_confidence.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Low confidence (< {}) - {} mappings",
            report.threshold,
            report.low_confidence.len()
        ));
        lines.push("--------------------".to_owned());
        for item in &report.low_confidence {
            lines.push(format!(
                "  {} -> {} ({}, {})",
                item.subject, item.object, item.predicate, item.confidence
            ));
        }
    }
    let gaps_dcterms = report.gap_dcterms();
    if !gaps_dcterms.is_empty() {
        lines.push(String::new());
        lines.push(format!("Gaps - dcterms ({} unmapped)", gaps_dcterms.len()));
        lines.push("--------------------".to_owned());
        for term in gaps_dcterms {
            lines.push(format!("  {term}"));
        }
    }
    let gaps_dcmitype = report.gap_dcmitype();
    if !gaps_dcmitype.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Gaps - dcmitype ({} unmapped)",
            gaps_dcmitype.len()
        ));
        lines.push("--------------------".to_owned());
        for term in gaps_dcmitype {
            lines.push(format!("  {term}"));
        }
    }
    lines.join("\n")
}

pub fn check_existence(
    identifiers: &[String],
    project_root: &Path,
    timeout: Duration,
    chunk_size: usize,
    delay: Duration,
) -> gmeow_errors::Result<BTreeMap<String, ExistenceStatus>> {
    if !(1..=WIKIDATA_MAX_IDS_PER_REQUEST).contains(&chunk_size) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mapping {
            detail: format!("chunk_size must be between 1 and {WIKIDATA_MAX_IDS_PER_REQUEST}"),
        }));
    }
    let mut statuses = BTreeMap::new();
    let mut queryable = Vec::new();
    for identifier in identifiers {
        if is_valid_id(identifier) {
            queryable.push(identifier.clone());
        } else {
            statuses.insert(identifier.clone(), ExistenceStatus::BadSyntax);
        }
    }
    if queryable.is_empty() {
        return Ok(statuses);
    }

    for (chunk_index, chunk) in queryable.chunks(chunk_size).enumerate() {
        let payload = fetch_entities(chunk, project_root, timeout)?;
        let entities = wikidata_entities(&payload)?;
        for identifier in chunk {
            let entity = entities.get(identifier);
            let status = match entity {
                None => ExistenceStatus::Missing,
                Some(value) if value.get("missing").is_some() => ExistenceStatus::Missing,
                Some(value) if value.get("redirects").is_some() => ExistenceStatus::Redirect,
                Some(_) => ExistenceStatus::Ok,
            };
            statuses.insert(identifier.clone(), status);
        }
        if chunk_index + 1 < queryable.chunks(chunk_size).len() && !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
    Ok(statuses)
}

fn fetch_entities(
    chunk: &[String],
    project_root: &Path,
    timeout: Duration,
) -> gmeow_errors::Result<Value> {
    let key = cache_key(chunk);
    if let Some(cached) = load_cached(project_root, &key)? {
        return Ok(cached);
    }
    let ids = chunk.join("%7C");
    let url = format!(
        "{WIKIDATA_API}?action=wbgetentities&ids={ids}&props=info%7Clabels&format=json&languages=en&languagefallback=1"
    );
    let response = ureq::get(&url)
        .header("User-Agent", "gmeow-tools/0.1 (ontology mapping validator)")
        .config()
        .timeout_global(Some(timeout))
        .build()
        .call()
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mapping {
                detail: format!("Wikidata request failed: {e}"),
            })
        })?;
    let mut reader = response.into_body().into_reader();
    let mut body = String::new();
    reader.read_to_string(&mut body).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mapping {
            detail: format!("Wikidata response read failed: {e}"),
        })
    })?;
    let payload: Value = serde_json::from_str(&body).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("Wikidata JSON parse failed: {e}"),
        })
    })?;
    wikidata_entities(&payload)?;
    save_cached(project_root, &key, &payload)?;
    Ok(payload)
}

fn wikidata_entities(payload: &Value) -> gmeow_errors::Result<&serde_json::Map<String, Value>> {
    if let Some(error) = payload.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let info = error
            .get("info")
            .and_then(Value::as_str)
            .unwrap_or("no details");
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mapping {
            detail: format!("Wikidata API error {code}: {info}"),
        }));
    }
    if payload.get("success").and_then(Value::as_i64) != Some(1) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mapping {
            detail: "Wikidata response missing success=1".to_owned(),
        }));
    }
    payload
        .get("entities")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mapping {
                detail: "Wikidata response missing entities object".to_owned(),
            })
        })
}

fn cache_key(identifiers: &[String]) -> String {
    let mut sorted = identifiers.to_vec();
    sorted.sort();
    let mut hasher = Sha256::new();
    hasher.update(sorted.join("|").as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn cache_path(project_root: &Path, key: &str) -> PathBuf {
    project_root
        .join(".cache")
        .join("wikidata")
        .join(format!("{key}.json"))
}

fn load_cached(project_root: &Path, key: &str) -> gmeow_errors::Result<Option<Value>> {
    let path = cache_path(project_root, key);
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(&path).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("failed to read cache metadata {}: {e}", path.display()),
        })
    })?;
    let modified = metadata.modified().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("failed to read cache mtime {}: {e}", path.display()),
        })
    })?;
    if SystemTime::now()
        .duration_since(modified)
        .unwrap_or(DEFAULT_CACHE_TTL + Duration::from_secs(1))
        > DEFAULT_CACHE_TTL
    {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("failed to read cache {}: {e}", path.display()),
        })
    })?;
    match serde_json::from_str(&text) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}

fn save_cached(project_root: &Path, key: &str, payload: &Value) -> gmeow_errors::Result<()> {
    let path = cache_path(project_root, key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!("failed to create cache dir {}: {e}", parent.display()),
            })
        })?;
    }
    let text = serde_json::to_string(payload).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Serialize {
            detail: format!("cache JSON failed: {e}"),
        })
    })?;
    let mut file = fs::File::create(&path).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("failed to write cache {}: {e}", path.display()),
        })
    })?;
    file.write_all(text.as_bytes()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("failed to write cache {}: {e}", path.display()),
        })
    })
}

pub(crate) fn load_mapping_rows(mappings_dir: &Path) -> gmeow_errors::Result<Vec<MappingRow>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(mappings_dir).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!(
                "failed to read mappings dir {}: {e}",
                mappings_dir.display()
            ),
        })
    })? {
        let path = entry
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    detail: format!("failed to read mappings dir entry: {e}"),
                })
            })?
            .path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".sssom.tsv"))
        {
            paths.push(path);
        }
    }
    paths.sort();

    let mut rows = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!("failed to read {}: {e}", path.display()),
            })
        })?;
        let set = purrdf::sssom::parse_tsv(&text).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("failed to parse {}: {}", path.display(), e.message),
            })
        })?;
        let source_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mapping {
                    detail: format!("mapping path has no UTF-8 stem: {}", path.display()),
                })
            })?
            .to_owned();
        for mapping in set.mappings {
            let subject_iri = expand_entity(&mapping.subject_id, &set.meta.curie_map)?;
            let object_iri = expand_entity(&mapping.object_id, &set.meta.curie_map)?;
            rows.push(MappingRow {
                subject_id: mapping.subject_id,
                predicate_id: mapping.predicate_id,
                object_id: mapping.object_id,
                object_label: mapping.object_label.unwrap_or_default(),
                confidence: mapping.confidence,
                source_stem: source_stem.clone(),
                subject_iri,
                object_iri,
            });
        }
    }
    Ok(rows)
}

fn expand_entity(
    entity: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<String> {
    if entity.starts_with('<') && entity.ends_with('>') {
        return Ok(entity[1..entity.len() - 1].to_owned());
    }
    if entity.starts_with("http://")
        || entity.starts_with("https://")
        || entity.starts_with("urn:")
        || entity.starts_with("file:")
    {
        return Ok(entity.to_owned());
    }
    let Some((prefix, local)) = entity.split_once(':') else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mapping {
            detail: format!("not a CURIE: {entity:?}"),
        }));
    };
    let namespace = prefixes.get(prefix).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mapping {
            detail: format!("unknown prefix {prefix:?} in {entity:?}"),
        })
    })?;
    Ok(format!("{namespace}{local}"))
}

fn collect_wikidata_ids_from_rows(rows: &[MappingRow]) -> Vec<String> {
    let mut ids = Vec::new();
    for row in rows {
        if let Some(local) = local_name(&row.object_iri) {
            ids.push(local.to_owned());
        }
        if let Some(local) = local_name_wdt(&row.object_iri) {
            ids.push(local.to_owned());
        }
    }
    ids
}

pub fn collect_wikidata_ids(mappings_dir: &Path) -> gmeow_errors::Result<Vec<String>> {
    let rows = load_mapping_rows(mappings_dir)?;
    Ok(collect_wikidata_ids_from_rows(&rows))
}

/// Every external IRI GMEOW aligns to, from the SSSOM mappings (mirrors
/// `mappings.aligned_iris`).
///
/// Loads every `*.sssom.tsv` under `mappings_dir` via [`load_mapping_rows`] and
/// collects every IRI mentioned as a mapping *subject* or *object* (never a
/// predicate) into a sorted set, with CURIEs already expanded by `expand_entity`.
/// This is the alignment-graph walk `coverage::run_coverage` classifies against.
///
/// # Errors
///
/// Fails if the mappings dir cannot be read or a TSV fails to parse.
pub(crate) fn aligned_iris(mappings_dir: &Path) -> gmeow_errors::Result<BTreeSet<String>> {
    let rows = load_mapping_rows(mappings_dir)?;
    let mut iris: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        iris.insert(row.subject_iri);
        iris.insert(row.object_iri);
    }
    Ok(iris)
}

fn collect_ontology_terms(root: &Path) -> gmeow_errors::Result<OntologyTerms> {
    let paths = slice_module_files(root)?;
    let ds = store::dataset_from_paths(&paths)?;
    let mut terms = OntologyTerms::default();
    let Some(type_id) = ds.term_id_by_value(&TermValue::iri(rdf::TYPE)) else {
        return Ok(terms);
    };
    for q in ds.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
        let TermRef::Iri(subject) = ds.resolve(q.s) else {
            continue;
        };
        let TermRef::Iri(object) = ds.resolve(q.o) else {
            continue;
        };
        let iri = subject.to_owned();
        match object {
            value if value == owl::CLASS => {
                terms.classes.insert(iri);
            }
            value if value == owl::OBJECT_PROPERTY => {
                terms.properties.insert(iri);
            }
            value if value == owl::DATATYPE_PROPERTY => {
                terms.properties.insert(iri);
            }
            value if value == OWL_NAMED_INDIVIDUAL => {
                terms.individuals.insert(iri);
            }
            _ => {}
        }
    }
    Ok(terms)
}

fn slice_module_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let slices = root.join("slices");
    let mut paths = Vec::new();
    for group in fs::read_dir(&slices).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("failed to read slices dir {}: {e}", slices.display()),
        })
    })? {
        let group_path = group
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    detail: format!("failed to read slices group: {e}"),
                })
            })?
            .path();
        if !group_path.is_dir() {
            continue;
        }
        for slice in fs::read_dir(&group_path).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!("failed to read slice group {}: {e}", group_path.display()),
            })
        })? {
            let module = slice
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Io {
                        detail: format!("failed to read slice dir: {e}"),
                    })
                })?
                .path()
                .join("module.ttl");
            if module.exists() {
                paths.push(module);
            }
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("no slice module files found in {}", slices.display()),
        }));
    }
    Ok(paths)
}

fn add_domain_count(counts: &mut BTreeMap<String, DomainCounts>, row: &MappingRow) {
    let entry = counts.entry(row.source_stem.clone()).or_default();
    entry.total += 1;
    match row.predicate_id.as_str() {
        "skos:exactMatch" => entry.exact_match += 1,
        "skos:closeMatch" => entry.close_match += 1,
        "skos:relatedMatch" => entry.related_match += 1,
        _ => {}
    }
}

fn dc_namespace(iri: &str) -> Option<&'static str> {
    if iri.starts_with(DCTERMS_NS) {
        Some("dcterms")
    } else if iri.starts_with(DC_NS) {
        Some("dc")
    } else if iri.starts_with(DCMITYPE_NS) {
        Some("dcmitype")
    } else {
        None
    }
}

fn expected_dc_namespace(
    iri: &str,
    expected_dcterms: &BTreeSet<String>,
    expected_dc: &BTreeSet<String>,
    expected_dcmitype: &BTreeSet<String>,
) -> Option<&'static str> {
    match dc_namespace(iri) {
        Some("dcterms") if expected_dcterms.contains(iri) => Some("dcterms"),
        Some("dc") if expected_dc.contains(iri) => Some("dc"),
        Some("dcmitype") if expected_dcmitype.contains(iri) => Some("dcmitype"),
        _ => None,
    }
}

fn expected_set(namespace: &str, locals: &[&str]) -> BTreeSet<String> {
    locals
        .iter()
        .map(|local| format!("{namespace}{local}"))
        .collect()
}

fn render_domain_and_predicates(
    lines: &mut Vec<String>,
    domain_counts: &BTreeMap<String, DomainCounts>,
    predicate_counts: &BTreeMap<String, usize>,
) {
    lines.push(String::new());
    lines.push("By domain".to_owned());
    lines.push("--------------------".to_owned());
    for (domain, counts) in domain_counts {
        lines.push(format!(
            "  {domain:<40} total={:>3}  exact={:>3}  close={:>3}  related={:>3}",
            counts.total, counts.exact_match, counts.close_match, counts.related_match
        ));
    }
    lines.push(String::new());
    lines.push("By predicate".to_owned());
    lines.push("--------------------".to_owned());
    let mut predicate_items: Vec<_> = predicate_counts.iter().collect();
    predicate_items.sort_by(|(a_pred, a_count), (b_pred, b_count)| {
        b_count.cmp(a_count).then_with(|| a_pred.cmp(b_pred))
    });
    for (predicate, count) in predicate_items {
        lines.push(format!("  {predicate:<40} {count}"));
    }
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

impl WikidataCoverageReport {
    pub fn class_coverage(&self) -> f64 {
        ratio(self.mapped_classes.len(), self.total_classes)
    }

    pub fn property_coverage(&self) -> f64 {
        ratio(self.mapped_properties.len(), self.total_properties)
    }

    pub fn individual_coverage(&self) -> f64 {
        ratio(self.mapped_individuals.len(), self.total_individuals)
    }

    pub fn gap_classes(&self) -> Vec<String> {
        sorted_difference(&self.all_classes, &self.mapped_classes)
    }

    pub fn gap_properties(&self) -> Vec<String> {
        sorted_difference(&self.all_properties, &self.mapped_properties)
    }

    pub fn gap_individuals(&self) -> Vec<String> {
        sorted_difference(&self.all_individuals, &self.mapped_individuals)
    }
}

impl DcCoverageReport {
    pub fn dcterms_coverage(&self) -> f64 {
        ratio(self.mapped_dcterms.len(), self.total_dcterms)
    }

    pub fn dcmitype_coverage(&self) -> f64 {
        ratio(self.mapped_dcmitype.len(), self.total_dcmitype)
    }

    pub fn gap_dcterms(&self) -> Vec<String> {
        let expected = expected_set(DCTERMS_NS, EXPECTED_DCTERMS);
        sorted_difference(&expected, &self.mapped_dcterms)
    }

    pub fn gap_dcmitype(&self) -> Vec<String> {
        let expected = expected_set(DCMITYPE_NS, EXPECTED_DCMITYPE);
        sorted_difference(&expected, &self.mapped_dcmitype)
    }
}

fn ratio(mapped: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        mapped as f64 / total as f64
    }
}

fn sorted_difference(all: &BTreeSet<String>, mapped: &BTreeSet<String>) -> Vec<String> {
    all.difference(mapped).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_qids_and_pids() {
        for valid in ["Q1", "Q42", "P31"] {
            assert!(is_valid_id(valid), "{valid}");
        }
        for invalid in ["", "42", "Q", "Q0", "Q01", "Q12abc", "P0"] {
            assert!(!is_valid_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn syntax_iri_flags_namespace_misuse() {
        assert_eq!(
            check_syntax_iri("https://www.wikidata.org/entity/Q42", false)[0].kind,
            NamespaceMisuse::HttpsUrlShouldBeCurie
        );
        assert!(
            check_syntax_iri("https://www.wikidata.org/entity/P31", false)[0]
                .message
                .contains("wd:P31")
        );
        assert_eq!(
            check_syntax_iri("http://www.wikidata.org/entity/P31", false)[0].kind,
            NamespaceMisuse::WdPropShouldBeWdt
        );
        assert!(check_syntax_iri("http://www.wikidata.org/entity/P31", true).is_empty());
        assert_eq!(
            check_syntax_iri("http://www.wikidata.org/prop/direct/Q42", false)[0].kind,
            NamespaceMisuse::WdtItemShouldBeWd
        );
        assert_eq!(
            check_syntax_iri("http://www.wikidata.org/entity/Q0", false)[0].kind,
            NamespaceMisuse::BadSyntax
        );
        // HTTPS direct-property namespace: previously unrecognized (dropped); now flagged
        // with the wdt: CURIE suggestion, mirroring the HTTPS-entity branch.
        assert_eq!(
            check_syntax_iri("https://www.wikidata.org/prop/direct/P31", false)[0].kind,
            NamespaceMisuse::HttpsUrlShouldBeCurie
        );
        assert!(
            check_syntax_iri("https://www.wikidata.org/prop/direct/P31", false)[0]
                .message
                .contains("wdt:P31")
        );
        assert_eq!(
            check_syntax_iri("https://www.wikidata.org/prop/direct/P0", false)[0].kind,
            NamespaceMisuse::BadSyntax
        );
    }

    #[test]
    fn validates_lexeme_and_sense_ids() {
        // Well-formed lexeme ids and sense ids pass the entity-syntax gate…
        for valid in ["L7", "L1119", "L14462", "L7-S1", "L1570700-S2"] {
            assert!(
                is_valid_entity_id(valid),
                "{valid} should be a valid entity id"
            );
        }
        // …while malformed lexeme/sense ids are rejected exactly like a malformed QID.
        for invalid in [
            "L", "L0", "L01", "L7x", "L7-S", "L7-S0", "L7-Sx", "L-S1", "S1",
        ] {
            assert!(!is_valid_entity_id(invalid), "{invalid} must be rejected");
        }
        // The kinds are distinct: a lexeme id is not a QID/PID, and a QID/PID never carries a
        // sense suffix (a sense hangs off a lexeme, never an item or property).
        assert!(!is_valid_lexeme_id("Q7"));
        assert!(!is_valid_sense_id("Q42-S3"));
        assert!(!is_valid_sense_id("P31-S1"));
        assert!(is_valid_sense_id("L42-S3"));
    }

    #[test]
    fn syntax_iri_accepts_wd_lexeme_and_sense_ids_and_flags_malformed_ones() {
        // A well-formed lexeme id and sense id under the wd: namespace raise no misuse.
        assert!(check_syntax_iri("http://www.wikidata.org/entity/L7", true).is_empty());
        assert!(check_syntax_iri("http://www.wikidata.org/entity/L7-S1", true).is_empty());
        // A malformed lexeme/sense id is flagged BadSyntax, like a malformed QID.
        assert_eq!(
            check_syntax_iri("http://www.wikidata.org/entity/L0", true)[0].kind,
            NamespaceMisuse::BadSyntax
        );
        assert_eq!(
            check_syntax_iri("http://www.wikidata.org/entity/L7-S0", true)[0].kind,
            NamespaceMisuse::BadSyntax
        );
        // A QID with a sense suffix is not a real sense id — flagged BadSyntax.
        assert_eq!(
            check_syntax_iri("http://www.wikidata.org/entity/Q42-S3", true)[0].kind,
            NamespaceMisuse::BadSyntax
        );
        // The HTTPS entity namespace suggests the wd: CURIE for a lexeme id, mirroring QIDs.
        assert_eq!(
            check_syntax_iri("https://www.wikidata.org/entity/L7", true)[0].kind,
            NamespaceMisuse::HttpsUrlShouldBeCurie
        );
    }

    #[test]
    fn dc_expected_sets_are_counted_like_python_report() {
        assert_eq!(EXPECTED_DC.len(), 15);
        assert_eq!(EXPECTED_DCTERMS.len(), 46);
        assert_eq!(EXPECTED_DCMITYPE.len(), 12);
    }

    #[test]
    fn dc_expected_namespace_ignores_out_of_scope_terms() {
        let expected_dcterms = expected_set(DCTERMS_NS, EXPECTED_DCTERMS);
        let expected_dc = expected_set(DC_NS, EXPECTED_DC);
        let expected_dcmitype = expected_set(DCMITYPE_NS, EXPECTED_DCMITYPE);

        assert_eq!(
            expected_dc_namespace(
                "http://purl.org/dc/terms/title",
                &expected_dcterms,
                &expected_dc,
                &expected_dcmitype,
            ),
            Some("dcterms")
        );
        assert_eq!(
            expected_dc_namespace(
                "http://purl.org/dc/terms/notARealDctermsTerm",
                &expected_dcterms,
                &expected_dc,
                &expected_dcmitype,
            ),
            None
        );
        assert_eq!(
            expected_dc_namespace(
                "http://purl.org/dc/dcmitype/StillImage",
                &expected_dcterms,
                &expected_dc,
                &expected_dcmitype,
            ),
            Some("dcmitype")
        );
    }

    #[test]
    fn wikidata_entities_rejects_api_errors_and_malformed_payloads() {
        let error = serde_json::json!({
            "error": {
                "code": "bad-request",
                "info": "bad ids"
            }
        });
        let error_diag = wikidata_entities(&error).unwrap_err();
        assert!(error_diag.is::<crate::error::Mapping>());
        assert!(
            error_diag
                .message()
                .contains("Wikidata API error bad-request")
        );

        let missing_success = serde_json::json!({
            "entities": {}
        });
        assert!(
            wikidata_entities(&missing_success)
                .unwrap_err()
                .message()
                .contains("success=1")
        );

        let missing_entities = serde_json::json!({
            "success": 1
        });
        assert!(
            wikidata_entities(&missing_entities)
                .unwrap_err()
                .message()
                .contains("entities object")
        );

        let ok = serde_json::json!({
            "success": 1,
            "entities": {
                "Q42": {}
            }
        });
        assert!(wikidata_entities(&ok).unwrap().contains_key("Q42"));
    }

    #[test]
    fn check_existence_rejects_invalid_chunk_sizes() {
        let root = tempfile::tempdir().unwrap();
        let identifiers = vec!["Q42".to_owned()];
        let timeout = Duration::from_secs(1);
        let delay = Duration::ZERO;

        let zero = check_existence(&identifiers, root.path(), timeout, 0, delay).unwrap_err();
        assert!(zero.is::<crate::error::Mapping>());
        assert!(zero.message().contains("between 1 and 50"));
        let too_large = check_existence(&identifiers, root.path(), timeout, 51, delay).unwrap_err();
        assert!(too_large.message().contains("between 1 and 50"));
    }

    #[test]
    fn slice_module_discovery_fails_loudly_when_empty() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("slices")).unwrap();
        let err = slice_module_files(root.path()).unwrap_err();
        assert!(err.is::<crate::error::Io>());
        assert!(err.message().contains("no slice module files found"));
    }
}
