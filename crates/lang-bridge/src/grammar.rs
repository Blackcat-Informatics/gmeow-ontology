// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The grammar bridges: lift an EBNF / ABNF **grammar text** into a first-class
//! [`Grammar`] object over the `lang:Grammar` / `lang:GrammarRule` vocabulary, and round-trip
//! it exactly.
//!
//! A grammar is NOT a Form-AST form (it licenses forms; it never owns their identity), so a
//! grammar lift carries no [`Form`](gmeow_lang_form::Form)s: it carries the grammar object,
//! the `logic:Correspondence` the round-trip is decided over, and the loss-ledger row of the
//! RDF projection. Like every `lang:` bridge, the lift CARRIES a `logic:Correspondence` (an
//! [`Isomorphism`](MorphismClass::Isomorphism) with a discharged `SectionLaw`) rather than a
//! bespoke round-trip harness.
//!
//! The round-trip is **decidable** because a grammar has a canonical form
//! ([`Grammar::canonicalize`]) that mirrors [`LegPath::normalize`]'s flatten discipline: sort
//! rules by name, flatten nested `Seq`/`Alt`, drop singleton `Group`/`Seq`/`Alt`, and collapse
//! bounded repetitions to their `Star`/`Plus`/`Opt` special cases. **Two grammars are
//! ISOMORPHIC iff their canonical forms are `==`.** The Gate-3 round-trip is therefore the
//! structural identity `parse(serialize(g.canonicalize())).canonicalize() == g.canonicalize()`
//! — an equality over canonical trees, not a data-execution round-trip.
//!
//! CRITICAL: these bridges parse **grammar notation** (EBNF / ABNF), NOT RDF. The one and
//! only RDF/GTS parser stack in the workspace is the external `purrdf` crate; this module
//! never tokenizes Turtle, N-Triples, or GTS documents. A grammar of Turtle is a description
//! of the language the native `purrdf` codecs interpret — the self-hosting statement is made
//! at the level of the grammar object (and the `lang:grammarFor` link in the ontology), never
//! by rerouting a parse call site through a second hand-rolled RDF parser.

use gmeow_lang_form::SurfaceForm;
use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeCondition,
    DischargeVerdict, LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind,
};

use crate::bridge::{Bridge, IngestDiagnostic, LangFailure, Lifted};
use crate::emit::{digest16, ntriples_sorted};
use crate::plain_text::{UNDETERMINED_SCRIPT, normalization_label};

/// The `lang:` namespace base, byte-identical to the other `lang:` producers so every
/// `lang:` local name resolves to the same IRI across bridges.
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

/// The `rdf:type` predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `rdfs:label` predicate IRI.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The example-instance base the carried grammar-round-trip correspondence IRI lives under,
/// matching the base every other `lang:` producer content-addresses its minted individuals
/// under.
const GRAMMAR_CORR_BASE: &str = "http://example.org/lang/grammar-correspondence/";

/// The `logic:getLeg` program IRI: parse a grammar byte stream into the grammar object.
const GRAMMAR_GET_LEG: &str = "https://blackcatinformatics.ca/lang/grammarParseLeg";

/// The `logic:putLeg` program IRI: serialize the grammar object back to bytes.
const GRAMMAR_PUT_LEG: &str = "https://blackcatinformatics.ca/lang/grammarSerializeLeg";

/// Which formalism a [`Grammar`] is expressed in. Selects the concrete surface syntax the
/// bridges parse and emit, and the `lang:grammarFormalism` individual the RDF projection
/// declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formalism {
    /// W3C-style EBNF (`Name ::= expr`, postfix `* + ?`, `[...]` char classes, `#xNN` hex,
    /// `A - B` exclusion) — the notation the W3C Turtle grammar is written in.
    Ebnf,
    /// RFC-5234 ABNF (`name = elements`, `/` alternation, prefix `n*m` repetition, `[...]`
    /// optional groups, `(...)` groups, `%xNN` / `%xNN-NN` numeric terminals).
    Abnf,
}

impl Formalism {
    /// The `lang:GrammarFormalism` individual local name for this formalism, exactly as it
    /// appears in `module.ttl`.
    pub fn individual_local_name(self) -> &'static str {
        match self {
            Formalism::Ebnf => "ebnfFormalism",
            Formalism::Abnf => "abnfFormalism",
        }
    }
}

