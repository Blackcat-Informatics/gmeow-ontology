// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compositional lowering: a quantified subject–verb–object [`Form::Composed`] sentence is
//! lowered — one declared stage at a time — into a full first-order [`Formula`] the native
//! `logic:` reasoner can consume.
//!
//! The lowering is compositional in the Montague sense: the meaning of the whole is a
//! function of the meanings of the constituents and the way they combine. A determiner is a
//! generalized quantifier (`every`/`all`/`each` → `∀ … →`, `a`/`an`/`some` → `∃ … ∧`), a
//! common noun and a transitive verb are one- and two-place predicates, and surface linear
//! order fixes the quantifier scope (subject wide, object narrow). "Every cat chases a mouse"
//! therefore lowers to
//! `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))`.
//!
//! Two invariants make this honest rather than a black box:
//!
//! * **Total, hard-failing coverage of the fragment.** A sentence outside the modeled
//!   quantified-SVO class is NEVER silently coerced into a plausible-but-wrong formula; it
//!   raises a [`LoweringError`] naming the exact construct (the `lang:SilentIngestDrop` floor,
//!   applied to lowering).
//! * **Per-stage preservation records.** Every lowering step — constituent identification,
//!   quantifier scoping, predicate–argument binding — is recorded as a [`LoweringStage`]
//!   carrying a `logic:preservationKind`, so no lowering step is undeclared (the
//!   `lang:UndeclaredLoweringStage` floor). For the modeled fragment the compositional
//!   formula captures the sentence's truth conditions exactly, so each stage is
//!   [`PreservationKind::Exact`]; a lowering that had to approximate would record the honest
//!   weaker kind instead.
//!
//! A second, dogfooding lowering — [`grammar_rule_to_derivation`] — turns a context-free
//! grammar rule `S → NP VP` into the span-indexed derivation rule
//! `S(i, k) :- NP(i, j), VP(j, k)`, encoded in the same logic-compile IR. This makes "a parse
//! is a derivation" a first-class, executable-as-logic statement: the grammar's licensing
//! relation IS a datalog chart, not a separate hand-rolled parser.

use gmeow_lang_form::{Form, MorphFeature, Slot};
use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE, PreservationKind, Term};

use crate::emit::{digest16, ntriples_sorted};
use crate::grammar::{Formalism, Grammar, GrammarRule, RuleExpr, canonicalize_expr};

/// The `lang:` namespace base, byte-identical to the other `lang:` producers.
use gmeow_ns::LANG_NS;

/// The base a modeled common-noun / transitive-verb predicate IRI is minted under: a
/// documented `lang:`-adjacent namespace, joined to the constituent's lemma. `cat` becomes
/// `<https://blackcatinformatics.ca/lang/predicate/cat>`. Keeping the predicate a pure
/// function of the lemma (not a fresh blank/skolem) is what makes the lowering compositional —
/// the same lexeme lowers to the same predicate everywhere.
pub const PREDICATE_NS: &str = "https://blackcatinformatics.ca/lang/predicate/";

/// The base a grammar nonterminal's span relation IRI is minted under (the `S`, `NP`, `VP`
/// chart predicates of [`grammar_rule_to_derivation`]).
pub const NONTERMINAL_NS: &str = "https://blackcatinformatics.ca/lang/nonterminal/";

/// The base a grammar terminal's span relation IRI is minted under.
pub const TERMINAL_NS: &str = "https://blackcatinformatics.ca/lang/terminal/";

/// The `rdf:type` predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `rdfs:comment` predicate IRI.
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";

// --------------------------------------------------------------------------- //
// Errors
// --------------------------------------------------------------------------- //

/// A hard failure raised when a sentence falls outside the modeled quantified-SVO fragment,
/// or a constituent cannot be lowered. The offending construct is named exactly — the lowering
/// refuses rather than emitting a plausible-but-wrong formula (the `lang:SilentIngestDrop`
/// floor, applied to lowering).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringError {
    /// The construct outside the modeled fragment, named exactly.
    pub construct: String,
}

impl LoweringError {
    /// A construct outside the modeled fragment.
    fn unmodeled(construct: impl Into<String>) -> Self {
        LoweringError {
            construct: construct.into(),
        }
    }
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "sentence outside the modeled quantified-SVO fragment: {}",
            self.construct
        )
    }
}

impl std::error::Error for LoweringError {}

// --------------------------------------------------------------------------- //
// Per-stage preservation records
// --------------------------------------------------------------------------- //

