// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The deterministic RDF projection of a typed [`ReasoningResult`] into the
//! `graph/reasoning` named graph (C7).
//!
//! This is the dual-carriage projection layer for the reasoning lane: the pipeline
//! carries the live typed [`ReasoningResult`] as a
//! [`PipelineHandle::Reasoning`](../../pipeline/src/bundle.rs) handle AND the
//! repo-free `gmeow.gts` carries an equivalent RDF named graph a consumer can query
//! without re-running the engine. This module mints the latter.
//!
//! # What it captures
//!
//! A single content-addressed subject (the *result node*) carries:
//!
//! * the **five axes** — `logic:resultInput` / `resultEvaluation` /
//!   `resultCompleteness` / `resultInformation`, each linking the result node to the
//!   axis individual the enum's [`iri()`](InputStatus::iri) already mints, plus the
//!   `preservation` axis as a *set* of `logic:resultPreservationPolarity` links (one
//!   per [`PreservationKind`]) and `logic:resultUnsupportedConstruct` literals.
//! * the **provenance** bundle — contract hash, query/conclusion text, proof and
//!   counterproof derivation references (+ their sorted cited IRIs), engine
//!   name/version, consumed/declared budget + the tripped limit, the world /
//!   standpoint / time / path context, certified fragment, assumptions (sorted),
//!   and the contradiction witnesses (sorted, with their premises).
//! * a faithful **payload summary** — the payload discriminant plus a count, and for
//!   the `Inferred` surface the derived (non-EDB) axiom triples as
//!   `logic:resultDerivedAxiom` reified statements carrying their exact rule,
//!   immediate premises, source reifiers, and derivation identity (the closure itself
//!   remains the reason stage's `dataset`, so asserted rows are not duplicated here).
//!
//! # Determinism
//!
//! Every set/collection is materialized through a [`BTreeSet`] or an explicit sort
//! before emission, and the emitter writes **sorted N-Triples** (one canonical line
//! per triple, then `lines.sort()`), so the projection bytes are byte-stable across
//! runs of the same result. The subject IRI is `sha256` of the *un-subjected* triple
//! body, so two structurally-equal results mint the same node (content-addressed
//! identity) and a single result is reproducible.
//!
//! # Round-trip honesty (Principle 17)
//!
//! The projection is **faithful but not a total inverse**. What round-trips exactly
//! from `graph/reasoning` back to a [`ReasoningResult`] (via [`parse_reasoning_graph`]):
//! the five scalar axes, the preservation polarity set + unsupported constructs, the
//! whole provenance bundle (contract hash, query, conclusion, proof/counterproof refs,
//! engine, budget, context, certified fragment, assumptions, contradiction witnesses),
//! and the payload **discriminant**. What does NOT round-trip is the payload *contents*
//! of the `Bindings` / `Marginals` surfaces. Derived `Inferred` rows do round-trip
//! their full rule, immediate premises, source reifiers, and content-addressed identity.
//! So the re-derived result carries an
//! `Empty`/`Inferred(derived-only)` payload faithful for the *handle's* purpose (a
//! consumer reads the verdict + provenance, and reads the closure from the dataset),
//! and the parser is documented as reconstructing the verdict-and-provenance result,
//! not the original bindings. This mirrors the C6 precedent: exact where it holds
//! (axes/provenance), faithful-subset where the payload rows are carried elsewhere.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, PreservationKind};
use purrdf::{DatasetView, GraphMatch, QuadIds, RdfDataset, TermId, TermRef};
use sha2::{Digest, Sha256};

mod native;

use crate::conjecture::{ConjectureAnswer, ConjectureDischarge, ConjectureLifecycleState};
use crate::explain::{
    canonical_rule_iri, decode_receipt_rule_identity, encode_receipt_rule_identity,
    receipt_for_axiom,
};
use crate::reason::el::InferredAxiom;
use crate::result::{
    Assumption, BudgetLimit, CompletenessStatus, ContradictionWitness, DerivationRef, EngineId,
    EvaluationStatus, InformationState, InputStatus, PreservationClaim, ReasoningResult,
    ResultClaim, ResultContext, ResultPayload, ResultProvenance,
};

/// Wrap a reasoning-result-projection condition message as a typed diagnostic on
/// the shared substrate, preserving the authored text verbatim.
fn result_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Result { detail })
}

/// The `graph/reasoning` named-graph IRI — the snapshot folds this projection here.
pub const GRAPH_REASONING: &str = "https://blackcatinformatics.ca/gmeow/graph/reasoning";
/// The content-addressed result-node IRI base (`+ sha256(body)`).
const RESULT_IRI_BASE: &str = "https://blackcatinformatics.ca/gmeow/graph/reasoning/result/";
/// The content-addressed conjecture-node IRI base (`+ sha256(body)`). DISTINCT from
/// [`RESULT_IRI_BASE`] so a conjecture node never collides with the reasoning-result
/// node it embeds and links to via `logic:conjectureVerdict`.
const CONJECTURE_IRI_BASE: &str = "https://blackcatinformatics.ca/gmeow/graph/conjecture/";
/// The content-addressed IRI base for the POSITIVE promotion leg's target — the
/// `logic:FormalizationCandidate` a corroborated conjecture proposes (`logic:conjecture-
/// PromotionCandidate`). DISTINCT from (and not a prefix-collision with) [`CONJECTURE_IRI_BASE`]
/// — the segment is `conjecture-promotion`, never `conjecture/…`, so the conjecture-node
/// scan never mistakes a candidate node for the conjecture subject.
const PROMOTION_CANDIDATE_IRI_BASE: &str =
    "https://blackcatinformatics.ca/gmeow/graph/conjecture-promotion/";
/// The content-addressed IRI base for the SYMMETRIC anti-conjecture leg's target — the
/// candidate `logic:NonEntailmentObligation` a refuted conjecture proposes (`logic:anti-
/// ConjectureObligationCandidate`). DISTINCT from [`CONJECTURE_IRI_BASE`] for the same reason.
const OBLIGATION_CANDIDATE_IRI_BASE: &str =
    "https://blackcatinformatics.ca/gmeow/graph/conjecture-obligation/";
/// The `math:` namespace (the math→logic twin edges: the always-present
/// `math:conjectureUnderTest` bridge and the refutation-only `math:hasCounterexample`).
const MATH_NAMESPACE: &str = "https://blackcatinformatics.ca/math/";
/// The `gmeow:` namespace (the projection's provenance edges).
const GMEOW_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// The deterministic activity IRI a conjecture-verdict projection was generated by.
const CONJECTURE_ACTIVITY: &str = "https://blackcatinformatics.ca/gmeow/activity/conjecture-test";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const PROV_WAS_DERIVED_FROM: &str = "http://www.w3.org/ns/prov#wasDerivedFrom";
const PROV_VALUE: &str = "http://www.w3.org/ns/prov#value";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_NONNEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";

