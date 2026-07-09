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
    compare_external_corpus, compare_subsumption, divergence_diag_ledger, divergence_findings,
    dl_gap_rows, enforce,
};
pub use rl::{RlClosure, RlTriple, rl_closure};

use crate::facts::TypedFactSet;
use crate::nemo_engine::TypedRow;
use crate::oracle::{ForwardBudget, ForwardOracle, forward_oracle};
use crate::result::{ReasoningResult, ResultProvenance};
use crate::store::WorldStore;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTriple, TermValue};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

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
/// Returns `Err` if the source store cannot be loaded, if the Nemo chase
/// fails to parse/validate/evaluate/decode, or if coverage/consistency scanning
/// fails.
pub(crate) fn reason_closure(
    edb: &RdfDataset,
) -> gmeow_errors::Result<(Vec<InferredAxiom>, dl::DlVerdict)> {
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
/// Returns `Err` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate/decode, or if coverage/consistency
/// scanning fails.
pub fn reason_all(edb: &RdfDataset) -> gmeow_errors::Result<ReasoningResult> {
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
/// Returns `Err` if the Nemo projection of the program rules fails (e.g. a head
/// variable unbound by any body atom), or if the chase fails to parse/validate/evaluate/decode.
pub fn reason_program(
    program: &gmeow_logic_compile::ir::LogicProgram,
    edb: &RdfDataset,
) -> gmeow_errors::Result<ReasoningResult> {
    use gmeow_logic_compile::projections::text::{extract_nemo_rules_section, project_nemo};

    // 1. Lower the full-FOL formulas through the relational-core waist.
    let (formula_rls, formula_preservation) = crate::relational_core::formula_eval_rls(program);

    // 2. The program's own Horn rules, via the canonical Nemo projection (rules section only;
    //    facts come from `edb`).
    // The reasoning surface consumes only the `.rls` rule text, not the loss ledger, so the
    // nemo projection's drops are interned into a throwaway store.
    let program_nemo = project_nemo(
        program,
        &mut gmeow_logic_compile::loss_ledger::LossLedger::new(),
    )?;
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

/// Reason over `program` against `edb` and project the resulting closure (asserted +
/// derived axioms) back into a frozen [`RdfDataset`] a SPARQL consumer can query.
///
/// This is the closure→RDF bridge the native competency-question lane (`crates/slicetest`)
/// runs over: it evaluates [`reason_program`] and re-materializes every [`InferredAxiom`] as
/// a quad in its world graph, so a query sees the FULL entailment closure (the reified n-ary
/// tuples included), not just the asserted data. Both asserted (`is_edb`) and derived axioms
/// are emitted so a query over the closure sees the complete graph.
///
/// The per-axiom `subject`/`predicate` are IRIs; the `object` is the `term_display` surface
/// (`<iri>`, `_:blank`, or a literal) re-parsed via [`crate::rule_ir::surface_to_value`]. The
/// `world` string becomes the quad's graph name when it is an absolute IRI (a bodyless-rule
/// `"default"` world lands in the default graph).
///
/// # Errors
///
/// Returns `Err` if [`reason_program`] fails, if an object surface cannot be
/// re-parsed, or if the projected dataset fails the freeze-time structural contract.
/// Whether `value` is an absolute IRI (carries a `scheme:` prefix per RFC 3986). Used to
/// decide whether a reasoned axiom's non-default `world` is a genuine named graph. A robust
/// scheme check — NOT `contains("://")`, which silently misses schemeless-authority worlds
/// (`urn:`, `did:`, `tag:`, `mailto:`) and would demote them to the default graph (a
/// world-scoping / information-loss defect).
fn is_absolute_iri(value: &str) -> bool {
    match value.find(':') {
        Some(0) => false,
        Some(idx) => {
            let scheme = &value[..idx];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    }
}

pub fn reason_program_closure_dataset(
    program: &gmeow_logic_compile::ir::LogicProgram,
    edb: &RdfDataset,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let result = reason_program(program, edb)?;
    let mut builder = RdfDatasetBuilder::new();
    for ax in result.inferred() {
        let subject = RdfTerm::iri(ax.subject.clone());
        let object = term_value_to_rdf_term(&crate::rule_ir::surface_to_value(&ax.object)?)?;
        let mut quad = RdfQuad::new(subject, ax.predicate.clone(), object);
        // The world travels as a plain string. The reasoner's default-world sentinel
        // ([`rl::DEFAULT_WORLD`], where un-named / default-graph EDB is reasoned) projects
        // back to the RDF DEFAULT graph, so a graph-clause-free competency query over the
        // closure sees it. A genuinely NAMED world (an absolute-IRI graph other than the
        // sentinel) is preserved as a named graph.
        if ax.world != rl::DEFAULT_WORLD && is_absolute_iri(&ax.world) {
            quad = quad.in_graph(RdfTerm::iri(ax.world.clone()));
        }
        builder.push_owned_quad(&quad);
    }
    builder.freeze().map_err(|e| reason_err(e.to_string()))
}

/// Re-materialize a native [`TermValue`] (as produced by
/// [`crate::rule_ir::surface_to_value`]) into an owned [`RdfTerm`].
///
/// `surface_to_value` only ever yields an IRI, a blank node, or a literal, so a triple
/// term is a hard error rather than a silent drop (the no-optionality discipline).
fn term_value_to_rdf_term(value: &TermValue) -> gmeow_errors::Result<RdfTerm> {
    Ok(match value {
        TermValue::Iri(iri) => RdfTerm::iri(iri.clone()),
        TermValue::Blank { label, .. } => RdfTerm::blank_node(label.clone()),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let literal = match language {
                Some(lang) => RdfLiteral::language_tagged(lexical_form.clone(), lang.clone()),
                None => RdfLiteral::typed(lexical_form.clone(), datatype.clone()),
            };
            RdfTerm::literal(literal)
        }
        TermValue::Triple { s, p, o } => {
            let predicate = match p.as_ref() {
                TermValue::Iri(iri) => iri.clone(),
                other => {
                    return Err(reason_err(format!(
                        "closure→RDF: triple-term predicate must be an IRI, got {other:?}"
                    )));
                }
            };
            RdfTerm::triple(RdfTriple::new(
                term_value_to_rdf_term(s)?,
                predicate,
                term_value_to_rdf_term(o)?,
            ))
        }
    })
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
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    let rules = crate::physical::parse_existential_rules(nary_head_rls)?;
    let store = WorldStore::new();
    store.load_dataset(edb)?;
    let (_admission, outcome) = crate::physical::chase_materialize(&store, &rules, None)?;
    let budgeted = match outcome {
        crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
        crate::physical::NativeOutcome::Unsupported(kind) => {
            return Err(reason_err(format!(
                "n-ary head derivation: the native restricted chase declined the program \
                 ({kind:?}) — an uncertified (non-terminating) existential rule set the reasoner \
                 cannot materialize"
            )));
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
/// Returns `Err` if the merged dataset fails the freeze-time structural
/// contract, or if the chase fails to parse/validate/evaluate/decode.
pub fn reason_all_with_data(
    bundle: &RdfDataset,
    user: &RdfDataset,
) -> gmeow_errors::Result<ReasoningResult> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(bundle);
    builder.push_dataset(user);
    let merged = builder.freeze().map_err(|e| reason_err(e.to_string()))?;
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
fn subject_iri(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(reason_err(format!(
            "reasoning row subject must be an IRI (or Skolem IRI) term, got {other:?}"
        ))),
    }
}

/// The raw world string of a typed world term.
///
/// The world position of a ternary reasoning fact is always a plain string
/// literal (the Nemo string-constant treatment); any other shape is a hard error.
fn world_string(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            ..
        } if datatype == "http://www.w3.org/2001/XMLSchema#string" => Ok(lexical_form.clone()),
        other => Err(reason_err(format!(
            "reasoning row world must be a plain string literal, got {other:?}"
        ))),
    }
}

/// Decode one typed antecedent row into a `(subject, predicate, object)` triple.
///
/// The antecedent rows are the same ternary shape as derived rows: subject is
/// an IRI term, object is any typed term (surfaced as its display string), and
/// the third value is the world string constant (dropped here — premises carry
/// only the triple shape).
fn decode_premise(row: &TypedRow) -> gmeow_errors::Result<(String, String, String)> {
    if row.args.len() != 3 {
        return Err(reason_err(format!(
            "antecedent row has arity {} (expected 3): {row:?}",
            row.args.len()
        )));
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
/// Returns `Err` if the source store cannot be loaded, if the Nemo
/// chase fails to parse/validate/evaluate/decode, or if a materialized row is
/// not the ternary reasoning shape.
pub(crate) fn run_reasoning(
    edb: &RdfDataset,
    rules: &str,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    // The primary reasoning path funnels through the single naming site
    // `forward_oracle()` (the native core after the Task-6 flip).
    let oracle = forward_oracle();
    run_reasoning_with(edb, rules, &oracle)
}

/// Run the shared chase machinery over `rules` using a SPECIFIC [`ForwardOracle`],
/// returning the coerced [`InferredAxiom`] closure.
///
/// [`run_reasoning`] delegates here with the production `forward_oracle()`. The
/// native↔Nemo differential cross-check ([`crosscheck_native_vs_nemo`]) is the
/// only other caller: it drives the SAME corpus and rule text through BOTH the
/// native oracle and the retained Nemo oracle so the two closures can be compared
/// row-for-row. Factoring the oracle out keeps the EDB construction and the
/// [`chase_rows_to_inferred`] coercion byte-identical across engines, so any
/// residual difference in the compared closures is a genuine engine divergence,
/// never a harness artifact.
///
/// # Errors
///
/// Returns `Err` if the source store cannot be loaded, if the chase fails
/// to parse/validate/evaluate/decode, or if a materialized row is not the ternary
/// reasoning shape.
pub(crate) fn run_reasoning_with(
    edb: &RdfDataset,
    rules: &str,
    oracle: &dyn ForwardOracle,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
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

    // 3. Run the typed chase through the CHOSEN forward oracle (the adapter renders
    //    the fact lines internally).
    let chase = oracle.materialize(&edb_facts, rules, &ForwardBudget::UNBOUNDED)?;

    // 4. Coerce every ternary typed row into an InferredAxiom.
    chase_rows_to_inferred(&chase)
}

/// `rdfs:subClassOf` — the subsumption predicate the native↔Nemo differential compares.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// Extract the `(subclass, superclass, world)` subsumption tuples from a coerced
/// closure — every `rdfs:subClassOf` axiom (asserted echo + derived), sorted and
/// deduplicated.
///
/// Both sides of the native↔Nemo differential run through the identical
/// [`chase_rows_to_inferred`] coercion, so the subject/object/world encodings line
/// up exactly and [`ledger::compare_subsumption`] classifies any residual
/// difference as a genuine engine divergence.
fn subsumption_tuples(inferred: &[InferredAxiom]) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = inferred
        .iter()
        .filter(|ax| ax.predicate == RDFS_SUBCLASS_OF)
        .map(|ax| (ax.subject.clone(), ax.object.clone(), ax.world.clone()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Run the native ↔ Nemo differential subsumption cross-check over `edb`.
///
/// This is the scheduled cross-check lane's engine: it dual-runs the SAME committed
/// reasoning corpus under the fixed DL calculus ([`dl::dl_rules`]) through BOTH the
/// production native oracle ([`crate::oracle::forward_oracle`]) and the retained
/// Nemo bootstrap oracle ([`crate::oracle::nemo_forward_oracle`]) — the last
/// remaining Nemo entry point on any non-test path — then folds the two subsumption
/// closures into a [`DivergenceLedger`] via [`ledger::compare_subsumption`].
///
/// The native closure is the `native` side and the Nemo closure the `oracle` side,
/// so a `NemoOnly` subsumption surfaces as an `OracleOnly` row that
/// [`ledger::enforce`] FAILS on (a native coverage regression: Nemo derived a
/// subsumption the production native path did not). A native-only subsumption is
/// expected superset richness and passes. The whole fixed DL calculus is gap-zero
/// on the committed bundle, so a healthy run is pure agreement.
///
/// Both engines see byte-identical EDB and rule text and share the
/// [`chase_rows_to_inferred`] coercion, so the ledger isolates the engine
/// difference and nothing else. Nemo runs UNBUDGETED (`ForwardBudget::UNBOUNDED`),
/// exactly as it did when it was the production chase, so this is a faithful
/// dual-run, never a downgraded approximation.
///
/// # Errors
///
/// Returns `Err` if either engine's chase fails to
/// parse/validate/evaluate/decode over the committed corpus.
pub fn crosscheck_native_vs_nemo(edb: &RdfDataset) -> gmeow_errors::Result<DivergenceLedger> {
    let rules = dl::dl_rules();

    // Native: the production forward oracle (the single-naming-site native core).
    let native_oracle = forward_oracle();
    let native = run_reasoning_with(edb, &rules, &native_oracle)?;

    // Nemo: the retained bootstrap oracle, reached ONLY through its dedicated
    // off-primary-path provider — the last production consumer keeping Nemo alive.
    let nemo_oracle = crate::oracle::nemo_forward_oracle();
    let nemo = run_reasoning_with(edb, &rules, &nemo_oracle)?;

    let native_subs = subsumption_tuples(&native);
    let nemo_subs = subsumption_tuples(&nemo);
    let rows = ledger::compare_subsumption(&native_subs, &nemo_subs);
    Ok(ledger::build_ledger(
        rows,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

/// Coerce a typed chase result into the `Vec<InferredAxiom>` closure the DL/EL
/// post-passes and result folds consume.
///
/// Every reasoning fact is the ternary `predicate(subject, object, world)`. The
/// rule texts the reasoning chase runs — `EL_RULES`, `dl_rules()`, and the
/// ternary projections `reason_program` appends — are repo-owned and declare
/// ONLY ternary relations, so a non-ternary row indicates a rule-text bug and is
/// a hard error. (This differs from `materialize`'s explicit non-quad bucket:
/// there the rule text is caller-supplied and may legitimately declare helper
/// predicates of other arities.)
///
/// Factored out of [`run_reasoning`] so the coercion is INDEPENDENT of which
/// [`ForwardOracle`] produced the closure: a caller holding a native-produced
/// AND a Nemo-produced [`crate::oracle::TypedChaseResult`] of the same program
/// can coerce BOTH identically and feed the resulting closures through the
/// provenance-blind DL post-pass, demonstrating engine-invariance of the
/// downstream verdict.
///
/// # Errors
///
/// Returns `Err` if a materialized row is not the ternary reasoning
/// shape or if a subject/world/premise term cannot be decoded.
pub(crate) fn chase_rows_to_inferred(
    chase: &crate::oracle::TypedChaseResult,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    let mut inferred: Vec<InferredAxiom> = Vec::new();
    for (row, prov) in &chase.rows {
        if row.args.len() != 3 {
            return Err(reason_err(format!(
                "reasoning chase produced a non-ternary row for predicate \
                 {:?} (arity {}): the fixed reasoning rule texts declare only \
                 ternary relations, so this is a rule-text bug",
                row.predicate,
                row.args.len()
            )));
        }

        let predicate = row.predicate.clone();
        let subject = subject_iri(&row.args[0])?;
        let object = crate::provenance::term_display(&row.args[1]);
        let world = world_string(&row.args[2])?;

        let mut premises = prov
            .antecedents
            .iter()
            .map(decode_premise)
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
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
    fn is_absolute_iri_recognizes_schemeless_authority_worlds() {
        // http(s) worlds — the common case — stay named.
        assert!(is_absolute_iri(
            "https://blackcatinformatics.ca/gmeow/graph/w"
        ));
        assert!(is_absolute_iri("http://example.org/g"));
        // Schemeless-authority IRIs (no `://`) are ALSO absolute named worlds — the old
        // `contains("://")` check silently demoted these to the default graph.
        assert!(is_absolute_iri(
            "urn:uuid:2c8f0a1e-0000-4000-8000-000000000001"
        ));
        assert!(is_absolute_iri("did:example:123"));
        assert!(is_absolute_iri("tag:blackcat,2026:world"));
        assert!(is_absolute_iri("mailto:someone@example.org"));
        // A bare token / relative reference is NOT absolute.
        assert!(!is_absolute_iri("c14n44"));
        assert!(!is_absolute_iri("world-1"));
        assert!(!is_absolute_iri(":no-scheme"));
        assert!(!is_absolute_iri("1http://bad-scheme")); // scheme must start with a letter
    }

    /// The flip observable (adversary F5): after promoting the native core,
    /// `forward_oracle()` — the SOLE naming site the primary reasoning path funnels
    /// through (`run_reasoning` line ~476) — resolves to the native engine, and the
    /// retained Nemo oracle is reachable ONLY via its distinct off-path provider.
    ///
    /// This does not rest on the contract-hash bump: it verifies the ENGINE ON THE
    /// PATH structurally (the seam name), then drives `reason_all` end-to-end over
    /// the fixed DL calculus to prove the native path actually DECIDES the whole
    /// closure (a native OracleOnly/DlGap regression would surface as an `Err` or a
    /// missing entailment here, not a silent byte-diff).
    #[test]
    fn nemo_off_primary_reasoning_path() {
        // 1. Structural observable: the production forward seam is native.
        assert_eq!(
            crate::oracle::forward_oracle().name(),
            "native",
            "the primary reasoning path's forward oracle must be the native engine"
        );
        // 2. Nemo is retained but OFF the primary path — reachable only via its own
        //    distinct provider (the parity/cross-check seam), never `forward_oracle`.
        assert_eq!(
            crate::oracle::nemo_forward_oracle().name(),
            "nemo",
            "Nemo stays reachable only through its dedicated off-path provider"
        );
        assert_ne!(
            crate::oracle::forward_oracle().name(),
            crate::oracle::nemo_forward_oracle().name(),
            "the production engine and the bootstrap oracle must be distinct engines"
        );

        // 3. End-to-end: `reason_all` (→ `reason_closure` → `run_reasoning` →
        //    `forward_oracle().materialize`) decides the full fixed DL calculus on
        //    native. A ⊑ B, A ⊑ C, B ⊓ C ⊑ ⊥, x : A must be found inconsistent —
        //    exercising subsumption transitivity AND the disjointness clash the
        //    native chase now materializes end-to-end.
        let store = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let result = reason_all(store.as_ref()).expect("native reason_all must decide the closure");
        assert!(
            !result.is_consistent(),
            "native path must derive the disjointness inconsistency (no DlGap regression)"
        );
        assert!(
            !result.inferred().is_empty(),
            "native path must materialize a non-empty subsumption closure"
        );
    }

    /// Production-surface antecedent guard (gap G3): the primary reasoning path
    /// (`reason_all` → `reason_closure` → `run_reasoning` → `forward_oracle().materialize`
    /// → `chase_rows_to_inferred`) must carry REAL native premises end-to-end, not
    /// just non-empty inferred facts. `forward_oracle()` funnels the binary
    /// seminaive branch here; A⊑B, B⊑C derives the transitive A⊑C, whose
    /// `InferredAxiom::premises` must be NON-EMPTY (it cites its two body facts).
    /// Falsifiable: the escaped empty-antecedents bug leaves EVERY derived
    /// `premises` empty, tripping this at the production observable.
    #[test]
    fn reason_all_derived_axioms_carry_nonempty_premises() {
        let store = dataset(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
        let result = reason_all(store.as_ref()).expect("native reason_all must decide the closure");

        // The transitive subClassOf(A, C) is derived (is_edb == false) and must
        // cite its immediate antecedents through `InferredAxiom::premises`.
        // `subject`/`predicate` are bare IRIs; `object` is `term_display`ed (an IRI
        // renders angle-bracketed), so match the object against its display form.
        let object_c = format!("<{C}>");
        let derived_transitive = result.inferred().iter().find(|ax| {
            !ax.is_edb && ax.predicate == SUBCLASS && ax.subject == A && ax.object == object_c
        });
        let axiom = derived_transitive.unwrap_or_else(|| {
            panic!(
                "transitive subClassOf(A, C) must be derived; got {:?}",
                result.inferred()
            )
        });
        assert!(
            !axiom.premises.is_empty(),
            "derived subClassOf(A, C) must carry NON-EMPTY premises on the production path \
             (the empty-antecedents bug fails here); got {axiom:?}"
        );
    }

    /// The Task-7 scheduled cross-check engine: dual-running the SAME corpus through
    /// the native oracle and the retained Nemo oracle over the fixed DL calculus must
    /// agree — pure agreement, no Nemo-only (OracleOnly) regression, and the verdict
    /// passes. Non-vacuity is pinned too: the transitive `A ⊑ C` plus the two asserted
    /// echoes give `agree > 0`, so the lane's `passed && agree > 0` gate is real.
    #[test]
    fn materialize_crosscheck_native_vs_nemo_agrees_on_subclass_chain() {
        // A ⊑ B ⊑ C — both engines derive the transitive A ⊑ C, so the differential
        // ledger is pure Agree.
        let store = dataset(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
        let ledger =
            crosscheck_native_vs_nemo(store.as_ref()).expect("native↔Nemo dual-run must succeed");
        let verdict = ledger::enforce(&ledger);
        assert!(
            verdict.passed,
            "native and Nemo must agree over the fixed DL calculus: {:?}; rows {:#?}",
            verdict.reasons, ledger.rows
        );
        assert_eq!(
            ledger.oracle_only, 0,
            "no Nemo-only subsumption (a native coverage regression): {:#?}",
            ledger.rows
        );
        assert!(
            ledger.agree > 0,
            "non-vacuous: native and Nemo actually agree on ≥1 subsumption: {:#?}",
            ledger.rows
        );
        // The transitive A ⊑ C is derived by BOTH engines and classified Agree.
        let agrees: Vec<(&str, &str)> = ledger
            .rows
            .iter()
            .filter(|r| r.kind == DivergenceKind::Agree)
            .map(|r| (r.subject.as_str(), r.object.as_str()))
            .collect();
        assert!(
            agrees.contains(&(A, C)),
            "transitive A ⊑ C must be a shared Agree row: {agrees:?}"
        );
    }

    /// A Nemo-only subsumption the native path missed is an `OracleOnly` row that the
    /// scheduled lane's `enforce` verdict FAILS on — the differential's whole purpose.
    /// A native-only subsumption is expected superset richness and passes. This pins
    /// the failure semantics the real engines never trip on the gap-zero corpus.
    #[test]
    fn crosscheck_ledger_fails_on_nemo_only_and_passes_on_native_only() {
        let native = vec![(A.to_owned(), B.to_owned(), W.to_owned())];
        let nemo_extra = vec![
            (A.to_owned(), B.to_owned(), W.to_owned()),
            (B.to_owned(), C.to_owned(), W.to_owned()),
        ];
        // Nemo derived B ⊑ C that native did not → OracleOnly → the lane must fail.
        let rows = ledger::compare_subsumption(&native, &nemo_extra);
        let bad = ledger::build_ledger(rows, Vec::new(), Vec::new(), Vec::new());
        assert_eq!(bad.oracle_only, 1, "the Nemo-only subsumption is counted");
        assert!(
            !ledger::enforce(&bad).passed,
            "a Nemo-only subsumption must fail the differential"
        );
        // The converse — native richer than Nemo — is not a failure.
        let rows = ledger::compare_subsumption(&nemo_extra, &native);
        let rich = ledger::build_ledger(rows, Vec::new(), Vec::new(), Vec::new());
        assert_eq!(rich.native_only, 1);
        assert!(
            ledger::enforce(&rich).passed,
            "a native-only subsumption is expected richness, not a failure"
        );
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

    /// The E8 group-action law `(g·h)·x = g·(h·x)`: TERNARY `comp`/`act` atoms in the
    /// BODY (reified) and a BINARY `eq` head (a plain binary tuple, not reified). Seeded on
    /// a concrete compatible action so both bracketings reach the SAME value `r`; then the
    /// binary consequent `eq(r, r)` must be derived. This is the e8-symmetry law shape
    /// evaluating end-to-end through `reason_program`.
    #[test]
    fn reason_program_closure_dataset_carries_the_derived_nary_tuple() {
        // The closure→RDF bridge (the native competency lane's substrate): the det law's
        // closure dataset, obtained via reason_program_closure_dataset, must contain the
        // DERIVED reified argument triple logic:naryArg0(R, dA) — a triple no query over the
        // asserted EDB alone could see (R is a chase-minted reifier).
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

        let closure = reason_program_closure_dataset(&program, edb.as_ref())
            .expect("closure dataset must build");

        // Scan the projected closure for logic:naryArg0(R, dA): R is the chase-minted
        // content-addressed reifier, dA the concrete det value.
        let na0 = logic_nary_arg(0);
        let found = closure.owned_quads().any(|q| {
            q.predicate == na0
                && q.object == RdfTerm::iri(DET_A)
                && matches!(&q.subject, RdfTerm::Iri(s) if s.starts_with(NARY_REIFIER_PREFIX))
        });
        assert!(
            found,
            "the closure dataset must carry the derived logic:naryArg0(R, dA) triple; quads: {:?}",
            closure
                .owned_quads()
                .map(|q| (q.subject.clone(), q.predicate.clone(), q.object.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reason_program_evaluates_the_group_action_law_end_to_end() {
        // ∀g,h,x,gh,r1,hx,r2. comp(g,h,gh) ∧ act(gh,x,r1) ∧ act(h,x,hx) ∧ act(g,hx,r2) → eq(r1,r2)
        const COMP: &str = "http://gmeow.example/comp";
        const ACT: &str = "http://gmeow.example/act";
        const EQ: &str = "http://gmeow.example/eq";
        let v = |n: &str| Term::var(n).unwrap();
        let law = Formula::Forall {
            vars: ["g", "h", "x", "gh", "r1", "hx", "r2"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(COMP, vec![v("g"), v("h"), v("gh")]),
                    fml_atom(ACT, vec![v("gh"), v("x"), v("r1")]),
                    fml_atom(ACT, vec![v("h"), v("x"), v("hx")]),
                    fml_atom(ACT, vec![v("g"), v("hx"), v("r2")]),
                ])),
                Box::new(fml_atom(EQ, vec![v("r1"), v("r2")])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // A concrete compatible action where (g·h)·x and g·(h·x) both reach `r`.
        const G: &str = "http://gmeow.example/g";
        const H: &str = "http://gmeow.example/h";
        const XPT: &str = "http://gmeow.example/pt";
        const GH: &str = "http://gmeow.example/gh";
        const HX: &str = "http://gmeow.example/hx";
        const R: &str = "http://gmeow.example/r";
        let a0 = logic_nary_arg(0);
        let a1 = logic_nary_arg(1);
        let a2 = logic_nary_arg(2);
        // comp/act are ternary → the EDB atoms are authored PRE-REIFIED (instanceOf + naryArg).
        let mut quads = Vec::new();
        let mut reify = |node: &str, rel: &str, s: &str, t: &str, u: &str| {
            quads.push(quad(node, LOGIC_INSTANCE_OF, rel));
            quads.push(quad(node, &a0, s));
            quads.push(quad(node, &a1, t));
            quads.push(quad(node, &a2, u));
        };
        reify("http://gmeow.example/r_comp", COMP, G, H, GH); // g·h = gh
        reify("http://gmeow.example/r_act1", ACT, GH, XPT, R); // (g·h)·x = r
        reify("http://gmeow.example/r_act2", ACT, H, XPT, HX); // h·x = hx
        reify("http://gmeow.example/r_act3", ACT, G, HX, R); // g·(h·x) = r
        let edb = dataset(quads);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // The binary consequent eq(r1, r2) = eq(r, r) must be derived.
        let eq_rr = result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == EQ && ax.subject == R && ax.object == format!("<{R}>"));
        assert!(
            eq_rr,
            "the group-action law must derive eq(r, r); closure: {:?}",
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
            "the group-action law lowers exactly: {:?}",
            result.preservation
        );
    }

    /// The homomorphic-encryption law `Dec(E(a) ⊗ E(b)) = a ⊕ b`: BINARY `enc`/`dec`
    /// atoms (plain body triples) plus TERNARY `ctMul`/`ptAdd` atoms (reified body atoms)
    /// and a BINARY `eq` head. Seeded on concrete values so the decrypted ciphertext
    /// product and the plaintext sum reach the SAME value `p`; then `eq(p, p)` must be
    /// derived. This is the homomorphic-encryption law shape evaluating end-to-end.
    #[test]
    fn reason_program_evaluates_the_he_law_end_to_end() {
        // ∀a,b,ea,eb,prod,decv,sum.
        //   enc(a,ea) ∧ enc(b,eb) ∧ ctMul(ea,eb,prod) ∧ dec(prod,decv) ∧ ptAdd(a,b,sum) → eq(decv,sum)
        const ENC: &str = "http://gmeow.example/enc";
        const DEC: &str = "http://gmeow.example/dec";
        const CTMUL: &str = "http://gmeow.example/ctMul";
        const PTADD: &str = "http://gmeow.example/ptAdd";
        const EQ: &str = "http://gmeow.example/eq";
        let v = |n: &str| Term::var(n).unwrap();
        let law = Formula::Forall {
            vars: ["a", "b", "ea", "eb", "prod", "decv", "sum"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(ENC, vec![v("a"), v("ea")]),
                    fml_atom(ENC, vec![v("b"), v("eb")]),
                    fml_atom(CTMUL, vec![v("ea"), v("eb"), v("prod")]),
                    fml_atom(DEC, vec![v("prod"), v("decv")]),
                    fml_atom(PTADD, vec![v("a"), v("b"), v("sum")]),
                ])),
                Box::new(fml_atom(EQ, vec![v("decv"), v("sum")])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // Concrete values: encrypt a→ea, b→eb; the ciphertext product decrypts to `p`, and
        // the plaintext sum is the SAME `p` (the homomorphic property holds on this instance).
        const A: &str = "http://gmeow.example/pa";
        const B: &str = "http://gmeow.example/pb";
        const EA: &str = "http://gmeow.example/ea";
        const EB: &str = "http://gmeow.example/eb";
        const PROD: &str = "http://gmeow.example/prod";
        const P: &str = "http://gmeow.example/p";
        let a0 = logic_nary_arg(0);
        let a1 = logic_nary_arg(1);
        let a2 = logic_nary_arg(2);
        // Binary enc/dec are PLAIN triples; ternary ctMul/ptAdd are PRE-REIFIED.
        let mut quads = vec![
            quad(A, ENC, EA),   // enc(a) = ea
            quad(B, ENC, EB),   // enc(b) = eb
            quad(PROD, DEC, P), // dec(prod) = p
        ];
        let mut reify = |node: &str, rel: &str, s: &str, t: &str, u: &str| {
            quads.push(quad(node, LOGIC_INSTANCE_OF, rel));
            quads.push(quad(node, &a0, s));
            quads.push(quad(node, &a1, t));
            quads.push(quad(node, &a2, u));
        };
        reify("http://gmeow.example/r_ctmul", CTMUL, EA, EB, PROD); // ea ⊗ eb = prod
        reify("http://gmeow.example/r_ptadd", PTADD, A, B, P); // a ⊕ b = p
        let edb = dataset(quads);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // The binary consequent eq(decv, sum) = eq(p, p) must be derived.
        let eq_pp = result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == EQ && ax.subject == P && ax.object == format!("<{P}>"));
        assert!(
            eq_pp,
            "the homomorphic-encryption law must derive eq(p, p); closure: {:?}",
            result
                .inferred()
                .iter()
                .filter(|a| a.predicate == EQ)
                .map(|a| (&a.subject, &a.object))
                .collect::<Vec<_>>()
        );
        // A fully-evaluable law (binary + reified-ternary body) lowers exactly (no residue).
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "the homomorphic-encryption law lowers exactly: {:?}",
            result.preservation
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
