// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, ontology-independent OWL-2 reasoning over the Nemo chase.
//!
//! This module hosts fixed entailment rule sets — like ELK's built-in
//! calculus — that run over an arbitrary TBox/ABox through the world-scoped
//! ternary gmeow encoding. Unlike the user-authored `logic:` programs the
//! the compiler pipeline projects, these rule sets are intrinsic to the
//! reasoner: they encode the OWL semantics themselves, not a domain ontology.
//!
//! Provides the EL subsumption closure ([`el`]), the predicate-as-DATA RL/DL
//! native closure ([`rl`] + [`dl`]), and the divergence ledger ([`ledger`])
//! comparing the native engine against the classic oracles.

pub mod artifacts;
pub mod dl;
pub mod el;
pub mod ledger;
pub mod perf_ledger;
pub mod rl;

pub use dl::{dl_consistency, DlVerdict, InconsistencyWitness, UnsatClass};
pub use el::{el_closure, ElClosure, InferredAxiom};
pub use ledger::{
    build_ledger, compare_consistency, compare_external_corpus, compare_subsumption,
    divergence_findings, dl_gap_rows, enforce, DivergenceKind, DivergenceLedger,
    ExternalComparison, LedgerRow, LedgerVerdict,
};
pub use rl::{rl_closure, RlClosure, RlTriple};

use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow};
use crate::result::{ReasoningResult, ResultProvenance};
use crate::store::WorldStore;
use gmeow_rdf::{RdfDataset, RdfDatasetBuilder};

/// The content-addressed identity of the native EL/DL/RL reasoning contract —
/// the `contract_hash` every native-reason result is produced under.
///
/// The hash covers ALL source that defines the reasoning contract:
/// * the three fixed rule texts (`dl_rules()`, `EL_RULES`, `RL_RULES`) whose
///   change alters which axioms the Nemo chase derives;
/// * the full source of `dl.rs`, which owns the post-pass functions
///   `augment_inferred_with_dl`, `verdict_from_inferred`, `scan_coverage`, and
///   `classify_coverage` — any edit to those changes the contract semantics even
///   when the rule text is unchanged;
/// * the source of this file (`mod.rs`), which owns the `run_reasoning` and
///   `reason_closure` orchestration glue.
///
/// A change to any of these files will produce a different hash, invalidating
/// cached results produced under the old contract.
fn native_contract_hash() -> String {
    let contract = format!(
        "{dl_rules}\n{el_rules}\n{rl_rules}\n{dl_src}\n{mod_src}",
        dl_rules = dl::dl_rules(),
        el_rules = el::EL_RULES,
        rl_rules = rl::RL_RULES,
        dl_src = include_str!("dl.rs"),
        mod_src = include_str!("mod.rs"),
    );
    crate::provenance::sha1_hex(&contract)
}

/// Run the native single-chase pipeline and return the shared
/// `(closure, DlVerdict)` it produces.
///
/// `run_reasoning → augment_inferred_with_dl → sort → verdict_from_inferred`:
/// the closure is the asserted + derived IRI-object triples; the verdict is the
/// DL consistency / unsatisfiability record read off that same closure. Both the
/// typed [`reason_all`] result and the verdict-only [`dl::dl_consistency`] entry
/// point fold from this one pipeline so they can never disagree.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo chase
/// fails to parse/validate/evaluate/decode, or if coverage/consistency scanning
/// fails.
pub(crate) fn reason_closure(
    edb: &RdfDataset,
) -> Result<(Vec<InferredAxiom>, dl::DlVerdict), String> {
    let mut inferred = run_reasoning(edb, &dl::dl_rules())?;
    dl::augment_inferred_with_dl(&mut inferred, edb)?;
    inferred.sort();
    let verdict = dl::verdict_from_inferred(&inferred, edb)?;
    Ok((inferred, verdict))
}