/// `logic:` IRI helper. All container/structure predicates this projection mints
/// live in the logic namespace (consistent with the axis individuals), so the
/// projection never collides with the gmeow vocabulary surface.
fn logic(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

/// A node term for N-Triples emission: an IRI, a blank node, or a typed literal.
#[derive(Clone)]
enum Node {
    /// Generated self-reference, distinct from any identical authored IRI.
    SelfReference(String),
    Iri(String),
    Blank(String),
    /// An authored or inferred native RDF value, retained through the output boundary.
    Value(purrdf::TermValue),
    Lit {
        lex: String,
        datatype: String,
    },
}

impl Node {
    fn iri(s: impl Into<String>) -> Self {
        Node::Iri(s.into())
    }
    fn blank(s: impl Into<String>) -> Self {
        Node::Blank(s.into())
    }
    fn string(s: impl Into<String>) -> Self {
        Node::Lit {
            lex: s.into(),
            datatype: XSD_STRING.to_owned(),
        }
    }
    fn integer(n: u64) -> Self {
        Node::Lit {
            lex: n.to_string(),
            datatype: XSD_INTEGER.to_owned(),
        }
    }
    fn nonnegative_integer(n: u64) -> Self {
        Node::Lit {
            lex: n.to_string(),
            datatype: XSD_NONNEGATIVE_INTEGER.to_owned(),
        }
    }
    /// An `xsd:anyURI` typed literal (the `logic:obligationForbiddenPredicate` datatype —
    /// a predicate IRI carried as a lexical URI, exactly as the authored obligations do).
    fn any_uri(s: impl Into<String>) -> Self {
        Node::Lit {
            lex: s.into(),
            datatype: XSD_ANY_URI.to_owned(),
        }
    }
    /// Render this node in canonical N-Triples term syntax.
    fn render(&self) -> String {
        match self {
            Node::Iri(iri) | Node::SelfReference(iri) => format!("<{iri}>"),
            Node::Blank(id) => format!("_:{id}"),
            Node::Value(value) => crate::provenance::term_display(value),
            Node::Lit { lex, datatype } => {
                format!("\"{}\"^^<{datatype}>", escape_literal(lex))
            }
        }
    }
}

/// Escape a lexical form for an N-Triples quoted literal (RDF 1.1 §7).
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Never emit a raw control scalar into the RDF transport. In particular,
            // structured formula content keys deliberately contain NUL separators;
            // preserving those bytes as UCHAR escapes keeps the N-Triples parseable
            // while round-tripping the exact lexical form.
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A triple `(subject, predicate, object)` accumulated before sorting.
struct Triple {
    subject: Node,
    predicate: String,
    object: Node,
}

/// The accumulating triple sink: collects triples, mints stable blank-node ids, and
/// renders a deterministic (sorted, deduplicated) N-Triples body.
#[derive(Default)]
struct Sink {
    triples: Vec<Triple>,
    next_blank: u64,
}

impl Sink {
    fn push(&mut self, subject: Node, predicate: impl Into<String>, object: Node) {
        self.triples.push(Triple {
            subject,
            predicate: predicate.into(),
            object,
        });
    }
    /// Mint a fresh, deterministically-numbered blank node id. Determinism holds
    /// because the emission order over the (sorted) inputs is fixed.
    fn fresh_blank(&mut self, hint: &str) -> String {
        let id = format!("{hint}{}", self.next_blank);
        self.next_blank += 1;
        id
    }
    /// Render the body as sorted, deduplicated N-Triples lines (no trailing newline
    /// per the join), with a placeholder substituted for the not-yet-known subject.
    fn render_lines(&self) -> Vec<String> {
        self.render_lines_in_graph(None)
    }

    fn render_lines_in_graph(&self, graph: Option<&str>) -> Vec<String> {
        let mut lines: BTreeSet<String> = BTreeSet::new();
        let graph = graph.map_or_else(String::new, |iri| format!(" <{iri}>"));
        for t in &self.triples {
            lines.insert(format!(
                "{} <{}> {}{graph} .",
                t.subject.render(),
                t.predicate,
                t.object.render()
            ));
        }
        lines.into_iter().collect()
    }
}

/// The placeholder subject the body is built against before the content-addressed
/// node IRI is known. It is substituted for the real IRI in the final pass.
const RESULT_PLACEHOLDER: &str = "urn:gmeow:reasoning-result:self";

/// Project a contextual result and its evidence DAG using the same RDF result
/// model as the native closure. Judgment anchors describe the assessed expression
/// and context; they never assert the expression as an unconditional fact.
pub fn project_contextual_assessment(
    assessment: &crate::contextual::ContextualAssessment,
) -> gmeow_errors::Result<String> {
    Ok(contextual_assessment_sink(assessment)?
        .0
        .render_lines()
        .join("\n")
        + "\n")
}

/// Export the complete contextual assessment as RDF 1.2 N-Quads. Reasoning and
/// proof records occupy `graph/reasoning`; the shared diagnostic renderer emits
/// every ledger witness in `graph/diagnostics`, retaining its fingerprint and
/// source anchor. The result and its selected attribution link those witnesses.
pub fn project_contextual_dataset(
    assessment: &crate::contextual::ContextualAssessment,
) -> gmeow_errors::Result<String> {
    let (mut sink, result_iri) = contextual_assessment_sink(assessment)?;
    let mut report = gmeow_errors::Report::new("gmeow-logic.contextual");
    for node in assessment.diagnostics.emit_sorted() {
        let finding = node.to_finding("gmeow-logic.contextual");
        let finding_iri = finding
            .finding_iri
            .as_ref()
            .expect("ledger witnesses have fingerprints");
        sink.push(
            Node::iri(&result_iri),
            PROV_WAS_DERIVED_FROM,
            Node::iri(finding_iri),
        );
        if let Some(context) = &assessment.result.provenance.context.attributed {
            sink.push(
                Node::iri(finding_iri),
                PROV_WAS_DERIVED_FROM,
                Node::iri(context),
            );
        }
        report.add_finding(finding);
    }
    let mut lines = sink
        .render_lines_in_graph(Some(GRAPH_REASONING))
        .into_iter()
        .collect::<BTreeSet<_>>();
    lines.extend(
        gmeow_errors::render::to_gmeow_rdf(&report)
            .lines()
            .map(str::to_owned),
    );
    Ok(lines.into_iter().collect::<Vec<_>>().join("\n") + "\n")
}

fn emit_temporal_basis(sink: &mut Sink, result: &Node, basis: &crate::contextual::TemporalBasis) {
    use crate::contextual::TemporalBasis;
    let (identity, class, properties) = match basis {
        TemporalBasis::Journal(prefix) => (
            &prefix.identity,
            "ObservedTemporalPrefix",
            vec![
                ("prefixJournal", Node::iri(&prefix.journal)),
                ("prefixEnactment", Node::iri(&prefix.enactment)),
                ("prefixHead", Node::iri(&prefix.head)),
                (
                    "prefixInitialHead",
                    Node::string(format!("blake3:{}", prefix.initial_head)),
                ),
                (
                    "prefixHeadHash",
                    Node::string(format!("blake3:{}", prefix.head_hash)),
                ),
                (
                    "journalBoundary",
                    Node::iri(logic(if prefix.finalized {
                        "FinalizedJournalBoundary"
                    } else {
                        "OpenJournalBoundary"
                    })),
                ),
            ],
        ),
        TemporalBasis::Path(observation) => (
            &observation.identity,
            "ObservedPathPrefix",
            vec![
                ("observedPath", Node::iri(&observation.path)),
                ("pathObservationWorld", Node::iri(&observation.world)),
                (
                    "pathObservationStandpoint",
                    Node::iri(&observation.standpoint),
                ),
                (
                    "pathObservationDigest",
                    Node::string(&observation.source_digest),
                ),
                (
                    "pathBoundary",
                    Node::iri(logic(if observation.finalized {
                        "FinalizedPathBoundary"
                    } else {
                        "OpenPathBoundary"
                    })),
                ),
            ],
        ),
    };
    let subject = Node::iri(identity);
    sink.push(
        result.clone(),
        logic("observedTemporalPrefix"),
        subject.clone(),
    );
    sink.push(subject.clone(), RDF_TYPE, Node::iri(logic(class)));
    for (property, object) in properties {
        sink.push(subject.clone(), logic(property), object);
    }
}

fn contextual_assessment_sink(
    assessment: &crate::contextual::ContextualAssessment,
) -> gmeow_errors::Result<(Sink, String)> {
    let projection = ResultProjection::reasoning(&assessment.result)?;
    let result_iri = projection.node_iri;
    let result = Node::iri(&result_iri);
    let mut sink = Sink::default();
    sink.push(
        Node::iri(&assessment.request),
        logic("contextualResult"),
        result.clone(),
    );
    if let Some(cause) = assessment.interrupted {
        sink.push(
            result.clone(),
            logic("incompleteCause"),
            Node::iri(cause.iri()),
        );
    }
    for basis in &assessment.temporal_prefixes {
        emit_temporal_basis(&mut sink, &result, basis);
    }
    for anchor in &assessment.anchors {
        let subject = Node::iri(&anchor.identity);
        sink.push(
            subject.clone(),
            RDF_TYPE,
            Node::iri(logic("ContextualJudgment")),
        );
        sink.push(
            subject.clone(),
            logic("judgmentContext"),
            Node::iri(&anchor.context),
        );
        sink.push(
            subject.clone(),
            logic("judgmentFormulaKey"),
            Node::string(&anchor.formula_key),
        );
        sink.push(
            subject.clone(),
            logic("judgmentInstruction"),
            Node::nonnegative_integer(anchor.instruction as u64),
        );
        sink.push(
            subject,
            logic("judgmentContextHash"),
            Node::string(&anchor.context_digest),
        );
    }
    for inference in &assessment.inferences {
        let subject = Node::iri(&inference.identity);
        sink.push(
            subject.clone(),
            RDF_TYPE,
            Node::iri(logic("ModalInference")),
        );
        sink.push(
            subject.clone(),
            logic("inferenceContext"),
            Node::iri(&inference.context),
        );
        sink.push(
            subject.clone(),
            gmeow("viaRule"),
            Node::iri(&inference.rule),
        );
        for antecedent in &inference.antecedents {
            sink.push(
                subject.clone(),
                PROV_WAS_DERIVED_FROM,
                Node::iri(antecedent),
            );
        }
    }
    for evidence in &assessment.native_evidence {
        let subject = Node::iri(evidence.identity());
        let row = evidence.row();
        sink.push(
            subject.clone(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
            Node::Value(purrdf::TermValue::Triple {
                s: Box::new(purrdf::TermValue::iri(&row.subject)),
                p: Box::new(purrdf::TermValue::iri(&row.predicate)),
                o: Box::new(purrdf::TermValue::iri(evidence.object())),
            }),
        );
        sink.push(subject.clone(), gmeow("inWorld"), Node::iri(&row.graph));
        sink.push(subject.clone(), gmeow("viaRule"), Node::iri(&row.rule_iri));
        sink.push(
            subject.clone(),
            gmeow("inferenceKind"),
            Node::iri(gmeow("Deduction")),
        );
        sink.push(
            subject.clone(),
            logic("derivationIdentifier"),
            Node::string(&row.derivation_id),
        );
        sink.push(
            subject.clone(),
            PROV_VALUE,
            Node::string(crate::explain::encode_receipt_rule_identity(
                evidence.raw_rule_identity(),
            )),
        );
        for antecedent in &row.source_quad_ids {
            sink.push(
                subject.clone(),
                PROV_WAS_DERIVED_FROM,
                Node::iri(antecedent),
            );
        }
    }
    // Scope generated components under this result's content address so that
    // separate requests cannot merge local blank labels such as deriv0.
    let scoped = |node: Node| match node {
        Node::Blank(label) => Node::iri(format!("{result_iri}/component/{label}")),
        other => other,
    };
    for triple in projection.sink.triples {
        sink.push(
            scoped(triple.subject),
            triple.predicate,
            scoped(triple.object),
        );
    }
    Ok((sink, result_iri))
}

/// Project a [`ReasoningResult`] into the deterministic `graph/reasoning` N-Triples
/// body (a `String`; one sorted canonical triple per line, trailing newline).
///
/// The result node is content-addressed: its IRI is [`RESULT_IRI_BASE`] + the
/// `sha256` (hex) of the placeholder-subjected body, so a structurally-equal result
/// mints the same node and a single result is byte-reproducible.
pub fn project_reasoning_result(result: &ReasoningResult) -> gmeow_errors::Result<String> {
    Ok(ResultProjection::reasoning(result)?.to_ntriples())
}

/// One validated terminal projection, shared by its content address and output
/// sinks. The producer retains the typed result through all intermediate stages.
pub struct ResultProjection {
    node_iri: String,
    sink: Sink,
}

impl ResultProjection {
    /// Prepare the complete selected result projection once.
    ///
    /// # Errors
    /// Rejects invalid result evidence or an unrepresentable native receipt.
    pub fn reasoning(result: &ReasoningResult) -> gmeow_errors::Result<Self> {
        result.validate()?;
        let sink = reasoning_sink(result)?;
        let (mut sink, node_iri) =
            content_addressed_sink(sink, RESULT_IRI_BASE, RESULT_PLACEHOLDER);
        for axiom in result.inferred().iter().filter(|axiom| {
            !axiom.is_edb
                && axiom.world == GRAPH_REASONING
                && axiom.rule_name.as_deref() == Some(crate::contextual::RULE_IRI)
        }) {
            sink.push(
                Node::iri(&axiom.subject),
                &axiom.predicate,
                Node::Value(axiom.object.clone()),
            );
        }
        Ok(Self { node_iri, sink })
    }

    /// Prepare one conjecture and its embedded reasoning evidence. The node
    /// address and every terminal format share this single lowering.
    ///
    /// # Errors
    /// Rejects invalid result evidence or an incomplete conjecture contract.
    pub fn conjecture(input: &ConjectureVerdictInput) -> gmeow_errors::Result<Self> {
        let (sink, node_iri) = conjecture_projection_sink(input)?;
        Ok(Self { node_iri, sink })
    }

    /// Borrow the address minted during this exact projection.
    pub fn node_iri(&self) -> &str {
        &self.node_iri
    }

    /// Serialize only at the selected terminal text boundary.
    pub fn to_ntriples(&self) -> String {
        self.sink.render_lines().join("\n") + "\n"
    }

    /// Move the already-prepared native triples into the RDF sink.
    ///
    /// # Errors
    /// Rejects malformed RDF terms or statement metadata.
    pub fn into_dataset(self) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
        native_dataset(self.sink)
    }
}

/// Project the governed reasoning summary directly into a native dataset.
///
/// This shares the typed emission and content-address calculation with
/// [`project_reasoning_result`], without serializing and reparsing RDF. Binding
/// and marginal rows remain in the native result; their exact kind and count
/// are represented here alongside the axes, provenance, and derived receipts.
///
/// # Errors
/// Rejects a summary that cannot form a valid native RDF dataset.
pub fn project_reasoning_dataset(
    result: &ReasoningResult,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    ResultProjection::reasoning(result)?.into_dataset()
}

/// Retain a contextual assessment's result and evidence directly as native RDF.
///
/// # Errors
/// Rejects an invalid result or malformed native statement metadata.
pub fn project_contextual_assessment_dataset(
    assessment: &crate::contextual::ContextualAssessment,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    native_dataset(contextual_assessment_sink(assessment)?.0)
}

/// Publish the canonical contextual records directly into native execution.
/// The terminal RDF writer and the native producer share the same projection;
/// no dataset, parser, or lexical RDF round trip mediates these statements.
pub(crate) fn contextual_assessment_facts(
    assessment: &crate::contextual::ContextualAssessment,
) -> gmeow_errors::Result<Vec<crate::rule_ir::Fact>> {
    fn value(node: Node) -> gmeow_errors::Result<purrdf::TermValue> {
        match node {
            Node::Iri(iri) => Ok(purrdf::TermValue::iri(iri)),
            Node::Value(value) => Ok(value),
            Node::Lit { lex, datatype } => Ok(purrdf::TermValue::Literal {
                lexical_form: lex,
                datatype,
                language: None,
                direction: None,
            }),
            Node::Blank(_) | Node::SelfReference(_) => Err(result_err(
                "native contextual projection contains an unresolved generated identity".into(),
            )),
        }
    }
    let (sink, _) = contextual_assessment_sink(assessment)?;
    let mut facts = std::collections::BTreeMap::new();
    let predicates = contextual_projection_predicates();
    for row in sink.triples {
        let fact = crate::rule_ir::Fact {
            subject: value(row.subject)?,
            predicate: row.predicate,
            object: value(row.object)?,
        };
        if fact.subject.as_iri().is_none() {
            return Err(result_err(
                "native contextual projection requires IRI subjects".into(),
            ));
        }
        let declared = if fact.predicate == RDF_TYPE {
            fact.object
                .as_iri()
                .is_some_and(|class| CONTEXTUAL_CLASSES.iter().any(|local| class == logic(local)))
        } else {
            predicates.contains(&fact.predicate)
        };
        if !declared {
            return Err(result_err(format!(
                "contextual projection writes an undeclared native effect: {}",
                fact.predicate
            )));
        }
        facts.insert(fact.key(), fact);
    }
    Ok(facts.into_values().collect())
}

const CONTEXTUAL_CLASSES: &[&str] = &[
    "ReasoningResult",
    "ContextualJudgment",
    "ModalInference",
    "Derivation",
    "ObservedTemporalPrefix",
    "ObservedPathPrefix",
];

fn contextual_projection_predicates() -> std::collections::BTreeSet<String> {
    [
        "contextualResult",
        "incompleteCause",
        "observedTemporalPrefix",
        "prefixJournal",
        "prefixEnactment",
        "prefixHead",
        "prefixInitialHead",
        "prefixHeadHash",
        "journalBoundary",
        "observedPath",
        "pathObservationWorld",
        "pathObservationStandpoint",
        "pathObservationDigest",
        "pathBoundary",
        "judgmentContext",
        "judgmentFormulaKey",
        "judgmentInstruction",
        "judgmentContextHash",
        "inferenceContext",
        "derivationIdentifier",
        "resultInput",
        "resultEvaluation",
        "resultCompleteness",
        "resultInformation",
        "resultPreservationPolarity",
        "resultUnsupportedConstruct",
        "resultClaim",
        "resultContractHash",
        "resultQuery",
        "resultConclusion",
        "resultProof",
        "resultCounterproof",
        "derivationId",
        "citesIri",
        "resultWorld",
        "resultStandpoint",
        "resultAttributedContext",
        "resultTime",
        "resultPath",
        "resultEngineName",
        "resultEngineVersion",
        "resultBudgetConsumed",
        "resultBudgetAllowance",
        "resultBudgetLimit",
        "resultCertifiedFragment",
        "resultAssumption",
        "resultPayloadKind",
        "resultPayloadCount",
    ]
    .into_iter()
    .map(logic)
    .chain([
        PROV_WAS_DERIVED_FROM.to_owned(),
        PROV_VALUE.to_owned(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies".to_owned(),
        gmeow("inWorld"),
        gmeow("viaRule"),
        gmeow("inferenceKind"),
    ])
    .collect()
}

/// Complete typed write contract of the contextual projection. Emission checks
/// every row against this same declaration before a native candidate is created.
pub(crate) fn contextual_projection_effects() -> Vec<crate::physical::StatementPattern> {
    contextual_projection_predicates()
        .iter()
        .map(|predicate| crate::physical::StatementPattern::relation(Some(predicate), None))
        .chain(CONTEXTUAL_CLASSES.iter().map(|class| {
            crate::physical::StatementPattern::relation(Some(RDF_TYPE), Some(&logic(class)))
        }))
        .collect()
}

/// The result vocabulary owns its statement layer. Reifier declarations and all
/// their annotations are retained in the same graph as the ordinary records.
fn native_dataset(sink: Sink) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    const REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
    let reifiers = sink
        .triples
        .iter()
        .filter(|row| row.predicate == REIFIES)
        .map(|row| match &row.subject {
            Node::Iri(iri) => Ok(iri.clone()),
            _ => Err(result_err(
                "native contextual receipt requires an IRI identity".into(),
            )),
        })
        .collect::<gmeow_errors::Result<BTreeSet<_>>>()?;
    let native = |node: Node| -> gmeow_errors::Result<purrdf::RdfTerm> {
        Ok(match node {
            Node::Iri(iri) => purrdf::RdfTerm::Iri(iri),
            Node::SelfReference(_) => {
                unreachable!("finalization resolves every generated reference")
            }
            Node::Blank(id) => purrdf::RdfTerm::BlankNode(id),
            Node::Value(value) => crate::reason::term_value_to_rdf_term(&value)?,
            Node::Lit { lex, datatype } => {
                purrdf::RdfTerm::Literal(purrdf::RdfLiteral::typed(lex, datatype))
            }
        })
    };
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for triple in sink.triples {
        let annotation = matches!(&triple.subject, Node::Iri(iri) if reifiers.contains(iri));
        let subject = native(triple.subject)?;
        let object = native(triple.object)?;
        if triple.predicate == REIFIES {
            let purrdf::RdfTerm::Triple(statement) = object else {
                return Err(result_err(
                    "native contextual reifier requires a quoted statement".into(),
                ));
            };
            builder.push_owned_reifier(&purrdf::RdfReifier::new(subject, *statement));
        } else if annotation {
            builder.push_owned_annotation(&purrdf::RdfAnnotation::new(
                subject,
                triple.predicate,
                object,
            ));
        } else {
            builder.push_owned_quad(&purrdf::RdfQuad::new(subject, triple.predicate, object));
        }
    }
    builder
        .freeze()
        .map_err(|error| result_err(error.to_string()))
}

fn reasoning_sink(result: &ReasoningResult) -> gmeow_errors::Result<Sink> {
    let mut sink = Sink::default();
    let subject = Node::SelfReference(RESULT_PLACEHOLDER.to_owned());

    sink.push(
        subject.clone(),
        RDF_TYPE,
        Node::iri(logic("ReasoningResult")),
    );

    // ── the five axes ──────────────────────────────────────────────────────────
    sink.push(
        subject.clone(),
        logic("resultInput"),
        Node::iri(result.input.iri()),
    );
    sink.push(
        subject.clone(),
        logic("resultEvaluation"),
        Node::iri(result.evaluation.iri()),
    );
    sink.push(
        subject.clone(),
        logic("resultCompleteness"),
        Node::iri(result.completeness.iri()),
    );
    sink.push(
        subject.clone(),
        logic("resultInformation"),
        Node::iri(result.information.iri()),
    );
    project_preservation(&mut sink, &subject, &result.preservation);

    // ── provenance ───────────────────────────────────────────────────────────────
    project_provenance(&mut sink, &subject, &result.provenance)?;

    // ── payload summary ──────────────────────────────────────────────────────────
    project_payload(&mut sink, &subject, &result.payload);

    Ok(sink)
}

/// Emit the preservation axis as a set of polarity links + unsupported-construct
/// literals (both already `BTreeSet`-ordered in the claim).
fn project_preservation(sink: &mut Sink, subject: &Node, claim: &PreservationClaim) {
    for kind in &claim.polarities {
        sink.push(
            subject.clone(),
            logic("resultPreservationPolarity"),
            Node::iri(kind.iri()),
        );
    }
    for construct in &claim.unsupported_constructs {
        sink.push(
            subject.clone(),
            logic("resultUnsupportedConstruct"),
            Node::string(construct.clone()),
        );
    }
}

/// Emit the full provenance bundle (every field, sorted where it is a set/list).
fn project_provenance(
    sink: &mut Sink,
    subject: &Node,
    prov: &ResultProvenance,
) -> gmeow_errors::Result<()> {
    sink.push(
        subject.clone(),
        logic("resultClaim"),
        Node::iri(logic(prov.claim.local_name())),
    );
    sink.push(
        subject.clone(),
        logic("resultContractHash"),
        Node::string(prov.contract_hash.clone()),
    );
    sink.push(
        subject.clone(),
        logic("resultQuery"),
        Node::string(prov.query.clone()),
    );
    sink.push(
        subject.clone(),
        logic("resultConclusion"),
        Node::string(prov.conclusion.clone()),
    );
    if let Some(proof) = &prov.proof {
        project_derivation(sink, subject, "resultProof", proof);
    }
    if let Some(counter) = &prov.counterproof {
        project_derivation(sink, subject, "resultCounterproof", counter);
    }
    project_context(sink, subject, &prov.context);
    project_engine(sink, subject, &prov.engine);
    project_budget(sink, subject, &prov.consumed_budget);
    if let Some(fragment) = &prov.certified_fragment {
        sink.push(
            subject.clone(),
            logic("resultCertifiedFragment"),
            Node::iri(fragment.clone()),
        );
    }
    for assumption in &prov.assumptions {
        sink.push(
            subject.clone(),
            logic("resultAssumption"),
            Node::iri(assumption_iri(*assumption)),
        );
    }
    // Sort witnesses into a canonical order before emission so the blank-node
    // numbering (which depends on emission order) is deterministic regardless of
    // caller ordering.  Two structurally-equal ReasoningResults whose witness
    // vecs are permutations of each other must mint the same digest.
    let mut sorted_witnesses: Vec<&ContradictionWitness> =
        prov.contradiction_witnesses.iter().collect();
    sorted_witnesses.sort();
    for witness in sorted_witnesses {
        project_witness(sink, subject, witness);
    }
    if let Some(execution) = &prov.native_execution {
        sink.push(
            subject.clone(),
            logic("resultNativeExecution"),
            Node::Lit {
                lex: native::encode(execution)?,
                datatype: "http://www.w3.org/2001/XMLSchema#hexBinary".into(),
            },
        );
    }
    Ok(())
}

/// Emit a proof/counterproof derivation reference (id + sorted cited IRIs) as a
/// blank-node `logic:Derivation` linked by `predicate_local`.
fn project_derivation(sink: &mut Sink, subject: &Node, predicate_local: &str, d: &DerivationRef) {
    let node = Node::blank(sink.fresh_blank("deriv"));
    sink.push(subject.clone(), logic(predicate_local), node.clone());
    sink.push(node.clone(), RDF_TYPE, Node::iri(logic("Derivation")));
    sink.push(
        node.clone(),
        logic("derivationId"),
        Node::string(d.derivation_id.clone()),
    );
    for iri in &d.cited_iris {
        sink.push(node.clone(), logic("citesIri"), Node::iri(iri.clone()));
    }
}

/// Emit the world/standpoint/time/path context.
///
/// The world is emitted as an IRI link ONLY when non-empty. The `reason` surface
/// carries per-axiom worlds on the closure payload and leaves the result-level
/// context world empty (an empty IRI `<>` is not absolute and would be invalid RDF),
/// so an empty world is OMITTED here and reconstructed as empty on parse.
fn project_context(sink: &mut Sink, subject: &Node, ctx: &ResultContext) {
    if !ctx.world.is_empty() {
        sink.push(
            subject.clone(),
            logic("resultWorld"),
            Node::iri(ctx.world.clone()),
        );
    }
    if let Some(standpoint) = &ctx.standpoint {
        sink.push(
            subject.clone(),
            logic("resultStandpoint"),
            Node::iri(standpoint.clone()),
        );
    }
    if let Some(context) = &ctx.attributed {
        sink.push(
            subject.clone(),
            logic("resultAttributedContext"),
            Node::iri(context.clone()),
        );
    }
    if let Some(time) = &ctx.time {
        sink.push(
            subject.clone(),
            logic("resultTime"),
            Node::string(time.clone()),
        );
    }
    if let Some(path) = &ctx.path {
        sink.push(
            subject.clone(),
            logic("resultPath"),
            Node::iri(path.clone()),
        );
    }
}

/// Emit the engine identity (name + version).
fn project_engine(sink: &mut Sink, subject: &Node, engine: &EngineId) {
    sink.push(
        subject.clone(),
        logic("resultEngineName"),
        Node::string(engine.name.clone()),
    );
    sink.push(
        subject.clone(),
        logic("resultEngineVersion"),
        Node::string(engine.version.clone()),
    );
}

/// Emit the consumed/declared budget and the tripped limit (when any).
fn project_budget(sink: &mut Sink, subject: &Node, budget: &crate::result::BudgetUsage) {
    sink.push(
        subject.clone(),
        logic("resultBudgetConsumed"),
        Node::integer(budget.consumed),
    );
    if let Some(allowance) = budget.allowance {
        sink.push(
            subject.clone(),
            logic("resultBudgetAllowance"),
            Node::integer(allowance),
        );
    }
    if let Some(limit) = budget.limit {
        sink.push(
            subject.clone(),
            logic("resultBudgetLimit"),
            Node::string(budget_limit_wire(limit).to_owned()),
        );
    }
}

/// Emit a contradiction witness (individual + world + sorted premise triples) as a
/// blank-node `logic:ContradictionWitness`.
fn project_witness(sink: &mut Sink, subject: &Node, w: &ContradictionWitness) {
    let node = Node::blank(sink.fresh_blank("witness"));
    sink.push(subject.clone(), logic("resultContradiction"), node.clone());
    emit_witness_body(sink, &node, w);
}

/// Emit the internal triples of a `logic:ContradictionWitness` node (its type,
/// `witnessIndividual` / `witnessWorld`, and the sorted `witnessPremise` set). Shared
/// by the reasoning-result projection (linked via `resultContradiction`) and the
/// conjecture-verdict projection (linked via `conjectureRefutationWitness`), so both
/// witness shapes are byte-identical and round-trip through the same reader.
fn emit_witness_body(sink: &mut Sink, node: &Node, w: &ContradictionWitness) {
    sink.push(
        node.clone(),
        RDF_TYPE,
        Node::iri(logic("ContradictionWitness")),
    );
    sink.push(
        node.clone(),
        logic("witnessIndividual"),
        Node::iri(w.individual.clone()),
    );
    sink.push(
        node.clone(),
        logic("witnessWorld"),
        Node::iri(w.world.clone()),
    );
    // Premises are emitted as a sorted set of opaque premise strings so the witness
    // shape is deterministic without minting yet another blank-node tier per premise.
    let mut premises: BTreeSet<String> = BTreeSet::new();
    for (s, p, o) in &w.premises {
        premises.insert(format!("{s} {p} {o}"));
    }
    for premise in premises {
        sink.push(node.clone(), logic("witnessPremise"), Node::string(premise));
    }
}

/// Emit a faithful summary of the payload: its discriminant + row/axiom count, and
/// for the `Inferred` surface the *derived* (non-EDB) axiom triples as reified rows.
fn project_payload(sink: &mut Sink, subject: &Node, payload: &ResultPayload) {
    let (kind, count) = match payload {
        ResultPayload::Inferred(axioms) => ("inferred", axioms.len() as u64),
        ResultPayload::Bindings(rows) => ("bindings", rows.len() as u64),
        ResultPayload::Marginals(rows) => ("marginals", rows.len() as u64),
        ResultPayload::Empty => ("empty", 0),
    };
    sink.push(
        subject.clone(),
        logic("resultPayloadKind"),
        Node::string(kind.to_owned()),
    );
    sink.push(
        subject.clone(),
        logic("resultPayloadCount"),
        Node::integer(count),
    );
    if let ResultPayload::Inferred(axioms) = payload {
        project_derived_axioms(sink, subject, axioms);
    }
}

/// Emit the derived (non-EDB) closure axioms as `logic:resultDerivedAxiom`
/// blank-node rows with their complete derivation receipts.
fn project_derived_axioms(sink: &mut Sink, subject: &Node, axioms: &[InferredAxiom]) {
    let mut rows: Vec<&InferredAxiom> = axioms.iter().filter(|axiom| !axiom.is_edb).collect();
    rows.sort();
    for axiom in rows {
        let node = Node::blank(sink.fresh_blank("axiom"));
        sink.push(subject.clone(), logic("resultDerivedAxiom"), node.clone());
        sink.push(node.clone(), RDF_TYPE, Node::iri(logic("DerivedAxiom")));
        sink.push(
            node.clone(),
            logic("axiomSubject"),
            axiom_term(&axiom.subject),
        );
        sink.push(
            node.clone(),
            logic("axiomPredicate"),
            axiom_term(&axiom.predicate),
        );
        sink.push(
            node.clone(),
            logic("axiomObject"),
            Node::Value(axiom.object.clone()),
        );
        sink.push(node.clone(), logic("axiomWorld"), axiom_term(&axiom.world));

        let receipt = receipt_for_axiom(axiom);
        sink.push(
            node.clone(),
            gmeow("viaRule"),
            Node::iri(receipt.row.rule_iri.clone()),
        );
        sink.push(
            node.clone(),
            logic("derivationIdentifier"),
            Node::string(receipt.row.derivation_id.clone()),
        );
        sink.push(
            node.clone(),
            PROV_VALUE,
            Node::string(encode_receipt_rule_identity(&receipt.raw_rule_identity)),
        );
        if let Some(evidence) = &axiom.modal_evaluation {
            sink.push(node.clone(), PROV_VALUE, Node::string(evidence.to_wire()));
        }
        for source in receipt.row.source_quad_ids {
            sink.push(node.clone(), PROV_WAS_DERIVED_FROM, Node::iri(source));
        }
        // Keep the complete native premise list even when importing a malformed
        // record for diagnostics. Zipping it with sources would silently discard
        // an extra premise instead of letting receipt admission reject it.
        for (index, (premise_subject, premise_predicate, premise_object)) in
            axiom.premises.iter().enumerate()
        {
            sink.push(
                node.clone(),
                PROV_VALUE,
                Node::string(encode_premise(
                    index,
                    premise_subject,
                    premise_predicate,
                    premise_object,
                )),
            );
        }
    }
}

fn encode_premise(index: usize, subject: &str, predicate: &str, object: &str) -> String {
    format!("{index}\0{subject}\0{predicate}\0{object}")
}

/// Normalize a native-engine term string (`<iri>` / `_:b` / `"lit"…` / bare-iri) into
/// a projection [`Node`]. The native chase emits subject/object/world in N3 term form
/// (a surrounding `<>` for IRIs); a bare IRI (no brackets) is treated as an IRI too.
/// Literals are carried as `xsd:string` (the projection records the answer *shape*,
/// and the full closure with exact datatypes rides the reason stage's dataset).
fn axiom_term(value: &str) -> Node {
    // A receipt carries the exact source spelling as a lexical value, just as
    // literal objects below do. Triple terms must never enter the IRI branch.
    if value.starts_with("<<") {
        return Node::string(value.to_owned());
    }
    if let Some(inner) = value.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        return Node::iri(inner.to_owned());
    }
    if let Some(blank) = value.strip_prefix("_:") {
        return Node::blank(blank.to_owned());
    }
    if value.starts_with('"') {
        // A literal term — keep the displayed lexical-with-quotes verbatim as a string
        // so the derived-row shape is recorded losslessly-enough for the summary.
        return Node::string(value.to_owned());
    }
    Node::iri(value.to_owned())
}

