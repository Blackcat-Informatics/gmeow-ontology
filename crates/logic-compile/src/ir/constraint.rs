// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Procedural constraints — the closed-world, integrity-condition subset of the IR.
//!
//! A [`ConstraintIr`] is the IR realization of [`NodeKind::Constraint`]: a closed-world
//! *integrity condition* whose violation is a **finding**, not a derivation (contrast
//! [`super::LogicRule`], whose satisfaction *produces* a head). It reuses the realized
//! first-order [`Formula`] core verbatim — the integrity condition is an outer
//! range-restricted `∀`-guarded [`Formula::Forall`] whose body is the per-focus condition —
//! and reuses [`ShapeTarget`] / [`ShaclSeverity`] verbatim from the sibling
//! [`ValidationShapeIr`](super::ValidationShapeIr). It is NOT a new canonical construct and
//! NOT a parallel shape DSL: it is the typed home for the closed-world *procedural* checks
//! (choice groups, guarded requiredness, disjunctive requiredness, cross-node co-occurrence,
//! forbidden patterns, …) that later tasks project to `sh:SPARQLConstraint`.
//!
//! Identity is the content-addressed [`ConstraintIr::content_key`], folded over the
//! iri + target + integrity-formula key + severity. The advisory `message` is
//! **load-bearing-false** — it never enters the content key (two constraints differing only
//! in their message share an identity). The `formalizes` back-reference mirrors the
//! `logic:formalizes` *annotation* property (which carries "no DL or EL profile weight"), so
//! it is likewise annotation-level and excluded from the content key.

use super::validation::{ShaclSeverity, ShapeTarget};
use super::{Formula, SEP, Term};

/// The `rdf:type` IRI — the relation of a class-membership guard atom `rdf:type(this, C)`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Length-prefix a free-form fragment so field boundaries can never collide when fragments
/// are concatenated into a content key (mirrors the `validation` module's helper verbatim).
fn key_field(s: &str) -> String {
    format!("{}:{s}", s.len())
}

/// A named closed-world procedural constraint (`logic:Constraint`): the typed home for a
/// closed-world integrity condition whose violation is a finding. The canonical form the
/// `sh:SPARQLConstraint` surface projects from. Identity is the content-addressed
/// [`Self::content_key`]; the `iri` is the sort key.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintIr {
    /// IRI string of the constraint individual (identity / sort key).
    pub iri: String,
    /// The closed-world integrity condition: an outer range-restricted `∀`-guarded
    /// [`Formula::Forall`] whose body is the per-focus condition. Reuses the realized FOL
    /// [`Formula`] core verbatim — no bespoke constraint AST.
    pub integrity: Formula,
    /// The focus-node selector, DERIVED from the outermost `∀`'s guard atom (the class the
    /// bound `$this`-analogue is `rdf:type`-restricted to, or the predicate it is the
    /// subject / object of). Never authored directly — [`Self::new`] extracts it and
    /// hard-fails if the integrity is not a range-restricted `∀`-guarded condition.
    pub target: ShapeTarget,
    /// The `sh:severity` a violation reports at.
    pub severity: ShaclSeverity,
    /// The advisory violation message (`None` ⇒ none). **Load-bearing-false**: carried for
    /// validation-failure UX but MUST NOT enter [`Self::content_key`], so two constraints
    /// differing only in message share one identity.
    pub message: Option<String>,
    /// The gmeow-domain term this constraint formalizes (`None` ⇒ none) — the back-reference
    /// later projected as `logic:formalizes`. Annotation-level (like the `logic:formalizes`
    /// annotation property, which carries no DL/EL profile weight), so excluded from the
    /// content key.
    pub formalizes: Option<String>,
}