/// Run native predicate-as-DATA entailment + DL consistency, returning the typed
/// [`ReasoningResult`] (#768, ME2) — the single shared result model every
/// consumer reads.
///
/// The DL verdict is folded into the result via
/// [`ReasoningResult::from_dl_verdict`]: an inconsistent verdict becomes
/// `information=both` carrying its contradiction witnesses; a consistent verdict
/// is `information=supported` (conclusively, when no construct is uncovered);
/// uncovered DL constructs surface in `preservation.unsupported_constructs` and
/// drop the completeness to `incomplete`. The DL-only diagnostics not part of the
/// shared model (the construct coverage inventory, the unsatisfiable-class set)
/// are recovered from the shared closure by [`dl::scan_coverage`] /
/// [`dl::unsatisfiable_from_inferred`] where a consumer needs them.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate/decode, or if coverage/consistency
/// scanning fails.
pub fn reason_all(edb: &RdfDataset) -> Result<ReasoningResult, String> {
    let (inferred, verdict) = reason_closure(edb)?;
    Ok(typed_result(inferred, &verdict))
}

/// Reason over a canonical [`gmeow_logic_compile::ir::LogicProgram`]'s rules AND full-FOL formulas against `edb`,
/// returning the shared typed [`ReasoningResult`].
///
/// This is the program-carrying entry the full-FOL formula layer flows through to actual
/// evaluation. The pipeline is `Formula → relational-core lowering → EvalRule → RLS →
/// chase`, run alongside the program's own Horn rules (the canonical Nemo projection) and
/// the fixed DL calculus, in one chase over `edb`:
///
/// 1. `relational_core::lower_formulas` legalizes each formula: the Horn-expressible
///    fragment becomes evaluable rules; everything beyond it (disjunctive heads,
///    `∃`-functions, sequence markers, …) is carried as flagged residue.
/// 2. The evaluable rules are rendered to Nemo `.rls` ([`crate::rule_ir::eval_rules_to_rls`])
///    in the same ternary encoding as the program rules, so they join the same chase.
/// 3. The result's preservation claim UNIONS the lowering residue with the DL coverage gap
///    ([`ReasoningResult::from_dl_verdict_with_preservation`]): a non-evaluable formula is
///    disclosed (`{sound-under}` + `unsupported_constructs`), never silently absent.
///
/// The program's ground facts (axioms), if any, are expected in `edb` — the data graph is
/// the fact source, the program is the rule/formula source (the conformance-harness split).
///
/// # Errors
///
/// Returns `Err(String)` if the Nemo projection of the program rules fails (e.g. a head
/// variable unbound by any body atom), or if the chase fails to parse/validate/evaluate/decode.
pub fn reason_program(
    program: &gmeow_logic_compile::ir::LogicProgram,
    edb: &RdfDataset,
) -> Result<ReasoningResult, String> {
    use gmeow_logic_compile::projections::text::{extract_nemo_rules_section, project_nemo};

    // 1. Lower the full-FOL formulas through the relational-core waist.
    let (formula_rls, formula_preservation) = crate::relational_core::formula_eval_rls(program);

    // 2. The program's own Horn rules, via the canonical Nemo projection (rules section only;
    //    facts come from `edb`).
    let program_nemo = project_nemo(program)?;
    let program_rules = extract_nemo_rules_section(&program_nemo.content)?;

    // 3. Run program rules + formula-derived rules ALONGSIDE the fixed DL calculus, so the
    //    program's consequences and DL consistency are computed in one chase.
    let rules = format!("{}\n{program_rules}\n{formula_rls}", dl::dl_rules());
    let mut inferred = run_reasoning(edb, &rules)?;
    dl::augment_inferred_with_dl(&mut inferred, edb)?;
    inferred.sort();
    let verdict = dl::verdict_from_inferred(&inferred, edb)?;

    // 4. Fold into the shared result, unioning the formula-lowering residue into the
    //    preservation claim.
    let provenance = ResultProvenance::native(native_contract_hash(), "");
    Ok(ReasoningResult::from_dl_verdict_with_preservation(
        inferred,
        &verdict,
        &formula_preservation,
        provenance,
    ))
}