/// The body of a grammar production — a structural expression tree over the notation. The
/// tree is surface-syntax-independent: EBNF `x*` and ABNF `*x` both lift to [`RuleExpr::Star`],
/// so identity is decided over structure, not over the notation's spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleExpr {
    /// A reference to another rule by name (`nonterminal`).
    Ref(String),
    /// A literal terminal string — the content BETWEEN the quotes, delimiter-independent
    /// (`'a'` and `"a"` both lift to `Terminal("a")`). The quote a serialization uses is
    /// re-derived from the content, so the delimiter choice is never part of identity.
    Terminal(String),
    /// A character-class body (EBNF `[...]`): the raw text between the brackets, retained
    /// verbatim (ranges, negation `^`, `#xNN` members) — never re-interpreted, so it
    /// round-trips exactly.
    CharClass(String),
    /// A single hexadecimal character terminal — the hex digits only (EBNF `#xNN`, ABNF
    /// `%xNN`). The base prefix is the formalism's, re-derived on serialization.
    Hex(String),
    /// An ABNF numeric range terminal (`%xLO-HI`): the low and high hex bounds.
    Range(String, String),
    /// Left-to-right concatenation (`a b c`).
    Seq(Vec<RuleExpr>),
    /// Alternation (`a | b` in EBNF, `a / b` in ABNF).
    Alt(Vec<RuleExpr>),
    /// EBNF exclusion / difference: `A - B` (match `A` but not `B`).
    Diff(Box<RuleExpr>, Box<RuleExpr>),
    /// Zero-or-more repetition (EBNF `x*`, ABNF `*x`).
    Star(Box<RuleExpr>),
    /// One-or-more repetition (EBNF `x+`, ABNF `1*x`).
    Plus(Box<RuleExpr>),
    /// Optional (EBNF `x?`, ABNF `[x]`).
    Opt(Box<RuleExpr>),
    /// A bounded ABNF repetition `min*max x` (either bound optional); the special cases
    /// collapse to [`Star`](RuleExpr::Star) / [`Plus`](RuleExpr::Plus) / [`Opt`](RuleExpr::Opt)
    /// under [`canonicalize_expr`].
    Repeat(Option<u32>, Option<u32>, Box<RuleExpr>),
    /// An explicit grouping `(...)`. A grouping carries no identity of its own — it is a
    /// precedence device — so [`canonicalize_expr`] drops it, unwrapping the inner expression.
    Group(Box<RuleExpr>),
}

/// A single grammar production: `name` and its body expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarRule {
    /// The rule's nonterminal name (the left-hand side).
    pub name: String,
    /// The rule's body (the right-hand side).
    pub body: RuleExpr,
}

/// A whole grammar: its formalism and its production rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grammar {
    /// The formalism the grammar is expressed in.
    pub formalism: Formalism,
    /// The production rules, in source order (identity is decided over the CANONICAL form,
    /// which sorts them — see [`Grammar::canonicalize`]).
    pub rules: Vec<GrammarRule>,
}

impl Grammar {
    /// The CANONICAL form: sort rules by name and canonicalize every body. Two grammars are
    /// ISOMORPHIC iff their canonical forms are `==` — the decidable identity the Gate-3
    /// round-trip compares. Mirrors [`LegPath::normalize`]'s flatten discipline (flatten
    /// nested `Seq`/`Alt`, drop singleton `Group`/`Seq`/`Alt`) lifted to grammar bodies.
    pub fn canonicalize(&self) -> Grammar {
        let mut rules: Vec<GrammarRule> = self
            .rules
            .iter()
            .map(|r| GrammarRule {
                name: r.name.clone(),
                body: canonicalize_expr(&r.body),
            })
            .collect();
        rules.sort_by(|a, b| a.name.cmp(&b.name));
        Grammar {
            formalism: self.formalism,
            rules,
        }
    }
}

/// The canonical form of one body expression: cancel groupings, flatten nested `Seq`/`Alt`,
/// drop singleton `Seq`/`Alt`, and collapse bounded repetitions to their `Star`/`Plus`/`Opt`
/// special cases. Idempotent — `canonicalize_expr(canonicalize_expr(e)) == canonicalize_expr(e)`.
pub fn canonicalize_expr(e: &RuleExpr) -> RuleExpr {
    match e {
        RuleExpr::Ref(_) | RuleExpr::Terminal(_) | RuleExpr::CharClass(_) | RuleExpr::Hex(_) => {
            e.clone()
        }
        RuleExpr::Range(lo, hi) => RuleExpr::Range(lo.clone(), hi.clone()),
        // A grouping is pure precedence — unwrap it, so `a | (b | c)` and `(a | b) | c`
        // canonicalize to the same flattened `Alt`.
        RuleExpr::Group(inner) => canonicalize_expr(inner),
        RuleExpr::Seq(parts) => {
            let mut flat = Vec::new();
            for p in parts {
                match canonicalize_expr(p) {
                    RuleExpr::Seq(inner) => flat.extend(inner),
                    other => flat.push(other),
                }
            }
            if flat.len() == 1 {
                flat.pop().expect("len checked")
            } else {
                RuleExpr::Seq(flat)
            }
        }
        RuleExpr::Alt(parts) => {
            let mut flat = Vec::new();
            for p in parts {
                match canonicalize_expr(p) {
                    RuleExpr::Alt(inner) => flat.extend(inner),
                    other => flat.push(other),
                }
            }
            if flat.len() == 1 {
                flat.pop().expect("len checked")
            } else {
                RuleExpr::Alt(flat)
            }
        }
        RuleExpr::Diff(a, b) => RuleExpr::Diff(
            Box::new(canonicalize_expr(a)),
            Box::new(canonicalize_expr(b)),
        ),
        RuleExpr::Star(x) => RuleExpr::Star(Box::new(canonicalize_expr(x))),
        RuleExpr::Plus(x) => RuleExpr::Plus(Box::new(canonicalize_expr(x))),
        RuleExpr::Opt(x) => RuleExpr::Opt(Box::new(canonicalize_expr(x))),
        RuleExpr::Repeat(min, max, x) => {
            let inner = Box::new(canonicalize_expr(x));
            match (min, max) {
                (Some(0), None) | (None, None) => RuleExpr::Star(inner),
                (Some(1), None) => RuleExpr::Plus(inner),
                (Some(0), Some(1)) => RuleExpr::Opt(inner),
                _ => RuleExpr::Repeat(*min, *max, inner),
            }
        }
    }
}

