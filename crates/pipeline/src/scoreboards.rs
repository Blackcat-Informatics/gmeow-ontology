// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native evaluator scoreboards for repository gates (#946).
//!
//! The Python command surface remains a thin interface, but the claim-audit and
//! acceptance scoreboard authority lives here.  This module starts with the
//! claim audit: committed SPARQL gates, SHACL findings, flat claim JSON, and the
//! canonical diagnostics projection.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_diagnostics::{Finding, Location, Report, Severity};
use oxigraph::model::Term;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use serde::Serialize;

const GM: &str = "https://blackcatinformatics.ca/gmeow/";

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
}