impl ConstraintIr {
    /// Construct a procedural constraint, DERIVING [`Self::target`] from the integrity
    /// formula's outermost `∀` guard. **Hard-fails** with a clear message when `integrity`
    /// is not a range-restricted, `∀`-guarded condition — i.e. it must be
    /// `∀ this. guard(this) → condition(this)` where `guard(this)` names either a class
    /// membership (`rdf:type(this, C)` ⇒ [`ShapeTarget::Class`]) or a predicate the focus is
    /// the subject / object of (⇒ [`ShapeTarget::SubjectsOf`] / [`ShapeTarget::ObjectsOf`]).
    /// Validates the IRI is a non-empty string.
    pub fn new(
        iri: impl Into<String>,
        integrity: Formula,
        severity: ShaclSeverity,
        message: Option<String>,
    ) -> Result<Self, String> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err("ConstraintIr.iri must be a non-empty IRI string".to_owned());
        }
        if let Some(msg) = &message
            && msg.trim().is_empty()
        {
            return Err(
                "ConstraintIr.message must be a non-empty string when present; pass None to \
                 leave it unset"
                    .to_owned(),
            );
        }
        let target = target_from_integrity(&integrity)?;
        Ok(Self {
            iri,
            integrity,
            target,
            severity,
            message,
            formalizes: None,
        })
    }

    /// Attach the `logic:formalizes` back-reference (the gmeow-domain term the constraint
    /// formalizes). Chainable; annotation-level, so it never perturbs the content key. A
    /// blank term is rejected (a required back-reference that says nothing is a determinism
    /// hazard, not a silent no-op).
    pub fn with_formalizes(mut self, formalizes: impl Into<String>) -> Result<Self, String> {
        let formalizes = formalizes.into();
        if formalizes.trim().is_empty() {
            return Err(
                "ConstraintIr.with_formalizes: the formalized term must be a non-empty IRI"
                    .to_owned(),
            );
        }
        self.formalizes = Some(formalizes);
        Ok(self)
    }

    /// Stable sort key for canonical ordering — the constraint IRI is unique.
    pub fn sort_key(&self) -> String {
        self.iri.clone()
    }

    /// A deterministic full-content key for canonical equality. Public to the crate so
    /// [`super::LogicProgram::canonical_key`] can fold it into the program key at the fixed
    /// tail. Folded over `iri` + `target` + `integrity`'s alpha/order-normalized key +
    /// `severity`. The advisory `message` and the annotation-level `formalizes` are
    /// **excluded** by design.
    pub(crate) fn content_key(&self) -> String {
        format!(
            "iri={}{SEP}{}{SEP}integrity={}{SEP}sev={}",
            key_field(&self.iri),
            self.target.content_key(),
            key_field(&self.integrity.content_key()),
            self.severity.as_str(),
        )
    }
}