// --- Precedence-driven serialization ---------------------------------------- //

/// The binding tightness of an expression node, loosest (1) to tightest (5). The serializer
/// inserts a grouping around a child ONLY when its precedence is below the context's minimum,
/// so `serialize(parse(text))` reconstructs the same tree.
///
/// This is the SINGLE operator-precedence table the codec shares across every grammar
/// formalism — the `Ebnf` and `Abnf` serializers both drive their grouping decisions through
/// it, which is precisely what lets the notation's typed views agree on one canonical tree.
/// [`expr_precedence`] re-exports it so the cross-surface coherence gate can assert the ladder
/// directly.
pub fn expr_precedence(e: &RuleExpr) -> u8 {
    prec(e)
}

/// The binding tightness of an expression node, loosest (1) to tightest (5). The serializer
/// inserts a grouping around a child ONLY when its precedence is below the context's minimum,
/// so `serialize(parse(text))` reconstructs the same tree.
fn prec(e: &RuleExpr) -> u8 {
    match e {
        RuleExpr::Alt(_) => 1,
        RuleExpr::Seq(_) => 2,
        RuleExpr::Diff(_, _) => 3,
        RuleExpr::Star(_) | RuleExpr::Plus(_) | RuleExpr::Opt(_) | RuleExpr::Repeat(_, _, _) => 4,
        RuleExpr::Ref(_)
        | RuleExpr::Terminal(_)
        | RuleExpr::CharClass(_)
        | RuleExpr::Hex(_)
        | RuleExpr::Range(_, _)
        | RuleExpr::Group(_) => 5,
    }
}

/// Quote a terminal's content with a delimiter that does not occur in it. A parsed terminal's
/// content never contains its own delimiter, so it contains AT MOST one quote kind and a valid
/// delimiter always exists — the choice is a pure function of the content, keeping the
/// serialization deterministic and the content the sole identity.
fn quote_terminal(value: &str) -> String {
    if !value.contains('\'') {
        format!("'{value}'")
    } else {
        // Content has a `'`, so (never containing its own delimiter) it has no `"`.
        format!("\"{value}\"")
    }
}

/// Serialize one body expression under a formalism, wrapping in a grouping when the node's
/// precedence falls below `parent_min`.
fn serialize_expr(f: Formalism, e: &RuleExpr, parent_min: u8, out: &mut String) {
    let wrap = prec(e) < parent_min;
    if wrap {
        out.push('(');
    }
    match e {
        RuleExpr::Ref(s) => out.push_str(s),
        RuleExpr::Terminal(v) => out.push_str(&quote_terminal(v)),
        RuleExpr::CharClass(c) => {
            out.push('[');
            out.push_str(c);
            out.push(']');
        }
        RuleExpr::Hex(h) => {
            out.push_str(match f {
                Formalism::Ebnf => "#x",
                Formalism::Abnf => "%x",
            });
            out.push_str(h);
        }
        RuleExpr::Range(lo, hi) => {
            // Numeric ranges are an ABNF construct; an EBNF grammar carries ranges inside a
            // `CharClass` instead, so an EBNF `Range` never arises from a parse.
            debug_assert_eq!(f, Formalism::Abnf, "Range is an ABNF-only construct");
            out.push_str("%x");
            out.push_str(lo);
            out.push('-');
            out.push_str(hi);
        }
        RuleExpr::Alt(parts) => {
            let sep = match f {
                Formalism::Ebnf => " | ",
                Formalism::Abnf => " / ",
            };
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                serialize_expr(f, p, 2, out);
            }
        }
        RuleExpr::Seq(parts) => {
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                serialize_expr(f, p, 3, out);
            }
        }
        RuleExpr::Diff(a, b) => {
            debug_assert_eq!(f, Formalism::Ebnf, "Diff is an EBNF-only construct");
            serialize_expr(f, a, 4, out);
            out.push_str(" - ");
            serialize_expr(f, b, 4, out);
        }
        RuleExpr::Star(x) => serialize_simple_repetition(f, "*", "*", x, out),
        RuleExpr::Plus(x) => serialize_simple_repetition(f, "+", "1*", x, out),
        RuleExpr::Opt(x) => match f {
            Formalism::Ebnf => {
                serialize_expr(f, x, 5, out);
                out.push('?');
            }
            Formalism::Abnf => {
                out.push_str("[ ");
                serialize_expr(f, x, 0, out);
                out.push_str(" ]");
            }
        },
        RuleExpr::Repeat(min, max, x) => {
            // Only ABNF spells a bounded repetition; canonicalization has already collapsed the
            // `Star`/`Plus`/`Opt` special cases, so what remains is a genuine `min*max` bound.
            debug_assert_eq!(
                f,
                Formalism::Abnf,
                "bounded Repeat is an ABNF-only construct"
            );
            if let (Some(a), Some(b)) = (min, max)
                && a == b
            {
                // Exact repetition `nElement` (no star).
                out.push_str(&a.to_string());
                serialize_expr(f, x, 5, out);
                if wrap {
                    out.push(')');
                }
                return;
            }
            if let Some(a) = min {
                out.push_str(&a.to_string());
            }
            out.push('*');
            if let Some(b) = max {
                out.push_str(&b.to_string());
            }
            serialize_expr(f, x, 5, out);
        }
        RuleExpr::Group(inner) => {
            out.push('(');
            serialize_expr(f, inner, 0, out);
            out.push(')');
        }
    }
    if wrap {
        out.push(')');
    }
}

