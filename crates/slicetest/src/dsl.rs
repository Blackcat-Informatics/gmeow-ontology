// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Load a slice-resident test-DSL spec file and introspect its cells.
//!
//! A `tests/*.ttl` spec is itself ontology data in the `gmeow:` test-DSL
//! vocabulary (`dsl/tests/vocabulary.ttl`). Rather than keep a hand-written
//! deserializer in lockstep with that vocabulary, the harness loads each spec
//! into an oxigraph store (lenient parsing, the same primitive the validation
//! path uses) and SPARQL-introspects the three cell types into typed Rust
//! structs. The nested `ExpectedRow -> rowCell -> ExpectedCell` shape of a
//! SELECT competency question is pulled out declaratively in one join and
//! grouped in Rust.

use std::collections::BTreeMap;
use std::path::Path;

use oxigraph::model::Term;
use oxigraph::sparql::{QueryResults, QuerySolution, SparqlEvaluator};
use oxigraph::store::Store;

use gmeow_validate::store::build_store;

/// The GMEOW namespace; the test-DSL terms live directly under it.
pub const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The three cell collections parsed out of one `tests/*.ttl` spec file.
#[derive(Debug, Clone, Default)]
pub struct SpecFile {
    pub competency: Vec<CompetencyQuestion>,
    pub structural: Vec<StructuralAssertion>,
    pub conformance: Vec<ExampleConformance>,
}

/// A `gmeow:CompetencyQuestion` cell.
#[derive(Debug, Clone)]
pub struct CompetencyQuestion {
    pub iri: String,
    /// Inline `gmeow:cqQuery` (mutually exclusive with `query_file`).
    pub query_inline: Option<String>,
    /// `gmeow:cqQueryFile` — REPO-ROOT-relative path to a `.rq` file.
    pub query_file: Option<String>,
    /// `gmeow:cqExpectAsk` — expected ASK boolean (ASK questions only).
    pub expect_ask: Option<bool>,
    /// `gmeow:cqExpectRowCount` — coarse expected SELECT row count.
    pub expect_row_count: Option<u64>,
    /// `gmeow:cqExactRows` — whether the enumerated rows are the COMPLETE set.
    pub exact_rows: bool,
    /// `gmeow:cqExpectRow` — enumerated expected SELECT rows.
    pub expected_rows: Vec<ExpectedRow>,
    pub rationale: Option<String>,
}

/// One enumerated SELECT result row (`gmeow:ExpectedRow`).
#[derive(Debug, Clone)]
pub struct ExpectedRow {
    /// One cell per projected variable.
    pub cells: Vec<ExpectedCell>,
}

/// One variable-to-value binding within an [`ExpectedRow`] (`gmeow:ExpectedCell`).
#[derive(Debug, Clone)]
pub struct ExpectedCell {
    /// The SPARQL variable name, WITHOUT the leading `?`.
    pub var: String,
    /// The expected bound value — an IRI (`gmeow:cellValueIri`) or a literal
    /// (`gmeow:cellValueLiteral`), kept as an oxigraph [`Term`] so it compares
    /// directly against a query solution binding.
    pub value: Term,
}

/// A `gmeow:StructuralAssertion` cell.
#[derive(Debug, Clone)]
pub struct StructuralAssertion {
    pub iri: String,
    pub polarity: Polarity,
    /// `gmeow:saPattern` — a SPARQL ASK (mutually exclusive with `shape`).
    pub pattern: Option<String>,
    /// `gmeow:saShape` — an `sh:NodeShape` IRI (mutually exclusive with `pattern`).
    pub shape: Option<String>,
    /// `gmeow:saScope` — defaults to [`Scope::ModuleAndExamples`] when omitted.
    pub scope: Scope,
    pub rationale: Option<String>,
}

/// A `gmeow:ExampleConformance` cell.
#[derive(Debug, Clone)]
pub struct ExampleConformance {
    pub iri: String,
    /// `gmeow:exampleFile` — SLICE-relative path to the example/counter-example.
    pub file: String,
    pub outcome: Outcome,
    /// `gmeow:expectedViolationCode` — the `shacl.<LocalName>` code (violates only).
    pub violation_code: Option<String>,
    pub rationale: Option<String>,
}

/// `gmeow:saPolarity` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Must,
    MustNot,
}

/// `gmeow:saScope` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Module,
    ModuleAndExamples,
}

