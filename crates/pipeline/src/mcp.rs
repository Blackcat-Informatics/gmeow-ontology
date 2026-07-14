// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native MCP surfaces over the bundled GMEOW snapshot.
//!
//! `McpView` loads the bundled `gmeow.gts` snapshot ONCE (the narrow waist,
//! bundle-only — never the repo) and serves the `export`-backed surfaces —
//! `lookup_term`, `llms_txt`, `llms_full`, `doc_card`, `okf_index` — over a
//! per-language [`FoldView`]. The standard `llms.txt`/`doc_card` surfaces
//! make the docs themselves agent-consumable: the index links into the published
//! site (URLs recovered from the `gmeow:graph/documentation` graph) and the card
//! is the per-term, context-window-ready twin of the site's `card.md`. `McpServer`
//! owns the stdio JSON-RPC loop, startup language validation, resource routing,
//! and grounded-memory triad; the native `gmeow`/`gmeow-dev` CLI is the launcher.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Value, json};

use gmeow_errors::ResultExt;
use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};
use gmeow_logic::conjecture::{ConjectureLifecycleState, conjecture_test};
use gmeow_logic::explain::{self, LazyExplanationIndex, Row, reifier_from_row};
use gmeow_logic::provenance::{reifier_from_strings, term_display};
use gmeow_logic::query_ir::Budget;
use gmeow_logic::reason::reason_all_budgeted;
use gmeow_logic::result::{CompletenessStatus, EvaluationStatus, ReasoningResult};
use gmeow_logic::result_rdf::{
    ConjectureVerdictInput, conjecture_node_iri, project_conjecture_verdict,
    project_conjecture_withdrawal, project_reasoning_result,
};
use gmeow_logic::transaction::execute::{CommitMode, TxReceipt, execute_transaction};
use gmeow_logic::verify::{embedded_verify_queries, verify_with_reasoning_result};
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE, Term as IrTerm};
use gmeow_validate::local_oracle::{self, EntailmentView, FixtureView};
use purrdf::gts::examples::agent_memory::{
    Memory, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
};
use purrdf::gts::model::{Term as GtsTerm, TermKind as GtsTermKind};
use purrdf::gts::writer::Writer as GtsWriter;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};
use sha2::{Digest, Sha256};

use crate::stages::export::{self, ConsumerResolution, FoldView, Term};
use crate::stages::fold_arena;

// The internal→BCP-47 display-language map is carried on the lang: carrier
// varieties: each lang:LanguageVariety bears its internal tag through
// lang:carrierTag and its generated (folded) external tag through gmeow:bcp47Tag.
const LANGUAGE_CLASS: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
const LANGUAGE_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The `gmeow:` vocabulary namespace — the base of the documentation-graph
/// predicate and enumeration IRIs (`gmeow:docFixtureKind…`, etc.).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const TOOL_AGENT_NS: &str = "urn:gmeow:tool:";
/// The distinct external-provenance named graph the read-only local overlay is
/// re-homed into (the origin marker). Overlay triples are visible to reads
/// (`bundle ∪ overlay`) but quarantined under this graph — NEVER unioned into the
/// signed `gmeow:` canon and NEVER written back.
const EXTERNAL_OVERLAY_GRAPH: &str = "urn:gmeow:mcp:overlay:external";

/// The GMEOW namespace the native validation surface reasons in — the SAME
/// namespace the CLI `gmeow validate` passes to `gmeow_validate::data_validate`, so
/// `validate_local` never diverges from the shipped validator.
const MCP_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// The origin marker stamped on every `validate_local` finding's primary location —
/// the transient, inline data has no file path, so this synthetic origin identifies
/// the tool that produced the finding.
const VALIDATE_LOCAL_ORIGIN: &str = "mcp:validate_local";

/// A generous ceiling on the inline `data` payload `validate_local` accepts (8 MiB).
/// A larger payload is a HARD FAIL with a finding-style error — never silently
/// truncated (a truncated RDF graph would mis-parse and mislead).
const MAX_VALIDATE_DATA_BYTES: usize = 8 * 1024 * 1024;

/// The pre-reasoning hard ceiling on the `verify_graph` overlay size (quad count).
/// An overlay larger than this is REFUSED before any reasoning runs, so an
/// agent-supplied external annex can never push the governed forward closure past a
/// bounded starting EDB. 100_000 quads is a generous local-graph bound — far above
/// any hand-authored annex — yet keeps the `bundle ∪ overlay` EDB, and thus the
/// budgeted chase over it, bounded. Exceeding it is a HARD FAIL (the bounded agent
/// path), never a silently truncated graph.
const MAX_VERIFY_OVERLAY_QUADS: usize = 100_000;

/// The pre-*read* hard ceiling on the `verify_graph` overlay file size (raw bytes on
/// disk), checked via [`std::fs::metadata`] BEFORE the file is ever `fs::read` into
/// memory. [`MAX_VERIFY_OVERLAY_QUADS`] alone bounds the PARSED quad count, but that
/// check only runs AFTER the whole file has already been loaded and parsed — so an
/// agent-supplied overlay path pointing at a huge file (or a file with a single
/// enormous literal that parses to very few quads) could exhaust memory before the
/// quad ceiling ever gets a chance to refuse it. 16 MiB is generous — the existing
/// 100,000-quad ceiling, serialized as short synthetic IRIs/literals, tops out around
/// ~4 MiB, and no hand-authored local annex plausibly approaches this — yet it bounds
/// the raw bytes read into memory to a fixed, small multiple of that. Exceeding it is
/// a HARD FAIL BEFORE the read (the bounded agent path), never a truncated read.
const MAX_VERIFY_OVERLAY_BYTES: u64 = 16 * 1024 * 1024;

/// The forward-chase derivation-step budget every agent-facing reasoning tool
/// (`verify_graph`, `explain_quad`, `conjecture_test`, `store_conjecture`) runs under when
/// the caller OMITS `max_steps`. R4 forbids exposing an unbudgeted
/// Turing-complete evaluation to an agent loop, so an omitted `max_steps` is never `None`
/// (unbounded) on these paths — see [`governed_budget`].
///
/// Deliberately SMALL, not just finite: [`reason_all_budgeted`] only skips the expensive DL
/// consistency post-pass while the governor actually CUTS the chase
/// ([`gmeow_logic::reason::reason_all_budgeted`]'s `BudgetExhausted` branch); once
/// `max_steps` reaches the true closure size the run instead COMPLETES and pays the full DL
/// consistency scan — on the shipped bundle that is the same ~500 s whole-graph cost the
/// off-gate `whole_bundle_coherence_gate_*` suite measures, an unusable default latency for
/// an interactive agent call. 64 mirrors the value the sibling `explain_quad_*_heavy_offgate`
/// tests already use for this exact reason ("a small governor budget already derives
/// IRI-object quads... a larger budget explodes the closure without adding coverage") and is
/// empirically well below the shipped bundle's true closure size (measured: `max_steps: 100`
/// still exhausts the budget in ~15 s; `max_steps: 500` reaches the true closure and falls
/// into the ~500 s DL post-pass).
const DEFAULT_MAX_STEPS: u64 = 64;

/// The hard ceiling no agent-supplied `max_steps` may exceed, on the same tools as
/// [`DEFAULT_MAX_STEPS`]. Tied to [`MAX_VERIFY_OVERLAY_QUADS`] — the pre-reasoning overlay
/// EDB quad ceiling already governing `verify_graph` — so the post-EDB forward-chase step
/// budget rides the same order of magnitude as the bounded starting EDB it chases over. A
/// caller-supplied value above this is CLAMPED down, never honored past the ceiling. Unlike
/// the small [`DEFAULT_MAX_STEPS`], this ceiling is reached only by an agent's OWN explicit,
/// informed request for a deeper (possibly slow, but always finite) evaluation — never by an
/// omitted argument.
const HARD_MAX_STEPS: u64 = MAX_VERIFY_OVERLAY_QUADS as u64;

/// The answer-binding cap every agent-facing reasoning tool runs under when the caller
/// OMITS `max_answers`; see [`DEFAULT_MAX_STEPS`]. Matches its scale: both bound the same
/// "generous but small" no-args default.
const DEFAULT_MAX_ANSWERS: usize = 64;

/// The hard ceiling no agent-supplied `max_answers` may exceed, on the same tools as
/// [`DEFAULT_MAX_ANSWERS`]. Matches [`MAX_VERIFY_OVERLAY_QUADS`] in order of magnitude —
/// the same "generous but bounded" scale as every other agent-facing ceiling in this file.
const HARD_MAX_ANSWERS: usize = MAX_VERIFY_OVERLAY_QUADS;

/// Build a governed [`Budget`] for an agent-facing MCP tool call — the ONLY way any
/// `tool_*` wrapper in this file may construct a `Budget` (R4: never expose an
/// unbudgeted Turing-complete evaluation to an agent loop).
///
/// * An omitted (`None`) user value falls back to the finite default
///   ([`DEFAULT_MAX_STEPS`] / [`DEFAULT_MAX_ANSWERS`]).
/// * A supplied user value is CLAMPED to the hard ceiling ([`HARD_MAX_STEPS`] /
///   [`HARD_MAX_ANSWERS`]) — the caller may only LOWER the bound, never raise it past the
///   server-side cap.
///
/// Both returned fields are therefore always `Some`: this helper can never produce the
/// unbounded `Budget { max_answers: None, max_steps: None }` an omitted-args call used to
/// build.
fn governed_budget(max_steps: Option<u64>, max_answers: Option<usize>) -> Budget {
    let max_steps = max_steps.unwrap_or(DEFAULT_MAX_STEPS).min(HARD_MAX_STEPS);
    let max_answers = max_answers
        .unwrap_or(DEFAULT_MAX_ANSWERS)
        .min(HARD_MAX_ANSWERS);
    Budget {
        max_answers: Some(max_answers),
        max_steps: Some(max_steps),
    }
}

/// SELECT the counter-example conformance fixtures from the documentation graph: the
/// fixture IRI, its authored violation code, the full Turtle body, its label, the
/// authored outcome/rationale, and each referenced documented term (repeatable).
const COUNTER_EXAMPLE_FIXTURE_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?f ?code ?text ?label ?outcome ?rationale ?term WHERE {
  ?f a gm:DocFixture ;
     gm:docFixtureKind gm:docFixtureKindCounterExample ;
     gm:docViolationCode ?code ;
     gm:docFixtureText ?text .
  OPTIONAL { ?f rdfs:label ?label }
  OPTIONAL { ?f gm:docExpectedOutcome ?outcome }
  OPTIONAL { ?f gm:conformanceRationale ?rationale }
  OPTIONAL { ?f gm:documents ?term }
}";

/// SELECT the well-formed conformance fixtures (positive exemplars) from the
/// documentation graph: the fixture IRI, the full Turtle body, its label, the
/// authored outcome, and each referenced documented term (the well-formed↔term join
/// key). A well-formed fixture carries no violation code.
const WELLFORMED_FIXTURE_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?f ?text ?label ?outcome ?term WHERE {
  ?f a gm:DocFixture ;
     gm:docFixtureKind gm:docFixtureKindWellformed ;
     gm:docFixtureText ?text .
  OPTIONAL { ?f rdfs:label ?label }
  OPTIONAL { ?f gm:docExpectedOutcome ?outcome }
  OPTIONAL { ?f gm:documents ?term }
}";

/// SELECT the entailment records from the documentation graph: the entailment IRI,
/// the documented term it grounds on, the rule, the conclusion, and each premise
/// (repeatable).
const ENTAILMENT_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
SELECT ?e ?term ?rule ?conclusion ?premise WHERE {
  ?e a gm:Entailment ;
     gm:documents ?term ;
     gm:entailmentRule ?rule ;
     gm:entailmentConclusion ?conclusion .
  OPTIONAL { ?e gm:entailmentPremise ?premise }
}";

/// SELECT the conformance fixtures documenting one specific term (both kinds), for
/// the `counter_examples` tool: the fixture IRI, its enumerated
/// `gmeow:docFixtureKind`, the full Turtle body, its label, and the authored
/// advisory fields. `term_iri` is a canonical term IRI already resolved from the
/// bundle's own term set, so embedding it as an IRI ref is not a caller-injected
/// value.
fn fixtures_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?f ?kind ?text ?label ?outcome ?code ?rationale WHERE {{
  ?f a gm:DocFixture ;
     gm:documents <{term_iri}> ;
     gm:docFixtureKind ?kind ;
     gm:docFixtureText ?text .
  OPTIONAL {{ ?f rdfs:label ?label }}
  OPTIONAL {{ ?f gm:docExpectedOutcome ?outcome }}
  OPTIONAL {{ ?f gm:docViolationCode ?code }}
  OPTIONAL {{ ?f gm:conformanceRationale ?rationale }}
}}"
    )
}

/// SELECT the entailment records documenting one specific term, for the
/// `entailments` tool: the entailment IRI, its rule, its conclusion, and each
/// premise (repeatable). See [`fixtures_by_term_query`] on the embedded IRI.
fn entailments_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?e ?rule ?conclusion ?premise WHERE {{
  ?e a gm:Entailment ;
     gm:documents <{term_iri}> ;
     gm:entailmentRule ?rule ;
     gm:entailmentConclusion ?conclusion .
  OPTIONAL {{ ?e gm:entailmentPremise ?premise }}
}}"
    )
}

/// SELECT the diagnostics evidence documenting one specific term, for the
/// full-tier `doc_card` panel: each grounded finding code and the evidence claim.
/// The `gmeow:docEvidenceKindDiagnostics` evidence node is projected into the
/// documentation graph per term that a finding structurally concerns. See
/// [`fixtures_by_term_query`] on the embedded IRI.
fn diagnostics_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?claim ?code WHERE {{
  ?e a gm:DocEvidence ;
     gm:docEvidenceKind gm:docEvidenceKindDiagnostics ;
     gm:documents <{term_iri}> ;
     gm:docClaim ?claim ;
     gm:docGroundedBy ?code .
}}"
    )
}

/// SELECT the projection-loss evidence documenting one specific term, for the
/// full-tier `doc_card` panel: each grounded loss target and the preservation
/// judgment (`gmeow:docJudgment`). The `gmeow:docEvidenceKindLoss` evidence node
/// is projected per term that degrades under one or more projections. See
/// [`fixtures_by_term_query`] on the embedded IRI.
fn loss_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?target ?judgment WHERE {{
  ?e a gm:DocEvidence ;
     gm:docEvidenceKind gm:docEvidenceKindLoss ;
     gm:documents <{term_iri}> ;
     gm:docGroundedBy ?target ;
     gm:docJudgment ?judgment .
}}"
    )
}

/// The output format of the `doc_card` tool: rendered Markdown or the neutral
/// [`gmeow_docs::card::Card`] serialized to JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardFormat {
    /// The card rendered through the single shared Markdown renderer.
    Markdown,
    /// The card serialized as a JSON object (deterministic field order).
    Json,
}

impl CardFormat {
    /// Parse the `format` argument — an UNKNOWN value is a HARD FAIL listing the
    /// valid values (never a silent default).
    fn parse(raw: Option<&str>) -> gmeow_errors::Result<Self> {
        match raw.unwrap_or("markdown") {
            "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            other => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "doc_card: unknown format `{other}`; valid values: markdown, json"
                ),
            })),
        }
    }

    /// The canonical label echoed back in the response envelope.
    fn label(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
        }
    }
}

/// Parse the `detail` argument into a [`gmeow_docs::card::CardDetail`] tier — an
/// UNKNOWN value is a HARD FAIL listing the valid values (never a silent default).
fn parse_card_detail(raw: Option<&str>) -> gmeow_errors::Result<gmeow_docs::card::CardDetail> {
    use gmeow_docs::card::CardDetail;
    match raw.unwrap_or("standard") {
        "summary" => Ok(CardDetail::Summary),
        "standard" => Ok(CardDetail::Standard),
        "full" => Ok(CardDetail::Full),
        other => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "doc_card: unknown detail `{other}`; valid values: summary, standard, full"
            ),
        })),
    }
}

/// The canonical `detail` label echoed back in the response envelope.
fn card_detail_label(detail: gmeow_docs::card::CardDetail) -> &'static str {
    use gmeow_docs::card::CardDetail;
    match detail {
        CardDetail::Summary => "summary",
        CardDetail::Standard => "standard",
        CardDetail::Full => "full",
    }
}

/// One-line and cap a fixture Turtle body to a short snippet for the full-tier
/// `doc_card` Do / Don't panels (the card is token-budgeted; the full body is
/// available through the `counter_examples` tool).
fn fixture_body_snippet(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    gmeow_docs::llms::cap_note(&one_line)
}

/// SELECT the documented competency questions for the `competency_questions` tool:
/// every `gmeow:DocumentedCompetency` carrying a runnable `gmeow:cqQueryText`, or —
/// when `term_iri` is `Some` — only those documenting that term. `cqQueryText` is a
/// required pattern (not OPTIONAL), so every returned record is a runnable question.
fn competency_query(term_iri: Option<&str>) -> String {
    let documents = term_iri
        .map(|iri| format!("     gm:documents <{iri}> ;\n"))
        .unwrap_or_default();
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?c ?q ?rationale ?count ?exact WHERE {{
  ?c a gm:DocumentedCompetency ;
{documents}     gm:cqQueryText ?q .
  OPTIONAL {{ ?c gm:cqRationale ?rationale }}
  OPTIONAL {{ ?c gm:cqExpectRowCount ?count }}
  OPTIONAL {{ ?c gm:cqExactRows ?exact }}
}}"
    )
}

/// SELECT the searchable documentation-entry records from the documentation graph:
/// each entry subject, its `rdf:type`, the REAL display label, the optional
/// definition, the site URL, the documented real IRI, and the repeatable advisory /
/// alignment / missing-coverage facets. The `docSearchLabel` pattern is required (not
/// OPTIONAL), so only genuine documentation-entry records (gmeow:DocumentedTerm /
/// DocumentedSlice / DocumentedConcern) match — never a bare evidence node.
const DOC_SEARCH_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
SELECT ?s ?type ?label ?definition ?url ?documents ?advice ?alignment ?missing WHERE {
  ?s rdf:type ?type ;
     gm:docSearchLabel ?label .
  OPTIONAL { ?s gm:docSearchDefinition ?definition }
  OPTIONAL { ?s gm:docUrl ?url }
  OPTIONAL { ?s gm:documents ?documents }
  OPTIONAL { ?s gm:docSearchAdvice ?advice }
  OPTIONAL { ?s gm:docSearchAlignment ?alignment }
  OPTIONAL { ?s gm:docMissesDimension ?missing }
}";

/// One ranked documentation search hit the `docs_search` tool returns. Carries the
/// same facet shape the site `search-index.json` record does (`kind`, `id`, `label`,
/// `definition`, `url`, `advice`, `alignments`, `missing_coverage`) plus a private
/// match `rank` used only for the deterministic ordering.
#[derive(Debug)]
struct SearchHit {
    /// The record kind (`term`, `slice`, `concern`) from its `rdf:type` local name.
    kind: String,
    /// The documented real IRI (`gmeow:documents`) — a value a caller can pass back to
    /// `lookup_term` / `doc_card`.
    id: String,
    /// The REAL display label (`gmeow:docSearchLabel`).
    label: String,
    /// The definition prose (`gmeow:docSearchDefinition`), absent when the record
    /// carries none.
    definition: Option<String>,
    /// The site-relative page URL (`gmeow:docUrl`), absent when the record carries none.
    url: Option<String>,
    /// The advisory-prose facet (`gmeow:docSearchAdvice`), sorted+deduped; empty when
    /// none.
    advice: Vec<String>,
    /// The crosswalk alignment facet (`gmeow:docSearchAlignment`), sorted+deduped;
    /// empty when none.
    alignments: Vec<String>,
    /// The missing-dimension facet — the local names of `gmeow:docMissesDimension`,
    /// sorted+deduped; empty when the record misses no dimension.
    missing_coverage: Vec<String>,
    /// The match rank (lower is better): 0 label, 1 definition, 2 advice. Not
    /// serialized — the tie-break beside the id gives a stable, reproducible order.
    rank: u8,
}

impl SearchHit {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "id": self.id,
            "label": self.label,
            "definition": self.definition,
            "url": self.url,
            "advice": self.advice,
            "alignments": self.alignments,
            "missing_coverage": self.missing_coverage,
        })
    }
}

/// Whether a lowercased searchable field matches the query: a substring hit on the
/// whole lowercased query, OR (token mode) every whitespace-split query token appears
/// somewhere in the field. Case-insensitive by construction (both sides lowercased).
fn field_matches(field_lc: &str, query_lc: &str, tokens: &[&str]) -> bool {
    field_lc.contains(query_lc)
        || (!tokens.is_empty() && tokens.iter().all(|t| field_lc.contains(t)))
}

/// The local name of an IRI: the tail after the last `/` or `#` (the whole string when
/// neither is present).
fn iri_local_name(iri: &str) -> &str {
    match iri.rfind(['/', '#']) {
        Some(i) => &iri[i + 1..],
        None => iri,
    }
}

/// Search the bundle's `gmeow:graph/documentation` projection for documentation-entry
/// records whose label / definition / advisory prose match `query`, returning up to
/// `limit` ranked [`SearchHit`]s.
///
/// The tool queries the documentation NAMED GRAPH — always present in every bundle —
/// NOT any packed `search-index.json` archive member (the lean `gmeow.gts` carries
/// none). An ABSENT/EMPTY documentation graph is therefore a HARD FAIL (a real defect,
/// never a silent empty result); a query that legitimately matches nothing returns an
/// empty vector (empty-but-ok is the caller's `{"ok":true,"results":[]}`).
///
/// Ranking is deterministic: records are ordered by match quality (a label match
/// outranks a definition match, which outranks an advice-only match) then by `id`
/// (the documented IRI), so the same query twice yields the same order.
fn search_documentation(
    docs: &Arc<purrdf::RdfDataset>,
    query: &str,
    limit: usize,
) -> gmeow_errors::Result<Vec<SearchHit>> {
    // HARD FAIL: the documentation graph is absent/empty in this bundle. docs_search
    // serves the documentation graph, so a missing graph is a genuine defect.
    if docs.quad_count() == 0 {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "docs_search: the gmeow:graph/documentation named graph is absent or empty \
                      in this bundle — the searchable documentation projection is missing"
                .to_string(),
        }));
    }
    let value = sparql_result_to_json(crate::stages::native_query::query(docs, DOC_SEARCH_QUERY)?)?;

    // Aggregate the (repeatable advice/alignment/missing) rows back per entry subject.
    #[derive(Default)]
    struct Acc {
        kind: String,
        id: String,
        label: String,
        definition: Option<String>,
        url: Option<String>,
        advice: BTreeSet<String>,
        alignments: BTreeSet<String>,
        missing: BTreeSet<String>,
    }
    let mut by_subject: BTreeMap<String, Acc> = BTreeMap::new();
    for row in select_rows(&value) {
        let (Some(subject), Some(ty), Some(label)) =
            (row.get("s"), row.get("type"), row.get("label"))
        else {
            continue;
        };
        let kind = match iri_local_name(ty) {
            "DocumentedTerm" => "term",
            "DocumentedSlice" => "slice",
            "DocumentedConcern" => "concern",
            // Any other typed subject is not a searchable documentation-entry record.
            _ => continue,
        };
        let acc = by_subject.entry(subject.clone()).or_default();
        acc.kind = kind.to_string();
        acc.label = label.clone();
        acc.id = row
            .get("documents")
            .cloned()
            .unwrap_or_else(|| subject.clone());
        if let Some(def) = row.get("definition") {
            acc.definition = Some(def.clone());
        }
        if let Some(url) = row.get("url") {
            acc.url = Some(url.clone());
        }
        if let Some(advice) = row.get("advice") {
            acc.advice.insert(advice.clone());
        }
        if let Some(alignment) = row.get("alignment") {
            acc.alignments.insert(alignment.clone());
        }
        if let Some(missing) = row.get("missing") {
            acc.missing.insert(iri_local_name(missing).to_string());
        }
    }

    let query_lc = query.to_lowercase();
    let tokens: Vec<&str> = query_lc.split_whitespace().collect();

    let mut hits: Vec<SearchHit> = Vec::new();
    for acc in by_subject.into_values() {
        let label_lc = acc.label.to_lowercase();
        let def_lc = acc.definition.as_deref().unwrap_or("").to_lowercase();
        let advice_lc = acc
            .advice
            .iter()
            .map(|a| a.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        // Rank by the strongest matching field (label > definition > advice); a record
        // matching none is not a hit.
        let rank = if field_matches(&label_lc, &query_lc, &tokens) {
            0
        } else if field_matches(&def_lc, &query_lc, &tokens) {
            1
        } else if field_matches(&advice_lc, &query_lc, &tokens) {
            2
        } else {
            continue;
        };
        hits.push(SearchHit {
            kind: acc.kind,
            id: acc.id,
            label: acc.label,
            definition: acc.definition,
            url: acc.url,
            advice: acc.advice.into_iter().collect(),
            alignments: acc.alignments.into_iter().collect(),
            missing_coverage: acc.missing.into_iter().collect(),
            rank,
        });
    }
    // Deterministic order: best match first, then by documented IRI as a stable
    // tie-break.
    hits.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id)));
    hits.truncate(limit);
    Ok(hits)
}

/// Extract the `results.bindings` of a SPARQL-1.1 JSON envelope into flat
/// `var → lexical-value` rows (each binding's `"value"`). Shared by
/// [`McpView::docs_select_rows`] and [`search_documentation`].
fn select_rows(value: &Value) -> Vec<BTreeMap<String, String>> {
    value
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(Value::as_array)
        .map(|bindings| {
            bindings
                .iter()
                .filter_map(Value::as_object)
                .map(|binding| {
                    binding
                        .iter()
                        .filter_map(|(var, cell)| {
                            cell.get("value")
                                .and_then(Value::as_str)
                                .map(|v| (var.clone(), v.to_owned()))
                        })
                        .collect::<BTreeMap<String, String>>()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The string value of `key` in a JSON object, or `""` when absent / non-string.
/// A small adapter so the full-tier `doc_card` panels can reuse the Task-4
/// `term_entailments` / `term_fixtures` JSON records without re-querying the graph.
fn value_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The string members of the `key` array in a JSON object, in order; empty when
/// absent or not an array. (Values are already deterministically ordered upstream.)
fn value_str_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Render one fixture record as the tool's JSON object. The advisory fields are
/// emitted as `null` when the slice authored none (a stable shape callers can read
/// without probing for key presence).
fn fixture_json(view: &FixtureView) -> Value {
    json!({
        "title": view.title,
        "text": view.text,
        "expected_outcome": view.expected_outcome,
        "violation_code": view.violation_code,
        "rationale": view.rationale,
    })
}

/// Render one competency-question record as the tool's JSON object. The optional
/// expectations are included only when authored; the row-count and exact-flag are
/// typed back to a JSON number / boolean from their `xsd:integer` / `xsd:boolean`
/// lexical forms (the raw lexeme is kept only if it is somehow not well-typed).
fn competency_json(query_text: &str, row: &BTreeMap<String, String>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("query_text".to_string(), json!(query_text));
    if let Some(rationale) = row.get("rationale") {
        obj.insert("rationale".to_string(), json!(rationale));
    }
    if let Some(count) = row.get("count") {
        let value = count
            .parse::<i64>()
            .map(|n| json!(n))
            .unwrap_or_else(|_| json!(count));
        obj.insert("expected_row_count".to_string(), value);
    }
    if let Some(exact) = row.get("exact") {
        let value = match exact.as_str() {
            "true" | "1" => json!(true),
            "false" | "0" => json!(false),
            other => json!(other),
        };
        obj.insert("exact_rows".to_string(), value);
    }
    Value::Object(obj)
}

/// A loaded, bundle-backed view over the GMEOW snapshot for the MCP consumer.
pub struct McpView {
    /// THIS server's view of the bundled snapshot as the native carrier dataset:
    /// the MCP server is a gts ARCHIVE CONSUMER — it imports `gmeow.gts` to
    /// the carrier representation ONCE and serves every surface off the shared export
    /// `FoldView`, exactly as the in-pipeline export leaf does.
    dataset: Arc<purrdf::RdfDataset>,
    /// Ontology title / version — language-independent (`fold_meta` reads the
    /// header via a token-minimal `value`, not a language selector), so they are
    /// resolved once at construction.
    title: String,
    version: String,
    /// `requested.join(",")` → collected terms, mirroring `_TERMS_CACHE`. Stored
    /// behind an `Arc` so the cache mutex is released before the (potentially
    /// large) render runs — concurrent reads of a cached entry never serialize
    /// behind one another's rendering.
    cache: Mutex<HashMap<String, Arc<Vec<Term>>>>,
    /// `term-IRI → published site URL`, built once from the
    /// `gmeow:graph/documentation` graph — language-independent, so it is cached
    /// across all `requested` lists. Empty when the doc graph is absent (then the
    /// `llms.txt` index renders linkless).
    doc_urls: OnceLock<Arc<HashMap<String, String>>>,
    /// The documentation named graph projected to a default-graph dataset once per
    /// server. Every documentation tool and full-card panel queries this immutable
    /// view; rebuilding it for each SPARQL query copies the same whole graph and turns
    /// a single card into several bundle-scale scans.
    documentation: OnceLock<Arc<purrdf::RdfDataset>>,
    /// The JSON Schema `$defs` key set folded into this bundle's `schemas-archive`
    /// blob — the model-existence signal `export::term_to_card`'s `python_model`
    /// gate reads (built once from `gts`, like `doc_urls`; see [`Self::modeled_defs`]).
    modeled_defs: OnceLock<Arc<BTreeSet<String>>>,
    /// The raw `gmeow.gts` snapshot bytes this view was imported from, retained
    /// verbatim. `from_snapshot` parses the bundle to the carrier dataset and
    /// then DISCARDS the bytes; the native validation surface
    /// (`validate_local`) needs them back — `gmeow_validate::data_validate` reads
    /// the folded `shapes-archive` blob directly from the raw GTS, not from the
    /// parsed dataset. Held behind an `Arc<[u8]>` so cloning the view is cheap
    /// and the (potentially large) bundle is never copied.
    gts: Arc<[u8]>,
}

impl McpView {
    fn from_dataset(
        dataset: Arc<purrdf::RdfDataset>,
        gts: Arc<[u8]>,
    ) -> gmeow_errors::Result<Self> {
        let (title, version) = {
            let view = FoldView::new(dataset.as_ref());
            export::fold_meta(&view)?
        };
        Ok(Self {
            dataset,
            title,
            version,
            cache: Mutex::new(HashMap::new()),
            doc_urls: OnceLock::new(),
            documentation: OnceLock::new(),
            modeled_defs: OnceLock::new(),
            gts,
        })
    }

    /// The raw `gmeow.gts` snapshot bytes this view serves, for the native
    /// validation surface that reads the folded `shapes-archive` blob directly.
    fn gts_bytes(&self) -> &[u8] {
        &self.gts
    }

    /// Resolve a CURIE / local name / IRI / unambiguous prefix to its public
    /// metadata record (JSON envelope with `"ok"`), or a not-found envelope.
    fn lookup_term_json(&self, term: &str, requested: Vec<String>) -> String {
        self.with_terms(requested, |terms| export::lookup_envelope(terms, term))
    }

    /// Resolve a CURIE / local name / IRI / label (or unambiguous prefix) to its
    /// canonical term IRI, via the SAME resolution path `lookup_term` / `doc_card`
    /// use. Propagates [`export::ConsumerResolution`] so the caller HARD-FAILS a
    /// cross-namespace collision with a typed ambiguity diagnostic and an unknown
    /// term with the unknown-term diagnostic — never a fabricated empty result or a
    /// silent pick. `export::resolve_term_iri` borrows zero-copy; this wrapper must
    /// allocate ONE owned `String` here because the resolved IRI has to outlive the
    /// cached `terms` slice that `with_terms` only lends for the closure's duration.
    fn resolve_term_iri(&self, term: &str, requested: Vec<String>) -> ConsumerResolution<String> {
        self.with_terms(requested, |terms| {
            match export::resolve_term_iri(terms, term) {
                ConsumerResolution::Resolved(iri) => ConsumerResolution::Resolved(iri.to_owned()),
                ConsumerResolution::Ambiguous { candidates } => {
                    ConsumerResolution::Ambiguous { candidates }
                }
                ConsumerResolution::NotFound => ConsumerResolution::NotFound,
            }
        })
    }

    /// The counter-example / well-formed conformance fixtures documenting `term_iri`,
    /// read from the `gmeow:graph/documentation` projection and split by
    /// `gmeow:docFixtureKind`. Each fixture is one record carrying its title, full
    /// Turtle body, and the authored advisory fields (expected outcome, violation
    /// code, conformance rationale) — `null` when the slice authored none. Both lists
    /// are ordered by fixture IRI, so the surface is deterministic. A term that
    /// documents no fixtures yields two empty lists (honest empty-but-ok).
    fn term_fixtures(&self, term_iri: &str) -> gmeow_errors::Result<(Vec<Value>, Vec<Value>)> {
        let query = fixtures_by_term_query(term_iri);
        // One record per fixture IRI: the fixture-scoped columns are single-valued,
        // so first-row-wins is the whole record. BTreeMap → fixture-IRI order.
        let mut by_fixture: BTreeMap<String, (String, FixtureView)> = BTreeMap::new();
        for row in self.docs_select_rows(&query)? {
            let (Some(fixture), Some(kind), Some(text)) =
                (row.get("f"), row.get("kind"), row.get("text"))
            else {
                continue;
            };
            by_fixture.entry(fixture.clone()).or_insert_with(|| {
                (
                    kind.clone(),
                    FixtureView {
                        title: row.get("label").cloned().unwrap_or_else(|| fixture.clone()),
                        text: text.clone(),
                        expected_outcome: row.get("outcome").cloned(),
                        violation_code: row.get("code").cloned(),
                        rationale: row.get("rationale").cloned(),
                    },
                )
            });
        }

        let wellformed_kind = format!("{GMEOW_NS}docFixtureKindWellformed");
        let counter_kind = format!("{GMEOW_NS}docFixtureKindCounterExample");
        let mut wellformed = Vec::new();
        let mut counter_examples = Vec::new();
        for (kind, view) in by_fixture.into_values() {
            let record = fixture_json(&view);
            if kind == counter_kind {
                counter_examples.push(record);
            } else if kind == wellformed_kind {
                wellformed.push(record);
            }
        }
        Ok((wellformed, counter_examples))
    }

    /// The reasoner entailments documenting `term_iri`, read from the
    /// `gmeow:graph/documentation` projection. Each `gmeow:Entailment` node is one
    /// record — its rule, its conclusion, and ALL its premises (every
    /// `gmeow:entailmentPremise`, sorted). Records are ordered by entailment IRI, so
    /// the surface is deterministic. A term with no entailments yields an empty list.
    fn term_entailments(&self, term_iri: &str) -> gmeow_errors::Result<Vec<Value>> {
        let query = entailments_by_term_query(term_iri);
        // Aggregate the (repeatable-premise) rows back per entailment IRI.
        let mut by_entailment: BTreeMap<String, (String, String, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(&query)? {
            let (Some(entailment), Some(rule), Some(conclusion)) =
                (row.get("e"), row.get("rule"), row.get("conclusion"))
            else {
                continue;
            };
            let entry = by_entailment
                .entry(entailment.clone())
                .or_insert_with(|| (rule.clone(), conclusion.clone(), BTreeSet::new()));
            if let Some(premise) = row.get("premise") {
                entry.2.insert(premise.clone());
            }
        }
        let out = by_entailment
            .into_values()
            .map(|(rule, conclusion, premises)| {
                json!({
                    "rule": rule,
                    "conclusion": conclusion,
                    "premises": premises.into_iter().collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(out)
    }

    /// The runnable competency questions from the `gmeow:graph/documentation`
    /// projection: every `gmeow:DocumentedCompetency` carrying a `gmeow:cqQueryText`,
    /// or — when `term_iri` is `Some` — only those documenting that term. Each record
    /// carries the runnable query text and the authored expectations
    /// (`gmeow:cqRationale`, `gmeow:cqExpectRowCount`, `gmeow:cqExactRows`) when
    /// present. Records are ordered by competency IRI, so the surface is
    /// deterministic.
    fn competency_questions(&self, term_iri: Option<&str>) -> gmeow_errors::Result<Vec<Value>> {
        let query = competency_query(term_iri);
        // One record per competency IRI (all columns single-valued). BTreeMap →
        // competency-IRI order.
        let mut by_competency: BTreeMap<String, Value> = BTreeMap::new();
        for row in self.docs_select_rows(&query)? {
            let (Some(competency), Some(query_text)) = (row.get("c"), row.get("q")) else {
                continue;
            };
            by_competency
                .entry(competency.clone())
                .or_insert_with(|| competency_json(query_text, &row));
        }
        Ok(by_competency.into_values().collect())
    }

    /// The standard llmstxt.org vocabulary index (`llms.txt`) for `requested`,
    /// with bullets linking into the published docs site.
    fn llms_txt_text(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        let doc_urls = self.doc_urls();
        self.with_terms(requested, |terms| {
            export::consumer_llms_txt(terms, &title, &version, &doc_urls)
        })
    }

    /// The complete inlined index (`llms-full.txt`) for `requested`.
    fn llms_full_text(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        let modeled_defs = self.modeled_defs();
        self.with_terms(requested, |terms| {
            export::consumer_llms_full(terms, &title, &version, &modeled_defs)
        })
    }

    /// A prompt-ready term card for one term at the requested detail tier and
    /// output format, wrapped in the cost-metadata envelope
    /// (`{"ok":true,"detail","format","bytes","tokens","card"}`).
    ///
    /// The card is built through the SINGLE shared builder + renderer
    /// (`export::doc_card_build` → `gmeow_docs::card`). For
    /// [`CardDetail::Full`](gmeow_docs::card::CardDetail::Full) the rich oracle
    /// panels (entailments, Do / Don't fixtures, diagnostics, projection loss) are
    /// populated by querying the `gmeow:graph/documentation` projection for the
    /// resolved term IRI. An UNKNOWN term is a HARD FAIL (`Err`).
    ///
    /// `format=markdown` renders through `render_card` (tier-gated); `format=json`
    /// serializes the tier-projected `Card`. `bytes`/`tokens` measure the returned
    /// card payload so callers can budget by tier.
    fn doc_card(
        &self,
        term: &str,
        detail: gmeow_docs::card::CardDetail,
        format: CardFormat,
        requested: Vec<String>,
    ) -> gmeow_errors::Result<String> {
        use gmeow_docs::card::CardDetail;
        let modeled_defs = self.modeled_defs();
        let built = self.with_terms(requested, |terms| {
            export::doc_card_build(terms, term, &modeled_defs)
        });
        let (title, mut card) = match built {
            ConsumerResolution::Resolved(pair) => pair,
            ConsumerResolution::Ambiguous { candidates } => {
                return Err(ambiguous_term_err(term, &candidates));
            }
            ConsumerResolution::NotFound => return Err(unknown_term_err(term)),
        };
        // The full tier is the oracle card: enrich the compact card with the rich
        // panels queried from the documentation graph by the resolved term IRI.
        if detail == CardDetail::Full {
            let iri = card.iri.clone();
            let (fixtures_do, fixtures_dont) = self.card_fixtures(&iri)?;
            card.entailments = self.card_entailments(&iri)?;
            card.fixtures_do = fixtures_do;
            card.fixtures_dont = fixtures_dont;
            card.diagnostics = self.card_diagnostics(&iri)?;
            card.loss = self.card_loss(&iri)?;
        }
        let (rendered, card_value) = match format {
            CardFormat::Markdown => {
                let md = gmeow_docs::card::render_card(&title, &card, detail);
                let value = Value::String(md.clone());
                (md, value)
            }
            CardFormat::Json => {
                let projected = card.projected(detail);
                let js = serde_json::to_string(&projected).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: format!("doc_card: serialize card to JSON: {e}"),
                    })
                })?;
                let value = serde_json::to_value(&projected).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: format!("doc_card: card to JSON value: {e}"),
                    })
                })?;
                (js, value)
            }
        };
        Ok(json!({
            "ok": true,
            "detail": card_detail_label(detail),
            "format": format.label(),
            "bytes": rendered.len(),
            "tokens": gmeow_docs::llms::estimate_tokens(&rendered),
            "card": card_value,
        })
        .to_string())
    }

    /// The full-tier entailment panel for `term_iri`: the reasoner derivations
    /// documenting the term, mapped from the SAME `term_entailments` query the
    /// `entailments` tool serves. Empty for a term with no derivations.
    fn card_entailments(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<Vec<gmeow_docs::card::CardEntailment>> {
        Ok(self
            .term_entailments(term_iri)?
            .iter()
            .map(|v| gmeow_docs::card::CardEntailment {
                rule: value_str(v, "rule"),
                conclusion: value_str(v, "conclusion"),
                premises: value_str_array(v, "premises"),
            })
            .collect())
    }

    /// The full-tier Do / Don't fixture panels for `term_iri`: the well-formed
    /// exemplars and the counter-examples, mapped from the SAME `term_fixtures`
    /// query the `counter_examples` tool serves. Each fixture body is one-lined and
    /// capped to a short snippet (the full body stays available via
    /// `counter_examples`). Both empty for a term documenting no fixtures.
    fn card_fixtures(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<(
        Vec<gmeow_docs::card::CardFixture>,
        Vec<gmeow_docs::card::CardFixture>,
    )> {
        let to_card = |v: &Value| gmeow_docs::card::CardFixture {
            title: value_str(v, "title"),
            body: fixture_body_snippet(&value_str(v, "text")),
        };
        let (wellformed, counter_examples) = self.term_fixtures(term_iri)?;
        Ok((
            wellformed.iter().map(&to_card).collect(),
            counter_examples.iter().map(&to_card).collect(),
        ))
    }

    /// The full-tier diagnostics panel for `term_iri`: the finding codes the term
    /// may hit, read from the `gmeow:docEvidenceKindDiagnostics` evidence in the
    /// documentation graph. Rows are ordered by finding code, so the panel is
    /// deterministic. Empty for a term no finding concerns.
    fn card_diagnostics(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<Vec<gmeow_docs::card::CardDiagnostic>> {
        let mut by_code: BTreeMap<String, String> = BTreeMap::new();
        for row in self.docs_select_rows(&diagnostics_by_term_query(term_iri))? {
            let (Some(code), Some(claim)) = (row.get("code"), row.get("claim")) else {
                continue;
            };
            by_code.entry(code.clone()).or_insert_with(|| claim.clone());
        }
        Ok(by_code
            .into_iter()
            .map(|(code, note)| gmeow_docs::card::CardDiagnostic { code, note })
            .collect())
    }

    /// The full-tier projection-loss panel for `term_iri`: the targets the term
    /// degrades into and each degradation's preservation judgment, read from the
    /// `gmeow:docEvidenceKindLoss` evidence in the documentation graph. Rows are
    /// ordered by target, so the panel is deterministic. Empty for a term that
    /// degrades under no projection.
    fn card_loss(&self, term_iri: &str) -> gmeow_errors::Result<Vec<gmeow_docs::card::CardLoss>> {
        let mut by_target: BTreeMap<String, String> = BTreeMap::new();
        for row in self.docs_select_rows(&loss_by_term_query(term_iri))? {
            let (Some(target), Some(judgment)) = (row.get("target"), row.get("judgment")) else {
                continue;
            };
            by_target
                .entry(target.clone())
                .or_insert_with(|| judgment.clone());
        }
        Ok(by_target
            .into_iter()
            .map(|(target, preservation)| gmeow_docs::card::CardLoss {
                target,
                preservation,
            })
            .collect())
    }

    /// The OKF manifest JSON envelope for `requested`.
    fn okf_index_json(&self, requested: Vec<String>) -> String {
        self.with_terms(requested, export::okf_index_envelope)
    }

    /// Run a SELECT / ASK SPARQL query over the `gmeow:graph/documentation` named
    /// graph (re-rooted to the default graph so a plain query with no `GRAPH`
    /// clause reaches it), returning a standard SPARQL-1.1 JSON-results envelope
    /// under `"ok"`. CONSTRUCT / DESCRIBE are rejected — the tool serves one result
    /// shape (bindings or a boolean), never a graph.
    fn query_docs_json(&self, sparql: &str) -> String {
        match self.run_docs_query(sparql) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    fn run_docs_query(&self, sparql: &str) -> gmeow_errors::Result<Value> {
        let docs = self.documentation();
        let result = crate::stages::native_query::query(docs, sparql)?;
        sparql_result_to_json(result)
    }

    /// Run a SELECT over the documentation graph and return its rows as flat
    /// `var → lexical-value` maps (the `results.bindings` of the SPARQL-1.1 JSON
    /// envelope, with each binding's `"value"` extracted). A missing/optional
    /// variable is simply absent from a row's map.
    fn docs_select_rows(
        &self,
        sparql: &str,
    ) -> gmeow_errors::Result<Vec<BTreeMap<String, String>>> {
        let value = self.run_docs_query(sparql)?;
        Ok(select_rows(&value))
    }

    /// Build the two fixture correspondence maps from the `gmeow:graph/documentation`
    /// projection: `finding-code → counter-example` (keyed on the authored
    /// `gmeow:docViolationCode`) and `finding-code → positive exemplar` (the
    /// well-formed sibling joined through a shared referenced term). Both are
    /// deterministic: a code that repeats across fixtures resolves to the
    /// lexicographically-first fixture IRI, and the well-formed sibling is the
    /// lexicographically-first well-formed fixture sharing a referenced term.
    fn fixture_maps(
        &self,
    ) -> gmeow_errors::Result<(BTreeMap<String, FixtureView>, BTreeMap<String, FixtureView>)> {
        // Counter-example fixtures: aggregate the (possibly multi-`documents`) rows
        // back per fixture IRI so a fixture is one record with its full referenced-term
        // set. BTreeMap keeps the fixture IRIs sorted → deterministic first-wins.
        let mut counter_by_fixture: BTreeMap<String, (FixtureView, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(COUNTER_EXAMPLE_FIXTURE_QUERY)? {
            let (Some(fixture), Some(code), Some(text)) =
                (row.get("f"), row.get("code"), row.get("text"))
            else {
                continue;
            };
            let entry = counter_by_fixture
                .entry(fixture.clone())
                .or_insert_with(|| {
                    (
                        FixtureView {
                            title: row.get("label").cloned().unwrap_or_else(|| fixture.clone()),
                            text: text.clone(),
                            expected_outcome: row.get("outcome").cloned(),
                            violation_code: Some(code.clone()),
                            rationale: row.get("rationale").cloned(),
                        },
                        BTreeSet::new(),
                    )
                });
            if let Some(term) = row.get("term") {
                entry.1.insert(term.clone());
            }
        }

        // Well-formed fixtures: same aggregation, no violation code (they violate
        // nothing).
        let mut wellformed_by_fixture: BTreeMap<String, (FixtureView, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(WELLFORMED_FIXTURE_QUERY)? {
            let (Some(fixture), Some(text)) = (row.get("f"), row.get("text")) else {
                continue;
            };
            let entry = wellformed_by_fixture
                .entry(fixture.clone())
                .or_insert_with(|| {
                    (
                        FixtureView {
                            title: row.get("label").cloned().unwrap_or_else(|| fixture.clone()),
                            text: text.clone(),
                            expected_outcome: row.get("outcome").cloned(),
                            violation_code: None,
                            rationale: row.get("rationale").cloned(),
                        },
                        BTreeSet::new(),
                    )
                });
            if let Some(term) = row.get("term") {
                entry.1.insert(term.clone());
            }
        }

        // code → counter-example (first fixture IRI wins) + the code's referenced terms.
        let mut counter_examples_by_code: BTreeMap<String, FixtureView> = BTreeMap::new();
        let mut terms_by_code: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (view, terms) in counter_by_fixture.values() {
            let Some(code) = view.violation_code.clone() else {
                continue;
            };
            counter_examples_by_code
                .entry(code.clone())
                .or_insert_with(|| view.clone());
            // Terms of the CHOSEN (first) counter-example anchor the well-formed join.
            terms_by_code.entry(code).or_insert_with(|| terms.clone());
        }

        // code → well-formed sibling with the largest referenced-term overlap
        // (most specific/relevant exemplar); ties broken deterministically by
        // the lexicographically smallest fixture IRI. `min_by_key` is used
        // (rather than `max_by_key`, which returns the LAST maximal element on
        // ties) with a key of `(Reverse(overlap), fixture_iri)` — since fixture
        // IRIs are unique, this always has a single minimum, so the choice is
        // fully deterministic and stable across runs.
        let mut wellformed_by_code: BTreeMap<String, FixtureView> = BTreeMap::new();
        for (code, ce_terms) in &terms_by_code {
            let best = wellformed_by_fixture
                .iter()
                .filter_map(|(fixture, (view, wf_terms))| {
                    let overlap = wf_terms.intersection(ce_terms).count();
                    (overlap > 0).then_some((overlap, fixture, view))
                })
                .min_by_key(|(overlap, fixture, _)| (Reverse(*overlap), (*fixture).clone()));
            if let Some((_overlap, _fixture, view)) = best {
                wellformed_by_code.insert(code.clone(), view.clone());
            }
        }

        Ok((counter_examples_by_code, wellformed_by_code))
    }

    /// Build `term-IRI → entailments` from the documentation graph's entailment
    /// records (`gmeow:Entailment`), aggregating each record's premises and grouping
    /// by the `gmeow:documents` term. Entailment records are iterated in sorted IRI
    /// order and each entailment's premises are sorted, so the map is deterministic.
    fn entailment_map(&self) -> gmeow_errors::Result<BTreeMap<String, Vec<EntailmentView>>> {
        // Aggregate the (possibly multi-premise) rows back per entailment IRI.
        let mut by_entailment: BTreeMap<String, (String, String, String, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(ENTAILMENT_QUERY)? {
            let (Some(entailment), Some(term), Some(rule), Some(conclusion)) = (
                row.get("e"),
                row.get("term"),
                row.get("rule"),
                row.get("conclusion"),
            ) else {
                continue;
            };
            let entry = by_entailment.entry(entailment.clone()).or_insert_with(|| {
                (
                    term.clone(),
                    rule.clone(),
                    conclusion.clone(),
                    BTreeSet::new(),
                )
            });
            if let Some(premise) = row.get("premise") {
                entry.3.insert(premise.clone());
            }
        }

        let mut entailments_by_term: BTreeMap<String, Vec<EntailmentView>> = BTreeMap::new();
        for (term, rule, conclusion, premises) in by_entailment.into_values() {
            entailments_by_term
                .entry(term)
                .or_default()
                .push(EntailmentView {
                    rule,
                    conclusion,
                    premises: premises.into_iter().collect(),
                });
        }
        Ok(entailments_by_term)
    }

    /// Run a SELECT / ASK SPARQL query over the bundle canon UNIONED with a
    /// READ-ONLY external overlay loaded from `overlay_path`, returning a standard
    /// SPARQL-1.1 JSON-results envelope under `"ok"`. See [`Self::run_local_query`]
    /// for the read-only / external-provenance contract.
    fn query_local_json(&self, overlay_path: &str, sparql: &str) -> String {
        match self.run_local_query(overlay_path, sparql) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Query `bundle ∪ overlay` where `overlay` is the user's LOCAL lower-tier
    /// graph file, loaded as a READ-ONLY external annex.
    ///
    /// CONTRACT (enforced here, not just documented):
    /// * the overlay is loaded into its own transient dataset from `overlay_path`
    ///   — the signed canon (`self.dataset`) is pushed VERBATIM and is NEVER
    ///   mutated;
    /// * every overlay triple is re-homed under the distinct external-provenance
    ///   graph [`EXTERNAL_OVERLAY_GRAPH`] (the origin marker), so external content
    ///   stays isolable via a `GRAPH` clause and is NEVER unioned into the signed
    ///   `gmeow:` canon graphs;
    /// * a default-graph copy makes reads see `bundle ∪ overlay`, but the whole
    ///   union is transient and discarded after the query — it is NEVER persisted,
    ///   NEVER folded into `gmeow.gts`, and NEVER written back to the canon or the
    ///   overlay file (the memory-write triad only ever touches `memory.gts`);
    /// * only SELECT / ASK are accepted (CONSTRUCT / DESCRIBE are rejected).
    fn run_local_query(&self, overlay_path: &str, sparql: &str) -> gmeow_errors::Result<Value> {
        let path = Path::new(overlay_path);
        // The media type is the file extension (ttl/nt/nq/trig/rdf/owl/xml/…); an
        // unknown extension HARD-FAILS in `parse_dataset` (no silent fallback).
        let media = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "text/turtle".to_string());
        let bytes = fs::read(path).with_ctx(|| format!("read overlay {overlay_path}"))?;
        let overlay = purrdf::parse_dataset(&bytes, &media, None)
            .with_ctx(|| format!("parse overlay {overlay_path}"))?;

        let mut builder = purrdf::RdfDatasetBuilder::new();
        // The signed canon — verbatim, never mutated.
        builder.push_dataset(self.dataset.as_ref());
        let external = purrdf::RdfTerm::Iri(EXTERNAL_OVERLAY_GRAPH.to_string());
        for quad in overlay.owned_quads() {
            // Default-graph copy → a plain query reads `bundle ∪ overlay`.
            let mut in_default = quad.clone();
            in_default.graph_name = None;
            builder.push_owned_quad(&in_default);
            // Origin-marked copy → external provenance, isolable via GRAPH.
            let mut tagged = quad;
            tagged.graph_name = Some(external.clone());
            builder.push_owned_quad(&tagged);
        }
        let dataset = builder.freeze()?;
        let result = crate::stages::native_query::query(&dataset, sparql)?;
        sparql_result_to_json(result)
    }

    /// Run the native reasoned-graph verify over the bundle canon UNIONED with a
    /// READ-ONLY external overlay loaded from `overlay_path`, returning the
    /// proof-carrying JSON envelope under `"ok"`. See [`Self::run_verify_graph`] for
    /// the read-only / external-annex contract and the completeness-gate judgment.
    fn verify_graph_json(&self, overlay_path: &str, budget: &Budget) -> String {
        match self.run_verify_graph(overlay_path, budget) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Reason-and-verify `bundle ∪ overlay`, where `overlay` is the user's LOCAL
    /// lower-tier graph file, loaded as a READ-ONLY external annex, then return a
    /// PROOF-CARRYING judgment: the completeness-gated coherence class, the two
    /// completeness/evaluation axes, the reasoned-graph verify findings, the cited
    /// IRIs, and the grounded `logic:ReasoningResult` N-Quads.
    ///
    /// CONTRACT (enforced here, not just documented) — the SAME overlay discipline as
    /// [`Self::run_local_query`]:
    /// * the overlay is loaded into its own transient dataset from `overlay_path`;
    ///   the signed canon (`self.dataset`) is pushed VERBATIM and is NEVER mutated;
    /// * every overlay quad is dual-copied — one into the default graph (so the DL
    ///   calculus and the flat verify queries read `bundle ∪ overlay`) and one
    ///   re-homed under the external-provenance graph [`EXTERNAL_OVERLAY_GRAPH`] — but
    ///   the whole union is transient and DROPPED after the call: never persisted,
    ///   never folded into `gmeow.gts`, never written back to the canon or the overlay;
    /// * the forward closure runs THROUGH [`reason_all_budgeted`] (the mid-chase step
    ///   governor), never the unbudgeted [`gmeow_logic::reason::reason_all`], so an
    ///   agent-influenced union can never run an unbounded Turing-complete closure;
    /// * an overlay whose file size exceeds [`MAX_VERIFY_OVERLAY_BYTES`] is a HARD FAIL
    ///   BEFORE it is even `fs::read` into memory — a stat, not a read, refuses an
    ///   oversized file (or a file with one enormous literal) before the bytes ever
    ///   land in the process;
    /// * an overlay exceeding [`MAX_VERIFY_OVERLAY_QUADS`] is a HARD FAIL BEFORE any
    ///   reasoning (the bounded agent path), never a truncated graph.
    ///
    /// # Completeness-gate judgment (`class_local_name`)
    ///
    /// The class is `completeness_class`'s T2 completeness-gate trichotomy — itself a
    /// thin wrapper over [`CoherenceOutcome::class_local_name_for`], the SAME gate
    /// the bundle-level coherence certifier (`certificate.rs`) uses, so this tool and
    /// [`Self::run_explain_quad`] can never diverge:
    /// * a witnessed DL contradiction (the forbidden glut under the default
    ///   forbid-glut policy) REFUTES coherence — but only a CONCLUSIVE check flatly
    ///   `Refused`s; a budget-cut check that ran into it can only attest;
    /// * else a CONCLUSIVE ([`ReasoningResult::is_conclusive`]) violation-free closure
    ///   with a NAMED certified fragment `CoherenceCertificate`s;
    /// * else (a budget-cut / non-conclusive closure, or a conclusive one naming no
    ///   certified fragment) the strongest honest claim is the strictly-weaker
    ///   `CoherenceCheckAttestation` — NEVER a certificate.
    ///
    /// # The `coherent` boolean agrees with `class_local_name` by construction
    ///
    /// `coherent` is `!completeness_refused(&result) && report.ok()` — the SAME
    /// `CoherenceOutcome` gate that decides `class_local_name`, ANDed with the
    /// bad-example verify findings. It is NEVER `report.ok()` alone: a conclusive DL
    /// glut that happens not to trip any bad-example verify query would otherwise
    /// leave `report.ok()` true while `class_local_name` reads `"Refused"` — a
    /// self-contradictory envelope. Routing both fields through one shared outcome
    /// makes `coherent:true` alongside `class_local_name:"Refused"` unrepresentable.
    ///
    /// [`ReasoningResult::is_conclusive`]: gmeow_logic::result::ReasoningResult::is_conclusive
    fn run_verify_graph(&self, overlay_path: &str, budget: &Budget) -> gmeow_errors::Result<Value> {
        let path = Path::new(overlay_path);
        // Media from the file extension (ttl/nt/nq/trig/rdf/…); an unknown extension
        // HARD-FAILS in `parse_dataset` (no silent fallback), exactly as query_local.
        let media = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "text/turtle".to_string());

        // Pre-READ hard bound: stat (not read) the file and refuse an oversized
        // overlay BEFORE it is ever loaded into memory. Statting is O(1) — it never
        // touches the file's contents — so a huge file (or one enormous literal) is
        // refused without the multi-megabyte `fs::read` + parse the quad ceiling below
        // would otherwise require to even measure it.
        let overlay_bytes = fs::metadata(path)
            .with_ctx(|| format!("stat overlay {overlay_path}"))?
            .len();
        if overlay_bytes > MAX_VERIFY_OVERLAY_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "verify_graph: overlay {overlay_path} is {overlay_bytes} bytes, \
                     exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte ceiling BEFORE any read; \
                     split the annex and verify the parts (no silent truncation)"
                ),
            }));
        }

        let bytes = fs::read(path).with_ctx(|| format!("read overlay {overlay_path}"))?;
        let overlay = purrdf::parse_dataset(&bytes, &media, None)
            .with_ctx(|| format!("parse overlay {overlay_path}"))?;

        // Pre-reasoning hard bound: refuse an oversized overlay before reasoning.
        let overlay_quads = overlay.quad_count();
        if overlay_quads > MAX_VERIFY_OVERLAY_QUADS {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "verify_graph: overlay {overlay_path} carries {overlay_quads} quads, \
                     exceeding the {MAX_VERIFY_OVERLAY_QUADS} quad ceiling; split the annex \
                     and verify the parts (no silent truncation)"
                ),
            }));
        }

        // Build the transient union EXACTLY as run_local_query: canon verbatim, then
        // each overlay quad dual-copied (default graph + external-provenance graph).
        let mut builder = purrdf::RdfDatasetBuilder::new();
        builder.push_dataset(self.dataset.as_ref());
        let external = purrdf::RdfTerm::Iri(EXTERNAL_OVERLAY_GRAPH.to_string());
        for quad in overlay.owned_quads() {
            let mut in_default = quad.clone();
            in_default.graph_name = None;
            builder.push_owned_quad(&in_default);
            let mut tagged = quad;
            tagged.graph_name = Some(external.clone());
            builder.push_owned_quad(&tagged);
        }
        let union = builder.freeze()?;

        // GOVERNED closure — reason_all_budgeted CUTS the forward chase mid-flight; a
        // budget-cut returns a non-conclusive BudgetExhausted/Incomplete/Undetermined
        // verdict (never a wrong `supported`/`both`). NEVER reason_all.
        let result = reason_all_budgeted(&union, budget)?;
        let report = verify_with_reasoning_result(&union, &result, &embedded_verify_queries())?;

        // The T2 completeness-gate trichotomy over `result` (see the method docs),
        // read entirely through `completeness_class` — the SAME
        // `CoherenceOutcome::class_local_name_for` gate `run_explain_quad` reads, so
        // the two tool paths can never diverge on whether a witnessed DL glut
        // (forbidden under the default forbid-glut policy) refutes coherence. No
        // per-caller downgrade is bolted on here: the gate itself already returns
        // `"Refused"` for a CONCLUSIVE closure that witnesses one, and the
        // strictly-weaker `CoherenceCheckAttestation` for a budget-cut closure that
        // ran into one (it discloses what it found without refuting wholesale).
        let class_local_name = completeness_class(&result);

        // `coherent` MUST be derived from the SAME `completeness_refused` gate that
        // decided `class_local_name`, combined with the bad-example verify findings —
        // never from `report.ok()` alone. A conclusive DL glut that happens to trip
        // no bad-example verify query still leaves `report.ok()` true, but the shared
        // gate REFUSES it; without this combination `coherent:true` could render
        // alongside `class_local_name:"Refused"`, a self-contradictory envelope
        // (CodeRabbit gap G4). `coherent` is true only when BOTH the shared outcome
        // permits it (non-refuting) AND no bad-example finding fired.
        let coherent = !completeness_refused(&result) && report.ok();

        // cited_iris: the DerivationRef cited-IRI surface when the verdict carries a
        // proof/counterproof, unioned with each finding's structured
        // `Finding::cited_iris` (the genuine `TermValue::Iri` bindings the offending
        // SPARQL solution rows carry — see `verify_with_reasoning_result`). NEVER
        // scraped from the rendered `message`/`detail` prose: an agent-controlled
        // overlay literal such as `"see <urn:fake>"` renders inside quotes and must
        // not be mistaken for a citation, which a text-scrape over angle brackets
        // cannot reliably tell apart from a genuine `<iri>` term. Sorted,
        // deduplicated via the BTreeSet.
        let mut cited_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        if let Some(proof) = &result.provenance.proof {
            cited_iris.extend(proof.cited_iris.iter().cloned());
        }
        if let Some(counterproof) = &result.provenance.counterproof {
            cited_iris.extend(counterproof.cited_iris.iter().cloned());
        }
        for finding in &report.findings {
            cited_iris.extend(finding.cited_iris.iter().cloned());
        }

        let findings: Vec<Value> = report
            .findings
            .iter()
            .map(|f| {
                json!({
                    "code": f.code,
                    "severity": f.severity.as_str(),
                    "message": f.message,
                    "detail": f.detail,
                })
            })
            .collect();

        // The grounded RDF judgment: the ReasoningResult node projected to N-Triples
        // (a subset of N-Quads) so an agent can reason over the verdict itself — its
        // completeness/evaluation axes and consumed budget ride the node.
        let judgment_nquads = project_reasoning_result(&result);

        Ok(json!({
            "ok": true,
            "class_local_name": class_local_name,
            "completeness": result.completeness.wire(),
            "evaluation": result.evaluation.wire(),
            "error_count": report.error_count(),
            "coherent": coherent,
            "findings": findings,
            "cited_iris": cited_iris.into_iter().collect::<Vec<_>>(),
            "judgment_nquads": judgment_nquads,
        }))
    }

    /// Run [`Self::run_explain_quad`] and render its proof-carrying envelope; an
    /// error (bad object surface, quad-not-in-closure, cross-world ambiguity, or a
    /// faithfulness violation) becomes the `{ok:false, error}` failure envelope,
    /// EXACTLY like [`Self::verify_graph_json`].
    fn explain_quad_json(
        &self,
        subject: &str,
        predicate: &str,
        obj_n3: &str,
        graph: &str,
        budget: &Budget,
    ) -> String {
        match self.run_explain_quad(subject, predicate, obj_n3, graph, budget) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Read the bundle's carried SCOPED COHERENCE CERTIFICATE as a budget-free,
    /// proof-carrying envelope (R6) and render it. BUDGET-FREE and REASON-FREE: the
    /// certificate was computed ONCE at pipeline time and folded into `graph/attestations`;
    /// this reads it straight off the bundled dataset (`self.dataset`) — it NEVER
    /// re-reasons. A bundle carrying no coherence artifact is a HARD FAIL rendered as the
    /// `{ok:false, error}` envelope (there is no silent recompute fallback), EXACTLY like
    /// [`Self::verify_graph_json`].
    fn coherence_certificate_json(&self) -> String {
        match coherence_certificate_envelope(self.dataset.as_ref()) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Reconstruct the FAITHFUL cited-IRI derivation skeleton for ONE arbitrary quad
    /// `(subject, predicate, obj_n3)` in world `graph`, over the bundle's governed
    /// forward closure. DISTINCT from [`Self::explain_finding_json`], which walks the
    /// pre-computed `graph/diagnostics` projection: this tool reconstructs the proof
    /// directly from the reasoner's premise-provenance, for any quad the closure
    /// entails (not only a published finding).
    ///
    /// CONTRACT (enforced here, not just documented):
    /// * the closure runs THROUGH [`reason_all_budgeted`] (the mid-chase step
    ///   governor), never the unbudgeted [`gmeow_logic::reason::reason_all`], so the
    ///   agent-driven target can never trigger an unbounded Turing-complete chase;
    /// * the target is located by its content-addressed reifier, WORLD-DISAMBIGUATED
    ///   by `graph` — a reifier shared across worlds that `graph` does not resolve to
    ///   exactly one row is a HARD FAIL (never an arbitrary pick), and a reifier no
    ///   row carries is a HARD FAIL `quad not in closure` (never an empty-but-ok proof);
    /// * only the SINGLE requested target is reconstructed
    ///   ([`LazyExplanationIndex::explain_one`], never `explain_all`), so the agent
    ///   never offloads whole-closure proof reconstruction onto the tool;
    /// * every cited IRI is re-checked against the full proof trace
    ///   ([`explain::assert_faithful`]) — a fabricated citation cannot escape.
    fn run_explain_quad(
        &self,
        subject: &str,
        predicate: &str,
        obj_n3: &str,
        graph: &str,
        budget: &Budget,
    ) -> gmeow_errors::Result<Value> {
        // The target's content-addressed reifier over the SAME canonical N3 object
        // surface `explain::Row` carries (`term_display`), so it joins the row set.
        let target_reifier = reifier_from_strings(subject, predicate, obj_n3);

        // GOVERNED closure over the whole bundle — reason_all_budgeted CUTS the chase
        // mid-flight on a budget breach (a non-conclusive verdict), NEVER reason_all.
        let result = reason_all_budgeted(self.dataset.as_ref(), budget)?;

        // The ONE row builder shared with `explanations_for_result`; this tool indexes
        // it and explains exactly one target (not the whole-closure `explain_all`).
        let rows = explain::rows_for_result(&result)?;
        let target_index = locate_explain_target(&rows, &target_reifier, graph)?;

        let explanation = LazyExplanationIndex::new(&rows).explain_one(target_index)?;
        // A fabricated cited IRI must never escape — re-verify against the full trace.
        explain::assert_faithful(&explanation, &rows)?;

        let step_skeleton: Vec<Value> = explanation
            .step_skeleton
            .iter()
            .map(|step| {
                json!({
                    "derivation_id": step.derivation_id,
                    "rule_iri": step.rule_iri,
                    "subject_iri": step.subject_iri,
                    "predicate_iri": step.predicate_iri,
                    "obj_n3": step.obj_n3,
                    "graph_iri": step.graph_iri,
                    "is_asserted": step.is_asserted,
                    "depth": step.depth,
                    "source_step_ids": step.source_step_ids,
                    "term_iris": step.term_iris,
                })
            })
            .collect();

        Ok(json!({
            "ok": true,
            "faithful": true,
            "markdown": explain::render_markdown(&explanation),
            "cited_iris": explanation.cited_iris.iter().cloned().collect::<Vec<_>>(),
            "step_skeleton": step_skeleton,
            "world_iri": explanation.world_iri,
            "completeness": completeness_class(&result),
            "judgment_nquads": project_reasoning_result(&result),
        }))
    }

    /// Explain a diagnostic witness over the bundle's `graph/diagnostics` named
    /// graph, addressed by its fingerprint IRI (a finding) or its anchor IRI (a
    /// cluster). Rehydrates the [`FindingIndex`] through the SAME native SPARQL
    /// reader the CLI `explain` uses, then returns the STRUCTURED witness surface:
    /// the focus finding (or the cluster's members), each with its provenance DAG,
    /// the aggregate ledger [`verdict`], the [`minimal_fatal_cut`] (fingerprint IRIs
    /// with codes), and the anchor cluster. An unknown/malformed target is a HARD
    /// FAIL (`Err`) — NEVER an empty-but-ok DAG.
    ///
    /// [`FindingIndex`]: crate::diagnostics_reader::FindingIndex
    /// [`verdict`]: crate::diagnostics_reader::verdict
    /// [`minimal_fatal_cut`]: crate::diagnostics_reader::minimal_fatal_cut
    fn explain_finding_json(&self, target: &str) -> gmeow_errors::Result<String> {
        use crate::diagnostics_reader::{
            explain_finding, minimal_fatal_cut, read_findings, render_shared_dag, verdict,
        };
        // The reader projects the `graph/diagnostics` named graph out of THIS
        // server's held snapshot — the exact carrier the export surfaces query.
        let index = read_findings(&self.dataset)?;

        let is_finding = index.get(target).is_some();
        let cluster: Vec<String> = index
            .findings
            .iter()
            .filter(|(_, f)| f.anchor_iri.as_deref() == Some(target))
            .map(|(iri, _)| iri.clone())
            .collect();
        let is_anchor = !cluster.is_empty();
        // HARD FAIL: neither a known fingerprint IRI nor a known anchor IRI. Never a
        // fabricated empty DAG rendered as a success.
        if !is_finding && !is_anchor {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "unknown explain target `{target}`: not a finding fingerprint IRI or an \
                     anchor IRI in graph/diagnostics"
                ),
            }));
        }

        let walk = |iri: &str| -> gmeow_errors::Result<String> {
            let dag = explain_finding(&index, iri).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("cannot walk provenance DAG for `{iri}`: {e}"),
                })
            })?;
            Ok(render_shared_dag(&dag))
        };

        // The focus: a single finding's DAG, or every cluster member's DAG.
        let (kind, focus_anchor, focus) = if is_finding {
            let f = index.get(target).expect("finding present");
            (
                "finding",
                f.anchor_iri.clone(),
                json!({
                    "finding_iri": target,
                    "code": f.code,
                    "severity": f.severity.as_str(),
                    "message": f.message,
                    "provenance_dag": walk(target)?,
                }),
            )
        } else {
            let mut members = Vec::with_capacity(cluster.len());
            for iri in &cluster {
                let f = index.get(iri).expect("cluster member present");
                members.push(json!({
                    "finding_iri": iri,
                    "code": f.code,
                    "severity": f.severity.as_str(),
                    "message": f.message,
                    "provenance_dag": walk(iri)?,
                }));
            }
            (
                "anchor",
                Some(target.to_owned()),
                json!({"anchor_iri": target, "members": members}),
            )
        };

        // The always-emitted substrate algebra: gate verdict + minimal fatal cut.
        let cut: Vec<Value> = minimal_fatal_cut(&index)
            .into_iter()
            .map(|iri| {
                let (code, message) = index
                    .get(&iri)
                    .map(|f| (f.code.clone(), f.message.clone()))
                    .unwrap_or_default();
                json!({"finding_iri": iri, "code": code, "message": message})
            })
            .collect();

        // The anchor cluster: every finding sharing the focus anchor (the code-blind
        // co-location the glut/Belnap join reads).
        let anchor_cluster: Vec<Value> = match focus_anchor.as_deref() {
            Some(anchor) => index
                .findings
                .values()
                .filter(|f| f.anchor_iri.as_deref() == Some(anchor))
                .map(|f| {
                    json!({
                        "finding_iri": f.finding_iri,
                        "code": f.code,
                        "severity": f.severity.as_str(),
                    })
                })
                .collect(),
            None => Vec::new(),
        };

        Ok(json!({
            "ok": true,
            "kind": kind,
            "target": target,
            "focus": focus,
            "anchor": focus_anchor,
            "anchor_cluster": anchor_cluster,
            "verdict": format!("{:?}", verdict(&index)),
            "minimal_fatal_cut": cut,
        })
        .to_string())
    }
}

/// A native SPARQL result rendered as a JSON envelope under `"ok"`: an ASK boolean,
/// SELECT bindings (SPARQL-1.1 JSON-results shape), or — for a CONSTRUCT / DESCRIBE
/// graph result — a hard error (these tools serve bindings or a boolean, never a
/// graph).
fn sparql_result_to_json(result: purrdf::SparqlResult) -> gmeow_errors::Result<Value> {
    match result {
        purrdf::SparqlResult::Boolean(value) => Ok(json!({"ok": true, "boolean": value})),
        purrdf::SparqlResult::Solutions {
            variables, rows, ..
        } => {
            let bindings: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    for (i, cell) in row.iter().enumerate() {
                        if let (Some(name), Some(term)) = (variables.get(i), cell.as_ref())
                            && let Some(value) = sparql_term_to_json(term)
                        {
                            obj.insert(name.clone(), value);
                        }
                    }
                    Value::Object(obj)
                })
                .collect();
            Ok(json!({
                "ok": true,
                "head": {"vars": variables},
                "results": {"bindings": bindings},
            }))
        }
        purrdf::SparqlResult::Graph(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "this tool accepts only SELECT and ASK queries; CONSTRUCT/DESCRIBE are not \
                      supported (it serves bindings or a boolean, never a graph)"
                .to_string(),
        })),
    }
}

/// One SPARQL binding rendered as a SPARQL-1.1 JSON-results term object. A quoted
/// triple term (rare in the documentation graph) has no standard binding shape and
/// is omitted.
fn sparql_term_to_json(term: &purrdf::TermValue) -> Option<Value> {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    match term {
        purrdf::TermValue::Iri(iri) => Some(json!({"type": "uri", "value": iri})),
        purrdf::TermValue::Blank { label, .. } => Some(json!({"type": "bnode", "value": label})),
        purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), json!("literal"));
            obj.insert("value".to_string(), json!(lexical_form));
            if let Some(lang) = language {
                obj.insert("xml:lang".to_string(), json!(lang));
            } else if datatype != XSD_STRING {
                obj.insert("datatype".to_string(), json!(datatype));
            }
            Some(Value::Object(obj))
        }
        purrdf::TermValue::Triple { .. } => None,
    }
}

impl McpView {
    /// The documentation graph re-rooted to the default graph, projected once and
    /// shared by every query surface for this server.
    fn documentation(&self) -> &Arc<purrdf::RdfDataset> {
        self.documentation.get_or_init(|| {
            Arc::new(
                self.dataset
                    .project_named_graph(crate::stages::carrier::GRAPH_DOCUMENTATION),
            )
        })
    }

    /// The `term-IRI → site URL` map, built once from the documentation graph and
    /// cached (language-independent).
    fn doc_urls(&self) -> Arc<HashMap<String, String>> {
        Arc::clone(self.doc_urls.get_or_init(|| {
            let view = FoldView::new(self.dataset.as_ref());
            Arc::new(export::doc_url_map(&view))
        }))
    }

    /// The JSON Schema `$defs` key set folded into this bundle's `schemas-archive`
    /// blob — the "this class has a generated Pydantic model" existence signal
    /// `export::term_to_card`'s `python_model` gate reads (see
    /// `export::class_is_modeled`), built once from the raw snapshot bytes (like
    /// `doc_urls`, language-independent). Empty when the bundle carries no
    /// `schemas-archive` rep, mirroring `crate::bundle_blobs::Bundle`'s own
    /// wheel-only-install contract for this accessor — a card's `python_model`
    /// line is ancillary, never worth a hard crash of the whole server.
    fn modeled_defs(&self) -> Arc<BTreeSet<String>> {
        Arc::clone(self.modeled_defs.get_or_init(|| {
            let defs = crate::bundle_blobs::Bundle::from_snapshot(&self.gts)
                .and_then(|b| b.modeled_def_keys())
                .unwrap_or_default();
            Arc::new(defs)
        }))
    }

    /// Run `f` over the terms collected for `requested`, collecting (and caching)
    /// on first use per requested-tag list.
    fn with_terms<R>(&self, requested: Vec<String>, f: impl FnOnce(&[Term]) -> R) -> R {
        let key = requested.join(",");
        let terms = {
            let mut cache = self.cache.lock().expect("McpView term cache poisoned");
            Arc::clone(cache.entry(key).or_insert_with(|| {
                let view = FoldView::with_requested(self.dataset.as_ref(), requested);
                Arc::new(export::collect_terms(&view))
            }))
        };
        f(terms.as_slice())
    }
}

/// Which tool surface an [`McpServer`] advertises: the bundle-only consumer
/// surface, or the repository-anchored developer surface (dev adds the
/// repo-reading maintenance tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpMode {
    /// Bundle-only surface — the shippable `gmeow mcp` server.
    Consumer,
    /// Consumer surface plus the repo-reading dev tools (`gmeow-dev mcp`).
    Dev,
}

impl McpMode {
    fn includes_dev_tools(self) -> bool {
        self == Self::Dev
    }
}

/// A Rust MCP server over the bundled snapshot and optional repository root.
pub struct McpServer {
    view: McpView,
    mode: McpMode,
    root: Option<PathBuf>,
    tag_map: BTreeMap<String, String>,
    available: BTreeSet<String>,
    startup_requested: Vec<String>,
}

impl McpServer {
    /// Build a native MCP server over the bundled `gmeow.gts` snapshot bytes.
    /// `root` (the checkout path) is required only for the [`McpMode::Dev`]
    /// repo-reading tools; the consumer surface passes `None`. Hard-fails if the
    /// snapshot does not read or the startup language (`GMEOW_LANG`) is unknown.
    pub fn from_snapshot(
        snapshot: &[u8],
        root: Option<PathBuf>,
        mode: McpMode,
    ) -> gmeow_errors::Result<Self> {
        let bundle = purrdf::import_gts_events(snapshot)
            .with_ctx(|| "read snapshot gmeow.gts".to_string())?;
        let dataset = bundle.dataset;
        let tag_map = language_tag_map(dataset.as_ref());
        let mut available: BTreeSet<String> =
            tag_map.values().map(|v| v.to_ascii_lowercase()).collect();
        available.insert("en".to_string());
        let startup_requested =
            resolve_lang(env::var("GMEOW_LANG").ok().as_deref(), &tag_map, &available)?;
        Ok(Self {
            view: McpView::from_dataset(dataset, Arc::from(snapshot))?,
            mode,
            root,
            tag_map,
            available,
            startup_requested,
        })
    }

    fn requested_from_args(&self, args: &Value) -> gmeow_errors::Result<Vec<String>> {
        match args.get("lang").and_then(Value::as_str) {
            Some(lang) => resolve_lang(Some(lang), &self.tag_map, &self.available),
            None => Ok(self.startup_requested.clone()),
        }
    }

    fn tools_result(&self) -> Value {
        let mut tools = vec![
            tool(
                "lookup_term",
                "Resolve a bundled GMEOW term.",
                &[("term", "string"), ("lang", "string")],
            ),
            tool(
                "llms_txt",
                "Return the standard bundled vocabulary index.",
                &[("lang", "string")],
            ),
            tool(
                "llms_full",
                "Return the complete inlined bundled vocabulary index.",
                &[("lang", "string")],
            ),
            tool(
                "doc_card",
                "Return a prompt-ready term card for a bundled term, wrapped in a \
                 cost-metadata envelope ({ok, detail, format, bytes, tokens, card}). \
                 `detail` selects a token-budgeted tier: `summary` (title + definition \
                 only), `standard` (the compact card — default), or `full` (the oracle \
                 card: compact card plus entailments, Do / Don't fixtures, diagnostics, \
                 and projection-loss panels, queried from gmeow:graph/documentation). \
                 `format` is `markdown` (default) or `json` (the neutral Card object). \
                 An unknown detail/format is a hard error.",
                &[
                    ("term", "string"),
                    ("detail", "string"),
                    ("format", "string"),
                    ("lang", "string"),
                ],
            ),
            tool(
                "okf_index",
                "Return the OKF manifest JSON envelope.",
                &[("lang", "string")],
            ),
            tool(
                "query_docs",
                "Run a SELECT or ASK SPARQL query over the bundled documentation graph \
                 (gmeow:graph/documentation) and return SPARQL-1.1 JSON results.",
                &[("query", "string")],
            ),
            tool(
                "docs_search",
                "Full-text search the bundled documentation graph (gmeow:graph/documentation) \
                 for terms, slices, and concerns whose label, definition, or advisory prose \
                 match `query` (case-insensitive substring / token match). Returns ranked \
                 records — a label match outranks a definition match outranks an advice match, \
                 tie-broken by IRI — each with its kind, documented IRI (id), label, definition, \
                 site URL, and the same advice / alignments / missing_coverage facets the site \
                 search index carries. Pass an optional `limit` (default 20). A query that \
                 matches nothing returns an empty result list.",
                &[("query", "string"), ("limit", "integer")],
            ),
            tool(
                "query_local",
                "Run a SELECT or ASK SPARQL query over the bundle UNIONED with a READ-ONLY \
                 local overlay graph file (path). The overlay is loaded as an EXTERNAL, \
                 read-only annex: its triples are visible to reads (bundle \u{222a} overlay, \
                 also isolable via GRAPH <urn:gmeow:mcp:overlay:external>) but are NEVER merged \
                 into the signed gmeow: canon and NEVER written back to disk. Accepts Turtle / \
                 TriG / N-Triples / N-Quads / RDF-XML by file extension.",
                &[("path", "string"), ("query", "string")],
            ),
            tool(
                "verify_graph",
                "Reason over the bundle UNIONED with a READ-ONLY local overlay graph file \
                 (path), then run the native reasoned-graph verify (the bad-example negative \
                 tests + non-entailment obligations) and return a PROOF-CARRYING judgment. \
                 The overlay is loaded as an EXTERNAL, read-only annex exactly like \
                 query_local: its triples join the reasoning default world (bundle \u{222a} \
                 overlay, also isolable via GRAPH <urn:gmeow:mcp:overlay:external>) but are \
                 NEVER merged into the signed gmeow: canon and NEVER written back to disk; the \
                 whole union is transient and discarded after the call. The forward closure \
                 runs under a mid-chase step governor: max_steps bounds the derivation budget \
                 and max_answers the answer cap. The response carries class_local_name \
                 (CoherenceCertificate for a conclusive coherent closure, \
                 CoherenceCheckAttestation for a budget-cut closure, Refused for a witnessed \
                 forbidden contradiction), the completeness/evaluation axes, the verify \
                 findings, the cited IRIs, and judgment_nquads (the grounded \
                 logic:ReasoningResult). An overlay exceeding the size ceiling or an unknown \
                 file extension is a hard error. Accepts Turtle / TriG / N-Triples / N-Quads / \
                 RDF-XML by file extension.",
                &[
                    ("path", "string"),
                    ("max_steps", "integer"),
                    ("max_answers", "integer"),
                ],
            ),
            tool(
                "explain_quad",
                "Reconstruct the FAITHFUL cited-IRI derivation skeleton for ONE arbitrary quad \
                 the bundle's reasoned closure entails: the target `(subject, predicate, \
                 object_value)` in named graph `graph`. Reasons over the bundle under the \
                 mid-chase step governor (max_steps bounds the derivation budget), locates the \
                 target by its content-addressed reifier WORLD-DISAMBIGUATED by `graph`, then \
                 reconstructs and re-verifies ONLY that quad's proof tree. Returns the DFS \
                 step_skeleton (target first, each step carrying its rule, derivation id, \
                 (S,P,O), world, asserted flag, depth, antecedent step ids, and cited term \
                 IRIs), the complete cited_iris set, a Markdown rendering, the world_iri, the \
                 completeness class (CoherenceCertificate for a conclusive closure, \
                 CoherenceCheckAttestation for a budget-cut one), and judgment_nquads (the \
                 grounded logic:ReasoningResult). `object_kind` is `iri` or `literal` (inferred \
                 from object_value when omitted); `object_datatype` types a literal object. This \
                 is DISTINCT from explain_finding: explain_finding addresses a PUBLISHED \
                 diagnostic by its fingerprint/anchor IRI over the graph/diagnostics projection, \
                 while explain_quad proves an arbitrary entailed quad from the reasoner's premise \
                 provenance. A quad the closure does not entail, or a reifier shared across \
                 worlds that `graph` does not resolve, is a hard error (never an empty-but-ok \
                 proof).",
                &[
                    ("subject", "string"),
                    ("predicate", "string"),
                    ("object_value", "string"),
                    ("object_kind", "string"),
                    ("object_datatype", "string"),
                    ("graph", "string"),
                    ("max_steps", "integer"),
                ],
            ),
            tool(
                "coherence_certificate",
                "Read the bundle's SCOPED COHERENCE CERTIFICATE — a budget-free, \
                 proof-carrying coherence attestation computed ONCE at pipeline time over the \
                 whole assembled bundle and folded into graph/attestations. This tool reads it \
                 straight off the bundled dataset: it takes NO inputs, runs NO reasoning, and \
                 never recomputes. The response carries class_local_name (CoherenceCertificate \
                 for a conclusive, fragment-scoped, violation-free closure; the strictly-weaker \
                 CoherenceCheckAttestation otherwise — an attestation is NEVER reported as a \
                 certificate), issues_certificate, is_refused, the pinned bundle_hash and \
                 per-graph axiom_hashes (the tamper surface), contract_hash, engine, \
                 certified_fragment, the completeness/evaluation axes, contradiction_policy, \
                 projection_losses, and forbidden_violations. A bundle carrying no coherence \
                 artifact is a hard error (never a silent recompute).",
                &[],
            ),
            tool(
                "validate_local",
                "Validate an inline RDF graph against the bundled GMEOW shapes and disciplines \
                 (the same core as `gmeow validate`), then CLOSE THE LOOP: every finding is \
                 returned with its rule catalog helpUri, the CORRESPONDING counter-example fixture \
                 (matched by violation code), a positive well-formed exemplar, and the entailments \
                 of the term it concerns. `data` is the RDF text; `format` is one of \
                 turtle|ttl|text/turtle, ntriples|nt|n-triples, nquads|nq|n-quads, trig, \
                 rdfxml|rdf+xml|xml, jsonld|json-ld. Set `deep` true to ALSO run the Tier-2 \
                 semantic pass (reasons over your data unioned with the whole bundle's axioms, \
                 like `gmeow validate --deep` — powerful but multiple minutes; default false). \
                 Nothing is written (the data is validated in-memory and discarded); an unknown \
                 format or an oversized payload is a hard error.",
                &[
                    ("data", "string"),
                    ("format", "string"),
                    ("deep", "boolean"),
                ],
            ),
            tool(
                "explain_finding",
                "Explain a diagnostic witness over the bundled graph/diagnostics projection, \
                 addressed by its fingerprint IRI (a finding) or its anchor IRI (a cluster): \
                 returns the provenance DAG, the aggregate ledger gate verdict, the minimal fatal \
                 cut (fingerprint IRIs + codes), and the anchor cluster. An unknown target is a \
                 hard error (never an empty-but-ok DAG).",
                &[("target_iri", "string")],
            ),
            tool(
                "store_claim",
                "Append one attributed memory claim, executed as a Transaction-Logic \
                 transaction (the executional-entailment verdict gates the commit). Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
                &[
                    ("text", "string"),
                    ("source", "string"),
                    ("confidence", "number"),
                    ("according_to", "string"),
                    ("dry_run", "boolean"),
                ],
            ),
            tool(
                "conjecture_test",
                "Test — a PURE hypothetical evaluation of a candidate logic: formula against a \
                 KB: compute and return the engine verdict, but NEVER TR-commit and NEVER \
                 append to the conjecture library. formula is a Turtle logic: document naming \
                 one candidate; kb is a Turtle KB; standpoint is the required reified scope \
                 (P9); math_conjecture optionally names the math:Conjecture twin. max_steps / \
                 max_answers optionally bound the isolated scenario evaluation (a \
                 derived-closure-size ceiling: exceeding it stamps BudgetExhausted → lifecycle \
                 open). For the committing counterpart, see store_conjecture.",
                &[
                    ("formula", "string"),
                    ("kb", "string"),
                    ("standpoint", "string"),
                    ("math_conjecture", "string"),
                    ("max_steps", "integer"),
                    ("max_answers", "integer"),
                ],
            ),
            tool(
                "store_conjecture",
                "Store — evaluate a candidate logic: formula against a KB and, TR-gated on the \
                 persistConjecture schema (the executional-entailment verdict gates the \
                 commit), APPEND the engine verdict to the append-only conjecture library. \
                 formula is a Turtle logic: document naming one candidate; kb is a Turtle KB; \
                 standpoint is the required reified scope (P9); math_conjecture optionally \
                 names the math:Conjecture twin. max_steps / max_answers optionally bound the \
                 isolated scenario evaluation (a derived-closure-size ceiling: exceeding it \
                 stamps BudgetExhausted → lifecycle open). Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing written) — a \
                 hypothetical-commit witness.",
                &[
                    ("formula", "string"),
                    ("kb", "string"),
                    ("standpoint", "string"),
                    ("math_conjecture", "string"),
                    ("dry_run", "boolean"),
                    ("max_steps", "integer"),
                    ("max_answers", "integer"),
                ],
            ),
            tool(
                "refute_conjecture",
                "Author-withdraw a stored conjecture — the store_conjecture compensation (P10, \
                 as revise_belief compensates store_claim), executed as a Transaction-Logic \
                 transaction on the withdrawConjecture schema. It APPENDS a compensating \
                 author-withdrawn segment to the append-only conjecture library, flipping the \
                 node's effective logic:conjectureLifecycleState to logic:ConjectureWithdrawn \
                 (recorded, never deleted — prior segments stay intact). conjecture_id is the \
                 stored logic:Conjecture node IRI; reason is an optional author note. The TR \
                 precondition — the conjecture is still in the library and NOT already \
                 withdrawn — is decided from the live library state by segment order, so an \
                 unknown id or an already-withdrawn node is rejected before any write. Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
                &[
                    ("conjecture_id", "string"),
                    ("reason", "string"),
                    ("dry_run", "boolean"),
                ],
            ),
            tool(
                "recall",
                "Recall stored memory claims.",
                &[
                    ("query", "string"),
                    ("min_confidence", "number"),
                    ("limit", "integer"),
                    ("include_suppressed", "boolean"),
                ],
            ),
            tool(
                "revise_belief",
                "Suppress a stored claim without deleting history (the store_claim \
                 compensation, P10), executed as a Transaction-Logic transaction whose \
                 precondition is that the target claim exists. Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing suppressed).",
                &[
                    ("claim_id", "string"),
                    ("reason", "string"),
                    ("superseded_by", "string"),
                    ("dry_run", "boolean"),
                ],
            ),
            tool(
                "counter_examples",
                "Return the conformance fixtures documenting a bundled term, split into \
                 well-formed exemplars and counter-examples, each with its full Turtle body and \
                 the authored expected outcome / violation code / conformance rationale (read from \
                 gmeow:graph/documentation). A term with no fixtures returns empty lists; an \
                 unknown term is a hard error.",
                &[("term", "string")],
            ),
            tool(
                "entailments",
                "Return the reasoner entailments documenting a bundled term — each derivation's \
                 rule, conclusion, and every premise (read from gmeow:graph/documentation). A term \
                 with no entailments returns an empty list; an unknown term is a hard error.",
                &[("term", "string")],
            ),
            tool(
                "competency_questions",
                "Return the runnable competency questions from gmeow:graph/documentation, each with \
                 its SPARQL query text and the authored rationale / expected row count / exact-rows \
                 flag. With the OPTIONAL `term`, only that term's questions (an unknown term is a \
                 hard error); without `term`, the whole index.",
                &[("term", "string")],
            ),
        ];
        if self.mode.includes_dev_tools() {
            tools.extend([
                tool("validate", "Run the native validation/check surface.", &[]),
                tool(
                    "reason",
                    "Run native reasoning over the bundled snapshot.",
                    &[],
                ),
                tool(
                    "regenerate",
                    "Run the native pipeline regenerate surface.",
                    &[],
                ),
                tool(
                    "constitution",
                    "Read the checked-out GMEOW Constitution.",
                    &[],
                ),
                tool(
                    "slice_quality",
                    "Score a slice against the slice-quality rubric and return its per-axis grades and ranked uplift advice.",
                    &[("path", "string")],
                ),
            ]);
        }
        json!({ "tools": tools })
    }

    fn resources_result(&self) -> Value {
        let mut resources = vec![
            resource(
                "gmeow://ontology/llms.txt",
                "llms.txt",
                "Standard bundled vocabulary index.",
                "text/plain",
            ),
            resource(
                "gmeow://ontology/llms-full.txt",
                "llms-full.txt",
                "Complete inlined bundled vocabulary index.",
                "text/plain",
            ),
            resource(
                "gmeow://ontology/okf-index",
                "okf-index",
                "OKF manifest JSON envelope.",
                "application/json",
            ),
        ];
        if self.mode.includes_dev_tools() {
            resources.push(resource(
                "gmeow://ontology/constitution",
                "constitution",
                "The checked-out GMEOW Constitution.",
                "text/markdown",
            ));
        }
        json!({ "resources": resources })
    }

    fn call_tool_result(&self, name: &str, args: &Value) -> Value {
        if let Some(err) = args.get("__parse_error").and_then(Value::as_str) {
            return tool_text(json!({"ok": false, "error": err}).to_string(), true);
        }
        let result = match name {
            "lookup_term" => self.tool_lookup_term(args),
            "llms_txt" => self.tool_llms_txt(args),
            "llms_full" => self.tool_llms_full(args),
            "doc_card" => self.tool_doc_card(args),
            "okf_index" => self.tool_okf_index(args),
            "query_docs" => self.tool_query_docs(args),
            "docs_search" => self.tool_docs_search(args),
            "query_local" => self.tool_query_local(args),
            "verify_graph" => self.tool_verify_graph(args),
            "explain_quad" => self.tool_explain_quad(args),
            "coherence_certificate" => self.tool_coherence_certificate(args),
            "validate_local" => self.tool_validate_local(args),
            "explain_finding" => self.tool_explain_finding(args),
            "store_claim" => self.tool_store_claim(args),
            "conjecture_test" => self.tool_conjecture_test(args),
            "store_conjecture" => self.tool_store_conjecture(args),
            "refute_conjecture" => self.tool_refute_conjecture(args),
            "recall" => self.tool_recall(args),
            "revise_belief" => self.tool_revise_belief(args),
            "counter_examples" => self.tool_counter_examples(args),
            "entailments" => self.tool_entailments(args),
            "competency_questions" => self.tool_competency_questions(args),
            "validate" if self.mode.includes_dev_tools() => self.tool_validate(),
            "reason" if self.mode.includes_dev_tools() => self.tool_reason(),
            "regenerate" if self.mode.includes_dev_tools() => self.tool_regenerate(),
            "constitution" if self.mode.includes_dev_tools() => self.tool_constitution(),
            "slice_quality" if self.mode.includes_dev_tools() => self.tool_slice_quality(args),
            _ => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("unknown tool: {name}"),
            })),
        };
        match result {
            Ok(text) => tool_text(text, false),
            Err(err) => tool_text(
                json!({"ok": false, "error": err.to_string()}).to_string(),
                true,
            ),
        }
    }

    fn read_resource_result(&self, uri: &str) -> Value {
        match self.read_resource_text(uri) {
            Ok((mime, text)) => json!({
                "contents": [{"uri": uri, "mimeType": mime, "text": text}],
            }),
            Err(err) => json!({
                "contents": [{"uri": uri, "mimeType": "application/json", "text": json!({"ok": false, "error": err.to_string()}).to_string()}],
                "isError": true,
            }),
        }
    }

    fn handle_message(&self, message: &str) -> String {
        let parsed: Value = match serde_json::from_str(message) {
            Ok(value) => value,
            Err(err) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": err.to_string()},
                })
                .to_string();
            }
        };
        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = parsed.get("method").and_then(Value::as_str) else {
            return rpc_error(id, -32600, "missing method");
        };
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "gmeow", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"tools": {}, "resources": {}},
            }),
            "tools/list" => self.tools_result(),
            "resources/list" => self.resources_result(),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "tools/call requires params.name");
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                self.call_tool_result(name, &args)
            }
            "resources/read" => {
                let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "resources/read requires params.uri");
                };
                self.read_resource_result(uri)
            }
            "shutdown" => json!({}),
            method if method.starts_with("notifications/") => return String::new(),
            _ => return rpc_error(id, -32601, &format!("unknown method: {method}")),
        };
        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
    }

    /// Serve the stdio JSON-RPC 2.0 MCP loop: one request per line on stdin, one
    /// response per line on stdout, until EOF. Blocking; the native `gmeow mcp` /
    /// `gmeow-dev mcp` launchers call this directly.
    pub fn run_stdio(&self) -> gmeow_errors::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let response = self.handle_message(&line);
            if !response.is_empty() {
                writeln!(stdout, "{response}")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    fn tool_lookup_term(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.lookup_term_json(term, requested))
    }

    fn tool_llms_txt(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_txt_text(requested))
    }

    fn tool_llms_full(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_full_text(requested))
    }

    fn tool_doc_card(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let detail = parse_card_detail(optional_str(args, "detail"))?;
        let format = CardFormat::parse(optional_str(args, "format"))?;
        let requested = self.requested_from_args(args)?;
        self.view.doc_card(term, detail, format, requested)
    }

    fn tool_okf_index(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.okf_index_json(requested))
    }

    /// `counter_examples`: the conformance fixtures documenting a term, split into
    /// the well-formed exemplars and the counter-examples. An UNKNOWN term is a HARD
    /// FAIL (`Err` → error envelope); a KNOWN term that simply documents no fixtures
    /// is an honest empty-but-ok result (both lists empty).
    fn tool_counter_examples(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let iri = self.resolve_term_or_err(term, args)?;
        let (wellformed, counter_examples) = self.view.term_fixtures(&iri)?;
        Ok(json!({
            "ok": true,
            "term": term,
            "wellformed": wellformed,
            "counter_examples": counter_examples,
        })
        .to_string())
    }

    /// `entailments`: the reasoner derivations documenting a term, each with its
    /// rule, conclusion, and every premise. Unknown term → hard error; a known term
    /// with no derivations → empty-but-ok.
    fn tool_entailments(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let iri = self.resolve_term_or_err(term, args)?;
        let entailments = self.view.term_entailments(&iri)?;
        Ok(json!({
            "ok": true,
            "term": term,
            "entailments": entailments,
        })
        .to_string())
    }

    /// `competency_questions`: the runnable competency questions. With a `term`, only
    /// that term's questions (unknown term → hard error); without a `term`, the whole
    /// index of documented competency questions.
    fn tool_competency_questions(&self, args: &Value) -> gmeow_errors::Result<String> {
        match optional_str(args, "term") {
            Some(term) => {
                let iri = self.resolve_term_or_err(term, args)?;
                let questions = self.view.competency_questions(Some(&iri))?;
                Ok(json!({"ok": true, "term": term, "questions": questions}).to_string())
            }
            None => {
                let questions = self.view.competency_questions(None)?;
                Ok(json!({"ok": true, "questions": questions}).to_string())
            }
        }
    }

    /// Resolve a term string to its canonical IRI, HARD-FAILING (`Err`) on an unknown
    /// term OR a cross-namespace bare-name collision — the shared resolution guard for
    /// the documentation-surface tools (`counter_examples`, `entailments`,
    /// `competency_questions`). Ambiguity carries the TYPED `McpAmbiguousTerm`
    /// diagnostic (sorted candidate CURIEs); an unknown term keeps the generic
    /// unknown-term diagnostic — never a silent pick.
    fn resolve_term_or_err(&self, term: &str, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        match self.view.resolve_term_iri(term, requested) {
            ConsumerResolution::Resolved(iri) => Ok(iri),
            ConsumerResolution::Ambiguous { candidates } => {
                Err(ambiguous_term_err(term, &candidates))
            }
            ConsumerResolution::NotFound => Err(unknown_term_err(term)),
        }
    }

    fn tool_query_docs(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        Ok(self.view.query_docs_json(query))
    }

    /// `docs_search`: rank the documented terms / slices / concerns whose searchable
    /// facets match `query` over the `gmeow:graph/documentation` projection. An
    /// absent/empty documentation graph is a HARD FAIL; a query matching nothing is an
    /// honest empty-but-ok result.
    fn tool_docs_search(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        let limit = optional_limit(args, "limit")?.unwrap_or(20);
        let docs = self.view.documentation();
        let hits = search_documentation(docs, query, limit)?;
        let results: Vec<Value> = hits.iter().map(SearchHit::to_json).collect();
        Ok(json!({"ok": true, "query": query, "results": results}).to_string())
    }

    fn tool_query_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let path = required_str(args, "path")?;
        let query = required_str(args, "query")?;
        Ok(self.view.query_local_json(path, query))
    }

    /// Reason-and-verify the bundle UNIONED with a READ-ONLY local overlay graph file,
    /// returning the proof-carrying judgment envelope. A thin wrapper over
    /// [`McpView::run_verify_graph`]: it reads the `path` and the `max_steps` /
    /// `max_answers` budget off the args and delegates the whole overlay/union/govern/
    /// verify/judge discipline to the view core (one implementation).
    fn tool_verify_graph(&self, args: &Value) -> gmeow_errors::Result<String> {
        let path = required_str(args, "path")?;
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        let budget = governed_budget(max_steps, max_answers);
        Ok(self.view.verify_graph_json(path, &budget))
    }

    /// Reconstruct the FAITHFUL cited-IRI derivation skeleton for one quad in the
    /// bundle's governed closure, returning the proof-carrying envelope. A thin
    /// wrapper over [`McpView::run_explain_quad`]: it validates the required quad
    /// components, builds the canonical N3 object surface (Step B) from the structured
    /// `object_value` / `object_kind` / `object_datatype` args, reads the `max_steps`
    /// governor, and delegates the reason/locate/explain/faithfulness discipline to
    /// the view core (one implementation).
    ///
    /// This is the CONSUMER quad-explainer — distinct from `explain_finding`, which
    /// addresses a published diagnostic by its fingerprint/anchor IRI.
    fn tool_explain_quad(&self, args: &Value) -> gmeow_errors::Result<String> {
        let subject = required_str(args, "subject")?;
        let predicate = required_str(args, "predicate")?;
        let object_value = required_str(args, "object_value")?;
        let graph = required_str(args, "graph")?;
        // `object_kind` is optional: an explicit `iri`/`literal` overrides, else it is
        // inferred from the object surface. `object_datatype` types a literal only.
        let object_kind = optional_str(args, "object_kind");
        let object_datatype = optional_str(args, "object_datatype");
        let obj_n3 = object_term_n3(object_value, object_kind, object_datatype)?;

        let max_steps = optional_step_count(args, "max_steps")?;
        // R4: `explain_quad` never exposes `max_answers`, but
        // `governed_budget` still stamps a finite default there — no field of the
        // constructed `Budget` is ever `None` on an agent-facing path.
        let budget = governed_budget(max_steps, None);
        Ok(self
            .view
            .explain_quad_json(subject, predicate, &obj_n3, graph, &budget))
    }

    /// Surface the bundle's carried coherence certificate/attestation (R6). A thin,
    /// INPUT-FREE wrapper over [`McpView::coherence_certificate_json`]: the certificate is
    /// read straight off the bundled dataset (disk-free, reason-free), never recomputed.
    fn tool_coherence_certificate(&self, _args: &Value) -> gmeow_errors::Result<String> {
        Ok(self.view.coherence_certificate_json())
    }

    /// Validate an inline RDF `data` graph (in `format`) against the bundle's own
    /// shapes + disciplines through the DEEP (`--deep`) semantic pass — the SAME
    /// `gmeow_validate::data_validate::run` core the CLI `gmeow validate --deep`
    /// drives — then CLOSE THE LOOP by enriching each finding with the bundle's
    /// teaching surface: the rule's catalog help URI, the CORRESPONDING
    /// counter-example (matched by violation code), a positive exemplar, and the
    /// term's entailments.
    ///
    /// Transient discipline: nothing is written. The data arrives inline as a string
    /// arg (unlike `query_local`, which reads a file), is validated against the
    /// retained snapshot bytes, and is discarded. An unrecognized `format` or an
    /// oversized payload is a HARD FAIL, never a silent mis-parse or truncation.
    fn tool_validate_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let canonical = canonical_rdf_format(format)?;

        if data.len() > MAX_VALIDATE_DATA_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "validate_local: data payload is {} bytes, exceeding the {} byte ceiling; \
                     split the graph and validate the parts (no silent truncation)",
                    data.len(),
                    MAX_VALIDATE_DATA_BYTES
                ),
            }));
        }

        // Tier-1 SHACL + disciplines always run (fast, ~1s). The Tier-2 semantic
        // pass — reasoning over the user data unioned with the whole bundle's
        // axioms — is opt-in via `deep` (default false), exactly like
        // `gmeow validate --deep`: it is powerful but reasons over the entire
        // bundle (multiple minutes), so an interactive agent requests it only when
        // structural conformance is not enough. When enabled, a contract-invalid
        // input is emitted by the engine as a hard Error finding — surfaced here
        // verbatim, never post-processed away.
        let deep = optional_bool(args, "deep").unwrap_or(false);
        let report = gmeow_validate::data_validate::run(
            data.as_bytes(),
            canonical,
            self.view.gts_bytes(),
            MCP_NAMESPACE,
            VALIDATE_LOCAL_ORIGIN,
            deep,
        )?;

        // Loop closure: extract the correspondence maps from the documentation graph
        // and enrich each finding through the wasm-clean pure join.
        let (counter_examples_by_code, wellformed_by_code) = self.view.fixture_maps()?;
        let entailments_by_term = self.view.entailment_map()?;
        let enriched = local_oracle::enrich_report(
            &report,
            &counter_examples_by_code,
            &wellformed_by_code,
            &entailments_by_term,
        );
        serde_json::to_string(&enriched).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("validate_local: serialize enriched report: {e}"),
            })
        })
    }

    fn tool_explain_finding(&self, args: &Value) -> gmeow_errors::Result<String> {
        let target = required_str(args, "target_iri")?;
        self.view.explain_finding_json(target)
    }

    fn tool_store_claim(&self, args: &Value) -> gmeow_errors::Result<String> {
        let text = required_str(args, "text")?;
        let confidence = optional_f64(args, "confidence")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);

        // store_claim's precondition — a well-formed claim is presented — obtains once the input
        // validates (required text + in-range confidence, enforced above). Run the action as a
        // TR transaction; the engine's executional entailment over THIS start state is the gate
        // (no synthetic boolean — an absent precondition would fail the run).
        let obtains = [MCP_WELL_FORMED_CLAIM];
        let receipt = execute_memory_txn(MCP_STORE_CLAIM_SCHEMA, &obtains, dry_run)?;
        match &receipt {
            TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("store_claim precondition unmet: {reason}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is written or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        let memory = self.memory()?;
        let claim = memory.store(
            text,
            StoreOptions {
                source: optional_str(args, "source"),
                confidence,
                according_to: optional_str(args, "according_to"),
            },
        )?;
        let response =
            json!({"ok": true, "claim": claim_json(&claim), "transaction": txn_json(&receipt)})
                .to_string();
        let generated = [claim.id.as_str()];
        let call = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}store_claim"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["text", "source", "confidence", "according_to", "dry_run"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &generated,
            },
        )?;
        // Record the trajectory-audit context on the recorded call so the committed turn is cold-auditable.
        let at_time = call.created.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "record_tool_call did not stamp a creation time".to_string(),
            })
        })?;
        write_audit_segment(
            memory.path(),
            &call.id,
            MCP_STORE_CLAIM_SCHEMA,
            &obtains,
            at_time,
        )?;
        Ok(response)
    }

    fn tool_recall(&self, args: &Value) -> gmeow_errors::Result<String> {
        let limit = optional_limit(args, "limit")?.unwrap_or(10);
        let claims = self.memory()?.recall(RecallOptions {
            query: optional_str(args, "query").unwrap_or(""),
            min_confidence: optional_f64(args, "min_confidence")?,
            limit,
            include_suppressed: optional_bool(args, "include_suppressed").unwrap_or(false),
        })?;
        Ok(json!({
            "ok": true,
            "claims": claims.iter().map(claim_json).collect::<Vec<_>>(),
        })
        .to_string())
    }

    fn tool_revise_belief(&self, args: &Value) -> gmeow_errors::Result<String> {
        let claim_id = required_str(args, "claim_id")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let memory = self.memory()?;
        let claims = memory.claims()?;
        let known: BTreeSet<&str> = claims.iter().map(|claim| claim.id.as_str()).collect();
        let active: BTreeSet<&str> = claims
            .iter()
            .filter(|claim| !claim.suppressed)
            .map(|claim| claim.id.as_str())
            .collect();
        if let Some(successor) = optional_str(args, "superseded_by")
            && !known.contains(successor)
        {
            return Ok(json!({
                "ok": false,
                "error": format!("unknown superseded_by id: {successor}"),
            })
            .to_string());
        }

        // revise_belief's precondition — the target claim exists — obtains iff claim_id is a known
        // claim (the existing pre-flight check, now expressed AS the TR precondition). The del
        // effect retires the active claim, so claimInMemory obtains iff it is not already
        // suppressed. The engine's executional entailment is the gate; an unknown id fails the run.
        let mut obtains: Vec<&str> = Vec::new();
        if known.contains(claim_id) {
            obtains.push(MCP_TARGET_CLAIM_EXISTS);
        }
        if active.contains(claim_id) {
            obtains.push(MCP_CLAIM_IN_MEMORY);
        }
        let receipt = execute_memory_txn(MCP_REVISE_BELIEF_SCHEMA, &obtains, dry_run)?;
        match &receipt {
            TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("unknown claim id: {claim_id}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is suppressed or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "suppressed": claim_id,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        memory.revise(
            claim_id,
            RevisionOptions {
                reason: optional_str(args, "reason"),
                superseded_by: optional_str(args, "superseded_by"),
            },
        )?;
        let response = json!({
            "ok": true,
            "suppressed": claim_id,
            "superseded_by": optional_str(args, "superseded_by"),
            "transaction": txn_json(&receipt),
        })
        .to_string();
        let call = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}revise_belief"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["claim_id", "reason", "superseded_by", "dry_run"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &[],
            },
        )?;
        let at_time = call.created.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "record_tool_call did not stamp a creation time".to_string(),
            })
        })?;
        write_audit_segment(
            memory.path(),
            &call.id,
            MCP_REVISE_BELIEF_SCHEMA,
            &obtains,
            at_time,
        )?;
        Ok(response)
    }

    /// `conjecture_test` — the issue's "test" leg: a PURE hypothetical evaluation of a
    /// candidate `logic:` formula against a KB. Computes and returns the projected engine
    /// verdict; NEVER TR-commits and NEVER appends to the conjecture library. For the
    /// committing counterpart ("store"), see [`Self::tool_store_conjecture`].
    ///
    /// `formula` is a Turtle `logic:` document naming the candidate (exactly one top-level
    /// `logic:Formula`, or exactly one ground `logic:` axiom lifted to a binary atom); `kb`
    /// is a Turtle KB the candidate is tested against; `standpoint` is the REQUIRED reified
    /// scope (Principle 9); `math_conjecture` optionally names the `math:Conjecture` twin so
    /// the statement is bridged to the runtime `logic:Conjecture` node via
    /// `math:conjectureUnderTest` (on every verdict) and a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    fn tool_conjecture_test(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        // The CLI's `gmeow conjecture test` shares `run_conjecture_test_pure` but calls it
        // directly with its own (possibly unbounded) args — this governance is scoped to
        // the agent-facing MCP wrapper.
        let budget = governed_budget(max_steps, max_answers);

        // The parse → test → project path is the SHARED evaluation core (also behind
        // `store_conjecture` and the CLI). This tool never TR-gates and never appends — it is
        // a thin wrapper rendering the pure verdict as the JSON response.
        let out = run_conjecture_test_pure(&ConjectureRunPureInput {
            formula_ttl: formula_src,
            kb_ttl: kb_src,
            standpoint,
            math_conjecture,
            max_steps: budget.max_steps,
            max_answers: budget.max_answers,
        })?;

        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        Ok(json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            // T1: the same grounded-judgment key/shape `verify_graph`/`explain_quad` carry —
            // here the embedded `logic:ReasoningResult` the engine's verdict was read from,
            // wrapped in the content-addressed `logic:Conjecture` node.
            "judgment_nquads": out.verdict_nt,
        })
        .to_string())
    }

    /// `store_conjecture` — the issue's "store" leg: test a candidate `logic:` formula against
    /// a KB, project the engine verdict, and — TR-gated on the `persistConjecture` schema —
    /// APPEND it to the append-only conjecture library. Runs the SAME evaluation as
    /// `conjecture_test`; the difference is entirely in the tail (TR-gate + persist).
    ///
    /// `formula` / `kb` / `standpoint` / `math_conjecture` are as `conjecture_test`;
    /// `dry_run=true` computes and returns the verdict (via a hypothetical TR commit) but
    /// WRITES NOTHING.
    fn tool_store_conjecture(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        // The CLI's `gmeow conjecture test` shares `run_conjecture_test` but calls it
        // directly with its own (possibly unbounded) args — this governance is scoped to
        // the agent-facing MCP wrapper.
        let budget = governed_budget(max_steps, max_answers);

        // The parse → test → project → TR-gate → persist path is the SHARED persisting core
        // (also behind the CLI `gmeow conjecture test`); the tool is a thin wrapper rendering
        // its outcome as the JSON response.
        let out = run_conjecture_test(&ConjectureRunInput {
            formula_ttl: formula_src,
            kb_ttl: kb_src,
            standpoint,
            math_conjecture,
            dry_run,
            max_steps: budget.max_steps,
            max_answers: budget.max_answers,
        })?;

        // The five-field verdict summary + witness, rendered for every response path.
        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        // T1: the verdict — and its grounded `judgment_nquads` projection — was ALREADY
        // computed by `evaluate_conjecture` before the TR gate ran, so it is available (and
        // carried) on every path below, including precondition-unmet: the engine's judgment
        // about the candidate is real regardless of whether the persist itself succeeded.
        if let Some(reason) = &out.precondition_unmet {
            return Ok(json!({
                "ok": false,
                "error": format!("persistConjecture precondition unmet: {reason}"),
                "verdict": verdict_json,
                "witness": witness_json,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            })
            .to_string());
        }
        if out.dry_run {
            return Ok(json!({
                "ok": true,
                "dry_run": true,
                "verdict": verdict_json,
                "witness": witness_json,
                "conjecture": out.node_iri,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            })
            .to_string());
        }

        Ok(json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            "transaction": txn_json(&out.receipt),
            "judgment_nquads": out.verdict_nt,
        })
        .to_string())
    }

    /// `refute_conjecture` — the compensating author-WITHDRAWAL counterpart of
    /// `store_conjecture` (P10, exactly as `revise_belief` compensates `store_claim`). It
    /// appends one compensating "author-withdrawn" segment to the append-only conjecture
    /// library, flipping the target node's EFFECTIVE `logic:conjectureLifecycleState` to
    /// `logic:ConjectureWithdrawn` — recorded, never deleted; the prior segments stay intact.
    ///
    /// The write is a REAL TR gate on the `withdrawConjecture` schema, whose precondition —
    /// the conjecture is still in the library and NOT already withdrawn — is DERIVED from the
    /// live library state read back by SEGMENT ORDER ([`read_conjecture_library`], R3): the
    /// `conjectureInLibrary` situation obtains iff the node exists and its effective state is
    /// not yet `Withdrawn`. An unknown id or an already-withdrawn node yields an empty start
    /// state, so the executional-entailment run FAILS the commit and the tool returns
    /// `ok:false` before writing. `dry_run=true` witnesses the hypothetical commit and appends
    /// nothing.
    fn tool_refute_conjecture(&self, args: &Value) -> gmeow_errors::Result<String> {
        let conjecture_id = required_str(args, "conjecture_id")?;
        let reason = optional_str(args, "reason").unwrap_or("");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let path = conjecture_path()?;

        // The read (library's EFFECTIVE state, by segment order) → precondition-check → (on a
        // real commit) library-append → audit-append sequence runs ENTIRELY inside ONE held
        // exclusive lock (`with_conjecture_lock`, CodeRabbit Major): without it, two concurrent
        // `refute_conjecture` calls against the same id could both read "not yet withdrawn",
        // both pass the precondition, and both commit a withdrawal segment (lost-update). The
        // lock forces the second caller to observe the FIRST caller's already-committed
        // `ConjectureWithdrawn` state before it decides anything.
        with_conjecture_lock(&path, || {
            // Read the library's EFFECTIVE state by segment order (last-writer-wins). The node
            // is withdrawable iff it is a stored conjecture whose effective state is not already
            // Withdrawn — the `del(conjectureInLibrary)` of a prior withdrawal retired it.
            let library = read_conjecture_library(&path)?;
            let effective = library.get(conjecture_id).copied();
            let exists = effective.is_some();
            let in_library = matches!(
                effective,
                Some(state) if state != ConjectureLifecycleState::Withdrawn
            );

            // The precondition `conjectureInLibrary` obtains iff the node is present and not yet
            // withdrawn — the engine's executional entailment over THIS derived start state is
            // the commit gate (an absent situation fails the run; no synthetic boolean).
            let mut obtains: Vec<&str> = Vec::new();
            if in_library {
                obtains.push(MCP_CONJECTURE_IN_LIBRARY);
            }
            let receipt = execute_memory_txn(MCP_WITHDRAW_CONJECTURE_SCHEMA, &obtains, dry_run)?;
            // T1: the compensating withdrawal's OWN grounded RDF projection — content-addressed
            // on (conjecture_id, reason), pure and side-effect-free — computed once here so BOTH
            // the hypothetical (dry-run) and committed success paths can carry it under the SAME
            // `judgment_nquads` key the read tools and the other two conjecture tools use. The
            // precondition-unmet path below deliberately does NOT carry it: no withdrawal was
            // validly grounded (unknown id, or already withdrawn), so emitting this body there
            // would fabricate a judgment about a state change that never obtained.
            let nt_body = project_conjecture_withdrawal(conjecture_id, reason);
            match &receipt {
                TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                    let detail = if !exists {
                        format!("unknown conjecture id: {conjecture_id}")
                    } else {
                        format!("conjecture already withdrawn: {conjecture_id}")
                    };
                    return Ok(json!({
                        "ok": false,
                        "error": format!("withdrawConjecture precondition unmet: {detail}"),
                        "transaction": txn_json(&receipt),
                    })
                    .to_string());
                }
                // Sandbox run: the compensating verdict is witnessed, nothing is appended.
                TxReceipt::HypotheticalSuccess { .. } => {
                    return Ok(json!({
                        "ok": true,
                        "dry_run": true,
                        "conjecture": conjecture_id,
                        "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                        "transaction": txn_json(&receipt),
                        "judgment_nquads": nt_body,
                    })
                    .to_string());
                }
                TxReceipt::CommittedSuccess { .. } => {}
            }

            // Committed: APPEND the compensating author-withdrawal segment (the target node
            // re-marked `ConjectureWithdrawn`, author reason, reviewer-asserted provenance)
            // TOGETHER with its cold-auditable trajectory segment (keyed to a content-addressed
            // call id) as ONE atomic file replace — so a failure building or committing either
            // segment can never leave the withdrawal applied without its audit record, or vice
            // versa. The library is still append-only overall: no PRIOR segment's bytes are
            // touched, only new bytes are added.
            let withdrawal_segment = build_conjecture_verdict_segment(&nt_body)?;
            let call_id = format!(
                "urn:gmeow:conjecture-call:{}",
                sha256_hex(format!("withdraw\u{1}{conjecture_id}\u{1}{reason}").as_bytes())
            );
            let audit_segment = build_audit_segment(
                &call_id,
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &[MCP_CONJECTURE_IN_LIBRARY],
                "1970-01-01T00:00:00Z",
            );
            append_conjecture_segments(&path, &[withdrawal_segment, audit_segment])?;
            Ok(json!({
                "ok": true,
                "conjecture": conjecture_id,
                "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                "transaction": txn_json(&receipt),
                "judgment_nquads": nt_body,
            })
            .to_string())
        })
    }

    fn tool_validate(&self) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        let report = crate::run::run_full(&root, 1, crate::run::RunMode::Check)?;
        Ok(json!({
            "ok": report.is_clean(),
            "mode": "check",
            "produced": report.produced,
            "reproduced": report.reproduced,
            "drifted": report.drifted,
        })
        .to_string())
    }

    /// Score ONE slice on demand and return its grades + advice as JSON. This is a
    /// read-only advisory surface: it computes a fresh assessment for the caller and
    /// folds nothing. The whole-repo `gmeow:QualityAssessment` graph is instead attached
    /// to the carrier by the regeneration pipeline (`stage-source-load` via
    /// [`gmeow_slice_quality::assessment_nquads`]) so it ships inside `gmeow.gts`; this
    /// tool never mutates the bundle.
    fn tool_slice_quality(&self, args: &Value) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        let rel = required_str(args, "path")?;
        let slice_dir = resolve_slice_dir(&root, rel)?;
        let report = gmeow_slice_quality::report::score_slice(&root, &slice_dir).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("slice_quality: {e}"),
            })
        })?;
        let grades: Vec<Value> = report
            .assessment
            .grades
            .iter()
            .map(|g| {
                let axis = g.axis_iri.rsplit(['/', '#']).next().unwrap_or(&g.axis_iri);
                json!({ "axis": axis, "tier": g.tier.label, "score": g.score })
            })
            .collect();
        let advice: Vec<Value> = report
            .advisories
            .iter()
            .map(|f| json!({ "code": f.code, "message": f.message }))
            .collect();
        Ok(json!({
            "slice": report.assessment.slice,
            "rollup_tier": report.assessment.rollup.label,
            "grades": grades,
            "advice": advice,
        })
        .to_string())
    }

    fn tool_regenerate(&self) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        let report = crate::run::run_full(&root, 1, crate::run::RunMode::Regenerate)?;
        Ok(json!({
            "ok": true,
            "mode": "regenerate",
            "produced": report.produced,
            "reproduced": report.reproduced,
        })
        .to_string())
    }

    fn tool_reason(&self) -> gmeow_errors::Result<String> {
        let result =
            gmeow_logic::reason::reason_all(self.view.graph_dataset()?.as_ref()).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("native reasoning failed: {e}"),
                })
            })?;
        Ok(json!({
            "ok": true,
            "input": result.input.wire(),
            "evaluation": result.evaluation.wire(),
            "completeness": result.completeness.wire(),
            "information": result.information.wire(),
        })
        .to_string())
    }

    fn tool_constitution(&self) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        Ok(fs::read_to_string(root.join("CONSTITUTION.md"))?)
    }

    fn read_resource_text(&self, uri: &str) -> gmeow_errors::Result<(&'static str, String)> {
        let (base, query) = uri.split_once('?').unwrap_or((uri, ""));
        let requested = lang_from_query(query)
            .map(|raw| resolve_lang(Some(raw), &self.tag_map, &self.available))
            .transpose()?
            .unwrap_or_else(|| self.startup_requested.clone());
        match base {
            "gmeow://ontology/llms.txt" => Ok(("text/plain", self.view.llms_txt_text(requested))),
            "gmeow://ontology/llms-full.txt" => {
                Ok(("text/plain", self.view.llms_full_text(requested)))
            }
            "gmeow://ontology/okf-index" => {
                Ok(("application/json", self.view.okf_index_json(requested)))
            }
            "gmeow://ontology/constitution" if self.mode.includes_dev_tools() => {
                self.tool_constitution().map(|text| ("text/markdown", text))
            }
            _ => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("unknown resource: {uri}"),
            })),
        }
    }

    fn memory(&self) -> gmeow_errors::Result<Memory> {
        let path = memory_path()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        Ok(Memory::new(path))
    }

    fn root_path(&self) -> gmeow_errors::Result<PathBuf> {
        self.root.clone().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "repository root is required for dev MCP tools".to_string(),
            })
        })
    }
}

impl McpView {
    fn graph_dataset(&self) -> gmeow_errors::Result<Arc<purrdf::RdfDataset>> {
        // The carrier IS the dataset — no gts round-trip (GTS is exit-only).
        Ok(Arc::clone(&self.dataset))
    }
}

// --------------------------------------------------------------------------- //
// The shared conjecture-test core (one evaluation implementation, three surfaces).
//
// [`evaluate_conjecture`] is the SINGLE parse → test → project path: it parses the candidate
// document and KB, re-homes the KB into the isolated scenario world, runs the native engine,
// and projects the verdict to deterministic N-Triples. Nothing calls the engine or the
// projector a second time.
//
// Two public entries sit on top of it, matching the issue's test / store decomposition:
//   - [`run_conjecture_test_pure`] — the "test" leg: evaluate only, NEVER TR-gate, NEVER
//     persist. Behind the MCP `conjecture_test` tool.
//   - [`run_conjecture_test`] — the "store" leg: evaluate, then TR-gate on the
//     `persistConjecture` schema and, on a committed precondition-met run, APPEND the verdict
//     to the append-only conjecture library. Behind the MCP `store_conjecture` tool AND the
//     public `gmeow conjecture test` CLI subcommand (unchanged CLI behavior).
// Neither surface re-implements the evaluation; each renders its own outcome (JSON / text).
// --------------------------------------------------------------------------- //

/// The inputs shared by both evaluation entries: the candidate `logic:` Turtle document, the
/// KB Turtle it is tested against, the REQUIRED reified standpoint scope (Principle 9), an
/// optional `math:Conjecture` twin, and the optional derived-closure budget.
pub struct ConjectureRunPureInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// Optional post-hoc derived-closure-size ceiling on the isolated scenario evaluation: when
    /// the derived (non-EDB) closure exceeds this many steps the run is stamped `BudgetExhausted`
    /// → lifecycle Open → discharge Unknown. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling in answer bindings; see
    /// [`max_steps`](Self::max_steps). `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// The inputs to one PERSISTING conjecture test: [`ConjectureRunPureInput`]'s fields plus
/// whether the run is a sandbox (`dry_run`, writes nothing).
pub struct ConjectureRunInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// When true, compute and return the verdict but WRITE NOTHING to the library.
    pub dry_run: bool,
    /// Optional post-hoc derived-closure-size ceiling; see
    /// [`ConjectureRunPureInput::max_steps`]. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling; see
    /// [`ConjectureRunPureInput::max_answers`]. `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// A refutation's contradiction witness, flattened for every response surface.
pub struct ConjectureRunWitness {
    /// The individual forced into a clash.
    pub individual: String,
    /// The world the contradiction is local to.
    pub world: String,
    /// The premise triples that witness the clash, each rendered `"s p o"`.
    pub premises: Vec<String>,
}

/// The outcome of a [`run_conjecture_test_pure`] call: the projected verdict facets, the
/// refutation witness (when refuted), the content-addressed node IRI, and the projected
/// N-Triples body. Nothing is ever committed or persisted on this path.
pub struct ConjecturePureOutput {
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name (`ObligationDischarged` | `ObligationUnknown`).
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed `(formula × standpoint × KB-world)` conjecture node IRI.
    pub node_iri: String,
    /// The deterministic N-Triples body [`project_conjecture_verdict`] emitted.
    pub verdict_nt: String,
}

/// The outcome of a [`run_conjecture_test`] call: the projected verdict facets, the refutation
/// witness (when refuted), the content-addressed node IRI, the projected N-Triples body, and
/// the TR receipt gating the persist. `committed` is true exactly when the segment was appended.
pub struct ConjectureRunOutput {
    /// True exactly when the verdict segment + audit were appended to the library (a committed,
    /// precondition-met, non-dry run).
    pub committed: bool,
    /// True when the run was a sandbox (`dry_run`): the verdict is computed, nothing is written.
    pub dry_run: bool,
    /// `Some(reason)` when the TR precondition was UNMET (the persist was refused, nothing
    /// written); `None` otherwise.
    pub precondition_unmet: Option<String>,
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name (`ObligationDischarged` | `ObligationUnknown`).
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed `(formula × standpoint × KB-world)` conjecture node IRI.
    pub node_iri: String,
    /// The deterministic N-Triples body [`project_conjecture_verdict`] emitted.
    pub verdict_nt: String,
    /// The TR receipt gating the persist (rendered as the transaction summary by callers).
    pub receipt: TxReceipt,
}

/// The shared evaluation core's result: everything computed by parsing, testing, and
/// projecting one conjecture verdict, before either public entry decides what to do with it.
struct ConjectureEvaluation {
    lifecycle: String,
    information: String,
    evaluation: String,
    completeness: String,
    discharge: String,
    witness: Option<ConjectureRunWitness>,
    node_iri: String,
    verdict_nt: String,
    /// The candidate's content-addressing key, needed by the persisting tail's audit call id.
    content_key: String,
}

/// Parse the candidate document and KB, re-home the KB into the isolated scenario world, run
/// the native engine, and project the verdict to deterministic N-Triples. The SINGLE
/// evaluation path shared by [`run_conjecture_test_pure`] and [`run_conjecture_test`] — neither
/// TR-gates nor writes anything; that is each caller's own tail.
///
/// # Errors
///
/// Returns an error if the candidate document does not name exactly one candidate formula, if
/// the KB does not parse, if the native engine fails (see [`conjecture_test`]), or if a
/// refutation names a compound candidate with no soundly-derivable forbidden predicate.
fn evaluate_conjecture(
    formula_ttl: &str,
    kb_ttl: &str,
    standpoint: &str,
    math_conjecture: Option<&str>,
    max_steps: Option<u64>,
    max_answers: Option<usize>,
) -> gmeow_errors::Result<ConjectureEvaluation> {
    // (1) Parse the candidate document and extract exactly one candidate formula.
    let candidate = parse_candidate_formula(formula_ttl)?;

    // (2) Parse the KB and re-home every triple into the isolated scenario world, so the
    //     world-scoped DL calculus joins the KB with the asserted / evaluated candidate.
    let kb = rehome_kb_into_scenario(kb_ttl)?;

    // (3) Run the engine. The KB is borrowed and never mutated (isolation is inherent).
    let kb_world = format!(
        "{CONJECTURE_SCENARIO_WORLD}#kb-{}",
        sha256_hex(kb_ttl.as_bytes())
    );
    // The optional post-hoc closure-size ceiling (criterion (2)): a bound the surfaces MAY pass
    // and the oracle NEVER sees (it is applied above the run). Both `None` ⟹ unbounded.
    let budget = Budget {
        max_answers,
        max_steps,
    };
    let answer = conjecture_test(
        kb.as_ref(),
        CONJECTURE_SCENARIO_WORLD,
        &candidate,
        standpoint,
        &[],
        &budget,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture_test failed: {e}"),
        })
    })?;

    // (4) Project the verdict → deterministic N-Triples, and mint the content-addressed
    //     (formula × standpoint × KB-world) conjecture node IRI.
    let content_key = candidate.content_key();
    // The anti-conjecture leg's forbidden predicate: the refuted formula's PRINCIPAL
    // predicate (the predicate the closure must never draw). A refuted formula that names no
    // single predicate (a compound conjunction / disjunction / implication / biconditional)
    // has no soundly-derivable forbidden predicate — its `logic:NonEntailmentObligation`
    // forbidden predicate is a reviewer decision — so we HARD-FAIL rather than fabricate one
    // or emit a shape-invalid obligation node (Constitution: no fabrication, no optionality).
    let forbidden_predicate = candidate.principal_predicate();
    if answer.lifecycle == ConjectureLifecycleState::RefutedInStandpoint
        && forbidden_predicate.is_none()
    {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "conjecture refuted in standpoint <{standpoint}>, but its candidate formula \
                 is compound and names no single predicate: the anti-conjecture \
                 logic:NonEntailmentObligation's forbidden predicate cannot be soundly \
                 derived and must be a reviewer decision. Refute an atomic or universally \
                 quantified single-predicate claim, or author the obligation directly."
            ),
        }));
    }
    let verdict_input = ConjectureVerdictInput {
        content_key: &content_key,
        standpoint,
        kb_world: &kb_world,
        answer: &answer,
        math_conjecture,
        forbidden_predicate: forbidden_predicate.as_deref(),
    };
    let verdict_nt = project_conjecture_verdict(&verdict_input);
    let node_iri = conjecture_node_iri(&verdict_input);

    let verdict = &answer.verdict;
    let witness = answer.witness.as_ref().map(|w| ConjectureRunWitness {
        individual: w.individual.clone(),
        world: w.world.clone(),
        premises: w
            .premises
            .iter()
            .map(|(s, p, o)| format!("{s} {p} {o}"))
            .collect(),
    });
    let lifecycle = answer.lifecycle.wire().to_string();
    let information = verdict.information.wire().to_string();
    let evaluation = verdict.evaluation.wire().to_string();
    let completeness = verdict.completeness.wire().to_string();
    let discharge = answer.discharge.local_name().to_string();

    Ok(ConjectureEvaluation {
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        content_key,
    })
}

/// Run one conjecture test PURELY — the issue's "test" leg: evaluate the candidate against the
/// KB and project the verdict, but NEVER TR-gate and NEVER append to the conjecture library.
/// Behind the MCP `conjecture_test` tool. For the committing counterpart ("store"), see
/// [`run_conjecture_test`].
///
/// # Errors
///
/// See [`evaluate_conjecture`].
pub fn run_conjecture_test_pure(
    input: &ConjectureRunPureInput,
) -> gmeow_errors::Result<ConjecturePureOutput> {
    let ConjectureRunPureInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    } = *input;
    let eval = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;
    Ok(ConjecturePureOutput {
        lifecycle: eval.lifecycle,
        information: eval.information,
        evaluation: eval.evaluation,
        completeness: eval.completeness,
        discharge: eval.discharge,
        witness: eval.witness,
        node_iri: eval.node_iri,
        verdict_nt: eval.verdict_nt,
    })
}

/// Run one conjecture test end-to-end — the issue's "store" leg: evaluate the candidate
/// against the KB (see [`evaluate_conjecture`]), TR-gate the write on the `persistConjecture`
/// schema, and — on a committed, precondition-met run — APPEND the verdict segment plus a
/// cold-auditable trajectory segment to the append-only conjecture library.
///
/// This is the shared persisting core behind both the MCP `store_conjecture` tool and the
/// `gmeow conjecture test` CLI subcommand. It never mutates the caller's KB (isolation is
/// inherent) and, on a `dry_run` or a precondition-unmet run, writes nothing.
///
/// # Errors
///
/// Returns an error if [`evaluate_conjecture`] fails, or if the TR transaction or the library
/// append fails.
pub fn run_conjecture_test(
    input: &ConjectureRunInput,
) -> gmeow_errors::Result<ConjectureRunOutput> {
    let ConjectureRunInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        dry_run,
        max_steps,
        max_answers,
    } = *input;

    let ConjectureEvaluation {
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        content_key,
    } = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;

    // (5) TR-gate the write on the persistConjecture schema. The precondition — a verdict has
    //     been presented — obtains once the engine returns a verdict (evaluation above).
    let obtains = [MCP_CONJECTURE_VERDICT_PRESENTED];
    let receipt = execute_memory_txn(MCP_PERSIST_CONJECTURE_SCHEMA, &obtains, dry_run)?;

    let mut out = ConjectureRunOutput {
        committed: false,
        dry_run,
        precondition_unmet: None,
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        receipt,
    };

    match &out.receipt {
        TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
            out.precondition_unmet = Some(reason.clone());
            return Ok(out);
        }
        // Sandbox run: the verdict is observed, nothing is appended to the library.
        TxReceipt::HypotheticalSuccess { .. } => return Ok(out),
        TxReceipt::CommittedSuccess { .. } => {}
    }

    // (6) Committed: APPEND the verdict segment plus its cold-auditable trajectory segment
    //     (keyed to a content-addressed call id) to the append-only library — TOGETHER, as ONE
    //     atomic file replace under ONE held lock, so a failure building or committing either
    //     segment can never leave the library holding the verdict without its audit record.
    let path = conjecture_path()?;
    let verdict_segment = build_conjecture_verdict_segment(&out.verdict_nt)?;
    let call_id = format!(
        "urn:gmeow:conjecture-call:{}",
        sha256_hex(format!("{}\u{1}{content_key}", out.node_iri).as_bytes())
    );
    let audit_segment = build_audit_segment(
        &call_id,
        MCP_PERSIST_CONJECTURE_SCHEMA,
        &obtains,
        "1970-01-01T00:00:00Z",
    );
    with_conjecture_lock(&path, || {
        append_conjecture_segments(&path, &[verdict_segment, audit_segment])
    })?;
    out.committed = true;
    Ok(out)
}

fn language_tag_map(dataset: &purrdf::RdfDataset) -> BTreeMap<String, String> {
    let graph = fold_arena::Graph::from_dataset(dataset);
    let graph = &graph;
    let iri_index: HashMap<&str, usize> = graph
        .terms
        .iter()
        .enumerate()
        .filter_map(|(idx, term)| {
            (term.kind == fold_arena::TermKind::Iri)
                .then(|| term.value.as_deref().map(|value| (value, idx)))
                .flatten()
        })
        .collect();
    let Some(type_tid) = iri_index.get(RDF_TYPE).copied() else {
        return BTreeMap::new();
    };
    let Some(language_tid) = iri_index.get(LANGUAGE_CLASS).copied() else {
        return BTreeMap::new();
    };
    let Some(language_tag_tid) = iri_index.get(LANGUAGE_TAG).copied() else {
        return BTreeMap::new();
    };
    let Some(bcp47_tid) = iri_index.get(BCP47_TAG).copied() else {
        return BTreeMap::new();
    };
    let subjects: BTreeSet<usize> = graph
        .quads
        .iter()
        .filter_map(|&(s, p, o, _)| (p == type_tid && o == language_tid).then_some(s))
        .collect();
    let mut out = BTreeMap::new();
    for subject in subjects {
        let internal = graph.quads.iter().find_map(|&(s, p, o, _)| {
            (s == subject && p == language_tag_tid)
                .then(|| graph.terms.get(o).and_then(|term| term.value.as_deref()))
                .flatten()
        });
        let bcp = graph.quads.iter().find_map(|&(s, p, o, _)| {
            (s == subject && p == bcp47_tid)
                .then(|| graph.terms.get(o).and_then(|term| term.value.as_deref()))
                .flatten()
        });
        if let (Some(internal), Some(bcp)) = (internal, bcp) {
            out.insert(internal.to_ascii_lowercase(), bcp.to_ascii_lowercase());
        }
    }
    out
}

fn resolve_lang(
    raw: Option<&str>,
    tag_map: &BTreeMap<String, String>,
    available: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(vec!["en".to_string()]);
    };
    if raw.trim().is_empty() {
        return Ok(vec!["en".to_string()]);
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for token in raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let normalized = if is_internal_tag(token) {
            let token_lower = token.to_ascii_lowercase();
            tag_map
                .get(&token_lower)
                .map(|tag| tag.to_ascii_lowercase())
                .ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: unknown_language(token, available),
                    })
                })?
        } else {
            token.to_ascii_lowercase()
        };
        if !available.contains(&normalized) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: unknown_language(token, available),
            }));
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        Ok(vec!["en".to_string()])
    } else {
        Ok(out)
    }
}

fn is_internal_tag(lang: &str) -> bool {
    let lower = lang.to_ascii_lowercase();
    let Some(suffix) = lower.strip_prefix("x-gmeow-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn unknown_language(tag: &str, available: &BTreeSet<String>) -> String {
    let mut tags: Vec<&str> = available.iter().map(String::as_str).collect();
    tags.sort_by_key(|tag| (*tag != "en", *tag));
    format!(
        "unknown language tag '{tag}'. Available languages: {}",
        tags.join(", ")
    )
}

fn lang_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "lang").then_some(value)
    })
}

fn memory_path() -> gmeow_errors::Result<PathBuf> {
    if let Ok(path) = env::var("GMEOW_MEMORY_PATH")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path).expand_home());
    }
    let home = home_dir().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "neither HOME nor USERPROFILE is set and GMEOW_MEMORY_PATH is empty"
                .to_string(),
        })
    })?;
    Ok(Path::new(&home).join(".gmeow").join("memory.gts"))
}

fn home_dir() -> Option<String> {
    env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok()
}

trait ExpandHome {
    fn expand_home(self) -> PathBuf;
}

impl ExpandHome for PathBuf {
    fn expand_home(self) -> PathBuf {
        let Some(raw) = self.to_str() else {
            return self;
        };
        if raw == "~" {
            return home_dir().map(PathBuf::from).unwrap_or(self);
        }
        if let Some(rest) = raw.strip_prefix("~/")
            && let Some(home) = home_dir()
        {
            return Path::new(&home).join(rest);
        }
        self
    }
}

fn tool(name: &str, description: &str, properties: &[(&str, &str)]) -> Value {
    let required: Vec<&str> = properties
        .iter()
        .filter_map(|(prop, _)| {
            let required_by_name = matches!(
                *prop,
                "term"
                    | "text"
                    | "claim_id"
                    | "conjecture_id"
                    | "path"
                    | "target_iri"
                    | "data"
                    | "format"
                    | "query"
                    | "subject"
                    | "predicate"
                    | "object_value"
                    | "graph"
            );
            // Carve-outs: an arg whose shared name is required everywhere else is
            // OPTIONAL for a specific tool, so marking it required THERE would advertise
            // a dishonest schema. `competency_questions` accepts an optional `term` (the
            // whole-index form omits it); `recall` accepts an optional `query` (an empty
            // query recalls everything).
            let optional_here = (name == "competency_questions" && *prop == "term")
                || (name == "recall" && *prop == "query");
            (required_by_name && !optional_here).then_some(*prop)
        })
        .collect();
    let props = properties
        .iter()
        .map(|(name, kind)| ((*name).to_string(), json!({"type": kind})))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        },
    })
}

/// Canonicalize a caller-supplied RDF `format` token to the exact id
/// `gmeow_validate::data_validate` accepts. Accepts the common aliases per family;
/// an UNRECOGNIZED format is a HARD FAIL (the accepted set is listed in the error)
/// so a mistyped format can never silently mis-parse.
fn canonical_rdf_format(format: &str) -> gmeow_errors::Result<&'static str> {
    let normalized = format.trim().to_ascii_lowercase();
    let token = match normalized.as_str() {
        "turtle" | "ttl" | "text/turtle" => "turtle",
        "ntriples" | "nt" | "n-triples" => "n-triples",
        "nquads" | "nq" | "n-quads" => "n-quads",
        "trig" => "trig",
        "rdfxml" | "rdf+xml" | "xml" | "rdf" => "rdf+xml",
        "jsonld" | "json-ld" => "json-ld",
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "validate_local: unrecognized RDF format `{other}`; accepted: \
                     turtle|ttl|text/turtle, ntriples|nt|n-triples, nquads|nq|n-quads, trig, \
                     rdfxml|rdf+xml|xml|rdf, jsonld|json-ld"
                ),
            }));
        }
    };
    Ok(token)
}

fn resource(uri: &str, name: &str, description: &str, mime: &str) -> Value {
    json!({
        "uri": uri,
        "name": name,
        "description": description,
        "mimeType": mime,
    })
}

fn tool_text(text: String, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
    .to_string()
}

/// Resolve the `path` argument of the `slice_quality` tool to a concrete slice
/// directory, enforcing that it stays inside the repository's `slices/` tree.
///
/// The raw argument is joined onto the repo root (so callers keep passing a
/// root-relative `slices/<group>/<name>`), then canonicalized and checked for
/// containment under the canonical `slices/` directory. An absolute path or a `../`
/// sequence that escapes `slices/` — the classic path-traversal vectors — is
/// rejected rather than scored, so the tool can never be steered to read outside the
/// slice tree.
fn resolve_slice_dir(root: &Path, rel: &str) -> gmeow_errors::Result<PathBuf> {
    let err = |message: String| gmeow_errors::Diag::of_kind(crate::error::Mcp { message });
    let slices_root = root.join("slices");
    let canon_root = slices_root.canonicalize().map_err(|e| {
        err(format!(
            "slice_quality: cannot resolve slices root {}: {e}",
            slices_root.display()
        ))
    })?;
    let candidate = root.join(rel);
    let canon = candidate.canonicalize().map_err(|e| {
        err(format!(
            "slice_quality: cannot resolve slice path {rel:?} under the slices tree: {e}"
        ))
    })?;
    if !canon.starts_with(&canon_root) {
        return Err(err(format!(
            "slice_quality: path {rel:?} escapes the slices/ tree (path traversal rejected)"
        )));
    }
    Ok(canon)
}

/// The shared unknown-term hard fail for the resolution guard and `doc_card`: a
/// query that resolves to no bundled GMEOW term.
fn unknown_term_err(term: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Mcp {
        message: format!(
            "unknown term `{term}`: does not resolve to a bundled GMEOW term \
             (expected a CURIE, local name, IRI, or label)"
        ),
    })
}

/// The shared cross-namespace-collision hard fail: a bare local name that exactly
/// names terms in more than one namespace. Carries the TYPED `McpAmbiguousTerm`
/// code (distinct from the generic unknown-term `Mcp`), naming the query and listing
/// the sorted candidate CURIEs — the MCP twin of `gmeow-cli.describe.ambiguous`.
fn ambiguous_term_err(term: &str, candidates: &[String]) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::McpAmbiguousTerm {
        message: format!(
            "ambiguous term `{term}`: names terms in multiple namespaces: {}",
            candidates.join(" / ")
        ),
    })
}

/// The proof-carrying completeness class of a governed reasoning `result`, shared by
/// [`McpView::run_verify_graph`] and [`McpView::run_explain_quad`].
///
/// This is a thin wrapper over [`CoherenceOutcome::class_local_name_for`] — the SAME
/// completeness-gate trichotomy the bundle-level coherence certifier
/// (`certificate.rs`) uses to mint a real `logic:CoherenceCertificate`, under the
/// default classical policy ([`ContradictionPolicy::DEFAULT`], gaps and gluts both
/// forbidden — the conservative default a tool surface with no contract-specific
/// policy in hand must use). It requires BOTH:
/// * a CONCLUSIVE closure (a completed run OR a complete-for-fragment answer,
///   [`ReasoningResult::is_conclusive`]) — otherwise the strictly-weaker
///   `CoherenceCheckAttestation` claim is the strongest honest one, never a
///   certificate; AND
/// * no forbidden violation (a witnessed DL glut under the forbid-glut default) —
///   an inconsistent-but-conclusive closure is REFUSED (`"Refused"`), never labeled
///   `CoherenceCertificate`, on EITHER caller's path (no ad-hoc per-caller downgrade:
///   `run_verify_graph` no longer bolts one on separately — see its own docs).
///
/// The gate is never re-derived inline here (GREENFIELD/no-duplicate-logic) — the
/// one gate lives in `certificate.rs`, so the two tool paths can never diverge on
/// whether a genuine certificate is warranted.
fn completeness_class(result: &ReasoningResult) -> &'static str {
    CoherenceOutcome::class_local_name_for(result, ContradictionPolicy::DEFAULT)
}

/// `true` iff [`completeness_class`] would answer `"Refused"` for `result` — the
/// SAME gate, under the SAME default policy, exposed as a boolean for
/// [`McpView::run_verify_graph`]'s `coherent` field. Deriving `coherent` from this
/// (rather than from an independent signal such as "did any bad-example verify
/// query match") is what guarantees `coherent` and `class_local_name` can never
/// disagree: a conclusive DL glut that trips no bad-example query still REFUSES via
/// this gate, so `coherent` is forced `false` in lockstep with `class_local_name`
/// being `"Refused"` — never `coherent:true` alongside `class:Refused`.
fn completeness_refused(result: &ReasoningResult) -> bool {
    CoherenceOutcome::is_refused_for(result, ContradictionPolicy::DEFAULT)
}

/// Read the SCOPED COHERENCE CERTIFICATE carried in the bundle's `graph/attestations`
/// named graph and map it to the proof-carrying read envelope (R6).
///
/// This is BUDGET-FREE and REASON-FREE: the certificate was computed ONCE at pipeline
/// time (`crate::stages::carrier`, over the whole assembled carrier) and folded into the
/// bundle; this reads it straight off the loaded dataset — it NEVER re-reasons. The class
/// (`logic:CoherenceCertificate` vs the strictly-weaker `logic:CoherenceCheckAttestation`)
/// is read from the carried `rdf:type`, so an attestation is NEVER silently reported as a
/// certificate. The completeness/evaluation axes are recovered from the linked
/// `logic:ReasoningResult` node's `logic:resultCompleteness` / `logic:resultEvaluation`
/// status individuals.
///
/// # Errors
/// HARD-FAILS if the bundle carries no coherence artifact in `graph/attestations` (there
/// is NO silent recompute fallback — after the terminal fold every gmeow.gts carries one),
/// or if it carries more than one distinct coherence subject (an ambiguous bundle).
fn coherence_certificate_envelope(dataset: &purrdf::RdfDataset) -> gmeow_errors::Result<Value> {
    use purrdf::RdfTerm;

    let graph = crate::stages::release::GRAPH_ATTESTATIONS;
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let cert_type = format!("{LOGIC_NAMESPACE}CoherenceCertificate");
    let att_type = format!("{LOGIC_NAMESPACE}CoherenceCheckAttestation");

    let iri_of = |term: &RdfTerm| -> Option<String> {
        match term {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        }
    };
    let literal_of = |term: &RdfTerm| -> Option<String> {
        match term {
            RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
            _ => None,
        }
    };
    let in_graph = |g: &Option<RdfTerm>| matches!(g, Some(RdfTerm::Iri(iri)) if iri == graph);

    // 1. Locate THE coherence subject and its class local name (Certificate vs the
    //    strictly-weaker Attestation). A Refused outcome emits nothing, so a carried
    //    subject is always one of the two issued artifacts.
    let mut carried: Option<(String, &'static str)> = None;
    for q in dataset.owned_quads() {
        if !in_graph(&q.graph_name) || q.predicate != rdf_type {
            continue;
        }
        let (Some(subject), Some(obj)) = (iri_of(&q.subject), iri_of(&q.object)) else {
            continue;
        };
        let class_local = if obj == cert_type {
            "CoherenceCertificate"
        } else if obj == att_type {
            "CoherenceCheckAttestation"
        } else {
            continue;
        };
        match &carried {
            Some((existing, _)) if *existing != subject => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "coherence_certificate: the bundle carries more than one distinct \
                         coherence subject in {graph} ({existing} and {subject}); an ambiguous \
                         coherence artifact set is a hard failure"
                    ),
                }));
            }
            Some(_) => {}
            None => carried = Some((subject, class_local)),
        }
    }
    let Some((subject, class_local)) = carried else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "coherence_certificate: the bundle carries no coherence certificate or \
                 attestation in {graph} (no logic:CoherenceCertificate / \
                 logic:CoherenceCheckAttestation subject); the bundle is missing its \
                 pipeline-time coherence proof — refusing to recompute"
            ),
        }));
    };

    // 2. Gather the scoped payload off the subject's quads.
    let p = |local: &str| format!("{LOGIC_NAMESPACE}{local}");
    let (
        pred_bundle,
        pred_axiom,
        pred_contract,
        pred_engine,
        pred_fragment,
        pred_policy,
        pred_loss,
        pred_forbidden,
        pred_summarizes,
    ) = (
        p("bundleHash"),
        p("axiomHash"),
        p("contractHash"),
        p("engine"),
        p("certifiedFragment"),
        p("contradictionPolicy"),
        p("projectionLoss"),
        p("forbiddenViolationWitness"),
        p("summarizesResult"),
    );
    let mut bundle_hash: Option<String> = None;
    let mut contract_hash: Option<String> = None;
    let mut engine: Option<String> = None;
    let mut certified_fragment: Option<String> = None;
    let mut contradiction_policy: Option<String> = None;
    let mut result_node: Option<String> = None;
    let mut axiom_hashes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut projection_losses: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut forbidden_violations: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for q in dataset.owned_quads() {
        if !in_graph(&q.graph_name) || iri_of(&q.subject).as_deref() != Some(subject.as_str()) {
            continue;
        }
        let pred = q.predicate.as_str();
        if pred == pred_bundle {
            bundle_hash = literal_of(&q.object);
        } else if pred == pred_axiom {
            if let Some(v) = literal_of(&q.object) {
                axiom_hashes.insert(v);
            }
        } else if pred == pred_contract {
            contract_hash = literal_of(&q.object);
        } else if pred == pred_engine {
            engine = literal_of(&q.object);
        } else if pred == pred_fragment {
            certified_fragment = literal_of(&q.object);
        } else if pred == pred_policy {
            contradiction_policy = iri_of(&q.object);
        } else if pred == pred_loss {
            if let Some(v) = literal_of(&q.object) {
                projection_losses.insert(v);
            }
        } else if pred == pred_forbidden {
            if let Some(v) = iri_of(&q.object) {
                forbidden_violations.insert(v);
            }
        } else if pred == pred_summarizes {
            result_node = iri_of(&q.object);
        }
    }

    // 3. Recover the two completeness-gate axes off the linked logic:ReasoningResult node.
    let pred_completeness = p("resultCompleteness");
    let pred_evaluation = p("resultEvaluation");
    let mut completeness: Option<String> = None;
    let mut evaluation: Option<String> = None;
    if let Some(node) = &result_node {
        for q in dataset.owned_quads() {
            if !in_graph(&q.graph_name) || iri_of(&q.subject).as_deref() != Some(node.as_str()) {
                continue;
            }
            if q.predicate == pred_completeness {
                completeness = iri_of(&q.object)
                    .and_then(|iri| iri.strip_prefix(LOGIC_NAMESPACE).map(str::to_owned))
                    .and_then(|local| CompletenessStatus::from_local(&local))
                    .map(|s| s.wire().to_owned());
            } else if q.predicate == pred_evaluation {
                evaluation = iri_of(&q.object)
                    .and_then(|iri| iri.strip_prefix(LOGIC_NAMESPACE).map(str::to_owned))
                    .and_then(|local| EvaluationStatus::from_local(&local))
                    .map(|s| s.wire().to_owned());
            }
        }
    }

    // bundle_hash / axiom_hashes are the load-bearing tamper surface — a bundle that
    // carries a typed coherence subject but no bundle identity is corrupt, not partial.
    let Some(bundle_hash) = bundle_hash else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "coherence_certificate: coherence subject {subject} carries no \
                 logic:bundleHash (corrupt coherence artifact)"
            ),
        }));
    };

    Ok(json!({
        "ok": true,
        "issues_certificate": class_local == "CoherenceCertificate",
        "is_refused": false,
        "class_local_name": class_local,
        "bundle_hash": bundle_hash,
        "axiom_hashes": axiom_hashes.into_iter().collect::<Vec<_>>(),
        "contract_hash": contract_hash,
        "engine": engine,
        "certified_fragment": certified_fragment,
        "completeness": completeness,
        "evaluation": evaluation,
        "contradiction_policy": contradiction_policy,
        "projection_losses": projection_losses.into_iter().collect::<Vec<_>>(),
        "forbidden_violations": forbidden_violations.into_iter().collect::<Vec<_>>(),
    }))
}

/// Build the canonical N3 object surface for `explain_quad` — the EXACT byte form a
/// [`Row`]'s `obj` carries ([`term_display`] of the object [`TermValue`]) — so the
/// target reifier joins the reasoner's row set. Reuses the SAME serializer the row
/// builder feeds off (`gmeow_logic::provenance::term_display`); it never hand-rolls a
/// second quoting/escaping surface (a mismatch would forge a false "not in closure").
///
/// `kind` is `iri` or `literal`; when the caller omits it, it is inferred from the
/// object surface ([`infer_object_kind`]). `datatype` types a literal only — pairing
/// it with an `iri` object is a contradictory request and a HARD FAIL, as is any
/// `kind` other than `iri`/`literal`.
fn object_term_n3(
    value: &str,
    kind: Option<&str>,
    datatype: Option<&str>,
) -> gmeow_errors::Result<String> {
    let kind = kind.unwrap_or_else(|| infer_object_kind(value));
    let term = match kind {
        "iri" => {
            if datatype.is_some() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message:
                        "explain_quad: object_datatype is only valid for object_kind=literal, \
                              not an IRI object"
                            .to_owned(),
                }));
            }
            TermValue::iri(value)
        }
        "literal" => match datatype {
            Some(dt) => TermValue::typed_literal(value, dt),
            None => TermValue::simple_literal(value),
        },
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "explain_quad: object_kind must be `iri` or `literal`, got `{other}`"
                ),
            }));
        }
    };
    Ok(term_display(&term))
}

/// Infer whether a bare `object_value` is an IRI or a literal when the caller omits
/// `object_kind`. An absolute IRI carries a URI scheme (`ALPHA *( ALPHA / DIGIT / "+"
/// / "-" / "." ) ":"`) and no ASCII whitespace and is not a quoted literal; anything
/// else is a literal lexical form. The inference is only a convenience default — a
/// mis-inference cannot corrupt a result: a wrong reifier simply fails to join the
/// closure and HARD-FAILS as "not in closure", never a fabricated proof.
fn infer_object_kind(value: &str) -> &'static str {
    if is_absolute_iri_shape(value) {
        "iri"
    } else {
        "literal"
    }
}

/// `true` iff `value` has the shape of an absolute IRI: a non-empty
/// `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` scheme followed by `:`, with no ASCII
/// whitespace and no leading `"` (which would mark a literal).
fn is_absolute_iri_shape(value: &str) -> bool {
    if value.starts_with('"') || value.chars().any(|c| c.is_ascii_whitespace()) {
        return false;
    }
    let Some(colon) = value.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let mut scheme = value[..colon].chars();
    let first = scheme.next().expect("colon>0 guarantees a scheme char");
    first.is_ascii_alphabetic()
        && scheme.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Locate the single row in `rows` whose reifier is `target_reifier` in world
/// `graph`, WORLD-DISAMBIGUATING the join (R1). The reifier alone is not a unique key:
/// the same `(S, P, O)` in two worlds shares a reifier, so the intended world must be
/// supplied.
///
/// * No row (in any world) carries the reifier → HARD FAIL `quad not in closure`.
/// * `graph` resolves to exactly one row → that row.
/// * `graph` resolves to no row but the reifier lives in exactly one OTHER world →
///   HARD FAIL `quad not in closure` naming that world (the caller asked the wrong one).
/// * The reifier spans MORE THAN ONE world and `graph` does not resolve it to exactly
///   one row → HARD FAIL ambiguity (never an arbitrary pick).
///
/// When `graph` resolves to several rows but the reifier lives in ONLY that world, the
/// engine's `(graph, reifier)` identity is last-wins; this returns that same last row
/// so the reconstructed root matches the index's resolution deterministically.
fn locate_explain_target(
    rows: &[Row],
    target_reifier: &str,
    graph: &str,
) -> gmeow_errors::Result<usize> {
    let matches: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| reifier_from_row(row) == target_reifier)
        .map(|(index, _)| index)
        .collect();
    if matches.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "explain_quad: quad not in closure — reifier <{target_reifier}> matches no \
                 derived or asserted quad in the reasoned bundle (check the (subject, predicate, \
                 object) surface and the max_steps budget)"
            ),
        }));
    }
    // The distinct worlds the reifier appears in (sorted, for a deterministic message).
    let worlds: BTreeSet<&str> = matches
        .iter()
        .map(|&index| rows[index].graph.as_str())
        .collect();
    // The requested world's matching rows, in input order (last = the engine identity).
    let in_world: Vec<usize> = matches
        .iter()
        .copied()
        .filter(|&index| rows[index].graph == graph)
        .collect();

    let ambiguity = || {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "explain_quad: ambiguous quad — reifier <{target_reifier}> matches quads in {} \
                 distinct worlds ({}); the supplied graph <{graph}> does not resolve it to \
                 exactly one row. Re-issue with graph set to the intended world.",
                worlds.len(),
                worlds.iter().copied().collect::<Vec<_>>().join(", ")
            ),
        })
    };

    match in_world.len() {
        1 => Ok(in_world[0]),
        0 => {
            if worlds.len() > 1 {
                Err(ambiguity())
            } else {
                let other = worlds.iter().next().copied().unwrap_or_default();
                Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "explain_quad: quad not in closure — reifier <{target_reifier}> matches a \
                         quad in world <{other}>, not the supplied graph <{graph}>"
                    ),
                }))
            }
        }
        // The requested world carries the reifier on several rows. If the reifier ALSO
        // spans other worlds it is genuinely ambiguous; otherwise the engine identity is
        // last-wins, so we deterministically explain that last row.
        _ => {
            if worlds.len() > 1 {
                Err(ambiguity())
            } else {
                Ok(*in_world.last().expect("len > 1 has a last"))
            }
        }
    }
}

fn required_str<'a>(args: &'a Value, key: &str) -> gmeow_errors::Result<&'a str> {
    optional_str(args, key).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} is required"),
        })
    })
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn optional_f64(args: &Value, key: &str) -> gmeow_errors::Result<Option<f64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_f64().map(Some).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("{key} must be a finite number"),
            })
        }),
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be a number"),
        })),
    }
}

fn optional_limit(args: &Value, key: &str) -> gmeow_errors::Result<Option<usize>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n.as_i64().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be an integer"),
                })
            })?;
            usize::try_from(value).map(Some).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be non-negative"),
                })
            })
        }
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be an integer"),
        })),
    }
}

/// A strict non-negative `u64` argument (absent/null → `None`), used for the `max_steps`
/// closure-size ceiling: present must be a non-negative integer, anything else is a HARD FAIL.
fn optional_step_count(args: &Value, key: &str) -> gmeow_errors::Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n.as_i64().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be an integer"),
                })
            })?;
            u64::try_from(value).map(Some).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be non-negative"),
                })
            })
        }
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be an integer"),
        })),
    }
}

/// A strict boolean argument: present-and-bool → `Some`, absent/null → `None`, anything else is a
/// HARD FAIL (no silent coercion — `dry_run` is a named default, not a degraded fallback).
fn optional_bool_checked(args: &Value, key: &str) -> gmeow_errors::Result<Option<bool>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be a boolean"),
        })),
    }
}

// ── Transaction-Logic execution of the memory write triad ────────────────────
//
// The two memory WRITE tools run as Transaction-Logic transactions: the canonical action theory
// is the single authority, the engine's executional entailment over the real start state is the
// commit gate, `dry_run` selects the hypothetical (sandbox) operator, and every committed turn is
// recorded with the audit context the trajectory audit reads.

/// The canonical memory-triad action theory — the SINGLE authority for how store_claim and
/// revise_belief behave as transactions (their `logic:precondition` / `logic:effect` /
/// `logic:compensation`). Embedded at build so the shipped `gmeow` runs repo-free; the slice
/// file is the one source of truth, and the worked example and conformance case reference these
/// same schema IRIs (they encode no second copy).
const MCP_ACTION_POLICY_TTL: &str =
    include_str!("../../../slices/extensions/agentic/examples/mcp-action-policy.ttl");

/// The transient world the TR run reasons in — a fresh in-memory store per call, NEVER persisted.
/// The executed verdict gates the write; the materialized outcome rides the tool response.
const TXN_WORLD: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec";
const TXN_ROOT: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/txn";
const TXN_START: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/start";

/// The canonical action-schema and situation IRIs defined by `mcp-action-policy.ttl`.
const MCP_STORE_CLAIM_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/storeClaim";
const MCP_REVISE_BELIEF_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/reviseBelief";
const MCP_WELL_FORMED_CLAIM: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/wellFormedClaim";
const MCP_TARGET_CLAIM_EXISTS: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/targetClaimExists";
const MCP_CLAIM_IN_MEMORY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/claimInMemory";

/// The `persistConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The `conjecture_test` write triad instantiates this schema; the
/// executional-entailment verdict over the precondition gates the append to the library.
const MCP_PERSIST_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/persistConjecture";
const MCP_CONJECTURE_VERDICT_PRESENTED: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureVerdictPresented";

/// The `withdrawConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The compensating author-withdrawal counterpart of
/// `persistConjecture` (P10, `logic:compensation`): the precondition — the conjecture is
/// still in the library (not already withdrawn) — gates the compensating append.
const MCP_WITHDRAW_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/withdrawConjecture";
const MCP_CONJECTURE_IN_LIBRARY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureInLibrary";

/// The single, fixed ISOLATED scenario world every conjecture test reasons in. The KB the
/// caller supplies is re-homed into this world (so the world-scoped DL calculus joins the
/// KB facts with the asserted / evaluated candidate), and the run is inherently isolated —
/// [`conjecture_test`] copies the KB into a fresh dataset and never mutates the input.
const CONJECTURE_SCENARIO_WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/agentic/conjecture/scenario";

const LOGIC_INSTANTIATES_SCHEMA: &str = "https://blackcatinformatics.ca/logic/instantiatesSchema";
const LOGIC_TRANSITION_FROM_STATE: &str =
    "https://blackcatinformatics.ca/logic/transitionFromState";
const LOGIC_SITUATION_OBTAINS: &str = "https://blackcatinformatics.ca/logic/situationObtains";
const LOGIC_PROPER_PART_OF: &str = "https://blackcatinformatics.ca/logic/properPartOf";
const GMEOW_AT_TIME: &str = "https://blackcatinformatics.ca/gmeow/atTime";
const GMEOW_EVENT_TEMPORAL_FRAME: &str = "https://blackcatinformatics.ca/gmeow/eventTemporalFrame";
const GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN: &str =
    "https://blackcatinformatics.ca/gmeow/temporalFrameUTCGregorian";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// The canonical action theory as N-Quads in [`TXN_WORLD`], parsed once from the embedded slice
/// file. HARD FAIL if the embedded authority does not parse — that is a build-time invariant, not
/// a runtime fallback (the `canonical_action_policy_parses` test guards it).
fn action_policy_nquads() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        let dataset = purrdf::parse_dataset(MCP_ACTION_POLICY_TTL.as_bytes(), "text/turtle", None)
            .expect("canonical mcp-action-policy.ttl must parse (single authority)");
        let mut lines: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            // The engine reads only the structural action theory (precondition / effect / ins /
            // del / compensation), all IRI→IRI — keep those and drop the annotation literals
            // (labels, comments) the executional-entailment run never consults.
            .filter(|quad| {
                matches!(quad.subject, purrdf::RdfTerm::Iri(_))
                    && matches!(quad.object, purrdf::RdfTerm::Iri(_))
            })
            .map(|quad| {
                format!(
                    "{} <{}> {} <{TXN_WORLD}> .",
                    quad.subject, quad.predicate, quad.object
                )
            })
            .collect();
        lines.sort();
        lines.join("\n")
    })
}

/// Build the per-call one-step transaction world: the canonical action theory plus this call's
/// primitive program (`root` instantiates `schema_iri`, transitions from the start state) and the
/// start state's obtaining situations (`obtains`, derived from REAL memory state).
fn txn_world_nquads(schema_iri: &str, obtains: &[&str]) -> String {
    use std::fmt::Write as _;
    let policy = action_policy_nquads();
    // Pre-size to hold the cached policy verbatim plus this call's primitive program and one
    // line per obtaining situation, so the build never reallocates.
    let mut nq = String::with_capacity(policy.len() + obtains.len() * 128 + 256);
    nq.push_str(policy);
    nq.push('\n');
    // `String`'s `fmt::Write` is infallible, so the formatting `Result` is discarded.
    let _ = writeln!(
        nq,
        "<{TXN_ROOT}> <{LOGIC_INSTANTIATES_SCHEMA}> <{schema_iri}> <{TXN_WORLD}> ."
    );
    let _ = writeln!(
        nq,
        "<{TXN_ROOT}> <{LOGIC_TRANSITION_FROM_STATE}> <{TXN_START}> <{TXN_WORLD}> ."
    );
    for situation in obtains {
        let _ = writeln!(
            nq,
            "<{TXN_START}> <{LOGIC_SITUATION_OBTAINS}> <{situation}> <{TXN_WORLD}> ."
        );
    }
    nq
}

/// Execute one memory write action as a TR transaction. `obtains` is the set of situations that
/// obtain at the start state (real state); the engine's executional entailment over them is the
/// commit gate. `dry_run` selects the hypothetical (sandbox) operator.
fn execute_memory_txn(
    schema_iri: &str,
    obtains: &[&str],
    dry_run: bool,
) -> gmeow_errors::Result<TxReceipt> {
    let nq = txn_world_nquads(schema_iri, obtains);
    let mode = if dry_run {
        CommitMode::Hypothetical
    } else {
        CommitMode::Committed
    };
    execute_transaction(&nq, TXN_WORLD, TXN_ROOT, mode).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: e.message().to_owned(),
        })
    })
}

/// The TR outcome rendered for the tool response.
fn txn_json(receipt: &TxReceipt) -> Value {
    match receipt {
        TxReceipt::CommittedSuccess { path_len, .. } => {
            json!({"committed": true, "succeeded": true, "path_len": path_len})
        }
        TxReceipt::CommittedFailure { reason } => {
            json!({"committed": true, "succeeded": false, "reason": reason})
        }
        TxReceipt::HypotheticalSuccess { witness } => {
            json!({"committed": false, "succeeded": true, "witness": witness})
        }
        TxReceipt::HypotheticalFailure { reason } => {
            json!({"committed": false, "succeeded": false, "reason": reason})
        }
    }
}

fn gts_iri(value: &str) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    }
}

fn gts_literal_dt(value: &str, datatype: usize) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Literal,
        value: Some(value.to_string()),
        datatype: Some(datatype),
        lang: None,
        direction: None,
        reifier: None,
    }
}

fn push_gts_term(terms: &mut Vec<GtsTerm>, term: GtsTerm) -> usize {
    terms.push(term);
    terms.len() - 1
}

/// Append the trajectory-audit context segment for a just-recorded `gmeow:ToolCall` to the SAME
/// `memory.gts`, keyed to `call_id`: the call's `logic:instantiatesSchema`, its single
/// `logic:properPartOf` turn anchor (one call = one anchor — the stateless server mints no shared
/// turn state), its `gmeow:atTime` and the single canonical `gmeow:eventTemporalFrame`
/// (UTC-Gregorian, P11), the anchor's `logic:transitionFromState` start state, and the start's
/// obtaining situations. This is exactly the shape `emit_trajectory_audits` reads, so a cold trajectory
/// audit of `memory.gts` (unioned with the canonical action theory) verifies the executed turn.
fn write_audit_segment(
    memory_path: &Path,
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<()> {
    let segment = build_audit_segment(call_id, schema_iri, obtains, at_time);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(memory_path)?;
    file.write_all(&segment)?;
    Ok(())
}

/// Build one trajectory-audit context segment's serialized bytes — the PURE, side-effect-free
/// half of [`write_audit_segment`], factored out so the conjecture-library commit path can
/// build the verdict segment AND its audit segment in memory and commit both together via
/// [`append_conjecture_segments`] (one atomic replace), rather than two separate appends where
/// the second can fail after the first has already landed.
fn build_audit_segment(
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> Vec<u8> {
    let anchor = format!("{call_id}#turn");
    let start = format!("{call_id}#start");

    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();

    let t_call = push_gts_term(&mut terms, gts_iri(call_id));
    let t_anchor = push_gts_term(&mut terms, gts_iri(&anchor));
    let t_start = push_gts_term(&mut terms, gts_iri(&start));

    let t_inst = push_gts_term(&mut terms, gts_iri(LOGIC_INSTANTIATES_SCHEMA));
    let t_schema = push_gts_term(&mut terms, gts_iri(schema_iri));
    quads.push((t_call, t_inst, t_schema, None));

    let t_ppo = push_gts_term(&mut terms, gts_iri(LOGIC_PROPER_PART_OF));
    quads.push((t_call, t_ppo, t_anchor, None));

    let t_dt = push_gts_term(&mut terms, gts_iri(XSD_DATETIME));
    let t_at_time_p = push_gts_term(&mut terms, gts_iri(GMEOW_AT_TIME));
    let t_at_time_o = push_gts_term(&mut terms, gts_literal_dt(at_time, t_dt));
    quads.push((t_call, t_at_time_p, t_at_time_o, None));

    let t_frame_p = push_gts_term(&mut terms, gts_iri(GMEOW_EVENT_TEMPORAL_FRAME));
    let t_frame_o = push_gts_term(&mut terms, gts_iri(GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN));
    quads.push((t_call, t_frame_p, t_frame_o, None));

    let t_tfs = push_gts_term(&mut terms, gts_iri(LOGIC_TRANSITION_FROM_STATE));
    quads.push((t_anchor, t_tfs, t_start, None));

    let t_so = push_gts_term(&mut terms, gts_iri(LOGIC_SITUATION_OBTAINS));
    for situation in obtains {
        let t_sit = push_gts_term(&mut terms, gts_iri(situation));
        quads.push((t_start, t_so, t_sit, None));
    }

    let mut writer = GtsWriter::new("ai-package");
    writer.add_terms(&terms);
    writer.add_quads(&quads);
    writer.to_bytes()
}

// ── Conjecture-library persistence (append-only GTS ai-package, TR-gated) ─────
//
// The conjecture library is a SEPARATE, append-only GTS collection — the read-only twin of
// `memory.gts`. Each `conjecture_test` commit appends one `ai-package` segment carrying the
// `project_conjecture_verdict` graph (a content-addressed, standpoint-scoped
// `logic:Conjecture` node with its embedded `logic:ReasoningResult` and refutation witness).
// It is NEVER folded into the base KB reasoning graph (R2): the reasoner reads
// `graph_dataset()` / the caller's KB, never `conjectures.gts`.

/// The conjecture-library path: `GMEOW_CONJECTURE_PATH` (home-expanded) when set, else
/// `~/.gmeow/conjectures.gts`. Mirrors [`memory_path`].
fn conjecture_path() -> gmeow_errors::Result<PathBuf> {
    if let Ok(path) = env::var("GMEOW_CONJECTURE_PATH")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path).expand_home());
    }
    let home = home_dir().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "neither HOME nor USERPROFILE is set and GMEOW_CONJECTURE_PATH is empty"
                .to_string(),
        })
    })?;
    Ok(Path::new(&home).join(".gmeow").join("conjectures.gts"))
}

/// A deterministic lowercase-hex SHA-256 of `bytes` (the KB-world content address seed).
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// One term of the projection's closed N-Triples subset (`<iri>` / `_:b` / `"lex"^^<dt>`).
enum NtTerm {
    Iri(String),
    Blank(String),
    Lit { lex: String, datatype: String },
}

/// Take one N-Triples term off the front of `s`, returning `(term, rest)`. Handles the
/// closed subset [`project_conjecture_verdict`] emits: `<iri>`, `_:id`, and `"lex"^^<dt>`
/// typed literals (the literal walk is char-based, so it is UTF-8 safe and un-escapes).
fn take_nt_term(s: &str) -> Option<(NtTerm, &str)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('<') {
        let end = rest.find('>')?;
        return Some((NtTerm::Iri(rest[..end].to_owned()), &rest[end + 1..]));
    }
    if let Some(rest) = s.strip_prefix("_:") {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        return Some((NtTerm::Blank(rest[..end].to_owned()), &rest[end..]));
    }
    if let Some(rest) = s.strip_prefix('"') {
        let mut lex = String::new();
        let mut chars = rest.char_indices();
        while let Some((idx, ch)) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some((_, '\\')) => lex.push('\\'),
                    Some((_, '"')) => lex.push('"'),
                    Some((_, 'n')) => lex.push('\n'),
                    Some((_, 'r')) => lex.push('\r'),
                    Some((_, 't')) => lex.push('\t'),
                    Some((_, c)) => lex.push(c),
                    None => return None,
                },
                '"' => {
                    // Past the closing quote: read the required `^^<dt>` typed-literal suffix.
                    let after = rest[idx + 1..].strip_prefix("^^")?;
                    let after = after.strip_prefix('<')?;
                    let end = after.find('>')?;
                    let datatype = after[..end].to_owned();
                    return Some((NtTerm::Lit { lex, datatype }, &after[end + 1..]));
                }
                _ => lex.push(ch),
            }
        }
        return None;
    }
    None
}

/// Intern one N-Triples node into the GTS term table (deduplicated by semantic value), a
/// literal first interning its datatype IRI so the literal term references a live datatype id.
fn intern_nt_term(
    node: &NtTerm,
    terms: &mut Vec<GtsTerm>,
    seen: &mut HashMap<String, usize>,
) -> usize {
    let sig = match node {
        NtTerm::Iri(iri) => format!("I\u{1}{iri}"),
        NtTerm::Blank(id) => format!("B\u{1}{id}"),
        NtTerm::Lit { lex, datatype } => format!("L\u{1}{datatype}\u{1}{lex}"),
    };
    if let Some(&id) = seen.get(&sig) {
        return id;
    }
    let term = match node {
        NtTerm::Iri(iri) => gts_iri(iri),
        NtTerm::Blank(id) => GtsTerm {
            kind: GtsTermKind::Bnode,
            value: Some(id.clone()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        },
        NtTerm::Lit { lex, datatype } => {
            let dt_id = intern_nt_term(&NtTerm::Iri(datatype.clone()), terms, seen);
            gts_literal_dt(lex, dt_id)
        }
    };
    let id = push_gts_term(terms, term);
    seen.insert(sig, id);
    id
}

/// Build one conjecture-verdict segment (the `project_conjecture_verdict` N-Triples body) as
/// a GTS `ai-package` segment's serialized bytes — the PURE, side-effect-free half of the old
/// `write_conjecture_segment`. The body is parsed into `(subject, predicate, object)` triples,
/// interned as GTS terms (IRIs, blank nodes, and typed literals with their datatype), and
/// written via [`GtsWriter`]. Building the bytes is separated from appending them so a caller
/// can assemble MULTIPLE segments (e.g. the verdict segment AND its audit segment) in memory
/// and commit them together as one atomic file replace — see [`append_conjecture_segments`].
fn build_conjecture_verdict_segment(nt_body: &str) -> gmeow_errors::Result<Vec<u8>> {
    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (lineno, raw) in nt_body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = line.strip_suffix(" .").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("conjecture projection line {} missing ' .'", lineno + 1),
            })
        })?;
        let malformed = |what: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("conjecture projection line {} bad {what}", lineno + 1),
            })
        };
        let (subject, rest) = take_nt_term(line).ok_or_else(|| malformed("subject"))?;
        let (predicate, rest) =
            take_nt_term(rest.trim_start()).ok_or_else(|| malformed("predicate"))?;
        let (object, _rest) = take_nt_term(rest.trim_start()).ok_or_else(|| malformed("object"))?;
        // The projection's predicates are always IRIs; reject anything else fail-closed.
        if !matches!(predicate, NtTerm::Iri(_)) {
            return Err(malformed("predicate (non-IRI)"));
        }
        let s = intern_nt_term(&subject, &mut terms, &mut seen);
        let p = intern_nt_term(&predicate, &mut terms, &mut seen);
        let o = intern_nt_term(&object, &mut terms, &mut seen);
        quads.push((s, p, o, None));
    }

    let mut writer = GtsWriter::new("ai-package");
    writer.add_terms(&terms);
    writer.add_quads(&quads);
    Ok(writer.to_bytes())
}

/// The sidecar advisory-lock path for the conjecture library at `library_path`: the library
/// path with a literal `.lock` suffix appended (e.g. `conjectures.gts` → `conjectures.gts.lock`).
/// The lock file's own bytes are never read; it exists solely as a stable `flock`/`LockFileEx`
/// target that survives the library file being replaced out from under it by
/// [`append_conjecture_segments`]'s atomic rename (an `flock` on the DATA file itself would be
/// silently dropped by a rename-replace, since the lock is bound to the inode, not the path).
fn conjecture_lock_path(library_path: &Path) -> PathBuf {
    let mut os = library_path.as_os_str().to_owned();
    os.push(".lock");
    PathBuf::from(os)
}

/// Acquire ONE exclusive, cross-process advisory lock on `library_path`'s sidecar `.lock` file
/// for the duration of `f`, serializing every conjecture-library operation — reads, precondition
/// checks, and appends alike — against every other process/thread doing the same. Callers that
/// must read-then-decide-then-write (e.g. `refute_conjecture`'s "is this id still in the library
/// and not yet withdrawn?" precondition) run the ENTIRE read → check → append sequence inside
/// `f`, so two concurrent callers can no longer both observe the pre-write state and both commit
/// (the lost-update / double-write race CodeRabbit flagged). The lock is released when the guard
/// file handle drops at the end of this call, regardless of whether `f` succeeded.
fn with_conjecture_lock<T>(
    library_path: &Path,
    f: impl FnOnce() -> gmeow_errors::Result<T>,
) -> gmeow_errors::Result<T> {
    if let Some(parent) = library_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let lock_path = conjecture_lock_path(library_path);
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)?;
    // Blocking exclusive lock (`flock(LOCK_EX)` / `LockFileEx` exclusive) — a concurrent
    // holder blocks here rather than racing past a TOCTOU window.
    lock_file.lock()?;
    let result = f();
    let _ = lock_file.unlock();
    result
}

/// Atomically commit `segments` (each an already-serialized GTS `ai-package` segment, in order)
/// to the conjecture library at `path`, ALL-OR-NOTHING: the new file contents — the library's
/// current bytes (if any) followed by every segment in `segments`, in order — are assembled
/// ENTIRELY in memory, then committed via a same-directory temp file + `fsync` + atomic rename.
/// A rename either lands the WHOLE new file or leaves the PRIOR file completely untouched — so
/// if anything fails partway (e.g. the audit segment's bytes can't be built, or the temp write
/// fails), the library is never left holding some but not all of `segments` (closing the "audit
/// append fails, library append is left applied" half of the CodeRabbit gap). The caller MUST
/// already hold the conjecture-library lock (see [`with_conjecture_lock`]) — this function does
/// not lock by itself, so it can be called once per commit even when it writes >1 segment.
fn append_conjecture_segments(path: &Path, segments: &[Vec<u8>]) -> gmeow_errors::Result<()> {
    let mut bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    for segment in segments {
        bytes.extend_from_slice(segment);
    }

    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let mut tmp = tempfile::Builder::new()
        .prefix(".conjectures-")
        .suffix(".tmp")
        .tempfile_in(&dir)?;
    tmp.write_all(&bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

/// A per-segment collector for [`read_conjecture_library`]: it captures each GTS segment's
/// term table and quads IN FILE (append) ORDER, so a `logic:conjectureLifecycleState`
/// supersession can be resolved as last-writer-wins by SEGMENT ORDER.
///
/// Order is the ONLY sound disambiguator here (R3). The unioned dataset a plain
/// `import_gts_events` yields carries EVERY state a node ever held at once — after a store
/// then a refute, one node holds both its engine verdict (`Open`/`Corroborated`/`Refuted…`)
/// AND `ConjectureWithdrawn` — and `gmeow:atTime` cannot break the tie either, because every
/// audit segment stamps the SAME fixed determinism epoch. The streaming reader
/// (`read_to_sink` with `allow_segments = true`) is the one path that preserves per-segment
/// identity: each appended `GtsWriter` blob reads back as its own segment, delivered in
/// append order, so folding the lifecycle assertions in that order makes the LAST one win.
#[derive(Default)]
struct ConjectureSegments {
    /// One row per segment, indexed by segment order.
    segments: Vec<ConjectureSegmentRows>,
    /// The first reader diagnostic, if any — any diagnostic is a HARD read failure (no
    /// silent partial read of a corrupt library).
    diagnostic: Option<String>,
}

/// One segment's captured rows: its segment-local term table and its `(s, p, o)` quads.
#[derive(Default)]
struct ConjectureSegmentRows {
    /// Segment-local term id → interned term (ids are dense from 0 within a segment).
    terms: Vec<Option<GtsTerm>>,
    /// `(subject, predicate, object)` segment-local term ids (the graph slot is dropped —
    /// the conjecture library writes only default-graph triples).
    quads: Vec<(usize, usize, usize)>,
}

impl ConjectureSegments {
    /// The rows for `index`, growing the segment vector so an out-of-order or sparse
    /// segment index still lands in its own slot.
    fn seg(&mut self, index: usize) -> &mut ConjectureSegmentRows {
        if index >= self.segments.len() {
            self.segments
                .resize_with(index + 1, ConjectureSegmentRows::default);
        }
        &mut self.segments[index]
    }
}

impl purrdf::gts::reader::StreamingSink for ConjectureSegments {
    fn term(&mut self, segment_index: usize, term_id: usize, term: &GtsTerm) {
        let rows = self.seg(segment_index);
        if term_id >= rows.terms.len() {
            rows.terms.resize(term_id + 1, None);
        }
        rows.terms[term_id] = Some(term.clone());
    }

    fn quad(&mut self, segment_index: usize, quad: purrdf::gts::model::Quad) {
        let (subject, predicate, object, _graph) = quad;
        self.seg(segment_index)
            .quads
            .push((subject, predicate, object));
    }

    fn diagnostic(&mut self, diagnostic: &purrdf::gts::model::Diagnostic) {
        if self.diagnostic.is_none() {
            self.diagnostic = Some(format!("{}: {}", diagnostic.code, diagnostic.detail));
        }
    }
}

/// Read the append-only conjecture library at `path`, resolving each stored
/// `logic:Conjecture` node's **EFFECTIVE** `logic:conjectureLifecycleState` by SEGMENT
/// ORDER (last writer wins — see [`ConjectureSegments`] for why order, not the union or
/// `gmeow:atTime`, is the only sound key). A node typed `logic:Conjecture` in any segment is
/// in the library; its effective state is the object of the LAST lifecycle assertion for it
/// in append order. A missing library file is an EMPTY library (a first-ever refute of an
/// unknown id), not an error; any reader diagnostic or a torn trailing item is a HARD FAIL.
fn read_conjecture_library(
    path: &Path,
) -> gmeow_errors::Result<BTreeMap<String, ConjectureLifecycleState>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e.into()),
    };

    let mut sink = ConjectureSegments::default();
    let result = purrdf::gts::reader::read_to_sink(&bytes, true, None, &mut sink);
    if let Some(diag) = sink.diagnostic.take() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture library read diagnostic: {diag}"),
        }));
    }
    if let Some(first) = result.diagnostics.first() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "conjecture library read diagnostic: {}: {}",
                first.code, first.detail
            ),
        }));
    }
    if let Some(offset) = result.torn {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture library has a torn trailing item at byte {offset}"),
        }));
    }

    let conjecture_iri = format!("{LOGIC_NAMESPACE}Conjecture");
    let lifecycle_iri = format!("{LOGIC_NAMESPACE}conjectureLifecycleState");

    let mut is_conjecture: BTreeSet<String> = BTreeSet::new();
    let mut effective: BTreeMap<String, ConjectureLifecycleState> = BTreeMap::new();

    // Fold the segments in FILE ORDER — a later segment's lifecycle assertion for a node
    // supersedes an earlier one, so `insert` (last-writer-wins) IS the supersession rule.
    for seg in &sink.segments {
        let resolve_iri = |id: usize| -> Option<&str> {
            seg.terms
                .get(id)?
                .as_ref()
                .filter(|term| term.kind == GtsTermKind::Iri)
                .and_then(|term| term.value.as_deref())
        };
        for &(subject, predicate, object) in &seg.quads {
            let (Some(subj), Some(pred)) = (resolve_iri(subject), resolve_iri(predicate)) else {
                continue;
            };
            if pred == RDF_TYPE {
                if resolve_iri(object) == Some(conjecture_iri.as_str()) {
                    is_conjecture.insert(subj.to_owned());
                }
            } else if pred == lifecycle_iri
                && let Some(obj) = resolve_iri(object)
                && let Some(local) = obj.strip_prefix(LOGIC_NAMESPACE)
                && let Some(state) = ConjectureLifecycleState::from_local(local)
            {
                effective.insert(subj.to_owned(), state);
            }
        }
    }

    // A node is in the library iff it was typed `logic:Conjecture`; every such node carries a
    // lifecycle state (REQUIRED by logic:ConjectureShape), so the intersection is exactly the
    // library's conjecture set paired with its effective, segment-order-resolved state.
    Ok(effective
        .into_iter()
        .filter(|(node, _)| is_conjecture.contains(node))
        .collect())
}

/// Parse the candidate `logic:` document and extract exactly ONE candidate [`Formula`]: the
/// single top-level `logic:Formula` when present, else the single ground `logic:` axiom
/// lifted to a binary [`Formula::Atom`]. Any other shape (zero / multiple candidates) is a
/// hard, fail-closed error — a conjecture test names one formula, never a program.
fn parse_candidate_formula(formula_src: &str) -> gmeow_errors::Result<Formula> {
    let (program, _diags) = parse_logic_str(formula_src, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("candidate logic: document failed to parse: {e}"),
        })
    })?;
    let bad = |message: String| gmeow_errors::Diag::of_kind(crate::error::Mcp { message });
    // A reified `logic:Formula` is the primary surface: prefer the single top-level formula
    // even when the frontend leaks the formula's own structural triples as axioms.
    let formula_count = program.formulas.len();
    if formula_count == 1 {
        return Ok(program.formulas.into_iter().next().expect("len == 1"));
    }

    // No single top-level formula. A REIFIED trivially-Horn `logic:Formula` (a ground binary
    // atom — e.g. `relation=rdf:type, arg0=ex:a, arg1=ex:B`, the fact `ex:a rdf:type ex:B`) is
    // routed by the front-end to `LogicProgram.axioms` (its Horn home — `with_formulas`
    // hard-fails on a trivially-Horn leaf), so `formulas` is EMPTY and the candidate lives in
    // the axiom set. That set also carries the formula node's own `rdf:type logic:Formula`
    // self-typing, which leaks as a structural axiom — drop that noise, then a lone remaining
    // axiom IS the candidate fact, lifted here to a binary `Formula::Atom`. `conjecture_test`'s
    // `as_ground_fact` then asserts a ground lift as an EDB fact and decides it like any
    // candidate, so a reified ground-atom conjecture is a first-class, evaluated candidate —
    // never a panic (the previously dead `(0,1)` lift is now genuinely reachable).
    let logic_formula_iri = format!("{LOGIC_NAMESPACE}Formula");
    let mut candidate_axioms: Vec<_> = program
        .axioms
        .into_iter()
        .filter(|ax| !(ax.predicate == RDF_TYPE && ax.obj == logic_formula_iri))
        .collect();
    if formula_count == 0 && candidate_axioms.len() == 1 {
        let ax = candidate_axioms.pop().expect("len == 1");
        let object = if ax.obj_is_literal {
            IrTerm::Literal {
                lexical: ax.obj,
                datatype: None,
            }
        } else {
            IrTerm::Iri(ax.obj)
        };
        return Formula::atom(
            IrTerm::Iri(ax.predicate),
            vec![IrTerm::Iri(ax.subject), object],
        )
        .map_err(|e| bad(e.message().to_owned()));
    }
    Err(bad(format!(
        "candidate must be exactly one formula/atom, got {formula_count} formula(s) and \
         {} candidate axiom(s)",
        candidate_axioms.len()
    )))
}

/// Re-home every triple of the caller's KB Turtle into [`CONJECTURE_SCENARIO_WORLD`] as a
/// fresh, frozen [`purrdf::RdfDataset`]. World-homing is required because the DL consistency
/// calculus is world-scoped: KB facts must sit in the SAME world the candidate is asserted /
/// evaluated in for a disjointness clash to fire.
fn rehome_kb_into_scenario(
    kb_src: &str,
) -> gmeow_errors::Result<std::sync::Arc<purrdf::RdfDataset>> {
    let parsed = purrdf::parse_dataset(kb_src.as_bytes(), "text/turtle", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("KB Turtle failed to parse: {e}"),
        })
    })?;
    let world = RdfTerm::iri(CONJECTURE_SCENARIO_WORLD);
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        let rehomed =
            RdfQuad::new(quad.subject, quad.predicate, quad.object).in_graph(world.clone());
        builder.push_owned_quad(&rehomed);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("re-homed KB dataset failed to freeze: {e}"),
        })
    })
}

fn tool_arguments(args: &Value, keys: &[&str]) -> String {
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(value) = args.get(*key)
            && !value.is_null()
        {
            out.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(out).to_string()
}

fn claim_json(claim: &purrdf::gts::examples::agent_memory::Claim) -> Value {
    json!({
        "id": claim.id,
        "text": claim.text,
        "confidence": claim.confidence,
        "according_to": claim.according_to,
        "source": claim.source,
        "created": claim.created,
        "suppressed": claim.suppressed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};

    /// Append one hand-built conjecture-verdict segment to the append-only library at `path`,
    /// as a GTS `ai-package` segment, via the SAME locked/atomic commit path production code
    /// uses ([`with_conjecture_lock`] + [`append_conjecture_segments`]). Test-only: it seeds a
    /// library file with a single segment so segment-order-resolution tests
    /// ([`read_conjecture_library`]) can be driven without going through a full `store_conjecture`
    /// engine run. Production call sites build BOTH the verdict segment and its audit segment
    /// and commit them together via [`append_conjecture_segments`] directly (one atomic replace
    /// covering both), rather than through this single-segment helper.
    fn write_conjecture_segment(path: &Path, nt_body: &str) -> gmeow_errors::Result<()> {
        let segment = build_conjecture_verdict_segment(nt_body)?;
        with_conjecture_lock(path, || append_conjecture_segments(path, &[segment]))
    }

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvRestore(Vec<(&'static str, Option<OsString>)>);

    impl EnvRestore {
        fn capture(keys: &[&'static str]) -> Self {
            Self(keys.iter().map(|key| (*key, env::var_os(key))).collect())
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                // SAFETY: single-threaded test env mutation under the test's env lock.
                unsafe {
                    match value {
                        Some(value) => env::set_var(key, value),
                        None => env::remove_var(key),
                    }
                }
            }
        }
    }

    struct CwdRestore(PathBuf);

    impl CwdRestore {
        fn capture() -> Self {
            Self(env::current_dir().expect("current dir"))
        }
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            env::set_current_dir(&self.0).expect("restore current dir");
        }
    }

    fn snapshot() -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read(root.join("generated/dist/gmeow.gts")).expect("read committed snapshot")
    }

    fn text_payload(value: Value) -> Value {
        let text = value["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool text is JSON")
    }

    fn temp_memory() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", &path);
        }
        (dir, path)
    }

    #[test]
    fn consumer_view_retains_raw_snapshot_and_reaches_shapes_archive() {
        // The native validation surface (`validate_local`) needs the raw GTS bytes
        // back so `gmeow_validate` can read the folded `shapes-archive` blob — the
        // parsed carrier dataset does not carry it. Prove the bytes are retained
        // verbatim and that the shapes archive is reachable from them, so the
        // consumer server (the shippable `gmeow mcp`) can validate agent data.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        assert_eq!(
            server.view.gts_bytes(),
            bytes.as_slice(),
            "the view must retain the snapshot bytes verbatim",
        );
        let shapes = crate::bundle_blobs::Bundle::from_snapshot(server.view.gts_bytes())
            .expect("bundle parses from retained bytes")
            .shapes()
            .expect("shapes-archive readable from retained bytes");
        assert!(
            !shapes.is_empty(),
            "the shapes-archive blob must be reachable from the retained snapshot \
             bytes — it is the SHACL surface validate_local checks agent data against",
        );
    }

    #[test]
    fn modes_advertise_consumer_and_dev_surfaces() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let consumer_tools = consumer.tools_result().to_string();
        assert!(consumer_tools.contains("\"lookup_term\""));
        assert!(consumer_tools.contains("\"llms_txt\""));
        assert!(consumer_tools.contains("\"llms_full\""));
        assert!(consumer_tools.contains("\"okf_index\""));
        assert!(consumer_tools.contains("\"query_docs\""));
        assert!(consumer_tools.contains("\"store_claim\""));
        // The AI-agent docs surface: all five new tools are CONSUMER-visible
        // (served by the shippable `gmeow mcp` off the bundle alone), never
        // dev-gated. `validate_local` is distinct from the dev-only `validate`.
        assert!(consumer_tools.contains("\"validate_local\""));
        assert!(consumer_tools.contains("\"docs_search\""));
        assert!(consumer_tools.contains("\"counter_examples\""));
        assert!(consumer_tools.contains("\"entailments\""));
        assert!(consumer_tools.contains("\"competency_questions\""));
        assert!(!consumer_tools.contains("\"validate\""));
        assert!(!consumer_tools.contains("\"slice_quality\""));
        assert!(
            !consumer
                .resources_result()
                .to_string()
                .contains("constitution")
        );

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dev = McpServer::from_snapshot(&bytes, Some(root), McpMode::Dev).unwrap();
        let dev_tools = dev.tools_result().to_string();
        assert!(dev_tools.contains("\"validate\""));
        assert!(dev_tools.contains("\"reason\""));
        assert!(dev_tools.contains("\"regenerate\""));
        assert!(dev_tools.contains("\"constitution\""));
        assert!(dev_tools.contains("\"slice_quality\""));
        assert!(dev.resources_result().to_string().contains("constitution"));
    }

    #[test]
    fn slice_quality_tool_reports_grades_and_advice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dev = McpServer::from_snapshot(&bytes, Some(root), McpMode::Dev).unwrap();

        // Functional dispatch: the tool returns the documented JSON shape — grades as
        // {axis, tier, score} and advice as {code, message}.
        let out = text_payload(dev.call_tool_result(
            "slice_quality",
            &json!({"path": "slices/core/slice-quality-rubric"}),
        ));
        assert!(
            out.get("ok").is_none(),
            "a successful score carries no error envelope: {out}"
        );
        assert!(out["slice"].is_string(), "slice IRI present: {out}");
        assert!(
            out["rollup_tier"].is_string(),
            "roll-up tier present: {out}"
        );
        let grades = out["grades"].as_array().expect("grades array");
        assert!(!grades.is_empty(), "at least one axis grade: {out}");
        for g in grades {
            assert!(g["axis"].is_string(), "grade.axis is a string: {g}");
            assert!(g["tier"].is_string(), "grade.tier is a string: {g}");
            assert!(g["score"].is_number(), "grade.score is a number: {g}");
        }
        for a in out["advice"].as_array().expect("advice array") {
            assert!(a["code"].is_string(), "advice.code is a string: {a}");
            assert!(a["message"].is_string(), "advice.message is a string: {a}");
        }
    }

    #[test]
    fn slice_quality_tool_rejects_path_traversal() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dev = McpServer::from_snapshot(&bytes, Some(root), McpMode::Dev).unwrap();

        // An absolute path escapes the slices/ tree and must be rejected, not scored.
        let abs = text_payload(dev.call_tool_result("slice_quality", &json!({"path": "/etc"})));
        assert_eq!(abs["ok"], false, "absolute path must be rejected: {abs}");

        // A `../` sequence that climbs out of slices/ is rejected too.
        let up = text_payload(dev.call_tool_result(
            "slice_quality",
            &json!({"path": "slices/../../../etc/passwd"}),
        ));
        assert_eq!(up["ok"], false, "../ traversal must be rejected: {up}");
    }

    #[test]
    fn query_docs_selects_over_the_documentation_graph() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // A SELECT over the bundled documentation graph returns SPARQL-1.1 JSON
        // bindings (the doc graph carries a gmeow:DocumentedTerm per documented term).
        let select = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "SELECT ?s WHERE { ?s a <https://blackcatinformatics.ca/gmeow/DocumentedTerm> } LIMIT 3"}),
        ));
        assert_eq!(
            select["ok"], true,
            "query_docs SELECT must succeed: {select}"
        );
        assert_eq!(select["head"]["vars"][0], "s");
        assert!(
            select["results"]["bindings"]
                .as_array()
                .map(|b| !b.is_empty())
                .unwrap_or(false),
            "expected at least one DocumentedTerm binding: {select}"
        );

        // CONSTRUCT is rejected — the tool serves only bindings or a boolean.
        let construct = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], false);
        assert!(
            construct["error"]
                .as_str()
                .unwrap_or_default()
                .contains("SELECT and ASK")
        );
    }

    #[test]
    fn memory_triad_preserves_suppression_on_every_default_recall_path() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let canary = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "SUPPRESSED-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let canary_id = canary["claim"]["id"].as_str().unwrap().to_string();
        text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "CONTROL-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": canary_id, "reason": "revised"}),
        ));
        assert_eq!(revised["ok"], true);

        text_payload(server.call_tool_result("recall", &json!({"query": "launch window"})));
        let calls = Memory::new(memory_path).tool_calls().unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|call| call.tool.as_str())
                .collect::<Vec<_>>(),
            vec![
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:revise_belief"
            ]
        );
        assert_eq!(calls[0].generated, vec![canary_id.clone()]);
        let stored_result: Value =
            serde_json::from_str(calls[0].result.as_deref().unwrap()).unwrap();
        assert_eq!(stored_result["ok"], true);
        assert_eq!(stored_result["claim"]["id"], canary_id);
        let stored_arguments: Value =
            serde_json::from_str(calls[0].arguments.as_deref().unwrap()).unwrap();
        assert_eq!(
            stored_arguments["text"],
            "SUPPRESSED-CANARY belief about the launch window"
        );

        for args in [
            json!({}),
            json!({"query": "launch window"}),
            json!({"query": "SUPPRESSED-CANARY belief"}),
            json!({"query": "launch", "min_confidence": 0.5}),
            json!({"query": "", "limit": 100}),
        ] {
            let recalled = text_payload(server.call_tool_result("recall", &args));
            let texts: Vec<&str> = recalled["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["text"].as_str().unwrap())
                .collect();
            assert!(!texts.contains(&"SUPPRESSED-CANARY belief about the launch window"));
            assert!(texts.contains(&"CONTROL-CANARY belief about the launch window"));
        }

        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "launch window", "include_suppressed": true}),
        ));
        assert!(audit["claims"].as_array().unwrap().iter().any(|claim| {
            claim["text"] == "SUPPRESSED-CANARY belief about the launch window"
                && claim["suppressed"] == true
        }));
    }

    #[test]
    fn revision_rejects_unknown_ids_before_writing() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let claim =
            text_payload(server.call_tool_result("store_claim", &json!({"text": "a real belief"})));
        let claim_id = claim["claim"]["id"].as_str().unwrap();

        let missing = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": "urn:gmeow:assertion:no-such-id"}),
        ));
        assert_eq!(missing["ok"], false);

        let bad_successor = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "superseded_by": "urn:gmeow:assertion:ghost"}),
        ));
        assert_eq!(bad_successor["ok"], false);

        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "real belief"})));
        assert_eq!(live["claims"][0]["id"], claim_id);
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    #[test]
    fn canonical_action_policy_is_the_single_authority_and_parses() {
        // The embedded slice file is the one source of truth for the action theory.
        let policy = action_policy_nquads();
        assert!(!policy.is_empty());
        assert!(policy.contains(MCP_STORE_CLAIM_SCHEMA));
        assert!(policy.contains(MCP_REVISE_BELIEF_SCHEMA));
        assert!(policy.contains(TXN_WORLD));
    }

    /// T3 (issue 1312 dogfood): the proof-carrying diagnostics tool surface
    /// (`explain_quad`, `verify_graph`, `coherence_certificate`, `store_conjecture` /
    /// `refute_conjecture`) must be REPRESENTED in the canonical action theory the engine
    /// actually parses, not merely documented. This asserts against the projected N-Quads
    /// `action_policy_nquads()` returns — the SAME IRI→IRI-only quads
    /// [`txn_world_nquads`] feeds the executional-entailment engine — so a schema that
    /// only exists as a dropped label/comment would fail here.
    #[test]
    fn action_policy_covers_the_proof_carrying_read_and_conjecture_write_tools() {
        const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/";
        const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const LOGIC_ACTION_SCHEMA: &str = "https://blackcatinformatics.ca/logic/ActionSchema";
        const LOGIC_MCP_ACTION_SCHEMA: &str =
            "https://blackcatinformatics.ca/logic/McpActionSchema";
        const LOGIC_CAPABILITY: &str = "https://blackcatinformatics.ca/logic/capability";
        const LOGIC_COMPENSATION: &str = "https://blackcatinformatics.ca/logic/compensation";
        const LOGIC_EFFECT: &str = "https://blackcatinformatics.ca/logic/effect";

        let policy = action_policy_nquads();

        // The three new READ tools: each a plain logic:ActionSchema (mirroring ex:recall)
        // typed + capability-gated, and each carrying NO logic:compensation / logic:effect
        // (a read changes no state).
        for local in ["explainQuad", "verifyGraph", "coherenceCertificate"] {
            let subject = format!("{EX}{local}");
            let type_line =
                format!("<{subject}> <{RDF_TYPE}> <{LOGIC_ACTION_SCHEMA}> <{TXN_WORLD}> .");
            let capability_line = format!(
                "<{subject}> <{LOGIC_CAPABILITY}> <{EX}memoryReadCapability> <{TXN_WORLD}> ."
            );
            assert!(
                policy.contains(&type_line),
                "{local} must be typed logic:ActionSchema: missing {type_line:?} in:\n{policy}"
            );
            assert!(
                policy.contains(&capability_line),
                "{local} must carry logic:capability ex:memoryReadCapability: missing \
                 {capability_line:?}"
            );
            assert!(
                !policy.contains(&format!("<{subject}> <{LOGIC_COMPENSATION}>")),
                "{local} is a read and must carry NO logic:compensation"
            );
            assert!(
                !policy.contains(&format!("<{subject}> <{LOGIC_EFFECT}>")),
                "{local} is a read and must carry NO logic:effect"
            );
        }

        // The store_conjecture / refute_conjecture pair: ALREADY modeled by
        // ex:persistConjecture / ex:withdrawConjecture (no duplicate schema minted for the
        // MCP tool names) — confirm the store⇄refute compensation pairing survives the
        // projection filter.
        let persist_type = format!(
            "<{EX}persistConjecture> <{RDF_TYPE}> <{LOGIC_MCP_ACTION_SCHEMA}> <{TXN_WORLD}> ."
        );
        let persist_compensation = format!(
            "<{EX}persistConjecture> <{LOGIC_COMPENSATION}> <{EX}withdrawConjecture> <{TXN_WORLD}> ."
        );
        let withdraw_type = format!(
            "<{EX}withdrawConjecture> <{RDF_TYPE}> <{LOGIC_MCP_ACTION_SCHEMA}> <{TXN_WORLD}> ."
        );
        let withdraw_compensation = format!(
            "<{EX}withdrawConjecture> <{LOGIC_COMPENSATION}> <{EX}persistConjecture> <{TXN_WORLD}> ."
        );
        assert!(
            policy.contains(&persist_type),
            "persistConjecture (store_conjecture) must be typed logic:McpActionSchema"
        );
        assert!(
            policy.contains(&persist_compensation),
            "persistConjecture's compensation must be withdrawConjecture"
        );
        assert!(
            policy.contains(&withdraw_type),
            "withdrawConjecture (refute_conjecture) must be typed logic:McpActionSchema"
        );
        assert!(
            policy.contains(&withdraw_compensation),
            "withdrawConjecture's compensation must be persistConjecture"
        );
    }

    #[test]
    fn dry_run_must_be_a_boolean() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let bad = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "x", "dry_run": "yes"})),
        );
        assert_eq!(bad["ok"], false);
        assert!(
            bad["error"]
                .as_str()
                .unwrap()
                .contains("dry_run must be a boolean")
        );
    }

    #[test]
    fn store_claim_dry_run_computes_verdict_without_persisting() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let dry = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "a dry-run belief about orbits", "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);
        assert!(
            dry["transaction"]["witness"].as_str().is_some(),
            "a sandbox run leaves a content-addressed witness"
        );
        assert!(dry.get("claim").is_none(), "dry run writes no claim");

        // Nothing persisted: recall is empty and the memory holds no claims or tool calls.
        let recalled =
            text_payload(server.call_tool_result("recall", &json!({"query": "dry-run belief"})));
        assert!(recalled["claims"].as_array().unwrap().is_empty());
        let memory = Memory::new(&memory_path);
        assert!(memory.claims().unwrap().is_empty());
        assert!(memory.tool_calls().unwrap().is_empty());
    }

    #[test]
    fn committed_store_records_the_audit_context() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let stored = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "an audited belief about thrust", "confidence": 0.8}),
        ));
        assert_eq!(stored["ok"], true);
        assert_eq!(stored["transaction"]["committed"], true);
        assert_eq!(stored["transaction"]["succeeded"], true);

        // The committed turn is cold-auditable: the persisted memory.gts carries exactly the
        // predicates emit_trajectory_audits requires on the recorded ToolCall and its anchor.
        let raw = fs::read(&memory_path).unwrap();
        let bundle = purrdf::import_gts_events(&raw).expect("import memory.gts");
        let predicates: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .map(|quad| quad.predicate.clone())
            .collect();
        for predicate in [
            LOGIC_INSTANTIATES_SCHEMA,
            LOGIC_PROPER_PART_OF,
            GMEOW_AT_TIME,
            GMEOW_EVENT_TEMPORAL_FRAME,
            LOGIC_TRANSITION_FROM_STATE,
            LOGIC_SITUATION_OBTAINS,
        ] {
            assert!(
                predicates.contains(predicate),
                "memory.gts must carry {predicate} for the trajectory audit"
            );
        }
        // The single canonical temporal frame is recorded (P11 — one frame per trajectory).
        let frames: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .filter(|quad| quad.predicate == GMEOW_EVENT_TEMPORAL_FRAME)
            .map(|quad| quad.object.to_string())
            .collect();
        assert!(
            frames
                .iter()
                .all(|frame| frame.contains("temporalFrameUTCGregorian"))
        );
    }

    #[test]
    fn revise_belief_dry_run_does_not_suppress() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a revisable belief"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let dry = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);

        // The claim is still live — a sandbox revise suppresses nothing (P10 for free).
        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "revisable belief"})));
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    #[test]
    fn committed_revise_suppresses_but_never_deletes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a belief to retire"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "reason": "superseded"}),
        ));
        assert_eq!(revised["ok"], true);
        assert_eq!(revised["transaction"]["committed"], true);

        // Default recall hides it (suppressed) ...
        let default =
            text_payload(server.call_tool_result("recall", &json!({"query": "belief retire"})));
        assert!(default["claims"].as_array().unwrap().is_empty());
        // ... but it is still present (supersession, never erasure — P10).
        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "belief retire", "include_suppressed": true}),
        ));
        assert!(
            audit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .any(|claim| claim["id"] == claim_id.as_str() && claim["suppressed"] == true)
        );
    }

    #[test]
    fn startup_language_is_validated_and_json_rpc_dispatches() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let bytes = snapshot();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "notatag");
        }
        let err = match McpServer::from_snapshot(&bytes, None, McpMode::Consumer) {
            Ok(_) => panic!("invalid startup language must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("unknown language tag 'notatag'"));

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "fr");
        }
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let init: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        )
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");

        let tools: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#),
        )
        .unwrap();
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "lookup_term")
        );

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "X-GMEOW-FRENCH");
        }
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let fr = text_payload(
            server.call_tool_result("lookup_term", &json!({"term": "gmeow:EntityExistence"})),
        );
        assert_eq!(fr["label"], "Existence d'entit\u{e9}");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
    }

    /// A bare local name that collides across grounding namespaces HARD-FAILS on
    /// EVERY consumer surface — the MCP twin of `gmeow describe`'s typed ambiguity —
    /// never a silent first-exact pick (`.goals` NO OPTIONALITY). `Conjecture` names
    /// both `logic:Conjecture` and `math:Conjecture` in the shipped bundle.
    #[test]
    fn ambiguous_bare_name_hard_fails_across_mcp_surfaces() {
        let server = consumer_server();

        // `lookup_term`: the DISTINCT ambiguity envelope (ok:false) listing BOTH
        // sorted candidate CURIEs — not a silent pick of the first exact match.
        let looked =
            text_payload(server.call_tool_result("lookup_term", &json!({"term": "Conjecture"})));
        assert_eq!(looked["ok"], json!(false));
        let err = looked["error"].as_str().expect("ambiguity error string");
        assert_eq!(
            err, "ambiguous term 'Conjecture': logic:Conjecture, math:Conjecture",
            "lookup_term must emit the sorted-candidate ambiguity envelope"
        );

        // `doc_card`: hard fail (isError) with the same ambiguity — not a card.
        let card = server.call_tool_result("doc_card", &json!({"term": "Conjecture"}));
        assert_eq!(card["isError"], json!(true));
        let card_err = text_payload(card);
        assert_eq!(card_err["ok"], json!(false));
        let ce_msg = card_err["error"].as_str().expect("doc_card error");
        assert!(
            ce_msg.contains("logic:Conjecture") && ce_msg.contains("math:Conjecture"),
            "doc_card ambiguity must list sorted candidates, got {ce_msg:?}"
        );

        // The resolution guard feeding `counter_examples` / `entailments` /
        // `competency_questions` hard-fails too (isError) — never a silent IRI.
        let guarded = server.call_tool_result("counter_examples", &json!({"term": "Conjecture"}));
        assert_eq!(guarded["isError"], json!(true));

        // The ambiguity carries its OWN typed code, DISTINCT from the generic
        // unknown-term `pipeline.mcp` — greppable as `pipeline.mcp.ambiguous-term`.
        let requested = server.startup_requested.clone();
        let ConsumerResolution::Ambiguous { candidates } =
            server.view.resolve_term_iri("Conjecture", requested)
        else {
            panic!("`Conjecture` must resolve ambiguously across logic:/math:");
        };
        assert_eq!(
            candidates,
            vec![
                "logic:Conjecture".to_string(),
                "math:Conjecture".to_string()
            ],
            "candidates sorted + deduped"
        );
        let diag = ambiguous_term_err("Conjecture", &candidates);
        assert_eq!(diag.code(), crate::error::McpAmbiguousTerm::register());

        // Regression: unambiguous queries still RESOLVE on the same surface — the
        // ambiguity gate fires ONLY on genuine cross-namespace collisions.
        for (q, curie) in [
            ("lang:Denotation", "lang:Denotation"),
            ("math:Function", "math:Function"),
            ("Denotation", "lang:Denotation"),
        ] {
            let hit = text_payload(server.call_tool_result("lookup_term", &json!({"term": q})));
            assert_eq!(hit["ok"], json!(true), "`{q}` must still resolve");
            assert_eq!(hit["curie"], json!(curie), "`{q}` resolves to {curie}");
        }
    }

    #[test]
    fn default_memory_path_lives_under_home() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("HOME", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(server.call_tool_result("store_claim", &json!({"text": "durable belief"})));
        assert!(dir.path().join(".gmeow/memory.gts").exists());
    }

    #[test]
    fn memory_path_handles_userprofile_and_relative_files() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let _cwd = CwdRestore::capture();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
            env::remove_var("HOME");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("USERPROFILE", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "profile fallback belief"})),
        );
        assert!(dir.path().join(".gmeow/memory.gts").exists());

        let relative_dir = tempfile::tempdir().expect("relative tempdir");
        env::set_current_dir(relative_dir.path()).expect("set current dir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", "memory.gts");
        }
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "relative path belief"})),
        );
        assert!(relative_dir.path().join("memory.gts").exists());
    }

    /// The read-only local-ontology overlay: reads see `bundle ∪ overlay`, the
    /// overlay is provenance-isolated under the external graph, and nothing is
    /// written back — not the overlay file, not the canon, not memory.
    #[test]
    fn local_overlay_is_a_read_only_external_annex() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // A local lower-tier vocab file the agent supplies (not part of the canon).
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("local-vocab.ttl");
        let overlay_ttl = "<urn:ex:widget> <urn:ex:label> \"Local Widget\" .\n<urn:ex:widget> a <urn:ex:Thing> .\n";
        fs::write(&overlay_path, overlay_ttl).unwrap();
        let overlay_before = fs::read(&overlay_path).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        // Reads see the overlay unioned into the default graph.
        let seen = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "path": path_str,
                "query": "SELECT ?o WHERE { <urn:ex:widget> <urn:ex:label> ?o }",
            }),
        ));
        assert_eq!(seen["ok"], true, "overlay query must succeed: {seen}");
        assert_eq!(seen["results"]["bindings"][0]["o"]["value"], "Local Widget");

        // Reads ALSO see the bundle canon in the same active graph (union, not
        // replacement): a plain triple pattern still matches the signed ontology.
        let canon = text_payload(server.call_tool_result(
            "query_local",
            &json!({"path": path_str, "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(canon["ok"], true);
        assert_eq!(canon["boolean"], true);

        // The overlay is provenance-isolable under the distinct external graph — its
        // triples never bear a signed gmeow: graph name.
        let isolated = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "path": path_str,
                "query": "SELECT ?o WHERE { GRAPH <urn:gmeow:mcp:overlay:external> \
                          { <urn:ex:widget> <urn:ex:label> ?o } }",
            }),
        ));
        assert_eq!(
            isolated["ok"], true,
            "external-graph query must succeed: {isolated}"
        );
        assert_eq!(
            isolated["results"]["bindings"][0]["o"]["value"],
            "Local Widget"
        );

        // CONSTRUCT/DESCRIBE are rejected — the tool serves bindings or a boolean.
        let construct = text_payload(server.call_tool_result(
            "query_local",
            &json!({"path": path_str, "query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], false);
        assert!(
            construct["error"]
                .as_str()
                .unwrap_or_default()
                .contains("SELECT and ASK")
        );

        // Read-only: the overlay file is byte-for-byte unchanged and NOTHING was
        // written to memory (the write triad never touches the overlay or canon).
        assert_eq!(fs::read(&overlay_path).unwrap(), overlay_before);
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());
        drop(mem_dir);
        drop(overlay_dir);
    }

    /// verify_graph fires the matching `verify.<stem>` finding on a known bad-example
    /// overlay: a doxastic state whose asserted `gmeow:credence` is out of `[0,1]` — the
    /// exact violation `queries/verify/credence-out-of-range.rq` is a negative test for.
    /// The overlay joins the reasoning default world and the flat verify query matches it,
    /// so the response `findings` carries `verify.credence-out-of-range`.
    #[test]
    fn verify_graph_fires_on_a_bad_example_overlay_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("bad-credence.ttl");
        // A credence out of [0,1] is the credence-out-of-range bad example. `5` is
        // numeric and > 1, so the negative test returns an offending row.
        let overlay_ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <urn:ex:bad-credence-state> gmeow:credence 5 .\n";
        fs::write(&overlay_path, overlay_ttl).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        // A tiny step budget: the credence negative test fires from the ASSERTED union
        // graph regardless of closure depth, so the budget keeps the test fast.
        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 8})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let codes: Vec<String> = out["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .map(|f| f["code"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            codes.iter().any(|c| c == "verify.credence-out-of-range"),
            "the credence bad example must fire verify.credence-out-of-range: {out}"
        );
        assert!(
            out["error_count"].as_u64().unwrap_or(0) >= 1,
            "the violation must count as an error finding: {out}"
        );
        drop(overlay_dir);
    }

    /// Proof faithfulness: `cited_iris` must be derived ONLY from structured RDF
    /// binding terms, never scraped from rendered finding prose — an
    /// agent-controlled overlay literal carrying angle-bracket text must not be
    /// accepted as a genuine citation. The overlay's `gmeow:credence` value is the
    /// STRING literal `"see <urn:fake>"` (non-numeric, so `credence-out-of-range`
    /// fires) attached to the real subject `<urn:ex:forge-cited-iris-state>`. A
    /// text-scrape over the rendered `detail` (`credence="see <urn:fake>",
    /// state=<urn:ex:...>`) would forge `urn:fake` into `cited_iris`; the
    /// structured-term derivation must not, while the genuinely-cited subject IRI
    /// must still appear.
    #[test]
    fn verify_graph_cited_iris_excludes_forged_literal_text_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("forge-cited-iris.ttl");
        // `!isNumeric("see <urn:fake>")` is true, so this is a credence-out-of-range
        // bad example exactly like the sibling test above — but the credence VALUE
        // is a string literal carrying forged-looking angle-bracket text.
        let overlay_ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <urn:ex:forge-cited-iris-state> gmeow:credence \"see <urn:fake>\" .\n";
        fs::write(&overlay_path, overlay_ttl).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 8})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let codes: Vec<String> = out["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .map(|f| f["code"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            codes.iter().any(|c| c == "verify.credence-out-of-range"),
            "the forged-literal overlay must fire verify.credence-out-of-range: {out}"
        );
        let cited: Vec<String> = out["cited_iris"]
            .as_array()
            .expect("cited_iris array")
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            !cited.iter().any(|c| c.contains("urn:fake")),
            "a literal's angle-bracket text must never forge a citation: {out}"
        );
        assert!(
            cited.iter().any(|c| c == "urn:ex:forge-cited-iris-state"),
            "the finding's genuinely-cited subject IRI must still appear: {out}"
        );
        drop(overlay_dir);
    }

    /// Overlay isolation: verify_graph builds a TRANSIENT union and drops it — the signed
    /// canon `McpView::dataset` is never mutated. After the call the canon Arc is the SAME
    /// allocation with the SAME quad count, and the overlay file is byte-unchanged (the
    /// external annex is read-only and never merged into or written back from the canon).
    #[test]
    fn verify_graph_leaves_the_canon_dataset_untouched_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        let overlay_ttl = "<urn:ex:probe-s> <urn:ex:probe-p> <urn:ex:probe-o> .\n";
        fs::write(&overlay_path, overlay_ttl).unwrap();
        let overlay_before = fs::read(&overlay_path).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let before_ptr = Arc::as_ptr(&server.view.dataset);
        let before_count = server.view.dataset.quad_count();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 4})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

        // Canon Arc identity + quad count are unchanged — the union was transient.
        assert!(
            std::ptr::eq(Arc::as_ptr(&server.view.dataset), before_ptr),
            "the signed canon Arc must not be swapped by verify_graph"
        );
        assert_eq!(
            server.view.dataset.quad_count(),
            before_count,
            "the signed canon quad count must be unchanged (overlay never merged)"
        );
        // The overlay probe triple never leaks into the canon graphs.
        let leaked = server.view.dataset.quads().any(|q| {
            matches!(
                server.view.dataset.resolve(q.s),
                purrdf::prelude::TermRef::Iri(iri) if iri == "urn:ex:probe-s"
            )
        });
        assert!(
            !leaked,
            "overlay triple must not appear in the signed canon"
        );
        // Read-only: the overlay file is byte-for-byte unchanged.
        assert_eq!(fs::read(&overlay_path).unwrap(), overlay_before);
        drop(overlay_dir);
    }

    /// A budget-cut closure yields the strictly-weaker ATTESTATION, never a certificate:
    /// a `max_steps` of 1 cuts the forward chase over the large bundle union mid-flight,
    /// so `reason_all_budgeted` returns a non-conclusive BudgetExhausted / Incomplete
    /// verdict. The completeness gate then MUST render `CoherenceCheckAttestation` with a
    /// non-conclusive completeness axis — a certificate is impossible on an incomplete search.
    #[test]
    fn verify_graph_budget_cut_yields_attestation_never_certificate_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 1})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["class_local_name"], "CoherenceCheckAttestation",
            "a budget-cut closure must attest, never certify: {out}"
        );
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "a budget-cut closure must NEVER render a certificate: {out}"
        );
        // The completeness axis is non-conclusive (the mid-chase cut → Incomplete), and the
        // computation axis records the budget exhaustion.
        assert_eq!(out["completeness"], "incomplete", "{out}");
        assert_eq!(out["evaluation"], "budget-exhausted", "{out}");
        drop(overlay_dir);
    }

    /// R4: OMITTING `max_steps`/`max_answers` entirely must NEVER run an
    /// unbounded Turing-complete chase — `governed_budget` stamps the finite
    /// `DEFAULT_MAX_STEPS` server-side ceiling on every agent-facing call, never `None`.
    /// The real bundle's full DL closure is far larger than `DEFAULT_MAX_STEPS` (the sibling
    /// `verify_graph_budget_cut_yields_attestation_never_certificate_heavy_offgate` above
    /// already shows even `max_steps: 1` cuts it), so a call that omits the args entirely
    /// must land on the SAME governed, non-conclusive `CoherenceCheckAttestation` —
    /// `budget-exhausted` / `incomplete` — never a conclusive certificate an unbounded chase
    /// would be needed to produce.
    #[test]
    fn verify_graph_omitted_max_steps_is_governed_not_unbounded_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();
        let path_str = overlay_path.to_str().unwrap();

        // The exact agent-omission shape R4 forbids treating as unbounded: no `max_steps`,
        // no `max_answers` key at all in the call args.
        let out = text_payload(server.call_tool_result("verify_graph", &json!({"path": path_str})));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["class_local_name"], "CoherenceCheckAttestation",
            "an omitted-max_steps call over the large bundle union must still be GOVERNED \
             (budget-cut) by the default server-side ceiling, never a conclusive certificate \
             that only an unbounded chase could produce: {out}"
        );
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "an omitted-max_steps call must NEVER run to an unbounded conclusive closure: {out}"
        );
        assert_eq!(
            out["completeness"], "incomplete",
            "the default ceiling must cut the real bundle's closure: {out}"
        );
        assert_eq!(
            out["evaluation"], "budget-exhausted",
            "the omitted max_steps must resolve to the finite DEFAULT_MAX_STEPS, never \
             None/unbounded: {out}"
        );

        // The grounded judgment's own carried budget usage confirms the finite ceiling: the
        // consumed step count is bounded by (and the declared allowance matches) the SAME
        // DEFAULT_MAX_STEPS `governed_budget` stamps — never absent (which would mean
        // unbounded).
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        let parsed = gmeow_logic::result_rdf::parse_reasoning_graph(judgment)
            .expect("judgment_nquads parses as a reasoning-result graph");
        assert_eq!(
            parsed.provenance.consumed_budget.allowance,
            Some(DEFAULT_MAX_STEPS),
            "the grounded judgment must declare the DEFAULT_MAX_STEPS allowance, never an \
             absent (unbounded) allowance: {judgment}"
        );
        assert!(
            parsed.provenance.consumed_budget.consumed <= DEFAULT_MAX_STEPS,
            "consumed steps must never exceed the declared default allowance: {judgment}"
        );
        drop(overlay_dir);
    }

    /// An overlay exceeding `MAX_VERIFY_OVERLAY_QUADS` is a HARD FAIL — the bounded agent
    /// path — refused BEFORE any reasoning runs, never a silently truncated graph.
    #[test]
    fn verify_graph_rejects_an_oversized_overlay() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("oversized.ttl");
        // One distinct triple over the ceiling — the smallest overlay that trips it.
        let mut body = String::with_capacity((MAX_VERIFY_OVERLAY_QUADS + 1) * 40);
        for i in 0..=MAX_VERIFY_OVERLAY_QUADS {
            body.push_str(&format!("<urn:ex:s{i}> <urn:ex:p> <urn:ex:o{i}> .\n"));
        }
        fs::write(&overlay_path, &body).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 1})),
        );
        assert_eq!(
            out["ok"], false,
            "an oversized overlay must hard-fail: {out}"
        );
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("exceeding"),
            "the error must name the ceiling breach: {out}"
        );
        drop(overlay_dir);
    }

    /// The R4 byte gate: an overlay FILE exceeding `MAX_VERIFY_OVERLAY_BYTES` is
    /// refused via `fs::metadata` (a stat) BEFORE it is ever `fs::read` into memory or
    /// handed to the parser — so a huge file can never exhaust memory before the
    /// post-parse `MAX_VERIFY_OVERLAY_QUADS` ceiling gets a chance to run. The filler
    /// here is a single deliberately-oversized comment line, NOT well-formed RDF that
    /// would parse into many quads: if the byte gate did not run before the read/parse
    /// (i.e. this fix regressed), the file would still parse successfully (as an
    /// empty, all-comment document) and `verify_graph` would return `ok:true` instead
    /// of hard-failing on the byte ceiling, so this test would catch the regression
    /// either way — and it must never OOM proving it.
    #[test]
    fn verify_graph_rejects_an_overlay_over_the_byte_ceiling_before_read() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("oversized-bytes.ttl");
        // One byte over the ceiling — the smallest overlay that trips it. A single
        // `#`-prefixed line is cheap to build (one allocation, no per-quad
        // formatting) and never parses into any quads.
        let filler = vec![b'#'; (MAX_VERIFY_OVERLAY_BYTES + 1) as usize];
        fs::write(&overlay_path, &filler).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 1})),
        );
        assert_eq!(
            out["ok"], false,
            "an overlay over the byte ceiling must hard-fail: {out}"
        );
        let error = out["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(&MAX_VERIFY_OVERLAY_BYTES.to_string()) && error.contains("byte"),
            "the error must name the byte limit: {out}"
        );
        drop(overlay_dir);
    }

    /// A normal, well-under-ceiling overlay still succeeds through both the byte gate
    /// and the quad gate — the byte cap must never reject a legitimate small annex.
    #[test]
    fn verify_graph_accepts_a_normal_small_overlay() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("small.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 1})),
        );
        assert_eq!(
            out["ok"], true,
            "a normal small overlay must succeed: {out}"
        );
        drop(overlay_dir);
    }

    /// The grounded RDF judgment: `judgment_nquads` carries the `logic:ReasoningResult`
    /// node and round-trips through the shared reasoning-graph parser, so an agent can
    /// reason over the verdict itself. Its evaluation axis matches the JSON envelope's.
    #[test]
    fn verify_graph_judgment_nquads_grounds_the_reasoning_result_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 1})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the judgment must ground a logic:ReasoningResult node: {judgment}"
        );
        // It parses back through the shared reasoning-graph reader (round-trip honesty),
        // and its evaluation axis matches the envelope's.
        let parsed = gmeow_logic::result_rdf::parse_reasoning_graph(judgment)
            .expect("judgment_nquads parses as a reasoning-result graph");
        assert_eq!(
            parsed.evaluation.wire(),
            out["evaluation"].as_str().unwrap(),
            "the grounded judgment's evaluation axis must match the envelope: {out}"
        );
        drop(overlay_dir);
    }

    // ── explain_quad ──────────────────────────────────────────────────────────

    /// A synthetic explain row for the fast disambiguation / N3 unit tests.
    fn synthetic_row(graph: &str, subject: &str, predicate: &str, obj: &str) -> Row {
        Row {
            graph: graph.to_owned(),
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            obj: obj.to_owned(),
            derivation_id: "urn:ex:deriv".to_owned(),
            rule_iri: "urn:ex:rule".to_owned(),
            source_quad_ids: Vec::new(),
        }
    }

    /// FAST (on-gate) unit test of the world-disambiguation helper on synthetic rows:
    /// a reifier shared across two worlds resolves to exactly the row in the supplied
    /// graph; a non-resolving graph over that multi-world reifier is an ambiguity HARD
    /// FAIL (never an arbitrary pick); a reifier no row carries is `not in closure`.
    #[test]
    fn explain_quad_disambiguation_resolves_by_graph_and_hard_fails_across_worlds() {
        let (world_a, world_b) = ("urn:ex:world-a", "urn:ex:world-b");
        let (s, p, o) = ("urn:ex:s", "urn:ex:p", "<urn:ex:o>");
        let rows = vec![
            synthetic_row(world_a, s, p, o),
            synthetic_row(world_b, s, p, o),
        ];
        let reifier = reifier_from_row(&rows[0]);
        assert_eq!(
            reifier,
            reifier_from_row(&rows[1]),
            "identical (S,P,O) shares a reifier across worlds"
        );

        // The supplied graph disambiguates to exactly one row.
        assert_eq!(locate_explain_target(&rows, &reifier, world_a).unwrap(), 0);
        assert_eq!(locate_explain_target(&rows, &reifier, world_b).unwrap(), 1);

        // A non-resolving graph over a multi-world reifier → ambiguity hard fail.
        let ambiguous = locate_explain_target(&rows, &reifier, "urn:ex:world-c").unwrap_err();
        assert!(
            ambiguous.to_string().contains("ambiguous"),
            "a cross-world reifier without a resolving graph must be ambiguous: {ambiguous}"
        );

        // A reifier no row carries → not in closure (never an arbitrary pick).
        let missing = locate_explain_target(
            &rows,
            "https://blackcatinformatics.ca/gmeow/reifier/deadbeef",
            world_a,
        )
        .unwrap_err();
        assert!(
            missing.to_string().contains("not in closure"),
            "an unknown reifier must be `not in closure`: {missing}"
        );
    }

    /// FAST (on-gate): a single-world reifier queried in the WRONG world is
    /// `not in closure`, and the error names the world the quad actually lives in.
    #[test]
    fn explain_quad_wrong_single_world_is_not_in_closure() {
        let rows = vec![synthetic_row(
            "urn:ex:world-a",
            "urn:ex:s",
            "urn:ex:p",
            "<urn:ex:o>",
        )];
        let reifier = reifier_from_row(&rows[0]);
        let err = locate_explain_target(&rows, &reifier, "urn:ex:other-world").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not in closure"), "{msg}");
        assert!(
            msg.contains("urn:ex:world-a"),
            "names the actual world: {msg}"
        );
    }

    /// FAST (on-gate): the canonical object N3 surface `explain_quad` builds is
    /// byte-identical to what a `Row.obj` carries (`term_display`), across IRI, plain
    /// literal, and typed literal; a bad `object_kind` and an `object_datatype` on an
    /// IRI object are HARD FAILS; and the omitted-kind inference is deterministic.
    #[test]
    fn explain_quad_object_n3_is_the_canonical_row_surface() {
        // IRI object → `<iri>` (explicit and inferred).
        assert_eq!(
            object_term_n3("urn:ex:o", Some("iri"), None).unwrap(),
            "<urn:ex:o>"
        );
        assert_eq!(
            object_term_n3("urn:ex:o", None, None).unwrap(),
            "<urn:ex:o>"
        );
        // Plain literal → `"lex"` (xsd:string elided, exactly like term_display).
        assert_eq!(
            object_term_n3("hello", Some("literal"), None).unwrap(),
            "\"hello\""
        );
        // A bare non-IRI value with whitespace is inferred as a literal.
        assert_eq!(
            object_term_n3("just text", None, None).unwrap(),
            "\"just text\""
        );
        // Typed literal → `"lex"^^<dt>`.
        assert_eq!(
            object_term_n3(
                "5",
                Some("literal"),
                Some("http://www.w3.org/2001/XMLSchema#integer")
            )
            .unwrap(),
            "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        // The N3 surface must MATCH what a Row built from the same TermValue carries.
        let iri_display = term_display(&TermValue::iri("urn:ex:o"));
        assert_eq!(
            object_term_n3("urn:ex:o", Some("iri"), None).unwrap(),
            iri_display
        );

        // A non-iri/literal kind is a hard error.
        assert!(object_term_n3("x", Some("bnode"), None).is_err());
        // A datatype on an IRI object is a contradictory request — hard error.
        assert!(object_term_n3("urn:ex:o", Some("iri"), Some("http://x")).is_err());
    }

    /// Reason the shipped bundle under `max_steps` and return the premise-closed
    /// explain rows — the SAME row set `run_explain_quad` reconstructs a target from.
    fn reasoned_rows(server: &McpServer, max_steps: u64) -> Vec<Row> {
        let budget = Budget {
            max_answers: None,
            max_steps: Some(max_steps),
        };
        let result = reason_all_budgeted(server.view.dataset.as_ref(), &budget)
            .expect("the shipped bundle reasons under the governor");
        explain::rows_for_result(&result).expect("rows build from the reasoning result")
    }

    /// The first row satisfying `pred` whose `(graph, reifier)` identity is UNIQUE in
    /// `rows`, so `explain_quad` resolves to exactly that row (never a duplicate).
    fn find_unique_row(rows: &[Row], pred: impl Fn(&Row) -> bool) -> Option<&Row> {
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        for row in rows {
            *counts
                .entry((row.graph.clone(), reifier_from_row(row)))
                .or_default() += 1;
        }
        rows.iter()
            .find(|row| pred(row) && counts[&(row.graph.clone(), reifier_from_row(row))] == 1)
    }

    /// HEAVY (off-gate): a DERIVED quad in the reasoned bundle closure explains to a
    /// non-empty skeleton (target first, `is_asserted:false`, firing rule preserved),
    /// a non-empty cited-IRI set that includes the target's reifier, and `faithful:true`.
    #[test]
    fn explain_quad_explains_a_derived_quad_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // A small governor budget already derives IRI-object quads (subsumption /
        // typing) while the mid-chase cut keeps the whole-bundle reason bounded — a
        // larger budget explodes the closure without adding coverage.
        let max_steps = 64u64;
        let rows = reasoned_rows(&server, max_steps);
        let derived = find_unique_row(&rows, |row| {
            row.rule_iri != gmeow_logic::provenance::ASSERT_RULE_IRI
                && row.obj.starts_with('<')
                && row.obj.ends_with('>')
        })
        .expect("the reasoned bundle must derive at least one unique IRI-object quad");
        let object_value = derived.obj[1..derived.obj.len() - 1].to_owned();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": derived.subject,
                "predicate": derived.predicate,
                "object_value": object_value,
                "object_kind": "iri",
                "graph": derived.graph,
                "max_steps": max_steps,
            }),
        ));
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        assert_eq!(out["faithful"], true, "the proof must be faithful: {out}");
        let steps = out["step_skeleton"]
            .as_array()
            .expect("step_skeleton array");
        assert!(
            !steps.is_empty(),
            "a derived quad has a non-empty skeleton: {out}"
        );
        assert_eq!(
            steps[0]["is_asserted"], false,
            "the target step is derived: {out}"
        );
        assert_eq!(
            steps[0]["rule_iri"].as_str().unwrap(),
            derived.rule_iri,
            "the target step preserves the firing rule: {out}"
        );
        let cited = out["cited_iris"].as_array().expect("cited_iris array");
        assert!(!cited.is_empty(), "cited_iris must be non-empty: {out}");
        let reifier = reifier_from_row(derived);
        assert!(
            cited.iter().any(|c| c.as_str() == Some(reifier.as_str())),
            "the target reifier must be cited (the reifier matches the request): {out}"
        );
        assert_eq!(
            steps[0]["obj_n3"].as_str().unwrap(),
            derived.obj,
            "the target step's object N3 matches the row surface: {out}"
        );
    }

    /// HEAVY (off-gate): an ASSERTED (EDB) quad explains to a SINGLE step with
    /// `is_asserted:true` and `rule_iri` = the assert-rule IRI.
    #[test]
    fn explain_quad_explains_an_asserted_quad_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // Asserted rows are present regardless of chase depth, so a tiny budget suffices.
        let max_steps = 8u64;
        let rows = reasoned_rows(&server, max_steps);
        let asserted = find_unique_row(&rows, |row| {
            row.rule_iri == gmeow_logic::provenance::ASSERT_RULE_IRI
                && row.obj.starts_with('<')
                && row.obj.ends_with('>')
        })
        .expect("the reasoned bundle must carry a unique asserted IRI-object quad");
        let object_value = asserted.obj[1..asserted.obj.len() - 1].to_owned();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": asserted.subject,
                "predicate": asserted.predicate,
                "object_value": object_value,
                "object_kind": "iri",
                "graph": asserted.graph,
                "max_steps": max_steps,
            }),
        ));
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        assert_eq!(out["faithful"], true, "the proof must be faithful: {out}");
        let steps = out["step_skeleton"]
            .as_array()
            .expect("step_skeleton array");
        assert_eq!(
            steps.len(),
            1,
            "an asserted quad is a single leaf step: {out}"
        );
        assert_eq!(steps[0]["is_asserted"], true, "the leaf is asserted: {out}");
        assert_eq!(
            steps[0]["rule_iri"].as_str().unwrap(),
            gmeow_logic::provenance::ASSERT_RULE_IRI,
            "the asserted leaf carries the assert-rule IRI: {out}"
        );
    }

    /// HEAVY (off-gate): a quad the closure does not entail is a HARD FAIL
    /// (`ok:false` + `not in closure`), NEVER an empty-but-ok proof.
    #[test]
    fn explain_quad_hard_fails_on_a_quad_not_in_closure_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": "urn:ex:not-a-real-bundle-subject",
                "predicate": "urn:ex:not-a-real-bundle-predicate",
                "object_value": "urn:ex:not-a-real-bundle-object",
                "object_kind": "iri",
                "graph": "urn:ex:not-a-real-world",
                "max_steps": 8,
            }),
        ));
        assert_eq!(out["ok"], false, "a bogus quad must hard-fail: {out}");
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("not in closure"),
            "the error must say the quad is not in the closure: {out}"
        );
    }

    /// HEAVY (off-gate): `judgment_nquads` grounds the `logic:ReasoningResult` node
    /// and round-trips through the shared reasoning-graph reader.
    #[test]
    fn explain_quad_judgment_nquads_grounds_the_reasoning_result_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let max_steps = 8u64;
        let rows = reasoned_rows(&server, max_steps);
        let asserted = find_unique_row(&rows, |row| {
            row.rule_iri == gmeow_logic::provenance::ASSERT_RULE_IRI
                && row.obj.starts_with('<')
                && row.obj.ends_with('>')
        })
        .expect("the reasoned bundle must carry a unique asserted IRI-object quad");
        let object_value = asserted.obj[1..asserted.obj.len() - 1].to_owned();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": asserted.subject,
                "predicate": asserted.predicate,
                "object_value": object_value,
                "object_kind": "iri",
                "graph": asserted.graph,
                "max_steps": max_steps,
            }),
        ));
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the judgment must ground a logic:ReasoningResult node: {judgment}"
        );
        let parsed = gmeow_logic::result_rdf::parse_reasoning_graph(judgment)
            .expect("judgment_nquads parses as a reasoning-result graph");
        // The grounded judgment carries a defined completeness axis (round-trip honesty).
        assert!(
            !parsed.completeness.wire().is_empty(),
            "the grounded judgment must carry a completeness axis: {judgment}"
        );
    }

    /// Full MCP protocol conformance over the real JSON-RPC dispatch: the
    /// handshake, the discovery surfaces, a read tool call and a TR-write tool call
    /// with `dry_run=true` (asserting the write stays hypothetical), and that EVERY
    /// advertised tool is dispatch-callable (no `unknown tool`).
    #[test]
    fn json_rpc_protocol_conformance_round_trip() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let rpc = |body: &str| -> Value {
            let raw = server.handle_message(body);
            let value: Value = serde_json::from_str(&raw).expect("response is JSON");
            assert_eq!(value["jsonrpc"], "2.0", "JSON-RPC framing: {value}");
            value
        };

        // initialize
        let init = rpc(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");
        assert!(init["result"]["protocolVersion"].as_str().is_some());

        // tools/list
        let tools = rpc(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
        let tool_names: Vec<String> = tools["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "lookup_term",
            "query_docs",
            "query_local",
            "store_claim",
            "recall",
        ] {
            assert!(
                tool_names.iter().any(|n| n == expected),
                "missing {expected}"
            );
        }

        // resources/list
        let resources = rpc(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}"#);
        assert!(
            resources["result"]["resources"]
                .as_array()
                .map(|r| !r.is_empty())
                .unwrap_or(false)
        );

        // tools/call — a read tool (query_docs ASK) succeeds.
        let read = rpc(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_docs","arguments":{"query":"ASK { ?s ?p ?o }"}}}"#,
        );
        let read_text: Value =
            serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(read_text["ok"], true);
        assert_eq!(read_text["boolean"], true);

        // tools/call — a TR-write tool (store_claim) with dry_run=true stays
        // hypothetical: the verdict is computed but nothing is committed.
        let dry = rpc(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"store_claim","arguments":{"text":"a conformance probe belief","dry_run":true}}}"#,
        );
        let dry_text: Value =
            serde_json::from_str(dry["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(dry_text["ok"], true);
        assert_eq!(dry_text["dry_run"], true);
        assert_eq!(dry_text["transaction"]["committed"], false);
        assert!(dry_text.get("claim").is_none(), "dry run commits no claim");
        // Nothing persisted by the dry-run write.
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());

        // Every advertised tool is dispatch-callable (recognized by tools/call).
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();
        let mut call_args: HashMap<&str, Value> = HashMap::new();
        call_args.insert("lookup_term", json!({"term": "gmeow:Entity"}));
        call_args.insert("doc_card", json!({"term": "gmeow:Entity"}));
        call_args.insert("query_docs", json!({"query": "ASK { ?s ?p ?o }"}));
        call_args.insert("docs_search", json!({"query": "entity"}));
        call_args.insert(
            "query_local",
            json!({"path": overlay_path.to_str().unwrap(), "query": "ASK { ?s ?p ?o }"}),
        );
        // The default (Tier-1) `validate_local` path is fast, so a valid tiny graph
        // dispatches and returns a well-formed EnrichedReport. (Tier-2 `deep` is opt-in
        // and reasons over the whole bundle, minutes — never exercised in this loop.)
        call_args.insert(
            "validate_local",
            json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle"}),
        );
        call_args.insert("store_claim", json!({"text": "probe", "dry_run": true}));
        call_args.insert(
            "conjecture_test",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
            }),
        );
        call_args.insert(
            "store_conjecture",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
                "dry_run": true,
            }),
        );
        call_args.insert("recall", json!({}));
        call_args.insert(
            "revise_belief",
            json!({"claim_id": "urn:gmeow:assertion:none", "dry_run": true}),
        );
        call_args.insert(
            "refute_conjecture",
            json!({"conjecture_id": "urn:gmeow:conjecture:none", "dry_run": true}),
        );
        // Documentation-surface tools: real terms that carry live data in the shipped
        // bundle (gmeow:Activity documents fixtures, gmeow:Entity grounds
        // entailments); competency_questions dispatches in its whole-index form.
        call_args.insert("counter_examples", json!({"term": "gmeow:Activity"}));
        call_args.insert("entailments", json!({"term": "gmeow:Entity"}));
        call_args.insert("competency_questions", json!({}));
        for name in &tool_names {
            let args = call_args
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = server.call_tool_result(name, &args);
            let content = result["content"][0]["text"].as_str().expect("tool text");
            assert!(
                !content.contains("unknown tool"),
                "advertised tool {name} is not dispatch-callable: {content}"
            );
        }
        drop(overlay_dir);
    }

    /// Build a consumer server over the shipped bundle with a clean language env.
    fn consumer_server() -> McpServer {
        let bytes = snapshot();
        McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap()
    }

    /// The sorted finding-code multiset of a report.
    fn codes_of(report: &gmeow_errors::Report) -> Vec<String> {
        let mut codes: Vec<String> = report.findings.iter().map(|f| f.code.clone()).collect();
        codes.sort();
        codes
    }

    /// Select a REAL counter-example fixture from the SHIPPED bundle's
    /// `gmeow:graph/documentation` projection whose authored `gmeow:docViolationCode`
    /// is actually REPRODUCED by the Tier-1 SHACL engine on its own `gmeow:docFixtureText`
    /// body — the honest correspondence anchor (never weakened to "non-empty").
    ///
    /// The candidate for each code is the SAME one the enrichment join attaches:
    /// `fixture_maps` keys a code to the lexicographically-first fixture IRI carrying
    /// it, so selecting by `code → (first-fixture IRI, its text)` guarantees the
    /// attached counter-example's text equals the payload we validate. Returns
    /// `(fixture_iri, code, text)`. Panics with the observed codes if NONE reproduces
    /// (a real blocker, not a soft skip).
    fn select_reproducing_counter_example(server: &McpServer) -> (String, String, String) {
        let rows = server
            .view
            .docs_select_rows(COUNTER_EXAMPLE_FIXTURE_QUERY)
            .expect("query counter-example fixtures from graph/documentation");
        // First row per fixture IRI (the fixture's code + full body).
        let mut by_fixture: BTreeMap<String, (String, String)> = BTreeMap::new();
        for row in &rows {
            if let (Some(f), Some(code), Some(text)) =
                (row.get("f"), row.get("code"), row.get("text"))
            {
                by_fixture
                    .entry(f.clone())
                    .or_insert_with(|| (code.clone(), text.clone()));
            }
        }
        // code → (first-fixture IRI, its text) — matches `fixture_maps`' first-wins,
        // so the attached counter-example is exactly this fixture.
        let mut by_code: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (iri, (code, text)) in &by_fixture {
            by_code
                .entry(code.clone())
                .or_insert_with(|| (iri.clone(), text.clone()));
        }
        assert!(
            !by_code.is_empty(),
            "the shipped bundle carries NO bound counter-example fixtures"
        );
        for (code, (iri, text)) in &by_code {
            let report = gmeow_validate::data_validate::run_tier1(
                text.as_bytes(),
                "turtle",
                server.view.gts_bytes(),
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate the fixture body");
            if report.findings.iter().any(|f| &f.code == code) {
                return (iri.clone(), code.clone(), text.clone());
            }
        }
        panic!(
            "no bound counter-example fixture reproduced its authored violation code under \
             Tier-1 validation — observed candidate codes: {:?}",
            by_code.keys().collect::<Vec<_>>()
        );
    }

    /// PARITY + CORRESPONDENCE (end-to-end, production surface): drive the REAL
    /// `validate_local` tool (default fast Tier-1 path) over a REAL counter-example
    /// fixture from the shipped bundle, and assert (a) PARITY — the enriched finding
    /// codes EQUAL `run_tier1` (the `gmeow validate` core); and (b) CORRESPONDENCE —
    /// at least one finding carries a counter-example, EVERY attached counter-example
    /// corresponds by violation code + rule help URI, and the finding whose code is
    /// the chosen fixture's `docViolationCode` gets that fixture's exact body back.
    #[test]
    fn validate_local_enrichment_parity_and_correspondence() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (fixture_iri, code, text) = select_reproducing_counter_example(&server);
        eprintln!("validate_local test selected fixture={fixture_iri} code={code}");
        let help_uri = gmeow_validate::rule_catalog::help_uri_for(&code);

        // The CLI core: Tier-1 (the same `run_tier1` `gmeow validate` drives).
        let tier1 = gmeow_validate::data_validate::run_tier1(
            text.as_bytes(),
            "turtle",
            server.view.gts_bytes(),
            MCP_NAMESPACE,
            VALIDATE_LOCAL_ORIGIN,
        )
        .expect("tier-1 validate");
        let tier1_codes = codes_of(&tier1);

        // Drive the REAL tool by DISPATCH BY NAME (deep defaults to false → fast).
        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_ne!(
            enriched["ok"],
            Value::Null,
            "the tool returned an EnrichedReport: {enriched}"
        );
        let findings = enriched["findings"].as_array().expect("findings array");

        // PARITY: with deep off, the tool's finding codes EQUAL the `gmeow validate`
        // (`run_tier1`) codes exactly — validate_local drops/adds/mutates nothing.
        let mut local_codes: Vec<String> = findings
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect();
        local_codes.sort();
        assert_eq!(
            local_codes, tier1_codes,
            "validate_local must reproduce the gmeow-validate Tier-1 finding codes exactly"
        );

        // CORRESPONDENCE (BINDING): the finding whose code == the fixture's violation
        // code carries THAT fixture's body back, with the corresponding help URI.
        let matched: Vec<&Value> = findings
            .iter()
            .filter(|f| f["code"].as_str() == Some(code.as_str()))
            .collect();
        assert!(
            !matched.is_empty(),
            "the chosen fixture's code {code} must appear among the findings: {enriched}"
        );
        let with_ce = matched
            .iter()
            .find(|f| !f["counter_example"].is_null())
            .unwrap_or_else(|| {
                panic!("the matched finding {code} must carry a counter-example: {enriched}")
            });
        assert_eq!(
            with_ce["counter_example"]["violation_code"].as_str(),
            Some(code.as_str()),
            "the attached counter-example corresponds by violation code"
        );
        assert_eq!(
            with_ce["counter_example"]["text"].as_str(),
            Some(text.as_str()),
            "the finding whose code is the fixture's violation code gets that fixture's body back"
        );
        assert_eq!(
            with_ce["help_uri"].as_str(),
            Some(help_uri.as_str()),
            "the finding carries the rule catalog help URI for its code"
        );

        // NON-VACUITY + the CORRESPONDENCE INVARIANT across the whole report: at least
        // one finding got a counter-example, and EVERY attached counter-example's
        // violation code equals its OWN finding's code (by-code, never blanket).
        let attached = findings
            .iter()
            .filter(|f| !f["counter_example"].is_null())
            .count();
        assert!(
            attached >= 1,
            "at least one finding must carry a counter-example (non-vacuous): {enriched}"
        );
        for f in findings {
            if !f["counter_example"].is_null() {
                assert_eq!(
                    f["counter_example"]["violation_code"].as_str(),
                    f["code"].as_str(),
                    "every attached counter-example corresponds to ITS finding's code \
                     (correspondence, never blanket): {f}"
                );
                assert_eq!(
                    f["help_uri"].as_str(),
                    Some(
                        gmeow_validate::rule_catalog::help_uri_for(f["code"].as_str().unwrap())
                            .as_str()
                    ),
                    "every enriched finding carries its rule catalog help URI: {f}"
                );
            }
        }
    }

    /// SELECT a REAL well-formed conformance fixture from the SHIPPED bundle's
    /// `gmeow:graph/documentation` projection whose `gmeow:docFixtureText` body
    /// ACTUALLY validates clean under Tier-1 (no `Error`-severity finding) — the
    /// honest correspondence anchor for "a claim consistent with the bundled
    /// axioms", mirroring [`select_reproducing_counter_example`]'s reproduction
    /// discipline: never hand-authored, always a REAL fixture the engine itself
    /// agrees is clean. Returns `(fixture_iri, text)`. Panics with the candidate
    /// count if NONE reproduces (a real blocker, not a soft skip).
    fn select_reproducing_wellformed_example(server: &McpServer) -> (String, String) {
        let rows = server
            .view
            .docs_select_rows(WELLFORMED_FIXTURE_QUERY)
            .expect("query well-formed fixtures from graph/documentation");
        let mut by_fixture: BTreeMap<String, String> = BTreeMap::new();
        for row in &rows {
            if let (Some(f), Some(text)) = (row.get("f"), row.get("text")) {
                by_fixture.entry(f.clone()).or_insert_with(|| text.clone());
            }
        }
        assert!(
            !by_fixture.is_empty(),
            "the shipped bundle carries NO bound well-formed conformance fixtures"
        );
        for (iri, text) in &by_fixture {
            let report = gmeow_validate::data_validate::run_tier1(
                text.as_bytes(),
                "turtle",
                server.view.gts_bytes(),
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate the fixture body");
            if report.ok() {
                return (iri.clone(), text.clone());
            }
        }
        panic!(
            "no bound well-formed fixture actually validated clean under Tier-1 — {} \
             candidates checked, none reproduced a clean Tier-1 pass",
            by_fixture.len()
        );
    }

    /// ACCEPTANCE (R5, half 1): drive the REAL `validate_local` tool over a REAL
    /// well-formed fixture from the shipped bundle — a claim CONSISTENT with the
    /// bundled axioms — and assert the production surface reports it clean: `ok:
    /// true`, and every finding present (if any non-error advisory survives) still
    /// carries the full teaching surface (`help_uri`, and `entails`/
    /// `wellformed_exemplar` when the finding names a documented term). Never
    /// idealized: a clean claim MAY still carry Warning/Note/Info findings (the
    /// tool only hard-rejects on `Error` severity), so this asserts on `ok`, not
    /// on an empty findings array.
    #[test]
    fn validate_local_accepts_a_consistent_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (fixture_iri, text) = select_reproducing_wellformed_example(&server);
        eprintln!("validate_local clean-claim test selected fixture={fixture_iri}");

        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_eq!(
            enriched["ok"], true,
            "a claim consistent with the bundled axioms must validate clean: {enriched}"
        );
        assert_eq!(enriched["tool"].as_str(), Some("validate"));
        let findings = enriched["findings"].as_array().expect("findings array");
        // `ok: true` tolerates non-Error survivors; whichever DO appear must still
        // carry the full enrichment surface (never a bare code+message).
        for f in findings {
            assert!(
                f["help_uri"].as_str().is_some_and(|u| !u.is_empty()),
                "every surfaced finding carries a non-empty rule catalog help_uri: {f}"
            );
            assert!(
                f["finding_iri"].is_string(),
                "every finding has a stable IRI: {f}"
            );
        }
    }

    /// ACCEPTANCE (R5, half 2): drive the REAL `validate_local` tool over a REAL
    /// counter-example fixture from the shipped bundle — a claim that VIOLATES a
    /// bundled SHACL shape / modelling discipline — and assert the production
    /// surface REJECTS it: the tool surfaces the violating finding code at `Error`
    /// severity (proven against the raw Tier-1 `Report`, since the enriched
    /// envelope does not carry severity), and the enriched envelope is `ok: false`
    /// (never a silent clean pass on inconsistent input).
    #[test]
    fn validate_local_rejects_an_inconsistent_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (fixture_iri, code, text) = select_reproducing_counter_example(&server);
        eprintln!(
            "validate_local inconsistent-claim test selected fixture={fixture_iri} code={code}"
        );

        // The raw Tier-1 report (the `gmeow validate` core) proves the specific
        // finding code fires at `Error` severity — the hard-reject signal
        // `EnrichedFinding` does not itself carry.
        let tier1 = gmeow_validate::data_validate::run_tier1(
            text.as_bytes(),
            "turtle",
            server.view.gts_bytes(),
            MCP_NAMESPACE,
            VALIDATE_LOCAL_ORIGIN,
        )
        .expect("tier-1 validate the violating fixture");
        let tier1_finding = tier1
            .findings
            .iter()
            .find(|f| f.code == code)
            .unwrap_or_else(|| panic!("tier-1 report must reproduce code {code}: {tier1:?}"));
        assert_eq!(
            tier1_finding.severity,
            gmeow_errors::Severity::Error,
            "the chosen counter-example must violate at Error severity (a hard reject, not \
             an advisory): {tier1_finding:?}"
        );
        assert!(
            !tier1.ok(),
            "a report carrying an Error-severity finding must not be ok(): {tier1:?}"
        );

        // Drive the REAL production tool over the SAME violating claim.
        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_eq!(
            enriched["ok"], false,
            "a claim violating a bundled axiom must be rejected, not validated clean: {enriched}"
        );
        let findings = enriched["findings"].as_array().expect("findings array");
        assert!(
            findings
                .iter()
                .any(|f| f["code"].as_str() == Some(code.as_str())),
            "the rejecting tool must surface the specific violation code {code}: {enriched}"
        );
    }

    /// Compile a hand-authored draft-2020-12 schema and assert `instance` CONFORMS,
    /// surfacing every validation error verbatim on failure (a real payload the
    /// schema rejects is a schema bug to FIX, never to weaken).
    fn assert_conforms(schema: &Value, instance: &Value, what: &str) {
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(schema)
            .unwrap_or_else(|e| panic!("{what}: schema does not compile: {e}"));
        let errors: Vec<String> = validator
            .iter_errors(instance)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{what}: real payload does not conform to its schema:\n{}\ninstance: {instance}",
            errors.join("\n")
        );
    }

    /// SELF-DESCRIBING SURFACE (card): a REAL `doc_card format=json` payload — the
    /// exact bytes the packed `terms/{slug}/card.json` member carries — CONFORMS to
    /// the hand-authored `card.schema.json` (`gmeow_docs::card::card_json_schema`).
    /// Both the STANDARD tier (the `card.json` shape) and the FULL tier (every rich
    /// panel, exercising the `$defs`) are checked against the SAME schema.
    #[test]
    fn card_json_conforms_to_card_schema() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let schema = gmeow_docs::card::card_json_schema();

        // STANDARD tier — the `card.json` shape. `gmeow:Entity` is a real bundled term.
        let standard = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": "gmeow:Entity", "format": "json", "detail": "standard"}),
        ));
        assert_eq!(standard["ok"], true, "doc_card standard: {standard}");
        assert_conforms(&schema, &standard["card"], "card.json (standard tier)");

        // FULL tier — exercises the rich panels ($defs). `gmeow:Activity` documents
        // conformance fixtures and `gmeow:Entity` grounds entailments in the shipped
        // bundle, so at least one full card carries populated panels.
        for term in ["gmeow:Activity", "gmeow:Entity"] {
            let full = text_payload(server.call_tool_result(
                "doc_card",
                &json!({"term": term, "format": "json", "detail": "full"}),
            ));
            assert_eq!(full["ok"], true, "doc_card full for {term}: {full}");
            assert_conforms(&schema, &full["card"], "card.json (full tier)");
        }
    }

    /// SELF-DESCRIBING SURFACE (finding): a REAL `validate_local` envelope — produced
    /// by driving the tool over a REAL counter-example from the shipped bundle —
    /// CONFORMS to the hand-authored `validate-finding.schema.json`
    /// (`gmeow_validate::local_oracle::finding_json_schema`), exercising the finding /
    /// fixture / entailment `$defs` on a payload that carries an attached
    /// counter-example.
    #[test]
    fn enriched_report_conforms_to_finding_schema() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (_fixture_iri, _code, text) = select_reproducing_counter_example(&server);
        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        // The chosen fixture reproduces its violation, so the envelope is non-vacuous:
        // at least one finding, and at least one attached counter-example fixture.
        let findings = enriched["findings"].as_array().expect("findings array");
        assert!(!findings.is_empty(), "expected findings: {enriched}");
        assert!(
            findings.iter().any(|f| !f["counter_example"].is_null()),
            "expected an attached counter-example (exercises the fixture $def): {enriched}"
        );
        let schema = gmeow_validate::local_oracle::finding_json_schema();
        assert_conforms(&schema, &enriched, "validate_local EnrichedReport");
    }

    /// DEEP PASS (heavy): drive the tool end-to-end with `deep = true` over a REAL
    /// counter-example from the shipped bundle and assert the Tier-2 semantic pass ran
    /// (a `validate.deep.*` finding is present) AND that the Tier-1 surface is
    /// preserved (true tool-vs-`gmeow validate` parity). `#[ignore]`d because the
    /// native deep reasoner over the whole bundle runs well past the 120 s on-gate
    /// nextest cliff; run in the heavy lane / manually
    /// (`cargo nextest run -E 'test(validate_local_deep)' --run-ignored all`).
    /// `validate.deep.contract-invalid` is engine-enforced (it fires only on a bundle
    /// carrying a garbled `logic:admissibleValuation`, which the shipped bundle does
    /// not) and is regression-covered by
    /// `gmeow_validate::data_validate` `deep_pass_garbled_contract_produces_error_not_advisory`.
    #[test]
    #[ignore = "runs the full-bundle native deep reasoner (>120s); heavy lane only"]
    fn validate_local_deep_pass_surfaces_deep_finding() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (_iri, _code, text) = select_reproducing_counter_example(&server);

        // The CLI Tier-1 surface the deep run must preserve.
        let tier1 = gmeow_validate::data_validate::run_tier1(
            text.as_bytes(),
            "turtle",
            server.view.gts_bytes(),
            MCP_NAMESPACE,
            VALIDATE_LOCAL_ORIGIN,
        )
        .expect("tier-1 validate");
        let tier1_codes = codes_of(&tier1);

        // Explicitly request the Tier-2 pass via the `deep` arg.
        let enriched = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": text, "format": "turtle", "deep": true}),
        ));
        let findings = enriched["findings"].as_array().expect("findings array");

        assert!(
            findings
                .iter()
                .any(|f| f["code"].as_str().unwrap().starts_with("validate.deep.")),
            "the deep pass must run (a validate.deep.* finding must appear): {enriched}"
        );
        let mut tier1_surface: Vec<String> = findings
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .filter(|c| !c.starts_with("validate.deep."))
            .collect();
        tier1_surface.sort();
        assert_eq!(
            tier1_surface, tier1_codes,
            "the deep run must preserve the Tier-1 surface (parity with gmeow validate)"
        );
    }

    /// ROBUSTNESS: an unknown `format`, an oversized `data` payload, and malformed
    /// RDF each return a well-formed error envelope (`ok:false`) — never a panic, and
    /// a malformed graph surfaces as an Error, not a silent success.
    #[test]
    fn validate_local_hard_fails_bad_format_oversize_and_malformed() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // Unknown format → error envelope listing the accepted tokens.
        let bad_format = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": "<urn:s> <urn:p> <urn:o> .", "format": "bogus"}),
        ));
        assert_eq!(
            bad_format["ok"], false,
            "unknown format must be a hard fail"
        );
        assert!(
            bad_format["error"]
                .as_str()
                .unwrap()
                .contains("unrecognized RDF format"),
            "the error must name the offending format: {bad_format}"
        );

        // Oversized payload → error envelope, no truncation.
        let huge = format!(
            "<urn:s> <urn:p> \"{}\" .",
            "x".repeat(MAX_VALIDATE_DATA_BYTES + 1)
        );
        let oversize = text_payload(
            server.call_tool_result("validate_local", &json!({"data": huge, "format": "turtle"})),
        );
        assert_eq!(
            oversize["ok"], false,
            "oversized payload must be a hard fail"
        );
        assert!(
            oversize["error"].as_str().unwrap().contains("ceiling"),
            "the error must explain the size ceiling: {oversize}"
        );

        // Malformed Turtle → error envelope (the parse hard-fails; never a silent
        // empty-but-ok report).
        let malformed = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": "<urn:s> <urn:p> <urn:o>  # unterminated, no dot", "format": "turtle"}),
        ));
        assert_eq!(
            malformed["ok"], false,
            "malformed RDF must surface as an error, not a silent success: {malformed}"
        );
    }

    /// The `explain_finding` tool, driven by DISPATCH BY NAME over a server built
    /// from the shipped bundle: a real fingerprint IRI returns the finding's code +
    /// a gate verdict; an unknown IRI is a HARD FAIL (isError), never an empty DAG.
    #[test]
    fn explain_finding_tool_walks_a_real_witness_and_hard_fails_unknown() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // Obtain a real fingerprint IRI the SAME way `explain` does: the first key of
        // the FindingIndex the reader rehydrates from the server's held snapshot. An
        // empty graph/diagnostics is a blocker, not something to paper over.
        let index = crate::diagnostics_reader::read_findings(&server.view.dataset)
            .expect("read graph/diagnostics from shipped bundle");
        assert!(
            !index.is_empty(),
            "shipped bundle graph/diagnostics carries NO findings — explain_finding has no witness"
        );
        let real_iri = index.findings.keys().next().unwrap().clone();
        let expected_code = index.get(&real_iri).unwrap().code.clone();

        let ok = text_payload(
            server.call_tool_result("explain_finding", &json!({"target_iri": real_iri})),
        );
        assert_eq!(ok["ok"], true, "explain_finding must succeed: {ok}");
        assert_eq!(ok["kind"], "finding");
        assert_eq!(ok["focus"]["code"], expected_code);
        assert!(
            ok["verdict"].as_str().is_some(),
            "explain_finding must carry a gate verdict: {ok}"
        );
        assert!(
            ok["focus"]["provenance_dag"]
                .as_str()
                .unwrap()
                .contains(&real_iri),
            "provenance DAG must render the focus finding: {ok}"
        );

        // Unknown target → hard fail (isError result), never a success with an empty DAG.
        let bad = server.call_tool_result("explain_finding", &json!({"target_iri": "urn:nope"}));
        assert_eq!(
            bad["isError"], true,
            "unknown IRI must be a hard fail: {bad}"
        );
        let bad_text = text_payload(bad);
        assert_eq!(bad_text["ok"], false);
        assert!(
            bad_text["error"]
                .as_str()
                .unwrap_or_default()
                .contains("unknown explain target")
        );
    }

    /// The SPARQL half of the acceptance criterion, on the PRODUCTION MCP surface:
    /// `query_local` runs a SELECT over the bundle's `graph/diagnostics` named graph
    /// and returns a real finding's code — proving SPARQL over the diagnostics
    /// projection executes through the shipped query tool (its canon is the full
    /// signed dataset, which retains the diagnostics graph).
    #[test]
    fn query_local_selects_a_finding_code_over_graph_diagnostics() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // query_local requires an overlay path; a trivial annex suffices — the query
        // itself targets the bundle's graph/diagnostics named graph directly.
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();

        let query = "SELECT ?s ?code WHERE { \
                     GRAPH <https://blackcatinformatics.ca/gmeow/graph/diagnostics> { \
                     ?s a <https://blackcatinformatics.ca/gmeow/Finding> ; \
                     <https://blackcatinformatics.ca/gmeow/findingCode> ?code } } LIMIT 1";
        let res = text_payload(server.call_tool_result(
            "query_local",
            &json!({"path": overlay_path.to_str().unwrap(), "query": query}),
        ));
        assert_eq!(
            res["ok"], true,
            "query_local over graph/diagnostics must succeed: {res}"
        );
        let bindings = res["results"]["bindings"]
            .as_array()
            .expect("bindings array");
        assert!(
            !bindings.is_empty(),
            "expected at least one Finding binding from graph/diagnostics: {res}"
        );
        assert!(
            bindings[0]["code"]["value"].as_str().is_some(),
            "the Finding binding must carry a findingCode literal: {res}"
        );
        drop(overlay_dir);
    }

    // ── Conjecture-library persistence ───────────────────────────────────────

    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    const MATH_NS: &str = "https://blackcatinformatics.ca/math/";
    const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    /// A `∀x. trigger(x, mark) → rdf:type(x, <cls>)` candidate, authored as a reified
    /// `logic:Formula` (a single top-level formula — the trivially-Horn consequent is a
    /// sub-formula, so it never trips the `with_formulas` guard).
    fn forall_horn_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:cand a logic:Formula ;\n\
                 logic:forall ex:body ;\n\
                 logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
             ex:body a logic:Formula ;\n\
                 logic:antecedent ex:ant ;\n\
                 logic:consequent ex:con .\n\
             ex:ant a logic:Formula ;\n\
                 logic:relation ex:trigger ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
             ex:con a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB where the candidate's head class is DISJOINT with the individual's asserted type,
    /// so firing `rdf:type(a, <cls>)` forces an `owl:Nothing` clash ⇒ refutation + witness.
    fn refuting_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             ex:A owl:disjointWith ex:{cls_local} .\n"
        )
    }

    /// A KB where the candidate's head class is UNRELATED (no disjointness), so firing derives
    /// a new consistent fact ⇒ Open/Neither, no witness.
    fn open_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash.\n"
        )
    }

    fn temp_conjecture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conjectures.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_CONJECTURE_PATH", &path);
        }
        (dir, path)
    }

    /// The imported conjecture-library dataset (all appended segments unioned).
    fn read_conjectures(path: &Path) -> std::sync::Arc<purrdf::RdfDataset> {
        let bytes = fs::read(path).expect("read conjecture library");
        purrdf::import_gts_events(&bytes)
            .expect("import conjecture library")
            .dataset
    }

    /// Every subject typed `logic:Conjecture` in `dataset`.
    fn conjecture_nodes(dataset: &purrdf::RdfDataset) -> BTreeSet<String> {
        dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == RDF_TYPE_IRI
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}Conjecture"))
            })
            .filter_map(|q| match q.subject {
                RdfTerm::Iri(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// The `logic:witnessPremise` literal lexicals in `dataset`.
    fn witness_premises(dataset: &purrdf::RdfDataset) -> Vec<String> {
        dataset
            .owned_quads()
            .filter(|q| q.predicate == format!("{LOGIC_NS}witnessPremise"))
            .filter_map(|q| match q.object {
                RdfTerm::Literal(lit) => Some(lit.lexical_form),
                _ => None,
            })
            .collect()
    }

    struct ConjEnvGuard;
    impl ConjEnvGuard {
        fn set() -> (EnvRestore, ConjEnvGuard) {
            let env = EnvRestore::capture(&[
                "GMEOW_LANG",
                "GMEOW_MEMORY_PATH",
                "GMEOW_CONJECTURE_PATH",
                "HOME",
                "USERPROFILE",
            ]);
            unsafe {
                // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
                env::remove_var("GMEOW_LANG");
            }
            (env, ConjEnvGuard)
        }
    }

    #[test]
    fn persist_conjecture_precondition_gates_the_commit() {
        // The write is a REAL TR gate: with the precondition present the committed run
        // succeeds; with it absent the run FAILS (so the tool returns ok:false before writing).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let ok = execute_memory_txn(
            MCP_PERSIST_CONJECTURE_SCHEMA,
            &[MCP_CONJECTURE_VERDICT_PRESENTED],
            false,
        )
        .unwrap();
        assert!(
            matches!(ok, TxReceipt::CommittedSuccess { .. }),
            "the precondition-present committed run must succeed: {ok:?}"
        );
        let unmet = execute_memory_txn(MCP_PERSIST_CONJECTURE_SCHEMA, &[], false).unwrap();
        assert!(
            matches!(unmet, TxReceipt::CommittedFailure { .. }),
            "a persist with the precondition UNMET must fail the commit (the tool then returns \
             ok:false before writing): {unmet:?}"
        );
    }

    #[test]
    fn conjecture_test_is_pure_and_writes_nothing() {
        // R3a: the "test" leg (`conjecture_test`) is a PURE hypothetical evaluation. Driving it
        // with a candidate that, under the OLD single-tool surface, would have persisted a
        // refutation must still return the full verdict envelope while leaving the library file
        // byte-unchanged (here: absent) — no TR gate, no append, ever.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "the pure test must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());
        // No persist/transaction section at all on this surface.
        assert!(
            resp.get("transaction").is_none(),
            "conjecture_test must never render a transaction section: {resp}"
        );
        assert!(
            resp.get("committed").is_none(),
            "conjecture_test must never render a committed flag: {resp}"
        );

        // T1 (G7): the grounded judgment travels on the SAME `judgment_nquads` key
        // `verify_graph`/`explain_quad` carry — even on this pure, nothing-persisted path —
        // and round-trips through the shared reader as a real `logic:Conjecture` node
        // embedding a non-empty `logic:ReasoningResult`.
        let judgment = resp["judgment_nquads"]
            .as_str()
            .expect("conjecture_test must carry a judgment_nquads string");
        assert!(
            !judgment.trim().is_empty(),
            "judgment_nquads must not be empty"
        );
        let record = gmeow_logic::result_rdf::parse_conjecture_verdict(judgment)
            .expect("judgment_nquads must parse as a conjecture-verdict graph");
        assert_eq!(
            record.lifecycle,
            ConjectureLifecycleState::RefutedInStandpoint
        );
        assert!(
            !project_reasoning_result(&record.verdict).trim().is_empty(),
            "the embedded logic:ReasoningResult body must be non-empty"
        );

        // The library file is BYTE-UNCHANGED (still absent): no TR gate, no append.
        assert!(
            !path.exists(),
            "a pure conjecture_test call must write nothing to the library"
        );
    }

    #[test]
    fn store_conjecture_refutes_and_persists_with_witness() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        assert!(
            !path.exists(),
            "library must not exist before the first persist"
        );
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "refutation persist must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());

        // The library file grew, and the witness premises round-trip back through the GTS.
        assert!(
            path.exists(),
            "the conjecture library file must have been written"
        );
        let dataset = read_conjectures(&path);
        assert!(
            conjecture_nodes(&dataset).contains(&node),
            "the content-addressed conjecture node must be readable back: {node}"
        );
        // The standpoint scope is recoverable.
        assert!(dataset.owned_quads().any(|q| {
            q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                && q.object == RdfTerm::iri("http://ex/standpoint/alice")
        }));
        // The witness premises are recoverable.
        let premises = witness_premises(&dataset);
        assert!(
            !premises.is_empty(),
            "a refutation must persist recoverable witness premises"
        );
    }

    #[test]
    fn store_conjecture_bridges_math_twin_via_conjecture_under_test() {
        // Driving the real MCP `store_conjecture` tool (the shared conjecture-test core) with a
        // `math_conjecture` must persist the always-present structural twin bridge
        // `<math> math:conjectureUnderTest <logic:Conjecture-node>` (domain math:Conjecture,
        // range logic:Conjecture) — readable back out of the append-only GTS library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let math_iri = "https://blackcatinformatics.ca/math/conjecture/goldbach";
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "math_conjecture": math_iri,
            }),
        ));
        assert_eq!(resp["ok"], true, "math-twin persist must succeed: {resp}");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();

        let dataset = read_conjectures(&path);
        let under_test = format!("{MATH_NS}conjectureUnderTest");
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(math_iri)
                    && q.predicate == under_test
                    && q.object == RdfTerm::iri(node.clone())
            }),
            "the math:conjectureUnderTest bridge <{math_iri}> -> <{node}> must be recoverable \
             from the persisted GTS library"
        );
    }

    #[test]
    fn store_conjecture_dry_run_writes_nothing() {
        // The `dry_run` witness on `store_conjecture` is a HYPOTHETICAL commit: the verdict and
        // TR receipt are computed exactly as a real commit would be, but nothing is appended.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }),
        ));
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["transaction"]["committed"], false);
        // The verdict is still computed and returned.
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        // Nothing written: the library file does not exist / is zero bytes.
        assert!(
            !path.exists() || fs::metadata(&path).unwrap().len() == 0,
            "a dry run must write nothing to the library"
        );

        // T1 (G7): `store_conjecture`'s dry-run path STILL carries the grounded judgment under
        // the SAME `judgment_nquads` key the read tools and `conjecture_test` use — a
        // hypothetical persist is not a hypothetical verdict; the engine really ran.
        let judgment = resp["judgment_nquads"]
            .as_str()
            .expect("store_conjecture dry-run must carry a judgment_nquads string");
        assert!(
            !judgment.trim().is_empty(),
            "judgment_nquads must not be empty"
        );
        let record = gmeow_logic::result_rdf::parse_conjecture_verdict(judgment)
            .expect("judgment_nquads must parse as a conjecture-verdict graph");
        assert_eq!(
            record.lifecycle,
            ConjectureLifecycleState::RefutedInStandpoint
        );
        assert!(
            !project_reasoning_result(&record.verdict).trim().is_empty(),
            "the embedded logic:ReasoningResult body must be non-empty"
        );

        // A second, committing call on the SAME candidate now appends for real: the dry run
        // above left the library byte-unchanged, so this is the library's FIRST segment.
        let committed = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(committed["ok"], true);
        assert!(path.exists() && fs::metadata(&path).unwrap().len() > 0);
        // The committed path carries judgment_nquads too.
        assert!(
            committed["judgment_nquads"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "the committed store_conjecture response must also carry judgment_nquads: {committed}"
        );
    }

    /// Store a real conjecture through the shipped `store_conjecture` tool and return its
    /// content-addressed node IRI (the shared setup for the refute tests below).
    fn store_one_conjecture(server: &McpServer, cls: &str) -> String {
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate(cls),
                "kb": open_kb(cls),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "store must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "open");
        resp["conjecture"].as_str().expect("node iri").to_string()
    }

    #[test]
    fn refute_conjecture_withdraws_and_appends_author_segment() {
        // store_conjecture then refute_conjecture its node IRI: the library gains a NEW
        // append-only segment marking that node ConjectureWithdrawn with the author reason;
        // the reader's EFFECTIVE lifecycle is now Withdrawn; the PRIOR segment bytes are
        // byte-for-byte intact (append-only, never mutated).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let node = store_one_conjecture(&server, "B");
        // Before withdrawal the effective state is the engine verdict (Open), never Withdrawn.
        let before = read_conjecture_library(&path).unwrap();
        assert_eq!(
            before.get(&node).copied(),
            Some(ConjectureLifecycleState::Open)
        );
        let prior = fs::read(&path).expect("library bytes before refute");

        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "author retired this line of inquiry"}),
        ));
        assert_eq!(resp["ok"], true, "the withdrawal must commit: {resp}");
        assert_eq!(resp["conjecture"], node);
        assert_eq!(resp["lifecycle"], "withdrawn");
        assert_eq!(resp["transaction"]["committed"], true);
        assert_eq!(resp["transaction"]["succeeded"], true);
        // T1 (G7): the compensating withdrawal carries its own grounded RDF projection under
        // the SAME `judgment_nquads` key — the target node re-marked ConjectureWithdrawn.
        assert!(
            resp["judgment_nquads"]
                .as_str()
                .is_some_and(|s| s.contains("ConjectureWithdrawn")),
            "refute_conjecture must carry judgment_nquads naming ConjectureWithdrawn: {resp}"
        );

        // The EFFECTIVE lifecycle (segment order) is now Withdrawn.
        let after = read_conjecture_library(&path).unwrap();
        assert_eq!(
            after.get(&node).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the effective lifecycle after refute must be Withdrawn"
        );

        // Append-only: the file GREW and the prior bytes are an untouched prefix.
        let now = fs::read(&path).expect("library bytes after refute");
        assert!(
            now.len() > prior.len(),
            "the withdrawal must append new bytes"
        );
        assert_eq!(
            &now[..prior.len()],
            &prior[..],
            "prior segment bytes must be byte-for-byte intact (append-only)"
        );

        // The author reason literal round-trips out of the unioned library.
        let dataset = read_conjectures(&path);
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(node.clone())
                    && q.predicate == format!("{LOGIC_NS}withdrawalReason")
                    && matches!(
                        q.object,
                        RdfTerm::Literal(ref lit)
                            if lit.lexical_form == "author retired this line of inquiry"
                    )
            }),
            "the author withdrawal reason must be recoverable from the library"
        );
        // The withdrawal is reviewer-asserted, never engine-produced.
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(node.clone())
                    && q.predicate == format!("{LOGIC_NS}verdictProvenance")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}VerdictReviewerAsserted"))
            }),
            "the withdrawal must carry VerdictReviewerAsserted provenance"
        );
    }

    #[test]
    fn refute_conjecture_second_withdraw_rejected_by_segment_order() {
        // store -> withdraw -> withdraw again the SAME node: the second refute is rejected as
        // precondition-unmet because the EFFECTIVE state is already Withdrawn, decided by
        // SEGMENT ORDER (the last lifecycle assertion), not by the union or gmeow:atTime. The
        // rejected call appends NOTHING (the library stays byte-for-byte identical).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let node = store_one_conjecture(&server, "B");
        let first = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "first withdrawal"}),
        ));
        assert_eq!(
            first["ok"], true,
            "the first withdrawal must commit: {first}"
        );
        let after_first = fs::read(&path).expect("library bytes after first withdrawal");

        let second = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "second withdrawal"}),
        ));
        assert_eq!(
            second["ok"], false,
            "a second withdrawal must be rejected: {second}"
        );
        assert_eq!(second["transaction"]["committed"], true);
        assert_eq!(second["transaction"]["succeeded"], false);
        assert!(
            second["error"]
                .as_str()
                .is_some_and(|e| e.contains("already withdrawn")),
            "the rejection must name the already-withdrawn precondition: {second}"
        );
        // The rejected call wrote nothing.
        let after_second = fs::read(&path).expect("library bytes after rejected withdrawal");
        assert_eq!(
            after_first, after_second,
            "a rejected withdrawal must append nothing to the library"
        );
    }

    #[test]
    fn refute_conjecture_unknown_id_rejected() {
        // An unknown conjecture_id (nothing stored) fails the TR gate (empty start state) and
        // returns ok:false before any write — the library file is never created.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": "https://blackcatinformatics.ca/gmeow/graph/conjecture/ghost"}),
        ));
        assert_eq!(resp["ok"], false, "an unknown id must be rejected: {resp}");
        assert!(
            resp["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown conjecture id")),
            "the rejection must name the unknown id: {resp}"
        );
        assert!(
            !path.exists(),
            "a rejected withdrawal of an unknown id must write nothing"
        );
    }

    #[test]
    fn refute_conjecture_dry_run_writes_nothing() {
        // dry_run=true witnesses the hypothetical commit (lifecycle withdrawn, committed:false)
        // but appends NOTHING — the library file stays byte-unchanged.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let node = store_one_conjecture(&server, "B");
        let before = fs::read(&path).expect("library bytes before dry run");

        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "sandbox", "dry_run": true}),
        ));
        assert_eq!(resp["ok"], true, "the dry run must succeed: {resp}");
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["lifecycle"], "withdrawn");
        assert_eq!(resp["transaction"]["committed"], false);
        // T1 (G7): the hypothetical withdrawal still carries judgment_nquads — the RDF
        // projection is pure and side-effect-free, so witnessing it costs nothing written.
        assert!(
            resp["judgment_nquads"]
                .as_str()
                .is_some_and(|s| s.contains("ConjectureWithdrawn")),
            "refute_conjecture dry-run must carry judgment_nquads naming ConjectureWithdrawn: \
             {resp}"
        );

        // Nothing appended: bytes unchanged AND the effective lifecycle is still Open.
        let after = fs::read(&path).expect("library bytes after dry run");
        assert_eq!(before, after, "a dry run must write nothing to the library");
        let library = read_conjecture_library(&path).unwrap();
        assert_eq!(
            library.get(&node).copied(),
            Some(ConjectureLifecycleState::Open),
            "a dry run must not change the effective lifecycle"
        );
    }

    #[test]
    fn store_conjecture_and_refute_round_trip_keep_library_and_audit_consistent() {
        // G9 (CodeRabbit Major): store_conjecture and refute_conjecture must each land their
        // library segment AND their audit segment TOGETHER — never one without the other. Drive
        // the real `call_tool_result` surface for both tools and, after EACH commit, assert the
        // library dataset carries BOTH the verdict/withdrawal triples AND the matching
        // `logic:instantiatesSchema` audit marker for that same call — round-tripping cleanly.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let node = store_one_conjecture(&server, "B");
        let after_store = read_conjectures(&path);
        assert!(
            conjecture_nodes(&after_store).contains(&node),
            "the store's library segment must be present after the commit"
        );
        assert!(
            after_store.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
            }),
            "the store's audit segment must be present in the SAME commit as its library \
             segment (no library-without-audit gap): {after_store:?}"
        );

        let refute = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "round-trip proof"}),
        ));
        assert_eq!(refute["ok"], true, "the withdrawal must commit: {refute}");

        let after_refute = read_conjectures(&path);
        let library = read_conjecture_library(&path).unwrap();
        assert_eq!(
            library.get(&node).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the round-trip's effective state must be Withdrawn"
        );
        assert!(
            after_refute.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_WITHDRAW_CONJECTURE_SCHEMA)
            }),
            "the refute's audit segment must be present in the SAME commit as its withdrawal \
             segment: {after_refute:?}"
        );
        // The store's audit marker is STILL there too — append-only, nothing overwritten.
        assert!(
            after_refute.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
            }),
            "the store's audit marker must survive the later refute commit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn conjecture_persist_is_all_or_nothing_on_a_failed_commit() {
        // G9 (CodeRabbit Major): forcing the atomic-replace write to fail PARTWAY (the temp file
        // for the combined library+audit bytes can't even be created) must leave the library
        // BYTE-UNCHANGED — never holding the library segment without its audit segment (or vice
        // versa), because both are assembled in memory and committed via ONE rename. This
        // simulates the "audit append fails after the library append already landed" half of the
        // gap: with the fix, there is no such partial state to observe.
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        // Seed the library with one already-committed segment (as if a prior store succeeded).
        let seed_node = "https://blackcatinformatics.ca/gmeow/graph/conjecture/seed";
        let seed_nt = format!(
            "<{seed_node}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n\
             <{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureOpen> .\n"
        );
        write_conjecture_segment(&path, &seed_nt).unwrap();
        let before = fs::read(&path).expect("seeded library bytes");
        assert!(!before.is_empty());

        // Make the library's directory read-only: the existing `.lock` sidecar can still be
        // opened (it already exists), but `append_conjecture_segments` can no longer create the
        // same-directory temp file its atomic rename depends on — an I/O failure squarely inside
        // the combined library+audit commit, after both segments' bytes are already built.
        let dir = path
            .parent()
            .expect("library has a parent dir")
            .to_path_buf();
        let original_mode = fs::metadata(&dir).unwrap().permissions().mode();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod read-only");

        let outcome = (|| -> gmeow_errors::Result<()> {
            let lib_segment = build_conjecture_verdict_segment(&format!(
                "<{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureWithdrawn> .\n"
            ))?;
            let audit_segment = build_audit_segment(
                "urn:gmeow:conjecture-call:simulated-failure",
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &[MCP_CONJECTURE_IN_LIBRARY],
                "1970-01-01T00:00:00Z",
            );
            with_conjecture_lock(&path, || {
                append_conjecture_segments(&path, &[lib_segment, audit_segment])
            })
        })();

        // Restore permissions unconditionally before asserting, so the tempdir can still clean
        // itself up even if an assertion below panics.
        fs::set_permissions(&dir, fs::Permissions::from_mode(original_mode))
            .expect("chmod restore");

        assert!(
            outcome.is_err(),
            "the forced I/O failure must surface as an error, not a silent partial write"
        );
        let after = fs::read(&path).expect("library bytes after the failed commit");
        assert_eq!(
            before, after,
            "a failed combined library+audit commit must leave the library BYTE-UNCHANGED \
             (all-or-nothing) — never holding one segment without the other"
        );
        let library = read_conjecture_library(&path).unwrap();
        assert_eq!(
            library.get(seed_node).copied(),
            Some(ConjectureLifecycleState::Open),
            "the seed's state must still be Open — the failed withdrawal must not have applied"
        );
    }

    #[test]
    fn conjecture_lock_serializes_concurrent_writers() {
        // G9 (CodeRabbit Major): `with_conjecture_lock` must provide REAL mutual exclusion, not
        // just an in-process convention a second caller could sidestep — so this test opens the
        // SAME sidecar `.lock` file from two INDEPENDENT `std::fs::File` descriptors (one per
        // thread), mirroring how two separate OS processes would each open it themselves. Since
        // `flock` locks are scoped to the OPEN FILE DESCRIPTION (not the process), two distinct
        // descriptors contending on the same path exercise the identical kernel mechanism that
        // serializes real cross-process `store_conjecture` / `refute_conjecture` callers.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder_path = path.clone();
        let holder = std::thread::spawn(move || {
            with_conjecture_lock(&holder_path, || {
                started_tx.send(()).expect("signal lock acquired");
                release_rx.recv().expect("wait for release signal");
                Ok(())
            })
            .expect("holder must acquire and release cleanly")
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the holder thread must acquire the lock");

        let waiter_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waiter_done_writer = waiter_done.clone();
        let waiter_path = path.clone();
        let waiter = std::thread::spawn(move || {
            with_conjecture_lock(&waiter_path, || {
                waiter_done_writer.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .expect("waiter must eventually acquire and release cleanly")
        });

        // While the holder still owns the lock, the waiter's OWN, independently-opened file
        // descriptor must be blocked — proving this is a real `flock`, not an in-process no-op.
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(
            !waiter_done.load(std::sync::atomic::Ordering::SeqCst),
            "a second, independently-opened lock attempt must still be blocked while the first \
             holder is inside its critical section"
        );

        release_tx.send(()).expect("release the holder");
        holder.join().expect("holder thread must not panic");
        waiter.join().expect("waiter thread must not panic");
        assert!(
            waiter_done.load(std::sync::atomic::Ordering::SeqCst),
            "the second lock attempt must complete once the first holder releases"
        );
    }

    #[test]
    fn read_conjecture_library_resolves_effective_state_by_segment_order() {
        // The reader resolves the effective lifecycle purely by SEGMENT ORDER (last writer
        // wins) — NOT by the union (which would carry both states at once) and NOT by
        // gmeow:atTime (every segment shares the fixed determinism epoch). Two nodes are
        // written with OPPOSITE last-writer states to prove position alone decides: the state
        // that "sounds terminal" (Withdrawn) does NOT win unless it is written LAST.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        let base = "https://blackcatinformatics.ca/gmeow/graph/conjecture/";
        let reopened = format!("{base}reopened");
        let retired = format!("{base}retired");
        let ty = format!("<{reopened}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
        let ty2 = format!("<{retired}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
        let lc = |node: &str, state: &str| {
            format!("<{node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}{state}> .\n")
        };

        // `reopened`: Withdrawn FIRST, then Open — Open is last, so Open must win.
        write_conjecture_segment(
            &path,
            &format!("{ty}{}", lc(&reopened, "ConjectureWithdrawn")),
        )
        .unwrap();
        // `retired`: Open FIRST, then Withdrawn — Withdrawn is last, so Withdrawn must win.
        write_conjecture_segment(&path, &format!("{ty2}{}", lc(&retired, "ConjectureOpen")))
            .unwrap();
        write_conjecture_segment(&path, &lc(&reopened, "ConjectureOpen")).unwrap();
        write_conjecture_segment(&path, &lc(&retired, "ConjectureWithdrawn")).unwrap();

        let library = read_conjecture_library(&path).unwrap();
        assert_eq!(
            library.get(&reopened).copied(),
            Some(ConjectureLifecycleState::Open),
            "the LAST segment (Open) must win even though a prior segment said Withdrawn"
        );
        assert_eq!(
            library.get(&retired).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the LAST segment (Withdrawn) must win over the prior Open"
        );
    }

    /// A candidate authored as a REIFIED GROUND binary atom — the exact `logic:relation` /
    /// `logic:argument` reification every authored formula uses, but VARIABLE-FREE: the ground
    /// fact `ex:a rdf:type ex:<cls_local>`. The reconstructed `Formula::Atom` is trivially-Horn,
    /// so it once panicked `LogicProgram::with_formulas` during the candidate parse.
    fn reified_ground_atom_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:phi a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB that ASSERTS the ground fact the reified-ground-atom candidate names, so `KB ⊨ φ`.
    fn ground_atom_entailing_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a rdf:type ex:{cls_local} .\n"
        )
    }

    #[test]
    fn parse_candidate_reified_ground_atom_lifts_not_panics() {
        // F2 regression: a REIFIED GROUND binary atom is trivially-Horn, so the front-end must
        // route it to `LogicProgram.axioms` (not `with_formulas`, which hard-asserts) and
        // `parse_candidate_formula` must reconstruct it — cleanly, never a panic and never a
        // false "0 formula(s) and 0 axiom(s)" rejection.
        let candidate = parse_candidate_formula(&reified_ground_atom_candidate("B"))
            .expect("reified ground atom must lift to a candidate formula");
        match candidate {
            Formula::Atom { relation, args } => {
                assert_eq!(relation, IrTerm::Iri(RDF_TYPE_IRI.to_owned()));
                assert_eq!(
                    args,
                    vec![
                        IrTerm::Iri("http://ex/a".to_owned()),
                        IrTerm::Iri("http://ex/B".to_owned()),
                    ]
                );
            }
            other => panic!("expected a ground binary atom, got {other:?}"),
        }
    }

    #[test]
    fn conjecture_test_reified_ground_atom_evaluates_via_shipped_core() {
        // F2 regression on the SHIPPED surface: driving `run_conjecture_test` (the shared core
        // behind the CLI + MCP tool) with a reified ground-atom candidate must EVALUATE it (a KB
        // asserting the fact entails φ ⇒ corroborated) rather than panic (exit 101).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, _path) = temp_conjecture();

        let out = run_conjecture_test(&ConjectureRunInput {
            formula_ttl: &reified_ground_atom_candidate("B"),
            kb_ttl: &ground_atom_entailing_kb("B"),
            standpoint: "http://ex/standpoint/alice",
            math_conjecture: None,
            dry_run: true,
            max_steps: None,
            max_answers: None,
        })
        .expect("a reified ground-atom conjecture must evaluate, not panic");
        assert_eq!(out.lifecycle, "corroborated");
        assert_eq!(out.information, "supported");
        assert_eq!(out.evaluation, "completed");
    }

    /// A KB whose `ex:trigger` fires the `∀`-Horn candidate on SEVERAL individuals, so the
    /// candidate's derived (non-EDB) closure is strictly larger than a `max_steps`/`max_answers`
    /// bound of 1 — the isolated scenario evaluation exceeds the ceiling.
    fn multi_trigger_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:b ex:trigger ex:mark .\n\
             ex:c ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash, just several derived facts.\n"
        )
    }

    #[test]
    fn conjecture_test_budget_bound_forces_open_via_the_mcp_surface() {
        // GAP G1: the `max_steps` / `max_answers` bound is reachable from the SHIPPED MCP
        // surface (not just the logic-crate unit test). A run whose derived closure exceeds the
        // ceiling is truncated → evaluation budget-exhausted → lifecycle open → discharge
        // Unknown. This is a PURE assertion on `conjecture_test`: no persist tail exists on
        // this surface at all, so the library stays absent throughout.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // Unbounded control: the same candidate/KB runs to Completed (a non-budget verdict).
        let unbounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(unbounded["ok"], true);
        assert_ne!(
            unbounded["verdict"]["evaluation"], "budget-exhausted",
            "the unbounded control must not trip the ceiling: {unbounded}"
        );

        // Bounded: a ceiling of 1 truncates the multi-fact derived closure.
        let bounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "bounded run must compute a verdict: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["evaluation"], "budget-exhausted",
            "exceeding the ceiling must stamp BudgetExhausted: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "a budget-exhausted run is inconclusive → Open: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["discharge"], "ObligationUnknown",
            "a budget-exhausted run carries the obligation forward as Unknown: {bounded}"
        );

        // `max_answers` is the equivalent binding-count ceiling and trips the same way.
        let bounded_answers = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_answers": 1,
            }),
        ));
        assert_eq!(
            bounded_answers["verdict"]["evaluation"], "budget-exhausted",
            "max_answers must impose the same ceiling: {bounded_answers}"
        );
        // `conjecture_test` is PURE: none of these calls ever touch the library.
        assert!(
            !path.exists(),
            "conjecture_test calls must write nothing to the library"
        );
    }

    #[test]
    fn store_conjecture_budget_bound_forces_open_and_still_persists() {
        // R3b acceptance: a budget-exhausted `store_conjecture` (committing) must still yield
        // the non-conclusive verdict — lifecycle open, discharge Unknown — via the governor,
        // NEVER a false discharge, even though the run commits and appends to the library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        assert!(!path.exists());
        let bounded = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "a budget-exhausted commit must still compute+persist a verdict: {bounded}"
        );
        assert_eq!(bounded["verdict"]["evaluation"], "budget-exhausted");
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "never a false discharge: budget-exhausted must stay Open: {bounded}"
        );
        assert_eq!(bounded["verdict"]["discharge"], "ObligationUnknown");
        assert_eq!(bounded["transaction"]["committed"], true);

        // The non-conclusive verdict is still a real, committed, append-only segment.
        assert!(
            path.exists() && fs::metadata(&path).unwrap().len() > 0,
            "a committed budget-exhausted run must still append its Open verdict"
        );
        let node = bounded["conjecture"]
            .as_str()
            .expect("node iri")
            .to_string();
        assert!(conjecture_nodes(read_conjectures(&path).as_ref()).contains(&node));
    }

    #[test]
    fn store_conjecture_persists_are_append_only() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let first = fs::read(&path).expect("first library bytes");
        let first_len = first.len();
        assert!(first_len > 0);

        // A DISTINCT conjecture (different head class) appends a second segment.
        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let second = fs::read(&path).expect("second library bytes");
        assert!(
            second.len() > first_len,
            "the second persist must APPEND, growing the file"
        );
        assert_eq!(
            &second[..first_len],
            &first[..],
            "the first segment's bytes must be intact (append-only, never mutated)"
        );
        // Both conjectures are readable back.
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn store_conjecture_library_is_isolated_from_the_base_kb() {
        // R2: the caller's KB text is unchanged by the call, and the library is a DISTINCT
        // file the reasoner never folds into its base graph.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, mem_path) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let kb = refuting_kb("B");
        let kb_before = kb.clone();
        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": kb,
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        // The KB argument the tool was given is an owned String — the tool cannot mutate the
        // caller's copy. (Isolation is inherent: store_conjecture borrows and copies the KB.)
        assert_eq!(kb_before, refuting_kb("B"));
        // The conjecture library is a distinct file, NOT the memory store, and never the base
        // reasoning graph (reason reads graph_dataset(), never conjecture_path()).
        assert!(path.exists());
        assert_ne!(path, mem_path);
        // The bundled reasoning surface is unaffected — reason still runs cleanly over the
        // untouched base graph (the library is never unioned in).
        let reasoned = text_payload(server.call_tool_result("reason", &json!({})));
        // `reason` is dev-only; over a Consumer server it returns an error, proving the base
        // reasoning path does not consult the library either way. What matters for R2 is that
        // the library file and the KB are untouched, asserted above.
        let _ = reasoned;
    }

    #[test]
    fn same_formula_two_standpoints_mints_two_nodes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let a = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let b = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/bob",
            }),
        ));
        assert_ne!(
            a["conjecture"], b["conjecture"],
            "the same formula in two standpoints must mint two DISTINCT nodes (P9)"
        );
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn conjecture_library_corpus_query_recovers_open_and_refuted() {
        // A corpus competency: persist several DISTINCT conjectures (open + refuted) in one
        // standpoint, then scan the collection for all of them, with witness premises for the
        // refuted ones recoverable.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // Two refuted (B, D disjoint) and two open (C, E unrelated).
        for cls in ["B", "D"] {
            let r = text_payload(server.call_tool_result(
                "store_conjecture",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": refuting_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "refuted-in-standpoint", "{r}");
        }
        for cls in ["C", "E"] {
            let r = text_payload(server.call_tool_result(
                "store_conjecture",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": open_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "open", "{r}");
        }

        let dataset = read_conjectures(&path);
        // All four conjectures are recoverable.
        assert_eq!(conjecture_nodes(&dataset).len(), 4);
        // All are scoped to the team standpoint.
        let team_scoped = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                    && q.object == RdfTerm::iri("http://ex/standpoint/team")
            })
            .count();
        assert_eq!(team_scoped, 4, "every conjecture is standpoint-scoped");
        // The two refuted conjectures each carry a recoverable individual + premises.
        let refuted = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureLifecycleState")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}ConjectureRefutedInStandpoint"))
            })
            .count();
        assert_eq!(refuted, 2, "exactly the two disjointness cases are refuted");
        assert!(
            witness_premises(&dataset).len() >= 2,
            "each refutation persists recoverable witness premises"
        );
    }

    /// `counter_examples` over the live bundle: a term documenting fixtures yields
    /// the real, split fixture bodies; a resolvable term documenting none yields the
    /// honest empty-but-ok shape; an unknown term is a hard error envelope.
    ///
    /// `gmeow:Activity` documents BOTH a well-formed exemplar and a counter-example
    /// in the shipped `gmeow:graph/documentation` graph (verified by the projection
    /// query in Task 1); `gmeow:AboutnessMode` is a documented term that authors no
    /// fixtures.
    #[test]
    fn tool_counter_examples_surface() {
        let server = consumer_server();

        // A term with fixtures → real, non-empty split bodies.
        let hit = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
        );
        assert_eq!(hit["ok"], true);
        assert_eq!(hit["term"], "gmeow:Activity");
        let counter = hit["counter_examples"]
            .as_array()
            .expect("counter_examples is an array");
        let wellformed = hit["wellformed"]
            .as_array()
            .expect("wellformed is an array");
        assert!(
            !counter.is_empty(),
            "gmeow:Activity documents at least one counter-example: {hit}"
        );
        assert!(
            !wellformed.is_empty(),
            "gmeow:Activity documents at least one well-formed exemplar: {hit}"
        );
        // The counter-example carries a real Turtle body AND a violation code.
        let ce = &counter[0];
        assert!(
            ce["text"].as_str().is_some_and(|t| t.contains(':')),
            "counter-example carries a real Turtle body: {ce}"
        );
        assert!(
            ce["violation_code"].as_str().is_some_and(|c| !c.is_empty()),
            "counter-example carries an authored violation code: {ce}"
        );
        // The well-formed exemplar has a real body and NO violation code.
        let wf = &wellformed[0];
        assert!(
            wf["text"].as_str().is_some_and(|t| t.contains(':')),
            "well-formed exemplar carries a real Turtle body: {wf}"
        );
        assert!(
            wf["violation_code"].is_null(),
            "well-formed exemplar has no violation code: {wf}"
        );
        // Deterministic: a second call is byte-identical.
        let again = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
        );
        assert_eq!(hit, again, "counter_examples output is deterministic");

        // A resolvable term with NO fixtures → empty-but-ok (NOT an error).
        let empty = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:AboutnessMode"})),
        );
        assert_eq!(
            empty["ok"], true,
            "no-fixture term is empty-but-ok: {empty}"
        );
        assert_eq!(empty["wellformed"], json!([]));
        assert_eq!(empty["counter_examples"], json!([]));

        // An unknown term → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "counter_examples",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
        assert!(
            unknown["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown term")),
            "unknown-term error names the failure: {unknown}"
        );
    }

    /// `entailments` over the live bundle: a term with derivations yields every
    /// entailment's rule/conclusion with its premises preserved; an unknown term is a
    /// hard error. `gmeow:Entity` grounds >1000 entailment records in the shipped
    /// documentation graph.
    #[test]
    fn tool_entailments_surface() {
        let server = consumer_server();

        let hit =
            text_payload(server.call_tool_result("entailments", &json!({"term": "gmeow:Entity"})));
        assert_eq!(hit["ok"], true);
        assert_eq!(hit["term"], "gmeow:Entity");
        let entailments = hit["entailments"]
            .as_array()
            .expect("entailments is an array");
        assert!(
            !entailments.is_empty(),
            "gmeow:Entity grounds at least one entailment: {}",
            &hit.to_string()[..hit.to_string().len().min(400)]
        );
        // Every record carries a non-empty rule and conclusion.
        for e in entailments {
            assert!(
                e["rule"].as_str().is_some_and(|r| !r.is_empty()),
                "entailment carries a rule: {e}"
            );
            assert!(
                e["conclusion"].as_str().is_some_and(|c| !c.is_empty()),
                "entailment carries a conclusion: {e}"
            );
            assert!(e["premises"].is_array(), "premises is an array: {e}");
        }
        // Premises are preserved: at least one derivation carries premises.
        let total_premises: usize = entailments
            .iter()
            .map(|e| e["premises"].as_array().map_or(0, Vec::len))
            .sum();
        assert!(
            total_premises > 0,
            "at least one gmeow:Entity entailment preserves its premises"
        );

        // Unknown term → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "entailments",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
    }

    /// Select the RICHEST-surface term in the shipped bundle for the tier tests:
    /// the term (a) grounding at least one reasoner entailment AND (b) documenting at
    /// least one conformance fixture, maximizing the total panel count (entailments +
    /// fixtures), tie-broken by IRI so the choice is deterministic. Panics (a real
    /// blocker, never a soft skip) if the bundle documents no such term.
    ///
    /// Uses exactly TWO bulk queries (the entailment map + a single fixture-by-term
    /// scan) and intersects them — never a per-term query per candidate.
    fn richest_card_term(server: &McpServer) -> String {
        let entailments = server
            .view
            .entailment_map()
            .expect("entailment map from the shipped documentation graph");
        // One bulk scan: fixture count per documented term.
        let fixtures_query = format!(
            "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
        );
        let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
        for row in server
            .view
            .docs_select_rows(&fixtures_query)
            .expect("fixture-by-term scan over graph/documentation")
        {
            if let Some(term) = row.get("term") {
                *fixtures_per_term.entry(term.clone()).or_default() += 1;
            }
        }
        let mut candidates: Vec<(usize, String)> = entailments
            .iter()
            .filter_map(|(iri, ents)| {
                fixtures_per_term
                    .get(iri)
                    .map(|&fx| (ents.len() + fx, iri.clone()))
            })
            .collect();
        // Most panels first, then lexicographically-first IRI (deterministic).
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .next()
            .map(|(_, iri)| iri)
            .expect("the shipped bundle documents a term with entailments AND fixtures")
    }

    /// A documented term that still carries full-tier panels (≥1 entailment AND ≥1
    /// fixture) but the FEWEST of them — the cheapest term whose `full` card exercises
    /// every rich-panel path. Tier/determinism assertions that render the full card
    /// several times use this instead of [`richest_card_term`] (whose ~1000-entailment
    /// surface is only needed for the byte-ceiling proof), so they render fast.
    fn modest_panel_card_term(server: &McpServer) -> String {
        let entailments = server
            .view
            .entailment_map()
            .expect("entailment map from the shipped documentation graph");
        let fixtures_query = format!(
            "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
        );
        let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
        for row in server
            .view
            .docs_select_rows(&fixtures_query)
            .expect("fixture-by-term scan over graph/documentation")
        {
            if let Some(term) = row.get("term") {
                *fixtures_per_term.entry(term.clone()).or_default() += 1;
            }
        }
        let mut candidates: Vec<(usize, String)> = entailments
            .iter()
            .filter_map(|(iri, ents)| {
                fixtures_per_term
                    .get(iri)
                    .map(|&fx| (ents.len() + fx, iri.clone()))
            })
            .collect();
        // FEWEST panels first, then lexicographically-first IRI (deterministic).
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .next()
            .map(|(_, iri)| iri)
            .expect("the shipped bundle documents a term with entailments AND fixtures")
    }

    /// `doc_card` tiers: `summary` is the leanest surface (title + definition only,
    /// under a pinned byte ceiling, none of the advisory / panel sections); `full`
    /// is strictly larger and carries the rich oracle panels (Entailments / Do /
    /// Don't headers).
    #[test]
    fn tool_doc_card_tier_byte_ceiling() {
        const SUMMARY_CEILING: usize = 1500;
        let server = consumer_server();
        let term = richest_card_term(&server);

        let summary = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "summary"})),
        );
        let full = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})),
        );
        let s_card = summary["card"].as_str().expect("summary markdown card");
        let f_card = full["card"].as_str().expect("full markdown card");

        // Summary is title + definition ONLY, under the ceiling.
        assert!(
            s_card.len() < SUMMARY_CEILING,
            "summary card ({} bytes) must be under {SUMMARY_CEILING}: {s_card}",
            s_card.len()
        );
        assert!(
            s_card.starts_with("# "),
            "summary carries the H1 title: {s_card}"
        );
        let summary_body = s_card
            .strip_prefix("# ")
            .and_then(|s| s.split_once("\n\n"))
            .map_or("", |(_, rest)| rest);
        assert!(
            !summary_body.trim().is_empty(),
            "summary carries a definition after the title: {s_card}"
        );
        // NONE of the advisory / metadata / panel surface at the summary tier.
        assert!(
            !s_card.contains("- category:"),
            "no metadata header: {s_card}"
        );
        assert!(
            !s_card.contains("**Use when:**"),
            "no advisory fields: {s_card}"
        );
        assert!(
            !s_card.contains("## Entailments"),
            "no rich panels: {s_card}"
        );

        // Full is strictly larger and carries the panel section headers.
        assert!(
            f_card.len() > s_card.len(),
            "full ({}) must exceed summary ({})",
            f_card.len(),
            s_card.len()
        );
        assert!(
            f_card.contains("## Entailments"),
            "full carries Entailments: {f_card}"
        );
        assert!(
            f_card.contains("## Do") || f_card.contains("## Don't"),
            "full carries a Do / Don't fixture panel: {f_card}"
        );
    }

    /// Single-renderer authority: `doc_card` at `detail=standard` is BYTE-IDENTICAL
    /// to rendering the shared compact `Card` through `render_card` at `Standard` —
    /// the tier gating never perturbs the docs-site card the standard tier mirrors.
    #[test]
    fn tool_doc_card_standard_is_byte_identical_to_compact_render() {
        let server = consumer_server();
        let term = "gmeow:EntityExistence";

        let envelope = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "standard"})),
        );
        let card_md = envelope["card"].as_str().expect("standard markdown card");

        // Independent expected: the SAME shared builder + renderer at Standard.
        let requested = server.startup_requested.clone();
        let modeled_defs = server.view.modeled_defs();
        let expected = server.view.with_terms(requested, |terms| {
            let (title, card) = export::doc_card_build(terms, term, &modeled_defs)
                .resolved()
                .expect("known term resolves");
            gmeow_docs::card::render_card(&title, &card, gmeow_docs::card::CardDetail::Standard)
        });
        assert_eq!(
            card_md, expected,
            "standard tier must be byte-identical to the compact single-renderer output"
        );
        // Default (no `detail`) is Standard.
        let defaulted = text_payload(server.call_tool_result("doc_card", &json!({"term": term})));
        assert_eq!(defaulted["detail"], "standard");
        assert_eq!(defaulted["card"].as_str().expect("card"), expected);
    }

    /// `doc_card` `format=json`: byte-stable across calls; standard-tier JSON omits
    /// the full-tier rich fields; full-tier JSON carries them.
    #[test]
    fn tool_doc_card_json_determinism_and_tier_fields() {
        let server = consumer_server();
        let docs_a = server.view.documentation();
        let docs_b = server.view.documentation();
        assert!(
            Arc::ptr_eq(docs_a, docs_b),
            "documentation queries must share one projected graph"
        );
        // Determinism + tier-field presence hold for ANY term that carries panels,
        // so use the cheapest such term (not the ~1000-entailment richest term) —
        // this test renders the full card three times, so a lean term keeps it fast.
        let term = modest_panel_card_term(&server);

        // Byte-identical raw tool text across two identical calls.
        let call = || {
            server.call_tool_result(
                "doc_card",
                &json!({"term": term, "detail": "full", "format": "json"}),
            )["content"][0]["text"]
                .as_str()
                .expect("json tool text")
                .to_string()
        };
        assert_eq!(call(), call(), "json card is byte-stable across calls");

        // Standard-tier JSON: a Card object WITHOUT the full-tier rich fields.
        let std_json = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "standard", "format": "json"}),
        ));
        assert_eq!(std_json["format"], "json");
        let std_card = &std_json["card"];
        assert!(std_card.is_object(), "json card is an object: {std_card}");
        assert!(
            std_card.get("entailments").is_none(),
            "no entailments at standard"
        );
        assert!(
            std_card.get("fixtures_do").is_none(),
            "no fixtures_do at standard"
        );
        assert!(
            std_card.get("fixtures_dont").is_none(),
            "no fixtures_dont at standard"
        );
        assert!(
            std_card.get("diagnostics").is_none(),
            "no diagnostics at standard"
        );
        assert!(std_card.get("loss").is_none(), "no loss at standard");

        // Full-tier JSON DOES carry the rich panels.
        let full_json = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "full", "format": "json"}),
        ));
        assert!(
            full_json["card"].get("entailments").is_some(),
            "full json carries entailments: {}",
            &full_json.to_string()[..full_json.to_string().len().min(400)]
        );
    }

    /// `doc_card` cost metadata: every envelope carries positive `bytes`/`tokens`,
    /// monotonically non-decreasing across summary ≤ standard ≤ full for one term.
    #[test]
    fn tool_doc_card_cost_metadata_is_monotone() {
        let server = consumer_server();
        let term = richest_card_term(&server);

        let tier = |detail: &str| {
            text_payload(
                server.call_tool_result("doc_card", &json!({"term": term, "detail": detail})),
            )
        };
        let summary = tier("summary");
        let standard = tier("standard");
        let full = tier("full");

        for env in [&summary, &standard, &full] {
            assert!(
                env["bytes"].as_u64().expect("bytes") > 0,
                "bytes > 0: {env}"
            );
            assert!(
                env["tokens"].as_u64().expect("tokens") > 0,
                "tokens > 0: {env}"
            );
        }
        let bytes = |e: &Value| e["bytes"].as_u64().unwrap();
        let tokens = |e: &Value| e["tokens"].as_u64().unwrap();
        assert!(
            bytes(&summary) <= bytes(&standard),
            "bytes summary ≤ standard"
        );
        assert!(bytes(&standard) <= bytes(&full), "bytes standard ≤ full");
        assert!(
            tokens(&summary) <= tokens(&standard),
            "tokens summary ≤ standard"
        );
        assert!(tokens(&standard) <= tokens(&full), "tokens standard ≤ full");

        // Unknown detail / format is a hard error listing the valid values.
        let bad_detail = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "verbose"})),
        );
        assert_eq!(
            bad_detail["ok"], false,
            "unknown detail hard-fails: {bad_detail}"
        );
        let bad_format = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "format": "yaml"})),
        );
        assert_eq!(
            bad_format["ok"], false,
            "unknown format hard-fails: {bad_format}"
        );
    }

    /// `doc_card` full tier populates the rich panels FROM the documentation graph:
    /// the full markdown inlines an actual entailment conclusion and a fixture title
    /// the sibling `entailments` / `counter_examples` tools report for the term.
    #[test]
    fn tool_doc_card_full_inlines_graph_panels() {
        let server = consumer_server();
        let term = richest_card_term(&server);

        let full = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})),
        );
        let f_card = full["card"].as_str().expect("full markdown card");

        // An entailment's conclusion (from the SAME graph the `entailments` tool reads).
        let ents = text_payload(server.call_tool_result("entailments", &json!({"term": term})));
        let conclusion = ents["entailments"][0]["conclusion"]
            .as_str()
            .expect("the richest term grounds an entailment with a conclusion");
        assert!(
            f_card.contains(conclusion),
            "full card inlines the entailment conclusion {conclusion:?}"
        );

        // A fixture title (from the SAME graph the `counter_examples` tool reads).
        let fixtures =
            text_payload(server.call_tool_result("counter_examples", &json!({"term": term})));
        let title = fixtures["counter_examples"]
            .get(0)
            .and_then(|f| f["title"].as_str())
            .or_else(|| {
                fixtures["wellformed"]
                    .get(0)
                    .and_then(|f| f["title"].as_str())
            })
            .expect("the richest term documents a fixture with a title");
        assert!(
            f_card.contains(title),
            "full card inlines the fixture title {title:?}"
        );
    }

    /// `competency_questions` over the live bundle: the index form (no `term`) returns
    /// every runnable question, each carrying a `query_text` that PARSES as a valid
    /// SPARQL SELECT; the per-term form returns that term's subset. `gmeow:Agent`
    /// documents competency questions in the shipped documentation graph.
    #[test]
    fn tool_competency_questions_surface() {
        let server = consumer_server();

        // Index form: no `term`.
        let index = text_payload(server.call_tool_result("competency_questions", &json!({})));
        assert_eq!(index["ok"], true);
        assert!(
            index.get("term").is_none(),
            "index form carries no term key: {}",
            &index.to_string()[..index.to_string().len().min(200)]
        );
        let questions = index["questions"]
            .as_array()
            .expect("questions is an array");
        assert!(!questions.is_empty(), "the competency index is non-empty");
        for q in questions {
            assert!(
                q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
                "every competency question carries a runnable query_text: {q}"
            );
        }

        // The first question's query_text round-trips through the native SPARQL
        // parser as an executable SELECT (over an empty dataset — parse+plan only).
        let first_query = questions[0]["query_text"].as_str().expect("query_text");
        let empty = std::sync::Arc::new(
            purrdf::RdfDatasetBuilder::new()
                .freeze()
                .expect("empty dataset"),
        );
        let parsed = crate::stages::native_query::query(&empty, first_query)
            .expect("competency query_text is a valid SPARQL query");
        assert!(
            matches!(parsed, purrdf::SparqlResult::Solutions { .. }),
            "competency query_text is a SPARQL SELECT: {first_query}"
        );

        // Deterministic index.
        let index_again = text_payload(server.call_tool_result("competency_questions", &json!({})));
        assert_eq!(index, index_again, "competency index is deterministic");

        // Per-term form: gmeow:Agent's subset — non-empty, each with a query_text.
        let per_term = text_payload(
            server.call_tool_result("competency_questions", &json!({"term": "gmeow:Agent"})),
        );
        assert_eq!(per_term["ok"], true);
        assert_eq!(per_term["term"], "gmeow:Agent");
        let agent_questions = per_term["questions"]
            .as_array()
            .expect("questions is an array");
        assert!(
            !agent_questions.is_empty(),
            "gmeow:Agent documents at least one competency question: {per_term}"
        );
        for q in agent_questions {
            assert!(
                q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
                "per-term competency question carries a query_text: {q}"
            );
        }
        assert!(
            agent_questions.len() <= questions.len(),
            "a term's competency subset is no larger than the whole index"
        );

        // Unknown term (per-term form) → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "competency_questions",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
    }

    /// A tiny synthetic documentation dataset for the `search_documentation` unit
    /// tests: two class terms whose `to_gmeow_rdf` projection carries the new
    /// `docSearch*` facets, parsed back into an [`purrdf::RdfDataset`] exactly the way
    /// the production carrier holds the bundle's documentation graph. This does NOT
    /// depend on the committed bundle (which only gains the facets after regenerate) —
    /// it exercises the projection + search end to end from the model.
    fn synthetic_docs_dataset() -> Arc<purrdf::RdfDataset> {
        use gmeow_docs::{DocLinkage, DocTerm, DocTermCategory};
        let ns = "https://blackcatinformatics.ca/gmeow/";
        let model = gmeow_docs::DocsModel {
            terms: vec![
                DocTerm {
                    iri: format!("{ns}Cat"),
                    curie: "gmeow:Cat".to_string(),
                    label: Some("Cat".to_string()),
                    definition: Some("A small domesticated feline.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{ns}slice/zoo"),
                    scope_notes: vec![
                        "Prefer for a domestic cat; avoid for a wildcat.".to_string(),
                    ],
                    ..Default::default()
                },
                DocTerm {
                    iri: format!("{ns}Feline"),
                    curie: "gmeow:Feline".to_string(),
                    label: Some("Feline".to_string()),
                    definition: Some("The cat family of mammals.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{ns}slice/zoo"),
                    ..Default::default()
                },
            ],
            // A crosswalk linkage on Cat → the alignment facet token `exactMatch:Q146`.
            linkages: vec![DocLinkage {
                mapping_set: None,
                subject: format!("{ns}Cat"),
                subject_curie: "gmeow:Cat".to_string(),
                predicate: "http://www.w3.org/2004/02/skos/core#exactMatch".to_string(),
                object: "http://www.wikidata.org/entity/Q146".to_string(),
                justification: None,
                confidence: None,
                owner_slice: format!("{ns}slice/zoo"),
            }],
            ..Default::default()
        };
        let nquads = gmeow_docs::to_gmeow_rdf(&model, &BTreeMap::new());
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("to_gmeow_rdf emits valid N-Quads")
    }

    /// `search_documentation` over the synthetic dataset: matches on label /
    /// definition / advice, attaches the advice + alignment + missing-coverage facets,
    /// ranks a label match above a definition match, returns empty for a non-match, and
    /// is deterministic.
    #[test]
    fn search_documentation_matches_facets_ranks_and_is_deterministic() {
        let dataset_arc = synthetic_docs_dataset();
        let dataset =
            Arc::new(dataset_arc.project_named_graph(crate::stages::carrier::GRAPH_DOCUMENTATION));
        let cat_iri = "https://blackcatinformatics.ca/gmeow/Cat";
        let feline_iri = "https://blackcatinformatics.ca/gmeow/Feline";

        // "cat": Cat matches by LABEL (rank 0); Feline matches by DEFINITION ("the cat
        // family …", rank 1) — so Cat sorts first.
        let hits = search_documentation(&dataset, "cat", 20).expect("search ok");
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![cat_iri, feline_iri],
            "label match outranks definition match"
        );
        let cat = &hits[0];
        assert_eq!(cat.kind, "term");
        assert_eq!(cat.label, "Cat");
        assert_eq!(
            cat.definition.as_deref(),
            Some("A small domesticated feline.")
        );
        assert_eq!(
            cat.advice,
            vec!["Prefer for a domestic cat; avoid for a wildcat.".to_string()],
            "the advice facet is attached"
        );
        assert_eq!(
            cat.alignments,
            vec!["exactMatch:Q146".to_string()],
            "the alignment facet is attached"
        );
        assert!(
            !cat.missing_coverage.is_empty(),
            "an under-documented term carries missing-coverage dimensions: {:?}",
            cat.missing_coverage
        );

        // An advice-only match: "wildcat" appears only in Cat's advice prose.
        let advice_hits = search_documentation(&dataset, "wildcat", 20).expect("search ok");
        let advice_ids: Vec<&str> = advice_hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(advice_ids, vec![cat_iri], "advice prose is searchable");

        // A definition-only match: "mammals" appears only in Feline's definition.
        let def_hits = search_documentation(&dataset, "mammals", 20).expect("search ok");
        let def_ids: Vec<&str> = def_hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(def_ids, vec![feline_iri], "definition prose is searchable");

        // A non-matching query is empty-but-ok (never a hard fail on a populated graph).
        let none = search_documentation(&dataset, "xylophone", 20).expect("search ok");
        assert!(none.is_empty(), "a non-matching query returns no hits");

        // Determinism: the same query twice yields the same order.
        let a = search_documentation(&dataset, "cat", 20).expect("search ok");
        let b = search_documentation(&dataset, "cat", 20).expect("search ok");
        assert_eq!(
            a.iter().map(|h| &h.id).collect::<Vec<_>>(),
            b.iter().map(|h| &h.id).collect::<Vec<_>>(),
            "search order is reproducible"
        );

        // The limit is honored.
        let limited = search_documentation(&dataset, "cat", 1).expect("search ok");
        assert_eq!(limited.len(), 1, "limit caps the result count");
    }

    /// `search_documentation` HARD-FAILS when the documentation graph is absent/empty
    /// — docs_search serves the documentation graph, so a missing graph is a defect,
    /// never a silent empty result.
    #[test]
    fn search_documentation_hard_fails_on_absent_documentation_graph() {
        let empty = purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset");
        let err = search_documentation(&empty, "cat", 20)
            .expect_err("an absent documentation graph is a hard fail");
        assert!(
            err.to_string().contains("graph/documentation"),
            "the hard-fail error names the missing documentation graph: {err}"
        );
    }

    /// `docs_search` dispatches over the shipped bundle and returns an OK envelope with
    /// a `results` array. (The committed bundle carries the documentation graph but not
    /// yet the `docSearch*` facets — those land after regenerate — so the live match
    /// set is validated by the synthetic unit test above; here we prove the tool wires
    /// through, never hard-fails on the populated graph, and is deterministic.)
    #[test]
    fn docs_search_tool_dispatches_over_the_bundle() {
        let server = consumer_server();
        let hit = text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
        assert_eq!(
            hit["ok"], true,
            "docs_search returns ok over the bundle: {hit}"
        );
        assert_eq!(hit["query"], "entity");
        assert!(hit["results"].is_array(), "results is an array: {hit}");
        let again =
            text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
        assert_eq!(hit, again, "docs_search output is deterministic");
    }

    // ── coherence_certificate (R6) ─────────────────────────────────────────────────

    /// A consistent native [`ReasoningResult`], with the given evaluation/completeness
    /// axes and optional certified fragment — the minimum a coherence outcome reads.
    fn coherence_result(
        evaluation: EvaluationStatus,
        completeness: CompletenessStatus,
        fragment: Option<&str>,
    ) -> ReasoningResult {
        use gmeow_logic::result::{
            InformationState, InputStatus, PreservationClaim, ResultPayload, ResultProvenance,
        };
        let mut provenance = ResultProvenance::native("contract:abc", "world:default");
        provenance.certified_fragment = fragment.map(str::to_owned);
        ReasoningResult {
            input: InputStatus::Valid,
            evaluation,
            completeness,
            preservation: PreservationClaim::exact(),
            information: InformationState::Supported,
            provenance,
            payload: ResultPayload::Empty,
            row_schema: None,
        }
    }

    /// Parse a `graph/attestations` N-Quads document into a dataset the read helper reads.
    fn dataset_of(nquads: &str) -> Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("parse coherence N-Quads")
    }

    /// The carrier folds a CONCLUSIVE, fragment-scoped, violation-free closure as a real
    /// `logic:CoherenceCertificate`; the read tool surfaces `issues_certificate:true`, the
    /// certificate class, and the pinned bundle_hash / per-graph axiom_hashes VERBATIM (the
    /// tamper surface) — proving the digests are exactly `per_graph_axiom_hashes` and not
    /// fabricated.
    #[test]
    fn coherence_certificate_surfaces_a_certificate_with_the_pinned_digests() {
        use gmeow_logic::certificate::{
            CoherenceOutcome, ContradictionPolicy, per_graph_axiom_hashes,
        };
        use purrdf::gts::writer::digest_string;

        // A small axiom-bearing dataset whose per-graph digests the certificate pins.
        let axioms = dataset_of(
            "<https://e/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://e/B> <https://e/w> .\n",
        );
        let axiom_hashes = per_graph_axiom_hashes(axioms.as_ref(), digest_string);
        assert!(!axiom_hashes.is_empty(), "the fixture pins ≥1 axiom digest");
        let bundle_hash = digest_string(b"bundle-identity-bytes");

        let result = coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("fragment:test"),
        );
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            bundle_hash.clone(),
            axiom_hashes.clone(),
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(outcome.issues_certificate(), "the fixture must certify");

        let dataset = dataset_of(&outcome.to_nquads(crate::stages::release::GRAPH_ATTESTATIONS));
        let env = coherence_certificate_envelope(dataset.as_ref()).expect("certificate present");

        assert_eq!(env["ok"], true);
        assert_eq!(env["issues_certificate"], true);
        assert_eq!(env["is_refused"], false);
        assert_eq!(env["class_local_name"], "CoherenceCertificate");
        // The surfaced bundle_hash / axiom_hashes are the pipeline-pinned digests, VERBATIM.
        assert_eq!(env["bundle_hash"], bundle_hash);
        assert!(
            env["bundle_hash"].as_str().is_some_and(|s| !s.is_empty()),
            "bundle_hash is non-empty: {env}"
        );
        let surfaced: Vec<String> = env["axiom_hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            surfaced,
            axiom_hashes.into_iter().collect::<Vec<_>>(),
            "axiom_hashes are the per_graph_axiom_hashes digests, not fabricated: {env}"
        );
        assert!(
            surfaced.iter().all(|h| h.contains(':') && h.len() > 8),
            "axiom digests are non-trivial content addresses: {surfaced:?}"
        );
        // The two completeness-gate axes round-trip off the linked result node.
        assert_eq!(env["evaluation"], "completed");
        assert_eq!(env["completeness"], "unknown");
        assert_eq!(env["contract_hash"], "contract:abc");
    }

    /// R6 regression: a bounded/incomplete closure yields the strictly-weaker
    /// `logic:CoherenceCheckAttestation`; the read tool must report
    /// `issues_certificate:false` and `class_local_name:CoherenceCheckAttestation` — a
    /// regression must NEVER silently upgrade an attestation to a certificate.
    #[test]
    fn coherence_certificate_maps_an_attestation_never_a_certificate() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let result = coherence_result(
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::Incomplete,
            None,
        );
        assert!(!result.is_conclusive());
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle".to_owned(),
            ["blake3:axioms".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(!outcome.issues_certificate());

        let dataset = dataset_of(&outcome.to_nquads(crate::stages::release::GRAPH_ATTESTATIONS));
        let env = coherence_certificate_envelope(dataset.as_ref()).expect("attestation present");
        assert_eq!(env["ok"], true);
        assert_eq!(
            env["issues_certificate"], false,
            "an attestation is NOT a certificate: {env}"
        );
        assert_eq!(env["class_local_name"], "CoherenceCheckAttestation");
        assert_eq!(env["evaluation"], "budget-exhausted");
        assert_eq!(env["completeness"], "incomplete");
    }

    /// HARD-FAIL: a bundle carrying no coherence artifact in `graph/attestations` is an
    /// error — there is NO silent recompute fallback.
    #[test]
    fn coherence_certificate_hard_fails_on_a_bundle_without_a_certificate() {
        let stripped = dataset_of("<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n");
        let err = coherence_certificate_envelope(stripped.as_ref())
            .expect_err("a bundle with no coherence artifact must hard-fail");
        assert!(
            err.to_string()
                .contains("no coherence certificate or attestation"),
            "the hard fail names the missing artifact: {err}"
        );
    }

    /// A bundle carrying more than one distinct coherence subject is ambiguous and a hard
    /// failure (no silent first-wins).
    #[test]
    fn coherence_certificate_hard_fails_on_an_ambiguous_bundle() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = crate::stages::release::GRAPH_ATTESTATIONS;
        let a = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("frag:a"),
            ),
            "blake3:bundle-a".to_owned(),
            ["blake3:axioms-a".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let b = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("frag:b"),
            ),
            "blake3:bundle-b".to_owned(),
            ["blake3:axioms-b".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let both = format!("{}{}", a.to_nquads(graph), b.to_nquads(graph));
        let err = coherence_certificate_envelope(dataset_of(&both).as_ref())
            .expect_err("two coherence subjects must hard-fail");
        assert!(
            err.to_string()
                .contains("more than one distinct coherence subject"),
            "the hard fail names the ambiguity: {err}"
        );
    }

    /// Drive the REAL `coherence_certificate` tool through `call_tool_result` over a bundle
    /// that carries the certificate in `graph/attestations` — the same disk-free, reason-free
    /// read path the shipped consumer surface uses. Whole-bundle emit/import is slow → off-gate.
    #[test]
    fn coherence_certificate_tool_reads_the_carried_bundle_heavy_offgate() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:test"),
            ),
            "blake3:carried-bundle".to_owned(),
            ["blake3:axioms-carried".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();

        // A minimal snapshot: the required ontology header (the importer hard-fails
        // without it) plus the certificate's graph/attestations named graph, emitted as a
        // real gmeow.gts bundle.
        let doc = format!(
            "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             {}",
            outcome.to_nquads(crate::stages::release::GRAPH_ATTESTATIONS)
        );
        let dataset = dataset_of(&doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit tiny cert-carrying snapshot");

        let server = McpServer::from_snapshot(&gts, None, McpMode::Consumer).unwrap();
        let out = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
        assert_eq!(
            out["ok"], true,
            "the tool reads the carried certificate: {out}"
        );
        assert_eq!(out["class_local_name"], "CoherenceCertificate");
        assert_eq!(out["issues_certificate"], true);
        assert_eq!(out["bundle_hash"], "blake3:carried-bundle");
        assert_eq!(
            out["axiom_hashes"],
            json!(["blake3:axioms-carried"]),
            "the per-graph axiom digests ride the read envelope: {out}"
        );
        // Deterministic: the read is a pure projection of the carried quads.
        let again = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
        assert_eq!(out, again, "the certificate read is deterministic");
    }

    /// G3 (HIGH, CodeRabbit): an INCONSISTENT-but-CONCLUSIVE closure must never
    /// surface as `CoherenceCertificate` on the tool surface — `completeness_class`
    /// used to return `CoherenceCertificate` for ANY `is_conclusive()` result, with no
    /// check for a named certified fragment or for the absence of a forbidden
    /// violation, and `run_explain_quad` (unlike `run_verify_graph`, which bolted on
    /// its own ad-hoc `Refused` downgrade) had NO protection at all.
    ///
    /// Drives the REAL `verify_graph` tool (`call_tool_result`, not internals) with a
    /// tiny canon plus an overlay that is GENUINELY inconsistent — `A ⊑ B`, `A ⊑ C`,
    /// `B disjointWith C`, `x : A` forces `x` into `owl:Nothing` (the exact fixture
    /// `reason_all_single_chase_yields_inconsistent_and_nonempty_closure` proves
    /// derives `InformationState::Both` in one completed chase, i.e. CONCLUSIVE, not
    /// budget-cut) — and asserts the response's `class_local_name` is NEVER
    /// `CoherenceCertificate`. `class_local_name` is `completeness_class`'s output
    /// (folded through `CoherenceOutcome::class_local_name_for`, the SAME gate
    /// `run_explain_quad` now reads through), so this proves the shared gate, not a
    /// per-tool special case.
    ///
    /// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
    /// not `_heavy_offgate`.
    #[test]
    fn verify_graph_inconsistent_but_conclusive_never_certifies() {
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        // A minimal canon: just the required ontology header (the importer hard-fails
        // without it) — the SAME pattern `coherence_certificate_tool_reads_the_carried_
        // bundle_heavy_offgate` uses.
        let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
        let dataset = dataset_of(doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit tiny header-only canon");

        let server = McpServer::from_snapshot(&gts, None, McpMode::Consumer).unwrap();

        // The overlay: a genuine DL contradiction — A ⊑ B, A ⊑ C, B disjointWith C,
        // x : A forces x into owl:Nothing. Un-graphed triples reason under the single
        // default world, and the whole tiny canon+overlay union closes well under the
        // governed step ceiling — CONCLUSIVE, never budget-cut.
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("contradiction.ttl");
        fs::write(
            &overlay_path,
            "<http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/B> .\n\
             <http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/A> .\n",
        )
        .unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 64})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

        // The closure genuinely completed (conclusive), not a budget-cut — so the
        // downgrade below is caused by the witnessed glut, not by non-conclusiveness.
        assert_eq!(
            out["evaluation"], "completed",
            "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
        );

        // The falsifiable assertion: an inconsistent-but-conclusive closure must NEVER
        // render `CoherenceCertificate` — the shared `CoherenceOutcome` gate downgrades
        // it to the flat refusal instead.
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "an inconsistent-but-conclusive closure must never be labeled a \
             CoherenceCertificate: {out}"
        );
        assert_eq!(
            out["class_local_name"], "Refused",
            "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal, per \
             the SAME CoherenceOutcome gate the bundle-level coherence certifier uses: {out}"
        );
        drop(overlay_dir);
    }

    /// CodeRabbit gap G4 (HIGH): `verify_graph`'s `coherent` field MUST agree with
    /// `class_local_name` — both MUST be derived from the SAME `CoherenceOutcome`
    /// gate, never from two independent signals.
    ///
    /// The overlay carries a genuine DL glut via plain PAIRWISE `owl:disjointWith`
    /// (A ⊑ B, A ⊑ C, B disjointWith C, x : A forces x into owl:Nothing) —
    /// DELIBERATELY not an `owl:AllDisjointClasses` set, so the ONE bad-example
    /// verify query that could independently catch a disjoint-axis violation
    /// (`class-in-two-disjoint-axes.rq`, which matches only `owl:AllDisjointClasses`
    /// membership) does NOT fire: `report.ok()` is true. Before the G4 fix,
    /// `coherent` was read straight from `report.ok()`, so this exact fixture would
    /// render the self-contradictory `coherent:true` alongside
    /// `class_local_name:"Refused"`. The fix routes `coherent` through the SAME
    /// shared `completeness_refused` gate that decides `class_local_name`, so the
    /// two fields can never disagree.
    ///
    /// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
    /// not `_heavy_offgate`.
    #[test]
    fn verify_graph_coherent_never_disagrees_with_refused_class() {
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        // The header-only canon PLUS the `axis-not-disjoint.rq` bad-example query's
        // own required orthogonality matrix (an `owl:AllDisjointClasses` set naming
        // the seven fixed identity axes) — otherwise that unrelated bad-example
        // query fires on ANY header-only canon (it demands the matrix exist at all),
        // which would make `report.ok()` false for a reason having nothing to do
        // with this test's glut and defeat the falsifiability of the assertion below.
        let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             _:axdisj <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#AllDisjointClasses> .\n\
             _:axdisj <http://www.w3.org/2002/07/owl#members> _:axlist0 .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderIdentity> .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist1 .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderExpression> .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist2 .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexAssignedAtBirth> .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist3 .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexualOrientation> .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist4 .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/RomanticOrientation> .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist5 .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/PronounSet> .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist6 .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/Honorific> .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .\n";
        let dataset = dataset_of(doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit tiny header-only canon");

        let server = McpServer::from_snapshot(&gts, None, McpMode::Consumer).unwrap();

        // The glut: same shape as `verify_graph_inconsistent_but_conclusive_never_
        // certifies`, but PAIRWISE `owl:disjointWith` on classes NOT named in the
        // orthogonality matrix above — so neither `axis-not-disjoint.rq` (satisfied
        // by the matrix) nor `class-in-two-disjoint-axes.rq` (requires
        // `owl:AllDisjointClasses` membership, which g4B/g4C never join) can match.
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("glut-no-bad-example-match.ttl");
        fs::write(
            &overlay_path,
            "<http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4B> .\n\
             <http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/g4A> .\n",
        )
        .unwrap();
        let path_str = overlay_path.to_str().unwrap();

        let out = text_payload(
            server.call_tool_result("verify_graph", &json!({"path": path_str, "max_steps": 64})),
        );
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["evaluation"], "completed",
            "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
        );

        // The class label refutes coherence via the DL glut...
        assert_eq!(
            out["class_local_name"], "Refused",
            "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal: {out}"
        );
        // ...and `coherent` MUST agree — never `coherent:true` alongside
        // `class_local_name:"Refused"`, even though no bad-example verify query
        // fired on this fixture's pairwise `owl:disjointWith` shape.
        assert_eq!(
            out["coherent"], false,
            "coherent must be false whenever class_local_name is Refused, regardless of \
             whether any bad-example verify query matched: {out}"
        );
        drop(overlay_dir);
    }
}
