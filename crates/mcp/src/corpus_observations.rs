// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit producer observations for corpus-backed MCP verification and explanation.
//! Each request is independently cached by the owning fixture action. Tests only
//! read its authenticated response and evidence; no corpus closure runs in a test.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use gmeow_logic::explain::Row;
use gmeow_logic::result::{BudgetUsage, CompletenessStatus, EvaluationStatus, ReasoningResult};
use purrdf::RdfDataset;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{McpServer, McpView, SegmentSet, explain, governed_budget, reifier_from_row};

/// Known credence violation, with an eight-step allowance.
pub const BAD_CREDENCE: &str = "mcp-native-bad-credence-v1.json";
/// Literal prose must not forge a structured citation.
pub const FORGED_CITATION: &str = "mcp-native-forged-citation-v1.json";
/// The external annex must leave the signed carrier untouched.
pub const ISOLATION: &str = "mcp-native-overlay-isolation-v1.json";
/// One-step exhaustion and its grounded judgment share one execution.
pub const BUDGET_CUT: &str = "mcp-native-budget-cut-v1.json";
/// Omitted allowance is governed by the mandatory finite default.
pub const OMITTED_BUDGET: &str = "mcp-native-omitted-budget-v1.json";
/// A normal small annex passes the byte and quad ingress bounds.
pub const NORMAL_OVERLAY: &str = "mcp-native-normal-overlay-v1.json";
/// An asserted target and its grounded judgment under eight steps.
pub const ASSERTED_EXPLANATION: &str = "mcp-native-asserted-explanation-v1.json";
/// A derived target with genuine rule premises under sixty-four steps.
pub const DERIVED_EXPLANATION: &str = "mcp-native-derived-explanation-v1.json";
/// An unknown target fails, retaining the exact governed request.
pub const ABSENT_EXPLANATION: &str = "mcp-native-absent-explanation-v1.json";
/// Exact shipped dictionary bytes used by synthetic runtime-store tests.
///
/// This producer artifact lets tests author user-store records without loading the
/// ontology bundle into a second producer path.
pub const MEMORY_HOT_MEDIUM: &str = "mcp-memory-hot-medium-v1.zdict";
/// Complete, finite set of supported observation actions.
pub const ARTIFACTS: &[&str] = &[
    BAD_CREDENCE,
    FORGED_CITATION,
    ISOLATION,
    BUDGET_CUT,
    OMITTED_BUDGET,
    NORMAL_OVERLAY,
    ASSERTED_EXPLANATION,
    DERIVED_EXPLANATION,
    ABSENT_EXPLANATION,
];

/// Independent projection and governance fields of the exact native result.
#[derive(Debug, Serialize, Deserialize)]
pub struct NativeObservation {
    /// Projection from the typed native result, compared with its response envelope.
    pub judgment_nquads: String,
    /// Actual native completeness, independent of response wording.
    pub completeness: CompletenessStatus,
    /// Actual native evaluation status.
    pub evaluation: EvaluationStatus,
    /// The shared governor's consumed and admitted allowance.
    pub consumed_budget: BudgetUsage,
}

impl NativeObservation {
    fn observe(result: &ReasoningResult) -> gmeow_errors::Result<Self> {
        result.validate_native_closure()?;
        Ok(Self {
            judgment_nquads: gmeow_logic::result_rdf::project_reasoning_result(result)?,
            completeness: result.completeness,
            evaluation: result.evaluation,
            consumed_budget: result.provenance.consumed_budget,
        })
    }
}

/// Compact target identity selected from the genuine premise-closed row set.
#[derive(Debug, Serialize, Deserialize)]
pub struct Target {
    /// Exact firing rule, including the asserted-rule identity.
    pub rule_iri: String,
    /// Native row's canonical N3 object surface.
    pub obj: String,
    /// Actual target's content-addressed statement reifier.
    pub reifier: String,
}