/// Serialize a `Star`/`Plus` repetition: EBNF postfixes `ebnf_op`, ABNF prefixes `abnf_prefix`.
fn serialize_simple_repetition(
    f: Formalism,
    ebnf_op: &str,
    abnf_prefix: &str,
    x: &RuleExpr,
    out: &mut String,
) {
    match f {
        Formalism::Ebnf => {
            serialize_expr(f, x, 5, out);
            out.push_str(ebnf_op);
        }
        Formalism::Abnf => {
            out.push_str(abnf_prefix);
            serialize_expr(f, x, 5, out);
        }
    }
}

/// Serialize a whole grammar to its canonical layout: rules sorted by name, one `name <sep>
/// body` per line (`::=` for EBNF, `=` for ABNF), each terminated by a newline. A pure
/// function of the grammar's canonical structure.
pub fn serialize_grammar(g: &Grammar) -> String {
    let mut rules = g.rules.clone();
    rules.sort_by(|a, b| a.name.cmp(&b.name));
    let sep = match g.formalism {
        Formalism::Ebnf => " ::= ",
        Formalism::Abnf => " = ",
    };
    let mut out = String::new();
    for r in &rules {
        out.push_str(&r.name);
        out.push_str(sep);
        serialize_expr(g.formalism, &r.body, 0, &mut out);
        out.push('\n');
    }
    out
}

// --- The RDF projection ----------------------------------------------------- //

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

/// Emit the `lang:Grammar` / `lang:GrammarRule` triples for a grammar rooted at `grammar_iri`,
/// as a deterministic (sorted, deduped) N-Triples byte stream. The grammar is typed
/// `lang:Grammar`, carries its `lang:grammarFormalism`, and contributes ONE `lang:GrammarRule`
/// per production — each content-addressed on `(name, canonical body)` via [`digest16`], typed
/// `lang:GrammarRule`, linked back with `lang:grammarRuleOf`, and labelled with its name.
pub fn grammar_to_ntriples(g: &Grammar, grammar_iri: &str) -> Vec<u8> {
    let canon = g.canonicalize();
    let formalism_iri = format!("{LANG_NS}{}", canon.formalism.individual_local_name());
    let mut lines = vec![
        format!("<{grammar_iri}> <{RDF_TYPE}> <{LANG_NS}Grammar> ."),
        format!("<{grammar_iri}> <{LANG_NS}grammarFormalism> <{formalism_iri}> ."),
    ];
    for rule in &canon.rules {
        let body_text = {
            let mut s = String::new();
            serialize_expr(canon.formalism, &rule.body, 0, &mut s);
            s
        };
        let rule_key = format!("{}\u{1f}{}", rule.name, body_text);
        let rule_iri = format!(
            "{grammar_iri}/rule/{}",
            digest16("lang-grammar-rule", &rule_key)
        );
        lines.push(format!(
            "<{rule_iri}> <{RDF_TYPE}> <{LANG_NS}GrammarRule> ."
        ));
        lines.push(format!(
            "<{rule_iri}> <{LANG_NS}grammarRuleOf> <{grammar_iri}> ."
        ));
        lines.push(format!(
            "<{rule_iri}> <{RDFS_LABEL}> \"{}\" .",
            escape_literal(&rule.name)
        ));
    }
    ntriples_sorted(lines)
}

// --- The carried correspondence --------------------------------------------- //