/// Resolve generated references without changing authored IRIs or literal data.
/// Encoded proof premises retain exactly the bytes their receipts authenticate.
fn content_addressed_sink(mut sink: Sink, iri_base: &str, placeholder_urn: &str) -> (Sink, String) {
    let node_iri = format!("{iri_base}{}", digest_lines(&sink.render_lines()));
    for triple in &mut sink.triples {
        for node in [&mut triple.subject, &mut triple.object] {
            if let Node::SelfReference(placeholder) = node {
                assert_eq!(
                    placeholder, placeholder_urn,
                    "the generated reference belongs to this projection"
                );
                *node = Node::Iri(node_iri.clone());
            }
        }
    }
    (sink, node_iri)
}

/// The `sha256` (lowercase hex) of the newline-joined body lines — the content-address
/// digest a node IRI is minted from (so two structurally-equal bodies mint one node).
fn digest_lines(lines: &[String]) -> String {
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// The `sha256` (lowercase hex) of an arbitrary string — the KB-world hash the conjecture
/// content-address folds in (so the same formula tested in two worlds mints two nodes).
fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// The content-addressed result-node IRI for `result` (the subject the projection
/// mints). Useful for a consumer that wants the node IRI without re-parsing.
pub fn result_node_iri(result: &ReasoningResult) -> gmeow_errors::Result<String> {
    Ok(ResultProjection::reasoning(result)?.node_iri)
}

// ── Assumption / budget-limit wire helpers (stable, projection-local) ────────────

/// The `logic:` individual IRI for an [`Assumption`] (PascalCase local name).
fn assumption_iri(a: Assumption) -> String {
    logic(match a {
        Assumption::ClosedWorld => "ClosedWorldAssumption",
        Assumption::OpenWorld => "OpenWorldAssumption",
        Assumption::UniqueName => "UniqueNameAssumption",
        Assumption::EntrenchmentRevision => "EntrenchmentRevisionAssumption",
        Assumption::SkolemWitness => "SkolemWitnessAssumption",
    })
}

/// Inverse of [`assumption_iri`] (parsing back from `graph/reasoning`).
fn assumption_from_iri(iri: &str) -> Option<Assumption> {
    let local = iri.strip_prefix(LOGIC_NAMESPACE)?;
    Some(match local {
        "ClosedWorldAssumption" => Assumption::ClosedWorld,
        "OpenWorldAssumption" => Assumption::OpenWorld,
        "UniqueNameAssumption" => Assumption::UniqueName,
        "EntrenchmentRevisionAssumption" => Assumption::EntrenchmentRevision,
        "SkolemWitnessAssumption" => Assumption::SkolemWitness,
        _ => return None,
    })
}

/// The wire value for a [`BudgetLimit`] (reuses the enum's canonical wire).
fn budget_limit_wire(limit: BudgetLimit) -> &'static str {
    limit.wire()
}

