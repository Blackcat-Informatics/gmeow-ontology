// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SHACL Advanced Features (SHACL-AF) **rule** projection of the canonical `logic:`
//! program — the computation-surface projection (`design/LOGIC-SHACL-AF.md`).
//!
//! Computation (derivation / aggregation, the "map/reduce" of the external "RDF needs
//! a computation layer" proposal) is authored ONCE as `logic:` rules and PROJECTED to a
//! SHACL-AF `sh:SPARQLRule` surface — power is added to the Turing-complete canon and
//! emitted, never bolted onto SHACL (Principles 17/4/12). Each `logic:` derivation rule
//! becomes one `sh:NodeShape` carrying a `sh:rule [ a sh:SPARQLRule ; sh:construct … ]`
//! whose `CONSTRUCT { head } WHERE { body }` is the SPARQL lowering of the rule, reusing
//! the SAME term conventions as the Datalog / N3 targets (`<iri>`, `?var`, `a` for
//! `rdf:type`), so the surfaces cannot drift.
//!
//! This is a SHACL **rule** (inference / derivation) surface, kept deliberately distinct
//! from the SHACL **constraint** surfaces (`generated/shapes/*.ttl`: `sh:sparql` /
//! `sh:SPARQLTarget`) the result/frame projections emit — a `constraint` is not a
//! `derivation-rule`. The emitted document therefore lives under `generated/shacl-af/`,
//! not `generated/shapes/`.
//!
//! ## What is projected, and where the loss is
//!
//! The faithfully projectable fragment is the **stratified Horn-with-stratified-negation**
//! fragment a SHACL-AF SPARQL rule can carry: a positive body lowers to graph patterns, a
//! negation-as-failure body atom to `FILTER NOT EXISTS`, an inequality guard to
//! `FILTER(?a != ?b)`. Within it the projection is sound (`SoundUnderApproximation`).
//! Outside it — full first-order formula bodies (`program.formulas`), existential
//! (value-inventing) heads, and the modal / world / standpoint context of a
//! contextualized rule — has no faithful SHACL-AF rule form: a context-scoped rule is NOT
//! projected (emitting it over the default graph would be unsound) and is recorded as a
//! ledgered drop, never dropped in silence. The surface is **emit-only**: there is no
//! parse-back from `sh:SPARQLRule` into a `logic:` rule (Principle 4).

use gmeow_errors::abox::{BOX_ABOX, abox_annotation_turtle_lines};

use super::super::ir::{LogicAxiom, LogicProgram, LogicRule};
use super::sparql_lower::{sparql_literal, sparql_predicate};
use super::{
    GMEOW_NS, LOGIC_NS, ProjectionResult, RDF_TYPE, contract_drop_notes, is_modal_or_scoped,
    target_meta,
};

const SH_NS: &str = "http://www.w3.org/ns/shacl#";

/// A SPARQL term token for a subject/object position. A variable equal to `focus_var`
/// renders as `focus_render` (`?this` in a target SELECT, `$this` in a rule CONSTRUCT);
/// any other variable stays itself; an IRI is `<iri>`; a literal is single-quoted.
fn sparql_term(
    value: &str,
    is_literal: bool,
    focus_var: Option<&str>,
    focus_render: &str,
) -> String {
    if value.starts_with('?') {
        if focus_var == Some(value) {
            focus_render.to_owned()
        } else {
            value.to_owned()
        }
    } else if is_literal {
        sparql_literal(value)
    } else {
        format!("<{value}>")
    }
}

fn sparql_atomic(
    term: &crate::ir::AtomicTerm,
    focus_var: Option<&str>,
    focus_render: &str,
) -> String {
    if let Some(variable) = term.as_variable() {
        if focus_var == Some(variable) {
            focus_render.to_owned()
        } else {
            variable.to_owned()
        }
    } else {
        purrdf::turtle::emit_term(&term.rdf_term().expect("native RDF value"))
    }
}

