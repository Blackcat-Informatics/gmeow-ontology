// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::error::TstpParse;

const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

/// The flagship fixture: a derivation OUR OWN reasoner produced, byte-pinned against
/// `ProofTree::to_tstp` by `gmeow_conformance::external::tptp::lower_fol`.
const FIXTURE: &[u8] = include_bytes!("../../fixtures/theorem-subclass.tstp");

/// E prover shape: `fof` conclusions, quantifiers, equality, `file(…)` leaves, a
/// `negated_conjecture`, and `status(thm)` throughout.
const EPROVER_FOF: &[u8] = include_bytes!("../../fixtures/eprover-fof.tstp");

/// Vampire shape: a `conjecture` step, a DERIVED `negated_conjecture`, and empty
/// inference status lists.
const VAMPIRE: &[u8] = include_bytes!("../../fixtures/vampire-cnf-refutation.tstp");

/// E prover shape including the clausification prefix: `status(cth)`, `status(esa)`,
/// and a `<useful_info>` field — the three residue-bearing constructs.
const EPROVER_CLAUSIFY: &[u8] = include_bytes!("../../fixtures/eprover-clausify-status.tstp");

/// A synthetic derivation exercising what the fixture does not: several parents in one
/// inference, a disjunctive and a negated conclusion, a variable, a nested term, a
/// repeated sub-term, and a shared sub-proof.
const RICH: &[u8] = b"cnf(ax_p, axiom, p(f(a), X)).\n\
                          cnf(ax_q, axiom, q(f(a))).\n\
                          cnf(d_left, plain, ( ~r(f(a)) | s(b) ), \
                              inference(res, [status(thm)], [ax_p, ax_q])).\n\
                          cnf(d_right, plain, t(b), inference(res, [status(thm)], [ax_q])).\n\
                          cnf(d_top, plain, $false, \
                              inference(unit, [status(thm)], [d_left, d_right])).\n";

const RDF_TYPE_LINE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

fn turtle(source: &[u8]) -> String {
    lift(source, BASE)
        .unwrap_or_else(|e| panic!("the derivation must lift: {e}"))
        .turtle
}

fn count(ttl: &str, predicate: &str) -> usize {
    ttl.matches(&format!("<{predicate}>")).count()
}

/// How many subjects the graph types as `math:{class}`.
///
/// Exact rather than substring: `math:Proof` must not be counted by `math:ProofStep`,
/// nor `math:Axiom` by anything sharing the word.
fn typed(ttl: &str, class: &str) -> usize {
    typed_as(ttl, &math(class))
}

fn typed_as(ttl: &str, class: &str) -> usize {
    let suffix = format!("{RDF_TYPE_LINE} <{class}> .");
    ttl.lines().filter(|line| line.ends_with(&suffix)).count()
}

// -- a tiny reader over the emitted Turtle ---------------------------------

/// One triple of the canonical, one-triple-per-line Turtle the [`Sink`] serializes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Triple {
    subject: String,
    predicate: String,
    object: String,
    literal: bool,
}

/// Read the emitted graph back as triples — the ONLY channel the reconstruction below
/// is allowed to use. No lift state, no parser, just the bytes a consumer receives.
fn triples(ttl: &str) -> Vec<Triple> {
    let mut out = Vec::new();
    for line in ttl.lines() {
        if line.is_empty() || line.starts_with('@') {
            continue;
        }
        let rest = line
            .strip_suffix(" .")
            .unwrap_or_else(|| panic!("a Turtle line ends in ` .`: {line}"));
        let (subject, rest) = rest.split_once(' ').expect("subject then predicate");
        let (predicate, object) = rest.split_once(' ').expect("predicate then object");
        let literal = object.starts_with('"');
        out.push(Triple {
            subject: unwrap_iri(subject),
            predicate: unwrap_iri(predicate),
            object: if literal {
                unwrap_literal(object)
            } else {
                unwrap_iri(object)
            },
            literal,
        });
    }
    out
}

fn unwrap_iri(term: &str) -> String {
    term.strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or_else(|| panic!("an IRI term is angle-bracketed: {term}"))
        .to_owned()
}

/// The lexical form of a Turtle literal, with the escapes the codec writes undone.
fn unwrap_literal(term: &str) -> String {
    let body = term
        .strip_prefix('"')
        .expect("a literal starts with a quote");
    let end = {
        let bytes = body.as_bytes();
        let mut cursor = 0;
        loop {
            assert!(cursor < bytes.len(), "an unterminated literal: {term}");
            match bytes[cursor] {
                b'\\' => cursor += 2,
                b'"' => break cursor,
                _ => cursor += 1,
            }
        }
    };
    let mut out = String::with_capacity(end);
    let mut chars = body[..end].chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => panic!("a dangling escape in {term}"),
        }
    }
    out
}

/// A tiny read-only index over the emitted graph.
struct Graph {
    triples: Vec<Triple>,
}

impl Graph {
    fn of(ttl: &str) -> Self {
        Self {
            triples: triples(ttl),
        }
    }

    fn objects(&self, subject: &str, predicate: &str) -> Vec<String> {
        self.triples
            .iter()
            .filter(|t| t.subject == subject && t.predicate == predicate)
            .map(|t| t.object.clone())
            .collect()
    }

    fn object(&self, subject: &str, predicate: &str) -> String {
        let found = self.objects(subject, predicate);
        let [one] = found.as_slice() else {
            panic!("expected exactly one <{predicate}> on <{subject}>, found {found:?}");
        };
        one.clone()
    }

    fn label(&self, subject: &str) -> String {
        self.object(subject, LABEL)
    }

    fn subjects_typed(&self, class: &str) -> BTreeSet<String> {
        self.triples
            .iter()
            .filter(|t| t.predicate == crate::ns::RDF_TYPE && t.object == class)
            .map(|t| t.subject.clone())
            .collect()
    }

    /// The subject labelled exactly `text`, when there is exactly one.
    fn labelled(&self, text: &str) -> String {
        let found: Vec<String> = self
            .triples
            .iter()
            .filter(|t| t.predicate == LABEL && t.object == text)
            .map(|t| t.subject.clone())
            .collect();
        let [one] = found.as_slice() else {
            panic!("expected exactly one node labelled `{text}`, found {found:?}");
        };
        one.clone()
    }