/// Inverse of [`budget_limit_wire`].
fn budget_limit_from_wire(wire: &str) -> Option<BudgetLimit> {
    Some(match wire {
        "answers" => BudgetLimit::Answers,
        "inference" => BudgetLimit::Inference,
        "depth" => BudgetLimit::Depth,
        _ => return None,
    })
}

// ── The parser: graph/reasoning → ReasoningResult (verdict + provenance) ─────────

/// Re-derive the verdict-and-provenance [`ReasoningResult`] from a `graph/reasoning`
/// N-Triples body (the cache / handle re-derivation path).
///
/// **Faithful subset** (Principle 17): this reconstructs the five axes, the
/// preservation claim, and the entire provenance bundle exactly. The inferred payload
/// reconstructs every derived row with its verified rule, immediate premises, source
/// reifiers, and derivation identity; binding and marginal rows remain in their owning
/// stage dataset. The re-derived result is the handle a
/// consumer needs: it reads the verdict + provenance from here and the closure quads
/// from the dataset. See the module docs for the exact round-trip contract.
///
/// # Errors
/// Returns `Err` if the body is missing the result subject, an axis IRI is
/// unrecognized, or a required scalar provenance field is absent (fail-closed).
pub fn parse_reasoning_graph(nt_body: &str) -> gmeow_errors::Result<ReasoningResult> {
    let dataset = parse_nt(nt_body)?;
    parse_reasoning_dataset(&dataset, GraphMatch::Default)
}

/// Recover a reasoning handle directly from one selected graph in a native dataset.
/// PurRDF's indexes and borrowed terms are reused; no graph copy, RDF text, or
/// second triple index is constructed. Derived rows retain every receipt check.
///
/// # Errors
/// Rejects malformed result projections and missing required provenance.
pub fn parse_reasoning_dataset(
    dataset: &RdfDataset,
    graph: GraphMatch,
) -> gmeow_errors::Result<ReasoningResult> {
    read_reasoning_result(&ResultGraph::new(dataset, graph)?)
}

