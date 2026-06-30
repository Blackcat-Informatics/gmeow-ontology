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

use super::super::ir::{LogicAxiom, LogicProgram, LogicRule};
use super::{
    contract_drop_notes, is_modal_or_scoped, target_meta, ProjectionResult, GMEOW_NS, LOGIC_NS,
    RDF_TYPE,
};

const SH_NS: &str = "http://www.w3.org/ns/shacl#";

/// A SPARQL literal rendered for embedding inside a Turtle `"""…"""` SHACL string: use a
/// single-quoted SPARQL string (so an inner `"` never collides with the triple-quoted
/// SHACL literal), escaping `\` and `'`. Mirrors the `result-shapes` embedding convention.
fn sparql_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

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

/// The SPARQL predicate token (`a` for `rdf:type`, else `<iri>`). Predicates are always
/// IRIs in the `logic:` IR.
fn sparql_predicate(predicate: &str) -> String {
    if predicate == RDF_TYPE {
        "a".to_owned()
    } else {
        format!("<{predicate}>")
    }
}

/// Render one body atom as a SPARQL triple pattern `subj pred obj .`.
fn body_triple(atom: &LogicAxiom, focus_var: Option<&str>, focus_render: &str) -> String {
    let s = sparql_term(&atom.subject, false, focus_var, focus_render);
    let p = sparql_predicate(&atom.predicate);
    let o = sparql_term(&atom.obj, atom.obj_is_literal, focus_var, focus_render);
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
        if atom.subject.starts_with('?') {
            bound.insert(atom.subject.clone());
        }
        // A literal object is never a variable; a variable object is a positive binding.
        if atom.obj.starts_with('?') && !atom.obj_is_literal {
            bound.insert(atom.obj.clone());
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
    let target = axiom.obj.rsplit(['/', '#']).next().unwrap_or(&axiom.obj);
    let sanitized: String = target
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("GenSubsumptionRule_{sanitized}_a{index}")
}

/// Emit one `sh:NodeShape` carrying a `sh:SPARQLTarget` (selecting the focus nodes as `?this`)
/// and a `sh:rule`/`sh:SPARQLRule` whose `sh:construct` derives `$this <head_pred> <head_obj>`
/// per focus node. Shared by the derivation-rule and the subsumption-axiom projections so the
/// two cannot drift.
fn emit_rule_shape(
    local: &str,
    label: &str,
    head_pred: &str,
    head_obj: &str,
    target_where: &str,
    construct_where: &str,
) -> String {
    format!(
        "gmeow:{local}\n\
         \x20   a sh:NodeShape ;\n\
         \x20   rdfs:label \"{label}\"@en ;\n\
         \x20   sh:target [\n\
         \x20       a sh:SPARQLTarget ;\n\
         \x20       sh:select \"\"\"SELECT ?this WHERE {{ {target_where} }}\"\"\" ;\n\
         \x20   ] ;\n\
         \x20   sh:rule [\n\
         \x20       a sh:SPARQLRule ;\n\
         \x20       sh:construct \"\"\"CONSTRUCT {{ $this {head_pred} {head_obj} }} WHERE {{ {construct_where} }}\"\"\" ;\n\
         \x20   ] ."
    )
}

/// Project the canonical `logic:` program to a SHACL-AF `sh:SPARQLRule` rule document.
///
/// Each non-modal Horn rule with a variable (focus) head subject becomes one
/// `sh:NodeShape` with a `sh:SPARQLTarget` selecting the focus nodes and a
/// `sh:rule`/`sh:SPARQLRule` whose `sh:construct` derives the head per focus node. A
/// modal/scoped rule (no faithful SHACL-AF context form) and a ground-subject or
/// existential rule are NOT emitted — each is recorded as a ledgered drop. The full-FOL
/// `program.formulas` residue rides in via [`contract_drop_notes`].
pub fn project_shacl_af(program: &LogicProgram) -> ProjectionResult {
    let (kind, complexity, drops) = target_meta("shacl-af");

    let mut blocks: Vec<String> = vec![format!(
        "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
         # SHACL-AF rule projection of the canonical logic: program (design/LOGIC-SHACL-AF.md):\n\
         # derivation/aggregation authored in logic: and projected to sh:SPARQLRule\n\
         # (Principle 17 — computation added to the canon and emitted, never bolted onto SHACL).\n\
         @prefix gmeow: <{GMEOW_NS}> .\n\
         @prefix sh:    <{SH_NS}> .\n\
         @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> ."
    )];

    let mut actual_drops: Vec<String> = Vec::new();

    for (i, rule) in program.rules.iter().enumerate() {
        // A modal / world / standpoint-scoped rule (on the rule or any of its atoms) has no
        // faithful SHACL-AF form over the default graph — projecting it would be unsound, so
        // it is carried-and-flagged in the canon, not emitted here.
        let rule_modal = rule.scope.modality != super::super::ir::LogicModality::None
            || rule.scope.standpoint.is_some()
            || rule.scope.time.is_some();
        let atom_modal = is_modal_or_scoped(&rule.head) || rule.body.iter().any(is_modal_or_scoped);
        if rule_modal || atom_modal {
            actual_drops.push(format!(
                "rule deriving <{}> is context-scoped (modal/standpoint/time); it has no faithful \
                 SHACL-AF projection over the default graph and is carried in the canonical logic: \
                 layer, not emitted",
                rule.head.predicate
            ));
            continue;
        }

        // The focus is the head subject when it is a variable. A ground-subject head (no focus
        // variable) cannot be expressed as a focus-node SHACL-AF rule soundly; record it and skip.
        if !rule.head.subject.starts_with('?') {
            actual_drops.push(format!(
                "rule deriving <{}> has a ground (non-variable) head subject; the focus-node \
                 SHACL-AF rule form needs a variable subject, so it is not emitted",
                rule.head.predicate
            ));
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
            actual_drops.push(format!(
                "rule deriving <{}> has a head subject variable not positively bound by the body \
                 (it occurs only under negation or an inequality guard); no sound SHACL-AF \
                 CONSTRUCT exists, so it is carried in the canon, not emitted",
                rule.head.predicate
            ));
            continue;
        }
        let focus_var = Some(rule.head.subject.as_str());
        // A head object variable absent from the positive body would invent a value (existential),
        // which a sound CONSTRUCT cannot do.
        if rule.head.obj.starts_with('?') && !pos_bound.contains(&rule.head.obj) {
            actual_drops.push(format!(
                "rule deriving <{}> has an existential (positively unbound) head object variable; \
                 no sound SHACL-AF CONSTRUCT exists, so it is carried in the canon, not emitted",
                rule.head.predicate
            ));
            continue;
        }

        let local = rule_shape_local(rule, i);
        let head = &rule.head;
        let head_pred = sparql_predicate(&head.predicate);

        // Target SELECT: focus → ?this. Rule CONSTRUCT + its WHERE: focus → $this.
        let target_where = render_where(rule, focus_var, "?this");
        let construct_where = render_where(rule, focus_var, "$this");
        let head_obj_construct = sparql_term(&head.obj, head.obj_is_literal, focus_var, "$this");
        let label = format!(
            "SHACL-AF projection of the logic: rule deriving <{}> (generated)",
            head.predicate
        );
        blocks.push(emit_rule_shape(
            &local,
            &label,
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
        let ground =
            !axiom.subject.starts_with('?') && !axiom.obj.starts_with('?') && !axiom.obj_is_literal;
        let projectable = !axiom.negated && !scoped && ground;
        let is_subclass = axiom.predicate == subclass_pred || axiom.predicate == RDFS_SUBCLASS;
        let is_subprop = axiom.predicate == subprop_pred || axiom.predicate == RDFS_SUBPROP;

        if projectable && is_subclass {
            // cax-sco: every instance of the subclass is an instance of the superclass.
            let local = axiom_shape_local(axiom, i);
            let sub = format!("<{}>", axiom.subject);
            let sup = format!("<{}>", axiom.obj);
            let label = format!(
                "SHACL-AF projection of the logic: subClassOf axiom (<{}> subClassOf <{}>) (generated)",
                axiom.subject, axiom.obj
            );
            blocks.push(emit_rule_shape(
                &local,
                &label,
                "a",
                &sup,
                &format!("?this a {sub} ."),
                &format!("$this a {sub} ."),
            ));
        } else if projectable && is_subprop {
            // prp-spo1: a subject related by the subproperty is related by the superproperty.
            let local = axiom_shape_local(axiom, i);
            let subp = format!("<{}>", axiom.subject);
            let supp = format!("<{}>", axiom.obj);
            let label = format!(
                "SHACL-AF projection of the logic: subPropertyOf axiom (<{}> subPropertyOf <{}>) (generated)",
                axiom.subject, axiom.obj
            );
            blocks.push(emit_rule_shape(
                &local,
                &label,
                &supp,
                "?o",
                &format!("?this {subp} ?o ."),
                &format!("$this {subp} ?o ."),
            ));
        } else {
            let obj_disp = if axiom.obj_is_literal {
                format!("\"{}\"", axiom.obj)
            } else {
                format!("<{}>", axiom.obj)
            };
            actual_drops.push(format!(
                "ground axiom <{}> <{}> {obj_disp} is an asserted fact (not a class/property \
                 subsumption), so it has no SHACL-AF sh:SPARQLRule derivation form and is carried \
                 in the canonical RDF-1.2 layer",
                axiom.subject, axiom.predicate
            ));
        }
    }

    // The full-FOL formula layer + reasoning contracts are beyond the Horn-with-NAF fragment;
    // disclose each as a flagged residue note (carried in the canon, never silent).
    actual_drops.extend(contract_drop_notes(program, "SHACL-AF"));

    let content = format!("{}\n", blocks.join("\n\n"));
    ProjectionResult {
        target: "shacl-af".to_owned(),
        content,
        is_rdf: false,
        preservation: kind,
        complexity: complexity.to_owned(),
        lossy_drops: drops.into_iter().map(str::to_owned).collect(),
        actual_drops,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
    use super::*;

    fn var_axiom(s: &str, p: &str, o: &str, o_lit: bool) -> LogicAxiom {
        LogicAxiom::new(s, p, o, o_lit, false, ContextualScope::default()).unwrap()
    }

    /// A small program with one Horn derivation rule:
    /// `gmeow:knowsAbout(?agent, ?subject) :- logic:assessedAgent(?a, ?agent),
    ///  logic:assessmentSubject(?a, ?subject).`
    fn ladder_program() -> LogicProgram {
        let head = var_axiom(
            "?agent",
            "https://blackcatinformatics.ca/gmeow/knowsAbout",
            "?subject",
            false,
        );
        let body = vec![
            var_axiom(
                "?a",
                "https://blackcatinformatics.ca/logic/assessedAgent",
                "?agent",
                false,
            ),
            var_axiom(
                "?a",
                "https://blackcatinformatics.ca/logic/assessmentSubject",
                "?subject",
                false,
            ),
        ];
        let rule = LogicRule::new(head, body, vec![], ContextualScope::default());
        LogicProgram::new(vec![], vec![rule], vec![], None)
    }

    #[test]
    fn projects_a_horn_rule_to_a_sparql_rule_node_shape() {
        let result = project_shacl_af(&ladder_program());
        let ttl = &result.content;
        // The doc declares its preservation honestly.
        assert_eq!(result.target, "shacl-af");
        assert_eq!(result.preservation.as_str(), "SoundUnderApproximation");
        // One NodeShape carrying a SHACL-AF SPARQLRule with a CONSTRUCT.
        assert!(ttl.contains("a sh:NodeShape"), "no NodeShape:\n{ttl}");
        assert!(ttl.contains("a sh:SPARQLRule"), "no SPARQLRule:\n{ttl}");
        assert!(
            ttl.contains("sh:construct"),
            "the rule must carry a CONSTRUCT:\n{ttl}"
        );
        // The head subject is projected as the focus node $this, the head predicate as an IRI.
        assert!(
            ttl.contains(
                "CONSTRUCT { $this <https://blackcatinformatics.ca/gmeow/knowsAbout> ?subject }"
            ),
            "head not lowered as a focus-node CONSTRUCT:\n{ttl}"
        );
        // The body atoms become graph patterns with the focus var bound to $this.
        assert!(
            ttl.contains("<https://blackcatinformatics.ca/logic/assessedAgent> $this"),
            "body atom not lowered with the focus var:\n{ttl}"
        );
        // The SPARQLTarget selects the focus nodes as ?this.
        assert!(
            ttl.contains("SELECT ?this WHERE"),
            "no SPARQLTarget select:\n{ttl}"
        );
    }

    #[test]
    fn naf_and_distinct_lower_to_filters() {
        let head = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/gmeow/derived",
            "?y",
            false,
        );
        let mut neg = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/blocked",
            "?y",
            false,
        );
        neg.negated = true;
        let pos = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/links",
            "?y",
            false,
        );
        let rule = LogicRule::new(
            head,
            vec![pos, neg],
            vec![("?x".to_owned(), "?y".to_owned())],
            ContextualScope::default(),
        );
        let program = LogicProgram::new(vec![], vec![rule], vec![], None);
        let ttl = project_shacl_af(&program).content;
        assert!(
            ttl.contains("FILTER NOT EXISTS"),
            "negation-as-failure must lower to FILTER NOT EXISTS:\n{ttl}"
        );
        assert!(
            ttl.contains("FILTER ( $this != ?y )") || ttl.contains("FILTER ( ?y != $this )"),
            "the inequality guard must lower to a FILTER:\n{ttl}"
        );
    }

    #[test]
    fn modal_scoped_rule_is_carried_not_emitted() {
        let scope = ContextualScope {
            standpoint: Some("https://blackcatinformatics.ca/gmeow/someStandpoint".to_owned()),
            ..ContextualScope::default()
        };
        let head = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/gmeow/scopedDerived",
            "?y",
            false,
        );
        let body = vec![var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/links",
            "?y",
            false,
        )];
        let rule = LogicRule::new(head, body, vec![], scope);
        let program = LogicProgram::new(vec![], vec![rule], vec![], None);
        let result = project_shacl_af(&program);
        // A context-scoped rule is NOT emitted as a SPARQLRule …
        assert!(
            !result.content.contains("scopedDerived"),
            "a context-scoped rule must not be emitted:\n{}",
            result.content
        );
        // … and the drop is disclosed, never silent.
        assert!(
            result
                .actual_drops
                .iter()
                .any(|d| d.contains("context-scoped")),
            "the skipped modal rule must be recorded as a drop: {:?}",
            result.actual_drops
        );
    }

    #[test]
    fn head_object_bound_only_by_negation_is_carried_not_emitted() {
        // Head: gmeow:derived(?x, ?y). Body binds ?x positively, but ?y appears ONLY inside a
        // negated atom (FILTER NOT EXISTS), so ?y is out of scope in the surrounding SPARQL.
        // Emitting would produce an unbound CONSTRUCT object — the rule must be carried, not emitted.
        let head = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/gmeow/derived",
            "?y",
            false,
        );
        let pos = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/links",
            "?z",
            false,
        );
        let mut neg = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/blocked",
            "?y",
            false,
        );
        neg.negated = true;
        let rule = LogicRule::new(head, vec![pos, neg], vec![], ContextualScope::default());
        let program = LogicProgram::new(vec![], vec![rule], vec![], None);
        let result = project_shacl_af(&program);
        assert!(
            !result.content.contains("a sh:SPARQLRule"),
            "a rule with a negation-only head object must NOT be emitted:\n{}",
            result.content
        );
        assert!(
            result
                .actual_drops
                .iter()
                .any(|d| d.contains("existential") && d.contains("head object")),
            "the unbound head object must be a ledgered drop, never silent: {:?}",
            result.actual_drops
        );
    }

    #[test]
    fn head_subject_bound_only_by_negation_is_carried_not_emitted() {
        // Head subject ?x is a variable but appears ONLY inside a negated body atom, so it is not
        // positively bound: the target SELECT ?this would be unbound. Carry, do not emit.
        let head = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/gmeow/derived",
            "?y",
            false,
        );
        let pos = var_axiom(
            "?w",
            "https://blackcatinformatics.ca/logic/links",
            "?y",
            false,
        );
        let mut neg = var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/blocked",
            "?y",
            false,
        );
        neg.negated = true;
        let rule = LogicRule::new(head, vec![pos, neg], vec![], ContextualScope::default());
        let program = LogicProgram::new(vec![], vec![rule], vec![], None);
        let result = project_shacl_af(&program);
        assert!(
            !result.content.contains("a sh:SPARQLRule"),
            "a rule with a negation-only head subject must NOT be emitted:\n{}",
            result.content
        );
        assert!(
            result
                .actual_drops
                .iter()
                .any(|d| d.contains("head subject") && d.contains("not positively bound")),
            "the unbound head subject must be a ledgered drop, never silent: {:?}",
            result.actual_drops
        );
    }

    #[test]
    fn subclass_axiom_projects_to_a_subsumption_rule() {
        // A ground subClassOf axiom is projected to a cax-sco sh:SPARQLRule that materializes the
        // subsumption — maximal utility, the SHACL-AF surface actually computes the closure.
        let axiom = LogicAxiom::ground(
            "https://example.org/HonorsStudent",
            "https://blackcatinformatics.ca/logic/subClassOf",
            "https://example.org/Student",
            false,
        )
        .unwrap();
        let program = LogicProgram::new(vec![axiom], vec![], vec![], None);
        let result = project_shacl_af(&program);
        let ttl = &result.content;
        assert!(
            ttl.contains("a sh:SPARQLRule"),
            "the subClassOf axiom must project to a SPARQLRule:\n{ttl}"
        );
        assert!(
            ttl.contains("CONSTRUCT { $this a <https://example.org/Student> }"),
            "cax-sco must derive the superclass type:\n{ttl}"
        );
        assert!(
            ttl.contains("$this a <https://example.org/HonorsStudent> ."),
            "the rule must trigger on the subclass type:\n{ttl}"
        );
    }

    #[test]
    fn non_subsumption_axiom_is_carried_not_silently_dropped() {
        // A ground metamodel/type axiom (not a subsumption) has no derivation form: it must be a
        // ledgered drop, never a silent disappearance.
        let axiom = LogicAxiom::ground(
            "https://example.org/Student",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "https://blackcatinformatics.ca/logic/Role",
            false,
        )
        .unwrap();
        let program = LogicProgram::new(vec![axiom], vec![], vec![], None);
        let result = project_shacl_af(&program);
        assert!(
            !result.content.contains("a sh:SPARQLRule"),
            "a non-subsumption ground axiom must NOT be projected as a rule:\n{}",
            result.content
        );
        assert!(
            result
                .actual_drops
                .iter()
                .any(|d| d.contains("ground axiom") && d.contains("asserted fact")),
            "the non-projected ground axiom must be a ledgered drop, never silent: {:?}",
            result.actual_drops
        );
    }
}