/// The get/put [`LegPath`] pair the carried correspondence's round-trip is decided over: the
/// put leg is the structural inverse of the get leg, so
/// [`exact_round_trip_holds`](crate::exact_round_trip_holds) holds by construction.
#[must_use]
pub fn grammar_leg_pair() -> (LegPath, LegPath) {
    let get = LegPath::Seq(vec![
        LegPath::Step(format!("{LANG_NS}parseGrammarRule")),
        LegPath::Step(format!("{LANG_NS}canonicalizeGrammarBody")),
    ]);
    let put = get.invert();
    (get, put)
}

/// Build the EXACT round-trip `logic:Correspondence` a grammar lift carries for a grammar whose
/// canonical serialization hashes to `source_key`: an
/// [`Isomorphism`](MorphismClass::Isomorphism) on the satisfaction-preserving rung,
/// `mnemomorphic` (the canonical grammar retains the whole structure), whose `GetPut`, `PutGet`,
/// and `SectionLaw` claims are conclusively discharged — the canonical round-trip trivially
/// satisfies them. The IRI is content-addressed on the canonical bytes.
pub fn grammar_correspondence(source_key: &str) -> Correspondence {
    let iri = format!(
        "{GRAMMAR_CORR_BASE}{}",
        digest16("lang-grammar-corr", source_key)
    );
    let discharged = |law: CorrespondenceLaw| LawClaimIr {
        law,
        verdict: DischargeVerdict::ObligationDischarged,
        condition: Some(DischargeCondition::DischargeSyntacticReachability),
    };
    Correspondence::new(
        iri,
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        // The canonical grammar retains the whole structural witness — what lets the round-trip
        // claim SectionLaw.
        true,
        Some(Determinacy::Crisp),
        Some(GRAMMAR_GET_LEG.to_owned()),
        Some(GRAMMAR_PUT_LEG.to_owned()),
        vec![
            discharged(CorrespondenceLaw::GetPut),
            discharged(CorrespondenceLaw::PutGet),
            discharged(CorrespondenceLaw::SectionLaw),
        ],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("exact grammar correspondence is well-formed by construction")
}

// --- The parser ------------------------------------------------------------- //

/// Hard-fail helper: a grammar-notation construct the bridge does not model is named exactly,
/// never silently dropped (the `lang:SilentIngestDrop` floor).
fn unmodeled(construct: impl Into<String>) -> IngestDiagnostic {
    IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: construct.into(),
    }
}

/// A recursive-descent parser over one production's right-hand side, operating on a fixed
/// character buffer. The lexing is inline: whitespace separates concatenation items and every
/// token is recognised at the point it is needed.
struct ExprParser {
    chars: Vec<char>,
    pos: usize,
    formalism: Formalism,
}