fn read_reasoning_result(triples: &ResultGraph<'_>) -> gmeow_errors::Result<ReasoningResult> {
    let results = triples
        .iter()
        .filter(|t| t.predicate == RDF_TYPE && t.object_iri() == Some(logic("ReasoningResult")))
        .map(|t| t.subject)
        .collect::<BTreeSet<_>>();
    if results.is_empty() {
        return Err(result_err(
            "graph/reasoning: no logic:ReasoningResult subject".into(),
        ));
    }
    // An aggregate can also carry contextual children. Their request links
    // distinguish them from the aggregate handle; row order cannot choose it.
    let candidates = if results.len() > 1 {
        let contextual = triples
            .iter()
            .filter(|t| t.predicate == logic("contextualResult"))
            .filter_map(|t| t.object_node())
            .collect::<BTreeSet<_>>();
        results
            .difference(&contextual)
            .copied()
            .collect::<BTreeSet<_>>()
    } else {
        results
    };
    if candidates.len() != 1 {
        return Err(result_err(format!(
            "graph/reasoning: expected one aggregate logic:ReasoningResult subject, found {}",
            candidates.len(),
        )));
    }
    let subject = candidates.into_iter().next().expect("one result");

    let one_iri = |local: &str| -> Option<String> {
        triples
            .for_subject(subject)
            .find(|t| t.predicate == logic(local))
            .and_then(|t| t.object_iri())
    };
    let one_str = |local: &str| -> Option<String> {
        triples
            .for_subject(subject)
            .find(|t| t.predicate == logic(local))
            .and_then(|t| t.object_string())
    };

    let input = InputStatus::from_local(local_of(&req(one_iri("resultInput"), "resultInput")?)?)
        .ok_or_else(|| result_err("graph/reasoning: unrecognized resultInput".to_owned()))?;
    let evaluation = EvaluationStatus::from_local(local_of(&req(
        one_iri("resultEvaluation"),
        "resultEvaluation",
    )?)?)
    .ok_or_else(|| result_err("graph/reasoning: unrecognized resultEvaluation".to_owned()))?;
    let completeness = CompletenessStatus::from_local(local_of(&req(
        one_iri("resultCompleteness"),
        "resultCompleteness",
    )?)?)
    .ok_or_else(|| result_err("graph/reasoning: unrecognized resultCompleteness".to_owned()))?;
    let information = InformationState::from_local(local_of(&req(
        one_iri("resultInformation"),
        "resultInformation",
    )?)?)
    .ok_or_else(|| result_err("graph/reasoning: unrecognized resultInformation".to_owned()))?;

    // preservation: the polarity set + unsupported constructs.
    let mut preservation = PreservationClaim::default();
    for t in triples.for_subject(subject) {
        if t.subject == subject
            && t.predicate == logic("resultPreservationPolarity")
            && let Some(iri) = t.object_iri()
            && let Some(kind) = preservation_from_iri(&iri)
        {
            preservation.polarities.insert(kind);
        }
        if t.subject == subject
            && t.predicate == logic("resultUnsupportedConstruct")
            && let Some(s) = t.object_string()
        {
            preservation.unsupported_constructs.insert(s);
        }
    }

    // provenance: scalars + derivations + context + engine + budget + witnesses. The
    // world is OPTIONAL (the `reason` surface leaves the result-level context world
    // empty — it is carried per-axiom on the closure); an absent `resultWorld`
    // reconstructs as empty.
    let world = one_iri("resultWorld").unwrap_or_default();
    let mut prov = ResultProvenance::native(
        req(one_str("resultContractHash"), "resultContractHash")?,
        world.clone(),
    );
    let claim_rows: Vec<_> = triples
        .for_subject(subject)
        .filter(|row| row.predicate == logic("resultClaim"))
        .collect();
    let [claim] = claim_rows.as_slice() else {
        return Err(result_err(
            "graph/reasoning: expected exactly one resultClaim".into(),
        ));
    };
    let claim_iri = claim.object_iri().ok_or_else(|| {
        result_err("graph/reasoning: resultClaim must name a proposition scope".into())
    })?;
    prov.claim = claim_iri
        .strip_prefix(LOGIC_NAMESPACE)
        .and_then(ResultClaim::from_local)
        .ok_or_else(|| result_err("graph/reasoning: unknown resultClaim".into()))?;
    let execution_rows = triples
        .for_subject(subject)
        .filter(|row| row.predicate == logic("resultNativeExecution"))
        .collect::<Vec<_>>();
    prov.native_execution = match execution_rows.as_slice() {
        [] => None,
        [row] => match row.object {
            TermRef::Literal {
                lexical,
                datatype,
                language: None,
                direction: None,
            } if matches!(
                triples.dataset.resolve(datatype),
                TermRef::Iri("http://www.w3.org/2001/XMLSchema#hexBinary")
            ) =>
            {
                Some(native::decode(lexical)?)
            }
            _ => {
                return Err(result_err(
                    "graph/reasoning: resultNativeExecution requires hexBinary".into(),
                ));
            }
        },
        _ => {
            return Err(result_err(
                "graph/reasoning: duplicate resultNativeExecution".into(),
            ));
        }
    };
    prov.query = req(one_str("resultQuery"), "resultQuery")?;
    prov.conclusion = req(one_str("resultConclusion"), "resultConclusion")?;
    prov.proof = parse_derivation(triples, subject, "resultProof")?;
    prov.counterproof = parse_derivation(triples, subject, "resultCounterproof")?;
    let attributed_contexts = triples
        .for_subject(subject)
        .filter(|triple| triple.predicate == logic("resultAttributedContext"))
        .map(|triple| {
            triple.object_iri().ok_or_else(|| {
                result_err("graph/reasoning: resultAttributedContext must be an IRI".into())
            })
        })
        .collect::<gmeow_errors::Result<BTreeSet<_>>>()?;
    if attributed_contexts.len() > 1 {
        return Err(result_err(
            "graph/reasoning: resultAttributedContext must be single-valued".into(),
        ));
    }
    prov.context = ResultContext {
        world,
        standpoint: one_iri("resultStandpoint"),
        attributed: attributed_contexts.into_iter().next(),
        time: one_str("resultTime"),
        path: one_iri("resultPath"),
    };
    prov.engine = EngineId {
        name: req(one_str("resultEngineName"), "resultEngineName")?,
        version: req(one_str("resultEngineVersion"), "resultEngineVersion")?,
    };
    prov.consumed_budget = crate::result::BudgetUsage {
        consumed: req(one_str("resultBudgetConsumed"), "resultBudgetConsumed")?
            .parse::<u64>()
            .map_err(|e| {
                result_err(format!(
                    "graph/reasoning: resultBudgetConsumed not a u64: {e}"
                ))
            })?,
        allowance: one_str("resultBudgetAllowance").and_then(|s| s.parse().ok()),
        limit: one_str("resultBudgetLimit").and_then(|s| budget_limit_from_wire(&s)),
    };
    prov.certified_fragment = one_iri("resultCertifiedFragment");
    prov.projection_class = preservation.clone();
    for t in triples.for_subject(subject) {
        if t.subject == subject
            && t.predicate == logic("resultAssumption")
            && let Some(iri) = t.object_iri()
            && let Some(a) = assumption_from_iri(&iri)
        {
            prov.assumptions.insert(a);
        }
    }
    prov.contradiction_witnesses = parse_witnesses(triples, subject)?;
    prov.contradiction_witnesses.sort();

    // Payload discriminant — derived rows retain their complete receipts.
    let payload = match one_str("resultPayloadKind").as_deref() {
        Some("inferred") => ResultPayload::Inferred(parse_derived_axioms(triples, subject)?),
        Some("bindings") => ResultPayload::Bindings(Vec::new()),
        Some("marginals") => ResultPayload::Marginals(Vec::new()),
        _ => ResultPayload::Empty,
    };

    let result = ReasoningResult {
        input,
        evaluation,
        completeness,
        preservation,
        information,
        provenance: prov,
        payload,
        row_schema: None,
    };
    result.validate()?;
    Ok(result)
}

/// Admit a linked component by its RDF role in the selected graph. Ordinary
/// results use blank resources; contextual results give the same components
/// stable IRIs. Literal and quoted terms never become component addresses.
fn component_node(
    triples: &ResultGraph<'_>,
    link: ParsedTriple<'_>,
    class: &str,
) -> gmeow_errors::Result<TermId> {
    let node = link.object_resource().ok_or_else(|| {
        result_err(format!(
            "graph/reasoning: {} must link a {class} resource",
            link.predicate
        ))
    })?;
    let expected = logic(class);
    if !triples.for_subject(node).any(|field| {
        field.predicate == RDF_TYPE
            && matches!(field.object, TermRef::Iri(iri) if iri == expected.as_str())
    }) {
        return Err(result_err(format!(
            "graph/reasoning: {} component must be typed logic:{class} in the selected graph",
            link.predicate
        )));
    }
    Ok(node)
}

/// Resolve a single selected component link, distinguishing genuine absence
/// from multiple or malformed links instead of silently discarding evidence.
fn optional_component(
    triples: &ResultGraph<'_>,
    subject: TermId,
    predicate_local: &str,
    class: &str,
) -> gmeow_errors::Result<Option<TermId>> {
    let predicate = logic(predicate_local);
    let mut links = triples
        .for_subject(subject)
        .filter(|field| field.predicate == predicate);
    let Some(link) = links.next() else {
        return Ok(None);
    };
    if links.next().is_some() {
        return Err(result_err(format!(
            "graph/reasoning: {predicate_local} must be single-valued"
        )));
    }
    component_node(triples, link, class).map(Some)
}

/// Borrow one required component field without choosing among competing values.
fn component_field<'a>(
    triples: &ResultGraph<'a>,
    node: TermId,
    local: &str,
) -> gmeow_errors::Result<ParsedTriple<'a>> {
    let predicate = logic(local);
    let mut fields = triples
        .for_subject(node)
        .filter(|field| field.predicate == predicate);
    let field = fields.next().ok_or_else(|| {
        result_err(format!(
            "graph/reasoning: component {node:?} is missing {local}"
        ))
    })?;
    if fields.next().is_some() {
        return Err(result_err(format!(
            "graph/reasoning: component {node:?} requires one {local}"
        )));
    }
    Ok(field)
}

/// Component identifiers and encoded premises are authored as xsd:string. A
/// language, direction or different datatype cannot be erased into that role.
fn component_text(
    triples: &ResultGraph<'_>,
    field: ParsedTriple<'_>,
) -> gmeow_errors::Result<String> {
    match field.object {
        TermRef::Literal {
            lexical,
            datatype,
            language: None,
            direction: None,
        } if matches!(triples.dataset.resolve(datatype), TermRef::Iri(XSD_STRING)) => {
            Ok(lexical.to_owned())
        }
        _ => Err(result_err(format!(
            "graph/reasoning: {} must be an xsd:string literal",
            field.predicate
        ))),
    }
}

/// Parse the complete proof/counterproof component reached from this result.
fn parse_derivation(
    triples: &ResultGraph<'_>,
    subject: TermId,
    predicate_local: &str,
) -> gmeow_errors::Result<Option<DerivationRef>> {
    let Some(node) = optional_component(triples, subject, predicate_local, "Derivation")? else {
        return Ok(None);
    };
    let derivation_id = component_text(triples, component_field(triples, node, "derivationId")?)?;
    let cited_iris: BTreeSet<String> = triples
        .for_subject(node)
        .filter(|t| t.predicate == logic("citesIri"))
        .map(|t| {
            t.object_iri()
                .ok_or_else(|| result_err("graph/reasoning: citesIri must be an IRI".into()))
        })
        .collect::<gmeow_errors::Result<_>>()?;
    Ok(Some(DerivationRef {
        derivation_id,
        cited_iris,
    }))
}

/// Parse the contradiction witnesses (the premises are recovered as opaque
/// `s p o` strings split back into a 3-tuple).
fn parse_witnesses(
    triples: &ResultGraph<'_>,
    subject: TermId,
) -> gmeow_errors::Result<Vec<ContradictionWitness>> {
    let mut out = Vec::new();
    for link in triples
        .for_subject(subject)
        .filter(|t| t.predicate == logic("resultContradiction"))
    {
        let node = component_node(triples, link, "ContradictionWitness")?;
        out.push(parse_witness_body(triples, node)?);
    }
    Ok(out)
}

/// Parse derived-axiom rows and verify each complete derivation receipt.
fn parse_derived_axioms(
    triples: &ResultGraph<'_>,
    subject: TermId,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    let mut out = Vec::new();
    for link in triples
        .for_subject(subject)
        .filter(|t| t.predicate == logic("resultDerivedAxiom"))
    {
        let node = component_node(triples, link, "DerivedAxiom")?;
        // Subject, predicate and context retain their resource surfaces. The
        // object below is recovered directly from the native term dictionary.
        let field = |local: &str| -> String {
            let t = triples
                .for_subject(node)
                .find(|t| t.predicate == logic(local));
            match t {
                Some(t) => {
                    if let Some(iri) = t.object_iri() {
                        iri
                    } else if let Some(b) = t.object_blank() {
                        format!("_:{}", triples.blank_label(b))
                    } else {
                        t.object_string().unwrap_or_default()
                    }
                }
                None => String::new(),
            }
        };
        let object = triples
            .for_subject(node)
            .find(|triple| triple.predicate == logic("axiomObject"))
            .map(|triple| triples.dataset.term_value(triple.object_id))
            .ok_or_else(|| {
                result_err(format!(
                    "graph/reasoning: derived axiom {node:?} is missing its native object"
                ))
            })?;
        let rule_name = triples
            .for_subject(node)
            .find(|triple| triple.predicate == gmeow("viaRule"))
            .and_then(|t| t.object_iri())
            .ok_or_else(|| {
                result_err(format!(
                    "graph/reasoning: derived axiom {node:?} is missing its full gmeow:viaRule IRI"
                ))
            })?;
        let derivation_id = triples
            .for_subject(node)
            .find(|triple| triple.predicate == logic("derivationIdentifier"))
            .and_then(|t| t.object_string())
            .ok_or_else(|| {
                result_err(format!(
                    "graph/reasoning: derived axiom {node:?} is missing logic:derivationIdentifier"
                ))
            })?;
        let emitted_sources: BTreeSet<String> = triples
            .for_subject(node)
            .filter(|triple| triple.predicate == PROV_WAS_DERIVED_FROM)
            .map(|triple| {
                triple.object_iri().ok_or_else(|| {
                    result_err(format!(
                        "graph/reasoning: derived axiom {node:?} has a non-IRI source reifier"
                    ))
                })
            })
            .collect::<gmeow_errors::Result<_>>()?;
        let mut premises_by_index = BTreeMap::new();
        let mut receipt_rule_identity = None;
        let mut modal_evaluation = None;
        for wire in triples
            .for_subject(node)
            .filter(|triple| triple.predicate == PROV_VALUE)
        {
            let wire = wire.object_string().ok_or_else(|| {
                result_err(format!(
                    "graph/reasoning: derived axiom {node:?} has a non-literal receipt value"
                ))
            })?;
            if let Some(evidence) = crate::modal::ModalEvaluation::from_wire(&wire) {
                if modal_evaluation.replace(evidence?).is_some() {
                    return Err(result_err(
                        "graph/reasoning: repeated contextual modal evaluation".to_owned(),
                    ));
                }
                continue;
            }
            if let Some(raw_rule_identity) = decode_receipt_rule_identity(&wire) {
                if receipt_rule_identity
                    .replace(raw_rule_identity.to_owned())
                    .is_some()
                {
                    return Err(result_err(format!(
                        "graph/reasoning: derived axiom {node:?} repeats its raw receipt rule identity"
                    )));
                }
                continue;
            }
            let (index, premise) = decode_premise(&wire)?;
            if premises_by_index.insert(index, premise).is_some() {
                return Err(result_err(format!(
                    "graph/reasoning: derived axiom {node:?} repeats premise index {index}"
                )));
            }
        }
        if premises_by_index
            .keys()
            .copied()
            .ne(0..premises_by_index.len())
        {
            return Err(result_err(format!(
                "graph/reasoning: derived axiom {node:?} premise indexes are not contiguous from zero"
            )));
        }
        let receipt_rule_identity = receipt_rule_identity.ok_or_else(|| {
            result_err(format!(
                "graph/reasoning: derived axiom {node:?} is missing its raw receipt rule identity"
            ))
        })?;
        if canonical_rule_iri(&receipt_rule_identity) != rule_name {
            return Err(result_err(format!(
                "graph/reasoning: derived axiom {node:?} public gmeow:viaRule does not canonicalize its raw receipt rule identity"
            )));
        }
        let premises: Vec<(String, String, String)> = premises_by_index.into_values().collect();
        let axiom = InferredAxiom {
            modal_evaluation: modal_evaluation.map(Box::new),
            subject: field("axiomSubject"),
            predicate: field("axiomPredicate"),
            object,
            world: field("axiomWorld"),
            is_edb: false,
            rule_name: Some(receipt_rule_identity),
            premises,
        };
        match &axiom.modal_evaluation {
            Some(evidence) => evidence.validate_axiom(&axiom)?,
            None if axiom.rule_name.as_deref() == Some(crate::modal::MODAL_RULE_IRI) => {
                return Err(result_err(
                    "graph/reasoning: missing contextual modal evidence".to_owned(),
                ));
            }
            None => {}
        }
        let receipt = receipt_for_axiom(&axiom);
        if receipt.row.rule_iri != rule_name {
            return Err(result_err(format!(
                "graph/reasoning: derived axiom {node:?} public firing-rule identity does not match its receipt"
            )));
        }
        let expected_sources: BTreeSet<String> = receipt.row.source_quad_ids.into_iter().collect();
        if emitted_sources != expected_sources {
            return Err(result_err(format!(
                "graph/reasoning: derived axiom {node:?} source reifiers do not match its source premises"
            )));
        }
        if receipt.row.derivation_id != derivation_id {
            return Err(result_err(format!(
                "graph/reasoning: derived axiom {node:?} derivation identity does not match its rule and source premises"
            )));
        }
        out.push(axiom);
    }
    Ok(out)
}