/// `gmeow:expectedOutcome` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Conforms,
    Violates,
}

// ── Introspection queries ──────────────────────────────────────────────────────

const PREFIX: &str = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n";

const Q_COMPETENCY: &str = "
SELECT ?cq ?queryFile ?query ?expectAsk ?rowCount ?exactRows ?rationale WHERE {
  ?cq a gmeow:CompetencyQuestion .
  OPTIONAL { ?cq gmeow:cqQueryFile ?queryFile }
  OPTIONAL { ?cq gmeow:cqQuery ?query }
  OPTIONAL { ?cq gmeow:cqExpectAsk ?expectAsk }
  OPTIONAL { ?cq gmeow:cqExpectRowCount ?rowCount }
  OPTIONAL { ?cq gmeow:cqExactRows ?exactRows }
  OPTIONAL { ?cq gmeow:cqRationale ?rationale }
}";

const Q_ROWS: &str = "
SELECT ?cq ?row ?var ?iri ?lit WHERE {
  ?cq gmeow:cqExpectRow ?row .
  ?row gmeow:rowCell ?cell .
  ?cell gmeow:cellVar ?var .
  OPTIONAL { ?cell gmeow:cellValueIri ?iri }
  OPTIONAL { ?cell gmeow:cellValueLiteral ?lit }
}";

const Q_STRUCTURAL: &str = "
SELECT ?sa ?polarity ?pattern ?shape ?scope ?rationale WHERE {
  ?sa a gmeow:StructuralAssertion ;
      gmeow:saPolarity ?polarity .
  OPTIONAL { ?sa gmeow:saPattern ?pattern }
  OPTIONAL { ?sa gmeow:saShape ?shape }
  OPTIONAL { ?sa gmeow:saScope ?scope }
  OPTIONAL { ?sa gmeow:saRationale ?rationale }
}";

const Q_CONFORMANCE: &str = "
SELECT ?ec ?file ?outcome ?code ?rationale WHERE {
  ?ec a gmeow:ExampleConformance ;
      gmeow:exampleFile ?file ;
      gmeow:expectedOutcome ?outcome .
  OPTIONAL { ?ec gmeow:expectedViolationCode ?code }
  OPTIONAL { ?ec gmeow:conformanceRationale ?rationale }
}";

// ── Public entry point ─────────────────────────────────────────────────────────

/// Load one `tests/*.ttl` spec file and introspect every cell it declares.
///
/// # Errors
///
/// Returns `Err(String)` if the file cannot be parsed, an introspection query
/// fails, or a cell declares an unrecognized controlled-vocabulary value.
pub fn load_spec(path: &Path) -> Result<SpecFile, String> {
    let store = build_store(std::slice::from_ref(&path.to_path_buf()))
        .map_err(|e| format!("failed to load spec {}: {e}", path.display()))?;
    Ok(SpecFile {
        competency: parse_competency(&store)?,
        structural: parse_structural(&store)?,
        conformance: parse_conformance(&store)?,
    })
}

// ── Parsers ────────────────────────────────────────────────────────────────────