/// One declared lowering step and the `logic:preservationKind` it discharges. Every step the
/// lowering performs contributes exactly one record, so the resulting [`Lowering`] can never
/// carry an undeclared step (the `lang:UndeclaredLoweringStage` floor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringStage {
    /// The stage's stable name (one of [`REQUIRED_STAGES`]).
    pub name: String,
    /// The `logic:preservationKind` this stage discharges — [`PreservationKind::Exact`] for
    /// the modeled fragment, whose compositional formula captures the sentence's truth
    /// conditions exactly.
    pub preservation: PreservationKind,
    /// A human-readable account of what the stage did, for the ledger row / RDF comment.
    pub note: String,
}

/// The ordered, closed set of stage names a compositional SVO lowering declares. The lowering
/// emits exactly these, in this order — [`Lowering::assert_all_stages_declared`] pins the two
/// in sync so a future refactor that adds a silent step is caught.
pub const REQUIRED_STAGES: [&str; 3] = [
    "constituent-identification",
    "quantifier-scoping",
    "predicate-argument-binding",
];

/// The product of a compositional lowering: the first-order [`Formula`] and the per-stage
/// preservation records that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lowering {
    /// The compositional first-order formula.
    pub formula: Formula,
    /// The per-stage preservation records, in application order.
    pub stages: Vec<LoweringStage>,
}

impl Lowering {
    /// The declared stage names, in application order.
    pub fn stage_names(&self) -> Vec<&str> {
        self.stages.iter().map(|s| s.name.as_str()).collect()
    }

    /// Hard-assert that every required stage is declared, in order — the check that makes
    /// "no lowering step is undeclared" a machine-verified invariant rather than a comment.
    pub fn assert_all_stages_declared(&self) -> Result<(), LoweringError> {
        let got = self.stage_names();
        if got != REQUIRED_STAGES {
            return Err(LoweringError::unmodeled(format!(
                "undeclared lowering stage: expected stages {:?}, recorded {:?}",
                REQUIRED_STAGES, got
            )));
        }
        Ok(())
    }

    /// Emit the lowering as a deterministic (sorted, deduped) N-Triples byte stream rooted at
    /// `subject_iri`: the lowering typed `lang:CompositionalLowering`, carrying its formula
    /// content key, plus one `lang:LoweringStage` per stage — each carrying its
    /// `logic:preservationKind`, its `lang:stageName`, its order, and its comment. Reuses the
    /// shared [`ntriples_sorted`] canonicalizer so the digest and line ordering match every
    /// other `lang:` producer.
    pub fn to_ntriples(&self, subject_iri: &str) -> Vec<u8> {
        let mut lines = vec![
            format!("<{subject_iri}> <{RDF_TYPE}> <{LANG_NS}CompositionalLowering> ."),
            format!(
                "<{subject_iri}> <{LANG_NS}formulaKey> \"{}\" .",
                escape_literal(&self.formula.content_key())
            ),
        ];
        for (order, stage) in self.stages.iter().enumerate() {
            let stage_iri = format!(
                "{subject_iri}/stage/{}",
                digest16("lang-lowering-stage", &stage.name)
            );
            lines.push(format!(
                "<{stage_iri}> <{RDF_TYPE}> <{LANG_NS}LoweringStage> ."
            ));
            lines.push(format!(
                "<{stage_iri}> <{LANG_NS}loweringStageOf> <{subject_iri}> ."
            ));
            lines.push(format!(
                "<{stage_iri}> <{LANG_NS}stageName> \"{}\" .",
                escape_literal(&stage.name)
            ));
            lines.push(format!(
                "<{stage_iri}> <{LANG_NS}stageOrder> \
                 \"{order}\"^^<http://www.w3.org/2001/XMLSchema#integer> ."
            ));
            lines.push(format!(
                "<{stage_iri}> <{LOGIC_NAMESPACE}preservationKind> <{}> .",
                stage.preservation.iri()
            ));
            lines.push(format!(
                "<{stage_iri}> <{RDFS_COMMENT}> \"{}\" .",
                escape_literal(&stage.note)
            ));
        }
        ntriples_sorted(lines)
    }
}

/// Escape a string literal for an N-Triples object (`"..."`): backslash, double-quote, and the
/// line-ending controls, per the N-Triples grammar.
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

// --------------------------------------------------------------------------- //
// Determiner → generalized quantifier
// --------------------------------------------------------------------------- //

/// The two generalized-quantifier readings the fragment models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quantifier {
    /// `every` / `all` / `each`: a restricted universal `∀v(restrictor → scope)`.
    Universal,
    /// `a` / `an` / `some`: a restricted existential `∃v(restrictor ∧ scope)`.
    Existential,
}

/// Classify a determiner lemma into its quantifier reading, or hard-fail naming the
/// unmodeled determiner. Case-insensitive on the lemma.
fn classify_determiner(lemma: &str) -> Result<Quantifier, LoweringError> {
    match lemma.to_ascii_lowercase().as_str() {
        "every" | "all" | "each" => Ok(Quantifier::Universal),
        "a" | "an" | "some" => Ok(Quantifier::Existential),
        other => Err(LoweringError::unmodeled(format!(
            "unmodeled determiner quantifier '{other}' (modeled: every/all/each, a/an/some)"
        ))),
    }
}