fn decode_premise(wire: &str) -> gmeow_errors::Result<(usize, (String, String, String))> {
    let mut parts = wire.splitn(4, '\0');
    let index = parts
        .next()
        .ok_or_else(|| result_err("graph/reasoning: malformed premise index".to_owned()))?
        .parse::<usize>()
        .map_err(|_| result_err("graph/reasoning: malformed premise index".to_owned()))?;
    let subject = parts
        .next()
        .ok_or_else(|| result_err("graph/reasoning: malformed premise subject".to_owned()))?;
    let predicate = parts
        .next()
        .ok_or_else(|| result_err("graph/reasoning: malformed premise predicate".to_owned()))?;
    let object = parts
        .next()
        .ok_or_else(|| result_err("graph/reasoning: malformed premise object".to_owned()))?;
    Ok((
        index,
        (subject.to_owned(), predicate.to_owned(), object.to_owned()),
    ))
}

/// The `logic:` IRI for a [`PreservationKind`] back to the kind.
fn preservation_from_iri(iri: &str) -> Option<PreservationKind> {
    let local = iri.strip_prefix(LOGIC_NAMESPACE)?;
    Some(match local {
        "ExactPreservation" => PreservationKind::Exact,
        "SoundUnderApproximation" => PreservationKind::SoundUnder,
        "CompleteOverApproximation" => PreservationKind::CompleteOver,
        "ValidationOnly" => PreservationKind::ValidationOnly,
        "InconsistencyPreserving" => PreservationKind::InconsistencyPreserving,
        "InconsistencyReflecting" => PreservationKind::InconsistencyReflecting,
        "Unsupported" => PreservationKind::Unsupported,
        _ => return None,
    })
}

/// The local name of a `logic:`-prefixed IRI.
fn local_of(iri: &str) -> gmeow_errors::Result<&str> {
    iri.strip_prefix(LOGIC_NAMESPACE).ok_or_else(|| {
        result_err(format!(
            "graph/reasoning: axis IRI not in logic namespace: {iri}"
        ))
    })
}

/// Require a present value (fail-closed).
fn req<T>(v: Option<T>, what: &str) -> gmeow_errors::Result<T> {
    v.ok_or_else(|| result_err(format!("graph/reasoning: missing required field {what}")))
}

// ── Indexed native graph reader; text parsing only at the external boundary ──

/// A borrowed row in the projection's closed vocabulary. Topology keeps native
/// IDs, including blank scopes; only values entering a result become owned.
#[derive(Clone, Copy)]
struct ParsedTriple<'a> {
    subject: TermId,
    predicate: &'a str,
    object_id: TermId,
    object: TermRef<'a>,
}

impl ParsedTriple<'_> {
    fn object_iri(&self) -> Option<String> {
        match self.object {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        }
    }
    fn object_blank(&self) -> Option<TermId> {
        matches!(self.object, TermRef::Blank { .. }).then_some(self.object_id)
    }
    /// A component's resource address retains either its IRI or exact blank scope.
    fn object_resource(&self) -> Option<TermId> {
        matches!(self.object, TermRef::Iri(_) | TermRef::Blank { .. }).then_some(self.object_id)
    }
    fn object_node(&self) -> Option<TermId> {
        matches!(self.object, TermRef::Iri(_)).then_some(self.object_id)
    }
    fn object_string(&self) -> Option<String> {
        match self.object {
            TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
            _ => None,
        }
    }
}

struct ResultGraph<'a> {
    dataset: &'a RdfDataset,
    graph: GraphMatch,
    // Only the RDF 1.2 side tables need an auxiliary subject index. Ordinary
    // verdict and axiom rows reuse the native dataset's existing index.
    side_rows: BTreeMap<TermId, Vec<ParsedTriple<'a>>>,
}

impl<'a> ResultGraph<'a> {
    fn new(dataset: &'a RdfDataset, graph: GraphMatch) -> gmeow_errors::Result<Self> {
        let mut reader = Self {
            dataset,
            graph,
            side_rows: BTreeMap::new(),
        };
        for (s, p, o, g) in dataset
            .annotations_with_graph()
            .filter(|row| graph.matches(row.3))
        {
            if dataset
                .quads_for_pattern(Some(s), Some(p), Some(o), graph)
                .next()
                .is_none()
            {
                let row = reader.resolve(QuadIds { s, p, o, g });
                reader.side_rows.entry(s).or_default().push(row);
            }
        }
        for (s, o, _) in dataset
            .reifiers_with_graph()
            .filter(|row| graph.matches(row.2))
        {
            const REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
            let ordinary = dataset.term_id_by_iri(REIFIES).is_some_and(|p| {
                dataset
                    .quads_for_pattern(Some(s), Some(p), Some(o), graph)
                    .next()
                    .is_some()
            });
            if !ordinary {
                reader.side_rows.entry(s).or_default().push(ParsedTriple {
                    subject: s,
                    predicate: REIFIES,
                    object_id: o,
                    object: dataset.resolve(o),
                });
            }
        }
        for triple in reader.iter() {
            if !matches!(
                dataset.resolve(triple.subject),
                TermRef::Iri(_) | TermRef::Blank { .. }
            ) {
                return Err(result_err("graph/reasoning: non-node subject".into()));
            }
        }
        Ok(reader)
    }

    fn resolve(&self, quad: QuadIds) -> ParsedTriple<'a> {
        let TermRef::Iri(predicate) = self.dataset.resolve(quad.p) else {
            unreachable!("frozen RDF predicates are IRIs")
        };
        ParsedTriple {
            subject: quad.s,
            predicate,
            object_id: quad.o,
            object: self.dataset.resolve(quad.o),
        }
    }

    fn iter(&self) -> impl Iterator<Item = ParsedTriple<'a>> + '_ {
        self.dataset
            .quads_for_pattern(None, None, None, self.graph)
            .map(|quad| self.resolve(quad))
            .chain(
                self.side_rows
                    .values()
                    .flat_map(|rows| rows.iter().copied()),
            )
    }

    fn for_subject(&self, subject: TermId) -> impl Iterator<Item = ParsedTriple<'a>> + '_ {
        self.dataset
            .quads_for_pattern(Some(subject), None, None, self.graph)
            .map(|quad| self.resolve(quad))
            .chain(
                self.side_rows
                    .get(&subject)
                    .into_iter()
                    .flat_map(|rows| rows.iter().copied()),
            )
    }

    fn blank_label(&self, id: TermId) -> &str {
        let TermRef::Blank { label, .. } = self.dataset.resolve(id) else {
            unreachable!("blank link checked")
        };
        label
    }

    fn node_iri(&self, id: TermId) -> String {
        let TermRef::Iri(iri) = self.dataset.resolve(id) else {
            unreachable!("IRI link checked")
        };
        iri.to_owned()
    }
}

fn parse_nt(body: &str) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    purrdf::parse_dataset(body.as_bytes(), "application/n-triples", None)
        .map_err(|e| result_err(format!("graph/reasoning: N-Triples parse: {e}")))
}

// ── The conjecture verdict → attributed RDF projection ──────────────────────────

/// The placeholder subject the conjecture body is built against before its
/// content-addressed node IRI is known (substituted in the final pass, exactly as
/// [`RESULT_PLACEHOLDER`] is for the reasoning result).
const CONJECTURE_PLACEHOLDER: &str = "urn:gmeow:conjecture-verdict:self";

/// A `math:` IRI helper (the single math→logic twin edge this projection mints).
fn math(local: &str) -> String {
    format!("{MATH_NAMESPACE}{local}")
}

/// A `gmeow:` IRI helper (the provenance edges).
fn gmeow(local: &str) -> String {
    format!("{GMEOW_NAMESPACE}{local}")
}

/// The input to [`project_conjecture_verdict`]: the tested formula's identity, its
/// standpoint + KB-world scope, the engine-produced [`ConjectureAnswer`], and (when the
/// conjecture is the runtime twin of a `math:Conjecture`) the math statement node so the
/// refutation's counterexample can be re-exposed structurally via `math:hasCounterexample`.
pub struct ConjectureVerdictInput<'a> {
    /// The candidate formula's alpha-normalized `content_key` (formula identity, carried
    /// as the `logic:conjectureFormula` literal so identity stays queryable across
    /// standpoints).
    pub content_key: &'a str,
    /// The reified standpoint IRI the verdict is scoped to (REQUIRED — Principle 9).
    pub standpoint: &'a str,
    /// The KB world IRI/label the verdict was computed against; folded into the
    /// content-address as `sha256` so the same formula in two worlds mints two nodes.
    pub kb_world: &'a str,
    /// The engine-produced answer (verdict + witness + lifecycle + discharge).
    pub answer: &'a ConjectureAnswer,
    /// When the conjecture formalizes a `math:Conjecture`, that math statement's IRI. When
    /// `Some`, the always-present structural twin `<math_conjecture> math:conjectureUnderTest
    /// <conjecture-node>` is emitted for EVERY verdict (corroborated / refuted / open /
    /// withdrawn); additionally the refutation-only `<math_conjecture> math:hasCounterexample
    /// <witness>` edge is emitted iff the answer also carries a refutation witness.
    pub math_conjecture: Option<&'a str>,
    /// The candidate formula's PRINCIPAL predicate IRI (its
    /// `Formula::principal_predicate`, in `gmeow-logic-compile`),
    /// the sound `logic:obligationForbiddenPredicate` of the anti-conjecture
    /// `logic:NonEntailmentObligation` a REFUTED conjecture proposes. REQUIRED (must be
    /// `Some`) exactly for a [`ConjectureLifecycleState::RefutedInStandpoint`] answer whose
    /// formula names a single predicate; the caller hard-fails (never fabricates one) for a
    /// refuted *compound* formula that names no single predicate. Unused for the corroborated
    /// / open / withdrawn legs.
    pub forbidden_predicate: Option<&'a str>,
}

/// Project an engine-produced [`ConjectureAnswer`] into deterministic, sorted N-Triples:
/// one content-addressed `logic:Conjecture` node (keyed on the formula `content_key`, the
/// standpoint, and the KB-world hash — so the SAME formula tested in two standpoints or
/// two worlds mints two DISTINCT nodes, Principle 9 / no silent overwrite), the embedded
/// `logic:ReasoningResult` graph its verdict was read from (its OWN content-addressed
/// node, linked via `logic:conjectureVerdict`), the refutation witness (when present), the
/// provenance edges, the always-present `math:conjectureUnderTest` twin bridge (when a math
/// statement is named), and the refutation-only `math:hasCounterexample` twin edge.
pub fn project_conjecture_verdict(input: &ConjectureVerdictInput) -> gmeow_errors::Result<String> {
    Ok(ResultProjection::conjecture(input)?.to_ntriples())
}