/// Render one body atom as a SPARQL triple pattern `subj pred obj .`.
fn body_triple(atom: &LogicAxiom, focus_var: Option<&str>, focus_render: &str) -> String {
    let s = sparql_term(&atom.subject, false, focus_var, focus_render);
    let p = sparql_predicate(&atom.predicate);
    let o = sparql_atomic(&atom.obj, focus_var, focus_render);
    format!("{s} {p} {o} .")
}

/// The set of variables a rule's body binds **positively**: the subject/object variables
/// of its non-negated body atoms. A variable that occurs only inside a negated atom
/// (lowered to `FILTER NOT EXISTS { … }`) is out of scope in the surrounding SPARQL, and an
/// inequality guard (`distinct_pairs`, lowered to `FILTER`) constrains but never binds — so
/// neither contributes a binding. A `CONSTRUCT`/target `SELECT` head variable absent from
/// this set would emit an **unbound** variable, so the projection must refuse it.
fn positive_body_vars(rule: &LogicRule) -> std::collections::BTreeSet<String> {
    let mut bound = std::collections::BTreeSet::new();
    for atom in &rule.body {
        if atom.negated {
            continue;
        }
        // Ingress resolves the authored variable syntax. A native Literal cannot
        // bind a variable merely because its lexical form begins with a question mark.
        if atom.subject.starts_with('?') {
            bound.insert(atom.subject.clone());
        }
        if let Some(variable) = atom.obj.as_variable() {
            bound.insert(variable.to_owned());
        }
    }
    bound
}

/// Render the shared `WHERE { … }` group body of a rule: positive atoms as graph
/// patterns, NAF atoms as `FILTER NOT EXISTS`, inequality guards as `FILTER(?a != ?b)`.
/// `focus_render` distinguishes the target SELECT (`?this`) from the rule CONSTRUCT
/// (`$this`).
fn render_where(rule: &LogicRule, focus_var: Option<&str>, focus_render: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for atom in &rule.body {
        if atom.negated {
            parts.push(format!(
                "FILTER NOT EXISTS {{ {} }}",
                body_triple(atom, focus_var, focus_render)
            ));
        } else {
            parts.push(body_triple(atom, focus_var, focus_render));
        }
    }
    for (a, b) in &rule.distinct_pairs {
        let ra = if focus_var == Some(a.as_str()) {
            focus_render
        } else {
            a.as_str()
        };
        let rb = if focus_var == Some(b.as_str()) {
            focus_render
        } else {
            b.as_str()
        };
        parts.push(format!("FILTER ( {ra} != {rb} )"));
    }
    parts.join(" ")
}

/// A deterministic, collision-free local name for the generated rule shape of `rule` at
/// position `index`: `GenComputeRule_<head-predicate-local>_r<index>` (the index keeps it
/// unique even when two rules share a head predicate).
fn rule_shape_local(rule: &LogicRule, index: usize) -> String {
    let pred = if rule.head.predicate == RDF_TYPE {
        "type"
    } else {
        rule.head
            .predicate
            .rsplit(['/', '#'])
            .next()
            .unwrap_or(&rule.head.predicate)
    };
    let sanitized: String = pred
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("GenComputeRule_{sanitized}_r{index}")
}

/// A deterministic, collision-free local name for a generated subsumption-axiom rule shape:
/// `GenSubsumptionRule_<superclass-or-superproperty-local>_a<index>` (the axiom `index` keeps
/// it unique even when two axioms share a target term).
fn axiom_shape_local(axiom: &LogicAxiom, index: usize) -> String {
    let object = axiom.obj.as_iri().expect("subsumption target is an IRI");
    let target = object.rsplit(['/', '#']).next().unwrap_or(object);
    let sanitized: String = target
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("GenSubsumptionRule_{sanitized}_a{index}")
}

