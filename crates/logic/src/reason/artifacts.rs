// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native RDF 1.2 Turtle artifact builders for the reasoning lane (issue #666).
//!
//! The Java/Docker-free authority lane (Principles 17/18) emits three committed
//! artifacts from a single [`ReasonResult`]:
//!
//! * **inferred-closure** — the told-vs-inferred derived axioms, each carrying an
//!   RDF 1.2 reifier annotated with its derivation provenance.
//! * **reasoning-explanations** — a per-axiom proof skeleton linking each
//!   conclusion (a triple term) to its premises and firing rule.
//! * **dl-el-crosscheck-report** — the report-only native↔oracle divergence
//!   ledger (#666 enforces; this lane only records the native verdict).
//!
//! These builders are the Rust port of the retired Python emitters
//! (`gmeow_tools.reason.build_*_ttl`). They serialize via the gmeow-rdf
//! [`gmeow_rdf::turtle`] emitter (clean full-IRI RDF 1.2), so the structure
//! (`[] rdf:reifies << … >>`, triple-term objects, anonymous reifiers) matches
//! the committed artifacts and the drift gate (RDFC-1.0 isomorphism) stays green.

use gmeow_rdf::turtle::{emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};
use gmeow_rdf::{RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfStore, RdfTerm, RdfTriple};
use oxigraph::model::vocab::xsd;
use oxigraph::model::{Literal, Term};

use crate::encode::decode_nemo_term;
use crate::reason::el::InferredAxiom;
use crate::reason::ReasonResult;

// ── Namespaces ──────────────────────────────────────────────────────────────────