/// Reason over a user-supplied data graph MERGED with the bundle's axioms, returning
/// the same shared typed [`ReasoningResult`] as [`reason_all`].
///
/// The merge is the cross-dataset re-intern
/// ([`RdfDatasetBuilder::push_dataset`](gmeow_rdf::RdfDatasetBuilder::push_dataset)),
/// so it carries the FULL RDF 1.2 statement layer of both inputs — the user's
/// reifier bindings and annotations are not dropped. The chase then runs over the
/// single merged dataset, so an inconsistency entailed only by the user's data
/// against the bundled TBox surfaces as `information=both` with its contradiction
/// witnesses, exactly as a same-graph inconsistency would.
///
/// # Errors
///
/// Returns `Err(String)` if the merged dataset fails the freeze-time structural
/// contract, or if the chase fails to parse/validate/evaluate/decode.
pub fn reason_all_with_data(
    bundle: &RdfDataset,
    user: &RdfDataset,
) -> Result<ReasoningResult, String> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(bundle);
    builder.push_dataset(user);
    let merged = builder.freeze().map_err(|e| e.to_string())?;
    reason_all(&merged)
}

/// Fold a `(closure, DlVerdict)` pair into the typed [`ReasoningResult`] under the
/// native reasoning contract. Shared by [`reason_all`] and the PyO3 boundary so
/// the typed result and the historical DL dict are projected from one fold.
///
/// The native consistency run spans every world in the bundle; the per-axiom
/// worlds are carried on the closure payload, so the result-level context world
/// is left unset (the aggregate run is not pinned to one world).
pub(crate) fn typed_result(
    inferred: Vec<InferredAxiom>,
    verdict: &dl::DlVerdict,
) -> ReasoningResult {
    let provenance = ResultProvenance::native(native_contract_hash(), "");
    ReasoningResult::from_dl_verdict(inferred, verdict, provenance)
}

/// Decode one antecedent chase row into a `(subject, predicate, object)` triple.
///
/// The antecedent rows are the same ternary shape as derived rows: subject is
/// an IRI term, object is any Nemo term (decoded to its display string), and
/// the third value is the world string constant (dropped here — premises carry
/// only the triple shape).
fn decode_premise(row: &ChaseRow) -> Result<(String, String, String), String> {
    if row.values.len() != 3 {
        return Err(format!(
            "antecedent row has arity {} (expected 3): {row:?}",
            row.values.len()
        ));
    }
    let subject = decode_iri_term(&row.values[0])?;
    let object = decode_nemo_term(&row.values[1])?.to_string();
    Ok((subject, row.predicate.clone(), object))
}

