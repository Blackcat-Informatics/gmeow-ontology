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

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, PreservationKind};
use sha2::{Digest, Sha256};

use crate::conjecture::{ConjectureAnswer, ConjectureDischarge, ConjectureLifecycleState};
use crate::reason::el::InferredAxiom;
use crate::result::{
    Assumption, BudgetLimit, CompletenessStatus, ContradictionWitness, DerivationRef, EngineId,
    EvaluationStatus, InformationState, InputStatus, PreservationClaim, ReasoningResult,
    ResultContext, ResultPayload, ResultProvenance,
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
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
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
    finalize_with_base(sink, RESULT_IRI_BASE, RESULT_PLACEHOLDER)
}

/// Content-address a sink's placeholder-built body against `iri_base` (the node IRI is
/// `iri_base + sha256(body)`), substitute `placeholder_urn` for the minted IRI, re-sort,
/// and emit the trailing-newline N-Triples document. The generalization of [`finalize`]
/// over the (base, placeholder) pair so a second projection (the conjecture verdict) can
/// mint content-addressed nodes off its OWN base without colliding with the reasoning
/// result base.
fn finalize_with_base(sink: Sink, iri_base: &str, placeholder_urn: &str) -> String {
    let lines = sink.render_lines();
    let node_iri = format!("{iri_base}{}", digest_lines(&lines));
    let placeholder = format!("<{placeholder_urn}>");
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
pub fn result_node_iri(result: &ReasoningResult) -> String {
    let body = project_reasoning_result(result);
    // The node IRI is the unique `RESULT_IRI_BASE`-prefixed subject in the body.
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix(&format!("<{RESULT_IRI_BASE}"))
            && let Some(end) = rest.find('>')
        {
            return format!("{RESULT_IRI_BASE}{}", &rest[..end]);
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
pub fn parse_reasoning_graph(nt_body: &str) -> gmeow_errors::Result<ReasoningResult> {
    let triples = parse_nt(nt_body)?;
    // The single subject typed logic:ReasoningResult.
    let subject = triples
        .iter()
        .find(|t| t.predicate == RDF_TYPE && t.object_iri() == Some(logic("ReasoningResult")))
        .map(|t| t.subject.clone())
        .ok_or_else(|| {
            result_err("graph/reasoning: no logic:ReasoningResult subject".to_owned())
        })?;

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
    for t in &triples {
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
    prov.query = req(one_str("resultQuery"), "resultQuery")?;
    prov.conclusion = req(one_str("resultConclusion"), "resultConclusion")?;
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
    for t in &triples {
        if t.subject == subject
            && t.predicate == logic("resultAssumption")
            && let Some(iri) = t.object_iri()
            && let Some(a) = assumption_from_iri(&iri)
        {
            prov.assumptions.insert(a);
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
        out.push(parse_witness_body(triples, &node));
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
        // axiom_term() can emit IRIs, blank nodes (_:b), or string literals.
        // The original code only called object_iri(), so blank-node and literal
        // axiom terms were silently lost (empty-string fallback).  Reconstruct
        // all three shapes so every emitted axiom term round-trips.
        //
        // IRI: object_iri() returns the bare IRI value (no angle brackets),
        //   which matches both the bare-IRI stored form (the test fixture) and
        //   the bracketed <iri> form from the native chase (both normalise to
        //   the same IRI node on emission, and the bare value round-trips).
        // Blank: object_blank() returns the label without "_:"; re-add it so
        //   axiom_term() on the next projection recognises the blank form.
        // Literal: object_string() returns the unescaped lex, which IS the
        //   original value stored in InferredAxiom (axiom_term kept the whole
        //   `"lit"` form as the lex, so it survives escape→unescape intact).
        let field = |local: &str| -> String {
            let t = triples
                .iter()
                .find(|t| t.subject == node && t.predicate == logic(local));
            match t {
                Some(t) => {
                    if let Some(iri) = t.object_iri() {
                        iri
                    } else if let Some(b) = t.object_blank() {
                        format!("_:{b}")
                    } else {
                        t.object_string().unwrap_or_default()
                    }
                }
                None => String::new(),
            }
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
fn parse_nt(body: &str) -> gmeow_errors::Result<Vec<ParsedTriple>> {
    let mut out = Vec::new();
    for (lineno, raw) in body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = line.strip_suffix(" .").ok_or_else(|| {
            result_err(format!("graph/reasoning: line {} missing ' .'", lineno + 1))
        })?;
        let (subject, rest) = take_term(line).ok_or_else(|| {
            result_err(format!("graph/reasoning: line {} bad subject", lineno + 1))
        })?;
        let (predicate, rest) = take_term(rest.trim_start()).ok_or_else(|| {
            result_err(format!(
                "graph/reasoning: line {} bad predicate",
                lineno + 1
            ))
        })?;
        let (object, _rest) = take_term(rest.trim_start()).ok_or_else(|| {
            result_err(format!("graph/reasoning: line {} bad object", lineno + 1))
        })?;
        let subject = node_name(&subject).ok_or_else(|| {
            result_err(format!(
                "graph/reasoning: line {} non-node subject",
                lineno + 1
            ))
        })?;
        let predicate = match predicate {
            TermLex::Iri(i) => i,
            _ => {
                return Err(result_err(format!(
                    "graph/reasoning: line {} non-IRI predicate",
                    lineno + 1
                )));
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
pub fn project_conjecture_verdict(input: &ConjectureVerdictInput) -> String {
    let answer = input.answer;

    // The embedded reasoning-result graph keeps its OWN content-addressed node IRI; the
    // conjecture node links to it rather than re-subjecting it.
    let result_body = project_reasoning_result(&answer.verdict);
    let result_iri = result_node_iri(&answer.verdict);

    let mut sink = Sink::default();
    let subject = Node::iri(CONJECTURE_PLACEHOLDER);

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
            Node::iri(result_iri),
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
            let forbidden = input.forbidden_predicate.expect(
                "caller contract: a refuted conjecture must carry its formula's principal \
                 predicate as the anti-conjecture obligation's forbidden predicate",
            );
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

    let conjecture_body = finalize_with_base(sink, CONJECTURE_IRI_BASE, CONJECTURE_PLACEHOLDER);

    // Merge the conjecture node graph with the embedded reasoning-result graph into one
    // sorted, deduplicated N-Triples document (both are already canonical; the union is
    // re-sorted so the whole graph round-trips).
    let mut lines: BTreeSet<String> = BTreeSet::new();
    for line in result_body.lines() {
        lines.insert(line.to_owned());
    }
    for line in conjecture_body.lines() {
        lines.insert(line.to_owned());
    }
    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out
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
pub fn conjecture_node_iri(input: &ConjectureVerdictInput) -> String {
    let body = project_conjecture_verdict(input);
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix(&format!("<{CONJECTURE_IRI_BASE}"))
            && let Some(end) = rest.find('>')
        {
            return format!("{CONJECTURE_IRI_BASE}{}", &rest[..end]);
        }
    }
    // Unreachable: the rdf:type triple always carries the node subject.
    CONJECTURE_IRI_BASE.to_owned()
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
    let triples = parse_nt(nt_body)?;
    let subject = triples
        .iter()
        .find(|t| t.predicate == RDF_TYPE && t.object_iri() == Some(logic("Conjecture")))
        .map(|t| t.subject.clone())
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Result {
                detail: "graph/conjecture: no logic:Conjecture subject".to_owned(),
            })
        })?;

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
    let verdict = parse_reasoning_graph(nt_body)?;

    // The refutation witness (linked via conjectureRefutationWitness, not resultContradiction).
    let witness = triples
        .iter()
        .find(|t| t.subject == subject && t.predicate == logic("conjectureRefutationWitness"))
        .and_then(|t| t.object_blank())
        .map(|node| parse_witness_body(&triples, &node));

    // The always-present structural twin bridge: the `math:Conjecture` whose
    // `math:conjectureUnderTest` edge names THIS `logic:Conjecture` node as its object.
    let math_conjecture = triples
        .iter()
        .find(|t| {
            t.predicate == math("conjectureUnderTest") && t.object_iri() == Some(subject.clone())
        })
        .map(|t| t.subject.clone());

    // The two symmetric promotion legs (present exactly on their lifecycle).
    let promotion_candidate = parse_promotion_candidate(&triples, &subject);
    let obligation_candidate = parse_obligation_candidate(&triples, &subject);

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
    triples: &[ParsedTriple],
    subject: &str,
) -> Option<PromotionCandidateRecord> {
    let node = triples
        .iter()
        .find(|t| t.subject == *subject && t.predicate == logic("conjecturePromotionCandidate"))
        .and_then(|t| t.object_iri())?;
    let carrier_iri = |local: &str| -> String {
        triples
            .iter()
            .find(|t| t.subject == node && t.predicate == logic(local))
            .and_then(|t| t.object_iri())
            .unwrap_or_default()
    };
    let carrier_str = |local: &str| -> String {
        triples
            .iter()
            .find(|t| t.subject == node && t.predicate == logic(local))
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
        node,
    })
}

/// Re-read the anti-conjecture leg from a parsed body: the candidate
/// `logic:NonEntailmentObligation` linked from `subject` via
/// `logic:antiConjectureObligationCandidate`, with its forbidden predicate and (sorted)
/// discharge conditions. `None` when no such edge is present.
fn parse_obligation_candidate(
    triples: &[ParsedTriple],
    subject: &str,
) -> Option<ObligationCandidateRecord> {
    let node = triples
        .iter()
        .find(|t| {
            t.subject == *subject && t.predicate == logic("antiConjectureObligationCandidate")
        })
        .and_then(|t| t.object_iri())?;
    let forbidden_predicate = triples
        .iter()
        .find(|t| t.subject == node && t.predicate == logic("obligationForbiddenPredicate"))
        .and_then(|t| t.object_string())
        .unwrap_or_default();
    let mut discharge_conditions: Vec<String> = triples
        .iter()
        .filter(|t| t.subject == node && t.predicate == logic("obligationDischargeCondition"))
        .filter_map(|t| t.object_iri())
        .collect();
    discharge_conditions.sort();
    Some(ObligationCandidateRecord {
        node,
        forbidden_predicate,
        discharge_conditions,
    })
}

/// Parse the internal triples of a `logic:ContradictionWitness` node (its
/// `witnessIndividual` / `witnessWorld` / `witnessPremise` set) into a
/// [`ContradictionWitness`]. Shared inverse of [`emit_witness_body`].
fn parse_witness_body(triples: &[ParsedTriple], node: &str) -> ContradictionWitness {
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
    ContradictionWitness {
        individual,
        world,
        premises,
    }
}

#[cfg(test)]
mod tests;