impl ExprParser {
    fn new(text: &str, formalism: Formalism) -> Self {
        ExprParser {
            chars: text.chars().collect(),
            pos: 0,
            formalism,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Parse the full expression and require the input to be exhausted.
    fn parse(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        let e = self.parse_alt()?;
        self.skip_ws();
        if let Some(c) = self.peek() {
            return Err(unmodeled(format!(
                "unexpected '{c}' at column {} in grammar expression",
                self.pos
            )));
        }
        Ok(e)
    }

    fn alt_marker(&self) -> char {
        match self.formalism {
            Formalism::Ebnf => '|',
            Formalism::Abnf => '/',
        }
    }

    fn parse_alt(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        let mut branches = vec![self.parse_seq()?];
        loop {
            self.skip_ws();
            if self.peek() == Some(self.alt_marker()) {
                self.bump();
                branches.push(self.parse_seq()?);
            } else {
                break;
            }
        }
        if branches.len() == 1 {
            Ok(branches.pop().expect("len checked"))
        } else {
            Ok(RuleExpr::Alt(branches))
        }
    }

    fn parse_seq(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if !self.starts_item() {
                break;
            }
            items.push(self.parse_diff()?);
        }
        if items.is_empty() {
            return Err(unmodeled("empty sequence in grammar expression".to_owned()));
        }
        if items.len() == 1 {
            Ok(items.pop().expect("len checked"))
        } else {
            Ok(RuleExpr::Seq(items))
        }
    }

    /// Whether the next token can begin a concatenation item (a primary), stopping the
    /// sequence loop on alternation markers, close-brackets, and end of input.
    fn starts_item(&self) -> bool {
        match self.peek() {
            None => false,
            Some(c) => {
                if c == self.alt_marker() || c == ')' || c == ']' {
                    return false;
                }
                // EBNF `-` at sequence position is the difference operator, consumed inside
                // `parse_diff` after the first primary — it never STARTS an item.
                if c == '-' {
                    return false;
                }
                true
            }
        }
    }

    fn parse_diff(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        let left = self.parse_postfix()?;
        if self.formalism == Formalism::Ebnf {
            self.skip_ws();
            if self.peek() == Some('-') {
                self.bump();
                self.skip_ws();
                let right = self.parse_postfix()?;
                return Ok(RuleExpr::Diff(Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }

    fn parse_postfix(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        // ABNF repetition is a PREFIX (`*x`, `1*4 x`, `2x`); EBNF repetition is a POSTFIX.
        if self.formalism == Formalism::Abnf
            && let Some(rep) = self.try_parse_abnf_repetition()?
        {
            return Ok(rep);
        }
        let mut e = self.parse_primary()?;
        if self.formalism == Formalism::Ebnf {
            loop {
                match self.peek() {
                    Some('*') => {
                        self.bump();
                        e = RuleExpr::Star(Box::new(e));
                    }
                    Some('+') => {
                        self.bump();
                        e = RuleExpr::Plus(Box::new(e));
                    }
                    Some('?') => {
                        self.bump();
                        e = RuleExpr::Opt(Box::new(e));
                    }
                    _ => break,
                }
            }
        }
        Ok(e)
    }

    /// Try to parse an ABNF repetition prefix `min*max` / `n` and its operand. Returns `None`
    /// when the next token is a plain element (no repetition prefix).
    fn try_parse_abnf_repetition(&mut self) -> Result<Option<RuleExpr>, IngestDiagnostic> {
        let start = self.pos;
        let min = self.read_number();
        if self.peek() == Some('*') {
            self.bump();
            let max = self.read_number();
            self.skip_ws();
            let operand = self.parse_primary()?;
            return Ok(Some(RuleExpr::Repeat(min, max, Box::new(operand))));
        }
        if let Some(n) = min {
            // A bare count `nElement` — exact repetition.
            self.skip_ws();
            let operand = self.parse_primary()?;
            return Ok(Some(RuleExpr::Repeat(Some(n), Some(n), Box::new(operand))));
        }
        // No leading number and no `*` — not a repetition; rewind.
        self.pos = start;
        Ok(None)
    }

    fn read_number(&mut self) -> Option<u32> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return None;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse().ok()
    }

    fn parse_primary(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.bump();
                let inner = self.parse_alt()?;
                self.skip_ws();
                if self.bump() != Some(')') {
                    return Err(unmodeled("unbalanced '(' in grammar expression".to_owned()));
                }
                Ok(RuleExpr::Group(Box::new(inner)))
            }
            Some('[') if self.formalism == Formalism::Abnf => {
                // ABNF optional group.
                self.bump();
                let inner = self.parse_alt()?;
                self.skip_ws();
                if self.bump() != Some(']') {
                    return Err(unmodeled("unbalanced '[' in ABNF optional".to_owned()));
                }
                Ok(RuleExpr::Opt(Box::new(inner)))
            }
            Some('[') => self.parse_char_class(),
            Some('\'') | Some('"') => self.parse_terminal(),
            Some('#') if self.formalism == Formalism::Ebnf => self.parse_ebnf_hex(),
            Some('%') if self.formalism == Formalism::Abnf => self.parse_abnf_numeric(),
            Some(c) if is_name_start(c, self.formalism) => Ok(RuleExpr::Ref(self.read_name())),
            Some(c) => Err(unmodeled(format!(
                "unmodeled grammar construct starting with '{c}' at column {}",
                self.pos
            ))),
            None => Err(unmodeled("unexpected end of grammar expression".to_owned())),
        }
    }

    /// Read a char-class body `[...]` verbatim up to the FIRST `]` (W3C EBNF classes do not
    /// escape `]`; a backslash inside is a literal member).
    fn parse_char_class(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        self.bump(); // '['
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == ']' {
                let body: String = self.chars[start..self.pos].iter().collect();
                self.bump(); // ']'
                return Ok(RuleExpr::CharClass(body));
            }
            self.pos += 1;
        }
        Err(unmodeled("unterminated '[' character class".to_owned()))
    }

    /// Read a quoted terminal; the content never contains its own delimiter.
    fn parse_terminal(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        let quote = self.bump().expect("caller checked a quote is present");
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                let value: String = self.chars[start..self.pos].iter().collect();
                self.bump(); // closing quote
                return Ok(RuleExpr::Terminal(value));
            }
            self.pos += 1;
        }
        Err(unmodeled(format!("unterminated {quote}-quoted terminal")))
    }

    /// Read an EBNF `#xNN` hexadecimal terminal.
    fn parse_ebnf_hex(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        self.bump(); // '#'
        if self.bump() != Some('x') {
            return Err(unmodeled(
                "'#' not followed by 'x' (only '#xNN' hex terminals are modeled)".to_owned(),
            ));
        }
        let digits = self.read_hex_digits();
        if digits.is_empty() {
            return Err(unmodeled("'#x' with no hex digits".to_owned()));
        }
        Ok(RuleExpr::Hex(digits))
    }

    /// Read an ABNF numeric terminal `%xNN`, range `%xNN-NN`, or concatenation `%xNN.NN`.
    fn parse_abnf_numeric(&mut self) -> Result<RuleExpr, IngestDiagnostic> {
        self.bump(); // '%'
        match self.bump() {
            Some('x') => {}
            Some(other) => {
                return Err(unmodeled(format!(
                    "unmodeled ABNF numeric base '%{other}' (only '%x' hex is modeled)"
                )));
            }
            None => return Err(unmodeled("'%' with no numeric base".to_owned())),
        }
        let first = self.read_hex_digits();
        if first.is_empty() {
            return Err(unmodeled("'%x' with no hex digits".to_owned()));
        }
        match self.peek() {
            Some('-') => {
                self.bump();
                let hi = self.read_hex_digits();
                if hi.is_empty() {
                    return Err(unmodeled("'%xNN-' with no upper bound".to_owned()));
                }
                Ok(RuleExpr::Range(first, hi))
            }
            Some('.') => {
                let mut parts = vec![RuleExpr::Hex(first)];
                while self.peek() == Some('.') {
                    self.bump();
                    let d = self.read_hex_digits();
                    if d.is_empty() {
                        return Err(unmodeled(
                            "'%xNN.' concatenation with a missing element".to_owned(),
                        ));
                    }
                    parts.push(RuleExpr::Hex(d));
                }
                Ok(RuleExpr::Seq(parts))
            }
            _ => Ok(RuleExpr::Hex(first)),
        }
    }

    fn read_hex_digits(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_hexdigit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn read_name(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_name_continue(c, self.formalism) {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }
}

/// Whether `c` can begin a rule name in `formalism`. EBNF names are alphanumeric plus
/// underscore; ABNF names begin with a letter.
fn is_name_start(c: char, formalism: Formalism) -> bool {
    match formalism {
        Formalism::Ebnf => c.is_ascii_alphabetic() || c == '_',
        Formalism::Abnf => c.is_ascii_alphabetic(),
    }
}

/// Whether `c` can continue a rule name. EBNF: alphanumeric or underscore. ABNF: alphanumeric
/// or hyphen (RFC-5234 rulenames).
fn is_name_continue(c: char, formalism: Formalism) -> bool {
    match formalism {
        Formalism::Ebnf => c.is_ascii_alphanumeric() || c == '_',
        Formalism::Abnf => c.is_ascii_alphanumeric() || c == '-',
    }
}

/// Whether a source line is a comment or blank line (skipped before rule parsing). A comment is
/// a line whose first non-whitespace character is `#` FOLLOWED by whitespace or end of line —
/// so a `#xNN` hex terminal (which is never at line start in a well-formed grammar) is never
/// mistaken for a comment.
fn is_skippable_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        return rest.is_empty() || rest.starts_with(char::is_whitespace);
    }
    // ABNF comments run from `;` to end of line; a whole-line comment starts with `;`.
    trimmed.starts_with(';')
}