/// The gmeow vocabulary namespace (term IRIs are `GMEOW_NS + local`).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
/// The IRI base for a reasoning rule (`GMEOW_NS + "rule/"`, percent-encoded name).
const RULE_IRI_BASE: &str = "https://blackcatinformatics.ca/gmeow/rule/";
/// `rdfs:subClassOf` — the subsumption predicate the ledger records native-only.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// `prov:wasDerivedBy`.
const PROV_WAS_DERIVED_BY: &str = "http://www.w3.org/ns/prov#wasDerivedBy";
/// `rdf:type` (emitted full so the canonical compare never depends on `a`).
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:label`.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `rdfs:comment`.
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";

/// `gmeow:` term IRI helper.
fn gmeow(local: &str) -> String {
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
# GMEOW native vs ELK/HermiT DL/EL crosscheck ledger (REPORT-ONLY).
# Built from the native EL/DL reasoning lane ONLY (Java/Docker-free). The
# oracle comparison and divergence ENFORCEMENT are deferred to the
# classic-cross-check lane (#666); this ledger records the native verdict,
# the native-only subsumption entailments, and the beyond-EL gaps. DO NOT
# EDIT.
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
fn derived_rule_iri(axiom: &InferredAxiom) -> Result<String, String> {
    match axiom.rule_name.as_deref() {
        Some(name) if !name.is_empty() => Ok(rule_iri(RULE_IRI_BASE, name)),
        _ => Err(format!(
            "derived axiom has no rule_name; the native engine must label every \
             inferred (non-EDB) axiom with the rule that produced it: \
             <{}> <{}> <{}>",
            axiom.subject, axiom.predicate, axiom.object
        )),
    }
}

/// Normalize a native-engine term into a bare IRI string.
///
/// The native engine emits subjects/predicates/worlds as bare IRI strings, but
/// objects already wrapped in `<...>` (the N3 display form of
/// `decode_nemo_term`). This collapses both to the bare inner IRI so a single
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
/// run-to-run (issue #883). Premises are canonicalized (sorted) at construction
/// time in `run_reasoning`, so this helper only orders the derived set by full
/// content so all three artifacts serialize byte-identically regardless of chase
/// order — killing the drift class at the single chokepoint every builder funnels
/// through. `sort` (stable) is used deliberately WITHOUT dedup: fully
/// content-equal duplicate derivations must be preserved so the emitted multiset
/// is unchanged.
fn derived_sorted(result: &ReasonResult) -> Vec<&InferredAxiom> {
    let mut axioms: Vec<&InferredAxiom> = result.inferred.iter().filter(|a| !a.is_edb).collect();
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
    result: &ReasonResult,
    merge_asserted: Option<&dyn RdfStore>,
) -> Result<String, String> {
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
pub fn build_explanations_ttl(result: &ReasonResult) -> Result<String, String> {
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
    match decode_nemo_term(display) {
        Ok(Term::NamedNode(node)) => RdfTerm::iri(node.into_string()),
        Ok(Term::Literal(literal)) => RdfTerm::literal(rdf_literal_from_oxigraph(&literal)),
        _ => iri_term(display),
    }
}

/// Convert an oxigraph [`Literal`] to the model [`RdfLiteral`], preserving a
/// language tag or a non-`xsd:string` datatype so [`emit_term`] re-serializes it
/// to the same Turtle literal form.
fn rdf_literal_from_oxigraph(literal: &Literal) -> RdfLiteral {
    if let Some(language) = literal.language() {
        RdfLiteral::language_tagged(literal.value(), language)
    } else if literal.datatype() == xsd::STRING {
        RdfLiteral::simple(literal.value())
    } else {
        RdfLiteral::typed(literal.value(), literal.datatype().as_str())
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

/// Render the report-only native↔oracle DL/EL crosscheck ledger as Turtle.
///
/// Built from the native results ONLY (the gate stays Java/Docker-free): the
/// oracle comparison and divergence enforcement are deferred to the
/// `classic-cross-check` lane (#666). Emits the ledger header (consistency
/// verdict + report-only note), one `gmeow:LedgerEntry` of kind
/// `gmeow:NativeOnly` per derived `rdfs:subClassOf` entailment, one
/// `gmeow:DlGap` per beyond-EL gap, and the entailment/gap counts.
pub fn build_dl_el_ledger_ttl(result: &ReasonResult) -> String {
    const DEFERRED_NOTE: &str = "oracle comparison deferred to classic-cross-check #666";
    let mut out = String::from(LEDGER_HEADER);

    out.push_str("\n# --- ledger header (report-only; #666 enforces) ---\n");
    out.push_str(&emit_resource(
        &gmeow("dl-el-crosscheck"),
        &[
            (
                RDF_TYPE.to_owned(),
                format!("<{}>", gmeow("CrosscheckLedger")),
            ),
            (
                gmeow("consistent"),
                if result.verdict.consistent {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                },
            ),
            (
                gmeow("oracleCrosscheck"),
                "\"deferred to classic-cross-check (#666); ledger is report-only\"@en".to_owned(),
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
                    format!("\"{}\"@en", escape_literal(DEFERRED_NOTE)),
                ),
            ],
        ));
    }

    // Beyond-EL DL gaps.
    out.push_str("\n# --- beyond-EL DL gaps ---\n");
    for (index, gap) in result.verdict.gaps.iter().enumerate() {
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
            (gmeow("gapCount"), result.verdict.gaps.len().to_string()),
        ],
    ));

    out
}

/// Escape a string for embedding in a double-quoted Turtle literal (mirrors the
/// gmeow-rdf emitter's literal escaping; inlined here for ledger string literals
/// that are not full [`gmeow_rdf::RdfLiteral`] terms).
fn escape_literal(value: &str) -> String {
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
fn asserted_turtle(store: &dyn RdfStore) -> Result<String, String> {
    let mut out = String::new();
    for quad in store.quads() {
        let quad = quad.map_err(|e| format!("asserted quad read error: {e:?}"))?;
        out.push_str(&emit_quad(&quad));
    }
    for reifier in store.reifiers() {
        let reifier = reifier.map_err(|e| format!("asserted reifier read error: {e:?}"))?;
        out.push_str(&emit_reifier(&reifier, &[]));
    }
    for annotation in store.annotations() {
        let annotation =
            annotation.map_err(|e| format!("asserted annotation read error: {e:?}"))?;
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
    use crate::reason::dl::DlVerdict;
    use crate::reason::el::InferredAxiom;

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
        // invalid Turtle in the proof skeleton (#666 / CodeRabbit review).
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

    fn result_with(inferred: Vec<InferredAxiom>, consistent: bool) -> ReasonResult {
        ReasonResult {
            inferred,
            verdict: DlVerdict {
                consistent,
                unsatisfiable_classes: vec![],
                inconsistencies: vec![],
                gaps: vec![],
            },
        }
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
        assert!(ttl.contains("rdf-syntax-ns#reifies> << "));
        assert!(ttl.contains("rule/el%3AsubClassOf-transitive"));
        assert!(
            ttl.contains("gmeow/inferenceKind> <https://blackcatinformatics.ca/gmeow/Deduction>")
        );
        assert!(ttl.contains("gmeow/inWorld> <https://blackcatinformatics.ca/gmeow/graph/imports>"));
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
        assert!(err.contains("no rule_name"), "got: {err}");
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
        assert!(ttl.contains("gmeow/concludes> << "));
        assert!(ttl.contains("gmeow/hasPremise> << <http://example.org/A>"));
        assert!(ttl.contains("\"derivation of an inferred axiom\"@en"));
    }

    #[test]
    fn ledger_header_entries_gaps_and_counts() {
        let mut verdict = DlVerdict {
            consistent: false,
            unsatisfiable_classes: vec![],
            inconsistencies: vec![],
            gaps: vec![gmeow_rdf::RdfLoss::new(
                "reason.dl-gap.complementOf",
                "beyond EL",
            )],
        };
        verdict.consistent = false;
        let result = ReasonResult {
            inferred: vec![axiom(
                "http://example.org/A",
                RDFS_SUBCLASS_OF,
                "http://example.org/C",
                Some("el:subClassOf-transitive"),
            )],
            verdict,
        };
        let ttl = build_dl_el_ledger_ttl(&result);
        assert!(ttl.contains("gmeow/consistent> false"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/CrosscheckLedger>"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/LedgerEntry>"));
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/DlGap>"));
        assert!(ttl.contains("reason.dl-gap.complementOf"));
        assert!(ttl.contains("gmeow/entailmentCount> 1"));
        assert!(ttl.contains("gmeow/gapCount> 1"));
    }
}