fn parse_competency(store: &Store) -> Result<Vec<CompetencyQuestion>, String> {
    // 1. Scalars: one solution per CompetencyQuestion (all OPTIONALs single-valued).
    let mut by_iri: BTreeMap<String, CompetencyQuestion> = BTreeMap::new();
    for sol in select(store, &with_prefix(Q_COMPETENCY))? {
        let iri = require_iri(&sol, "cq")?;
        by_iri.insert(
            iri.clone(),
            CompetencyQuestion {
                iri,
                query_inline: opt_string(&sol, "query"),
                query_file: opt_string(&sol, "queryFile"),
                expect_ask: opt_bool(&sol, "expectAsk"),
                expect_row_count: opt_u64(&sol, "rowCount")?,
                exact_rows: opt_bool(&sol, "exactRows").unwrap_or(false),
                expected_rows: Vec::new(),
                rationale: opt_string(&sol, "rationale"),
            },
        );
    }

    // 2. Rows: group cells by (cq, row), preserving one cell per variable.
    //    rows_by_cq: cq IRI -> (row term-string -> Vec<ExpectedCell>).
    let mut rows_by_cq: BTreeMap<String, BTreeMap<String, Vec<ExpectedCell>>> = BTreeMap::new();
    for sol in select(store, &with_prefix(Q_ROWS))? {
        let cq = require_iri(&sol, "cq")?;
        let row_key = sol
            .get("row")
            .map(Term::to_string)
            .ok_or("cqExpectRow row missing ?row binding")?;
        let var = opt_string(&sol, "var").ok_or("ExpectedCell missing gmeow:cellVar")?;
        let value = match (sol.get("iri"), sol.get("lit")) {
            (Some(iri), None) => iri.clone(),
            (None, Some(lit)) => lit.clone(),
            (Some(_), Some(_)) => {
                return Err(format!(
                    "ExpectedCell ?{var} binds both cellValueIri and cellValueLiteral (exactly one allowed)"
                ));
            }
            (None, None) => {
                return Err(format!(
                    "ExpectedCell ?{var} binds neither cellValueIri nor cellValueLiteral"
                ));
            }
        };
        rows_by_cq
            .entry(cq)
            .or_default()
            .entry(row_key)
            .or_default()
            .push(ExpectedCell { var, value });
    }
    for (cq, rows) in rows_by_cq {
        if let Some(question) = by_iri.get_mut(&cq) {
            question.expected_rows = rows
                .into_values()
                .map(|cells| ExpectedRow { cells })
                .collect();
        }
    }

    Ok(by_iri.into_values().collect())
}

fn parse_structural(store: &Store) -> Result<Vec<StructuralAssertion>, String> {
    let mut out = Vec::new();
    for sol in select(store, &with_prefix(Q_STRUCTURAL))? {
        let iri = require_iri(&sol, "sa")?;
        let polarity = match local_name(&require_iri(&sol, "polarity")?) {
            "must" => Polarity::Must,
            "mustNot" => Polarity::MustNot,
            other => return Err(format!("{iri}: unknown saPolarity gmeow:{other}")),
        };
        let scope = match sol.get("scope").and_then(term_iri) {
            None => Scope::ModuleAndExamples, // default per the DSL vocabulary
            Some(iri) => match local_name(&iri) {
                "scopeModule" => Scope::Module,
                "scopeModuleAndExamples" => Scope::ModuleAndExamples,
                other => return Err(format!("unknown saScope gmeow:{other}")),
            },
        };
        out.push(StructuralAssertion {
            iri,
            polarity,
            pattern: opt_string(&sol, "pattern"),
            shape: sol.get("shape").and_then(term_iri),
            scope,
            rationale: opt_string(&sol, "rationale"),
        });
    }
    Ok(out)
}

fn parse_conformance(store: &Store) -> Result<Vec<ExampleConformance>, String> {
    let mut out = Vec::new();
    for sol in select(store, &with_prefix(Q_CONFORMANCE))? {
        let iri = require_iri(&sol, "ec")?;
        let outcome = match local_name(&require_iri(&sol, "outcome")?) {
            "conforms" => Outcome::Conforms,
            "violates" => Outcome::Violates,
            other => return Err(format!("{iri}: unknown expectedOutcome gmeow:{other}")),
        };
        out.push(ExampleConformance {
            iri,
            file: opt_string(&sol, "file").ok_or("ExampleConformance missing gmeow:exampleFile")?,
            outcome,
            violation_code: opt_string(&sol, "code"),
            rationale: opt_string(&sol, "rationale"),
        });
    }
    Ok(out)
}

// ── SPARQL + term helpers ──────────────────────────────────────────────────────

fn with_prefix(body: &str) -> String {
    format!("{PREFIX}{body}")
}

/// Run a SELECT introspection query and collect its solutions.
fn select(store: &Store, query: &str) -> Result<Vec<QuerySolution>, String> {
    let results = SparqlEvaluator::new()
        .parse_query(query)
        .map_err(|e| format!("introspection query parse error: {e}"))?
        .on_store(store)
        .execute()
        .map_err(|e| format!("introspection query evaluation error: {e}"))?;
    let solutions = match results {
        QueryResults::Solutions(s) => s,
        QueryResults::Boolean(_) | QueryResults::Graph(_) => {
            return Err("introspection query must be a SELECT".to_owned());
        }
    };
    solutions
        .map(|sol| sol.map_err(|e| format!("introspection solution error: {e}")))
        .collect()
}

/// The local name of a `gmeow:` IRI (the part after the namespace).
fn local_name(iri: &str) -> &str {
    iri.strip_prefix(NS).unwrap_or(iri)
}