/// Strip an inline ABNF comment (`; …` to end of line) from a rule line, respecting quoted
/// terminals so a `;` inside `"…"` is not treated as a comment marker.
fn strip_abnf_inline_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == ';' {
                    break;
                }
                if c == '"' || c == '\'' {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

/// Parse a grammar text into a [`Grammar`] under `formalism`, or HARD FAIL naming the offending
/// construct. Non-UTF-8 input is a [`LangFailure::NonUtf8Surface`]; every notation violation is
/// a [`LangFailure::SilentIngestDrop`] — the bridge refuses rather than dropping a construct it
/// cannot account for.
pub fn parse_grammar(bytes: &[u8], formalism: Formalism) -> Result<Grammar, IngestDiagnostic> {
    let text = std::str::from_utf8(bytes).map_err(|e| IngestDiagnostic {
        failure_class: LangFailure::NonUtf8Surface,
        construct: format!(
            "non-UTF-8 grammar input: {} byte(s), first invalid byte at index {}",
            bytes.len(),
            e.valid_up_to()
        ),
    })?;
    let sep = match formalism {
        Formalism::Ebnf => "::=",
        Formalism::Abnf => "=",
    };
    let mut rules = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        if is_skippable_line(line) {
            continue;
        }
        let effective = match formalism {
            Formalism::Ebnf => line.to_owned(),
            Formalism::Abnf => strip_abnf_inline_comment(line),
        };
        if effective.trim().is_empty() {
            continue;
        }
        let (name_part, body_part) = effective.split_once(sep).ok_or_else(|| {
            unmodeled(format!(
                "grammar line has no '{sep}' rule separator: '{}'",
                line.trim()
            ))
        })?;
        // ABNF incremental alternation (`name =/ elements`) is not modeled — hard-fail rather
        // than silently treat it as a fresh rule that would shadow the base definition.
        if formalism == Formalism::Abnf && body_part.starts_with('/') {
            return Err(unmodeled(format!(
                "unmodeled ABNF incremental alternation '=/' in rule: '{}'",
                line.trim()
            )));
        }
        let name = name_part.trim().to_owned();
        if name.is_empty() || !name.chars().all(|c| is_name_continue(c, formalism)) {
            return Err(unmodeled(format!("malformed rule name: '{name}'")));
        }
        let mut parser = ExprParser::new(body_part, formalism);
        let body = parser.parse()?;
        rules.push(GrammarRule { name, body });
    }
    if rules.is_empty() {
        return Err(unmodeled("no grammar rules parsed".to_owned()));
    }
    Ok(Grammar { formalism, rules })
}

