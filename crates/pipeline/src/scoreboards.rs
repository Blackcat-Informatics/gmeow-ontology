// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native evaluator scoreboards for repository gates (#946).
//!
//! The Python command surface remains a thin interface, but the claim-audit and
//! acceptance scoreboard authority lives here.  This module starts with the
//! claim audit: committed SPARQL gates, SHACL findings, flat claim JSON, and the
//! canonical diagnostics projection.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::transform::{self, CellInput};
use crate::up_projection;
use gmeow_diagnostics::{Finding, Location, Report, Severity};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Quad, Term};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use serde::Serialize;

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL: &str = "http://www.w3.org/2002/07/owl#";

const STRUCTURAL_NAMESPACES: &[&str] = &[GM, RDF_NS, RDFS, OWL];
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
const SKOS_BROAD_MATCH: &str = "http://www.w3.org/2004/02/skos/core#broadMatch";
const SKOS_NARROW_MATCH: &str = "http://www.w3.org/2004/02/skos/core#narrowMatch";
const SKOS_RELATED_MATCH: &str = "http://www.w3.org/2004/02/skos/core#relatedMatch";
const SKOS_MAPPING_RELATION: &str = "http://www.w3.org/2004/02/skos/core#mappingRelation";

const PREFIXES: &[(&str, &str)] = &[
    ("gmeow", GM),
    ("rdf", RDF_NS),
    ("rdfs", RDFS),
    ("owl", OWL),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ("schema", "https://schema.org/"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("vcard", "http://www.w3.org/2006/vcard/ns#"),
    ("vcardx", "http://www.w3.org/2006/vcard/ns#"),
    ("org", "http://www.w3.org/ns/org#"),
    ("time", "http://www.w3.org/2006/time#"),
    ("sioc", "http://rdfs.org/sioc/ns#"),
    ("bibo", "http://purl.org/ontology/bibo/"),
    ("bf", "http://id.loc.gov/ontologies/bibframe/"),
    ("bibframe", "http://id.loc.gov/ontologies/bibframe/"),
    ("gedcom", "http://www.w3.org/2000/10/swap/pim/gedcom#"),
    ("rel", "http://purl.org/vocab/relationship/"),
    ("cc", "http://creativecommons.org/ns#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("spdx", "http://spdx.org/rdf/terms#"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("geosparql", "http://www.opengis.net/ont/geosparql#"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ical", "http://www.w3.org/2002/12/cal/ical#"),
    ("oa", "http://www.w3.org/ns/oa#"),
    ("iiif", "http://iiif.io/api/presentation/3#"),
    ("exif", "http://www.w3.org/2003/12/exif/ns#"),
    ("wgs84", "http://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("mads", "http://www.loc.gov/mads/rdf/v1#"),
    ("codemeta", "https://codemeta.github.io/terms/"),
    ("ontolex", "https://www.w3.org/ns/lemon/ontolex#"),
    ("lime", "https://www.w3.org/ns/lemon/lime#"),
    ("bot", "https://w3id.org/bot#"),
    ("dcat", "http://www.w3.org/ns/dcat#"),
];

const ACCEPTANCE_PROFILES: &[&str] = &[
    "schema-org",
    "geosparql",
    "vcard",
    "foaf",
    "ical",
    "owl-time",
    "odrl",
    "cc",
    "dcterms",
    "oai_dc",
    "spdx",
    "ontolex",
    "web-annotation",
    "skos",
    "bot",
    "mailmap",
    "exif",
    "iiif",
    "dcat",
    "org",
    "bibo",
    "bibframe",
    "gedcom",
    "sioc",
    "doap",
    "codemeta",
    "prov",
];

const VENDORED_DEFS: &[&str] = &["foaf", "vcard", "org", "prov", "time", "geo", "ontolex"];
const EXTERNAL_LINKAGE_VOCABS: &[&str] = &["owl"];

const AUDIT_HEADLINE: &[&str] = &[
    "claims-without-evidence",
    "claims-contradicted-by-higher-confidence",
    "stale-source-claims",
];

const FLAT_QUERY: &str = r#"
PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdfs:  <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?claim ?text ?model ?confidence ?span ?chunk ?source ?start ?end ?polarity
WHERE {
    ?claim a gmeow:StandpointClaim ;
           gmeow:observationMethod gmeow:methodLlmExtraction .
    OPTIONAL { ?claim rdfs:label ?text . }
    OPTIONAL { ?claim gmeow:vantage ?model . }
    OPTIONAL { ?claim gmeow:confidence ?confidence . }
    OPTIONAL {
        ?claim gmeow:groundedIn ?span .
        OPTIONAL { ?span gmeow:spanOfChunk ?chunk .
                   OPTIONAL { ?chunk gmeow:chunkOf ?source . } }
        OPTIONAL { ?span gmeow:spanStart ?start . }
        OPTIONAL { ?span gmeow:spanEnd ?end . }
        OPTIONAL { ?span gmeow:supportPolarity ?polarity . }
    }
}
ORDER BY ?claim ?span
"#;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimAuditReport {
    pub findings: BTreeMap<String, Vec<Vec<String>>>,
    pub shacl_errors: Vec<String>,
    pub shacl_warnings: Vec<String>,
    pub claims: Vec<FlatClaim>,
}

impl ClaimAuditReport {
    pub fn flagged(&self) -> usize {
        AUDIT_HEADLINE
            .iter()
            .map(|name| self.findings.get(*name).map_or(0, Vec::len))
            .sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FlatClaim {
    pub claim: String,
    pub text: Option<String>,
    pub model: Option<String>,
    pub method: String,
    pub confidence: Option<String>,
    pub evidence: Vec<ClaimEvidence>,
    pub flags: ClaimFlags,
    pub contradicts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ClaimEvidence {
    pub span: String,
    pub chunk: Option<String>,
    pub source: Option<String>,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub polarity: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ClaimFlags {
    pub ungrounded: bool,
    pub contradicted: bool,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateResult {
    pub name: String,
    pub passed: bool,
    pub hard: bool,
    pub summary: String,
    pub metrics: BTreeMap<String, f64>,
    pub detail: Vec<String>,
}

impl GateResult {
    fn new(name: &str, passed: bool, hard: bool, summary: String) -> Self {
        Self {
            name: name.to_owned(),
            passed,
            hard,
            summary,
            metrics: BTreeMap::new(),
            detail: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileAcceptance {
    pub source: String,
    pub source_triples: usize,
    pub output_triples: usize,
    pub gates: Vec<GateResult>,
}

impl FileAcceptance {
    pub fn passed(&self) -> bool {
        self.gates.iter().filter(|g| g.hard).all(|g| g.passed)
    }
}

pub fn claim_audit(root: &Path, files: &[PathBuf]) -> Result<ClaimAuditReport, String> {
    if files.is_empty() {
        return Err("audit requires at least one Turtle data file".to_owned());
    }
    let mut sources = ontology_source_files(root, false)?;
    sources.extend(files.iter().cloned());
    let store = gmeow_validate::store::load_sources_into_store(&sources)?;

    let mut report = ClaimAuditReport::default();
    for query_path in audit_query_files(root)? {
        let name = query_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("invalid audit query filename: {}", query_path.display()))?
            .to_owned();
        let text = fs::read_to_string(&query_path)
            .map_err(|e| format!("failed to read {}: {e}", query_path.display()))?;
        report.findings.insert(name, select_rows(&store, &text)?);
    }

    let shacl = run_claim_shacl(root, &store)?;
    report.shacl_errors = shacl.0;
    report.shacl_warnings = shacl.1;
    report.claims = flat_claims(&store, &report)?;
    Ok(report)
}

pub fn render_claim_audit_text(report: &ClaimAuditReport) -> String {
    let mut lines = Vec::new();
    for name in AUDIT_HEADLINE {
        let rows = report.findings.get(*name).cloned().unwrap_or_default();
        lines.push(format!("{name}: {}", rows.len()));
        for row in rows {
            lines.push(format!(
                "  {}",
                row.iter().map(|v| local(v)).collect::<Vec<_>>().join(" | ")
            ));
        }
    }
    let coverage = report.findings.get("evidence-coverage").map_or(0, Vec::len);
    lines.push(format!("claims audited: {coverage}"));
    if !report.shacl_errors.is_empty() {
        lines.push(format!("SHACL errors: {}", report.shacl_errors.len()));
    }
    lines.push(format!("SHACL warnings: {}", report.shacl_warnings.len()));
    lines.join("\n")
}

pub fn render_claim_audit_json(report: &ClaimAuditReport) -> Result<String, String> {
    serde_json::to_string_pretty(&serde_json::json!({ "claims": report.claims }))
        .map_err(|e| format!("render claim audit JSON: {e}"))
}

pub fn claim_audit_diagnostics(report: &ClaimAuditReport) -> Report {
    let mut out = Report::new("audit");
    for (stem, suffix) in [
        ("claims-without-evidence", "ungrounded-claim"),
        (
            "claims-contradicted-by-higher-confidence",
            "contradicted-claim",
        ),
        ("stale-source-claims", "stale-source"),
    ] {
        for row in report.findings.get(stem).into_iter().flatten() {
            let subject = row.first().cloned().unwrap_or_default();
            let mut finding = Finding::new(
                Severity::Warning,
                format!("audit.{suffix}"),
                if row.is_empty() {
                    stem.to_owned()
                } else {
                    row.join(" | ")
                },
            )
            .with_tool("audit");
            if !subject.is_empty() {
                finding.add_location(Location {
                    logical: Some(subject),
                    ..Location::default()
                });
            }
            out.add_finding(finding);
        }
    }
    for message in &report.shacl_errors {
        out.add_finding(
            Finding::new(Severity::Error, "audit.shacl-error", message).with_tool("audit"),
        );
    }
    for message in &report.shacl_warnings {
        out.add_finding(
            Finding::new(Severity::Warning, "audit.shacl-warning", message).with_tool("audit"),
        );
    }
    out
}

pub fn run_acceptance(root: &Path, source: &Path, descend: bool) -> Result<FileAcceptance, String> {
    let source_store = gmeow_validate::store::load_sources_into_store(&[source.to_path_buf()])?;
    let source_nt = gmeow_validate::store::dump_store_to_ntriples(&source_store)
        .map_err(|e| format!("serialize source graph: {e}"))?;
    let ontology_nt = ontology_nt(root)?;
    let tag_map = gmeow_validate::language_tags::load_tag_map(ontology_nt.as_bytes(), "nt")?;
    let inverse_tag_map = invert_tag_map(&tag_map);

    let lift = up_projection::up_project_nt(
        &source_nt,
        &sssom_texts(root)?,
        &projection_ttls(root)?,
        &ontology_nt,
        descend,
    )?;
    if lift.graph_nt.trim().is_empty() {
        return Err(format!(
            "transpile: nothing lifted to GMEOW from {} — empty draft",
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("source")
        ));
    }
    let draft_nt = retag_nt_to_internal(&lift.graph_nt, &inverse_tag_map)?;
    let draft_store = gmeow_validate::store::build_store_from_nt(&draft_nt)?;
    let transformed = transform::transform_nt(
        &draft_nt,
        &ontology_nt,
        &load_cells(root)?,
        &denied_cells(root)?,
        &projection_queries(root)?,
    )?;
    let output_nt = retag_nt_to_public(&transformed.base_plus_derived_nt, &tag_map)?;
    let output_store = gmeow_validate::store::build_store_from_nt(&output_nt)?;

    let gates = vec![
        gate_pure_gmeow(&draft_store)?,
        gate_round_trip(&source_store, &output_store, &tag_map)?,
        gate_size_invariant(&source_store, &output_store)?,
        gate_external_validator(root, &output_store, &tag_map)?,
        gate_coverage(
            &source_store,
            &output_store,
            lift.lifted,
            lift.gap_terms.len(),
        )?,
    ];
    Ok(FileAcceptance {
        source: source
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("source")
            .to_owned(),
        source_triples: store_len(&source_store)?,
        output_triples: store_len(&output_store)?,
        gates,
    })
}

pub fn run_acceptance_corpus(
    root: &Path,
    source: Option<&Path>,
    descend: bool,
) -> Result<Vec<FileAcceptance>, String> {
    let sources = match source {
        Some(path) => vec![path.to_path_buf()],
        None => default_corpus(root)?,
    };
    if sources.is_empty() {
        return Err("no source given and no external/ snapshots found".to_owned());
    }
    sources
        .iter()
        .map(|path| run_acceptance(root, path, descend))
        .collect()
}

pub fn default_corpus(root: &Path) -> Result<Vec<PathBuf>, String> {
    glob_ttl(
        &root
            .join("tests")
            .join("fixtures")
            .join("coverage")
            .join("external"),
    )
}

pub fn corpus_recall_pct(results: &[FileAcceptance]) -> f64 {
    let mut total_recovered = 0.0;
    let mut total_addressable = 0.0;
    for file in results {
        if let Some(gate) = file.gates.iter().find(|g| g.name == "round-trip-superset") {
            total_recovered += gate.metrics.get("recovered").copied().unwrap_or(0.0);
            total_addressable += gate.metrics.get("addressable").copied().unwrap_or(0.0);
        }
    }
    if total_addressable == 0.0 {
        100.0
    } else {
        100.0 * total_recovered / total_addressable
    }
}

pub fn render_acceptance_report(results: &[FileAcceptance]) -> String {
    let mut lines = vec!["# Transpile acceptance — real-data scoreboard\n".to_owned()];
    for file in results {
        let verdict = if file.passed() {
            "✅ PASS"
        } else {
            "❌ FAIL"
        };
        lines.push(format!("## {} — {verdict}\n", file.source));
        lines.push(format!(
            "source {} triples → consumer output {} triples\n",
            file.source_triples, file.output_triples
        ));
        lines.push("| gate | kind | verdict | summary |".to_owned());
        lines.push("|---|---|---|---|".to_owned());
        for gate in &file.gates {
            let kind = if gate.hard { "hard" } else { "scoreboard" };
            let verdict = if gate.passed {
                "✅"
            } else if gate.hard {
                "❌"
            } else {
                "🔴"
            };
            lines.push(format!(
                "| {} | {kind} | {verdict} | {} |",
                gate.name, gate.summary
            ));
        }
        lines.push(String::new());
        for gate in &file.gates {
            if !gate.detail.is_empty() {
                lines.push(format!("<details><summary>{}</summary>\n", gate.name));
                lines.extend(gate.detail.iter().cloned());
                lines.push("\n</details>\n".to_owned());
            }
        }
    }
    lines.join("\n") + "\n"
}

pub fn acceptance_diagnostics(results: &[FileAcceptance]) -> Report {
    let mut out = Report::new("acceptance");
    for file in results {
        for gate in &file.gates {
            if gate.passed {
                continue;
            }
            let mut finding = Finding::new(
                if gate.hard {
                    Severity::Error
                } else {
                    Severity::Note
                },
                format!("acceptance.{}", gate.name),
                format!("{}: {}", file.source, gate.summary),
            )
            .with_tool("acceptance");
            finding.add_location(Location {
                path: Some(file.source.clone()),
                ..Location::default()
            });
            if !gate.detail.is_empty() {
                finding.detail = Some(gate.detail.join("\n"));
            }
            out.add_finding(finding);
        }
    }
    out
}

fn ontology_source_files(root: &Path, include_imports: bool) -> Result<Vec<PathBuf>, String> {
    let ontology = root.join("ontology").join("gmeow.ttl");
    if !ontology.exists() {
        return Err(format!("root ontology not found: {}", ontology.display()));
    }
    let mut files = vec![ontology];
    files.extend(slice_files(root, "module.ttl")?);
    if include_imports {
        let imports = root.join("imports");
        files.extend(glob_ttl(&imports)?);
    }
    Ok(files.into_iter().filter(|p| p.exists()).collect())
}

fn slice_files(root: &Path, leaf: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let slices = root.join("slices");
    for group in sorted_dirs(&slices)? {
        for slice in sorted_dirs(&group)? {
            let path = slice.join(leaf);
            if path.exists() {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn sorted_dirs(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut dirs = Vec::new();
    for entry in
        fs::read_dir(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("failed to stat {}: {e}", entry.path().display()))?;
        if file_type.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn glob_ttl(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in
        fs::read_dir(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn audit_query_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = root.join("queries").join("audit");
    let files = glob_ttl_like(&dir, "rq")?;
    if files.is_empty() {
        return Err(format!("no audit queries found under {}", dir.display()));
    }
    Ok(files)
}

fn glob_ttl_like(path: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn select_rows(store: &Store, query: &str) -> Result<Vec<Vec<String>>, String> {
    let parsed = SparqlEvaluator::new()
        .parse_query(query)
        .map_err(|e| format!("SPARQL query parse failed: {e}"))?;
    let QueryResults::Solutions(solutions) = parsed
        .on_store(store)
        .execute()
        .map_err(|e| format!("SPARQL query evaluation failed: {e}"))?
    else {
        return Err("audit query did not return SELECT solutions".to_owned());
    };
    let variables = solutions.variables().to_vec();
    let mut rows = Vec::new();
    for solution in solutions {
        let solution = solution.map_err(|e| format!("SPARQL solution failed: {e}"))?;
        rows.push(
            variables
                .iter()
                .map(|var| solution.get(var).map_or_else(String::new, term_value))
                .collect(),
        );
    }
    Ok(rows)
}

fn run_claim_shacl(root: &Path, store: &Store) -> Result<(Vec<String>, Vec<String>), String> {
    let data_nt = gmeow_validate::store::dump_store_to_ntriples(store)
        .map_err(|e| format!("serialize audit SHACL data: {e}"))?;
    let data_store = gmeow_validate::store::build_store_from_nt(&data_nt)?;
    let shapes_ttl = shapes_turtle(root)?;
    let shapes = gmeow_shacl::engine::parse_shapes(&shapes_ttl)?;
    let shacl = gmeow_shacl::engine::validate(&data_store, &shapes);
    if shacl.conforms {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    for result in shacl.results {
        let line = shacl_line(&result);
        match result.severity {
            gmeow_shacl::report::Severity::Violation => violations.push(line),
            gmeow_shacl::report::Severity::Warning | gmeow_shacl::report::Severity::Info => {
                warnings.push(line);
            }
        }
    }
    let errors = if violations.is_empty() {
        Vec::new()
    } else {
        vec![format!("SHACL violations:\n{}", violations.join("\n"))]
    };
    let warnings = if warnings.is_empty() {
        Vec::new()
    } else {
        vec![format!("SHACL warnings:\n{}", warnings.join("\n"))]
    };
    Ok((errors, warnings))
}

fn shapes_turtle(root: &Path) -> Result<String, String> {
    let shapes_dir = root.join("shapes");
    let base = shapes_dir.join("gmeow-shapes.ttl");
    if !base.exists() {
        return Err(format!("SHACL shapes not found: {}", base.display()));
    }
    let mut files = vec![base.clone()];
    let excluded = BTreeSet::from([
        "mapping-dsl-shapes.ttl",
        "statement-dsl-shapes.ttl",
        "test-dsl-shapes.ttl",
        "slice-manifest-shapes.ttl",
        "gmeow-shapes.ttl",
    ]);
    for path in glob_ttl(&shapes_dir)? {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if !excluded.contains(name) {
                files.push(path);
            }
        }
    }
    let generated = glob_ttl(&root.join("generated").join("shapes"))?;
    if generated.is_empty() {
        return Err(format!(
            "no generated shapes under {}",
            root.join("generated").join("shapes").display()
        ));
    }
    files.extend(generated);
    files.extend(slice_files(root, "shapes.ttl")?);
    // The importable named prefix set (`gmeow:CorePrefixes`) must live in the
    // shapes graph the reader parses so that `sh:prefixes gmeow:CorePrefixes`
    // references on production shapes RESOLVE (not just fall back to the
    // document's own `@prefix` lines). This is the §2 "generalize sh:declare"
    // dogfood: the set is consumed, proving it is importable (#1009 §2).
    // It is a generated artifact, so a missing file is a real pipeline error.
    let core_prefixes = root.join(crate::stages::mappings::CORE_PREFIXES_PATH);
    if !core_prefixes.exists() {
        return Err(format!(
            "core prefix set not found (run `make regenerate`): {}",
            core_prefixes.display()
        ));
    }
    files.push(core_prefixes);
    files
        .iter()
        .map(|path| {
            fs::read_to_string(path)
                .map_err(|e| format!("failed to read SHACL shapes {}: {e}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("\n"))
}

fn shacl_line(result: &gmeow_shacl::report::ValidationResult) -> String {
    let focus = shacl_term_to_str(&result.focus_node);
    match &result.message {
        Some(message) => format!("{focus}: {message}"),
        None => focus,
    }
}

fn shacl_term_to_str(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        Term::Literal(l) => l.value().to_owned(),
        Term::Triple(t) => t.to_string(),
    }
}

fn flat_claims(store: &Store, report: &ClaimAuditReport) -> Result<Vec<FlatClaim>, String> {
    let flagged: BTreeMap<&str, BTreeSet<String>> = AUDIT_HEADLINE
        .iter()
        .map(|name| {
            (
                *name,
                report
                    .findings
                    .get(*name)
                    .into_iter()
                    .flatten()
                    .filter_map(|row| row.first().cloned())
                    .collect(),
            )
        })
        .collect();

    let mut contradiction_members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in report.findings.get("contradictions").into_iter().flatten() {
        if row.len() >= 4 {
            contradiction_members
                .entry(row[0].clone())
                .or_default()
                .insert(row[3].clone());
        }
    }

    let rows = select_rows(store, FLAT_QUERY)?;
    let mut claims: BTreeMap<String, FlatClaim> = BTreeMap::new();
    for row in rows {
        if row.is_empty() || row[0].is_empty() {
            continue;
        }
        let claim = row[0].clone();
        let contradicts = contradiction_members
            .values()
            .filter(|members| members.contains(&claim))
            .flat_map(|members| members.iter().filter(|other| *other != &claim).cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let entry = claims.entry(claim.clone()).or_insert_with(|| FlatClaim {
            claim: claim.clone(),
            text: nonempty(row.get(1)),
            model: nonempty(row.get(2)),
            method: "llm-extraction".to_owned(),
            confidence: nonempty(row.get(3)),
            evidence: Vec::new(),
            flags: ClaimFlags {
                ungrounded: flagged["claims-without-evidence"].contains(&claim),
                contradicted: flagged["claims-contradicted-by-higher-confidence"].contains(&claim),
                stale: flagged["stale-source-claims"].contains(&claim),
            },
            contradicts,
        });
        if let Some(span) = nonempty(row.get(4)) {
            entry.evidence.push(ClaimEvidence {
                span,
                chunk: nonempty(row.get(5)),
                source: nonempty(row.get(6)),
                start: parse_optional_i64(row.get(7))?,
                end: parse_optional_i64(row.get(8))?,
                polarity: nonempty(row.get(9)).map(|p| local(&p).to_owned()),
            });
        }
    }
    Ok(claims.into_values().collect())
}

fn nonempty(value: Option<&String>) -> Option<String> {
    value.and_then(|v| if v.is_empty() { None } else { Some(v.clone()) })
}

fn parse_optional_i64(value: Option<&String>) -> Result<Option<i64>, String> {
    let Some(value) = nonempty(value) else {
        return Ok(None);
    };
    value
        .parse::<i64>()
        .map(Some)
        .map_err(|e| format!("invalid integer literal {value:?}: {e}"))
}

fn term_value(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        Term::Literal(l) => l.value().to_owned(),
        Term::Triple(t) => t.to_string(),
    }
}

fn local(value: &str) -> &str {
    value.strip_prefix(GM).unwrap_or(value)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RdfTripleKey(String, String, String);

fn ontology_nt(root: &Path) -> Result<String, String> {
    let sources = ontology_source_files(root, false)?;
    let store = gmeow_validate::store::load_sources_into_store(&sources)?;
    gmeow_validate::store::dump_store_to_ntriples(&store)
        .map_err(|e| format!("serialize ontology graph: {e}"))
}

fn sssom_texts(root: &Path) -> Result<Vec<String>, String> {
    let dir = root.join("generated").join("mappings");
    let paths = glob_suffix(&dir, ".sssom.tsv")?;
    if paths.is_empty() {
        return Err(format!("no generated SSSOM files under {}", dir.display()));
    }
    read_texts(&paths)
}

fn projection_ttls(root: &Path) -> Result<Vec<String>, String> {
    let mut paths = glob_ttl(&root.join("dsl").join("mappings").join("projections"))?;
    paths.extend(slice_mapping_files(root)?);
    if paths.is_empty() {
        return Err("no projection mapping TTL files found".to_owned());
    }
    read_texts(&paths)
}

fn projection_queries(root: &Path) -> Result<Vec<(String, String)>, String> {
    let dir = root.join("generated").join("queries");
    ACCEPTANCE_PROFILES
        .iter()
        .map(|profile| {
            let path = dir.join(format!("{profile}.rq"));
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("failed to read projection query {}: {e}", path.display()))?;
            Ok(((*profile).to_owned(), text))
        })
        .collect()
}

fn load_cells(root: &Path) -> Result<Vec<CellInput>, String> {
    let mut paths = glob_ttl(&root.join("dsl").join("mappings").join("equivalences"))?;
    paths.extend(slice_mapping_files(root)?);
    if paths.is_empty() {
        return Err("no mapping cell TTL files found".to_owned());
    }
    let store = gmeow_validate::store::load_sources_into_store(&paths)?;
    let rows = select_rows(
        &store,
        r#"
PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
SELECT ?cell ?subj ?pred ?obj ?confidence
WHERE {
    ?cell a gmeow:TermEquivalence ;
          gmeow:alignSubject ?subj ;
          gmeow:alignPredicate ?pred ;
          gmeow:alignObject ?obj .
    OPTIONAL { ?cell gmeow:confidence ?confidence . }
}
ORDER BY ?cell
"#,
    )?;
    let mut cells = Vec::new();
    for row in rows {
        if row.len() < 4
            || row[0].is_empty()
            || row[1].is_empty()
            || row[2].is_empty()
            || row[3].is_empty()
        {
            continue;
        }
        cells.push(CellInput {
            iri: row[0].clone(),
            subject: row[1].clone(),
            predicate_curie: predicate_curie(&row[2]),
            object: row[3].clone(),
            confidence: row.get(4).cloned().unwrap_or_default(),
        });
    }
    Ok(cells)
}

fn denied_cells(root: &Path) -> Result<Vec<(String, String, String)>, String> {
    const ALIGNMENT_CHECKS: &[&str] = &[
        "inverse-direction",
        "domain-range",
        "property-character",
        "equivalence-collapse",
        "dc-refinement",
        "dc-hand-authored",
    ];
    let findings = gmeow_slice::lint_projection(root, false).map_err(|e| e.to_string())?;
    let alignment = findings
        .into_iter()
        .filter(|f| ALIGNMENT_CHECKS.contains(&f.check.as_str()))
        .collect::<Vec<_>>();
    let collapses = alignment
        .iter()
        .filter(|f| f.severity == "ERROR" && f.check == "equivalence-collapse")
        .collect::<Vec<_>>();
    if !collapses.is_empty() {
        let details = collapses
            .iter()
            .take(3)
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "equivalence-collapse ERROR - transform refused: {details}"
        ));
    }
    Ok(alignment
        .into_iter()
        .filter(|f| f.severity == "ERROR")
        .filter_map(|f| Some((f.subject_id?, f.predicate_id?, f.object_id?)))
        .collect())
}

fn slice_mapping_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let slices = root.join("slices");
    for group in sorted_dirs(&slices)? {
        for slice in sorted_dirs(&group)? {
            out.extend(glob_ttl(&slice.join("mappings"))?);
        }
    }
    out.sort();
    Ok(out)
}

fn glob_suffix(path: &Path, suffix: &str) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in
        fs::read_dir(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn read_texts(paths: &[PathBuf]) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|path| {
            fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))
        })
        .collect()
}

fn predicate_curie(iri: &str) -> String {
    match iri {
        "http://www.w3.org/2002/07/owl#equivalentClass" => "owl:equivalentClass".to_owned(),
        "http://www.w3.org/2002/07/owl#equivalentProperty" => "owl:equivalentProperty".to_owned(),
        SKOS_EXACT_MATCH => "skos:exactMatch".to_owned(),
        SKOS_CLOSE_MATCH => "skos:closeMatch".to_owned(),
        SKOS_RELATED_MATCH => "skos:relatedMatch".to_owned(),
        SKOS_BROAD_MATCH => "skos:broadMatch".to_owned(),
        SKOS_NARROW_MATCH => "skos:narrowMatch".to_owned(),
        _ => curie(iri),
    }
}

fn curie(iri: &str) -> String {
    for (prefix, ns) in sorted_prefixes() {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_owned()
}

fn vocab_of_iri(iri: &str) -> Option<&'static str> {
    for (prefix, ns) in sorted_prefixes() {
        if iri.starts_with(ns) {
            return Some(prefix);
        }
    }
    None
}

fn sorted_prefixes() -> Vec<(&'static str, &'static str)> {
    let mut prefixes = PREFIXES.to_vec();
    prefixes.sort_by_key(|item| std::cmp::Reverse(item.1.len()));
    prefixes
}

fn invert_tag_map(tag_map: &HashMap<String, String>) -> HashMap<String, String> {
    tag_map
        .iter()
        .map(|(internal, bcp)| (bcp.to_ascii_lowercase(), internal.clone()))
        .collect()
}

fn retag_nt_to_internal(
    nt: &str,
    inverse_tag_map: &HashMap<String, String>,
) -> Result<String, String> {
    retag_nt(nt, |lang| {
        if gmeow_validate::language_tags::is_internal_tag(lang) {
            None
        } else {
            inverse_tag_map.get(&lang.to_ascii_lowercase()).cloned()
        }
    })
}

fn retag_nt_to_public(nt: &str, tag_map: &HashMap<String, String>) -> Result<String, String> {
    retag_nt(nt, |lang| {
        if gmeow_validate::language_tags::is_internal_tag(lang) {
            tag_map
                .get(lang)
                .or_else(|| tag_map.get(&lang.to_ascii_lowercase()))
                .cloned()
        } else {
            None
        }
    })
}

fn retag_nt<F>(nt: &str, mut rewrite: F) -> Result<String, String>
where
    F: FnMut(&str) -> Option<String>,
{
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for quad in RdfParser::from_format(RdfFormat::NTriples)
        .lenient()
        .for_reader(nt.as_bytes())
    {
        let quad = quad.map_err(|e| format!("N-Triples parse error: {e}"))?;
        let object = match &quad.object {
            Term::Literal(lit) => match lit.language().and_then(&mut rewrite) {
                Some(new_lang) => Term::Literal(retag_literal(lit, &new_lang)),
                None => quad.object.clone(),
            },
            _ => quad.object.clone(),
        };
        let rewritten = Quad::new(
            quad.subject.clone(),
            quad.predicate.clone(),
            object,
            quad.graph_name.clone(),
        );
        store
            .insert(&rewritten)
            .map_err(|e| format!("store insert failed: {e}"))?;
    }
    gmeow_validate::store::dump_store_to_ntriples(&store)
        .map_err(|e| format!("serialize retagged graph: {e}"))
}

fn retag_literal(lit: &Literal, language: &str) -> Literal {
    match lit.direction() {
        Some(direction) => Literal::new_directional_language_tagged_literal_unchecked(
            lit.value(),
            language,
            direction,
        ),
        None => Literal::new_language_tagged_literal_unchecked(lit.value(), language),
    }
}

fn store_len(store: &Store) -> Result<usize, String> {
    store.len().map_err(|e| format!("store length failed: {e}"))
}

fn gate_pure_gmeow(draft: &Store) -> Result<GateResult, String> {
    let mut foreign: BTreeMap<String, usize> = BTreeMap::new();
    for quad in default_graph_quads(draft)? {
        let predicate = quad.predicate.as_str();
        if predicate != RDF_TYPE
            && !STRUCTURAL_NAMESPACES
                .iter()
                .any(|namespace| predicate.starts_with(namespace))
            && !is_structural_predicate(predicate)
        {
            let key = vocab_of_iri(predicate).unwrap_or(predicate).to_owned();
            *foreign.entry(key).or_default() += 1;
        }
        if predicate == RDF_TYPE {
            if let Term::NamedNode(object) = &quad.object {
                let object = object.as_str();
                if !STRUCTURAL_NAMESPACES
                    .iter()
                    .any(|namespace| object.starts_with(namespace))
                {
                    let key = format!("a {}", vocab_of_iri(object).unwrap_or(object));
                    *foreign.entry(key).or_default() += 1;
                }
            }
        }
    }
    let residue: usize = foreign.values().sum();
    let passed = residue == 0;
    let mut gate = GateResult::new(
        "pure-gmeow-intermediate",
        passed,
        true,
        if passed {
            "draft is pure GMEOW".to_owned()
        } else {
            format!("{residue} consumer-vocab residue triples")
        },
    );
    gate.metrics.insert("residue".to_owned(), residue as f64);
    gate.detail = foreign
        .into_iter()
        .map(|(vocab, count)| format!("{vocab}: {count}"))
        .collect();
    Ok(gate)
}

fn is_structural_predicate(predicate: &str) -> bool {
    matches!(
        predicate,
        SKOS_EXACT_MATCH
            | SKOS_CLOSE_MATCH
            | SKOS_BROAD_MATCH
            | SKOS_NARROW_MATCH
            | SKOS_RELATED_MATCH
            | SKOS_MAPPING_RELATION
    )
}

fn gate_round_trip(
    source: &Store,
    output: &Store,
    tag_map: &HashMap<String, String>,
) -> Result<GateResult, String> {
    let source_by_vocab = by_vocab(source, true, tag_map)?;
    let output_by_vocab = by_vocab(output, true, tag_map)?;
    let mut rows = Vec::new();
    let mut linkage_rows = Vec::new();
    let mut total_source = 0usize;
    let mut total_recovered = 0usize;
    for (vocab, want) in source_by_vocab {
        let have = output_by_vocab.get(&vocab).cloned().unwrap_or_default();
        let recovered = want.intersection(&have).count();
        let pct = if want.is_empty() {
            100.0
        } else {
            100.0 * recovered as f64 / want.len() as f64
        };
        let row = format!("{vocab}: {}/{} ({pct:.0}%)", recovered, want.len());
        if EXTERNAL_LINKAGE_VOCABS.contains(&vocab.as_str()) {
            linkage_rows.push(format!("{row} [external linkage - not modeled by design]"));
            continue;
        }
        total_source += want.len();
        total_recovered += recovered;
        rows.push(row);
    }
    let overall = if total_source == 0 {
        100.0
    } else {
        100.0 * total_recovered as f64 / total_source as f64
    };
    let mut detail = rows;
    if !linkage_rows.is_empty() {
        detail.push(String::new());
        detail.push("external linkage (excluded from headline):".to_owned());
        detail.extend(linkage_rows);
    }
    let mut gate = GateResult::new(
        "round-trip-superset",
        total_recovered == total_source,
        false,
        format!(
            "{total_recovered}/{total_source} addressable source triples recovered ({overall:.0}%)"
        ),
    );
    gate.metrics.insert("recall_pct".to_owned(), overall);
    gate.metrics
        .insert("recovered".to_owned(), total_recovered as f64);
    gate.metrics
        .insert("addressable".to_owned(), total_source as f64);
    gate.detail = detail;
    Ok(gate)
}

fn gate_size_invariant(source: &Store, output: &Store) -> Result<GateResult, String> {
    let source_len = store_len(source)?;
    let output_len = store_len(output)?;
    let passed = output_len > source_len;
    let mut gate = GateResult::new(
        "size-invariant",
        passed,
        true,
        format!(
            "output {output_len} {} source {source_len}",
            if passed { ">" } else { "<=" }
        ),
    );
    gate.metrics.insert(
        "ratio".to_owned(),
        if source_len == 0 {
            0.0
        } else {
            output_len as f64 / source_len as f64
        },
    );
    Ok(gate)
}

fn gate_external_validator(
    root: &Path,
    output: &Store,
    _tag_map: &HashMap<String, String>,
) -> Result<GateResult, String> {
    let mut detail = Vec::new();
    let leaked = default_graph_quads(output)?
        .iter()
        .filter(|quad| match &quad.object {
            Term::Literal(lit) => lit
                .language()
                .is_some_and(gmeow_validate::language_tags::is_internal_tag),
            _ => false,
        })
        .count();
    detail.push(format!(
        "x-gmeow tag leak: {leaked} literals{}",
        if leaked == 0 { " OK" } else { " (HARD FAIL)" }
    ));

    let emitted = emitted_terms_by_vocab(output)?;
    let mut unattested = 0usize;
    for prefix in VENDORED_DEFS {
        let known = known_terms(root, prefix)?;
        let missing = emitted
            .get(*prefix)
            .cloned()
            .unwrap_or_default()
            .difference(&known)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            unattested += missing.len();
            let names = missing
                .iter()
                .map(|iri| local_name(iri))
                .collect::<Vec<_>>()
                .join(", ");
            detail.push(format!(
                "{prefix}: {} unattested term(s): {names} [report]",
                missing.len()
            ));
        }
    }

    let shacl_violations = run_range_shacl(root, output, &mut detail)?;
    let mut gate = GateResult::new(
        "external-validator",
        leaked == 0,
        true,
        format!(
            "x-gmeow leak={leaked} (hard); unattested terms={unattested}, range-SHACL violations={shacl_violations} (report-only)"
        ),
    );
    gate.metrics
        .insert("x_gmeow_leak".to_owned(), leaked as f64);
    gate.metrics
        .insert("unattested_terms".to_owned(), unattested as f64);
    gate.metrics
        .insert("shacl_violations".to_owned(), shacl_violations as f64);
    gate.detail = detail;
    Ok(gate)
}

fn gate_coverage(
    source: &Store,
    output: &Store,
    lifted: usize,
    gap_terms: usize,
) -> Result<GateResult, String> {
    let table = vocab_coverage(output, source)?;
    let mut gate = GateResult::new(
        "honest-coverage",
        true,
        false,
        format!("{lifted} triples lifted to GMEOW, {gap_terms} gap term(s)"),
    );
    gate.metrics.insert("lifted".to_owned(), lifted as f64);
    gate.metrics
        .insert("gap_terms".to_owned(), gap_terms as f64);
    gate.detail = table.lines().map(str::to_owned).collect();
    Ok(gate)
}

fn default_graph_quads(store: &Store) -> Result<Vec<Quad>, String> {
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        out.push(quad.map_err(|e| format!("store iteration failed: {e}"))?);
    }
    Ok(out)
}

fn by_vocab(
    store: &Store,
    iri_subjects_only: bool,
    tag_map: &HashMap<String, String>,
) -> Result<BTreeMap<String, BTreeSet<RdfTripleKey>>, String> {
    let mut buckets: BTreeMap<String, BTreeSet<RdfTripleKey>> = BTreeMap::new();
    for quad in default_graph_quads(store)? {
        if iri_subjects_only && !matches!(quad.subject, NamedOrBlankNode::NamedNode(_)) {
            continue;
        }
        let Some(vocab) = triple_vocab(&quad.predicate, &quad.object) else {
            continue;
        };
        buckets
            .entry(vocab.to_owned())
            .or_default()
            .insert(normalized_key(
                &quad.subject,
                &quad.predicate,
                &quad.object,
                tag_map,
            ));
    }
    Ok(buckets)
}

fn triple_vocab(predicate: &NamedNode, object: &Term) -> Option<&'static str> {
    if predicate.as_str() == RDF_TYPE {
        if let Term::NamedNode(object) = object {
            return vocab_of_iri(object.as_str());
        }
    }
    vocab_of_iri(predicate.as_str())
}

fn normalized_key(
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
    object: &Term,
    tag_map: &HashMap<String, String>,
) -> RdfTripleKey {
    RdfTripleKey(
        subject.to_string(),
        predicate.to_string(),
        normalized_term(object, tag_map),
    )
}

fn normalized_term(term: &Term, tag_map: &HashMap<String, String>) -> String {
    match term {
        Term::Literal(lit) => {
            if let Some(language) = lit.language() {
                let normalized = if gmeow_validate::language_tags::is_internal_tag(language) {
                    tag_map
                        .get(language)
                        .or_else(|| tag_map.get(&language.to_ascii_lowercase()))
                        .cloned()
                        .unwrap_or_else(|| language.to_ascii_lowercase())
                } else {
                    language.to_ascii_lowercase()
                };
                return Term::Literal(retag_literal(lit, &normalized)).to_string();
            }
            term.to_string()
        }
        _ => term.to_string(),
    }
}

fn emitted_terms_by_vocab(output: &Store) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut terms: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for quad in default_graph_quads(output)? {
        if quad.predicate.as_str() != RDF_TYPE {
            if let Some(vocab) = vocab_of_iri(quad.predicate.as_str()) {
                terms
                    .entry(vocab.to_owned())
                    .or_default()
                    .insert(quad.predicate.as_str().to_owned());
            }
        }
        if quad.predicate.as_str() == RDF_TYPE {
            if let Term::NamedNode(object) = &quad.object {
                if let Some(vocab) = vocab_of_iri(object.as_str()) {
                    terms
                        .entry(vocab.to_owned())
                        .or_default()
                        .insert(object.as_str().to_owned());
                }
            }
        }
    }
    Ok(terms)
}

fn known_terms(root: &Path, prefix: &str) -> Result<BTreeSet<String>, String> {
    let path = root
        .join("imports")
        .join("targets")
        .join(format!("{prefix}.ttl"));
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let store = gmeow_validate::store::load_sources_into_store(&[path])?;
    let mut known = BTreeSet::new();
    for quad in default_graph_quads(&store)? {
        if let NamedOrBlankNode::NamedNode(subject) = quad.subject {
            known.insert(subject.as_str().to_owned());
        }
        if let Term::NamedNode(object) = quad.object {
            known.insert(object.as_str().to_owned());
        }
    }
    Ok(known)
}

fn run_range_shacl(root: &Path, output: &Store, detail: &mut Vec<String>) -> Result<usize, String> {
    let mut total = 0usize;
    for prefix in VENDORED_DEFS {
        let Some(shapes_ttl) = generate_range_shapes(root, prefix)? else {
            continue;
        };
        let shapes = gmeow_shacl::engine::parse_shapes(&shapes_ttl)?;
        let report = gmeow_shacl::engine::validate(output, &shapes);
        if report.conforms {
            continue;
        }
        let count = report.results.len();
        total += count;
        detail.push(format!(
            "{prefix}: {count} range-SHACL violation(s) [report-only]"
        ));
    }
    Ok(total)
}

fn generate_range_shapes(root: &Path, prefix: &str) -> Result<Option<String>, String> {
    let path = root
        .join("imports")
        .join("targets")
        .join(format!("{prefix}.ttl"));
    if !path.exists() {
        return Ok(None);
    }
    let Some(namespace) = PREFIXES
        .iter()
        .find_map(|(candidate, namespace)| (*candidate == prefix).then_some(*namespace))
    else {
        return Ok(None);
    };
    let store = gmeow_validate::store::load_sources_into_store(&[path])?;
    let rdfs_range = NamedNode::new(RDFS_RANGE).map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    for quad in store.quads_for_pattern(None, Some(rdfs_range.as_ref()), None, None) {
        let quad = quad.map_err(|e| format!("store iteration failed: {e}"))?;
        let NamedOrBlankNode::NamedNode(prop) = quad.subject else {
            continue;
        };
        let Term::NamedNode(range) = quad.object else {
            continue;
        };
        if !range.as_str().starts_with(namespace) {
            continue;
        }
        body.push(format!(
            "<{}-rangeShape> a sh:NodeShape ; sh:targetObjectsOf <{}> ; sh:class <{}> .",
            prop.as_str(),
            prop.as_str(),
            range.as_str()
        ));
    }
    if body.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n{}\n",
        body.join("\n")
    )))
}

fn vocab_coverage(output: &Store, source: &Store) -> Result<String, String> {
    let ours = by_namespace(vocab_terms(output)?);
    let theirs = by_namespace(vocab_terms(source)?);
    let mut lines = vec![
        "| vocabulary | terms in target | covered | missing |".to_owned(),
        "|---|---|---|---|".to_owned(),
    ];
    let mut total_target = 0usize;
    let mut total_covered = 0usize;
    let empty_terms = BTreeSet::new();
    for (vocab, target_terms) in theirs {
        let ours_terms = ours.get(&vocab).unwrap_or(&empty_terms);
        let covered = target_terms
            .intersection(ours_terms)
            .cloned()
            .collect::<BTreeSet<_>>();
        let missing = target_terms
            .difference(&covered)
            .cloned()
            .collect::<Vec<_>>();
        total_target += target_terms.len();
        total_covered += covered.len();
        let mut shown = missing
            .iter()
            .take(8)
            .map(|item| format!("`{item}`"))
            .collect::<Vec<_>>()
            .join(", ");
        if missing.len() > 8 {
            shown.push_str(&format!(" ... +{} more", missing.len() - 8));
        }
        lines.push(format!(
            "| {vocab} | {} | {} | {} |",
            target_terms.len(),
            covered.len(),
            if shown.is_empty() {
                "-"
            } else {
                shown.as_str()
            }
        ));
    }
    lines.push(format!(
        "| **total** | **{total_target}** | **{total_covered}** | |"
    ));
    Ok(lines.join("\n") + "\n")
}

fn vocab_terms(store: &Store) -> Result<BTreeSet<String>, String> {
    let mut terms = BTreeSet::new();
    for quad in default_graph_quads(store)? {
        terms.insert(quad.predicate.as_str().to_owned());
        if quad.predicate.as_str() == RDF_TYPE {
            if let Term::NamedNode(object) = quad.object {
                terms.insert(object.as_str().to_owned());
            }
        }
    }
    Ok(terms)
}

fn by_namespace(terms: BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for term in terms {
        let mut matched = false;
        for (prefix, namespace) in sorted_prefixes() {
            if let Some(local) = term.strip_prefix(namespace) {
                grouped
                    .entry(prefix.to_owned())
                    .or_default()
                    .insert(local.to_owned());
                matched = true;
                break;
            }
        }
        if !matched {
            let namespace = namespace_part(&term).to_owned();
            grouped
                .entry(namespace)
                .or_default()
                .insert(local_name(&term).to_owned());
        }
    }
    grouped
}

fn namespace_part(iri: &str) -> &str {
    if let Some((namespace, _local)) = iri.rsplit_once('#') {
        namespace
    } else if let Some((namespace, _local)) = iri.rsplit_once('/') {
        namespace
    } else {
        iri
    }
}

fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    #[test]
    fn claim_audit_flags_the_worked_fixture_without_deleting_claims() {
        let root = root();
        let report = claim_audit(
            &root,
            &[root.join("tests/fixtures/coverage/hallucination-kg.ttl")],
        )
        .expect("audit report");
        let ex = "https://blackcatinformatics.ca/gmeow/examples/hallucination-kg/";

        assert_eq!(
            report.findings["claims-without-evidence"][0][0],
            format!("{ex}claim-hallucinated")
        );
        assert_eq!(
            report.findings["stale-source-claims"][0][0],
            format!("{ex}claim-stale")
        );
        assert!(report.shacl_errors.is_empty());
        assert!(!report.shacl_warnings.is_empty());
        assert!(report
            .claims
            .iter()
            .any(|claim| claim.claim == format!("{ex}claim-hallucinated")));
    }

    #[test]
    fn flat_claim_json_preserves_flags_evidence_and_contradictions() {
        let root = root();
        let report = claim_audit(
            &root,
            &[root.join("tests/fixtures/coverage/hallucination-kg.ttl")],
        )
        .expect("audit report");
        let ex = "https://blackcatinformatics.ca/gmeow/examples/hallucination-kg/";
        let by_iri: BTreeMap<_, _> = report
            .claims
            .iter()
            .map(|claim| (claim.claim.as_str(), claim))
            .collect();

        assert_eq!(by_iri.len(), 5);
        let grounded = by_iri[format!("{ex}claim-grounded").as_str()];
        assert_eq!(grounded.confidence.as_deref(), Some("0.95"));
        assert_eq!(grounded.evidence[0].start, Some(60));
        assert_eq!(grounded.evidence[0].end, Some(141));
        assert_eq!(
            grounded.evidence[0].polarity.as_deref(),
            Some("polaritySupports")
        );

        assert!(
            by_iri[format!("{ex}claim-hallucinated").as_str()]
                .flags
                .ungrounded
        );
        assert!(by_iri[format!("{ex}claim-hallucinated").as_str()]
            .evidence
            .is_empty());
        assert!(by_iri[format!("{ex}claim-low").as_str()].flags.contradicted);
        assert_eq!(
            by_iri[format!("{ex}claim-low").as_str()].contradicts,
            vec![format!("{ex}claim-high")]
        );
        assert!(by_iri[format!("{ex}claim-stale").as_str()].flags.stale);

        let rendered = render_claim_audit_json(&report).expect("json");
        assert!(rendered.contains("\"claims\""));
        assert!(render_claim_audit_text(&report).contains("claims audited: 5"));
    }

    #[test]
    fn claim_audit_diagnostics_maps_headlines_and_shacl() {
        let report = ClaimAuditReport {
            findings: BTreeMap::from([
                (
                    "claims-without-evidence".to_owned(),
                    vec![vec!["ex:claim-a".to_owned(), "evidence".to_owned()]],
                ),
                (
                    "claims-contradicted-by-higher-confidence".to_owned(),
                    vec![vec!["ex:claim-b".to_owned(), "x".to_owned()]],
                ),
                (
                    "stale-source-claims".to_owned(),
                    vec![vec!["ex:claim-c".to_owned(), "src".to_owned()]],
                ),
            ]),
            shacl_errors: vec!["focus ex:x violates sh:minCount".to_owned()],
            shacl_warnings: vec!["focus ex:y soft warning".to_owned()],
            claims: Vec::new(),
        };

        let diag = claim_audit_diagnostics(&report);
        let by_code: BTreeMap<_, _> = diag
            .findings
            .iter()
            .map(|item| (item.code.as_str(), item.severity))
            .collect();
        assert_eq!(by_code["audit.ungrounded-claim"], Severity::Warning);
        assert_eq!(by_code["audit.contradicted-claim"], Severity::Warning);
        assert_eq!(by_code["audit.stale-source"], Severity::Warning);
        assert_eq!(by_code["audit.shacl-error"], Severity::Error);
        assert_eq!(by_code["audit.shacl-warning"], Severity::Warning);
        assert_eq!(diag.error_count(), 1);
    }

    #[test]
    fn acceptance_runs_the_real_external_fixture_natively() {
        let root = root();
        let result = run_acceptance(
            &root,
            &root.join("tests/fixtures/coverage/external/bii.ttl"),
            true,
        )
        .expect("native acceptance report");

        assert_eq!(result.source, "bii.ttl");
        assert!(result.source_triples > 0);
        assert!(result.output_triples > result.source_triples);
        assert!(
            result.passed(),
            "hard acceptance gates must pass: {result:#?}"
        );
        assert!(result
            .gates
            .iter()
            .any(|gate| gate.name == "round-trip-superset"));
        assert!(result
            .gates
            .iter()
            .any(|gate| gate.name == "external-validator"));
    }
}
