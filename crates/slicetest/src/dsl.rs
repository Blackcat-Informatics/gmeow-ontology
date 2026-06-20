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
#[derive(Debug, Clone, PartialEq)]
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
    /// `gmeow:cqReasoning` — the entailment lane (defaults to [`ReasoningProfile::None`]).
    pub reasoning: ReasoningProfile,
    pub rationale: Option<String>,
}

/// One enumerated SELECT result row (`gmeow:ExpectedRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedRow {
    /// One cell per projected variable.
    pub cells: Vec<ExpectedCell>,
}

/// One variable-to-value binding within an [`ExpectedRow`] (`gmeow:ExpectedCell`).
#[derive(Debug, Clone, PartialEq)]
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

/// `gmeow:cqReasoning` value — the entailment lane a competency question runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningProfile {
    /// `gmeow:reasoningNone` (the default): the asserted merged graph; SPARQL
    /// property paths supply transitive subclass/subproperty closure.
    #[default]
    None,
    /// `gmeow:reasoningRdfs`: the merged graph closed under RDFS (domain/range
    /// typing + type/subclass/subproperty propagation).
    Rdfs,
}

// ── Introspection queries ──────────────────────────────────────────────────────

const PREFIX: &str = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n";

const Q_COMPETENCY: &str = "
SELECT ?cq ?queryFile ?query ?expectAsk ?rowCount ?exactRows ?reasoning ?rationale WHERE {
  ?cq a gmeow:CompetencyQuestion .
  OPTIONAL { ?cq gmeow:cqQueryFile ?queryFile }
  OPTIONAL { ?cq gmeow:cqQuery ?query }
  OPTIONAL { ?cq gmeow:cqExpectAsk ?expectAsk }
  OPTIONAL { ?cq gmeow:cqExpectRowCount ?rowCount }
  OPTIONAL { ?cq gmeow:cqExactRows ?exactRows }
  OPTIONAL { ?cq gmeow:cqReasoning ?reasoning }
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
        let candidate = CompetencyQuestion {
            iri: iri.clone(),
            query_inline: opt_string(&sol, "query"),
            query_file: opt_string(&sol, "queryFile"),
            expect_ask: opt_bool(&sol, "expectAsk")?,
            expect_row_count: opt_u64(&sol, "rowCount")?,
            exact_rows: opt_bool(&sol, "exactRows")?.unwrap_or(false),
            expected_rows: Vec::new(),
            reasoning: match sol.get("reasoning").and_then(term_iri) {
                None => ReasoningProfile::None, // default
                Some(iri) => match local_name(&iri) {
                    "reasoningNone" => ReasoningProfile::None,
                    "reasoningRdfs" => ReasoningProfile::Rdfs,
                    other => return Err(format!("unknown cqReasoning gmeow:{other}")),
                },
            },
            rationale: opt_string(&sol, "rationale"),
        };
        // A multi-valued OPTIONAL (or a duplicated triple) can yield more than one
        // ?cq solution for the same question. Identical repeats are harmless, but
        // conflicting scalar fields would otherwise overwrite silently — making the
        // parsed spec depend on SPARQL solution order. Hard-fail on conflict.
        match by_iri.entry(iri) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(candidate);
            }
            std::collections::btree_map::Entry::Occupied(existing) => {
                if *existing.get() != candidate {
                    return Err(format!(
                        "competency question {} has conflicting duplicate definitions \
                         (multiple solutions with differing scalar fields)",
                        existing.key()
                    ));
                }
            }
        }
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

fn opt_bool(sol: &QuerySolution, var: &str) -> Result<Option<bool>, String> {
    match sol.get(var) {
        None => Ok(None),
        // xsd:boolean lexical space: true/false plus the 1/0 alternatives. Anything
        // else is a malformed literal that would silently read as `false` if coerced
        // — hard-fail instead (a typoed boolean must never quietly weaken a guard).
        Some(Term::Literal(l)) => match l.value() {
            "true" | "1" => Ok(Some(true)),
            "false" | "0" => Ok(Some(false)),
            other => Err(format!(
                "?{var} is not an xsd:boolean (true/false): {other:?}"
            )),
        },
        Some(other) => Err(format!("?{var} expected a literal, got {other}")),
    }
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
        // T3 finished the enumeration: the coarse row-count escape hatch is gone,
        // every one of the 48 roles is pinned, and cqExactRows is set.
        assert_eq!(roles.expect_row_count, None);
        assert!(roles.exact_rows);
        assert_eq!(roles.expected_rows.len(), 48, "all 48 roles enumerated");
    }

    #[test]
    fn parses_structural_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("structural.ttl"))
            .expect("structural.ttl must parse");
        // The T3 migration expanded the two T2 keystones to the full set of
        // module invariants lifted from test_epistemics.py.
        assert!(
            spec.structural.len() >= 13,
            "the migrated structural invariants are present, got {}",
            spec.structural.len()
        );

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

        // A migrated MUST-NOT over a VALUES set (the open-range invariant) parses.
        let open_range = spec
            .structural
            .iter()
            .find(|s| s.iri.ends_with("saSpineOpenRange"))
            .expect("migrated spine-open-range present");
        assert_eq!(open_range.polarity, Polarity::MustNot);
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

    /// Load inline Turtle into a store, mirroring the lenient parse the harness uses.
    fn store_from_turtle(ttl: &str) -> Store {
        use oxigraph::io::{RdfFormat, RdfParser};
        let store = Store::new().expect("store");
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store
                .insert(&triple.expect("valid turtle"))
                .expect("insert");
        }
        store
    }

    #[test]
    fn rejects_conflicting_duplicate_competency_questions() {
        // Two cqExpectRowCount values for the SAME question yield two ?cq solutions
        // with conflicting scalar fields. Silently keeping whichever the SPARQL
        // engine returned last would make the spec solution-order-dependent.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqDup a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqExpectRowCount 1, 2 .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("conflicting duplicate must hard-fail");
        assert!(
            err.contains("conflicting duplicate"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn identical_duplicate_competency_solutions_are_harmless() {
        // A repeated identical scalar (here a duplicated cqQueryFile triple) is not a
        // conflict — it must parse, collapsing to one question.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqSame a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\", \"q.rq\" .\n";
        let spec = parse_competency(&store_from_turtle(ttl)).expect("identical repeat is fine");
        assert_eq!(spec.len(), 1);
    }

    #[test]
    fn rejects_malformed_boolean_literal() {
        // A typoed boolean must hard-fail, never silently coerce to false (which would
        // quietly flip cqExactRows from exact-match to subset).
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqBad a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqExactRows \"ture\" .\n"; // codespell:ignore ture
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("malformed boolean must hard-fail");
        assert!(err.contains("xsd:boolean"), "unexpected error: {err}");
    }
}
