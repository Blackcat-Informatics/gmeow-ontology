// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The deterministic RDF projection of a typed [`ReasoningResult`] into the
//! `graph/reasoning` named graph (#1132 C7).
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
//!   `logic:resultDerivedAxiom` reified statements (the closure itself is the reason
//!   stage's `dataset`, so the graph/reasoning projection records the *shape* of the
//!   answer, not a second copy of every closure quad).
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
//! and the payload **discriminant**. What does NOT round-trip: the payload *contents*
//! of the `Bindings` / `Marginals` surfaces and the per-axiom premise lists of an
//! `Inferred` closure — those rows live in the reason stage's closure dataset (the
//! bundle's default graph), not re-copied here. So the re-derived result carries an
//! `Empty`/`Inferred(derived-only)` payload faithful for the *handle's* purpose (a
//! consumer reads the verdict + provenance, and reads the closure from the dataset),
//! and the parser is documented as reconstructing the verdict-and-provenance result,
//! not the original bindings. This mirrors the C6 precedent: exact where it holds
//! (axes/provenance), faithful-subset where the payload rows are carried elsewhere.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use gmeow_logic_compile::ir::{PreservationKind, LOGIC_NAMESPACE};
use sha2::{Digest, Sha256};

use crate::reason::el::InferredAxiom;
use crate::result::{
    Assumption, BudgetLimit, CompletenessStatus, ContradictionWitness, DerivationRef, EngineId,
    EvaluationStatus, InformationState, InputStatus, PreservationClaim, ReasoningResult,
    ResultContext, ResultPayload, ResultProvenance,
};