/// Observed immutability across an external-overlay execution.
#[derive(Debug, Serialize, Deserialize)]
pub struct Isolation {
    /// Whether the server retained the original carrier allocation.
    pub same_allocation: bool,
    /// Original carrier quad count.
    pub before_quads: usize,
    /// Carrier quad count after the request completed.
    pub after_quads: usize,
    /// Whether the synthetic overlay probe appeared in the signed carrier.
    pub probe_leaked: bool,
}

/// One authenticated corpus request and its exact production observations.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observation {
    /// Exact request, including omission versus explicit allowance selection.
    pub request: Value,
    /// Production response envelope, including expected hard refusals.
    pub response: Value,
    /// Native evidence exists for successful reasoning responses.
    pub native: Option<NativeObservation>,
    /// A positive explanation's independently selected target.
    pub target: Option<Target>,
    /// Overlay immutability evidence, present for verification requests.
    pub isolation: Option<Isolation>,
}

struct ExplainRun {
    result: ReasoningResult,
    rows: Vec<Row>,
}

/// Invocation-local producer state. It shares the already imported snapshot,
/// admitted view and the two explicitly selected explanation allowances.
pub struct Producer {
    server: McpServer,
    input: OnceLock<gmeow_logic::reason::PreparedReasoningInput>,
    eight: OnceLock<ExplainRun>,
    sixty_four: OnceLock<ExplainRun>,
}

impl Producer {
    /// Admit one selected native dataset and its exact source bundle bytes.
    ///
    /// # Errors
    /// Refuses an invalid bundle view or unavailable declared MCP capability.
    pub fn new(dataset: Arc<RdfDataset>, snapshot: Arc<[u8]>) -> gmeow_errors::Result<Self> {
        let view = Arc::new(McpView::from_dataset(dataset, snapshot)?);
        Ok(Self {
            server: McpServer::from_admitted_view(
                view,
                SegmentSet::linked(),
                crate::extension::Extension::new(),
                Some("en"),
            )?,
            input: OnceLock::new(),
            eight: OnceLock::new(),
            sixty_four: OnceLock::new(),
        })
    }

    /// Produce exactly one requested observation. Hits never call this method.
    ///
    /// # Errors
    /// Propagates actual request, native execution, evidence and serialization refusals.
    pub fn produce(&self, name: &str) -> gmeow_errors::Result<Vec<u8>> {
        let observation = match name {
            ASSERTED_EXPLANATION => self.explanation(false)?,
            DERIVED_EXPLANATION => self.explanation(true)?,
            ABSENT_EXPLANATION => self.absent_explanation()?,
            _ => self.verification(name)?,
        };
        serde_json::to_vec(&observation).map_err(gmeow_errors::Diag::from)
    }

    fn explain_run(&self, steps: u64) -> gmeow_errors::Result<&ExplainRun> {
        let cache = match steps {
            8 => &self.eight,
            64 => &self.sixty_four,
            _ => return Err(fail("unregistered explanation allowance")),
        };
        cache.get_or_try_init(|| {
            let input = self.input.get_or_try_init(|| {
                let edb = gmeow_logic::reasoning_graphs::project_object_level_edb(
                    &self.server.view.dataset,
                )?;
                gmeow_logic::reason::prepare_reasoning_input(&edb)
            })?;
            let result = gmeow_logic::reason::reason_all_budgeted(
                input.clone(),
                &gmeow_logic::reasoning_graphs::object_level_domains()?,
                &governed_budget(Some(steps), None),
            )?;
            result.validate_native_closure()?;
            let rows = explain::rows_for_result(&result)?;
            Ok(ExplainRun { result, rows })
        })
    }