    /// The `math:MathematicalStatement` of the step named `name`.
    fn statement_of(&self, name: &str) -> String {
        let node = self.labelled(name);
        self.triples
            .iter()
            .find(|t| {
                t.object == node
                    && (t.predicate == math("dependsOnAxiom") || t.predicate == math("hasPremise"))
                    && self
                        .subjects_typed(&math("MathematicalStatement"))
                        .contains(&t.subject)
            })
            .map(|t| t.subject.clone())
            .or_else(|| {
                let drawn = self.objects(&node, &math("hasConclusion"));
                drawn.into_iter().next()
            })
            .unwrap_or_else(|| panic!("step `{name}` has no statement"))
    }
}

/// One derivation step, as rebuilt from the emitted graph ALONE.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Rebuilt {
    name: String,
    role: String,
    derived: bool,
    rule: Option<String>,
    parents: BTreeSet<String>,
    conclusion: String,
}

/// Reconstruct the derivation from the lifted Turtle — the section/retraction claim,
/// executed.
///
/// Reads nothing but the graph. A derived step is a `math:ProofStep`: its name is its
/// `rdfs:label`, its rule the `rdfs:label` of its `math:usesInferenceRule` operation (or
/// none, for a bare DAG source), its parents the `rdfs:label`s of its `math:hasPremise`
/// targets, and its role and conclusion the label and `math:hasConclusion` of the
/// statement it draws. A leaf is a `math:MathematicalStatement` no step draws: its
/// formula object hangs off `math:dependsOnAxiom` (a law) or `math:hasPremise`.
fn reconstruct(ttl: &str) -> BTreeSet<Rebuilt> {
    let graph = Graph::of(ttl);
    let statements = graph.subjects_typed(&math("MathematicalStatement"));
    let mut out = BTreeSet::new();
    let mut drawn: BTreeSet<String> = BTreeSet::new();

    for step in graph.subjects_typed(&math("ProofStep")) {
        let target = graph.object(&step, &math("hasConclusion"));
        let (role, conclusion) = if statements.contains(&target) {
            drawn.insert(target.clone());
            (
                graph.label(&target),
                graph.label(&graph.object(&target, &math("hasConclusion"))),
            )
        } else {
            // No statement: the `unknown` role, whose absence the run enumerates.
            ("unknown".to_owned(), graph.label(&target))
        };
        out.insert(Rebuilt {
            name: graph.label(&step),
            role,
            derived: true,
            rule: graph
                .objects(&step, &math("usesInferenceRule"))
                .first()
                .map(|operation| graph.label(operation)),
            parents: graph
                .objects(&step, &math("hasPremise"))
                .iter()
                .map(|parent| graph.label(parent))
                .collect(),
            conclusion,
        });
    }

    for statement in &statements {
        if drawn.contains(statement) {
            continue;
        }
        let mut objects = graph.objects(statement, &math("dependsOnAxiom"));
        objects.extend(graph.objects(statement, &math("hasPremise")));
        let [leaf] = objects.as_slice() else {
            panic!("a leaf statement names exactly one formula object, found {objects:?}");
        };
        out.insert(Rebuilt {
            name: graph.label(leaf),
            role: graph.label(statement),
            derived: false,
            rule: None,
            parents: BTreeSet::new(),
            conclusion: graph.label(&graph.object(statement, &math("hasConclusion"))),
        });
    }
    out
}

/// The same view, taken from the PARSE rather than from the graph.
fn expected(source: &[u8]) -> BTreeSet<Rebuilt> {
    tstp::parse(source)
        .expect("the fixture parses")
        .steps()
        .iter()
        .map(|step| Rebuilt {
            name: step.name.clone(),
            role: step.role.as_str().to_owned(),
            derived: step.is_derived(),
            rule: step.rule().map(str::to_owned),
            parents: step.parents.iter().cloned().collect(),
            conclusion: step.conclusion.render(),
        })
        .collect()
}

// -- THE RUNG: the round-trip that earns the section/retraction claim ------

#[test]
fn the_lift_is_a_section_the_derivation_reconstructs_from_the_graph_alone() {
    // `Rung::section_retraction`'s own doc: "It is only honest if the lift carries every
    // step name, inference rule, parent edge, and rendered conclusion — i.e. if the
    // derivation genuinely reconstructs. The proof bridge owes a round-trip test for
    // that claim; without one this constructor must not be used." This is that test,
    // strengthened with the ROLE, over every source that claims the rung.
    for source in [FIXTURE, RICH, EPROVER_FOF, VAMPIRE] {
        let ttl = turtle(source);
        assert!(
            ttl.contains(&logic("SectionRetraction")),
            "this source must claim the strong rung"
        );
        assert_eq!(
            reconstruct(&ttl),
            expected(source),
            "the derivation did not reconstruct from the lifted graph"
        );
    }
}

#[test]
fn the_reconstruction_would_notice_a_dropped_step_a_wrong_rule_or_a_wrong_role() {
    // A round-trip test is only evidence if it can FAIL, so pin that the comparison is
    // sensitive to each of the five facts the rung rests on.
    let mut rebuilt = reconstruct(&turtle(RICH));
    let full = rebuilt.clone();
    assert_eq!(full.len(), 5, "five steps: 2 axioms + 3 inferences");

    let victim = full.iter().find(|s| s.derived).expect("a derived step");
    rebuilt.remove(victim);
    assert_ne!(rebuilt, full, "a dropped step must change the rebuild");

    let mut altered = victim.clone();
    altered.rule = Some("a-different-rule".to_owned());
    rebuilt.insert(altered.clone());
    assert_ne!(rebuilt, full, "a changed rule must change the rebuild");

    let mut rebuilt = full.clone();
    let leaf = full.iter().find(|s| !s.derived).expect("a leaf").clone();
    rebuilt.remove(&leaf);
    let mut relabelled = leaf.clone();
    relabelled.role = "negated_conjecture".to_owned();
    rebuilt.insert(relabelled);
    assert_ne!(rebuilt, full, "a changed ROLE must change the rebuild");
}

#[test]
fn the_rebuilt_conclusion_is_the_exact_tstp_surface_of_the_source() {
    let graph = Graph::of(&turtle(FIXTURE));
    let conclusions: BTreeSet<String> = graph
        .subjects_typed(&math("ProofStep"))
        .iter()
        .map(|step| {
            let statement = graph.object(step, &math("hasConclusion"));
            graph.label(&graph.object(&statement, &math("hasConclusion")))
        })
        .collect();
    assert!(
        conclusions.contains(
            "'https://blackcatinformatics.ca/gmeow/tptp#c'\
                 ('https://blackcatinformatics.ca/logic/entail/reserved#witness-\
                 d4a1e02579180296')"
        ),
        "the rendered conclusion keeps the full IRIs unshortened: {conclusions:?}"
    );
}

// -- the statement-role layer ---------------------------------------------