// --------------------------------------------------------------------------- //
// Constituent access
// --------------------------------------------------------------------------- //

/// The lemma and part of speech of a lexical form: a [`Form::Lexeme`] directly, or the lexeme a
/// [`Form::WordForm`] inflects (recursing through the inflection). `None` for a non-lexical
/// form (a phrase, morpheme, …), so the caller can hard-fail with the right construct name.
fn lexical_head(form: &Form) -> Option<(&str, Option<&str>)> {
    match form {
        Form::Lexeme {
            lemma,
            part_of_speech,
            ..
        } => Some((lemma.as_str(), part_of_speech.as_deref())),
        Form::WordForm { lexeme, .. } => lexical_head(lexeme),
        _ => None,
    }
}

/// Whether a slot's dependency relation equals `rel`.
fn dep_is(slot: &Slot, rel: &str) -> bool {
    slot.dep_relation.as_deref() == Some(rel)
}

/// Whether a slot's lexical head is tagged with part of speech `pos`.
fn pos_is(slot: &Slot, pos: &str) -> bool {
    lexical_head(&slot.form).and_then(|(_, p)| p) == Some(pos)
}

/// Extract the `(determiner-lemma, noun-lemma)` of a noun phrase constituent, or hard-fail
/// naming the construct. A modeled NP is a [`Form::Composed`] with a determiner slot
/// (`dep=det` or `POS=DET`) and a head-noun slot (`dep=root`/`head` or `POS=NOUN`). A bare
/// noun without a determiner is OUTSIDE the quantified fragment and is refused (not silently
/// read as an implicit existential).
fn noun_phrase_parts(form: &Form, role: &str) -> Result<(String, String), LoweringError> {
    let slots = match form {
        Form::Composed { slots, .. } => slots,
        Form::Lexeme { .. } | Form::WordForm { .. } => {
            return Err(LoweringError::unmodeled(format!(
                "{role} is a bare nominal without a determiner quantifier — outside the \
                 quantified-SVO fragment"
            )));
        }
        _ => {
            return Err(LoweringError::unmodeled(format!(
                "{role} is not a noun-phrase constituent"
            )));
        }
    };
    let det = slots
        .iter()
        .find(|s| dep_is(s, "det") || pos_is(s, "DET"))
        .ok_or_else(|| {
            LoweringError::unmodeled(format!("{role} noun phrase has no determiner quantifier"))
        })?;
    let noun = slots
        .iter()
        .find(|s| pos_is(s, "NOUN") || dep_is(s, "head") || dep_is(s, "root"))
        .ok_or_else(|| LoweringError::unmodeled(format!("{role} noun phrase has no head noun")))?;
    let det_lemma = lexical_head(&det.form)
        .ok_or_else(|| {
            LoweringError::unmodeled(format!("{role} determiner is not a lexical form"))
        })?
        .0
        .to_owned();
    let noun_lemma = lexical_head(&noun.form)
        .ok_or_else(|| LoweringError::unmodeled(format!("{role} head noun is not a lexical form")))?
        .0
        .to_owned();
    Ok((det_lemma, noun_lemma))
}

/// Mint the predicate atom `<PREDICATE_NS+lemma>(args)`.
fn predicate_atom(lemma: &str, args: Vec<Term>) -> Result<Formula, LoweringError> {
    let relation = Term::iri(format!("{PREDICATE_NS}{lemma}"))
        .map_err(|e| LoweringError::unmodeled(e.message()))?;
    Formula::atom(relation, args).map_err(|e| LoweringError::unmodeled(e.message()))
}

// --------------------------------------------------------------------------- //
// The lowering
// --------------------------------------------------------------------------- //

