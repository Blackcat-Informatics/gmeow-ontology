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
//! * **dl-el-crosscheck-report** — the native↔oracle divergence ledger; `DlGap`
//!   rows are coverage defects, so the committed bundle must emit zero.
//!
//! These builders are the canonical emitters for the reasoning artifacts (the
//! Python `build_*_ttl` emitters in `gmeow_tools.reason` they replaced were
//! retired). They serialize via the gmeow-rdf [`purrdf::turtle`] emitter
//! (clean full-IRI RDF 1.2), so its anonymous reifiers and `<<( … )>>` triple-term
//! objects match the committed artifacts and the drift gate (RDFC-1.0 isomorphism)
//! stays green.

use purrdf::turtle::{emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};
use purrdf::{
    RdfAnnotation, RdfDataset, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple, TermValue,
};

use crate::reason::dl::gaps_from_unsupported;
use crate::reason::el::InferredAxiom;
use crate::reason::ledger::{DivergenceLedger, divergence_findings};
use crate::result::ReasoningResult;
use crate::term_codec::decode_term;

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
/// `rdf:type` (emitted full so the canonical compare never depends on `a`).
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:label`.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `rdfs:comment`.
pub(crate) const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";

/// `gmeow:` term IRI helper.
pub(crate) fn gmeow(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}

// ── Banners ─────────────────────────────────────────────────────────────────────

/// Banner + minimal prefix block prepended to the inferred-closure artifact.
const CLOSURE_HEADER: &str = "\
# GMEOW native inferred closure (RDF 1.2).
# The told-vs-inferred derived axioms produced by the native EL/DL
# reasoning lane (gmeow_logic.reason_native, Java/Docker-free). Each
# inferred triple carries an RDF 1.2 reifier annotated with its
# derivation provenance (prov:wasDerivedBy / gmeow:viaRule). DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
";

/// Banner + prefix block prepended to the proof-skeleton explanations artifact.
const EXPLANATIONS_HEADER: &str = "\
# GMEOW native reasoning explanations (RDF 1.2 proof skeletons).
# For every derived axiom the native EL/DL lane produced, a derivation
# node links the conclusion (an RDF 1.2 reifier) to its premises and the
# rule that fired (gmeow:viaRule). Pure native-lane output. DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
";

/// Banner + prefix block prepended to the native↔oracle divergence ledger.
const LEDGER_HEADER: &str = "\
# GMEOW native vs entail-oracle DL/EL crosscheck ledger.
# Built from the native EL/DL reasoning lane (Java/Docker-free). The oracle
# comparison runs the in-process purrdf-entail oracle; DlGap rows are native
# coverage defects and the committed bundle must keep gapCount at 0.
# DO NOT EDIT.
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
        Some(name) if !name.is_empty() => Ok(rule_iri(RULE_IRI_BASE, name)),
        _ => Err(reason_err(format!(
            "derived axiom has no rule_name; the native engine must label every \
             inferred (non-EDB) axiom with the rule that produced it: \
             <{}> <{}> <{}>",
            axiom.subject, axiom.predicate, axiom.object
        ))),
    }
}

/// Normalize a native-engine term into a bare IRI string.
///
/// The native engine emits subjects/predicates/worlds as bare IRI strings, but
/// objects already wrapped in `<...>` (the N3 display form of
/// the typed term decoder). This collapses both to the bare inner IRI so a single
/// `RdfTerm::iri` never double-wraps the angle brackets. Mirrors the retired
/// Python `_iri_term`.
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

/// Build the `<s> <p> <o>` triple of an inferred axiom (all IRIs).
fn axiom_triple(axiom: &InferredAxiom) -> RdfTriple {
    RdfTriple::new(
        iri_term(&axiom.subject),
        axiom.predicate.clone(),
        iri_term(&axiom.object),
    )
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

/// Render the native told-vs-inferred closure as an RDF 1.2 Turtle document.
///
/// For every *derived* (non-EDB) axiom this emits the base triple plus an RDF
/// 1.2 reifier carrying its derivation provenance: `prov:wasDerivedBy` and
/// `gmeow:viaRule` (both pointing at the namespaced rule IRI),
/// `gmeow:inferenceKind gmeow:Deduction`, and `gmeow:inWorld` recording the
/// world. When `merge_asserted` is supplied, its told graph is prepended so the
/// document is the union of asserted and derived axioms (the `--merge` mode).
///
/// # Errors
///
/// Returns `Err` if any derived axiom is missing its `rule_name`.
pub fn build_inferred_closure_ttl(
    result: &ReasoningResult,
    merge_asserted: Option<&RdfDataset>,
) -> gmeow_errors::Result<String> {
    let mut out = String::from(CLOSURE_HEADER);

    if let Some(store) = merge_asserted {
        let asserted = asserted_turtle(store)?;
        if !asserted.is_empty() {
            out.push_str("\n# --- asserted (told) graph (union; --merge) ---\n");
            out.push_str(&asserted);
        }
    }

    out.push_str("\n# --- derived (inferred) closure ---\n");
    for axiom in derived_sorted(result) {
        let triple = axiom_triple(axiom);
        let rule = format!("<{}>", derived_rule_iri(axiom)?);
        let world = format!("<{}>", bare_iri(&axiom.world));
        out.push_str(&emit_quad(&RdfQuad::new(
            triple.subject.clone(),
            triple.predicate.clone(),
            triple.object.clone(),
        )));
        let reifier = RdfReifier::new(RdfTerm::blank_node("r"), triple);
        out.push_str(&emit_reifier(
            &reifier,
            &[
                (PROV_WAS_DERIVED_BY.to_owned(), rule.clone()),
                (gmeow("viaRule"), rule),
                (gmeow("inferenceKind"), format!("<{}>", gmeow("Deduction"))),
                (gmeow("inWorld"), world),
            ],
        ));
    }
    Ok(out)
}

// ── reasoning-explanations ──────────────────────────────────────────────────────

/// Render an RDF 1.2 proof skeleton for every derived axiom.
///
/// Each derivation node links the conclusion (an RDF 1.2 triple term via
/// `gmeow:concludes`) to its premises (`gmeow:hasPremise`, each also a triple
/// term) and the firing rule (`gmeow:viaRule`), recording the inference kind, an
/// English label, and the world.
///
/// # Errors
///
/// Returns `Err` if any derived axiom is missing its `rule_name`.
pub fn build_explanations_ttl(result: &ReasoningResult) -> gmeow_errors::Result<String> {
    let mut out = String::from(EXPLANATIONS_HEADER);
    out.push_str("\n# --- derivation proof skeletons ---\n");
    for axiom in derived_sorted(result) {
        let conclusion = emit_term(&RdfTerm::triple(axiom_triple(axiom)));
        let rule = format!("<{}>", derived_rule_iri(axiom)?);
        let world = format!("<{}>", bare_iri(&axiom.world));

        // Property list on an anonymous derivation node: type, conclusion, each
        // premise (canonically sorted at construction, deterministic), rule, kind, label, world.
        let mut properties: Vec<(String, String)> = Vec::new();
        properties.push((RDF_TYPE.to_owned(), format!("<{}>", gmeow("Derivation"))));
        properties.push((gmeow("concludes"), conclusion));
        for (ps, pp, po) in &axiom.premises {
            let premise = RdfTriple::new(RdfTerm::iri(ps.clone()), pp.clone(), premise_object(po));
            properties.push((gmeow("hasPremise"), emit_term(&RdfTerm::triple(premise))));
        }
        properties.push((gmeow("viaRule"), rule));
        properties.push((gmeow("inferenceKind"), format!("<{}>", gmeow("Deduction"))));
        properties.push((
            RDFS_LABEL.to_owned(),
            "\"derivation of an inferred axiom\"@en".to_owned(),
        ));
        properties.push((gmeow("inWorld"), world));

        out.push_str(&emit_anonymous_resource(&properties));
    }
    Ok(out)
}

/// Build the object term of a premise triple.
///
/// Premise objects arrive as the engine's N-Triples display string. Re-decode it
/// to the typed term so each kind round-trips correctly in the proof skeleton: an
/// IRI (`<iri>`) becomes a bare IRI term, and a literal (`"lex"`, `"lex"@lang`,
/// `"lex"^^<dt>`) stays a literal — emitting it as an IRI would produce invalid
/// Turtle. A form the decoder cannot read (it never occurs as a subsumption
/// premise object) falls back to the bare-IRI unwrap.
fn premise_object(display: &str) -> RdfTerm {
    match decode_term(display) {
        Ok(TermValue::Iri(iri)) => RdfTerm::iri(iri),
        Ok(TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        }) => RdfTerm::literal(rdf_literal_from_value(
            &lexical_form,
            &datatype,
            language.as_deref(),
        )),
        _ => iri_term(display),
    }
}

/// Convert a native literal's value-space components to the model [`RdfLiteral`],
/// preserving a language tag or a non-`xsd:string` datatype so [`emit_term`]
/// re-serializes it to the same Turtle literal form.
fn rdf_literal_from_value(
    lexical_form: &str,
    datatype: &str,
    language: Option<&str>,
) -> RdfLiteral {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    if let Some(language) = language {
        RdfLiteral::language_tagged(lexical_form, language)
    } else if datatype == XSD_STRING {
        RdfLiteral::simple(lexical_form)
    } else {
        RdfLiteral::typed(lexical_form, datatype)
    }
}

/// Emit an anonymous (`[]`) resource with a property list, matching the Python
/// `[] a gmeow:Derivation ; … .` block. The first property is the `rdf:type`.
fn emit_anonymous_resource(properties: &[(String, String)]) -> String {
    let mut out = String::from("[]");
    let mut first = true;
    for (predicate, object) in properties {
        if first {
            out.push_str(&format!(" <{predicate}> {object}"));
            first = false;
        } else {
            out.push_str(&format!(" ;\n   <{predicate}> {object}"));
        }
    }
    out.push_str(" .\n");
    out
}

// ── dl-el-crosscheck-report ─────────────────────────────────────────────────────

/// Render the native↔oracle DL/EL crosscheck ledger as Turtle.
///
/// Built from the native results ONLY (the gate stays Java/Docker-free). Emits
/// the ledger header, one `gmeow:LedgerEntry` of kind `gmeow:NativeOnly` per
/// derived `rdfs:subClassOf` entailment, one `gmeow:DlGap` per native coverage
/// defect, and the entailment/gap counts. The committed bundle is expected to
/// have zero `DlGap` rows.
pub fn build_dl_el_ledger_ttl(result: &ReasoningResult) -> String {
    const CROSSCHECK_NOTE: &str = "the independent purrdf entailment cross-check validates the native result; native gaps fail";
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
            (
                RDF_TYPE.to_owned(),
                format!("<{}>", gmeow("CrosscheckLedger")),
            ),
            (
                gmeow("consistent"),
                if result.is_consistent() {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                },
            ),
            (
                gmeow("oracleCrosscheck"),
                "\"classic-cross-check confirms the native result; DlGap is a failure\"@en"
                    .to_owned(),
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
        let subsumes = emit_term(&RdfTerm::triple(RdfTriple::new(
            iri_term(&axiom.subject),
            RDFS_SUBCLASS_OF,
            iri_term(&axiom.object),
        )));
        out.push_str(&emit_resource(
            &gmeow(&format!("ledger-entry-{index}")),
            &[
                (RDF_TYPE.to_owned(), format!("<{}>", gmeow("LedgerEntry"))),
                (gmeow("entryKind"), format!("<{}>", gmeow("NativeOnly"))),
                (gmeow("subsumes"), subsumes),
                (gmeow("inWorld"), format!("<{}>", bare_iri(&axiom.world))),
                (
                    RDFS_COMMENT.to_owned(),
                    format!("\"{}\"@en", escape_literal(CROSSCHECK_NOTE)),
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
                (RDF_TYPE.to_owned(), format!("<{}>", gmeow("DlGap"))),
                (
                    gmeow("gapCode"),
                    format!("\"{}\"@en", escape_literal(&gap.code)),
                ),
                (
                    RDFS_COMMENT.to_owned(),
                    format!("\"{}\"@en", escape_literal(&gap.message)),
                ),
            ],
        ));
    }

    // Counts.
    out.push_str("\n# --- counts ---\n");
    out.push_str(&emit_resource(
        &gmeow("dl-el-crosscheck"),
        &[
            (gmeow("entailmentCount"), subsumptions.len().to_string()),
            (gmeow("gapCount"), gaps.len().to_string()),
        ],
    ));

    out
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
    let mut props: Vec<(String, String)> = vec![
        (
            RDF_TYPE.to_owned(),
            format!("<{}>", logic("ReasoningResult")),
        ),
        (logic("resultInput"), format!("<{}>", result.input.iri())),
        (
            logic("resultEvaluation"),
            format!("<{}>", result.evaluation.iri()),
        ),
        (
            logic("resultCompleteness"),
            format!("<{}>", result.completeness.iri()),
        ),
        (
            logic("resultInformation"),
            format!("<{}>", result.information.iri()),
        ),
        (
            logic("contractHash"),
            format!("\"{}\"", escape_literal(&result.provenance.contract_hash)),
        ),
        (
            logic("engine"),
            format!(
                "\"{} {}\"",
                escape_literal(&result.provenance.engine.name),
                escape_literal(&result.provenance.engine.version)
            ),
        ),
        (
            logic("consumedBudget"),
            result.provenance.consumed_budget.consumed.to_string(),
        ),
    ];

    // The preservation polarity set + unsupported constructs (both sorted).
    for kind in &result.preservation.polarities {
        props.push((logic("resultPreservation"), format!("<{}>", kind.iri())));
    }
    for construct in &result.preservation.unsupported_constructs {
        props.push((
            logic("unsupportedConstruct"),
            format!("\"{}\"", escape_literal(construct)),
        ));
    }

    // The proof certificate: query/conclusion + proof/counterproof handles.
    if !result.provenance.query.is_empty() {
        props.push((
            logic("query"),
            format!("\"{}\"", escape_literal(&result.provenance.query)),
        ));
    }
    if !result.provenance.conclusion.is_empty() {
        props.push((
            logic("conclusion"),
            format!("\"{}\"", escape_literal(&result.provenance.conclusion)),
        ));
    }
    if let Some(proof) = &result.provenance.proof {
        props.push((
            logic("resultProof"),
            format!("<{}>", bare_iri(&proof.derivation_id)),
        ));
    }
    if let Some(counterproof) = &result.provenance.counterproof {
        props.push((
            logic("resultCounterproof"),
            format!("<{}>", bare_iri(&counterproof.derivation_id)),
        ));
    }

    // Belnap contradiction witnesses (justify information=both), sorted.
    for witness in &result.provenance.contradiction_witnesses {
        props.push((
            logic("contradictionWitness"),
            format!("<{}>", bare_iri(&witness.individual)),
        ));
    }

    // Declared closure/identity/revision/witness-policy assumptions, sorted.
    for assumption in &result.provenance.assumptions {
        props.push((
            logic("resultAssumption"),
            format!("\"{}\"", assumption.wire()),
        ));
    }

    // The world the answer holds in (when pinned).
    if !result.provenance.context.world.is_empty() {
        props.push((
            gmeow("inWorld"),
            format!("<{}>", bare_iri(&result.provenance.context.world)),
        ));
    }

    out.push_str(&emit_resource(&subject, &props));
    out
}

// ── fragment-subsumption correspondence (EL, RL, … lattice edges) ─────────────────

/// A certified fragment on the native-chase promotion lattice (EL ⊂ RL ⊂ DL …).
///
/// Carries the two pieces of per-fragment identity the correspondence artifact
/// varies over: `slug` is the lowercase IRI-local token that keys the reified
/// individuals (`{slug}-native-subsumption-correspondence`, …) so each fragment
/// edge gets stable, distinct subjects in the bundle; `label` is the uppercase
/// profile name used in the human-readable banner and comment. Everything else —
/// relation, morphism, preservation, discharged section-law — is IDENTICAL across
/// fragments (the whole point: one calculus, one claim shape per lattice edge).
struct SubsumptionFragment {
    /// IRI-local slug (`"el"`, `"rl"`) keying the reified individuals.
    slug: &'static str,
    /// Profile label (`"EL"`, `"RL"`) for the banner / comment prose.
    label: &'static str,
}

/// The EL fragment edge of the native-chase promotion lattice.
const EL_FRAGMENT: SubsumptionFragment = SubsumptionFragment {
    slug: "el",
    label: "EL",
};

/// The RL fragment edge (the next edge up: EL ⊂ RL) of the promotion lattice.
const RL_FRAGMENT: SubsumptionFragment = SubsumptionFragment {
    slug: "rl",
    label: "RL",
};

/// The DL fragment edge — the terminal edge of the EL ⊂ RL ⊂ DL fragment
/// lattice — over the world-scoped typed DL Horn closure. Existential obligations
/// share the native structured restricted chase, so this correspondence certifies
/// the same single-authority closure against the independent entailment reference.
const DL_FRAGMENT: SubsumptionFragment = SubsumptionFragment {
    slug: "dl",
    label: "DL",
};

/// Banner for a native⊒oracle fragment-subsumption correspondence artifact.
fn correspondence_header(fragment: &SubsumptionFragment) -> String {
    let label = fragment.label;
    format!(
        "\
# GMEOW native ⊒ oracle {label}-subsumption correspondence (RDF 1.2).
# The reified logic:Correspondence recording that the native forward engine
# SUBSUMES the independent purrdf entailment reference on the certified {label} fragment: the reference
# closure is a section/retraction of the native closure (put ∘ get = id over the
# {label} profile), a complete over-approximation carrying the native↔oracle
# divergence ledger as its loss cell. Pure native-lane output. DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
"
    )
}

/// Render the native⊒oracle EL-subsumption parity result as a bundle-borne
/// `logic:Correspondence` individual (Part B of the native-chase promotion).
///
/// Thin wrapper over the shared subsumption-correspondence builder, pinned to the EL
/// lattice edge; see that function for the full claim-shape rationale.
pub fn build_el_subsumption_correspondence_ttl(
    ledger: &DivergenceLedger,
    contract_hash: &str,
    view_engine: &str,
) -> String {
    build_subsumption_correspondence_ttl(&EL_FRAGMENT, ledger, contract_hash, view_engine)
}

/// Render the native⊒oracle RL-subsumption parity result as a bundle-borne
/// `logic:Correspondence` individual (the next edge of the EL ⊂ RL lattice).
///
/// Thin wrapper over the shared subsumption-correspondence builder, pinned to the RL
/// lattice edge; the RL closure is the larger OWL 2 RL/RDF deductive closure over
/// the 4-ary generic-triple encoding, and the reified claim shape is identical to
/// EL's (native ⊒ oracle, section/retraction, complete over-approximation, section
/// law discharged within the certified fragment).
pub fn build_rl_subsumption_correspondence_ttl(
    ledger: &DivergenceLedger,
    contract_hash: &str,
    view_engine: &str,
) -> String {
    build_subsumption_correspondence_ttl(&RL_FRAGMENT, ledger, contract_hash, view_engine)
}

/// Render the native⊒oracle DL-subsumption parity result as a bundle-borne
/// `logic:Correspondence` individual (the terminal edge of the EL ⊂ RL ⊂ DL
/// lattice).
///
/// Thin wrapper over the shared subsumption-correspondence builder, pinned to the DL
/// lattice edge. The certified fragment is the structured DL Horn closure (the EL
/// calculus plus the clash-detection DL rules); the reified claim shape
/// is identical to EL's and RL's (native ⊒ oracle, section/retraction, complete
/// over-approximation, section law discharged within the certified fragment).
pub fn build_dl_subsumption_correspondence_ttl(
    ledger: &DivergenceLedger,
    contract_hash: &str,
    view_engine: &str,
) -> String {
    build_subsumption_correspondence_ttl(&DL_FRAGMENT, ledger, contract_hash, view_engine)
}

/// Render a native⊒oracle fragment-subsumption parity result as a bundle-borne
/// `logic:Correspondence` individual (Part B of the native-chase promotion).
///
/// This REIFIES the gap-zero parity verdict as a first-class correspondence in
/// the existing correspondence calculus, reusing only declared `logic:`
/// vocabulary — no minted terms:
///
/// * `logic:correspondenceRelation logic:Subsumes` — the native closure subsumes
///   the oracle's (native ⊇ oracle);
/// * `logic:morphismClass logic:SectionRetraction` — the oracle closure is a
///   section/retraction of the native closure (`put ∘ get = id_S` over the fragment);
/// * `logic:preservationKind logic:CompleteOverApproximation` — the loss-ledger
///   polarity: the target does not miss answers, it may add some (native ⊇ oracle);
/// * `logic:mnemomorphic true` — the native get leg retains the full source
///   witness (the complete fragment closure), which discharges the section law;
/// * `logic:hasLawClaim` → a `logic:LawClaim` on `logic:SectionLaw` carrying
///   `logic:lawDischargeVerdict logic:ObligationDischarged` under
///   `logic:lawDischargeCondition logic:DischargeCertifiedFragment` — the exact
///   declared way to say "proved within a certified complete fragment" (the
///   profile over which the chase terminates and is complete);
/// * `logic:contractHash` + `logic:engine` — the proof-certificate binding to the
///   native contract and the two engines the parity ran between.
///
/// `fragment` selects the lattice edge (EL, RL, …): it keys the reified subjects
/// and names the profile in the prose, but the relation / morphism / preservation
/// / discharged-law claim is IDENTICAL across fragments (one calculus per edge).
/// The [`DivergenceLedger`] rides as the loss cell: its per-kind tallies are
/// carried as report-local `gmeow:` counts and every NON-`Agree` row is projected
/// to a `gmeow:Finding` (via [`divergence_findings`]) — a gap-zero ledger carries
/// zero findings, so a healthy run emits none. `contract_hash` is the native
/// contract digest ([`crate::reason::native_contract_hash`]); `view_engine` names
/// the comparison oracle.
fn build_subsumption_correspondence_ttl(
    fragment: &SubsumptionFragment,
    ledger: &DivergenceLedger,
    contract_hash: &str,
    view_engine: &str,
) -> String {
    let mut out = correspondence_header(fragment);
    out.push_str(&subsumption_correspondence_body(
        fragment,
        ledger,
        contract_hash,
        view_engine,
    ));
    out
}

/// The per-fragment body (everything after the prefix header) of a native⊒oracle
/// subsumption correspondence: the reified `logic:Correspondence`, its discharged
/// `logic:SectionLaw` claim, and the loss-cell findings. Factored out so the single
/// (`build_subsumption_correspondence_ttl`) builders share one claim shape.
fn subsumption_correspondence_body(
    fragment: &SubsumptionFragment,
    ledger: &DivergenceLedger,
    contract_hash: &str,
    view_engine: &str,
) -> String {
    let mut out = String::new();

    let slug = fragment.slug;
    let label = fragment.label;
    let correspondence = gmeow(&format!("{slug}-native-subsumption-correspondence"));
    let law_claim = gmeow(&format!("{slug}-native-subsumption-lawclaim"));

    out.push_str(&format!(
        "\n# --- the reified native ⊒ oracle {label} correspondence ---\n"
    ));
    out.push_str(&emit_resource(
        &correspondence,
        &[
            (
                RDF_TYPE.to_owned(),
                format!("<{}>", logic("Correspondence")),
            ),
            (
                logic("correspondenceRelation"),
                format!("<{}>", logic("Subsumes")),
            ),
            (
                logic("morphismClass"),
                format!("<{}>", logic("SectionRetraction")),
            ),
            (
                logic("preservationKind"),
                format!("<{}>", logic("CompleteOverApproximation")),
            ),
            (logic("mnemomorphic"), "true".to_owned()),
            (logic("hasLawClaim"), format!("<{law_claim}>")),
            (
                logic("contractHash"),
                format!("\"{}\"", escape_literal(contract_hash)),
            ),
            (
                logic("engine"),
                format!("\"native ⊒ {}\"", escape_literal(view_engine)),
            ),
            (
                RDFS_COMMENT.to_owned(),
                format!(
                    "\"native subsumes {} on the certified {label} fragment, proved gap-zero\"@en",
                    escape_literal(view_engine)
                ),
            ),
            (gmeow("agreeCount"), ledger.agree.to_string()),
            (gmeow("nativeOnlyCount"), ledger.native_only.to_string()),
            (gmeow("oracleOnlyCount"), ledger.oracle_only.to_string()),
            (gmeow("dlGapCount"), ledger.dl_gap.to_string()),
        ],
    ));

    // The section-law claim, discharged within the certified fragment — the
    // declared "proved in a certified complete fragment" status (no minted term).
    out.push_str("\n# --- the discharged section-law claim ---\n");
    out.push_str(&emit_resource(
        &law_claim,
        &[
            (RDF_TYPE.to_owned(), format!("<{}>", logic("LawClaim"))),
            (logic("lawClaimed"), format!("<{}>", logic("SectionLaw"))),
            (
                logic("lawDischargeVerdict"),
                format!("<{}>", logic("ObligationDischarged")),
            ),
            (
                logic("lawDischargeCondition"),
                format!("<{}>", logic("DischargeCertifiedFragment")),
            ),
        ],
    ));

    // The divergence ledger as the loss cell: every NON-Agree row is a gmeow:Finding.
    // A gap-zero (native ⊒ oracle, no oracle-only/dl-gap) run emits zero findings.
    out.push_str("\n# --- loss cell: the native↔oracle divergence ledger (findings) ---\n");
    for (index, finding) in divergence_findings(ledger).iter().enumerate() {
        out.push_str(&emit_resource(
            &gmeow(&format!("{slug}-correspondence-finding-{index}")),
            &[
                (RDF_TYPE.to_owned(), format!("<{}>", gmeow("Finding"))),
                (
                    gmeow("findingCode"),
                    format!("\"{}\"@en", escape_literal(&finding.code)),
                ),
                (
                    RDFS_COMMENT.to_owned(),
                    format!("\"{}\"@en", escape_literal(&finding.message)),
                ),
            ],
        ));
    }

    out
}

/// Escape a string for embedding in a double-quoted Turtle literal (mirrors the
/// gmeow-rdf emitter's literal escaping; inlined here for ledger string literals
/// that are not full [`purrdf::RdfLiteral`] terms).
pub(crate) fn escape_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reason::dl::{DlCoverage, DlVerdict, InconsistencyWitness};
    use crate::reason::el::InferredAxiom;
    use crate::result::ResultProvenance;

    /// A native provenance bundle for the test results.
    fn prov() -> ResultProvenance {
        ResultProvenance::native("test-contract", "")
    }

    fn axiom(s: &str, p: &str, o: &str, rule: Option<&str>) -> InferredAxiom {
        InferredAxiom {
            subject: s.to_owned(),
            predicate: p.to_owned(),
            object: o.to_owned(),
            world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
            is_edb: false,
            rule_name: rule.map(str::to_owned),
            premises: vec![(
                "http://example.org/A".to_owned(),
                RDFS_SUBCLASS_OF.to_owned(),
                "<http://example.org/B>".to_owned(),
            )],
        }
    }

    #[test]
    fn premise_object_preserves_iris_and_literals() {
        // An IRI premise object round-trips to a bare IRI term.
        assert_eq!(
            premise_object("<http://example.org/B>"),
            RdfTerm::iri("http://example.org/B")
        );
        // A typed literal stays a literal — emitting it as an IRI would produce
        // invalid Turtle in the proof skeleton (CodeRabbit review).
        assert_eq!(
            emit_term(&premise_object(
                "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
            )),
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        // Language-tagged and simple (xsd:string) literals likewise round-trip.
        assert_eq!(emit_term(&premise_object("\"hi\"@en")), "\"hi\"@en");
        assert_eq!(emit_term(&premise_object("\"plain\"")), "\"plain\"");
    }

    fn result_with(inferred: Vec<InferredAxiom>, consistent: bool) -> ReasoningResult {
        let verdict = DlVerdict {
            consistent,
            unsatisfiable_classes: vec![],
            // An inconsistent verdict folds to information=both, which requires a
            // justifying witness; supply one so the (debug-asserted) invariant holds.
            inconsistencies: if consistent {
                vec![]
            } else {
                vec![InconsistencyWitness {
                    individual: "http://example.org/x".to_owned(),
                    world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
                    premises: vec![],
                }]
            },
            coverage: DlCoverage {
                present: vec![],
                decided: vec![],
                unsupported: vec![],
            },
            gaps: vec![],
        };
        ReasoningResult::from_dl_verdict(inferred, &verdict, prov())
    }

    #[test]
    fn closure_emits_triple_and_reifier_with_provenance() {
        let result = result_with(
            vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                Some("el:subClassOf-transitive"),
            )],
            true,
        );
        let ttl = build_inferred_closure_ttl(&result, None).unwrap();
        assert!(ttl.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
        assert!(ttl.contains("rdf-syntax-ns#reifies> <<( "));
        assert!(ttl.contains("rule/el%3AsubClassOf-transitive"));
        assert!(
            ttl.contains("gmeow/inferenceKind> <https://blackcatinformatics.ca/gmeow/Deduction>")
        );
        assert!(
            ttl.contains("gmeow/inWorld> <https://blackcatinformatics.ca/gmeow/graph/imports>")
        );
    }

    #[test]
    fn closure_skips_edb_axioms() {
        let mut edb = axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/B",
            None,
        );
        edb.is_edb = true;
        let result = result_with(vec![edb], true);
        let ttl = build_inferred_closure_ttl(&result, None).unwrap();
        assert!(!ttl.contains("reifies"));
    }

    #[test]
    fn closure_missing_rule_name_fails_loud() {
        let result = result_with(
            vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                None,
            )],
            true,
        );
        let err = build_inferred_closure_ttl(&result, None).unwrap_err();
        assert!(err.message().contains("no rule_name"), "got: {err}");
    }

    #[test]
    fn explanations_emit_derivation_with_premise() {
        let result = result_with(
            vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                Some("el:subClassOf-transitive"),
            )],
            true,
        );
        let ttl = build_explanations_ttl(&result).unwrap();
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/Derivation>"));
        assert!(ttl.contains("gmeow/concludes> <<( "));
        assert!(ttl.contains("gmeow/hasPremise> <<( <http://example.org/A>"));
        assert!(ttl.contains("\"derivation of an inferred axiom\"@en"));
    }

    #[test]
    fn ledger_header_entries_gaps_and_counts() {
        let verdict = DlVerdict {
            consistent: false,
            unsatisfiable_classes: vec![],
            // information=both needs a justifying witness (invariant).
            inconsistencies: vec![InconsistencyWitness {
                individual: "http://example.org/x".to_owned(),
                world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
                premises: vec![],
            }],
            coverage: DlCoverage {
                present: vec!["complementOf".to_owned()],
                decided: vec![],
                unsupported: vec!["complementOf".to_owned()],
            },
            // gaps are reconstructed from coverage.unsupported by the builder, so
            // the input gaps here are immaterial to the ledger output.
            gaps: vec![],
        };
        let result = ReasoningResult::from_dl_verdict(
            vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                Some("el:subClassOf-transitive"),
            )],
            &verdict,
            prov(),
        );
        let ttl = build_dl_el_ledger_ttl(&result);
        assert!(ttl.contains("gmeow/consistent> false"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/CrosscheckLedger>"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/LedgerEntry>"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/DlGap>"));
        assert!(ttl.contains("reason.dl-gap.complementOf"));
        assert!(ttl.contains("gmeow/entailmentCount> 1"));
        assert!(ttl.contains("gmeow/gapCount> 1"));
    }

    #[test]
    fn reasoning_result_ttl_emits_status_fields_and_certificate() {
        // A consistent run: supported, completed, complete-for-fragment.
        let result = result_with(vec![], true);
        let ttl = build_reasoning_result_ttl(&result);
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/logic/ReasoningResult>"));
        assert!(
            ttl.contains("logic/resultInput> <https://blackcatinformatics.ca/logic/InputValid>")
        );
        assert!(ttl.contains(
            "logic/resultEvaluation> <https://blackcatinformatics.ca/logic/EvaluationCompleted>"
        ));
        assert!(ttl.contains(
            "logic/resultCompleteness> <https://blackcatinformatics.ca/logic/CompleteForFragment>"
        ));
        assert!(ttl.contains(
            "logic/resultInformation> <https://blackcatinformatics.ca/logic/InfoSupported>"
        ));
        assert!(ttl.contains("logic/contractHash>"));
        assert!(ttl.contains("logic/engine>"));
    }

    #[test]
    fn reasoning_result_ttl_inconsistent_is_both_with_witness() {
        // An inconsistent run: information=both, carrying a contradiction witness.
        let result = result_with(vec![], false);
        let ttl = build_reasoning_result_ttl(&result);
        assert!(
            ttl.contains(
                "logic/resultInformation> <https://blackcatinformatics.ca/logic/InfoBoth>"
            )
        );
        assert!(
            ttl.contains("logic/contradictionWitness> <http://example.org/x>"),
            "the glut must carry its witness: {ttl}"
        );
    }

    #[test]
    fn proof_and_counterproof_derivation_ids_are_sanitized_by_bare_iri() {
        // bare_iri strips a surrounding `<>` pair from a derivation_id.
        // A derivation_id stored as "<urn:x>" must emit as `<urn:x>`, NOT `<<urn:x>>`.
        use crate::result::{DerivationRef, InformationState};
        use std::collections::BTreeSet;

        let mut result = result_with(vec![], true);
        // Inject a proof and counterproof whose derivation_id is pre-wrapped in `<>`.
        // This simulates a derivation_id that accidentally carries angle-bracket delimiters.
        result.provenance.proof = Some(DerivationRef {
            derivation_id: "<urn:proof-x>".to_owned(),
            cited_iris: BTreeSet::new(),
        });
        result.provenance.counterproof = Some(DerivationRef {
            derivation_id: "<urn:counterproof-x>".to_owned(),
            cited_iris: BTreeSet::new(),
        });
        // Force information=both so validate() does not fire the glut-needs-witness
        // invariant. We override the information state directly; the unit test is
        // checking IRI sanitization, not state-machine rules.
        result.information = InformationState::Both;

        let ttl = build_reasoning_result_ttl(&result);

        // The emitted lines must use exactly one pair of angle brackets, not doubled.
        assert!(
            ttl.contains("logic/resultProof> <urn:proof-x>"),
            "bare_iri must strip the surrounding <> from the proof derivation_id; got:\n{ttl}"
        );
        assert!(
            ttl.contains("logic/resultCounterproof> <urn:counterproof-x>"),
            "bare_iri must strip the surrounding <> from the counterproof derivation_id; got:\n{ttl}"
        );
        // Regression guard: <<urn:…>> must NOT appear (double angle brackets = invalid Turtle).
        assert!(
            !ttl.contains("<<urn:"),
            "double angle-bracket leaked into Turtle output; got:\n{ttl}"
        );
    }
}
