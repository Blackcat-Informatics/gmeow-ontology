// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native RDF 1.2 Turtle artifact builders for the reasoning lane.
//!
//! The Java/Docker-free authority lane (Principles 17/18) emits three committed
//! artifacts from a single [`ReasoningResult`]:
//!
//! * **inferred-closure** — the told-vs-inferred derived axioms, each carrying an
//!   RDF 1.2 reifier annotated with its derivation provenance.
//! * **reasoning-explanations** — a per-axiom proof skeleton linking each
//!   conclusion (a triple term) to its premises and firing rule.
//! * **dl-el-crosscheck-report** — the native DL⊇EL divergence ledger, built from
//!   native results only (no external oracle); `DlGap` rows are coverage defects,
//!   so the committed bundle must emit zero.
//!
//! These builders are the canonical emitters for the reasoning artifacts (the
//! Python `build_*_ttl` emitters in `gmeow_tools.reason` they replaced were
//! retired). They serialize via the [`purrdf::turtle`] emitter
//! (clean full-IRI RDF 1.2), so its anonymous reifiers and `<<( … )>>` triple-term
//! objects match the committed artifacts and the drift gate (RDFC-1.0 isomorphism)
//! stays green.

use purrdf::turtle::{emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};
use purrdf::{
    BlankScope, RdfAnnotation, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfReifier,
    RdfTerm, RdfTriple, TermValue,
};

use crate::explain::{canonical_rule_iri, encode_receipt_rule_identity, receipt_for_axiom};
use crate::math_expression::{MATH_ALPHA_EQUIVALENCE_CLASS, MATH_ALPHA_EQUIVALENCE_CLASS_TYPE};
use crate::reason::dl::gaps_from_unsupported;
use crate::reason::el::InferredAxiom;
use crate::result::ReasoningResult;

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

// ── Namespaces ──────────────────────────────────────────────────────────────────

/// The gmeow vocabulary namespace (term IRIs are `GMEOW_NS + local`).
pub(crate) const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
/// The IRI base for a reasoning rule (`GMEOW_NS + "rule/"`, percent-encoded name).
const RULE_IRI_BASE: &str = "https://blackcatinformatics.ca/gmeow/rule/";
/// `rdfs:subClassOf` — the subsumption predicate the ledger records native-only.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// `prov:wasDerivedBy`.
const PROV_WAS_DERIVED_BY: &str = "http://www.w3.org/ns/prov#wasDerivedBy";
/// `prov:wasDerivedFrom` — the exact immediate-premise reifier links.
const PROV_WAS_DERIVED_FROM: &str = "http://www.w3.org/ns/prov#wasDerivedFrom";
/// `prov:value` — carries the tagged raw firing identity used by receipt hashing.
const PROV_VALUE: &str = "http://www.w3.org/ns/prov#value";
/// `logic:derivationIdentifier` — the content-addressed derivation IRI as data.
const LOGIC_DERIVATION_IDENTIFIER: &str =
    "https://blackcatinformatics.ca/logic/derivationIdentifier";