/// The `rdfs:isDefinedBy` target every minted SHACL-AF rule-shape individual
/// carries — mirrors the deterministic per-committed-path graph-identity IRI
/// naming convention `crate::stages::superset::rdf_fanout_graph_iri` uses in the
/// `gmeow-pipeline` crate for any RDF file under `generated/` (`RDF_FANOUT_NS` +
/// the path with its `generated/` prefix stripped). Computed here directly — no
/// reverse `logic-compile` → `pipeline` dependency exists — for the committed
/// path `generated/shacl-af/gmeow.shacl-af.ttl`
/// (`gmeow_pipeline::stages::compile_logic::SHACL_AF_PATH`). NOTE: that path
/// currently rides in the `REP_GENERATED` OPAQUE archive member
/// (`carrier.rs::build_archive_blobs`), not an authored `rdf-fanout` row in
/// `slices/core/pipeline/module.ttl` — this IRI is the document's stable
/// identity label, not (yet) an independently RDF-fold-verified named graph
/// inside `gmeow.gts`.
const SHACL_AF_GRAPH_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/graph/fanout/shacl-af/gmeow.shacl-af.ttl";

/// Emit one `sh:NodeShape` carrying a `sh:SPARQLTarget` (selecting the focus nodes as `?this`)
/// and a `sh:rule`/`sh:SPARQLRule` whose `sh:construct` derives `$this <head_pred> <head_obj>`
/// per focus node. Shared by the derivation-rule and the subsumption-axiom projections so the
/// two cannot drift. Carries the full four-annotation A-Box contract (`rdfs:label` /
/// `skos:definition` / `rdfs:isDefinedBy` / `gmeow:graphBoxRole`) every minted gmeow-namespaced
/// individual owes (`crates/errors/src/abox.rs`).
#[allow(clippy::too_many_arguments)]
fn emit_rule_shape(
    local: &str,
    label: &str,
    definition: &str,
    head_pred: &str,
    head_obj: &str,
    target_where: &str,
    construct_where: &str,
) -> String {
    let subject_iri = format!("{GMEOW_NS}{local}");
    let mut lines = vec![format!("gmeow:{local}"), "    a sh:NodeShape ;".to_string()];
    lines.extend(abox_annotation_turtle_lines(
        &subject_iri,
        label,
        definition,
        SHACL_AF_GRAPH_IRI,
        BOX_ABOX,
        "    ",
    ));
    lines.extend([
        "    sh:target [".to_string(),
        "        a sh:SPARQLTarget ;".to_string(),
        format!("        sh:select \"\"\"SELECT ?this WHERE {{ {target_where} }}\"\"\" ;"),
        "    ] ;".to_string(),
        "    sh:rule [".to_string(),
        "        a sh:SPARQLRule ;".to_string(),
        format!(
            "        sh:construct \"\"\"CONSTRUCT {{ $this {head_pred} {head_obj} }} WHERE {{ {construct_where} }}\"\"\" ;"
        ),
        "    ] .".to_string(),
    ]);
    lines.join("\n")
}