/// Run a fixed entailment rule set over `edb` through the Nemo chase.
///
/// Loads `edb` into a fresh [`WorldStore`], encodes every quad of every world
/// into the ternary gmeow EDB, prepends those facts to `rules`, runs the chase,
/// and decodes every 3-arity chase row into an [`InferredAxiom`] carrying its
/// raw provenance (EDB/IDB flag, firing rule name, immediate premises).
///
/// This is the shared chase machinery both [`el::el_closure`] and
/// [`dl::dl_consistency`] build on: the rule set is the only difference.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate, or if a derived row fails to decode.
pub(crate) fn run_reasoning(edb: &RdfDataset, rules: &str) -> Result<Vec<InferredAxiom>, String> {
    // 1. Load the source into a fresh world-indexed store.
    let store = WorldStore::new();
    store.load_dataset(edb)?;

    // 2. Encode every IRI-object quad of every world into ternary EDB fact lines.
    //    The fixed EL/DL calculi only fire on axioms whose object is an IRI
    //    (subClassOf, type, disjointWith, equivalentClass, subPropertyOf), so a
    //    literal-object quad (an annotation such as rdfs:comment / dc:creator)
    //    can never participate in any rule. Skipping them is therefore sound for
    //    the closure AND the verdict, and it is also necessary: real ontology
    //    annotations carry embedded newlines that would split the line-based Nemo
    //    .rls program (`encode_literal` escapes `\`/`"` but not control chars).
    let mut edb_facts: Vec<String> = Vec::new();
    for world in store.worlds() {
        for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
            if !matches!(quad.object, oxigraph::model::Term::NamedNode(_)) {
                continue;
            }
            edb_facts.push(encode_quad_to_nemo_fact(
                &quad.subject,
                &quad.predicate,
                &quad.object,
                &world,
            ));
        }
    }

    // 3. Build the program and run the chase.
    let rls = format!("{}\n{}", edb_facts.join("\n"), rules);
    let rows = run_chase(rls)?;

    // 4. Decode each ternary row into an InferredAxiom.
    let mut inferred: Vec<InferredAxiom> = Vec::new();
    for rwp in &rows {
        let row = &rwp.row;
        // Every reasoning fact is the ternary `predicate(subject, object, world)`;
        // a row with any other arity is not an inferred axiom (e.g. an internal
        // bookkeeping atom the chase may surface), so skip it rather than misdecode.
        if row.values.len() != 3 {
            continue;
        }

        let predicate = row.predicate.clone();
        let subject = decode_iri_term(&row.values[0])?;
        let object = decode_nemo_term(&row.values[1])?.to_string();
        let world = decode_string_constant(&row.values[2])?;

        let prov = &rwp.provenance;
        let mut premises = prov
            .antecedent_rows
            .iter()
            .map(decode_premise)
            .collect::<Result<Vec<_>, String>>()?;
        premises.sort();

        inferred.push(InferredAxiom {
            subject,
            predicate,
            object,
            world,
            is_edb: prov.is_edb,
            rule_name: prov.rule_name.clone(),
            premises,
        });
    }

    Ok(inferred)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const X: &str = "http://gmeow.example/x";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<gmeow_rdf::RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn reason_all_single_chase_yields_inconsistent_and_nonempty_closure() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A — one chase must derive both the
        // subsumption closure AND the inconsistency verdict (x forced into Nothing).
        let store = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let result = reason_all(store.as_ref()).expect("reason_all should succeed");

        assert!(
            !result.is_consistent(),
            "x forced into owl:Nothing must make the verdict inconsistent (information=both)"
        );
        assert_eq!(
            result.information,
            crate::result::InformationState::Both,
            "an inconsistent verdict is the four-valued Belnap glut"
        );
        assert!(
            !result.inferred().is_empty(),
            "the subsumption closure must be non-empty (asserted + derived axioms)"
        );
        assert!(
            result
                .provenance
                .contradiction_witnesses
                .iter()
                .any(|w| w.individual == X),
            "x must be a contradiction witness: {:?}",
            result.provenance.contradiction_witnesses
        );
    }

    #[test]
    fn reason_all_with_data_merges_user_abox_into_bundle_tbox() {
        // The contradiction is entailed only ACROSS the two inputs: the disjointness
        // TBox lives in `bundle`, the offending individual `x : A` in `user`. Neither
        // alone is inconsistent; the merge must feed both to the chase.
        let bundle = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
        ]);
        let user = dataset(vec![quad(X, TYPE, A)]);

        // The user ABox on its own (no TBox) is consistent.
        let user_only = reason_all(user.as_ref()).expect("reason_all over user-only");
        assert!(
            user_only.is_consistent(),
            "x : A with no disjointness axioms is consistent"
        );

        // Merged with the bundle TBox, x is forced into owl:Nothing.
        let merged = reason_all_with_data(bundle.as_ref(), user.as_ref())
            .expect("reason_all_with_data should succeed");
        assert!(
            !merged.is_consistent(),
            "user data merged with the bundle TBox entails an inconsistency"
        );
        assert!(
            merged
                .provenance
                .contradiction_witnesses
                .iter()
                .any(|w| w.individual == X),
            "x must be a contradiction witness in the merged run: {:?}",
            merged.provenance.contradiction_witnesses
        );
    }

    // ── Program-carrying reason: the full-FOL formula layer actually evaluates ──

    use gmeow_logic_compile::ir::{Formula, LogicProgram, PreservationKind, Term};

    const KNOWS: &str = "http://gmeow.example/knows";
    const TRUSTS: &str = "http://gmeow.example/trusts";
    const ALICE: &str = "http://gmeow.example/alice";
    const BOB: &str = "http://gmeow.example/bob";
    const SAM: &str = "http://gmeow.example/sam";

    fn fml_atom(rel: &str, args: Vec<Term>) -> Formula {
        Formula::atom(Term::iri(rel.to_owned()).unwrap(), args).unwrap()
    }

    #[test]
    fn reason_program_evaluates_a_horn_formula_end_to_end() {
        // ∀x. (knows(x, alice) → trusts(x, bob)) is Horn-expressible, so it must lower to a
        // rule that the chase fires: given knows(sam, alice), the program must DERIVE
        // trusts(sam, bob). This is the formula layer evaluating end-to-end (not dead code).
        let formula = Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::Implies(
                Box::new(fml_atom(
                    KNOWS,
                    vec![
                        Term::var("x").unwrap(),
                        Term::iri(ALICE.to_owned()).unwrap(),
                    ],
                )),
                Box::new(fml_atom(
                    TRUSTS,
                    vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]);
        let edb = dataset(vec![quad(SAM, KNOWS, ALICE)]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // Objects decode to their N3 surface (`<iri>`); subjects/predicates are bare IRIs.
        let bob_obj = format!("<{BOB}>");
        assert!(
            result
                .inferred()
                .iter()
                .any(|ax| { ax.subject == SAM && ax.predicate == TRUSTS && ax.object == bob_obj }),
            "the Horn formula must derive trusts(sam, bob); closure: {:?}",
            result
                .inferred()
                .iter()
                .map(|a| (&a.subject, &a.predicate, &a.object))
                .collect::<Vec<_>>()
        );
        // The Horn formula lowers exactly — it adds no formula residue to the claim.
        assert!(
            !result
                .preservation
                .unsupported_constructs
                .iter()
                .any(|c| c.contains("formula") || c.contains("disjunct")),
            "a fully-evaluable Horn formula adds no formula residue: {:?}",
            result.preservation.unsupported_constructs
        );
    }

    #[test]
    fn reason_program_discloses_non_horn_formula_residue() {
        // ∀x. (knows(x, alice) → (trusts(x, bob) ∨ trusts(x, sam))) has a disjunctive head:
        // it does NOT lower to a rule, so it must be disclosed as residue in the result's
        // preservation claim — flagged, never silently evaluated as one disjunct.
        let formula = Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::Implies(
                Box::new(fml_atom(
                    KNOWS,
                    vec![
                        Term::var("x").unwrap(),
                        Term::iri(ALICE.to_owned()).unwrap(),
                    ],
                )),
                Box::new(Formula::Or(vec![
                    fml_atom(
                        TRUSTS,
                        vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
                    ),
                    fml_atom(
                        TRUSTS,
                        vec![Term::var("x").unwrap(), Term::iri(SAM.to_owned()).unwrap()],
                    ),
                ])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]);
        let edb = dataset(vec![quad(SAM, KNOWS, ALICE)]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // Eval-path honesty: the disjunctive formula is disclosed (SoundUnder), and it does
        // NOT silently materialize either disjunct.
        assert!(
            result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "a non-evaluable formula must drop the claim to SoundUnder: {:?}",
            result.preservation.polarities
        );
        assert!(
            !result.preservation.unsupported_constructs.is_empty(),
            "the disjunctive residue must be disclosed, not silently absent"
        );
        assert!(
            !result.inferred().iter().any(|ax| ax.predicate == TRUSTS),
            "neither disjunct may be silently materialized: {:?}",
            result
                .inferred()
                .iter()
                .map(|a| (&a.subject, &a.predicate, &a.object))
                .collect::<Vec<_>>()
        );
    }
}