fn conjecture_projection_sink(
    input: &ConjectureVerdictInput,
) -> gmeow_errors::Result<(Sink, String)> {
    let answer = input.answer;
    let result_projection = ResultProjection::reasoning(&answer.verdict)?;
    let result_iri = result_projection.node_iri;

    let mut sink = Sink::default();
    let subject = Node::SelfReference(CONJECTURE_PLACEHOLDER.to_owned());

    sink.push(subject.clone(), RDF_TYPE, Node::iri(logic("Conjecture")));
    sink.push(
        subject.clone(),
        logic("conjectureFormula"),
        Node::string(input.content_key.to_owned()),
    );
    sink.push(
        subject.clone(),
        logic("conjectureStandpoint"),
        Node::iri(input.standpoint.to_owned()),
    );
    sink.push(
        subject.clone(),
        logic("conjectureKbWorldHash"),
        Node::string(sha256_hex(input.kb_world)),
    );
    sink.push(
        subject.clone(),
        logic("conjectureLifecycleState"),
        Node::iri(answer.lifecycle.iri()),
    );
    // A conjecture's defining mark against a plain candidate: its cases are engine-produced.
    sink.push(
        subject.clone(),
        logic("verdictProvenance"),
        Node::iri(logic("VerdictEngineProduced")),
    );
    sink.push(
        subject.clone(),
        logic("conjectureDischargeVerdict"),
        Node::iri(answer.discharge.iri()),
    );
    sink.push(
        subject.clone(),
        logic("conjectureVerdict"),
        Node::iri(result_iri.clone()),
    );

    // The always-present structural twin bridge: whenever this conjecture formalizes a
    // `math:Conjecture`, the statement-layer object is linked to THIS runtime-testable
    // `logic:Conjecture` node via `math:conjectureUnderTest` (domain math:Conjecture, range
    // logic:Conjecture — a math→logic edge, permitted because math: is last in the acyclic
    // grounding-layer order). Unlike the refutation-only `math:hasCounterexample` witness
    // edge, this twin is emitted for corroborated, refuted, open, and withdrawn verdicts
    // alike — any time a math twin is named — so the statement always resolves to the node
    // carrying its standpoint-scoped verdict.
    if let Some(math_conjecture) = input.math_conjecture {
        sink.push(
            Node::iri(math_conjecture.to_owned()),
            math("conjectureUnderTest"),
            subject.clone(),
        );
    }

    // The refutation witness (present exactly for a RefutedInStandpoint verdict): a
    // `logic:ContradictionWitness` node linked via `logic:conjectureRefutationWitness`,
    // its body byte-identical to the reasoning-result witness idiom.
    if let Some(witness) = &answer.witness {
        let node = Node::blank(sink.fresh_blank("witness"));
        sink.push(
            subject.clone(),
            logic("conjectureRefutationWitness"),
            node.clone(),
        );
        emit_witness_body(&mut sink, &node, witness);
        // The two-projection witness: the SAME node the logic: refutation exposes is the
        // math: counterexample, attached to the math statement via math:hasCounterexample.
        if let Some(math_conjecture) = input.math_conjecture {
            sink.push(
                Node::iri(math_conjecture.to_owned()),
                math("hasCounterexample"),
                node.clone(),
            );
        }
    }

    // Provenance: the verdict was generated by the conjecture-test activity and derived
    // from the reasoning-result node it was read from. Guard against a self-attestation
    // edge: the derived-from source must be a DISTINCT node, never a conjecture node
    // (the result node is content-addressed off a distinct base, so a `wasDerivedFrom`
    // pointing back at a conjecture is refused rather than fabricated).
    sink.push(
        subject.clone(),
        gmeow("wasGeneratedBy"),
        Node::iri(CONJECTURE_ACTIVITY.to_owned()),
    );
    if !result_iri.starts_with(CONJECTURE_IRI_BASE) {
        sink.push(
            subject.clone(),
            gmeow("wasDerivedFrom"),
            Node::iri(&result_iri),
        );
    }

    // ── The two symmetric promotion legs (LOGIC-FOUNDATION.md §"Two symmetric promotion
    // legs"). A conjecture that survives feeds forward, and so does one that dies. Each leg
    // is emitted EXACTLY on its epistemic lifecycle — corroborated → the POSITIVE promotion
    // leg only; refuted-in-standpoint → the SYMMETRIC anti-conjecture obligation leg only;
    // open / withdrawn → neither — matching each vocabulary term's "present exactly when …"
    // wording. Each target is a content-addressed node keyed on the SAME (formula ×
    // standpoint × KB-world) identity coordinates as the conjecture node, minted with the
    // carriers its own SHACL shape requires so it is well-formed, never a bare typed stub.
    match answer.lifecycle {
        ConjectureLifecycleState::Corroborated => {
            let node = Node::iri(promotion_candidate_iri(input));
            sink.push(
                subject.clone(),
                logic("conjecturePromotionCandidate"),
                node.clone(),
            );
            emit_promotion_candidate_body(&mut sink, &node, input);
        }
        ConjectureLifecycleState::RefutedInStandpoint => {
            // A refuted conjecture forbids its formula: the anti-conjecture obligation names
            // the refuted claim's principal predicate as the one the closure must never draw.
            // The caller (`run_conjecture_test`) guarantees this is `Some` for a refuted
            // answer whose formula names a single predicate, and hard-fails otherwise — so a
            // missing predicate here is a broken caller contract, never a fabricated node.
            let forbidden = input.forbidden_predicate.ok_or_else(|| {
                result_err("a refuted conjecture requires its formula's principal predicate".into())
            })?;
            let node = Node::iri(obligation_candidate_iri(input));
            sink.push(
                subject.clone(),
                logic("antiConjectureObligationCandidate"),
                node.clone(),
            );
            emit_obligation_candidate_body(&mut sink, &node, forbidden);
        }
        ConjectureLifecycleState::Open | ConjectureLifecycleState::Withdrawn => {}
    }

    let (mut sink, node_iri) =
        content_addressed_sink(sink, CONJECTURE_IRI_BASE, CONJECTURE_PLACEHOLDER);
    // Component scopes belong to their own result. Combining projections must
    // never merge local blank labels from two independently minted records.
    let scoped = |node: Node, owner: &str| match node {
        Node::Blank(label) => Node::iri(format!("{owner}/component/{label}")),
        other => other,
    };
    for triple in &mut sink.triples {
        triple.subject = scoped(triple.subject.clone(), &node_iri);
        triple.object = scoped(triple.object.clone(), &node_iri);
    }
    for triple in result_projection.sink.triples {
        sink.push(
            scoped(triple.subject, &result_iri),
            triple.predicate,
            scoped(triple.object, &result_iri),
        );
    }
    Ok((sink, node_iri))
}