    fn explanation(&self, derived: bool) -> gmeow_errors::Result<Observation> {
        let steps = if derived { 64 } else { 8 };
        let run = self.explain_run(steps)?;
        let mut counts = BTreeMap::new();
        for row in &run.rows {
            *counts
                .entry((row.graph.as_str(), reifier_from_row(row)))
                .or_insert(0usize) += 1;
        }
        let row = run
            .rows
            .iter()
            .find(|row| {
                (row.rule_iri != gmeow_logic::provenance::ASSERT_RULE_IRI) == derived
                    && row.obj.starts_with('<')
                    && row.obj.ends_with('>')
                    && counts.get(&(row.graph.as_str(), reifier_from_row(row))) == Some(&1)
            })
            .ok_or_else(|| fail("selected corpus closure has no unique requested IRI target"))?;
        let request = json!({
            "subject": row.subject, "predicate": row.predicate,
            "object_value": &row.obj[1..row.obj.len() - 1], "object_kind": "iri",
            "graph": row.graph, "max_steps": steps,
        });
        let response =
            self.server
                .with_explain_quad_request(&request, |_, s, p, o, graph, budget| {
                    if budget.max_steps != Some(steps) {
                        return Err(fail(
                            "request allowance differs from its selected native execution",
                        ));
                    }
                    McpView::explain_native_rows(&run.result, &run.rows, s, p, o, graph)
                })??;
        Ok(Observation {
            request,
            response,
            native: Some(NativeObservation::observe(&run.result)?),
            target: Some(Target {
                rule_iri: row.rule_iri.clone(),
                obj: row.obj.clone(),
                reifier: reifier_from_row(row),
            }),
            isolation: None,
        })
    }

    fn absent_explanation(&self) -> gmeow_errors::Result<Observation> {
        let run = self.explain_run(8)?;
        let request = json!({
            "subject": "urn:ex:not-a-real-bundle-subject",
            "predicate": "urn:ex:not-a-real-bundle-predicate",
            "object_value": "urn:ex:not-a-real-bundle-object", "object_kind": "iri",
            "graph": "urn:ex:not-a-real-world", "max_steps": 8,
        });
        let outcome = self
            .server
            .with_explain_quad_request(&request, |_, s, p, o, graph, _| {
                McpView::explain_native_rows(&run.result, &run.rows, s, p, o, graph)
            })?;
        let response = match outcome {
            Ok(value) => value,
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        Ok(Observation {
            request,
            response,
            native: None,
            target: None,
            isolation: None,
        })
    }

    fn verification(&self, name: &str) -> gmeow_errors::Result<Observation> {
        let request = match name {
            BAD_CREDENCE => {
                json!({"data": "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n<urn:ex:bad-credence-state> gmeow:credence 5 .\n", "format": "turtle", "max_steps": 8})
            }
            FORGED_CITATION => {
                json!({"data": "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n<urn:ex:forge-cited-iris-state> gmeow:credence \"see <urn:fake>\" .\n", "format": "turtle", "max_steps": 8})
            }
            ISOLATION => {
                json!({"data": "<urn:ex:probe-s> <urn:ex:probe-p> <urn:ex:probe-o> .\n", "format": "turtle", "max_steps": 4})
            }
            BUDGET_CUT => {
                json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle", "max_steps": 1})
            }
            OMITTED_BUDGET => {
                json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle"})
            }
            NORMAL_OVERLAY => {
                json!({"data": "<urn:ex:pasted> <urn:ex:label> \"Pasted Widget\" .\n", "format": "turtle", "max_steps": 1})
            }
            _ => return Err(fail("unregistered MCP corpus observation")),
        };
        let before = Arc::clone(&self.server.view.dataset);
        let before_quads = before.quad_count();
        let (response, native) = self
            .server
            .with_verify_graph_request(&request, McpView::run_verify_graph)??;
        let isolation = Isolation {
            same_allocation: Arc::ptr_eq(&before, &self.server.view.dataset),
            before_quads,
            after_quads: self.server.view.dataset.quad_count(),
            probe_leaked: self.server.view.dataset.owned_quads().any(|quad| {
                matches!(quad.subject, purrdf::RdfTerm::Iri(ref iri) if iri == "urn:ex:probe-s")
            }),
        };
        Ok(Observation {
            request,
            response,
            native: Some(NativeObservation::observe(&native)?),
            target: None,
            isolation: Some(isolation),
        })
    }
}

fn fail(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Mcp {
        message: message.to_owned(),
    })
}