/// Project the canonical `logic:` program to a SHACL-AF `sh:SPARQLRule` rule document.
///
/// Each non-modal Horn rule with a variable (focus) head subject becomes one
/// `sh:NodeShape` with a `sh:SPARQLTarget` selecting the focus nodes and a
/// `sh:rule`/`sh:SPARQLRule` whose `sh:construct` derives the head per focus node. A
/// modal/scoped rule (no faithful SHACL-AF context form) and a ground-subject or
/// existential rule are NOT emitted — each is recorded as a ledgered drop. The full-FOL
/// `program.formulas` residue rides in via [`contract_drop_notes`].
pub fn project_shacl_af(
    program: &LogicProgram,
    loss: &mut crate::loss_ledger::LossLedger,
) -> ProjectionResult {
    let (kind, complexity, drops) = target_meta("shacl-af");

    let mut blocks: Vec<String> = vec![format!(
        "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
         # SHACL-AF rule projection of the canonical logic: program (design/LOGIC-SHACL-AF.md):\n\
         # derivation/aggregation authored in logic: and projected to sh:SPARQLRule\n\
         # (Principle 17 — computation added to the canon and emitted, never bolted onto SHACL).\n\
         @prefix gmeow: <{GMEOW_NS}> .\n\
         @prefix sh:    <{SH_NS}> .\n\
         @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos:  <http://www.w3.org/2004/02/skos/core#> ."
    )];

    let mut actual_drops: Vec<String> = Vec::new();
    // Per-drop attribution to a DOCUMENTED gmeow: source term (keyed by exact note string):
    // a rule whose head predicate (or a ground axiom whose subject/predicate) is a gmeow: term
    // with no SHACL-AF derivation form carries this loss on that term's page.
    let mut attributed: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    for (i, rule) in program.rules.iter().enumerate() {
        // A modal / world / standpoint-scoped rule (on the rule or any of its atoms) has no
        // faithful SHACL-AF form over the default graph — projecting it would be unsound, so
        // it is carried-and-flagged in the canon, not emitted here.
        let rule_modal = rule.scope.modality != super::super::ir::LogicModality::None
            || rule.scope.standpoint.is_some()
            || rule.scope.time.is_some();
        let atom_modal = is_modal_or_scoped(&rule.head) || rule.body.iter().any(is_modal_or_scoped);
        if rule_modal || atom_modal {
            let note = format!(
                "rule deriving <{}> is context-scoped (modal/standpoint/time); it has no faithful \
                 SHACL-AF projection over the default graph and is carried in the canonical logic: \
                 layer, not emitted",
                rule.head.predicate
            );
            if let Some(src) = super::gmeow_term(&rule.head.predicate) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
            continue;
        }

        // Reduce (aggregation) rule: the "reduce" half of the computation surface. It projects to
        // an aggregating sh:SPARQLRule whose CONSTRUCT carries a GROUP-BY sub-SELECT, when the
        // shape is a single-group-key focus reduce: the head subject is the (sole) group key and
        // the focus node, the head object is the aggregate result variable, and the aggregated
        // variable is positively bound by the body. Anything else is carried as a ledgered drop.
        if let Some(agg) = &rule.aggregation {
            let head = &rule.head;
            let pos_bound = positive_body_vars(rule);
            let projectable = head.subject.starts_with('?')
                && agg.group_keys == [head.subject.clone()]
                && head.obj.as_variable() == Some(agg.result_var.as_str())
                && pos_bound.contains(&agg.aggregate_var);
            if !projectable {
                let note = format!(
                    "rule deriving <{}> is an aggregation (reduce) rule whose shape is not a \
                     single-group-key focus reduce (group key = head subject, head object = the \
                     aggregate result, aggregated variable positively bound); it is carried in the \
                     canon, not emitted",
                    head.predicate
                );
                if let Some(src) = super::gmeow_term(&head.predicate) {
                    attributed.insert(note.clone(), src);
                }
                actual_drops.push(note);
                continue;
            }
            let local = rule_shape_local(rule, i);
            let focus_var = Some(head.subject.as_str());
            let head_pred = sparql_predicate(&head.predicate);
            let body = render_where(rule, focus_var, "$this");
            let func = agg.function.to_ascii_uppercase();
            // CONSTRUCT { $this <pred> ?result } WHERE { SELECT $this (FUNC(?x) AS ?result)
            //   WHERE { body($this) } GROUP BY $this }
            let construct_where = format!(
                "SELECT $this ({func}({var}) AS {result}) WHERE {{ {body} }} GROUP BY $this",
                var = agg.aggregate_var,
                result = agg.result_var,
            );
            let label = format!(
                "SHACL-AF reduce projection of the logic: rule deriving <{}> ({} aggregation, generated)",
                head.predicate, func
            );
            let definition = format!(
                "SHACL-AF rule shape deriving <{}> via {} aggregation.",
                head.predicate, func
            );
            blocks.push(emit_rule_shape(
                &local,
                &label,
                &definition,
                &head_pred,
                &agg.result_var,
                &render_where(rule, focus_var, "?this"),
                &construct_where,
            ));
            continue;
        }

        // The focus is the head subject when it is a variable. A ground-subject head (no focus
        // variable) cannot be expressed as a focus-node SHACL-AF rule soundly; record it and skip.
        if !rule.head.subject.starts_with('?') {
            let note = format!(
                "rule deriving <{}> has a ground (non-variable) head subject; the focus-node \
                 SHACL-AF rule form needs a variable subject, so it is not emitted",
                rule.head.predicate
            );
            if let Some(src) = super::gmeow_term(&rule.head.predicate) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
            continue;
        }
        // Head-variable safety: every CONSTRUCT-head variable (subject AND object) must be bound
        // by a POSITIVE body atom. A head variable bound only inside a negated atom
        // (`FILTER NOT EXISTS`) or only by an inequality guard is out of scope in the surrounding
        // SPARQL — emitting it would produce an unbound variable in the target SELECT / rule
        // CONSTRUCT (selecting/deriving nothing, or an invalid query). The derivation is carried
        // in the canon and recorded as a ledgered drop, never emitted unsoundly nor dropped
        // silently.
        let pos_bound = positive_body_vars(rule);
        if !pos_bound.contains(&rule.head.subject) {
            let note = format!(
                "rule deriving <{}> has a head subject variable not positively bound by the body \
                 (it occurs only under negation or an inequality guard); no sound SHACL-AF \
                 CONSTRUCT exists, so it is carried in the canon, not emitted",
                rule.head.predicate
            );
            if let Some(src) = super::gmeow_term(&rule.head.predicate) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
            continue;
        }
        let focus_var = Some(rule.head.subject.as_str());
        // A head object variable absent from the positive body would invent a value (existential),
        // which a sound CONSTRUCT cannot do.
        if rule
            .head
            .obj
            .as_variable()
            .is_some_and(|variable| !pos_bound.contains(variable))
        {
            let note = format!(
                "rule deriving <{}> has an existential (positively unbound) head object variable; \
                 no sound SHACL-AF CONSTRUCT exists, so it is carried in the canon, not emitted",
                rule.head.predicate
            );
            if let Some(src) = super::gmeow_term(&rule.head.predicate) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
            continue;
        }

        let local = rule_shape_local(rule, i);
        let head = &rule.head;
        let head_pred = sparql_predicate(&head.predicate);

        // Target SELECT: focus → ?this. Rule CONSTRUCT + its WHERE: focus → $this.
        let target_where = render_where(rule, focus_var, "?this");
        let construct_where = render_where(rule, focus_var, "$this");
        let head_obj_construct = sparql_atomic(&head.obj, focus_var, "$this");
        let label = format!(
            "SHACL-AF projection of the logic: rule deriving <{}> (generated)",
            head.predicate
        );
        let definition = format!("SHACL-AF rule shape deriving <{}>.", head.predicate);
        blocks.push(emit_rule_shape(
            &local,
            &label,
            &definition,
            &head_pred,
            &head_obj_construct,
            &target_where,
            &construct_where,
        ));
    }

    // Axioms are ground TBox/ABox facts, not derivation rules. Class- and property-subsumption
    // axioms DO have a sound derivation form (cax-sco / prp-spo1) and are projected to a
    // sh:SPARQLRule that materializes the subsumption; every other ground axiom (type / metamodel
    // assertions, asserted relations, domain/range, modal or scoped axioms, literal-valued
    // assertions) has no SHACL-AF rule form and is carried in the canonical RDF-1.2 layer —
    // disclosed here as a ledgered drop, never silent (the no-silent-drop contract).
    let subclass_pred = format!("{LOGIC_NS}subClassOf");
    let subprop_pred = format!("{LOGIC_NS}subPropertyOf");
    const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const RDFS_SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    for (i, axiom) in program.axioms.iter().enumerate() {
        let scoped = is_modal_or_scoped(axiom)
            || axiom.scope.modality != super::super::ir::LogicModality::None
            || axiom.scope.standpoint.is_some()
            || axiom.scope.time.is_some();
        let ground = !axiom.subject.starts_with('?')
            && axiom.obj.as_variable().is_none()
            && axiom.obj.as_iri().is_some();
        let projectable = !axiom.negated && !scoped && ground;
        let is_subclass = axiom.predicate == subclass_pred || axiom.predicate == RDFS_SUBCLASS;
        let is_subprop = axiom.predicate == subprop_pred || axiom.predicate == RDFS_SUBPROP;

        if projectable && is_subclass {
            // cax-sco: every instance of the subclass is an instance of the superclass.
            let local = axiom_shape_local(axiom, i);
            let sub = format!("<{}>", axiom.subject);
            let sup = format!("<{}>", axiom.obj.as_iri().expect("projectable IRI"));
            let label = format!(
                "SHACL-AF projection of the logic: subClassOf axiom (<{}> subClassOf <{}>) (generated)",
                axiom.subject,
                axiom.obj.as_iri().expect("projectable IRI")
            );
            let definition = format!(
                "SHACL-AF rule shape materializing the subClassOf axiom <{}> subClassOf <{}>.",
                axiom.subject,
                axiom.obj.as_iri().expect("projectable IRI")
            );
            blocks.push(emit_rule_shape(
                &local,
                &label,
                &definition,
                "a",
                &sup,
                &format!("?this a {sub} ."),
                &format!("$this a {sub} ."),
            ));
        } else if projectable && is_subprop {
            // prp-spo1: a subject related by the subproperty is related by the superproperty.
            let local = axiom_shape_local(axiom, i);
            let subp = format!("<{}>", axiom.subject);
            let supp = format!("<{}>", axiom.obj.as_iri().expect("projectable IRI"));
            let label = format!(
                "SHACL-AF projection of the logic: subPropertyOf axiom (<{}> subPropertyOf <{}>) (generated)",
                axiom.subject,
                axiom.obj.as_iri().expect("projectable IRI")
            );
            let definition = format!(
                "SHACL-AF rule shape materializing the subPropertyOf axiom <{}> subPropertyOf <{}>.",
                axiom.subject,
                axiom.obj.as_iri().expect("projectable IRI")
            );
            blocks.push(emit_rule_shape(
                &local,
                &label,
                &definition,
                &supp,
                "?o",
                &format!("?this {subp} ?o ."),
                &format!("$this {subp} ?o ."),
            ));
        } else {
            let obj_disp = sparql_atomic(&axiom.obj, None, "?this");

            let note = format!(
                "ground axiom <{}> <{}> {obj_disp} is an asserted fact (not a class/property \
                 subsumption), so it has no SHACL-AF sh:SPARQLRule derivation form and is carried \
                 in the canonical RDF-1.2 layer",
                axiom.subject, axiom.predicate
            );
            // The dropped ground axiom is ABOUT its subject (prefer a gmeow: subject, else a
            // gmeow: predicate); attribute to that documented term when present.
            if let Some(src) = super::gmeow_endpoint(&axiom.subject, &axiom.predicate) {
                attributed.insert(note.clone(), src);
            }
            actual_drops.push(note);
        }
    }

    // The full-FOL formula layer + reasoning contracts are beyond the Horn-with-NAF fragment;
    // disclose each as a flagged residue note (carried in the canon, never silent).
    actual_drops.extend(contract_drop_notes(program, "SHACL-AF", &|_| false));

    let content = format!("{}\n", blocks.join("\n\n"));
    let structural: Vec<String> = drops.into_iter().map(str::to_owned).collect();
    let attributed_drops: Vec<(String, Option<String>)> = actual_drops
        .iter()
        .map(|note| (note.clone(), attributed.get(note).cloned()))
        .collect();
    loss.record_projection_drops_attributed("shacl-af", kind, &structural, &attributed_drops);
    ProjectionResult {
        target: "shacl-af".to_owned(),
        content,
        is_rdf: false,
        preservation: kind,
        complexity: complexity.to_owned(),
    }
}

#[path = "shacl_af.tests.rs"]
#[cfg(test)]
mod tests;
