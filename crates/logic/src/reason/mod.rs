// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, ontology-independent OWL-2 reasoning over the Nemo chase.
//!
//! This module hosts fixed, built-in entailment rule sets — an intrinsic
//! entailment calculus — that run over an arbitrary TBox/ABox through the world-scoped
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

pub use dl::{DlVerdict, InconsistencyWitness, UnsatClass, dl_consistency};
pub use el::{ElClosure, InferredAxiom, el_closure};
pub use ledger::{
    DivergenceKind, DivergenceLedger, ExternalComparison, LedgerRow, LedgerVerdict, build_ledger,
    compare_external_corpus, compare_subsumption, divergence_findings, dl_gap_rows, enforce,
};
pub use rl::{RlClosure, RlTriple, rl_closure};

use crate::facts::TypedFactSet;
use crate::nemo_engine::TypedRow;
use crate::oracle::{ForwardBudget, ForwardOracle, forward_oracle};
use crate::result::{ReasoningResult, ResultProvenance};
use crate::store::WorldStore;
use purrdf::{RdfDataset, RdfDatasetBuilder, TermValue};

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
/// cached results produced under the old contract. Public so a consumer holding a
/// shipped `graph/reasoning` verdict can refuse one minted under a different
/// contract than the engine it is about to trust it against.
pub fn native_contract_hash() -> String {
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
/// [`ReasoningResult`] (ME2) — the single shared result model every
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

    // 3b. n-ary HEAD-derivation rules (`Rel(a₀..aₙ)` in a rule head) invent a shared reifier
    //     null per firing — a value-inventing existential the Nemo PROVENANCE chase in
    //     `run_reasoning` cannot trace ("no trace tree"). They are evaluated SEPARATELY through
    //     the native restricted chase, which mints the reified tuple by content identity, and
    //     the derived reified triples are folded into the same closure.
    let nary_head_rls = crate::relational_core::formula_nary_head_rls(program);
    if !nary_head_rls.trim().is_empty() {
        inferred.extend(run_nary_head_chase(&nary_head_rls, edb)?);
    }

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

/// Evaluate the n-ary HEAD-derivation `.rls` (conjunctive-head existential rules) through the
/// native restricted chase over `edb`, returning the DERIVED reified tuples as
/// [`InferredAxiom`]s to fold into the reasoning closure.
///
/// The native chase is used because these rules invent a reifier null the Nemo provenance
/// trace cannot follow; it mints each reified tuple by content identity
/// ([`crate::provenance::mint_nary_reifier`]). Only the chase-DERIVED rows are returned — the
/// asserted-EDB echo the chase also produces is already present in the Nemo closure, so it is
/// dropped here to avoid duplication.
///
/// # Errors
///
/// Returns `Err` if the `.rls` cannot be parsed as existential rules, if the store fails to
/// load `edb`, or if the chase declines the program (an uncertified, non-terminating
/// existential set) — a first-class declared gap, never a silent drop.
fn run_nary_head_chase(
    nary_head_rls: &str,
    edb: &RdfDataset,
) -> Result<Vec<InferredAxiom>, String> {
    let rules = crate::physical::parse_existential_rules(nary_head_rls)?;
    let store = WorldStore::new();
    store.load_dataset(edb)?;
    let (_admission, outcome) = crate::physical::chase_materialize(&store, &rules, None)?;
    let budgeted = match outcome {
        crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
        crate::physical::NativeOutcome::Unsupported(kind) => {
            return Err(format!(
                "n-ary head derivation: the native restricted chase declined the program \
                 ({kind:?}) — an uncertified (non-terminating) existential rule set the reasoner \
                 cannot materialize"
            ));
        }
    };

    let mut out: Vec<InferredAxiom> = Vec::new();
    for row in budgeted.rows {
        // Drop the asserted-EDB echo (rule_iri == logic:assert); it is already in the closure
        // from the Nemo run. Keep only the chase-derived reified tuples.
        if row.rule_iri == crate::provenance::ASSERT_RULE_IRI {
            continue;
        }
        out.push(InferredAxiom {
            subject: subject_iri(&row.subject)?,
            predicate: row.predicate,
            object: crate::provenance::term_display(&row.object),
            world: row.graph,
            is_edb: false,
            rule_name: Some(row.rule_iri),
            premises: Vec::new(),
        });
    }
    Ok(out)
}

/// Reason over a user-supplied data graph MERGED with the bundle's axioms, returning
/// the same shared typed [`ReasoningResult`] as [`reason_all`].
///
/// The merge is the cross-dataset re-intern
/// ([`RdfDatasetBuilder::push_dataset`](purrdf::RdfDatasetBuilder::push_dataset)),
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

/// The bare IRI string of a typed subject term.
///
/// A world-scoped reasoning fact never carries a literal (or triple-term)
/// subject — blanks were Skolemized to IRIs before the chase — so any other
/// shape is a hard error.
fn subject_iri(term: &TermValue) -> Result<String, String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(format!(
            "reasoning row subject must be an IRI (or Skolem IRI) term, got {other:?}"
        )),
    }
}