fn term_iri(term: &Term) -> Option<String> {
    match term {
        Term::NamedNode(n) => Some(n.as_str().to_owned()),
        _ => None,
    }
}

fn require_iri(sol: &QuerySolution, var: &str) -> Result<String, String> {
    sol.get(var)
        .and_then(term_iri)
        .ok_or_else(|| format!("expected ?{var} to bind an IRI"))
}

/// The lexical value of a bound literal (or the IRI string of a named node).
fn opt_string(sol: &QuerySolution, var: &str) -> Option<String> {
    sol.get(var).map(|term| match term {
        Term::Literal(l) => l.value().to_owned(),
        Term::NamedNode(n) => n.as_str().to_owned(),
        other => other.to_string(),
    })
}

fn opt_bool(sol: &QuerySolution, var: &str) -> Option<bool> {
    sol.get(var).and_then(|term| match term {
        Term::Literal(l) => Some(l.value() == "true"),
        _ => None,
    })
}

fn opt_u64(sol: &QuerySolution, var: &str) -> Result<Option<u64>, String> {
    match sol.get(var) {
        None => Ok(None),
        Some(Term::Literal(l)) => l
            .value()
            .parse::<u64>()
            .map(Some)
            .map_err(|e| format!("?{var} is not a non-negative integer: {e}")),
        Some(other) => Err(format!("?{var} expected a literal, got {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;

    fn epistemics_tests_dir() -> std::path::PathBuf {
        paths::slices_root().join("core/epistemics/tests")
    }

    #[test]
    fn parses_competency_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("competency.ttl"))
            .expect("competency.ttl must parse");
        assert_eq!(spec.competency.len(), 2, "two competency questions");

        let agents = spec
            .competency
            .iter()
            .find(|c| c.iri.ends_with("cqAgentKinds"))
            .expect("cqAgentKinds present");
        assert_eq!(
            agents.query_file.as_deref(),
            Some("queries/competency/agents.rq")
        );
        assert!(agents.exact_rows);
        assert_eq!(
            agents.expected_rows.len(),
            6,
            "six fully-enumerated agent kinds"
        );
        // Each row has exactly one cell binding ?agentKind to a gmeow: IRI.
        for row in &agents.expected_rows {
            assert_eq!(row.cells.len(), 1);
            assert_eq!(row.cells[0].var, "agentKind");
            assert!(matches!(row.cells[0].value, Term::NamedNode(_)));
        }

        let roles = spec
            .competency
            .iter()
            .find(|c| c.iri.ends_with("cqContributionRoles"))
            .expect("cqContributionRoles present");
        assert_eq!(roles.expect_row_count, Some(48));
        assert_eq!(
            roles.expected_rows.len(),
            2,
            "two sample rows pre-migration"
        );
    }

    #[test]
    fn parses_structural_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("structural.ttl"))
            .expect("structural.ttl must parse");
        assert_eq!(spec.structural.len(), 2);

        let must = spec
            .structural
            .iter()
            .find(|s| s.iri.ends_with("saKnowsThatSubPropertyOfBelieves"))
            .expect("keystone entailment present");
        assert_eq!(must.polarity, Polarity::Must);
        assert_eq!(must.scope, Scope::Module);
        assert!(must.pattern.as_deref().unwrap().contains("subPropertyOf"));

        let must_not = spec
            .structural
            .iter()
            .find(|s| s.iri.ends_with("saNoTruthBit"))
            .expect("no-truth-bit present");
        assert_eq!(must_not.polarity, Polarity::MustNot);
    }

    #[test]
    fn parses_conformance_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("example-conformance.ttl"))
            .expect("example-conformance.ttl must parse");
        assert_eq!(spec.conformance.len(), 2);

        let conforms = spec
            .conformance
            .iter()
            .find(|c| c.outcome == Outcome::Conforms)
            .expect("conforming fixture present");
        assert_eq!(conforms.file, "examples/justification-and-defeat.ttl");
        assert!(conforms.violation_code.is_none());

        let violates = spec
            .conformance
            .iter()
            .find(|c| c.outcome == Outcome::Violates)
            .expect("violating fixture present");
        assert_eq!(
            violates.violation_code.as_deref(),
            Some("shacl.MinCountConstraintComponent")
        );
    }
}
