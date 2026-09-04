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
    RdfAnnotation, RdfDataset, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple, TermValue,
};

use crate::explain::{canonical_rule_iri, encode_receipt_rule_identity, receipt_for_axiom};
use crate::math_expression::{MATH_ALPHA_EQUIVALENCE_CLASS, MATH_ALPHA_EQUIVALENCE_CLASS_TYPE};
use crate::reason::dl::gaps_from_unsupported;
use crate::reason::el::InferredAxiom;
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
        let receipt = receipt_for_axiom(axiom);
        let rule = RdfTerm::iri(derived_rule_iri(axiom)?);
        let world = RdfTerm::iri(bare_iri(&axiom.world).to_owned());
        out.push_str(&emit_quad(&RdfQuad::new(
            triple.subject.clone(),
            triple.predicate.clone(),
            triple.object.clone(),
        )));
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
        annotations.extend(
            receipt
                .row
                .source_quad_ids
                .into_iter()
                .map(|source| (PROV_WAS_DERIVED_FROM.to_owned(), RdfTerm::iri(source))),
        );
        out.push_str(&emit_reifier(&reifier, &annotations));
    }
    out.push_str(&alpha_equivalence_section(alpha_edges));
    Ok(out)
}

/// Serialize the `math:alphaEquivalenceClass` edges as the closure's final section.
///
/// Empty — not even a banner — when there are no edges: an EDB carrying no `math:`
/// expression decides no identities, and a bare section header would read as a claim that
/// it did.
fn alpha_equivalence_section(alpha_edges: &[(String, String)]) -> String {
    if alpha_edges.is_empty() {
        return String::new();
    }
    let rule = RdfTerm::iri(rule_iri(RULE_IRI_BASE, MATH_EXPRESSION_IDENTITY_RULE));
    let mut out = String::from("\n# --- derived math: expression alpha-equivalence identity ---\n");
    let mut typed: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (expression, alpha_class) in alpha_edges {
        let triple = RdfTriple::new(
            RdfTerm::iri(expression.clone()),
            MATH_ALPHA_EQUIVALENCE_CLASS.to_owned(),
            RdfTerm::iri(alpha_class.clone()),
        );
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
                (gmeow("viaRule"), rule.clone()),
                (gmeow("inferenceKind"), RdfTerm::iri(gmeow("Deduction"))),
            ],
        ));
        typed.insert(alpha_class.as_str());
    }
    for alpha_class in typed {
        out.push_str(&emit_quad(&RdfQuad::new(
            RdfTerm::iri(alpha_class.to_owned()),
            RDF_TYPE.to_owned(),
            RdfTerm::iri(MATH_ALPHA_EQUIVALENCE_CLASS_TYPE.to_owned()),
        )));
    }
    out
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
    let mut out = String::from(EXPLANATIONS_HEADER);
    out.push_str("\n# --- derivation proof skeletons ---\n");
    for axiom in derived_sorted(result) {
        let receipt = receipt_for_axiom(axiom);
        let rule = derived_rule_iri(axiom)?;
        let mut properties: Vec<(String, RdfTerm)> = vec![
            (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("Derivation"))),
            (gmeow("concludes"), RdfTerm::triple(axiom_triple(axiom))),
            (
                LOGIC_DERIVATION_IDENTIFIER.to_owned(),
                RdfTerm::literal(RdfLiteral::simple(receipt.row.derivation_id.clone())),
            ),
        ];
        for (ps, pp, po) in &axiom.premises {
            let premise = RdfTriple::new(RdfTerm::iri(ps.clone()), pp.clone(), premise_object(po));
            properties.push((gmeow("hasPremise"), RdfTerm::triple(premise)));
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

// ── dl-el-crosscheck-report ─────────────────────────────────────────────────────

/// Render the native DL⊇EL crosscheck ledger as Turtle.
///
/// Built from the native results ONLY (the gate stays Java/Docker-free). Emits
/// the ledger header, one `gmeow:LedgerEntry` of kind `gmeow:NativeOnly` per
/// derived `rdfs:subClassOf` entailment, one `gmeow:DlGap` per native coverage
/// defect, and the entailment/gap counts. The committed bundle is expected to
/// have zero `DlGap` rows.
pub fn build_dl_el_ledger_ttl(result: &ReasoningResult) -> String {
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
            iri_term(&axiom.object),
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
        // invalid Turtle in the proof skeleton.
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
            boundary_findings: vec![],
        };
        ReasoningResult::from_dl_verdict(inferred, &verdict, prov())
    }

    #[test]
    fn closure_emits_triple_and_reifier_with_provenance() {
        let derived = axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/C",
            Some("el:subClassOf-transitive"),
        );
        let receipt = receipt_for_axiom(&derived);
        let canonical_rule = canonical_rule_iri("el:subClassOf-transitive");
        assert_eq!(receipt.row.rule_iri, canonical_rule);
        assert_eq!(receipt.raw_rule_identity, "el:subClassOf-transitive");
        assert_eq!(
            receipt.row.derivation_id,
            crate::provenance::mint_derivation_id(
                "el:subClassOf-transitive",
                &[receipt.row.source_quad_ids[0].as_str()]
            ),
            "the receipt hash preserves the native firing identity bytes"
        );
        let result = result_with(vec![derived], true);
        let ttl = build_inferred_closure_ttl(&result, None, &[]).unwrap();
        assert!(ttl.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
        assert!(ttl.contains("rdf-syntax-ns#reifies> <<( "));
        assert!(ttl.contains(&format!("<{}> <{canonical_rule}>", gmeow("viaRule"))));
        assert!(
            !ttl.contains("<el:subClassOf-transitive>"),
            "the raw firing label is receipt data, never a public rule resource"
        );
        assert!(ttl.contains("receipt-rule-identity"));
        assert!(ttl.contains("el:subClassOf-transitive"));
        assert!(ttl.contains(&receipt.row.derivation_id));
        assert!(ttl.contains(&receipt.row.source_quad_ids[0]));
        assert!(
            ttl.contains("gmeow/inferenceKind> <https://blackcatinformatics.ca/gmeow/Deduction>")
        );
        assert!(
            ttl.contains("gmeow/inWorld> <https://blackcatinformatics.ca/gmeow/graph/imports>")
        );
    }

    /// The α-equivalence section is the SHIPPED half of the expression-identity derivation:
    /// two α-equivalent expressions must land on ONE class individual, that individual must be
    /// typed exactly once however many expressions reach it, and every edge must carry the
    /// derivation's own rule provenance rather than borrowing an EL/DL rule's.
    #[test]
    fn closure_emits_one_joinable_class_for_alpha_equivalent_expressions() {
        const CLASS: &str = "https://blackcatinformatics.ca/math/alphaClass/deadbeef";
        let result = result_with(
            vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                Some("el:subClassOf-transitive"),
            )],
            true,
        );
        let edges = vec![
            ("http://example.org/first".to_owned(), CLASS.to_owned()),
            ("http://example.org/second".to_owned(), CLASS.to_owned()),
        ];
        let ttl = build_inferred_closure_ttl(&result, None, &edges).unwrap();
        for expression in ["first", "second"] {
            assert!(
                ttl.contains(&format!(
                    "<http://example.org/{expression}> <{MATH_ALPHA_EQUIVALENCE_CLASS}> <{CLASS}> ."
                )),
                "the α-class edge of {expression} must be an ordinary joinable triple"
            );
        }
        let typing = format!("<{CLASS}> <{RDF_TYPE}> <{MATH_ALPHA_EQUIVALENCE_CLASS_TYPE}> .");
        assert_eq!(
            ttl.matches(typing.as_str()).count(),
            1,
            "the shared class individual is typed exactly ONCE, not once per expression"
        );
        assert!(
            ttl.contains("rule/math-expression-identity"),
            "the α edges carry the expression-identity derivation's own rule provenance"
        );
    }

    /// No `math:` expression in the EDB means no identity was decided, so the section — banner
    /// included — is absent. A bare header would read as a decision that never happened.
    #[test]
    fn closure_omits_the_alpha_section_entirely_when_no_expression_is_decided() {
        let result = result_with(
            vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                Some("el:subClassOf-transitive"),
            )],
            true,
        );
        let ttl = build_inferred_closure_ttl(&result, None, &[]).unwrap();
        assert!(!ttl.contains("alpha-equivalence"));
        assert!(!ttl.contains(MATH_ALPHA_EQUIVALENCE_CLASS));
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
        let ttl = build_inferred_closure_ttl(&result, None, &[]).unwrap();
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
        let err = build_inferred_closure_ttl(&result, None, &[]).unwrap_err();
        assert!(err.message().contains("no rule_name"), "got: {err}");
    }

    #[test]
    fn explanations_emit_derivation_with_premise() {
        let derived = axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/C",
            Some("el:subClassOf-transitive"),
        );
        let receipt = receipt_for_axiom(&derived);
        let canonical_rule = canonical_rule_iri("el:subClassOf-transitive");
        let result = result_with(vec![derived], true);
        let ttl = build_explanations_ttl(&result).unwrap();
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/Derivation>"));
        assert!(ttl.contains("gmeow/concludes> <<( "));
        assert!(ttl.contains("gmeow/hasPremise> <<( <http://example.org/A>"));
        assert!(ttl.contains(&format!("<{}> <{canonical_rule}>", gmeow("viaRule"))));
        assert!(
            !ttl.contains("<el:subClassOf-transitive>"),
            "the raw firing label is receipt data, never a public rule resource"
        );
        assert!(ttl.contains("receipt-rule-identity"));
        assert!(ttl.contains("el:subClassOf-transitive"));
        assert!(ttl.contains(&receipt.row.derivation_id));
        assert!(ttl.contains("\"derivation of an inferred axiom\"@en"));
    }

    #[test]
    fn modal_artifacts_retain_the_exact_rule_sources_and_derivation_identity() {
        let modal = InferredAxiom {
            subject: "https://example.org/modal/F".to_owned(),
            predicate: crate::modal::MODAL_NECESSITY_FAILS.to_owned(),
            object: "<https://example.org/modal/B>".to_owned(),
            world: "https://example.org/modal/w0".to_owned(),
            is_edb: false,
            rule_name: Some(crate::modal::MODAL_RULE_IRI.to_owned()),
            premises: vec![(
                "https://example.org/modal/a".to_owned(),
                "https://example.org/modal/knows".to_owned(),
                "<https://example.org/modal/b>".to_owned(),
            )],
        };
        let receipt = receipt_for_axiom(&modal);
        let result = result_with(vec![modal], true);

        let closure = build_inferred_closure_ttl(&result, None, &[]).unwrap();
        let explanations = build_explanations_ttl(&result).unwrap();
        for artifact in [&closure, &explanations] {
            assert!(artifact.contains(&format!(
                "<{}> <{}>",
                gmeow("viaRule"),
                crate::modal::MODAL_RULE_IRI
            )));
            assert!(artifact.contains("receipt-rule-identity"));
            assert!(artifact.contains(&receipt.row.derivation_id));
            for source in &receipt.row.source_quad_ids {
                assert!(artifact.contains(source));
            }
        }
        assert!(
            explanations.contains(&format!("<{}>", receipt.row.derivation_id)),
            "the derivation is a named content-addressed resource"
        );
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
            boundary_findings: vec![],
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
        assert!(ttl.contains(&format!("gmeow/consistent> \"false\"^^<{XSD_BOOLEAN}>")));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/CrosscheckLedger>"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/LedgerEntry>"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/DlGap>"));
        assert!(ttl.contains("reason.dl-gap.complementOf"));
        assert!(ttl.contains(&format!("gmeow/entailmentCount> \"1\"^^<{XSD_INTEGER}>")));
        assert!(ttl.contains(&format!("gmeow/gapCount> \"1\"^^<{XSD_INTEGER}>")));
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