#[test]
fn every_tptp_role_lands_on_a_declared_statement_role_under_a_theory() {
    for (word, expected_role) in [
        ("axiom", "roleAxiom"),
        ("hypothesis", "roleAxiom"),
        ("assumption", "roleAxiom"),
        ("definition", "roleDefinition"),
        ("type", "roleDefinition"),
        ("fi_domain", "roleDefinition"),
        ("fi_functors", "roleDefinition"),
        ("fi_predicates", "roleDefinition"),
        ("lemma", "roleLemma"),
        ("plain", "roleLemma"),
        ("theorem", "roleTheorem"),
        ("corollary", "roleCorollary"),
        ("conjecture", "roleConjecture"),
        ("negated_conjecture", "roleConjecture"),
    ] {
        let source = format!(
            "cnf(a0, {word}, p(a)).\n\
                 cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n"
        );
        let ttl = turtle(source.as_bytes());
        let graph = Graph::of(&ttl);
        let statement = graph.statement_of("a0");
        assert_eq!(
            graph.label(&statement),
            word,
            "the statement's label IS the raw TPTP role word"
        );
        assert_eq!(
            graph.object(&statement, &math("statementRole")),
            math(expected_role),
            "`{word}` must hold math:{expected_role}"
        );
        let theory = graph.object(&statement, &math("roleInTheory"));
        assert!(
            graph
                .subjects_typed(&math("MathematicalTheory"))
                .contains(&theory),
            "a role is always held IN a theory: `{word}`"
        );
    }
}

#[test]
fn only_a_foundation_role_becomes_a_law_the_proof_depends_on() {
    for (word, is_law) in [
        ("axiom", true),
        ("hypothesis", true),
        ("assumption", true),
        ("negated_conjecture", false),
        ("conjecture", false),
        ("definition", false),
        ("lemma", false),
    ] {
        let source = format!(
            "cnf(a0, {word}, p(a)).\n\
                 cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n"
        );
        let ttl = turtle(source.as_bytes());
        assert_eq!(
            typed(&ttl, "Axiom"),
            usize::from(is_law),
            "`{word}` must{} be lifted as a math:Axiom",
            if is_law { "" } else { " never" }
        );
        let graph = Graph::of(&ttl);
        let proof = graph
            .subjects_typed(&math("Proof"))
            .iter()
            .next()
            .expect("one proof")
            .clone();
        assert_eq!(
            graph.objects(&proof, &math("dependsOnAxiom")).len(),
            usize::from(is_law),
            "the proof depends on `{word}` only when it is a law"
        );
    }
}

#[test]
fn a_negated_conjecture_makes_the_proof_a_refutation() {
    let ttl = turtle(EPROVER_FOF);
    let graph = Graph::of(&ttl);
    let proof = graph
        .subjects_typed(&math("Proof"))
        .iter()
        .next()
        .expect("one proof")
        .clone();
    let method = graph.object(&proof, &math("usesProofMethod"));
    assert!(
        graph.subjects_typed(&math("ProofMethod")).contains(&method),
        "the strategy is a first-class math:ProofMethod"
    );
    let label = graph.label(&method);
    assert!(label.contains("refutation"), "{label}");
    assert!(label.contains("negation of"), "{label}");

    // …and a derivation with no negated conjecture claims no strategy at all.
    let plain = turtle(FIXTURE);
    assert_eq!(
        typed(&plain, "ProofMethod"),
        0,
        "a strategy is claimed only when the derivation shows one"
    );
}

#[test]
fn a_theorem_role_statement_is_never_a_bare_truth_bit() {
    // math:UngroundedTheoremClaim: a math:roleTheorem statement needs a theory context
    // AND either a proof through math:provesStatement or a declared external warrant.
    let ttl = turtle(
        b"cnf(a0, axiom, p(a)).\n\
              cnf(t1, theorem, q(a), inference(r, [status(thm)], [a0])).\n\
              cnf(d2, plain, $false, inference(r, [status(thm)], [t1])).\n",
    );
    let graph = Graph::of(&ttl);
    let statement = graph.labelled("theorem");
    assert_eq!(
        graph.object(&statement, &math("statementRole")),
        math("roleTheorem")
    );
    let _theory = graph.object(&statement, &math("roleInTheory"));
    let warrant = graph.object(&statement, &math("externalWarrant"));
    assert_eq!(
        warrant,
        graph
            .subjects_typed(&math("MathematicalObject"))
            .iter()
            .find(|s| s.contains("proof-src-"))
            .expect("the retained source witness")
            .clone(),
        "a non-terminal theorem is warranted by the document that declares it"
    );
}

#[test]
fn a_file_source_becomes_an_external_warrant_naming_the_reference() {
    let ttl = turtle(EPROVER_FOF);
    let graph = Graph::of(&ttl);
    let warrants: BTreeSet<String> = graph
        .triples
        .iter()
        .filter(|t| t.predicate == math("externalWarrant"))
        .map(|t| graph.label(&t.object))
        .collect();
    assert_eq!(
        warrants,
        BTreeSet::from([
            "file('SYN075+1.p', ax_pq)".to_owned(),
            "file('SYN075+1.p', ax_id)".to_owned(),
            "file('SYN075+1.p', goal)".to_owned(),
            // E cites theory(equality) in the PARENT list of every equality inference
            // (rw / spm / sr here). It warrants those steps without being one, so it
            // lands as a warrant alongside the imported leaves.
            "theory(equality)".to_owned(),
        ]),
        "each imported leaf names the file/name pair it came from, and each equality \
             inference names the theory that licensed it"
    );
    for warrant in graph
        .triples
        .iter()
        .filter(|t| t.predicate == math("externalWarrant"))
        .map(|t| t.object.clone())
    {
        assert!(
            graph
                .subjects_typed(&math("MathematicalObject"))
                .contains(&warrant),
            "an external reference is a first-class object, not a bare string"
        );
    }
    // The negated conjecture came from the file too — and is STILL not a law.
    let negated = graph.statement_of("c_0_2");
    assert_eq!(graph.label(&negated), "negated_conjecture");
    assert!(
        graph
            .objects(&negated, &math("externalWarrant"))
            .iter()
            .any(|w| graph.label(w) == "file('SYN075+1.p', goal)")
    );
    assert_eq!(typed(&ttl, "Axiom"), 2, "only the two axioms are laws");
}

// -- fof conclusions -------------------------------------------------------