/// `rdf:type` (emitted full so the canonical compare never depends on `a`).
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:label`.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `rdfs:comment`.
pub(crate) const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
/// `xsd:boolean` — the datatype of the ledger's `gmeow:consistent` object.
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `xsd:integer` — the datatype of the numeric count objects (entailment/gap/budget).
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// `gmeow:` term IRI helper.
pub(crate) fn gmeow(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}

// ── Banners ─────────────────────────────────────────────────────────────────────

/// Banner + minimal prefix block prepended to the inferred-closure artifact.
const CLOSURE_HEADER: &str = "\
# GMEOW native inferred closure (RDF 1.2).
# The told-vs-inferred derived axioms produced by the native logic
# reasoning lane (EL/DL closure plus typed modal evaluation, Java/Docker-free), followed
# by the math: expression-identity derivation's math:alphaEquivalenceClass
# edges over the same asserted EDB. Each derived triple carries an RDF 1.2
# reifier annotated with its rule, exact source reifiers, and content-addressed
# derivation identity.
# DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
";

/// Banner + prefix block prepended to the proof-skeleton explanations artifact.
const EXPLANATIONS_HEADER: &str = "\
# GMEOW native reasoning explanations (RDF 1.2 proof skeletons).
# For every derived axiom the native logic lane produced, a content-addressed
# derivation node links the conclusion (an RDF 1.2 triple term) to its premises,
# exact source reifiers, and firing rule. Pure native-lane output. DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
";

/// Banner + prefix block prepended to the native DL⊇EL divergence ledger.
const LEDGER_HEADER: &str = "\
# GMEOW native DL/EL crosscheck ledger.
# Built from the native EL/DL reasoning lane (Java/Docker-free), from native
# results ONLY — a native DL⊇EL fragment comparison, with no external oracle.
# DlGap rows are native coverage defects and the committed bundle must keep
# gapCount at 0. DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
";

// ── Term helpers ────────────────────────────────────────────────────────────────

/// Mint the percent-encoded rule IRI for a *derived* axiom, failing loudly on a
/// missing rule name (no-optionality doctrine — a `None` rule on a derived axiom
/// is an engine-invariant violation, never a recoverable condition).
fn derived_rule_iri(axiom: &InferredAxiom) -> gmeow_errors::Result<String> {
    match axiom.rule_name.as_deref() {
        Some(name) if !name.is_empty() => Ok(canonical_rule_iri(name)),
        _ => Err(reason_err(format!(
            "derived axiom has no rule_name; the native engine must label every \
             inferred (non-EDB) axiom with the rule that produced it: \
             <{}> <{}> {}",
            axiom.subject,
            axiom.predicate,
            crate::provenance::term_display(&axiom.object)
        ))),
    }
}

/// Project a resource identifier carried by the subject or context surface.
/// Inferred objects use their native value and never pass through this helper.
fn iri_term(value: &str) -> RdfTerm {
    let inner = value
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(value);
    RdfTerm::iri(inner.to_owned())
}

/// The bare IRI string of a native-engine term (strip a surrounding `<>` pair).
fn bare_iri(value: &str) -> &str {
    value
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(value)
}

/// Project an inferred axiom's complete native object into its output triple.
fn axiom_triple(axiom: &InferredAxiom) -> gmeow_errors::Result<RdfTriple> {
    Ok(RdfTriple::new(
        iri_term(&axiom.subject),
        axiom.predicate.clone(),
        super::term_value_to_rdf_term(&axiom.object)?,
    ))
}

/// The derived (non-EDB) axioms of a result in a deterministic content order.
///
/// The native chase emits axioms in world-iteration order, which varies
/// run-to-run. Premises are canonicalized (sorted) at construction
/// time in `run_reasoning`, so this helper only orders the derived set by full
/// content so all three artifacts serialize byte-identically regardless of chase
/// order — killing the drift class at the single chokepoint every builder funnels
/// through. `sort` (stable) is used deliberately WITHOUT dedup: fully
/// content-equal duplicate derivations must be preserved so the emitted multiset
/// is unchanged.
fn derived_sorted(result: &ReasoningResult) -> Vec<&InferredAxiom> {
    let mut axioms: Vec<&InferredAxiom> = result.inferred().iter().filter(|a| !a.is_edb).collect();
    axioms.sort();
    axioms
}

// ── inferred-closure ────────────────────────────────────────────────────────────

/// The rule IRI name under which the `math:` expression-identity derivation publishes its
/// `math:alphaEquivalenceClass` edges. It is a derivation authority alongside the EL/DL
/// rules — the structural lowering of an authored expression through the content-addressed
/// term arena — so its output carries the SAME `prov:wasDerivedBy` / `gmeow:viaRule`
/// provenance every other row in this document does, under its own rule name.
const MATH_EXPRESSION_IDENTITY_RULE: &str = "math-expression-identity";

/// Render the native told-vs-inferred closure as an RDF 1.2 Turtle document.
///
/// For every *derived* (non-EDB) axiom this emits the base triple plus an RDF
/// 1.2 reifier carrying its derivation provenance: `prov:wasDerivedBy` and
/// `gmeow:viaRule` (both pointing at the canonical namespaced rule IRI), a
/// tagged `prov:value` retaining the raw firing identity used by the receipt hash,
/// `gmeow:inferenceKind gmeow:Deduction`, and `gmeow:inWorld` recording the
/// world. When `merge_asserted` is supplied, its told graph is prepended so the
/// document is the union of asserted and derived axioms (the `--merge` mode).
///
/// `alpha_edges` — the `(expression IRI, α-class IRI)` pairs
/// [`crate::math_expression::alpha_equivalence_edges`] derived over the SAME asserted EDB
/// this closure was reasoned from — is emitted as a final section: the
/// `math:alphaEquivalenceClass` edge itself plus the `rdf:type math:AlphaEquivalenceClass`
/// typing of the content-addressed individual it resolves to. That section is what makes the
/// α-equivalence identity a JOINABLE NODE in a shipped artifact rather than a value that
/// exists only inside a gate process: two α-equivalent expressions name the identical class
/// individual here, so a consumer holding only `gmeow.gts` or the committed closure file can
/// group them with an ordinary triple pattern. The pairs arrive already sorted by expression
/// IRI and each α-class IRI is a pure content digest, so the section is byte-stable. The
/// class typing is deduplicated: α-equivalent expressions share one individual and must not
/// type it twice.
///
/// # Errors
///
/// Returns `Err` if any derived axiom is missing its `rule_name`.
pub fn build_inferred_closure_ttl(
    result: &ReasoningResult,
    merge_asserted: Option<&RdfDataset>,
    alpha_edges: &[(String, String)],
) -> gmeow_errors::Result<String> {
    result.validate()?;
    let mut out = String::from(CLOSURE_HEADER);

    if let Some(store) = merge_asserted {
        let asserted = asserted_turtle(store)?;
        if !asserted.is_empty() {
            out.push_str("\n# --- asserted (told) graph (union; --merge) ---\n");
            out.push_str(&asserted);
        }
    }

    project_closure(result, alpha_edges, &mut out)?;
    Ok(out)
}

/// Project the closure once into both its required Turtle artifact and the
/// caller's native carrier. The caller adds its other typed graphs and freezes
/// once; there is no parsed or intermediate frozen closure dataset.
///
/// # Errors
/// Returns the same projection errors as [`build_inferred_closure_ttl`], or an
/// explicit refusal if no blank scope remains for anonymous proof reifiers.
pub fn build_inferred_closure_into(
    result: &ReasoningResult,
    alpha_edges: &[(String, String)],
    builder: &mut RdfDatasetBuilder,
) -> gmeow_errors::Result<String> {
    result.validate()?;
    let scope = closure_reifier_scope(result, builder)?;
    let mut sink = NativeClosureSink {
        text: String::from(CLOSURE_HEADER),
        builder,
        scope,
        ordinal: 0,
    };
    project_closure(result, alpha_edges, &mut sink)?;
    Ok(sink.text)
}

/// Only compact scope IDs are retained. Nested quoted terms and composite
/// literal blanks participate, so an anonymous proof node cannot alias data.
fn closure_reifier_scope(
    result: &ReasoningResult,
    builder: &RdfDatasetBuilder,
) -> gmeow_errors::Result<BlankScope> {
    fn collect(term: &TermValue, scopes: &mut std::collections::BTreeSet<BlankScope>) {
        match term {
            TermValue::Blank { scope, .. } => {
                scopes.insert(*scope);
            }
            TermValue::Triple { s, p, o } => {
                collect(s, scopes);
                collect(p, scopes);
                collect(o, scopes);
            }
            TermValue::Literal {
                lexical_form,
                datatype,
                ..
            } => {
                scopes.extend(
                    purrdf_core::cdt_blank::cdt_embedded_blanks(lexical_form, datatype)
                        .into_iter()
                        .map(|(_, scope)| scope),
                );
            }
            TermValue::Iri(_) => {}
        }
    }
    let mut scopes = builder.blank_identities().map(|(_, scope)| scope).collect();
    for axiom in result.inferred().iter().filter(|axiom| !axiom.is_edb) {
        collect(&axiom.object, &mut scopes);
    }
    let mut candidate = 1u32;
    for scope in scopes {
        if scope.0 < candidate {
            continue;
        }
        if scope.0 > candidate {
            break;
        }
        candidate = candidate.checked_add(1).ok_or_else(|| {
            reason_err("no blank scope remains for closure proof reifiers".to_owned())
        })?;
    }
    Ok(BlankScope(candidate))
}

trait ClosureSink {
    fn section(&mut self, text: &str);
    fn quad(&mut self, quad: &RdfQuad);
    fn reifier(&mut self, reifier: &RdfReifier, annotations: &[(String, RdfTerm)]);
}

impl ClosureSink for String {
    fn section(&mut self, text: &str) {
        self.push_str(text);
    }
    fn quad(&mut self, quad: &RdfQuad) {
        self.push_str(&emit_quad(quad));
    }
    fn reifier(&mut self, reifier: &RdfReifier, annotations: &[(String, RdfTerm)]) {
        self.push_str(&emit_reifier(reifier, annotations));
    }
}

struct NativeClosureSink<'a> {
    text: String,
    builder: &'a mut RdfDatasetBuilder,
    scope: BlankScope,
    ordinal: usize,
}

impl ClosureSink for NativeClosureSink<'_> {
    fn section(&mut self, text: &str) {
        self.text.section(text);
    }
    fn quad(&mut self, quad: &RdfQuad) {
        self.text.quad(quad);
        self.builder.push_owned_quad(quad);
    }
    fn reifier(&mut self, reifier: &RdfReifier, annotations: &[(String, RdfTerm)]) {
        self.text.reifier(reifier, annotations);
        let s = self.builder.intern_owned_term(&reifier.statement.subject);
        let p = self.builder.intern_iri(&reifier.statement.predicate);
        let o = self.builder.intern_owned_term(&reifier.statement.object);
        let triple = self.builder.intern_triple(s, p, o);
        let id = self
            .builder
            .intern_blank(&format!("proof{}", self.ordinal), self.scope);
        self.ordinal += 1;
        self.builder.push_reifier(id, triple);
        for (predicate, object) in annotations {
            let p = self.builder.intern_iri(predicate);
            let o = self.builder.intern_owned_term(object);
            self.builder.push_annotation(id, p, o);
        }
    }
}

fn project_closure(
    result: &ReasoningResult,
    alpha_edges: &[(String, String)],
    out: &mut impl ClosureSink,
) -> gmeow_errors::Result<()> {
    out.section("\n# --- derived (inferred) closure ---\n");
    for axiom in derived_sorted(result) {
        let triple = axiom_triple(axiom)?;
        let receipt = receipt_for_axiom(axiom);
        let rule = RdfTerm::iri(derived_rule_iri(axiom)?);
        let world = RdfTerm::iri(bare_iri(&axiom.world).to_owned());
        // Turtle cannot assert named-graph claims. A modal conclusion remains a
        // reified claim with C and its exact evaluation evidence, never a default axiom.
        if axiom.modal_evaluation.is_none() {
            out.quad(&RdfQuad::new(
                triple.subject.clone(),
                triple.predicate.clone(),
                triple.object.clone(),
            ));
        }
        let reifier = RdfReifier::new(RdfTerm::blank_node("r"), triple);
        let mut annotations = vec![
            (PROV_WAS_DERIVED_BY.to_owned(), rule.clone()),
            (gmeow("viaRule"), rule),
            (
                LOGIC_DERIVATION_IDENTIFIER.to_owned(),
                RdfTerm::literal(RdfLiteral::simple(receipt.row.derivation_id.clone())),
            ),
            (
                PROV_VALUE.to_owned(),
                RdfTerm::literal(RdfLiteral::simple(encode_receipt_rule_identity(
                    &receipt.raw_rule_identity,
                ))),
            ),
            (gmeow("inferenceKind"), RdfTerm::iri(gmeow("Deduction"))),
            (gmeow("inWorld"), world),
        ];
        if let Some(evidence) = &axiom.modal_evaluation {
            annotations.push((
                PROV_VALUE.to_owned(),
                RdfTerm::literal(RdfLiteral::simple(evidence.to_wire())),
            ));
        }
        annotations.extend(
            receipt
                .row
                .source_quad_ids
                .into_iter()
                .map(|source| (PROV_WAS_DERIVED_FROM.to_owned(), RdfTerm::iri(source))),
        );
        out.reifier(&reifier, &annotations);
    }
    alpha_equivalence_section(alpha_edges, out);
    Ok(())
}

/// Serialize the `math:alphaEquivalenceClass` edges as the closure's final section.
///
/// Empty — not even a banner — when there are no edges: an EDB carrying no `math:`
/// expression decides no identities, and a bare section header would read as a claim that
/// it did.
fn alpha_equivalence_section(alpha_edges: &[(String, String)], out: &mut impl ClosureSink) {
    if alpha_edges.is_empty() {
        return;
    }
    let rule = RdfTerm::iri(rule_iri(RULE_IRI_BASE, MATH_EXPRESSION_IDENTITY_RULE));
    out.section("\n# --- derived math: expression alpha-equivalence identity ---\n");
    let mut typed: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (expression, alpha_class) in alpha_edges {
        let triple = RdfTriple::new(
            RdfTerm::iri(expression.clone()),
            MATH_ALPHA_EQUIVALENCE_CLASS.to_owned(),
            RdfTerm::iri(alpha_class.clone()),
        );
        out.quad(&RdfQuad::new(
            triple.subject.clone(),
            triple.predicate.clone(),
            triple.object.clone(),
        ));
        let reifier = RdfReifier::new(RdfTerm::blank_node("r"), triple);
        out.reifier(
            &reifier,
            &[
                (PROV_WAS_DERIVED_BY.to_owned(), rule.clone()),
                (gmeow("viaRule"), rule.clone()),
                (gmeow("inferenceKind"), RdfTerm::iri(gmeow("Deduction"))),
            ],
        );
        typed.insert(alpha_class.as_str());
    }
    for alpha_class in typed {
        out.quad(&RdfQuad::new(
            RdfTerm::iri(alpha_class.to_owned()),
            RDF_TYPE.to_owned(),
            RdfTerm::iri(MATH_ALPHA_EQUIVALENCE_CLASS_TYPE.to_owned()),
        ));
    }
}

// ── reasoning-explanations ──────────────────────────────────────────────────────

/// Render an RDF 1.2 proof skeleton for every derived axiom.
///
/// Each content-addressed derivation node links the conclusion (an RDF 1.2 triple term via
/// `gmeow:concludes`) to its premises (`gmeow:hasPremise`, each also a triple
/// term) and the canonical firing rule (`gmeow:viaRule`), plus the raw firing
/// identity in a tagged `prov:value`, the inference kind, an English label, and
/// the world.
///
/// # Errors
///
/// Returns `Err` if any derived axiom is missing its `rule_name`.
pub fn build_explanations_ttl(result: &ReasoningResult) -> gmeow_errors::Result<String> {
    result.validate()?;
    let mut out = String::from(EXPLANATIONS_HEADER);
    out.push_str("\n# --- derivation proof skeletons ---\n");
    for axiom in derived_sorted(result) {
        let receipt = receipt_for_axiom(axiom);
        let rule = derived_rule_iri(axiom)?;
        let mut properties: Vec<(String, RdfTerm)> = vec![
            (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("Derivation"))),
            (gmeow("concludes"), RdfTerm::triple(axiom_triple(axiom)?)),
            (
                LOGIC_DERIVATION_IDENTIFIER.to_owned(),
                RdfTerm::literal(RdfLiteral::simple(receipt.row.derivation_id.clone())),
            ),
        ];
        if let Some(evidence) = &axiom.modal_evaluation {
            properties.push((
                PROV_VALUE.to_owned(),
                RdfTerm::literal(RdfLiteral::simple(evidence.to_wire())),
            ));
        } else {
            for (ps, pp, po) in &axiom.premises {
                let premise =
                    RdfTriple::new(RdfTerm::iri(ps.clone()), pp.clone(), premise_object(po)?);
                properties.push((gmeow("hasPremise"), RdfTerm::triple(premise)));
            }
        }
        properties.extend(
            receipt
                .row
                .source_quad_ids
                .iter()
                .cloned()
                .map(|source| (PROV_WAS_DERIVED_FROM.to_owned(), RdfTerm::iri(source))),
        );
        properties.push((gmeow("viaRule"), RdfTerm::iri(rule)));
        properties.push((
            PROV_VALUE.to_owned(),
            RdfTerm::literal(RdfLiteral::simple(encode_receipt_rule_identity(
                &receipt.raw_rule_identity,
            ))),
        ));
        properties.push((gmeow("inferenceKind"), RdfTerm::iri(gmeow("Deduction"))));
        properties.push((
            RDFS_LABEL.to_owned(),
            RdfTerm::literal(RdfLiteral::language_tagged(
                "derivation of an inferred axiom",
                "en",
            )),
        ));
        properties.push((
            gmeow("inWorld"),
            RdfTerm::iri(bare_iri(&axiom.world).to_owned()),
        ));

        out.push_str(&emit_resource(&receipt.row.derivation_id, &properties));
    }
    Ok(out)
}

/// Decode a conclusion or premise object through the native RDF 1.2 parser.
/// The common IRI case avoids a dataset allocation; literals, blank nodes and
/// recursive triple terms retain their actual RDF kinds. Invalid term syntax
/// fails rather than being reinterpreted as an IRI.
pub(crate) fn premise_object(display: &str) -> gmeow_errors::Result<RdfTerm> {
    if !display.starts_with(['"', '_']) && !display.starts_with("<<") {
        let value = bare_iri(display);
        let iri = purrdf::iri::parse(value)
            .map_err(|error| reason_err(format!("reasoning artifact IRI: {error}")))?;
        if !iri.has_scheme() {
            return Err(reason_err("reasoning artifact IRI must be absolute".into()));
        }
        return Ok(RdfTerm::iri(value));
    }
    const SUBJECT: &str = "urn:gmeow:artifact-term:subject";
    const PREDICATE: &str = "urn:gmeow:artifact-term:object";
    let input = format!("<{SUBJECT}> <{PREDICATE}> {display} .");
    let dataset = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None)
        .map_err(|error| reason_err(format!("reasoning artifact object: {error}")))?;
    if dataset
        .quads()
        .chain(dataset.reifier_quads())
        .chain(dataset.annotation_quads())
        .count()
        != 1
    {
        return Err(reason_err(
            "reasoning artifact object must be exactly one RDF term".into(),
        ));
    }
    let quad = dataset.owned_quads().next().ok_or_else(|| {
        reason_err("reasoning artifact object has no ordinary carrier triple".into())
    })?;
    if quad.subject != RdfTerm::iri(SUBJECT) || quad.predicate != PREDICATE {
        return Err(reason_err(
            "reasoning artifact object changed its carrier".into(),
        ));
    }
    Ok(quad.object)
}

// ── dl-el-crosscheck-report ─────────────────────────────────────────────────────

/// Render the native DL⊇EL crosscheck ledger as Turtle.
///
/// Built from the native results ONLY (the gate stays Java/Docker-free). Emits
/// the ledger header, one `gmeow:LedgerEntry` of kind `gmeow:NativeOnly` per
/// derived `rdfs:subClassOf` entailment, one `gmeow:DlGap` per native coverage
/// defect, and the entailment/gap counts. The committed bundle is expected to
/// have zero `DlGap` rows.
///
/// # Errors
/// Rejects inferred terms that cannot form valid RDF output.
pub fn build_dl_el_ledger_ttl(result: &ReasoningResult) -> gmeow_errors::Result<String> {
    const CROSSCHECK_NOTE: &str = "a native-only DL⊇EL subsumption entailment; a native DL coverage gap (DlGap) fails the gate";
    let mut out = String::from(LEDGER_HEADER);

    // The DL coverage gaps are reconstructed from the shared model's
    // unsupported-construct set via the one recipe `verdict_from_inferred` uses,
    // so the ledger stays byte-identical whether built from a DlVerdict or a typed
    // ReasoningResult. The committed bundle is gap-zero, so this is empty
    // on a healthy run; the set is already sorted (a BTreeSet).
    let gaps = gaps_from_unsupported(result.preservation.unsupported_constructs.iter());

    out.push_str("\n# --- ledger header (native coverage; gap-zero) ---\n");
    out.push_str(&emit_resource(
        &gmeow("dl-el-crosscheck"),
        &[
            (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("CrosscheckLedger"))),
            (
                gmeow("consistent"),
                RdfTerm::literal(RdfLiteral::typed(
                    if result.is_consistent() { "true" } else { "false" },
                    XSD_BOOLEAN,
                )),
            ),
            (
                gmeow("coverageNote"),
                RdfTerm::literal(RdfLiteral::language_tagged(
                    "native DL⊇EL gap-zero coverage (native results only, no external oracle); a DlGap is a native coverage defect and fails",
                    "en",
                )),
            ),
        ],
    ));

    // Native-only subsumption entailments (derived rdfs:subClassOf axioms).
    let subsumptions: Vec<&InferredAxiom> = derived_sorted(result)
        .into_iter()
        .filter(|a| a.predicate == RDFS_SUBCLASS_OF)
        .collect();

    out.push_str("\n# --- native-only subsumption entailments ---\n");
    for (index, axiom) in subsumptions.iter().enumerate() {
        let subsumes = RdfTerm::triple(RdfTriple::new(
            iri_term(&axiom.subject),
            RDFS_SUBCLASS_OF,
            super::term_value_to_rdf_term(&axiom.object)?,
        ));
        out.push_str(&emit_resource(
            &gmeow(&format!("ledger-entry-{index}")),
            &[
                (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("LedgerEntry"))),
                (gmeow("entryKind"), RdfTerm::iri(gmeow("NativeOnly"))),
                (gmeow("subsumes"), subsumes),
                (
                    gmeow("inWorld"),
                    RdfTerm::iri(bare_iri(&axiom.world).to_owned()),
                ),
                (
                    RDFS_COMMENT.to_owned(),
                    RdfTerm::literal(RdfLiteral::language_tagged(CROSSCHECK_NOTE, "en")),
                ),
            ],
        ));
    }

    // Native DL coverage defects.
    out.push_str("\n# --- native DL coverage defects ---\n");
    for (index, gap) in gaps.iter().enumerate() {
        out.push_str(&emit_resource(
            &gmeow(&format!("dl-gap-{index}")),
            &[
                (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("DlGap"))),
                (
                    gmeow("gapCode"),
                    RdfTerm::literal(RdfLiteral::language_tagged(gap.code.as_str(), "en")),
                ),
                (
                    RDFS_COMMENT.to_owned(),
                    RdfTerm::literal(RdfLiteral::language_tagged(gap.message.as_str(), "en")),
                ),
            ],
        ));
    }

    // Counts.
    out.push_str("\n# --- counts ---\n");
    out.push_str(&emit_resource(
        &gmeow("dl-el-crosscheck"),
        &[
            (
                gmeow("entailmentCount"),
                RdfTerm::literal(RdfLiteral::typed(
                    subsumptions.len().to_string(),
                    XSD_INTEGER,
                )),
            ),
            (
                gmeow("gapCount"),
                RdfTerm::literal(RdfLiteral::typed(gaps.len().to_string(), XSD_INTEGER)),
            ),
        ],
    ));

    Ok(out)
}

// ── reasoning-result + proof-certificate ────────────────────────────────────────

/// The `logic:` vocabulary namespace (the typed-result terms live here).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// `logic:` term IRI helper.
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// Banner for the typed reasoning-result + proof-certificate artifact.
const RESULT_HEADER: &str = "\
# GMEOW typed reasoning result + proof certificate (RDF 1.2).
# The single shared logic:ReasoningResult the native lane produced, serialized as
# its five orthogonal status fields (input, evaluation, completeness,
# preservation, information) plus the provenance bundle (contract hash, engine,
# proof/counterproof, contradiction witnesses, assumptions, consumed budget) —
# the proof certificate binding the verdict to what produced it. DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";

/// Render the typed [`ReasoningResult`] as a `logic:ReasoningResult` individual —
/// the proof-certificate surface (ME2): the five status fields projected to
/// their `module.ttl` value individuals plus the provenance bundle (contract
/// hash, engine, proof/counterproof, contradiction witnesses, assumptions). This
/// is a NEW, additive artifact — it does not touch the three historical
/// byte-pinned artifacts.
///
/// All multi-valued fields iterate sorted sets, so the output is deterministic.
pub fn build_reasoning_result_ttl(result: &ReasoningResult) -> String {
    let mut out = String::from(RESULT_HEADER);
    out.push_str("\n# --- reasoning result + proof certificate ---\n");

    let subject = gmeow("reasoning-result");
    let mut props: Vec<(String, RdfTerm)> = vec![
        (RDF_TYPE.to_owned(), RdfTerm::iri(logic("ReasoningResult"))),
        (logic("resultInput"), RdfTerm::iri(result.input.iri())),
        (
            logic("resultEvaluation"),
            RdfTerm::iri(result.evaluation.iri()),
        ),
        (
            logic("resultCompleteness"),
            RdfTerm::iri(result.completeness.iri()),
        ),
        (
            logic("resultInformation"),
            RdfTerm::iri(result.information.iri()),
        ),
        (
            logic("contractHash"),
            RdfTerm::literal(RdfLiteral::simple(result.provenance.contract_hash.as_str())),
        ),
        (
            logic("engine"),
            RdfTerm::literal(RdfLiteral::simple(format!(
                "{} {}",
                result.provenance.engine.name, result.provenance.engine.version
            ))),
        ),
        (
            logic("consumedBudget"),
            RdfTerm::literal(RdfLiteral::typed(
                result.provenance.consumed_budget.consumed.to_string(),
                XSD_INTEGER,
            )),
        ),
    ];

    // The preservation polarity set + unsupported constructs (both sorted).
    for kind in &result.preservation.polarities {
        props.push((logic("resultPreservation"), RdfTerm::iri(kind.iri())));
    }
    for construct in &result.preservation.unsupported_constructs {
        props.push((
            logic("unsupportedConstruct"),
            RdfTerm::literal(RdfLiteral::simple(construct.as_str())),
        ));
    }

    // The proof certificate: query/conclusion + proof/counterproof handles.
    if !result.provenance.query.is_empty() {
        props.push((
            logic("query"),
            RdfTerm::literal(RdfLiteral::simple(result.provenance.query.as_str())),
        ));
    }
    if !result.provenance.conclusion.is_empty() {
        props.push((
            logic("conclusion"),
            RdfTerm::literal(RdfLiteral::simple(result.provenance.conclusion.as_str())),
        ));
    }
    if let Some(proof) = &result.provenance.proof {
        props.push((
            logic("resultProof"),
            RdfTerm::iri(bare_iri(&proof.derivation_id).to_owned()),
        ));
    }
    if let Some(counterproof) = &result.provenance.counterproof {
        props.push((
            logic("resultCounterproof"),
            RdfTerm::iri(bare_iri(&counterproof.derivation_id).to_owned()),
        ));
    }

    // Belnap contradiction witnesses (justify information=both), sorted.
    for witness in &result.provenance.contradiction_witnesses {
        props.push((
            logic("contradictionWitness"),
            RdfTerm::iri(bare_iri(&witness.individual).to_owned()),
        ));
    }

    // Declared closure/identity/revision/witness-policy assumptions, sorted.
    for assumption in &result.provenance.assumptions {
        props.push((
            logic("resultAssumption"),
            RdfTerm::literal(RdfLiteral::simple(assumption.wire())),
        ));
    }

    // The world the answer holds in (when pinned).
    if !result.provenance.context.world.is_empty() {
        props.push((
            gmeow("inWorld"),
            RdfTerm::iri(bare_iri(&result.provenance.context.world).to_owned()),
        ));
    }

    out.push_str(&emit_resource(&subject, &props));
    out
}

// ── asserted (told) graph for --merge ────────────────────────────────────────────

/// Serialize the asserted store's quads + reifiers + annotations to Turtle.
///
/// The named-graph component is dropped (the closure document is a single Turtle
/// graph; worlds are carried as `gmeow:inWorld` annotations on the derived side).
/// RDF 1.2 reifiers and annotations the asserted statement layer carries are
/// emitted in their full-IRI shorthand form so the union document round-trips
/// under the RDF 1.2 Turtle parser.
///
/// # Errors
///
/// Returns `Err` if the store surfaces a quad/reifier/annotation read failure.
fn asserted_turtle(store: &RdfDataset) -> gmeow_errors::Result<String> {
    let mut out = String::new();
    for quad in store.owned_quads() {
        out.push_str(&emit_quad(&quad));
    }
    for reifier in store.owned_reifiers() {
        out.push_str(&emit_reifier(&reifier, &[]));
    }
    for annotation in store.owned_annotations() {
        out.push_str(&emit_annotation_triple(&annotation));
    }
    Ok(out)
}

/// Emit a standalone annotation triple `<reifier> <predicate> <object> .`.
fn emit_annotation_triple(annotation: &RdfAnnotation) -> String {
    format!(
        "{} <{}> {} .\n",
        emit_term(&annotation.reifier),
        annotation.predicate,
        emit_term(&annotation.object)
    )
}

#[path = "artifacts.tests.rs"]
#[cfg(test)]
mod tests;