/// Project an AUTHOR-driven conjecture WITHDRAWAL onto an EXISTING library node.
///
/// The compensating counterpart of [`project_conjecture_verdict`] (P10): where the verdict
/// projection MINTS a fresh content-addressed node from an ENGINE verdict — and hardcodes
/// `logic:verdictProvenance logic:VerdictEngineProduced` — this emits a small compensating
/// segment against a `logic:Conjecture` node IRI that ALREADY exists in the append-only
/// library. It flips the effective epistemic state to `logic:ConjectureWithdrawn`, records
/// the author's withdrawal `reason` (when non-empty), and marks the case
/// `logic:VerdictReviewerAsserted` — a withdrawal is an author action, NEVER engine-produced
/// (module.ttl's `logic:ConjectureWithdrawn`). The node is NOT re-typed (it is already
/// `rdf:type logic:Conjecture` from its store segment) and NO timestamp is carried: the
/// conjecture node graph stays timeless, exactly as [`project_conjecture_verdict`] emits it,
/// and the deterministic time rides the trajectory-audit segment written alongside. The body
/// is the CLOSED, sorted N-Triples subset the library segment writer parses.
pub fn project_conjecture_withdrawal(node_iri: &str, reason: &str) -> String {
    let mut sink = Sink::default();
    let subject = Node::iri(node_iri.to_owned());
    sink.push(
        subject.clone(),
        logic("conjectureLifecycleState"),
        Node::iri(logic("ConjectureWithdrawn")),
    );
    sink.push(
        subject.clone(),
        logic("verdictProvenance"),
        Node::iri(logic("VerdictReviewerAsserted")),
    );
    if !reason.is_empty() {
        sink.push(
            subject,
            logic("withdrawalReason"),
            Node::string(reason.to_owned()),
        );
    }
    let mut out = String::new();
    for line in sink.render_lines() {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// The content-address digest a promotion / obligation candidate node is keyed on: the
/// SAME `(content_key × standpoint × KB-world)` identity coordinates as the conjecture node
/// (Principle 9 — one candidate per formula-in-a-standpoint-in-a-world), so the leg target
/// is deterministic and re-derivable without re-parsing the body. The `\u{1}` separator is a
/// term the IRIs / keys never contain, so the concatenation is injective.
fn candidate_identity_digest(input: &ConjectureVerdictInput) -> String {
    sha256_hex(&format!(
        "{}\u{1}{}\u{1}{}",
        input.content_key, input.standpoint, input.kb_world
    ))
}

/// The deterministic IRI of the POSITIVE promotion leg's `logic:FormalizationCandidate`.
fn promotion_candidate_iri(input: &ConjectureVerdictInput) -> String {
    format!(
        "{PROMOTION_CANDIDATE_IRI_BASE}{}",
        candidate_identity_digest(input)
    )
}

/// The deterministic IRI of the anti-conjecture leg's `logic:NonEntailmentObligation`.
fn obligation_candidate_iri(input: &ConjectureVerdictInput) -> String {
    format!(
        "{OBLIGATION_CANDIDATE_IRI_BASE}{}",
        candidate_identity_digest(input)
    )
}

/// Emit the POSITIVE promotion leg's target: the `logic:FormalizationCandidate` proposing to
/// promote a CORROBORATED conjecture's formula to a canonical axiom. It carries the eight
/// universal candidate carriers `logic:FormalizationCandidateShape` requires so the node is
/// well-formed (never a bare typed stub), populated deterministically as a `logic:Candidate-
/// Proposed` candidate a reviewer STILL adjudicates — corroboration is provisional support,
/// never proof (design/LOGIC-FOUNDATION.md §"Two symmetric promotion legs"):
///  - source hash / extraction provenance / scope anchor it to the conjectured formula and
///    the engine run that produced it (SOUND: pure provenance of THIS corroboration);
///  - `logic:StratifiedNAFProfile` is the reasoning contract the conjecture chase runs under;
///  - `logic:CandidateProposed` is the entry lifecycle every automated extraction enters at;
///  - `logic:RiskCoreContaminating` is the honest worst-case for a promotion INTO the core,
///    which keeps the reviewer gate maximally strict;
///  - `logic:CategoryDerivationRule` / `logic:SoundUnderApproximation` follow the established
///    convention every machine-harvested axiom candidate in `module.ttl` records, refinable
///    by the reviewer the candidate is routed to.
fn emit_promotion_candidate_body(sink: &mut Sink, node: &Node, input: &ConjectureVerdictInput) {
    sink.push(
        node.clone(),
        RDF_TYPE,
        Node::iri(logic("FormalizationCandidate")),
    );
    sink.push(
        node.clone(),
        logic("candidateSourceHash"),
        Node::string(format!("sha256:{}", sha256_hex(input.content_key))),
    );
    sink.push(
        node.clone(),
        logic("candidateExtractionProvenance"),
        Node::string(format!(
            "engine-produced by the conjecture-test activity <{CONJECTURE_ACTIVITY}>: a \
             corroboration scoped to standpoint <{}> against KB-world-hash {}",
            input.standpoint,
            sha256_hex(input.kb_world)
        )),
    );
    sink.push(
        node.clone(),
        logic("candidateScope"),
        Node::string(input.content_key.to_owned()),
    );
    sink.push(
        node.clone(),
        logic("candidateContract"),
        Node::iri(logic("StratifiedNAFProfile")),
    );
    sink.push(
        node.clone(),
        logic("candidateCategory"),
        Node::iri(logic("CategoryDerivationRule")),
    );
    sink.push(
        node.clone(),
        logic("candidateLifecycle"),
        Node::iri(logic("CandidateProposed")),
    );
    sink.push(
        node.clone(),
        logic("candidateProjectionBehavior"),
        Node::iri(logic("SoundUnderApproximation")),
    );
    sink.push(
        node.clone(),
        logic("candidateSemanticRisk"),
        Node::iri(logic("RiskCoreContaminating")),
    );
}

/// Emit the anti-conjecture leg's target: the candidate `logic:NonEntailmentObligation` a
/// REFUTED conjecture proposes, forbidding its formula. It carries the two carriers
/// `logic:NonEntailmentObligationShape` requires so the node is well-formed:
///  - `logic:obligationForbiddenPredicate` — the refuted formula's principal predicate (the
///    predicate the closure must never draw), passed in from the caller's `Formula`;
///  - `logic:obligationDischargeCondition logic:DischargeFiniteClosure` — the engine-wired
///    condition the obligation is conclusively checkable under, matching exactly HOW the
///    refutation was found (a contradiction in the materialized finite closure of the
///    isolated scenario world).
fn emit_obligation_candidate_body(sink: &mut Sink, node: &Node, forbidden_predicate: &str) {
    sink.push(
        node.clone(),
        RDF_TYPE,
        Node::iri(logic("NonEntailmentObligation")),
    );
    sink.push(
        node.clone(),
        logic("obligationForbiddenPredicate"),
        Node::any_uri(forbidden_predicate.to_owned()),
    );
    sink.push(
        node.clone(),
        logic("obligationDischargeCondition"),
        Node::iri(logic("DischargeFiniteClosure")),
    );
}

/// The content-addressed conjecture-node IRI for `input` — the subject the projection
/// mints (useful for a consumer that wants the node IRI without re-parsing the body).
pub fn conjecture_node_iri(input: &ConjectureVerdictInput) -> gmeow_errors::Result<String> {
    Ok(ResultProjection::conjecture(input)?.node_iri)
}

/// The faithful re-read of a `project_conjecture_verdict` body: the conjecture's identity
/// (formula `content_key`, standpoint, KB-world hash), its epistemic lifecycle + discharge
/// verdict, the embedded reasoning-result [`ReasoningResult`] (via [`parse_reasoning_graph`]),
/// and the refutation witness (when present).
#[derive(Debug, Clone, PartialEq)]
pub struct ConjectureVerdictRecord {
    /// The candidate formula's alpha-normalized `content_key`.
    pub content_key: String,
    /// The standpoint IRI the verdict is scoped to.
    pub standpoint: String,
    /// The `sha256` of the KB world the verdict was computed against.
    pub kb_world_hash: String,
    /// The epistemic lifecycle state.
    pub lifecycle: ConjectureLifecycleState,
    /// The conclusiveness carrier (Discharged | Unknown).
    pub discharge: ConjectureDischarge,
    /// The embedded verdict result graph, re-derived to verdict + provenance.
    pub verdict: ReasoningResult,
    /// The refutation witness (present exactly for a RefutedInStandpoint verdict).
    pub witness: Option<ContradictionWitness>,
    /// The `math:Conjecture` statement IRI this node is the runtime twin of (present exactly
    /// when the body carries a `<math> math:conjectureUnderTest <this-node>` bridge edge).
    pub math_conjecture: Option<String>,
    /// The POSITIVE promotion leg (present exactly for a Corroborated verdict): the
    /// `logic:FormalizationCandidate` linked via `logic:conjecturePromotionCandidate`.
    pub promotion_candidate: Option<PromotionCandidateRecord>,
    /// The SYMMETRIC anti-conjecture leg (present exactly for a RefutedInStandpoint verdict):
    /// the candidate `logic:NonEntailmentObligation` linked via
    /// `logic:antiConjectureObligationCandidate`.
    pub obligation_candidate: Option<ObligationCandidateRecord>,
}

/// The re-read POSITIVE promotion leg: the `logic:FormalizationCandidate` a corroborated
/// conjecture proposes, with its node IRI and the eight universal candidate carriers.
#[derive(Debug, Clone, PartialEq)]
pub struct PromotionCandidateRecord {
    /// The content-addressed candidate node IRI.
    pub node: String,
    /// `logic:candidateSourceHash` (the `sha256:` content hash of the conjectured formula).
    pub source_hash: String,
    /// `logic:candidateExtractionProvenance` (what produced it — the conjecture-test run).
    pub extraction_provenance: String,
    /// `logic:candidateScope` (the formula the proposed axiom would constrain).
    pub scope: String,
    /// `logic:candidateContract` (the reasoning contract IRI the conjecture ran under).
    pub contract: String,
    /// `logic:candidateCategory` (the formalization-category IRI).
    pub category: String,
    /// `logic:candidateLifecycle` (the governance lifecycle IRI — `CandidateProposed`).
    pub lifecycle: String,
    /// `logic:candidateProjectionBehavior` (the preservation-kind IRI).
    pub projection_behavior: String,
    /// `logic:candidateSemanticRisk` (the semantic-risk IRI).
    pub semantic_risk: String,
}

/// The re-read anti-conjecture leg: the candidate `logic:NonEntailmentObligation` a refuted
/// conjecture proposes, with its node IRI, forbidden predicate, and discharge conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct ObligationCandidateRecord {
    /// The content-addressed obligation node IRI.
    pub node: String,
    /// `logic:obligationForbiddenPredicate` (the predicate the closure must never draw).
    pub forbidden_predicate: String,
    /// `logic:obligationDischargeCondition` IRIs (sorted), the conditions it is checkable under.
    pub discharge_conditions: Vec<String>,
}

/// Re-read a `project_conjecture_verdict` N-Triples body back into a
/// [`ConjectureVerdictRecord`] (the inverse round-trip). Mirrors [`parse_reasoning_graph`]'s
/// line-parsing idiom and reuses it for the embedded result graph.
///
/// # Errors
/// Returns `Err` if the body is missing the `logic:Conjecture` subject, a required scalar
/// (formula / standpoint / KB-world hash) is absent, a lifecycle / discharge IRI is
/// unrecognized, or the embedded reasoning-result graph does not parse (fail-closed).
pub fn parse_conjecture_verdict(nt_body: &str) -> gmeow_errors::Result<ConjectureVerdictRecord> {
    let dataset = parse_nt(nt_body)?;
    let triples = ResultGraph::new(&dataset, GraphMatch::Default)?;
    let subject = triples
        .iter()
        .find(|t| t.predicate == RDF_TYPE && t.object_iri() == Some(logic("Conjecture")))
        .map(|t| t.subject)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Result {
                detail: "graph/conjecture: no logic:Conjecture subject".to_owned(),
            })
        })?;

    let one_iri = |local: &str| -> Option<String> {
        triples
            .for_subject(subject)
            .find(|t| t.predicate == logic(local))
            .and_then(|t| t.object_iri())
    };
    let one_str = |local: &str| -> Option<String> {
        triples
            .for_subject(subject)
            .find(|t| t.predicate == logic(local))
            .and_then(|t| t.object_string())
    };

    let content_key = req(one_str("conjectureFormula"), "conjectureFormula")?;
    let standpoint = req(one_iri("conjectureStandpoint"), "conjectureStandpoint")?;
    let kb_world_hash = req(one_str("conjectureKbWorldHash"), "conjectureKbWorldHash")?;
    let lifecycle = ConjectureLifecycleState::from_local(local_of(&req(
        one_iri("conjectureLifecycleState"),
        "conjectureLifecycleState",
    )?)?)
    .ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Result {
            detail: "graph/conjecture: unrecognized conjectureLifecycleState".to_owned(),
        })
    })?;
    let discharge = ConjectureDischarge::from_local(local_of(&req(
        one_iri("conjectureDischargeVerdict"),
        "conjectureDischargeVerdict",
    )?)?)
    .ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Result {
            detail: "graph/conjecture: unrecognized conjectureDischargeVerdict".to_owned(),
        })
    })?;

    // The embedded reasoning-result graph re-derives via the existing reader (it keys off
    // the logic:ReasoningResult subject, so the conjecture node's triples do not interfere).
    let verdict = read_reasoning_result(&triples)?;

    // The refutation witness (linked via conjectureRefutationWitness, not resultContradiction).
    let witness = optional_component(
        &triples,
        subject,
        "conjectureRefutationWitness",
        "ContradictionWitness",
    )?
    .map(|node| parse_witness_body(&triples, node))
    .transpose()?;

    // The always-present structural twin bridge: the `math:Conjecture` whose
    // `math:conjectureUnderTest` edge names THIS `logic:Conjecture` node as its object.
    let math_conjecture = triples
        .iter()
        .find(|t| t.predicate == math("conjectureUnderTest") && t.object_node() == Some(subject))
        .map(|t| match dataset.resolve(t.subject) {
            TermRef::Iri(iri) => Ok(iri.to_owned()),
            _ => Err(result_err(
                "graph/conjecture: math:conjectureUnderTest subject must be an IRI".to_owned(),
            )),
        })
        .transpose()?;

    // The two symmetric promotion legs (present exactly on their lifecycle).
    let promotion_candidate = parse_promotion_candidate(&triples, subject);
    let obligation_candidate = parse_obligation_candidate(&triples, subject);

    Ok(ConjectureVerdictRecord {
        content_key,
        standpoint,
        kb_world_hash,
        lifecycle,
        discharge,
        verdict,
        witness,
        math_conjecture,
        promotion_candidate,
        obligation_candidate,
    })
}

/// Re-read the POSITIVE promotion leg from a parsed body: the `logic:FormalizationCandidate`
/// linked from `subject` via `logic:conjecturePromotionCandidate`, together with its eight
/// universal candidate carriers. `None` when no such edge is present.
fn parse_promotion_candidate(
    triples: &ResultGraph<'_>,
    subject: TermId,
) -> Option<PromotionCandidateRecord> {
    let node = triples
        .for_subject(subject)
        .find(|t| t.predicate == logic("conjecturePromotionCandidate"))
        .and_then(|t| t.object_node())?;
    let carrier_iri = |local: &str| -> String {
        triples
            .for_subject(node)
            .find(|t| t.predicate == logic(local))
            .and_then(|t| t.object_iri())
            .unwrap_or_default()
    };
    let carrier_str = |local: &str| -> String {
        triples
            .for_subject(node)
            .find(|t| t.predicate == logic(local))
            .and_then(|t| t.object_string())
            .unwrap_or_default()
    };
    Some(PromotionCandidateRecord {
        source_hash: carrier_str("candidateSourceHash"),
        extraction_provenance: carrier_str("candidateExtractionProvenance"),
        scope: carrier_str("candidateScope"),
        contract: carrier_iri("candidateContract"),
        category: carrier_iri("candidateCategory"),
        lifecycle: carrier_iri("candidateLifecycle"),
        projection_behavior: carrier_iri("candidateProjectionBehavior"),
        semantic_risk: carrier_iri("candidateSemanticRisk"),
        node: triples.node_iri(node),
    })
}

/// Re-read the anti-conjecture leg from a parsed body: the candidate
/// `logic:NonEntailmentObligation` linked from `subject` via
/// `logic:antiConjectureObligationCandidate`, with its forbidden predicate and (sorted)
/// discharge conditions. `None` when no such edge is present.
fn parse_obligation_candidate(
    triples: &ResultGraph<'_>,
    subject: TermId,
) -> Option<ObligationCandidateRecord> {
    let node = triples
        .for_subject(subject)
        .find(|t| t.predicate == logic("antiConjectureObligationCandidate"))
        .and_then(|t| t.object_node())?;
    let forbidden_predicate = triples
        .for_subject(node)
        .find(|t| t.predicate == logic("obligationForbiddenPredicate"))
        .and_then(|t| t.object_string())
        .unwrap_or_default();
    let mut discharge_conditions: Vec<String> = triples
        .for_subject(node)
        .filter(|t| t.predicate == logic("obligationDischargeCondition"))
        .filter_map(|t| t.object_iri())
        .collect();
    discharge_conditions.sort();
    Some(ObligationCandidateRecord {
        node: triples.node_iri(node),
        forbidden_predicate,
        discharge_conditions,
    })
}

/// Parse the internal triples of a `logic:ContradictionWitness` node (its
/// `witnessIndividual` / `witnessWorld` / `witnessPremise` set) into a
/// [`ContradictionWitness`]. Shared inverse of [`emit_witness_body`].
fn parse_witness_body(
    triples: &ResultGraph<'_>,
    node: TermId,
) -> gmeow_errors::Result<ContradictionWitness> {
    let individual = component_field(triples, node, "witnessIndividual")?
        .object_iri()
        .ok_or_else(|| result_err("graph/reasoning: witnessIndividual must be an IRI".into()))?;
    let world = component_field(triples, node, "witnessWorld")?
        .object_iri()
        .ok_or_else(|| result_err("graph/reasoning: witnessWorld must be an IRI".into()))?;
    let mut premises = Vec::new();
    for t in triples
        .for_subject(node)
        .filter(|t| t.predicate == logic("witnessPremise"))
    {
        let value = component_text(triples, t)?;
        let mut parts = value.splitn(3, ' ');
        let (Some(subject), Some(predicate), Some(object)) =
            (parts.next(), parts.next(), parts.next())
        else {
            return Err(result_err(
                "graph/reasoning: witnessPremise must retain its three source components".into(),
            ));
        };
        premises.push((subject.to_owned(), predicate.to_owned(), object.to_owned()));
    }
    Ok(ContradictionWitness {
        individual,
        world,
        premises,
    })
}

#[cfg(test)]
mod resource_tests;

#[cfg(test)]
mod tests;