#[test]
fn a_quantifier_lifts_into_a_real_binder_over_a_declaration_and_an_occurrence() {
    let ttl = turtle(
        b"fof(a0, axiom, ! [X] : (p(X) => q(X))).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n",
    );
    let graph = Graph::of(&ttl);
    let binder = graph.labelled("! [X] : (p(X) => q(X))");
    assert!(
        graph
            .subjects_typed(&math("BindingExpression"))
            .contains(&binder),
        "a written quantifier is a math:BindingExpression, not an application"
    );
    let declaration = graph.object(&binder, &math("boundVariable"));
    assert!(
        graph
            .subjects_typed(&math("VariableDeclaration"))
            .contains(&declaration),
        "a bound variable resolves to a BOUND declaration, never a free one"
    );
    assert_eq!(graph.label(&declaration), "X");
    assert_eq!(
        typed(&ttl, "FreeVariableDeclaration"),
        0,
        "nothing in this formula is free"
    );
    let occurrence = graph.object(&binder, &math("bindsOccurrence"));
    assert_eq!(
        graph.object(&occurrence, &math("declaredVariable")),
        declaration
    );
    assert_eq!(graph.object(&occurrence, &math("occursInScope")), binder);
    // The binder's body is its one contiguous slot.
    let slots = graph.objects(&binder, &math("argumentSlot"));
    assert_eq!(slots.len(), 1);
    assert_eq!(graph.object(&slots[0], &math("slotIndex")), "0");
    let body = graph.object(&slots[0], &math("slotExpression"));
    assert_eq!(graph.label(&body), "(p(X) => q(X))");
}

#[test]
fn a_variable_list_nests_one_binder_per_variable() {
    let ttl = turtle(
        b"fof(a0, axiom, ! [X, Y] : p(X, Y)).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n",
    );
    assert_eq!(
        typed(&ttl, "BindingExpression"),
        2,
        "math:BindingExpression names at most one bound variable, so a list nests"
    );
    let graph = Graph::of(&ttl);
    let outer = graph.labelled("! [X, Y] : p(X, Y)");
    let inner = graph.labelled("! [Y] : p(X, Y)");
    assert_ne!(outer, inner);
    assert_ne!(
        graph.object(&outer, &math("boundVariable")),
        graph.object(&inner, &math("boundVariable")),
        "each binder introduces its own declaration"
    );
}

#[test]
fn two_binders_reusing_one_glyph_introduce_two_distinct_declarations() {
    // math:VariableDeclaration's own definition: "Two binders that reuse the glyph i
    // introduce two distinct declarations." Content addressing must not collapse them.
    let ttl = turtle(
        b"fof(a0, axiom, (! [X] : p(X) & ! [X] : q(X))).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n",
    );
    assert_eq!(typed(&ttl, "BindingExpression"), 2);
    assert_eq!(typed(&ttl, "VariableDeclaration"), 2);
    let graph = Graph::of(&ttl);
    for occurrence in graph.subjects_typed(&math("VariableOccurrence")) {
        assert_eq!(
            graph.objects(&occurrence, &math("declaredVariable")).len(),
            1,
            "an occurrence resolves to exactly one declaration \
                 (math:UnscopedVariableOccurrence otherwise)"
        );
    }
}

#[test]
fn a_shadowed_glyph_resolves_to_the_innermost_binder() {
    let ttl = turtle(
        b"fof(a0, axiom, ! [X] : ? [X] : p(X)).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n",
    );
    let graph = Graph::of(&ttl);
    let inner = graph.labelled("? [X] : p(X)");
    let inner_occurrence = graph.object(&inner, &math("bindsOccurrence"));
    // `p(X)` is inside the existential, so its variable leaf must name THAT occurrence.
    let leaf = graph
        .subjects_typed(&math("VariableExpression"))
        .into_iter()
        .find(|v| graph.objects(v, &math("variableOccurrence")) == vec![inner_occurrence.clone()])
        .expect("the bound leaf resolves to the innermost binder");
    assert_eq!(graph.label(&leaf), "X");
}

#[test]
fn every_binary_connective_gets_its_own_named_operation() {
    let ttl = turtle(
        b"fof(a0, axiom, ((p <=> q) <~> (r ~| s))).\n\
              cnf(d1, plain, $false, inference(x, [status(thm)], [a0])).\n",
    );
    for label in [
        "logical equivalence (<=>)",
        "exclusive disjunction (<~>)",
        "joint denial (~|)",
    ] {
        assert!(ttl.contains(label), "missing the operation `{label}`");
    }
}

#[test]
fn equality_is_an_operation_rather_than_a_functor_named_equals() {
    let ttl = turtle(
        b"cnf(a0, axiom, f(a) = b).\n\
              fof(a1, axiom, c != d).\n\
              cnf(d1, plain, $false, inference(x, [status(thm)], [a0, a1])).\n",
    );
    assert!(ttl.contains("equality (=)"));
    let graph = Graph::of(&ttl);
    let equation = graph.labelled("f(a) = b");
    assert_eq!(graph.objects(&equation, &math("argumentSlot")).len(), 2);
    // A disequation is a negation OVER the equation, never a second equality operator.
    let disequation = graph.labelled("c != d");
    let operation = graph.object(&disequation, &math("operator"));
    assert_eq!(graph.label(&operation), "logical negation (~)");
}

#[test]
fn a_fof_conclusion_is_never_coerced_into_a_clause() {
    // `! [X] : (p(X) | q(X))` must NOT be read as the two-literal clause `p(X) | q(X)`.
    let ttl = turtle(
        b"fof(a0, axiom, ! [X] : (p(X) | q(X))).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n",
    );
    let graph = Graph::of(&ttl);
    let statement = graph.labelled("axiom");
    let conclusion = graph.object(&statement, &math("hasConclusion"));
    assert!(
        graph
            .subjects_typed(&math("BindingExpression"))
            .contains(&conclusion),
        "the conclusion is the BINDER, not the disjunction beneath it"
    );
}

// -- the source forms ------------------------------------------------------

#[test]
fn a_bare_dag_source_is_a_step_with_a_premise_and_no_inference_rule() {
    let ttl = turtle(
        b"cnf(a0, axiom, p(a)).\n\
              cnf(c1, plain, p(a), a0).\n\
              cnf(d2, plain, $false, inference(r, [status(thm)], [c1])).\n",
    );
    let graph = Graph::of(&ttl);
    let step = graph.labelled("c1");
    assert!(
        graph.objects(&step, &math("usesInferenceRule")).is_empty(),
        "a bare DAG source declares no rule, so none is invented"
    );
    assert_eq!(graph.objects(&step, &math("hasPremise")).len(), 1);
    assert_eq!(
        reconstruct(&ttl).len(),
        3,
        "and the step still rebuilds from the graph"
    );
}