/// Lower a quantified subject–verb–object sentence to its compositional first-order
/// [`Formula`], with a per-stage preservation record for every lowering step.
///
/// The input is a [`Form::Composed`] of the quantified-SVO class: a subject noun phrase whose
/// determiner is a modeled quantifier (`every`/`all`/`each` or `a`/`an`/`some`), a transitive
/// verb head, and an object noun phrase (likewise determiner-headed). "Every cat chases a
/// mouse" lowers to
/// `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))`.
///
/// Anything outside this fragment — a non-composed form, a non-sentence level, a missing
/// subject/verb/object, an unmodeled determiner, a bare (determiner-less) nominal — is a HARD
/// FAILURE ([`LoweringError`]) naming the exact construct. The lowering never emits a
/// plausible-but-wrong formula.
pub fn lower_svo(sentence: &Form) -> Result<Lowering, LoweringError> {
    // ---- Stage 1: constituent identification ------------------------------- //
    let slots = match sentence {
        Form::Composed { level, slots, .. } => {
            if level != "sentence" && level != "clause" {
                return Err(LoweringError::unmodeled(format!(
                    "composed level '{level}' is not a sentence/clause"
                )));
            }
            slots
        }
        _ => {
            return Err(LoweringError::unmodeled(
                "top-level form is not a Composed sentence".to_owned(),
            ));
        }
    };
    let subject_slot = slots
        .iter()
        .find(|s| dep_is(s, "nsubj"))
        .ok_or_else(|| LoweringError::unmodeled("sentence has no nsubj subject constituent"))?;
    let object_slot = slots
        .iter()
        .find(|s| dep_is(s, "obj") || dep_is(s, "dobj"))
        .ok_or_else(|| {
            LoweringError::unmodeled(
                "sentence has no obj/dobj object constituent — an intransitive clause is outside \
                 the transitive-SVO fragment",
            )
        })?;
    let verb_slot = slots
        .iter()
        .find(|s| pos_is(s, "VERB") || dep_is(s, "root"))
        .ok_or_else(|| LoweringError::unmodeled("sentence has no verb (root/VERB) constituent"))?;
    let verb_lemma = lexical_head(&verb_slot.form)
        .ok_or_else(|| LoweringError::unmodeled("verb constituent is not a lexical form"))?
        .0
        .to_owned();

    let (subject_det, subject_noun) = noun_phrase_parts(&subject_slot.form, "subject")?;
    let (object_det, object_noun) = noun_phrase_parts(&object_slot.form, "object")?;

    let stage_constituents = LoweringStage {
        name: REQUIRED_STAGES[0].to_owned(),
        preservation: PreservationKind::Exact,
        note: format!(
            "identified subject NP (det '{subject_det}', noun '{subject_noun}'), transitive verb \
             '{verb_lemma}', object NP (det '{object_det}', noun '{object_noun}') from the \
             constituency + dependency analysis",
        ),
    };

    // ---- Stage 2: quantifier scoping --------------------------------------- //
    let subject_quant = classify_determiner(&subject_det)?;
    let object_quant = classify_determiner(&object_det)?;
    let stage_scoping = LoweringStage {
        name: REQUIRED_STAGES[1].to_owned(),
        preservation: PreservationKind::Exact,
        note: format!(
            "mapped determiners to generalized quantifiers (subject '{subject_det}' → {}, object \
             '{object_det}' → {}); surface linear order fixes the scope (subject wide, object \
             narrow)",
            quantifier_label(subject_quant),
            quantifier_label(object_quant),
        ),
    };

    // ---- Stage 3: predicate–argument binding ------------------------------- //
    let x = Term::var("x").map_err(|e| LoweringError::unmodeled(e.message()))?;
    let y = Term::var("y").map_err(|e| LoweringError::unmodeled(e.message()))?;

    let verb_atom = predicate_atom(&verb_lemma, vec![x.clone(), y.clone()])?;
    let object_restrictor = predicate_atom(&object_noun, vec![y.clone()])?;
    let scope = quantify(object_quant, "y", object_restrictor, verb_atom);

    let subject_restrictor = predicate_atom(&subject_noun, vec![x.clone()])?;
    let formula = quantify(subject_quant, "x", subject_restrictor, scope);

    let stage_binding = LoweringStage {
        name: REQUIRED_STAGES[2].to_owned(),
        preservation: PreservationKind::Exact,
        note: format!(
            "bound the noun/verb lemmas to `{PREDICATE_NS}<lemma>` predicates and the subject/\
             object roles to argument positions (subject → arg 0, object → arg 1 of the \
             transitive verb)",
        ),
    };

    Ok(Lowering {
        formula,
        stages: vec![stage_constituents, stage_scoping, stage_binding],
    })
}

/// The display label of a quantifier, for a stage note.
fn quantifier_label(q: Quantifier) -> &'static str {
    match q {
        Quantifier::Universal => "∀ (restricted universal)",
        Quantifier::Existential => "∃ (restricted existential)",
    }
}

/// Bind `var` under `quant` over a restrictor and a scope: a universal is
/// `∀var(restrictor → scope)`, an existential is `∃var(restrictor ∧ scope)`.
fn quantify(quant: Quantifier, var: &str, restrictor: Formula, scope: Formula) -> Formula {
    match quant {
        Quantifier::Universal => Formula::Forall {
            vars: vec![var.to_owned()],
            body: Box::new(Formula::Implies(Box::new(restrictor), Box::new(scope))),
        },
        Quantifier::Existential => Formula::Exists {
            vars: vec![var.to_owned()],
            body: Box::new(Formula::And(vec![restrictor, scope])),
        },
    }
}

// --------------------------------------------------------------------------- //
// T2 — grammar rule → span-indexed derivation rule (dogfooding)
// --------------------------------------------------------------------------- //