/// The raw world string of a typed world term.
///
/// The world position of a ternary reasoning fact is always a plain string
/// literal (the Nemo string-constant treatment); any other shape is a hard error.
fn world_string(term: &TermValue) -> Result<String, String> {
    match term {
        TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            ..
        } if datatype == "http://www.w3.org/2001/XMLSchema#string" => Ok(lexical_form.clone()),
        other => Err(format!(
            "reasoning row world must be a plain string literal, got {other:?}"
        )),
    }
}

/// Decode one typed antecedent row into a `(subject, predicate, object)` triple.
///
/// The antecedent rows are the same ternary shape as derived rows: subject is
/// an IRI term, object is any typed term (surfaced as its display string), and
/// the third value is the world string constant (dropped here — premises carry
/// only the triple shape).
fn decode_premise(row: &TypedRow) -> Result<(String, String, String), String> {
    if row.args.len() != 3 {
        return Err(format!(
            "antecedent row has arity {} (expected 3): {row:?}",
            row.args.len()
        ));
    }
    let subject = subject_iri(&row.args[0])?;
    let object = crate::provenance::term_display(&row.args[1]);
    Ok((subject, row.predicate.clone(), object))
}

/// Run a fixed entailment rule set over `edb` through the Nemo chase.
///
/// Loads `edb` into a fresh [`WorldStore`], pushes every IRI-object quad of
/// every world into a typed EDB ([`TypedFactSet`]), runs the typed chase (the
/// Nemo adapter is the sole fact stringifier), and coerces every ternary typed
/// row into an [`InferredAxiom`] carrying its raw provenance (EDB/IDB flag,
/// firing rule name, immediate premises).
///
/// This is the shared chase machinery both [`el::el_closure`] and
/// [`dl::dl_consistency`] build on: the rule set is the only difference.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate/decode, or if a materialized row is
/// not the ternary reasoning shape.
pub(crate) fn run_reasoning(edb: &RdfDataset, rules: &str) -> Result<Vec<InferredAxiom>, String> {
    // 1. Load the source into a fresh world-indexed store.
    let store = WorldStore::new();
    store.load_dataset(edb)?;

    // 2. Push every IRI-object quad of every world into the typed EDB.
    //    The IRI-object filter is a SEMANTIC EL/DL restriction: the fixed
    //    calculi only fire on axioms whose object is an IRI (subClassOf, type,
    //    disjointWith, equivalentClass, subPropertyOf), so a literal-object
    //    quad (an annotation such as rdfs:comment / dc:creator) can never
    //    participate in any rule, and skipping them is sound for the closure
    //    AND the verdict. It is no longer a transport necessity: the typed
    //    adapter carries literal objects — control characters included —
    //    losslessly through the chase.
    let mut edb_facts = TypedFactSet::new();
    for world in store.worlds() {
        for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
            if !quad.o.is_iri() {
                continue;
            }
            // The predicate is always an IRI (RDF invariant); a non-IRI predicate
            // cannot be a relation name, so skip it defensively.
            let Some(predicate) = quad.p.as_iri() else {
                continue;
            };
            // Blank subjects/objects are Skolemized inside `push_quad`; the
            // world travels as a plain string literal.
            edb_facts.push_quad(&quad.s, predicate, &quad.o, &world);
        }
    }

    // 3. Run the typed chase through the forward oracle (the adapter renders the
    //    fact lines internally).
    let chase = forward_oracle().materialize(&edb_facts, rules, &ForwardBudget::UNBOUNDED)?;

    // 4. Coerce each ternary typed row into an InferredAxiom.
    let mut inferred: Vec<InferredAxiom> = Vec::new();
    for (row, prov) in &chase.rows {
        // Every reasoning fact is the ternary `predicate(subject, object, world)`.
        // The rule texts this chase runs — EL_RULES, dl_rules(), and the ternary
        // projections reason_program appends — are repo-owned and declare ONLY
        // ternary relations, so a non-ternary row indicates a rule-text bug and
        // is a hard error. (This differs from materialize's explicit non-quad
        // bucket: there the rule text is caller-supplied and may legitimately
        // declare helper predicates of other arities.)
        if row.args.len() != 3 {
            return Err(format!(
                "reasoning chase produced a non-ternary row for predicate \
                 {:?} (arity {}): the fixed reasoning rule texts declare only \
                 ternary relations, so this is a rule-text bug",
                row.predicate,
                row.args.len()
            ));
        }

        let predicate = row.predicate.clone();
        let subject = subject_iri(&row.args[0])?;
        let object = crate::provenance::term_display(&row.args[1]);
        let world = world_string(&row.args[2])?;

        let mut premises = prov
            .antecedents
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
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

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

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
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

    /// A law with TERNARY atoms in its BODY and a BINARY head (the associativity shape,
    /// like the algebra-axioms law) evaluates end-to-end: the reified n-ary body atoms
    /// join through the chase and the binary consequent is derived. This exercises the
    /// body-reification path (no head derivation) all the way through `reason_program`.
    #[test]
    fn reason_program_evaluates_an_nary_body_law_end_to_end() {
        // ∀a b c ab bc l r. op(a,b,ab) ∧ op(ab,c,l) ∧ op(b,c,bc) ∧ op(a,bc,r) → eq(l,r)
        // Seeded on a concrete associative table so both bracketings reach the SAME value v;
        // then eq(l,r) must be derived (l = v = r).
        const OP: &str = "http://gmeow.example/op";
        const EQ: &str = "http://gmeow.example/eq";
        let v = |n: &str| Term::var(n).unwrap();
        let law = Formula::Forall {
            vars: ["a", "b", "c", "ab", "bc", "l", "r"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(OP, vec![v("a"), v("b"), v("ab")]),
                    fml_atom(OP, vec![v("ab"), v("c"), v("l")]),
                    fml_atom(OP, vec![v("b"), v("c"), v("bc")]),
                    fml_atom(OP, vec![v("a"), v("bc"), v("r")]),
                ])),
                Box::new(fml_atom(EQ, vec![v("l"), v("r")])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // A concrete op table where (a·b)·c and a·(b·c) both reach `v` for a=x,b=y,c=z.
        // op is ternary → the EDB op facts are authored PRE-REIFIED (instanceOf + naryArg).
        const X: &str = "http://gmeow.example/x";
        const Y: &str = "http://gmeow.example/y";
        const Z: &str = "http://gmeow.example/z";
        const XY: &str = "http://gmeow.example/xy";
        const YZ: &str = "http://gmeow.example/yz";
        const V: &str = "http://gmeow.example/v";
        let io = "https://blackcatinformatics.ca/logic/instanceOf";
        let a0 = "https://blackcatinformatics.ca/logic/naryArg0";
        let a1 = "https://blackcatinformatics.ca/logic/naryArg1";
        let a2 = "https://blackcatinformatics.ca/logic/naryArg2";
        // Reify one op(s,t,u) tuple as instanceOf + naryArg triples on a fresh node.
        let mut quads = Vec::new();
        let mut reify = |node: &str, s: &str, t: &str, u: &str| {
            quads.push(quad(node, io, OP));
            quads.push(quad(node, a0, s));
            quads.push(quad(node, a1, t));
            quads.push(quad(node, a2, u));
        };
        reify("http://gmeow.example/r_xy", X, Y, XY); // x·y = xy
        reify("http://gmeow.example/r_xyz1", XY, Z, V); // (x·y)·z = v
        reify("http://gmeow.example/r_yz", Y, Z, YZ); // y·z = yz
        reify("http://gmeow.example/r_xyz2", X, YZ, V); // x·(y·z) = v
        let edb = dataset(quads);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // The binary consequent eq(l, r) = eq(v, v) must be derived.
        let eq_vv = result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == EQ && ax.subject == V && ax.object == format!("<{V}>"));
        assert!(
            eq_vv,
            "associativity must derive eq(v, v); closure: {:?}",
            result
                .inferred()
                .iter()
                .filter(|a| a.predicate == EQ)
                .map(|a| (&a.subject, &a.object))
                .collect::<Vec<_>>()
        );
        // A fully-evaluable n-ary body law lowers exactly (no residue).
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "an n-ary body law lowers exactly: {:?}",
            result.preservation
        );
    }

    // ── n-ary HEAD derivation: the det homomorphism law evaluates end-to-end ──

    const MATMUL: &str = "http://gmeow.example/matMul";
    const MUL: &str = "http://gmeow.example/mul";
    const DET: &str = "http://gmeow.example/det";
    const MAT_A: &str = "http://gmeow.example/A";
    const MAT_B: &str = "http://gmeow.example/B";
    const MAT_AB: &str = "http://gmeow.example/AB";
    const DET_A: &str = "http://gmeow.example/dA";
    const DET_B: &str = "http://gmeow.example/dB";
    const DET_AB: &str = "http://gmeow.example/dAB";
    const MATMUL_REIFIER: &str = "http://gmeow.example/reif/matMul-A-B-AB";
    const LOGIC_INSTANCE_OF: &str = "https://blackcatinformatics.ca/logic/instanceOf";
    const NARY_REIFIER_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/reifier/nary/";

    fn logic_nary_arg(i: usize) -> String {
        format!("https://blackcatinformatics.ca/logic/naryArg{i}")
    }

    #[test]
    fn reason_program_derives_an_nary_head_tuple_end_to_end() {
        // The determinant homomorphism law:
        //   ∀A,B,AB,dA,dB,dAB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) ∧ det(AB,dAB) → mul(dA,dB,dAB)
        // `matMul` is ternary → reified BODY atom; `mul` is ternary → reified HEAD (a derived
        // tuple). Seed a minimal deterministic pre-reified EDB (the matMul tuple as reified
        // instanceOf+naryArg triples, plus the three det facts) and assert the closure DERIVES
        // the reified `mul(dA,dB,dAB)` tuple.
        let law = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(
                        MATMUL,
                        vec![
                            Term::var("A").unwrap(),
                            Term::var("B").unwrap(),
                            Term::var("AB").unwrap(),
                        ],
                    ),
                    fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                    fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                    fml_atom(
                        DET,
                        vec![Term::var("AB").unwrap(), Term::var("dAB").unwrap()],
                    ),
                ])),
                Box::new(fml_atom(
                    MUL,
                    vec![
                        Term::var("dA").unwrap(),
                        Term::var("dB").unwrap(),
                        Term::var("dAB").unwrap(),
                    ],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // Pre-reified EDB: matMul(A,B,AB) as instanceOf + naryArg triples, plus the det facts.
        let na = logic_nary_arg(0);
        let nb = logic_nary_arg(1);
        let nab = logic_nary_arg(2);
        let edb = dataset(vec![
            quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
            quad(MATMUL_REIFIER, &na, MAT_A),
            quad(MATMUL_REIFIER, &nb, MAT_B),
            quad(MATMUL_REIFIER, &nab, MAT_AB),
            quad(MAT_A, DET, DET_A),
            quad(MAT_B, DET, DET_B),
            quad(MAT_AB, DET, DET_AB),
        ]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // Find the derived reifier R by the typing atom instanceOf(R, mul).
        let mul_obj = format!("<{MUL}>");
        let r = result
            .inferred()
            .iter()
            .find(|ax| ax.predicate == LOGIC_INSTANCE_OF && ax.object == mul_obj)
            .map(|ax| ax.subject.clone())
            .unwrap_or_else(|| {
                panic!(
                    "the law must DERIVE instanceOf(R, mul); closure: {:?}",
                    result
                        .inferred()
                        .iter()
                        .map(|a| (&a.subject, &a.predicate, &a.object))
                        .collect::<Vec<_>>()
                )
            });

        // The reifier is minted by TUPLE IDENTITY (mint_nary_reifier), not a frontier Skolem.
        assert!(
            r.starts_with(NARY_REIFIER_PREFIX),
            "R must be the content-addressed n-ary reifier IRI, got: {r}"
        );

        // Join on R: the three positional argument atoms carry the concrete det values.
        let has_arg = |i: usize, value: &str| {
            let pred = logic_nary_arg(i);
            let obj = format!("<{value}>");
            result
                .inferred()
                .iter()
                .any(|ax| ax.subject == r && ax.predicate == pred && ax.object == obj)
        };
        assert!(has_arg(0, DET_A), "naryArg0(R, dA) must be derived");
        assert!(has_arg(1, DET_B), "naryArg1(R, dB) must be derived");
        assert!(has_arg(2, DET_AB), "naryArg2(R, dAB) must be derived");

        // The law lowers exactly — no formula residue, preservation stays Exact.
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "a range-restricted n-ary head lowers exactly (no SoundUnder): {:?}",
            result.preservation
        );
        assert!(
            !result
                .preservation
                .unsupported_constructs
                .iter()
                .any(|c| c.contains("formula") || c.contains("not bound") || c.contains("nary")),
            "no n-ary head residue disclosed: {:?}",
            result.preservation.unsupported_constructs
        );
    }

    #[test]
    fn reason_program_discloses_nary_head_unbound_arg_residue() {
        // A head variable the body does not bind is a non-range-restricted existential: the law
        // is carried as residue (SoundUnder) and derives NOTHING, never an unsafe tuple.
        let law = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(
                // Body binds dA, dB but NOT dAB.
                Box::new(Formula::And(vec![
                    fml_atom(
                        MATMUL,
                        vec![
                            Term::var("A").unwrap(),
                            Term::var("B").unwrap(),
                            Term::var("AB").unwrap(),
                        ],
                    ),
                    fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                    fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                ])),
                Box::new(fml_atom(
                    MUL,
                    vec![
                        Term::var("dA").unwrap(),
                        Term::var("dB").unwrap(),
                        Term::var("dAB").unwrap(),
                    ],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);
        let na = logic_nary_arg(0);
        let nb = logic_nary_arg(1);
        let nab = logic_nary_arg(2);
        let edb = dataset(vec![
            quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
            quad(MATMUL_REIFIER, &na, MAT_A),
            quad(MATMUL_REIFIER, &nb, MAT_B),
            quad(MATMUL_REIFIER, &nab, MAT_AB),
            quad(MAT_A, DET, DET_A),
            quad(MAT_B, DET, DET_B),
        ]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        assert!(
            result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "an unsafe (non-range-restricted) head must drop the claim to SoundUnder: {:?}",
            result.preservation.polarities
        );
        assert!(
            result
                .preservation
                .unsupported_constructs
                .iter()
                .any(|c| c.contains("not bound by the body")),
            "the range-restriction residue must be disclosed: {:?}",
            result.preservation.unsupported_constructs
        );
        // Nothing of the mul tuple is materialized.
        let mul_obj = format!("<{MUL}>");
        assert!(
            !result
                .inferred()
                .iter()
                .any(|ax| ax.predicate == LOGIC_INSTANCE_OF && ax.object == mul_obj),
            "an unsafe head derives no tuple: {:?}",
            result
                .inferred()
                .iter()
                .map(|a| (&a.subject, &a.predicate, &a.object))
                .collect::<Vec<_>>()
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