#[test]
fn a_theory_and_an_introduced_source_are_external_references_too() {
    let ttl = turtle(
        b"cnf(a0, axiom, p(a), theory(equality)).\n\
              cnf(a1, definition, q(a), introduced(definition)).\n\
              cnf(d2, plain, $false, inference(r, [status(thm)], [a0, a1])).\n",
    );
    let graph = Graph::of(&ttl);
    let warrants: BTreeSet<String> = graph
        .triples
        .iter()
        .filter(|t| t.predicate == math("externalWarrant"))
        .map(|t| graph.label(&t.object))
        .collect();
    assert_eq!(
        warrants,
        BTreeSet::from([
            "theory(equality)".to_owned(),
            "introduced(definition)".to_owned()
        ])
    );
}

// -- the rung and its residue ---------------------------------------------

#[test]
fn a_derivation_with_nothing_to_declare_travels_at_the_section_retraction_rung() {
    let ttl = turtle(EPROVER_FOF);
    for required in [
        math("ProofIngestRun"),
        math("parseSource"),
        logic("instantiatesSchema"),
        logic("instantiatesPlan"),
        math("ingestCorrespondence"),
        logic("SectionRetraction"),
        logic("ExactPreservation"),
        logic("Equiv"),
        logic("Crisp"),
        logic("mnemomorphic"),
    ] {
        assert!(ttl.contains(&required), "the frame is missing `{required}`");
    }
    assert!(
        !ttl.contains(&math("unmappedConstruct")),
        "an exact lift enumerates no residue"
    );
    assert!(!ttl.contains(&logic("LossyLens")));
}

#[test]
fn a_non_thm_status_is_enumerated_as_residue_and_downgrades_the_rung() {
    let ttl = turtle(EPROVER_CLAUSIFY);
    assert!(
        ttl.contains(&logic("LossyLens")),
        "a run that cannot carry a stated fact must not claim SectionRetraction"
    );
    assert!(!ttl.contains(&logic("SectionRetraction")));
    assert!(ttl.contains("status(cth)"), "the cth token is enumerated");
    assert!(ttl.contains("status(esa)"), "the esa token is enumerated");
    assert!(
        ttl.contains("<useful_info> term `proof`"),
        "the 5th field is enumerated rather than dropped"
    );
    assert_eq!(
        count(&ttl, &math("unmappedConstruct")),
        3,
        "one residue row per stated fact the codomain cannot carry"
    );
    // …and the derivation still LIFTS: residue is not refusal.
    assert!(typed(&ttl, "ProofStep") > 0);
}

#[test]
fn the_unknown_role_is_residue_rather_than_an_invented_epistemic_status() {
    let ttl = turtle(
        b"cnf(a0, unknown, p(a)).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n",
    );
    assert!(ttl.contains(&logic("LossyLens")));
    assert!(ttl.contains("the formula role `unknown` on step `a0`"));
    let graph = Graph::of(&ttl);
    // One statement (for `d1`), never one for the role-less step.
    assert_eq!(
        graph.subjects_typed(&math("MathematicalStatement")).len(),
        1
    );
    assert_eq!(typed(&ttl, "Axiom"), 0, "`unknown` is not a law either");
}

#[test]
fn a_derivation_whose_every_inference_declares_thm_is_reported_verified() {
    let graph = Graph::of(&turtle(EPROVER_FOF));
    let result = graph
        .subjects_typed(&math("FormalVerificationResult"))
        .iter()
        .next()
        .expect("one result")
        .clone();
    assert_eq!(
        graph.object(&result, &math("verificationResult")),
        math("verificationPassed")
    );
}

#[test]
fn an_undeclared_or_non_thm_status_yields_unknown_rather_than_a_bare_failure() {
    // math:verificationUnknown: "never collapsed into a bare failure, because 'not
    // proved' is not 'refuted'". Vampire's empty status lists are exactly that case.
    for source in [VAMPIRE, EPROVER_CLAUSIFY] {
        let ttl = turtle(source);
        let graph = Graph::of(&ttl);
        let result = graph
            .subjects_typed(&math("FormalVerificationResult"))
            .iter()
            .next()
            .expect("one result")
            .clone();
        assert_eq!(
            graph.object(&result, &math("verificationResult")),
            math("verificationUnknown")
        );
        assert_eq!(
            typed(&ttl, "verificationFailed"),
            0,
            "not proved is not refuted"
        );
    }
}

// -- the shape bridges.ttl pins -------------------------------------------

#[test]
fn the_committed_fixture_lifts_every_expected_codomain_class() {
    let lifted = lift(FIXTURE, BASE).expect("the flagship fixture lifts");
    let ttl = &lifted.turtle;
    for (class, expected) in [
        ("ProofIngestRun", 1),
        ("ProofDependencyGraph", 1),
        ("Proof", 1),
        ("ProofStep", 2),
        ("Axiom", 1),
        ("MathematicalStatement", 3),
        ("MathematicalTheory", 1),
        ("FormalVerificationResult", 1),
        ("MathematicalObject", 1),
    ] {
        assert_eq!(typed(ttl, class), expected, "math:{class} count:\n{ttl}");
    }
    for class in [
        "ApplicationExpression",
        "SymbolReference",
        "MathematicalSymbol",
        "Operation",
        "ArgumentSlot",
    ] {
        assert!(
            typed(ttl, class) > 0,
            "the fixture must produce math:{class}"
        );
    }
    for (class, expected) in [("GoalExpression", 2), ("Situation", 2)] {
        assert_eq!(
            typed_as(ttl, &logic(class)),
            expected,
            "one sub-goal per derived step: logic:{class}"
        );
    }
    assert_eq!(typed_as(ttl, &gmeow("Observation")), 1);
    assert_eq!(typed_as(ttl, &gmeow("Standpoint")), 1);
    assert!(lifted.run_iri.contains("proof-run-"));
    assert!(lifted.codomain_nodes > 15, "a real derivation is dense");
}