// --- The bridges ------------------------------------------------------------ //

/// Build the [`Lifted`] product for a parsed `grammar`: no forms (a grammar is not a Form-AST
/// form), one surface carrying the CANONICAL grammar text (so [`Bridge::emit`] reproduces it),
/// the carried exact round-trip correspondence, and one ledger row recording the RDF
/// projection as [`PreservationKind::Exact`].
fn lift_grammar(grammar: &Grammar) -> Result<Lifted, IngestDiagnostic> {
    let canonical = grammar.canonicalize();
    let text = serialize_grammar(&canonical);
    let surface = SurfaceForm {
        text: text.clone(),
        script: UNDETERMINED_SCRIPT.to_owned(),
        encoding: "UTF-8".to_owned(),
        normalization: normalization_label(&text).to_owned(),
        collation: "und".to_owned(),
    };
    let correspondence = grammar_correspondence(&text);
    let grammar_iri = format!(
        "{GRAMMAR_CORR_BASE}{}/grammar",
        digest16("lang-grammar", &text)
    );
    let rdf = String::from_utf8(grammar_to_ntriples(&canonical, &grammar_iri)).map_err(|e| {
        IngestDiagnostic {
            failure_class: LangFailure::NonUtf8Surface,
            construct: format!(
                "grammar N-Triples projection is not UTF-8: first invalid byte at index {}",
                e.utf8_error().valid_up_to()
            ),
        }
    })?;
    let mut loss = crate::registry::LossLedger::new();
    let ledger = vec![crate::registry::emit_ledger_row(
        &mut loss,
        format!("lang-grammar:{}", digest16("lang-grammar", &text)),
        rdf,
        true,
        PreservationKind::Exact,
        "n/a".to_owned(),
        Vec::new(),
        Vec::new(),
    )];
    Ok(Lifted {
        forms: Vec::new(),
        surfaces: vec![surface],
        correspondence,
        ledger,
        loss,
    })
}

/// The W3C-style EBNF grammar bridge: lift `Name ::= expr` productions into a [`Grammar`] under
/// an exact round-trip `logic:Correspondence`, and round-trip the grammar isomorphically.
pub struct EbnfBridge;

impl EbnfBridge {
    /// Parse EBNF grammar text into a [`Grammar`], or hard-fail naming the construct.
    pub fn parse(&self, text: &str) -> Result<Grammar, IngestDiagnostic> {
        parse_grammar(text.as_bytes(), Formalism::Ebnf)
    }

    /// Serialize a [`Grammar`] to canonical EBNF text.
    pub fn serialize(&self, grammar: &Grammar) -> String {
        serialize_grammar(grammar)
    }

    /// Lift raw bytes into the [`Grammar`] object (the grammar the [`Lifted`] product carries
    /// via its ledger + correspondence).
    pub fn to_grammar(&self, bytes: &[u8]) -> Result<Grammar, IngestDiagnostic> {
        parse_grammar(bytes, Formalism::Ebnf)
    }
}

impl Bridge for EbnfBridge {
    fn lift(&self, bytes: &[u8]) -> Result<Lifted, IngestDiagnostic> {
        lift_grammar(&parse_grammar(bytes, Formalism::Ebnf)?)
    }

    fn emit(&self, lifted: &Lifted) -> Vec<u8> {
        lifted
            .surfaces
            .first()
            .map(|s| s.text.clone().into_bytes())
            .unwrap_or_default()
    }
}

/// The RFC-5234 ABNF grammar bridge: lift `name = elements` productions (with `/` alternation
/// and prefix `n*m` repetition) into a [`Grammar`] under an exact round-trip
/// `logic:Correspondence`, and round-trip the grammar isomorphically.
pub struct AbnfBridge;

impl AbnfBridge {
    /// Parse ABNF grammar text into a [`Grammar`], or hard-fail naming the construct.
    pub fn parse(&self, text: &str) -> Result<Grammar, IngestDiagnostic> {
        parse_grammar(text.as_bytes(), Formalism::Abnf)
    }

    /// Serialize a [`Grammar`] to canonical ABNF text.
    pub fn serialize(&self, grammar: &Grammar) -> String {
        serialize_grammar(grammar)
    }

    /// Lift raw bytes into the [`Grammar`] object.
    pub fn to_grammar(&self, bytes: &[u8]) -> Result<Grammar, IngestDiagnostic> {
        parse_grammar(bytes, Formalism::Abnf)
    }
}

impl Bridge for AbnfBridge {
    fn lift(&self, bytes: &[u8]) -> Result<Lifted, IngestDiagnostic> {
        lift_grammar(&parse_grammar(bytes, Formalism::Abnf)?)
    }

    fn emit(&self, lifted: &Lifted) -> Vec<u8> {
        lifted
            .surfaces
            .first()
            .map(|s| s.text.clone().into_bytes())
            .unwrap_or_default()
    }
}