/// A context-free grammar rule lowered to a span-indexed datalog derivation rule: the CFG
/// production `S → NP VP` becomes `S(i, k) :- NP(i, j), VP(j, k)`. The nonterminal `X` is a
/// binary span relation `X(start, end)`; a production of `n` symbols threads `n + 1` span
/// variables `i0 … in`, the head spanning `(i0, in)` and the `k`-th body symbol spanning
/// `(ik, i(k+1))`.
///
/// This is the "a parse is a derivation" structure made first-class: the grammar's licensing
/// relation IS a chart-parsing datalog program, encoded in the same logic-compile IR as every
/// other `logic:` rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivationRule {
    /// The left-hand-side nonterminal name.
    pub nonterminal: String,
    /// The head atom `LHS(i0, in)`.
    pub head: Formula,
    /// The body atoms `RHS_k(ik, i(k+1))`, in left-to-right production order.
    pub body: Vec<Formula>,
}

impl DerivationRule {
    /// The derivation rule as one implication `body → head` — a `logic:Formula` the reasoner
    /// can carry: `(NP(i0,i1) ∧ VP(i1,i2)) → S(i0,i2)`. A single-symbol body is carried as the
    /// bare antecedent (no vacuous `∧`).
    pub fn to_formula(&self) -> Formula {
        let antecedent = match self.body.as_slice() {
            [only] => only.clone(),
            _ => Formula::And(self.body.clone()),
        };
        Formula::Implies(Box::new(antecedent), Box::new(self.head.clone()))
    }
}

/// The span variable `i{n}`.
fn span_var(n: usize) -> Result<Term, LoweringError> {
    Term::var(format!("i{n}")).map_err(|e| LoweringError::unmodeled(e.message()))
}

/// The atom `<NONTERMINAL_NS+name>(i{start}, i{end})`.
fn nonterminal_atom(name: &str, start: usize, end: usize) -> Result<Formula, LoweringError> {
    let relation = Term::iri(format!("{NONTERMINAL_NS}{name}"))
        .map_err(|e| LoweringError::unmodeled(e.message()))?;
    Formula::atom(relation, vec![span_var(start)?, span_var(end)?])
        .map_err(|e| LoweringError::unmodeled(e.message()))
}

/// The atom `<TERMINAL_NS+digest>(i{start}, i{end})` for a terminal token — content-addressed
/// on the terminal text so distinct terminals get distinct span relations.
fn terminal_atom(text: &str, start: usize, end: usize) -> Result<Formula, LoweringError> {
    let relation = Term::iri(format!("{TERMINAL_NS}{}", digest16("lang-terminal", text)))
        .map_err(|e| LoweringError::unmodeled(e.message()))?;
    Formula::atom(relation, vec![span_var(start)?, span_var(end)?])
        .map_err(|e| LoweringError::unmodeled(e.message()))
}

/// The span atom for one right-hand-side symbol occupying `(ik, i(k+1))`. A [`RuleExpr::Ref`]
/// is a nonterminal span relation; a [`RuleExpr::Terminal`] is a terminal span relation. Any
/// other expression is not a plain grammar symbol and is treated as a single opaque terminal
/// spanning the position (a faithful "this sub-expression spans one cell" atom — never a silent
/// drop); the modeled SVO grammar contains only `Ref`s, so this fallback does not arise there.
fn symbol_atom(item: &RuleExpr, start: usize, end: usize) -> Result<Formula, LoweringError> {
    match item {
        RuleExpr::Ref(name) => nonterminal_atom(name, start, end),
        RuleExpr::Terminal(text) => terminal_atom(text, start, end),
        other => {
            let mut serialized = String::new();
            serialize_symbol(other, &mut serialized);
            terminal_atom(&serialized, start, end)
        }
    }
}

/// A minimal deterministic serialization of a non-symbol RHS expression, for content-addressing
/// the opaque-symbol fallback in [`symbol_atom`].
fn serialize_symbol(e: &RuleExpr, out: &mut String) {
    match e {
        RuleExpr::Ref(s) | RuleExpr::Terminal(s) | RuleExpr::CharClass(s) | RuleExpr::Hex(s) => {
            out.push_str(s);
        }
        RuleExpr::Range(lo, hi) => {
            out.push_str(lo);
            out.push('-');
            out.push_str(hi);
        }
        RuleExpr::Seq(parts) | RuleExpr::Alt(parts) => {
            for p in parts {
                serialize_symbol(p, out);
                out.push('·');
            }
        }
        RuleExpr::Diff(a, b) => {
            serialize_symbol(a, out);
            out.push('-');
            serialize_symbol(b, out);
        }
        RuleExpr::Star(x) | RuleExpr::Plus(x) | RuleExpr::Opt(x) | RuleExpr::Group(x) => {
            serialize_symbol(x, out);
        }
        RuleExpr::Repeat(_, _, x) => serialize_symbol(x, out),
    }
}