#[test]
fn the_proof_decomposes_into_its_steps_its_axioms_and_its_goal() {
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let proof = graph
        .subjects_typed(&math("Proof"))
        .iter()
        .next()
        .expect("one math:Proof")
        .clone();

    assert_eq!(
        graph.objects(&proof, &math("proofStep")).len(),
        2,
        "one math:proofStep per DERIVED step, and never one per axiom"
    );
    assert_eq!(
        graph.objects(&proof, &math("dependsOnAxiom")).len(),
        1,
        "the asserted leaf is a foundation the proof depends on"
    );
    let goal = graph.object(&proof, &math("provesGoal"));
    assert_eq!(
        graph.object(&goal, &logic("goalExpressionKind")),
        logic("AchievementGoal"),
        "the kind is a VALUE on the property, never a class the goal is typed with"
    );
    let situation = graph.object(&goal, &logic("boundSituationType"));
    assert!(
        graph
            .subjects_typed(&logic("Situation"))
            .contains(&situation),
        "the goal's bound situation type is a logic:Situation"
    );
    // The proof also names what it establishes: the terminal step's STATEMENT.
    let statement = graph.object(&proof, &math("provesStatement"));
    assert!(
        graph
            .subjects_typed(&math("MathematicalStatement"))
            .contains(&statement),
        "math:provesStatement names the role-bearing statement"
    );
    assert!(
        graph
            .label(&graph.object(&statement, &math("hasConclusion")))
            .contains("tptp#c"),
        "and that statement draws the terminal conclusion"
    );
}

#[test]
fn each_step_carries_its_premises_and_the_axioms_beneath_it() {
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let axiom = graph
        .subjects_typed(&math("Axiom"))
        .iter()
        .next()
        .expect("one axiom")
        .clone();
    let citing: Vec<&Triple> = graph
        .triples
        .iter()
        .filter(|t| t.predicate == math("dependsOnAxiom") && t.object == axiom)
        .collect();
    assert_eq!(
        citing.len(),
        3,
        "the axiom is a foundation of the step that cites it, of the proof, and of its \
             own statement: {citing:?}"
    );
    assert_eq!(
        count(&ttl, &math("hasPremise")),
        2,
        "one premise edge per cited parent"
    );
}

#[test]
fn the_qed_is_a_result_object_held_by_an_observation_from_a_named_vantage() {
    // math:FormalVerificationResult carries
    // gmeow:enforcesFailureClass math:UngroundedVerificationResult, so the grounding
    // observation is mandatory rather than decorative.
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let result = graph
        .subjects_typed(&math("FormalVerificationResult"))
        .iter()
        .next()
        .expect("one result object")
        .clone();
    let observation = graph
        .subjects_typed(&gmeow("Observation"))
        .iter()
        .next()
        .expect("one observation")
        .clone();
    let proof = graph
        .subjects_typed(&math("Proof"))
        .iter()
        .next()
        .expect("one proof")
        .clone();

    assert_eq!(
        graph.object(&observation, &gmeow("observationResult")),
        result
    );
    assert_eq!(graph.object(&observation, &gmeow("observedFeature")), proof);
    let vantage = graph.object(&observation, &gmeow("vantage"));
    assert!(
        graph
            .subjects_typed(&gmeow("Standpoint"))
            .contains(&vantage),
        "the vantage is a gmeow:Standpoint"
    );
    assert_eq!(
        graph.object(&result, &math("verifiedByEngine")),
        vantage,
        "'verified' answers BY WHOM: the engine on the result IS the observation's vantage"
    );
    assert_eq!(
        graph.object(&result, &math("verificationResult")),
        math("verificationPassed")
    );
    // The three roles stay distinct: no node is typed as two of process / result /
    // claim (math:ResultRoleConflation, math:ProcessObservationConflation).
    assert_ne!(result, observation);
    assert!(
        !graph
            .subjects_typed(&gmeow("Observation"))
            .contains(&result),
        "a result object roled as its own claim is ill-formed"
    );
    assert_eq!(
        typed(&ttl, "ProofCheckActivity"),
        0,
        "this lift ran no proof-assistant check, so it claims no math:ProofCheckActivity"
    );
}

#[test]
fn the_verification_claim_says_exactly_what_the_checker_did_not_do() {
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let standpoint = graph
        .subjects_typed(&gmeow("Standpoint"))
        .iter()
        .next()
        .expect("a standpoint")
        .clone();
    let label = graph.label(&standpoint);
    assert!(label.contains("well-founded"), "{label}");
    assert!(label.contains("status(thm)"), "{label}");
    assert!(
        label.contains("does NOT re-derive the inferences"),
        "the vantage must not imply a soundness check it never ran: {label}"
    );
}

#[test]
fn the_dependency_graph_is_emitted_with_exactly_the_shape_the_fixture_pins() {
    // math:ProofDependencyGraph's definition says it "names the math:Proof it
    // underlies"; math:dependencyGraphOf is that edge. The DAG carries it plus its
    // type, label, and generation back-edge — and nothing else, so a future addition
    // is a deliberate change to this list rather than a silent one.
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let dag = graph
        .subjects_typed(&math("ProofDependencyGraph"))
        .iter()
        .next()
        .expect("one DAG")
        .clone();
    let predicates: BTreeSet<String> = graph
        .triples
        .iter()
        .filter(|t| t.subject == dag)
        .map(|t| t.predicate.clone())
        .collect();
    assert_eq!(
        predicates,
        BTreeSet::from([
            crate::ns::RDF_TYPE.to_owned(),
            LABEL.to_owned(),
            gmeow("wasGeneratedBy"),
            math("dependencyGraphOf"),
        ])
    );
    // …and it points at the proof this run actually produced, not merely at some proof.
    let proof = graph
        .subjects_typed(&math("Proof"))
        .iter()
        .next()
        .expect("one proof")
        .clone();
    assert!(
        graph
            .objects(&dag, &math("dependencyGraphOf"))
            .contains(&proof),
        "the DAG must name the proof it underlies"
    );
    assert!(graph.label(&dag).contains("3 steps, 2 inferences"));
}

// -- the competency questions ---------------------------------------------