/// The `graph/reasoning` named-graph IRI — the snapshot folds this projection here.
pub const GRAPH_REASONING: &str = "https://blackcatinformatics.ca/gmeow/graph/reasoning";
/// The content-addressed result-node IRI base (`+ sha256(body)`).
const RESULT_IRI_BASE: &str = "https://blackcatinformatics.ca/gmeow/graph/reasoning/result/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// `logic:` IRI helper. All container/structure predicates this projection mints
/// live in the logic namespace (consistent with the axis individuals), so the
/// projection never collides with the gmeow vocabulary surface.
fn logic(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

/// A node term for N-Triples emission: an IRI, a blank node, or a typed literal.
#[derive(Clone)]
enum Node {
    Iri(String),
    Blank(String),
    Lit { lex: String, datatype: String },
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
    /// Render this node in canonical N-Triples term syntax.
    fn render(&self) -> String {
        match self {
            Node::Iri(iri) => format!("<{iri}>"),
            Node::Blank(id) => format!("_:{id}"),
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
            _ => out.push(ch),
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
        let mut lines: BTreeSet<String> = BTreeSet::new();
        for t in &self.triples {
            lines.insert(format!(
                "{} <{}> {} .",
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

/// Project a [`ReasoningResult`] into the deterministic `graph/reasoning` N-Triples
/// body (a `String`; one sorted canonical triple per line, trailing newline).
///
/// The result node is content-addressed: its IRI is [`RESULT_IRI_BASE`] + the
/// `sha256` (hex) of the placeholder-subjected body, so a structurally-equal result
/// mints the same node and a single result is byte-reproducible.
pub fn project_reasoning_result(result: &ReasoningResult) -> String {
    let mut sink = Sink::default();
    let subject = Node::iri(RESULT_PLACEHOLDER);

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
    project_provenance(&mut sink, &subject, &result.provenance);

    // ── payload summary ──────────────────────────────────────────────────────────
    project_payload(&mut sink, &subject, &result.payload);

    finalize(sink)
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
fn project_provenance(sink: &mut Sink, subject: &Node, prov: &ResultProvenance) {
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
    for witness in &prov.contradiction_witnesses {
        project_witness(sink, subject, witness);
    }
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
/// blank-node rows (subject/predicate/object + world). The full closure (incl. EDB)
/// is the reason stage's dataset; here we record only the *derived* shape so the
/// graph/reasoning projection summarizes the answer without duplicating every quad.
fn project_derived_axioms(sink: &mut Sink, subject: &Node, axioms: &[InferredAxiom]) {
    // Build a sorted, deduplicated set of derived rows so emission is deterministic
    // regardless of the closure's internal ordering.
    let mut rows: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    for ax in axioms {
        if ax.is_edb {
            continue;
        }
        rows.insert((
            ax.subject.clone(),
            ax.predicate.clone(),
            ax.object.clone(),
            ax.world.clone(),
        ));
    }
    for (s, p, o, w) in rows {
        let node = Node::blank(sink.fresh_blank("axiom"));
        sink.push(subject.clone(), logic("resultDerivedAxiom"), node.clone());
        sink.push(node.clone(), RDF_TYPE, Node::iri(logic("DerivedAxiom")));
        sink.push(node.clone(), logic("axiomSubject"), axiom_term(&s));
        sink.push(node.clone(), logic("axiomPredicate"), axiom_term(&p));
        sink.push(node.clone(), logic("axiomObject"), axiom_term(&o));
        sink.push(node.clone(), logic("axiomWorld"), axiom_term(&w));
    }
}

/// Normalize a native-engine term string (`<iri>` / `_:b` / `"lit"…` / bare-iri) into
/// a projection [`Node`]. The native chase emits subject/object/world in N3 term form
/// (a surrounding `<>` for IRIs); a bare IRI (no brackets) is treated as an IRI too.
/// Literals are carried as `xsd:string` (the projection records the answer *shape*,
/// and the full closure with exact datatypes rides the reason stage's dataset).
fn axiom_term(value: &str) -> Node {
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

/// Substitute the content-addressed subject IRI into the placeholder-built body and
/// emit the sorted, trailing-newline N-Triples document.
fn finalize(sink: Sink) -> String {
    let lines = sink.render_lines();
    // Content-address the node off the placeholder-subjected body (so two equal
    // results mint the same node; one result is reproducible).
    let mut hasher = Sha256::new();
    for line in &lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    let node_iri = format!("{RESULT_IRI_BASE}{hex}");
    let placeholder = format!("<{RESULT_PLACEHOLDER}>");
    let real = format!("<{node_iri}>");

    let mut out = String::new();
    // Substitute then RE-SORT: the subject swap changes the leading term, so the
    // pre-substitution sort no longer holds. Re-sorting keeps the bytes canonical.
    let mut substituted: Vec<String> = lines
        .iter()
        .map(|l| l.replace(&placeholder, &real))
        .collect();
    substituted.sort();
    for line in substituted {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// The content-addressed result-node IRI for `result` (the subject the projection
/// mints). Useful for a consumer that wants the node IRI without re-parsing.
pub fn result_node_iri(result: &ReasoningResult) -> String {
    let body = project_reasoning_result(result);
    // The node IRI is the unique `RESULT_IRI_BASE`-prefixed subject in the body.
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix(&format!("<{RESULT_IRI_BASE}")) {
            if let Some(end) = rest.find('>') {
                return format!("{RESULT_IRI_BASE}{}", &rest[..end]);
            }
        }
    }
    // Unreachable: the type triple always carries the node subject.
    RESULT_IRI_BASE.to_owned()
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
/// preservation claim, and the entire provenance bundle exactly; the payload is
/// reconstructed to its DISCRIMINANT with an empty/derived-only body (the binding /
/// marginal rows and the per-axiom premise lists are NOT carried in this graph — they
/// live in the reason stage's closure dataset). The re-derived result is the handle a
/// consumer needs: it reads the verdict + provenance from here and the closure quads
/// from the dataset. See the module docs for the exact round-trip contract.
///
/// # Errors
/// Returns `Err` if the body is missing the result subject, an axis IRI is
/// unrecognized, or a required scalar provenance field is absent (fail-closed).
pub fn parse_reasoning_graph(nt_body: &str) -> Result<ReasoningResult, String> {
    let triples = parse_nt(nt_body)?;
    // The single subject typed logic:ReasoningResult.
    let subject = triples
        .iter()
        .find(|t| t.predicate == RDF_TYPE && t.object_iri() == Some(logic("ReasoningResult")))
        .map(|t| t.subject.clone())
        .ok_or_else(|| "graph/reasoning: no logic:ReasoningResult subject".to_owned())?;

    let one_iri = |local: &str| -> Option<String> {
        triples
            .iter()
            .find(|t| t.subject == subject && t.predicate == logic(local))
            .and_then(|t| t.object_iri())
    };
    let one_str = |local: &str| -> Option<String> {
        triples
            .iter()
            .find(|t| t.subject == subject && t.predicate == logic(local))
            .and_then(|t| t.object_string())
    };

    let input = InputStatus::from_local(local_of(&req(one_iri("resultInput"), "resultInput")?)?)
        .ok_or_else(|| "graph/reasoning: unrecognized resultInput".to_owned())?;
    let evaluation = EvaluationStatus::from_local(local_of(&req(
        one_iri("resultEvaluation"),
        "resultEvaluation",
    )?)?)
    .ok_or_else(|| "graph/reasoning: unrecognized resultEvaluation".to_owned())?;
    let completeness = CompletenessStatus::from_local(local_of(&req(
        one_iri("resultCompleteness"),
        "resultCompleteness",
    )?)?)
    .ok_or_else(|| "graph/reasoning: unrecognized resultCompleteness".to_owned())?;
    let information = InformationState::from_local(local_of(&req(
        one_iri("resultInformation"),
        "resultInformation",
    )?)?)
    .ok_or_else(|| "graph/reasoning: unrecognized resultInformation".to_owned())?;

    // preservation: the polarity set + unsupported constructs.
    let mut preservation = PreservationClaim::default();
    for t in &triples {
        if t.subject == subject && t.predicate == logic("resultPreservationPolarity") {
            if let Some(iri) = t.object_iri() {
                if let Some(kind) = preservation_from_iri(&iri) {
                    preservation.polarities.insert(kind);
                }
            }
        }
        if t.subject == subject && t.predicate == logic("resultUnsupportedConstruct") {
            if let Some(s) = t.object_string() {
                preservation.unsupported_constructs.insert(s);
            }
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
    prov.query = one_str("resultQuery").unwrap_or_default();
    prov.conclusion = one_str("resultConclusion").unwrap_or_default();
    prov.proof = parse_derivation(&triples, &subject, "resultProof");
    prov.counterproof = parse_derivation(&triples, &subject, "resultCounterproof");
    prov.context = ResultContext {
        world,
        standpoint: one_iri("resultStandpoint"),
        time: one_str("resultTime"),
        path: one_iri("resultPath"),
    };
    prov.engine = EngineId {
        name: req(one_str("resultEngineName"), "resultEngineName")?,
        version: req(one_str("resultEngineVersion"), "resultEngineVersion")?,
    };
    prov.consumed_budget = crate::result::BudgetUsage {
        consumed: one_str("resultBudgetConsumed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        allowance: one_str("resultBudgetAllowance").and_then(|s| s.parse().ok()),
        limit: one_str("resultBudgetLimit").and_then(|s| budget_limit_from_wire(&s)),
    };
    prov.certified_fragment = one_iri("resultCertifiedFragment");
    prov.projection_class = preservation.clone();
    for t in &triples {
        if t.subject == subject && t.predicate == logic("resultAssumption") {
            if let Some(iri) = t.object_iri() {
                if let Some(a) = assumption_from_iri(&iri) {
                    prov.assumptions.insert(a);
                }
            }
        }
    }
    prov.contradiction_witnesses = parse_witnesses(&triples, &subject);
    prov.contradiction_witnesses.sort();

    // payload discriminant — reconstructed to its kind (rows not carried here).
    let payload = match one_str("resultPayloadKind").as_deref() {
        Some("inferred") => ResultPayload::Inferred(parse_derived_axioms(&triples, &subject)),
        Some("bindings") => ResultPayload::Bindings(Vec::new()),
        Some("marginals") => ResultPayload::Marginals(Vec::new()),
        _ => ResultPayload::Empty,
    };

    Ok(ReasoningResult::new(
        input,
        evaluation,
        completeness,
        preservation,
        information,
        prov,
        payload,
    ))
}

/// Parse a proof/counterproof derivation node linked by `predicate_local`.
fn parse_derivation(
    triples: &[ParsedTriple],
    subject: &str,
    predicate_local: &str,
) -> Option<DerivationRef> {
    let node = triples
        .iter()
        .find(|t| t.subject == subject && t.predicate == logic(predicate_local))
        .and_then(|t| t.object_blank())?;
    let derivation_id = triples
        .iter()
        .find(|t| t.subject == node && t.predicate == logic("derivationId"))
        .and_then(|t| t.object_string())?;
    let cited_iris: BTreeSet<String> = triples
        .iter()
        .filter(|t| t.subject == node && t.predicate == logic("citesIri"))
        .filter_map(|t| t.object_iri())
        .collect();
    Some(DerivationRef {
        derivation_id,
        cited_iris,
    })
}

/// Parse the contradiction witnesses (the premises are recovered as opaque
/// `s p o` strings split back into a 3-tuple).
fn parse_witnesses(triples: &[ParsedTriple], subject: &str) -> Vec<ContradictionWitness> {
    let mut out = Vec::new();
    for link in triples
        .iter()
        .filter(|t| t.subject == subject && t.predicate == logic("resultContradiction"))
    {
        let Some(node) = link.object_blank() else {
            continue;
        };
        let individual = triples
            .iter()
            .find(|t| t.subject == node && t.predicate == logic("witnessIndividual"))
            .and_then(|t| t.object_iri())
            .unwrap_or_default();
        let world = triples
            .iter()
            .find(|t| t.subject == node && t.predicate == logic("witnessWorld"))
            .and_then(|t| t.object_iri())
            .unwrap_or_default();
        let mut premises = Vec::new();
        for t in triples
            .iter()
            .filter(|t| t.subject == node && t.predicate == logic("witnessPremise"))
        {
            if let Some(s) = t.object_string() {
                let parts: Vec<&str> = s.splitn(3, ' ').collect();
                if let [a, b, c] = parts[..] {
                    premises.push((a.to_owned(), b.to_owned(), c.to_owned()));
                }
            }
        }
        out.push(ContradictionWitness {
            individual,
            world,
            premises,
        });
    }
    out
}

/// Parse the derived-axiom rows back into [`InferredAxiom`]s (derived, non-EDB; the
/// premise lists are not carried here, so each is empty).
fn parse_derived_axioms(triples: &[ParsedTriple], subject: &str) -> Vec<InferredAxiom> {
    let mut out = Vec::new();
    for link in triples
        .iter()
        .filter(|t| t.subject == subject && t.predicate == logic("resultDerivedAxiom"))
    {
        let Some(node) = link.object_blank() else {
            continue;
        };
        let field = |local: &str| {
            triples
                .iter()
                .find(|t| t.subject == node && t.predicate == logic(local))
                .and_then(|t| t.object_iri())
                .unwrap_or_default()
        };
        out.push(InferredAxiom {
            subject: field("axiomSubject"),
            predicate: field("axiomPredicate"),
            object: field("axiomObject"),
            world: field("axiomWorld"),
            is_edb: false,
            rule_name: None,
            premises: Vec::new(),
        });
    }
    out
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
fn local_of(iri: &str) -> Result<&str, String> {
    iri.strip_prefix(LOGIC_NAMESPACE)
        .ok_or_else(|| format!("graph/reasoning: axis IRI not in logic namespace: {iri}"))
}

/// Require a present value (fail-closed).
fn req<T>(v: Option<T>, what: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("graph/reasoning: missing required field {what}"))
}

// ── A minimal N-Triples reader (IRI / blank / typed-literal objects) ─────────────

/// A parsed triple with resolved term shapes for the projection's closed vocabulary.
struct ParsedTriple {
    subject: String,
    predicate: String,
    object: ParsedObject,
}

enum ParsedObject {
    Iri(String),
    Blank(String),
    Lit(String),
}

impl ParsedTriple {
    fn object_iri(&self) -> Option<String> {
        match &self.object {
            ParsedObject::Iri(i) => Some(i.clone()),
            _ => None,
        }
    }
    fn object_blank(&self) -> Option<String> {
        match &self.object {
            ParsedObject::Blank(b) => Some(b.clone()),
            _ => None,
        }
    }
    fn object_string(&self) -> Option<String> {
        match &self.object {
            ParsedObject::Lit(s) => Some(s.clone()),
            _ => None,
        }
    }
}

/// Parse the projection's own N-Triples body (the closed subset this module emits:
/// `<iri>`/`_:b` subjects, `<iri>` predicates, `<iri>`/`_:b`/`"lex"^^<dt>` objects).
fn parse_nt(body: &str) -> Result<Vec<ParsedTriple>, String> {
    let mut out = Vec::new();
    for (lineno, raw) in body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = line
            .strip_suffix(" .")
            .ok_or_else(|| format!("graph/reasoning: line {} missing ' .'", lineno + 1))?;
        let (subject, rest) = take_term(line)
            .ok_or_else(|| format!("graph/reasoning: line {} bad subject", lineno + 1))?;
        let (predicate, rest) = take_term(rest.trim_start())
            .ok_or_else(|| format!("graph/reasoning: line {} bad predicate", lineno + 1))?;
        let (object, _rest) = take_term(rest.trim_start())
            .ok_or_else(|| format!("graph/reasoning: line {} bad object", lineno + 1))?;
        let subject = node_name(&subject)
            .ok_or_else(|| format!("graph/reasoning: line {} non-node subject", lineno + 1))?;
        let predicate = match predicate {
            TermLex::Iri(i) => i,
            _ => {
                return Err(format!(
                    "graph/reasoning: line {} non-IRI predicate",
                    lineno + 1
                ))
            }
        };
        let object = match object {
            TermLex::Iri(i) => ParsedObject::Iri(i),
            TermLex::Blank(b) => ParsedObject::Blank(b),
            TermLex::Lit(l) => ParsedObject::Lit(l),
        };
        out.push(ParsedTriple {
            subject,
            predicate,
            object,
        });
    }
    Ok(out)
}

/// A lexed term: an IRI, a blank node, or the *unescaped lexical form* of a literal.
enum TermLex {
    Iri(String),
    Blank(String),
    Lit(String),
}

/// The string form of a subject node (IRI value or bare blank id — bare so it
/// compares equal to an object blank, which is also stored bare).
fn node_name(t: &TermLex) -> Option<String> {
    match t {
        TermLex::Iri(i) => Some(i.clone()),
        TermLex::Blank(b) => Some(b.clone()),
        TermLex::Lit(_) => None,
    }
}

/// Take one N-Triples term off the front of `s`, returning `(term, rest)`.
///
/// Handles the closed term subset this module emits: `<iri>`, `_:id`, and
/// `"lex"^^<dt>` typed literals. The literal walk is char-based (UTF-8 safe).
fn take_term(s: &str) -> Option<(TermLex, &str)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('<') {
        let end = rest.find('>')?;
        return Some((TermLex::Iri(rest[..end].to_owned()), &rest[end + 1..]));
    }
    if let Some(rest) = s.strip_prefix("_:") {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        return Some((TermLex::Blank(rest[..end].to_owned()), &rest[end..]));
    }
    if let Some(rest) = s.strip_prefix('"') {
        // Walk to the unescaped closing quote, char by char (UTF-8 safe).
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
                    // Past the closing quote: skip the `^^<dt>` typed-literal suffix.
                    let after = &rest[idx + 1..];
                    let after = after.strip_prefix("^^").unwrap_or(after);
                    let after = if let Some(r) = after.strip_prefix('<') {
                        let end = r.find('>')?;
                        &r[end + 1..]
                    } else {
                        after
                    };
                    return Some((TermLex::Lit(lex), after));
                }
                _ => lex.push(ch),
            }
        }
        return None;
    }
    None
}

#[cfg(test)]
mod tests;