/// The right-hand-side symbols of a production body: a [`RuleExpr::Seq`] flattens to its items,
/// any other expression is a single-symbol body.
fn rhs_symbols(body: &RuleExpr) -> Vec<RuleExpr> {
    match body {
        RuleExpr::Seq(parts) => parts.clone(),
        other => vec![other.clone()],
    }
}

/// Lower ONE grammar production to its span-indexed [`DerivationRule`]. The production's body
/// is canonicalized first (dropping precedence groupings, flattening `Seq`), then each RHS
/// symbol is threaded onto a fresh span variable. A production whose canonical body is a
/// top-level alternation is not a single derivation rule — use [`grammar_to_derivation_rules`],
/// which splits an `Alt` into one rule per branch; this function lowers the alternation as a
/// single rule over the whole disjunctive body, which is only meaningful for non-`Alt`
/// productions (the modeled SVO grammar's are all plain concatenations).
pub fn grammar_rule_to_derivation(rule: &GrammarRule) -> Result<DerivationRule, LoweringError> {
    production_to_derivation(&rule.name, &canonicalize_expr(&rule.body))
}

/// Lower a `(name, canonical-body)` production to its derivation rule.
fn production_to_derivation(name: &str, body: &RuleExpr) -> Result<DerivationRule, LoweringError> {
    let symbols = rhs_symbols(body);
    let n = symbols.len();
    let head = nonterminal_atom(name, 0, n)?;
    let body_atoms = symbols
        .iter()
        .enumerate()
        .map(|(k, sym)| symbol_atom(sym, k, k + 1))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DerivationRule {
        nonterminal: name.to_owned(),
        head,
        body: body_atoms,
    })
}

/// Lower every production of a grammar to derivation rules, splitting a production whose
/// canonical body is a top-level alternation into one rule per branch (a CFG `X → a | b` is two
/// productions `X → a`, `X → b`). Every branch is threaded independently onto fresh span
/// variables.
pub fn grammar_to_derivation_rules(
    grammar: &Grammar,
) -> Result<Vec<DerivationRule>, LoweringError> {
    let mut out = Vec::new();
    for rule in &grammar.rules {
        match canonicalize_expr(&rule.body) {
            RuleExpr::Alt(branches) => {
                for branch in branches {
                    out.push(production_to_derivation(&rule.name, &branch)?);
                }
            }
            body => out.push(production_to_derivation(&rule.name, &body)?),
        }
    }
    Ok(out)
}

/// The small subject–verb–object grammar the dogfooding derivation lowering demonstrates:
/// `S → NP VP`, `VP → V NP`, `NP → Det N`. Each production is a plain concatenation of
/// nonterminals, so each lowers to exactly one span-indexed chart rule.
pub fn svo_grammar() -> Grammar {
    let seq = |a: &str, b: &str| {
        RuleExpr::Seq(vec![
            RuleExpr::Ref(a.to_owned()),
            RuleExpr::Ref(b.to_owned()),
        ])
    };
    Grammar {
        formalism: Formalism::Ebnf,
        rules: vec![
            GrammarRule {
                name: "S".to_owned(),
                body: seq("NP", "VP"),
            },
            GrammarRule {
                name: "VP".to_owned(),
                body: seq("V", "NP"),
            },
            GrammarRule {
                name: "NP".to_owned(),
                body: seq("Det", "N"),
            },
        ],
    }
}

// --------------------------------------------------------------------------- //
// Flagship fixture (reused by the pipeline reasoner-consumption test)
// --------------------------------------------------------------------------- //

/// A [`Form::Lexeme`] with a part of speech.
fn lexeme(lemma: &str, pos: &str) -> Form {
    Form::Lexeme {
        sign_system: "en".to_owned(),
        lemma: lemma.to_owned(),
        part_of_speech: Some(pos.to_owned()),
    }
}

/// The determiner-headed noun phrase `Det N` as a composed constituent.
fn noun_phrase(det: &str, noun: &str) -> Form {
    Form::Composed {
        sign_system: "en".to_owned(),
        level: "np".to_owned(),
        analysis: None,
        head: Some(1),
        slots: vec![
            Slot {
                index: 0,
                role: Some("determiner".to_owned()),
                dep_relation: Some("det".to_owned()),
                depends_on: Some(1),
                form: lexeme(det, "DET"),
            },
            Slot {
                index: 1,
                role: Some("head".to_owned()),
                dep_relation: Some("root".to_owned()),
                depends_on: None,
                form: lexeme(noun, "NOUN"),
            },
        ],
    }
}