#[test]
fn the_graph_answers_the_proof_dependency_and_subgoal_competency_question() {
    // The BGP of `slices/grounding/math/queries/competency/
    // proof-dependency-graph-and-subgoals.rq`, walked by hand: ?run a
    // math:ProofIngestRun; ?dag wasGeneratedBy ?run, a math:ProofDependencyGraph;
    // ?proof wasGeneratedBy ?run, a math:Proof, math:provesGoal ?goal, math:proofStep
    // ?step.
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let mut rows = 0;
    for run in graph.subjects_typed(&math("ProofIngestRun")) {
        for dag in graph.subjects_typed(&math("ProofDependencyGraph")) {
            if !graph.objects(&dag, &gmeow("wasGeneratedBy")).contains(&run) {
                continue;
            }
            for proof in graph.subjects_typed(&math("Proof")) {
                if !graph
                    .objects(&proof, &gmeow("wasGeneratedBy"))
                    .contains(&run)
                {
                    continue;
                }
                for _goal in graph.objects(&proof, &math("provesGoal")) {
                    for _step in graph.objects(&proof, &math("proofStep")) {
                        rows += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        rows, 2,
        "one row per (goal, step) pair — the join must exist"
    );
}

#[test]
fn the_graph_answers_the_bridge_results_as_observations_competency_question() {
    // The BGP of `bridge-results-as-observations.rq`: ?obs a gmeow:Observation,
    // gmeow:observationResult ?result, gmeow:vantage ?vantage, gmeow:wasGeneratedBy
    // ?activity; ?activity a math:ProofIngestRun.
    let ttl = turtle(FIXTURE);
    let graph = Graph::of(&ttl);
    let runs = graph.subjects_typed(&math("ProofIngestRun"));
    let mut rows = 0;
    for observation in graph.subjects_typed(&gmeow("Observation")) {
        let _result = graph.object(&observation, &gmeow("observationResult"));
        let _vantage = graph.object(&observation, &gmeow("vantage"));
        for activity in graph.objects(&observation, &gmeow("wasGeneratedBy")) {
            if runs.contains(&activity) {
                rows += 1;
            }
        }
    }
    assert_eq!(rows, 1, "the held claim joins back to the bridge run");
}

// -- the doctrines ---------------------------------------------------------

#[test]
fn a_relift_of_the_same_derivation_is_byte_identical() {
    for source in [FIXTURE, EPROVER_FOF, EPROVER_CLAUSIFY] {
        assert_eq!(
            turtle(source),
            turtle(source),
            "the lift is idempotent: no clock, no counter"
        );
    }
}

#[test]
fn the_lift_is_independent_of_the_order_the_steps_are_written_in() {
    // Source order is not dependency order; the lift walks the DAG, so writing the
    // conclusion first must produce the same codomain (a different run IRI, since the
    // run is content-addressed on the SOURCE BYTES, but the same reconstruction).
    let forward = b"cnf(a0, axiom, p(a)).\n\
                        cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n";
    let backward = b"cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
                         cnf(a0, axiom, p(a)).\n";
    assert_eq!(
        reconstruct(&turtle(forward)),
        reconstruct(&turtle(backward)),
        "the lifted derivation is the DAG, not the file layout"
    );
}

#[test]
fn a_different_derivation_mints_a_different_run() {
    let a = lift(FIXTURE, BASE).expect("lifts");
    let b = lift(RICH, BASE).expect("lifts");
    assert_ne!(a.run_iri, b.run_iri);
}

#[test]
fn every_codomain_node_carries_the_back_edge_the_native_lint_reads() {
    for source in [RICH, EPROVER_FOF, VAMPIRE, EPROVER_CLAUSIFY] {
        let lifted = lift(source, BASE).expect("lifts");
        assert_eq!(
            count(&lifted.turtle, &gmeow("wasGeneratedBy")),
            lifted.codomain_nodes,
            "exactly one gmeow:wasGeneratedBy per generated node"
        );
    }
}

#[test]
fn a_lifted_graph_carries_no_private_use_language_tag() {
    for source in [FIXTURE, RICH, EPROVER_FOF, VAMPIRE, EPROVER_CLAUSIFY] {
        assert!(
            !turtle(source).contains("x-gmeow-"),
            "consumer output must not leak a private-use tag"
        );
    }
}

// -- content addressing ----------------------------------------------------

#[test]
fn a_repeated_conclusion_collapses_to_one_content_addressed_expression() {
    // Two steps concluding the SAME clause are one conclusion, reached twice.
    let source = b"cnf(a0, axiom, p(a)).\n\
                       cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
                       cnf(d2, plain, q(a), inference(s, [status(thm)], [d1])).\n";
    let ttl = turtle(source);
    let graph = Graph::of(&ttl);
    let conclusions: BTreeSet<String> = graph
        .subjects_typed(&math("ProofStep"))
        .iter()
        .map(|step| {
            let statement = graph.object(step, &math("hasConclusion"));
            graph.object(&statement, &math("hasConclusion"))
        })
        .collect();
    assert_eq!(
        conclusions.len(),
        1,
        "`q(a)` is ONE expression however many steps conclude it:\n{ttl}"
    );
    // …and so is the goal, whose identity logic:GoalExpression declares to be
    // structural: same kind, same operands, same bound situation type.
    assert_eq!(typed_as(&ttl, &logic("GoalExpression")), 1);
    assert_eq!(typed(&ttl, "ProofStep"), 2, "but the STEPS stay distinct");
}

#[test]
fn a_repeated_sub_term_is_one_node_and_distinct_structure_grows_the_graph() {
    let shared = lift(
        b"cnf(a0, axiom, p(f(a), f(a))).\n\
              cnf(d1, plain, q(f(a)), inference(r, [status(thm)], [a0])).\n",
        BASE,
    )
    .expect("lifts");
    let distinct = lift(
        b"cnf(a0, axiom, p(f(a), f(b))).\n\
              cnf(d1, plain, q(f(c)), inference(r, [status(thm)], [a0])).\n",
        BASE,
    )
    .expect("lifts");
    assert!(
        distinct.codomain_nodes > shared.codomain_nodes,
        "the fact count grows with DISTINCT structure, not with textual repetition"
    );
    let graph = Graph::of(&shared.turtle);
    assert_eq!(
        graph
            .triples
            .iter()
            .filter(|t| t.predicate == LABEL && t.object == "f(a)")
            .count(),
        1,
        "the repeated `f(a)` is one interned expression:\n{}",
        shared.turtle
    );
}

#[test]
fn alpha_equivalent_quantified_formulas_share_one_binder_term() {
    // Bound variables intern at their de-Bruijn distance, so renaming the glyph does
    // not mint a second term.
    let ttl = turtle(
        b"fof(a0, axiom, ! [X] : p(X)).\n\
              fof(a1, axiom, ! [Y] : p(Y)).\n\
              cnf(d1, plain, $false, inference(r, [status(thm)], [a0, a1])).\n",
    );
    assert_eq!(
        typed(&ttl, "BindingExpression"),
        1,
        "`! [X] : p(X)` and `! [Y] : p(Y)` are one term:\n{ttl}"
    );
}

#[test]
fn an_identical_sub_derivation_collapses_to_one_proof_term() {
    // Two steps reached by the same rule over the same sub-proof ARE the same proof
    // term, even though they remain two named steps.
    let source = b"cnf(a0, axiom, p(a)).\n\
                       cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
                       cnf(d2, plain, s(a), inference(r, [status(thm)], [a0])).\n\
                       cnf(d3, plain, t(a), inference(u, [status(thm)], [d1, d2])).\n";
    let ttl = turtle(source);
    let graph = Graph::of(&ttl);
    let terms: BTreeSet<String> = ["d1", "d2"]
        .iter()
        .map(|name| graph.object(&graph.labelled(name), &math("formalizesExpression")))
        .collect();
    assert_eq!(
        terms.len(),
        1,
        "`r(p(a))` is one proof term however many steps it justifies:\n{ttl}"
    );
    // The steps themselves do NOT collapse: a step's identity is its NAME.
    assert_eq!(typed(&ttl, "ProofStep"), 3);
    assert_eq!(reconstruct(&ttl).len(), 4, "and all four steps rebuild");
}

#[test]
fn a_proof_terms_operands_are_ordered_contiguous_zero_based_slots() {
    let ttl = turtle(RICH);
    let graph = Graph::of(&ttl);
    let top = graph.labelled("d_top");
    let term = graph.object(&top, &math("formalizesExpression"));
    let mut indexes: Vec<String> = graph
        .objects(&term, &math("argumentSlot"))
        .iter()
        .map(|slot| graph.object(slot, &math("slotIndex")))
        .collect();
    indexes.sort();
    assert_eq!(
        indexes,
        vec!["0".to_owned(), "1".to_owned()],
        "TSTP's operand ORDER survives as contiguous zero-based slots"
    );
    assert_eq!(
        graph.object(&term, &math("denotationKind")),
        math("denotesProof"),
        "a proof term declares the denotation kind the slice mints for exactly this case"
    );
}

#[test]
fn a_clause_lifts_into_a_typed_expression_ast_not_a_string() {
    let ttl = turtle(RICH);
    let graph = Graph::of(&ttl);
    // `~r(f(a)) | s(b)` is a disjunction of a negation and an atom, all structured.
    let disjunction = graph.labelled("~r(f(a)) | s(b)");
    assert_eq!(graph.objects(&disjunction, &math("argumentSlot")).len(), 2);
    let operation = graph.object(&disjunction, &math("operator"));
    assert_eq!(graph.label(&operation), "logical disjunction (|)");
    assert!(
        ttl.contains("logical negation (~)"),
        "the negated literal is an application of negation, not a prefixed string"
    );
    // A clause variable resolves to a FREE declaration: a clause's universal closure is
    // implicit, so no binder is invented, but there is no implicit free variable either.
    assert_eq!(typed(&ttl, "VariableExpression"), 1);
    assert_eq!(typed(&ttl, "VariableOccurrence"), 1);
    assert_eq!(typed(&ttl, "FreeVariableDeclaration"), 1);
    assert_eq!(typed(&ttl, "BindingExpression"), 0);
    // The empty clause rides as the defined atom, resolved through one symbol.
    assert_eq!(
        graph
            .triples
            .iter()
            .filter(|t| t.predicate == LABEL && t.object == "$false")
            .count(),
        2,
        "the `$false` symbol and the reference that resolves to it"
    );
}

#[test]
fn an_inference_rule_and_a_predicate_that_share_a_spelling_are_two_operations() {
    let ttl = turtle(
        b"cnf(a0, axiom, r(a)).\n\
              cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n",
    );
    let graph = Graph::of(&ttl);
    let named_r: BTreeSet<String> = graph
        .triples
        .iter()
        .filter(|t| t.predicate == LABEL && t.object == "r")
        .map(|t| t.subject.clone())
        .collect();
    assert_eq!(
        named_r.len(),
        2,
        "the inference rule `r` and the predicate `r` are different operators:\n{ttl}"
    );
}

// -- hard failures ---------------------------------------------------------

#[test]
fn a_malformed_derivation_is_a_typed_parse_failure_with_a_position() {
    let err = lift(b"cnf(a0, axiom, p(a)\n", BASE).expect_err("malformed TSTP must not lift");
    assert!(
        err.is::<TstpParse>(),
        "expected math.lift.proof.parse: {err}"
    );
    assert!(format!("{err}").contains("line "), "{err}");
}

#[test]
fn a_dangling_parent_is_a_typed_unliftable_failure() {
    let err = lift(
        b"cnf(a0, axiom, p(a)).\n\
              cnf(d1, plain, q(a), inference(r, [status(thm)], [ghost])).\n",
        BASE,
    )
    .expect_err("a dangling parent must not lift");
    assert!(
        err.is::<ProofUnliftable>(),
        "expected math.lift.proof.unliftable: {err}"
    );
    assert!(format!("{err}").contains("`ghost`"), "{err}");
}

#[test]
fn a_cyclic_dependency_graph_is_a_typed_unliftable_failure() {
    let err = lift(
        b"cnf(d1, plain, p(a), inference(r, [status(thm)], [d2])).\n\
              cnf(d2, plain, q(a), inference(r, [status(thm)], [d1])).\n",
        BASE,
    )
    .expect_err("a cycle must not lift");
    assert!(
        err.is::<ProofUnliftable>(),
        "expected math.lift.proof.unliftable: {err}"
    );
    assert!(format!("{err}").contains("cycle"), "{err}");
}

#[test]
fn an_out_of_fragment_construct_never_reaches_the_sink() {
    // The whole-or-nothing rule: an unliftable derivation produces NO triples at all.
    for source in [
        &b"tff(a0, type, a: $i).\n"[..],
        &b"include('Axioms/SET001-0.ax').\n"[..],
        &b"cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), mystery(problem)).\n"[..],
        &b"cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), [file('p', a), theory(equality)]).\n"[..],
    ] {
        assert!(
            lift(source, BASE).is_err(),
            "an unstructured construct must hard-fail rather than lift partially"
        );
    }
}

#[test]
fn the_committed_fixture_is_the_reasoners_own_derivation() {
    // The fixture is a PRODUCT of `gmeow_logic::proof_tree::ProofTree::to_tstp`,
    // byte-pinned by `gmeow_conformance::external::tptp::lower_fol`'s
    // `the_committed_tstp_fixture_is_exactly_what_our_reasoner_produces`. Both crates
    // include_ the same file, so a drift on either side is caught; this end asserts the
    // shape that pin guarantees.
    let text = std::str::from_utf8(FIXTURE).expect("utf-8");
    assert!(
        text.contains("produced by OUR OWN reasoner"),
        "the fixture's provenance header"
    );
    let derivation = tstp::parse(FIXTURE).expect("parses");
    assert_eq!(derivation.steps().len(), 3);
    assert_eq!(derivation.steps()[0].role, Role::Axiom);
    assert!(
        derivation
            .conclusion()
            .rule()
            .expect("a rule")
            .starts_with("https://blackcatinformatics.ca/gmeow/goal-directed/rule/"),
        "the rule names the content-addressed ground-instance firing"
    );
}