/// Derive the [`ShapeTarget`] from the outermost `∀` guard of a range-restricted integrity
/// condition, or hard-fail with a clear diagnostic. The accepted shape is
/// `∀ this[, …]. guard(this) → condition` where `guard(this)` is the antecedent of the `∀`
/// body's material implication — either a single atom or a conjunction of atoms — and names
/// how the focus `this` (the FIRST bound variable) ranges:
///
/// * `rdf:type(this, C)` ⇒ [`ShapeTarget::Class`] `C` (preferred when present),
/// * `P(this, _)` ⇒ [`ShapeTarget::SubjectsOf`] `P`,
/// * `P(_, this)` ⇒ [`ShapeTarget::ObjectsOf`] `P`.
fn target_from_integrity(integrity: &Formula) -> Result<ShapeTarget, String> {
    let Formula::Forall { vars, body } = integrity else {
        return Err(
            "ConstraintIr integrity must be a range-restricted universal \
             (∀ this. guard(this) → condition); the top node is not a ∀"
                .to_owned(),
        );
    };
    let focus = vars.first().ok_or_else(|| {
        "ConstraintIr integrity ∀ binds no focus variable; a range-restricted constraint needs \
         a bound $this-analogue"
            .to_owned()
    })?;
    let Formula::Implies(antecedent, _consequent) = body.as_ref() else {
        return Err("ConstraintIr integrity must be a guarded implication \
             (∀ this. guard(this) → condition); the ∀ body is not a material implication"
            .to_owned());
    };
    // The guard is either a single atom or a conjunction of atoms; gather the atoms.
    let guard_atoms: Vec<&Formula> = match antecedent.as_ref() {
        atom @ Formula::Atom { .. } => vec![atom],
        Formula::And(fs) => fs.iter().collect(),
        _ => {
            return Err(
                "ConstraintIr integrity guard must be an atom or a conjunction of atoms that \
                 range-restricts the focus variable"
                    .to_owned(),
            );
        }
    };

    // Prefer a class-membership guard `rdf:type(this, C)`.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && matches!(relation, Term::Iri(iri) if iri == RDF_TYPE)
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
            && let Term::Iri(class) = &args[1]
        {
            return Ok(ShapeTarget::Class(class.clone()));
        }
    }
    // Else a binary predicate guard with the focus as its subject.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && let Term::Iri(pred) = relation
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
        {
            return Ok(ShapeTarget::SubjectsOf(pred.clone()));
        }
    }
    // Else a binary predicate guard with the focus as its object.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && let Term::Iri(pred) = relation
            && args.len() == 2
            && matches!(&args[1], Term::Var(v) if v == focus)
        {
            return Ok(ShapeTarget::ObjectsOf(pred.clone()));
        }
    }
    Err(format!(
        "ConstraintIr integrity guard does not range-restrict the focus variable '{focus}': no \
         guard atom is rdf:type(this, C) or a binary predicate over this"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse_logic_str;

    /// The `logic:` namespace prefix + rdf, used by every authored-RDF fixture below.
    const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <https://ex/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
";

    /// Parse a `logic:` Turtle fixture and return its constraints (asserting no parse error).
    fn constraints_of(turtle: &str) -> Vec<ConstraintIr> {
        let src = format!("{PREFIXES}{turtle}");
        let (program, diagnostics) = parse_logic_str(&src, None).expect("fixture must parse");
        // A malformed-constraint fixture would surface a MALFORMED_CONSTRAINT warning; the
        // seven pattern fixtures are all well-formed, so none is expected.
        assert!(
            !diagnostics.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
            "unexpected MALFORMED_CONSTRAINT diagnostics: {diagnostics:?}"
        );
        program.constraints
    }

    /// A guarded `∀ this. rdf:type(this, ex:Widget) → <body>` scaffold, so each pattern
    /// fixture only has to author its per-focus condition `<body>`.
    fn guarded(iri: &str, body_ttl: &str, body_node: &str) -> String {
        format!(
            "\
{iri} a logic:Constraint ;
  logic:severity \"Violation\" ;
  logic:integrity {iri}_all .

{iri}_all a logic:Formula ;
  logic:forall {iri}_impl ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"this\" ] .

{iri}_impl a logic:Formula ;
  logic:antecedent {iri}_guard ;
  logic:consequent {body_node} .

{iri}_guard a logic:Formula ;
  logic:relation rdf:type ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termIri ex:Widget ] .

{body_ttl}
"
        )
    }

    #[test]
    fn p1_choice_group_exactly_one_round_trips() {
        // ∀ this. Widget(this) → ((∃a. hasA(this,a) ∧ ¬∃b. hasB(this,b))
        //                        ∨ (¬∃a. hasA(this,a) ∧ ∃b. hasB(this,b)))
        let body = "\
ex:c1_body a logic:Formula ;
  logic:or ex:c1_left , ex:c1_right .

ex:c1_left a logic:Formula ;
  logic:and ex:c1_a , ex:c1_notb .
ex:c1_right a logic:Formula ;
  logic:and ex:c1_nota , ex:c1_b .

ex:c1_a a logic:Formula ;
  logic:exists ex:c1_atomA ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"a\" ] .
ex:c1_b a logic:Formula ;
  logic:exists ex:c1_atomB ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c1_notb a logic:Formula ; logic:not ex:c1_b .
ex:c1_nota a logic:Formula ; logic:not ex:c1_a .

ex:c1_atomA a logic:Formula ;
  logic:relation ex:hasA ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"a\" ] .
ex:c1_atomB a logic:Formula ;
  logic:relation ex:hasB ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
        let cs = constraints_of(&guarded("ex:c1", body, "ex:c1_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        // The key is stable across a re-parse of the identical source.
        let again = constraints_of(&guarded("ex:c1", body, "ex:c1_body"));
        assert_eq!(cs[0].content_key(), again[0].content_key());
    }

    #[test]
    fn p2_guarded_implication_round_trips() {
        // ∀ this. Widget(this) → ∃c. companion(this, c)
        let body = "\
ex:c2_body a logic:Formula ;
  logic:exists ex:c2_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:c2_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
        let cs = constraints_of(&guarded("ex:c2", body, "ex:c2_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        assert!(cs[0].content_key().contains("class="));
    }

    #[test]
    fn p3_disjunctive_requiredness_round_trips() {
        // ∀ this. Widget(this) → (∃a. hasA(this,a) ∨ ∃b. hasB(this,b))
        let body = "\
ex:c3_body a logic:Formula ;
  logic:or ex:c3_a , ex:c3_b .
ex:c3_a a logic:Formula ;
  logic:exists ex:c3_atomA ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"a\" ] .
ex:c3_b a logic:Formula ;
  logic:exists ex:c3_atomB ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c3_atomA a logic:Formula ;
  logic:relation ex:hasA ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"a\" ] .
ex:c3_atomB a logic:Formula ;
  logic:relation ex:hasB ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
        let cs = constraints_of(&guarded("ex:c3", body, "ex:c3_body"));
        assert_eq!(cs.len(), 1);
        // Disjunctive body ⇒ the integrity formula carries the Disjunctive shape tag.
        assert!(
            cs[0]
                .integrity
                .shape_tags()
                .contains(&crate::ir::FormulaShape::Disjunctive)
        );
    }

    #[test]
    fn p4_path_value_type_membership_round_trips() {
        // ∀ this. Widget(this) → ∀v. part(this, v) → Part(v)
        let body = "\
ex:c4_body a logic:Formula ;
  logic:forall ex:c4_inner ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"v\" ] .
ex:c4_inner a logic:Formula ;
  logic:antecedent ex:c4_path ;
  logic:consequent ex:c4_type .
ex:c4_path a logic:Formula ;
  logic:relation ex:part ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"v\" ] .
ex:c4_type a logic:Formula ;
  logic:relation rdf:type ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"v\" ] ,
                 [ logic:termIndex 1 ; logic:termIri ex:Part ] .";
        let cs = constraints_of(&guarded("ex:c4", body, "ex:c4_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    }

    #[test]
    fn p5_cross_node_co_occurrence_round_trips() {
        // ∀ this. Widget(this) → ∀o. linked(this, o) → ∃m. marker(o, m)
        let body = "\
ex:c5_body a logic:Formula ;
  logic:forall ex:c5_inner ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"o\" ] .
ex:c5_inner a logic:Formula ;
  logic:antecedent ex:c5_link ;
  logic:consequent ex:c5_ex .
ex:c5_link a logic:Formula ;
  logic:relation ex:linked ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"o\" ] .
ex:c5_ex a logic:Formula ;
  logic:exists ex:c5_marker ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"m\" ] .
ex:c5_marker a logic:Formula ;
  logic:relation ex:marker ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"o\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"m\" ] .";
        let cs = constraints_of(&guarded("ex:c5", body, "ex:c5_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    }

    #[test]
    fn p6_aggregate_comparison_round_trips() {
        // ∀ this. Widget(this) → ∃n. (partCount(this, n) ∧ atMost(n, "10"^^xsd:integer))
        //
        // NOTE (P6 aggregation finding): the realized FOL `Formula` core has NO aggregate /
        // reduce node — `AggregateSpec` is a `LogicRule`-only construct with no formula-level
        // analogue. An aggregate comparison is therefore authored the ONLY honest FOL way: as
        // an atomic predication over a reified aggregate relation (`partCount(this, n)`) plus a
        // comparison atom (`atMost(n, 10)`). This is a genuine FOL encoding, not a stub — it
        // round-trips with a stable key like every other pattern.
        let body = "\
ex:c6_body a logic:Formula ;
  logic:exists ex:c6_conj ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"n\" ] .
ex:c6_conj a logic:Formula ;
  logic:and ex:c6_count , ex:c6_cmp .
ex:c6_count a logic:Formula ;
  logic:relation ex:partCount ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"n\" ] .
ex:c6_cmp a logic:Formula ;
  logic:relation ex:atMost ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"n\" ] ,
                 [ logic:termIndex 1 ; logic:termLiteral \"10\" ;
                   logic:termLiteralDatatype xsd:integer ] .";
        let cs = constraints_of(&guarded("ex:c6", body, "ex:c6_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        let again = constraints_of(&guarded("ex:c6", body, "ex:c6_body"));
        assert_eq!(cs[0].content_key(), again[0].content_key());
    }

    #[test]
    fn p7_forbidden_pattern_round_trips() {
        // ∀ this. Widget(this) → ¬∃b. forbidden(this, b)
        let body = "\
ex:c7_body a logic:Formula ; logic:not ex:c7_ex .
ex:c7_ex a logic:Formula ;
  logic:exists ex:c7_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c7_atom a logic:Formula ;
  logic:relation ex:forbidden ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
        let cs = constraints_of(&guarded("ex:c7", body, "ex:c7_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        assert!(
            cs[0]
                .integrity
                .shape_tags()
                .contains(&crate::ir::FormulaShape::StrongNegation)
        );
    }

    #[test]
    fn subjects_of_and_objects_of_targets_are_derived_from_the_guard() {
        // A predicate-guard `P(this, _)` ⇒ SubjectsOf; `P(_, this)` ⇒ ObjectsOf.
        let this = Term::Var("this".into());
        let other = Term::Var("y".into());
        let pred = Term::Iri("https://ex/P".into());
        let guard_subj = Formula::atom(pred.clone(), vec![this.clone(), other.clone()]).unwrap();
        let guard_obj = Formula::atom(pred.clone(), vec![other, this.clone()]).unwrap();
        let cond = Formula::atom(Term::Iri("https://ex/ok".into()), vec![this.clone()]).unwrap();
        let mk = |guard: Formula| Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(Box::new(guard), Box::new(cond.clone()))),
        };
        let subj = ConstraintIr::new(
            "https://ex/cs",
            mk(guard_subj),
            ShaclSeverity::Violation,
            None,
        )
        .unwrap();
        assert_eq!(subj.target, ShapeTarget::SubjectsOf("https://ex/P".into()));
        let obj = ConstraintIr::new(
            "https://ex/co",
            mk(guard_obj),
            ShaclSeverity::Violation,
            None,
        )
        .unwrap();
        assert_eq!(obj.target, ShapeTarget::ObjectsOf("https://ex/P".into()));
    }

    #[test]
    fn message_is_excluded_from_content_key() {
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/W".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
            )),
        };
        let a = ConstraintIr::new(
            "https://ex/c",
            integrity.clone(),
            ShaclSeverity::Violation,
            Some("first message".into()),
        )
        .unwrap();
        let b = ConstraintIr::new(
            "https://ex/c",
            integrity,
            ShaclSeverity::Violation,
            Some("a completely different message".into()),
        )
        .unwrap();
        assert_eq!(
            a.content_key(),
            b.content_key(),
            "message must not affect the content key"
        );
        // Formalizes is likewise annotation-level and excluded.
        let c = a.clone().with_formalizes("https://ex/gmeow/Term").unwrap();
        assert_eq!(a.content_key(), c.content_key());
    }

    #[test]
    fn target_extraction_hard_fails_on_a_non_guarded_formula() {
        // A bare atom (no ∀) is not a range-restricted constraint.
        let bare = Formula::atom(
            Term::Iri("https://ex/p".into()),
            vec![Term::Var("x".into()), Term::Var("y".into())],
        )
        .unwrap();
        let err =
            ConstraintIr::new("https://ex/c", bare, ShaclSeverity::Violation, None).unwrap_err();
        assert!(err.contains("range-restricted universal"), "got: {err}");

        // A ∀ whose body is not an implication (no guard) also fails.
        let unguarded = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(
                Formula::atom(
                    Term::Iri("https://ex/p".into()),
                    vec![Term::Var("this".into())],
                )
                .unwrap(),
            ),
        };
        let err = ConstraintIr::new("https://ex/c", unguarded, ShaclSeverity::Violation, None)
            .unwrap_err();
        assert!(err.contains("guarded implication"), "got: {err}");
    }

    #[test]
    fn empty_constraints_program_content_key_is_byte_identical() {
        // A program with no constraints must fold to the exact same canonical key as one
        // constructed before the constraints field existed — the append-only guarantee.
        use crate::ir::{LogicAxiom, LogicProgram};
        let ax = LogicAxiom::ground("https://ex/s", "https://ex/p", "https://ex/o", false).unwrap();
        let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
        let with_empty = LogicProgram::new(vec![ax], vec![], vec![], None).with_constraints(vec![]);
        assert_eq!(
            base.canonical_key(),
            with_empty.canonical_key(),
            "an empty-constraints program must keep the byte-identical historical key"
        );
        assert!(!base.canonical_key().contains("CONSTRAINTS"));
    }

    #[test]
    fn non_empty_constraints_perturb_the_program_key() {
        use crate::ir::LogicProgram;
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/W".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
            )),
        };
        let c =
            ConstraintIr::new("https://ex/c", integrity, ShaclSeverity::Violation, None).unwrap();
        let base = LogicProgram::new(vec![], vec![], vec![], None);
        let with_c = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        assert_ne!(base.canonical_key(), with_c.canonical_key());
        assert!(with_c.canonical_key().contains("CONSTRAINTS"));
    }
}