/// The flagship quantified-SVO sentence "every cat chases a mouse" as a [`Form::Composed`],
/// with a co-resident constituency + dependency analysis. The verb is a [`Form::WordForm`]
/// (`chases`, inflecting the lexeme `chase`), so lowering exercises the WordForm→Lexeme lemma
/// recursion. Exposed so the pipeline reasoner-consumption test lowers the SAME sentence this
/// crate's tests lower — one fixture, no divergence.
pub fn flagship_svo_sentence() -> Form {
    let chases = Form::WordForm {
        sign_system: "en".to_owned(),
        lexeme: Box::new(lexeme("chase", "VERB")),
        features: vec![
            MorphFeature {
                key: "Number".to_owned(),
                values: vec!["Sing".to_owned()],
                layer: None,
            },
            MorphFeature {
                key: "Person".to_owned(),
                values: vec!["3".to_owned()],
                layer: None,
            },
            MorphFeature {
                key: "Tense".to_owned(),
                values: vec!["Pres".to_owned()],
                layer: None,
            },
        ],
    };
    Form::Composed {
        sign_system: "en".to_owned(),
        level: "sentence".to_owned(),
        analysis: Some("svo-flagship".to_owned()),
        head: Some(1),
        slots: vec![
            Slot {
                index: 0,
                role: Some("subject".to_owned()),
                dep_relation: Some("nsubj".to_owned()),
                depends_on: Some(1),
                form: noun_phrase("every", "cat"),
            },
            Slot {
                index: 1,
                role: Some("predicate".to_owned()),
                dep_relation: Some("root".to_owned()),
                depends_on: None,
                form: chases,
            },
            Slot {
                index: 2,
                role: Some("object".to_owned()),
                dep_relation: Some("obj".to_owned()),
                depends_on: Some(1),
                form: noun_phrase("a", "mouse"),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-built expected formula for "every cat chases a mouse":
    /// `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))`.
    fn expected_flagship_formula() -> Formula {
        let cat_x = predicate_atom("cat", vec![Term::var("x").unwrap()]).unwrap();
        let mouse_y = predicate_atom("mouse", vec![Term::var("y").unwrap()]).unwrap();
        let chase_xy = predicate_atom(
            "chase",
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap();
        let inner = Formula::Exists {
            vars: vec!["y".to_owned()],
            body: Box::new(Formula::And(vec![mouse_y, chase_xy])),
        };
        Formula::Forall {
            vars: vec!["x".to_owned()],
            body: Box::new(Formula::Implies(Box::new(cat_x), Box::new(inner))),
        }
    }

    #[test]
    fn flagship_lowers_to_the_expected_compositional_formula() {
        let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship lowers");
        let expected = expected_flagship_formula();
        assert_eq!(
            lowering.formula.content_key(),
            expected.content_key(),
            "every cat chases a mouse must lower to ∀x(cat(x) → ∃y(mouse(y) ∧ chase(x,y)))"
        );
    }

    #[test]
    fn every_stage_carries_a_preservation_record() {
        let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship lowers");
        // No lowering step is undeclared: exactly the required stages, in order.
        lowering
            .assert_all_stages_declared()
            .expect("all stages declared");
        assert_eq!(lowering.stages.len(), REQUIRED_STAGES.len());
        for stage in &lowering.stages {
            // The modeled fragment lowers exactly.
            assert_eq!(stage.preservation, PreservationKind::Exact);
            assert!(!stage.note.trim().is_empty(), "stage note is present");
        }
        assert_eq!(lowering.stage_names(), REQUIRED_STAGES);
    }

    #[test]
    fn existential_subject_reads_as_and_not_implies() {
        // "some cat chases a mouse" → ∃x(cat(x) ∧ ∃y(mouse(y) ∧ chase(x,y))).
        let mut sentence = flagship_svo_sentence();
        if let Form::Composed { slots, .. } = &mut sentence
            && let Form::Composed { slots: np, .. } = &mut slots[0].form
        {
            np[0].form = lexeme("some", "DET");
        }
        let lowering = lower_svo(&sentence).expect("existential subject lowers");
        let cat_x = predicate_atom("cat", vec![Term::var("x").unwrap()]).unwrap();
        let mouse_y = predicate_atom("mouse", vec![Term::var("y").unwrap()]).unwrap();
        let chase_xy = predicate_atom(
            "chase",
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap();
        let inner = Formula::Exists {
            vars: vec!["y".to_owned()],
            body: Box::new(Formula::And(vec![mouse_y, chase_xy])),
        };
        let expected = Formula::Exists {
            vars: vec!["x".to_owned()],
            body: Box::new(Formula::And(vec![cat_x, inner])),
        };
        assert_eq!(lowering.formula.content_key(), expected.content_key());
    }

    #[test]
    fn non_composed_sentence_hard_fails() {
        let err = lower_svo(&lexeme("cat", "NOUN")).expect_err("a bare lexeme is not a sentence");
        assert!(err.construct.contains("not a Composed sentence"), "{err}");
    }

    #[test]
    fn unmodeled_determiner_hard_fails() {
        // "most cats chase a mouse" — 'most' is not a modeled first-order determiner.
        let mut sentence = flagship_svo_sentence();
        if let Form::Composed { slots, .. } = &mut sentence
            && let Form::Composed { slots: np, .. } = &mut slots[0].form
        {
            np[0].form = lexeme("most", "DET");
        }
        let err = lower_svo(&sentence).expect_err("'most' is unmodeled");
        assert!(err.construct.contains("unmodeled determiner"), "{err}");
    }

    #[test]
    fn intransitive_clause_hard_fails() {
        // A subject + verb with no object is outside the transitive-SVO fragment.
        let sentence = Form::Composed {
            sign_system: "en".to_owned(),
            level: "sentence".to_owned(),
            analysis: None,
            head: Some(1),
            slots: vec![
                Slot {
                    index: 0,
                    role: Some("subject".to_owned()),
                    dep_relation: Some("nsubj".to_owned()),
                    depends_on: Some(1),
                    form: noun_phrase("every", "cat"),
                },
                Slot {
                    index: 1,
                    role: Some("predicate".to_owned()),
                    dep_relation: Some("root".to_owned()),
                    depends_on: None,
                    form: lexeme("sleep", "VERB"),
                },
            ],
        };
        let err = lower_svo(&sentence).expect_err("intransitive clause is unmodeled");
        assert!(err.construct.contains("object constituent"), "{err}");
    }

    #[test]
    fn bare_nominal_subject_hard_fails() {
        // "cats chase a mouse" — a determiner-less subject is refused, never read as an
        // implicit existential.
        let mut sentence = flagship_svo_sentence();
        if let Form::Composed { slots, .. } = &mut sentence {
            slots[0].form = lexeme("cat", "NOUN");
        }
        let err = lower_svo(&sentence).expect_err("bare nominal subject is unmodeled");
        assert!(err.construct.contains("bare nominal"), "{err}");
    }

    #[test]
    fn grammar_rule_lowers_to_span_indexed_derivation() {
        // S → NP VP  ⇒  S(i0,i2) :- NP(i0,i1), VP(i1,i2).
        let s_rule = &svo_grammar().rules[0];
        let derivation = grammar_rule_to_derivation(s_rule).expect("S production lowers");
        assert_eq!(derivation.nonterminal, "S");

        let expected_head = nonterminal_atom("S", 0, 2).expect("S head atom");
        let expected_body = vec![
            nonterminal_atom("NP", 0, 1).expect("NP body atom"),
            nonterminal_atom("VP", 1, 2).expect("VP body atom"),
        ];
        assert_eq!(derivation.head.content_key(), expected_head.content_key());
        assert_eq!(derivation.body.len(), 2);
        assert_eq!(
            derivation.body[0].content_key(),
            expected_body[0].content_key()
        );
        assert_eq!(
            derivation.body[1].content_key(),
            expected_body[1].content_key()
        );

        // The rule as one implication `(NP(i0,i1) ∧ VP(i1,i2)) → S(i0,i2)`.
        let expected_formula = Formula::Implies(
            Box::new(Formula::And(expected_body)),
            Box::new(expected_head),
        );
        assert_eq!(
            derivation.to_formula().content_key(),
            expected_formula.content_key()
        );
    }

    #[test]
    fn whole_svo_grammar_lowers_to_three_chart_rules() {
        let rules = grammar_to_derivation_rules(&svo_grammar()).expect("SVO grammar lowers");
        assert_eq!(rules.len(), 3);
        let names: Vec<&str> = rules.iter().map(|r| r.nonterminal.as_str()).collect();
        assert_eq!(names, vec!["S", "VP", "NP"]);
        // Every production here is a two-symbol concatenation → head span (0,2), two body atoms.
        for rule in &rules {
            assert_eq!(rule.body.len(), 2);
            assert_eq!(
                rule.head.content_key(),
                nonterminal_atom(&rule.nonterminal, 0, 2)
                    .expect("head atom")
                    .content_key()
            );
        }
    }

    #[test]
    fn ntriples_emission_records_every_stage_preservation() {
        let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship lowers");
        let bytes = lowering.to_ntriples("http://example.org/lang/lowering/flagship");
        let text = String::from_utf8(bytes).expect("UTF-8 N-Triples");
        // One preservationKind triple per stage, all Exact for the modeled fragment.
        let exact = PreservationKind::Exact.iri();
        let count = text
            .lines()
            .filter(|l| l.contains("preservationKind") && l.contains(&exact))
            .count();
        assert_eq!(count, REQUIRED_STAGES.len());
        assert!(text.contains("CompositionalLowering"));
        assert!(text.contains("LoweringStage"));
    }
}
