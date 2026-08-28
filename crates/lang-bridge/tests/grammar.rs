// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate 3 — grammar round-trip + self-hosting.
//!
//! The core demonstrator: a grammar lifted from EBNF text, emitted, and re-lifted has an
//! IDENTICAL canonical form. Isomorphism is DEMONSTRATED by the equality of canonical trees,
//! not asserted by fiat — the same decidable-identity discipline as the `logic:` lens-law
//! spine ([`LegPath::normalize`]).

use gmeow_lang_bridge::{
    AbnfBridge, Bridge, EbnfBridge, Formalism, Grammar, GrammarRule, LangFailure, RuleExpr,
    exact_round_trip_holds, grammar_leg_pair, grammar_to_ntriples, is_exact_correspondence,
    parse_grammar,
};

/// The two shipped grammar sources, held under `slices/grounding/lang/grammars/`.
const TURTLE_EBNF: &str = include_str!("../../../slices/grounding/lang/grammars/turtle.ebnf");
const GTS_EBNF: &str = include_str!("../../../slices/grounding/lang/grammars/gts.ebnf");

/// The Gate-3 isomorphism demonstrator over one EBNF source: lift → emit → re-lift, and show
/// the canonical forms are equal. Returns the parsed grammar for further inspection.
fn assert_gate3_isomorphism(source: &str, label: &str) -> Grammar {
    let bridge = EbnfBridge;
    // Lift.
    let grammar = bridge.to_grammar(source.as_bytes()).expect("source lifts");
    let canon = grammar.canonicalize();

    // Emit the canonical form, then re-lift.
    let emitted = bridge.serialize(&canon);
    let relifted = bridge.parse(&emitted).expect("emitted grammar re-lifts");

    // DEMONSTRATED isomorphism: the canonical trees are equal.
    assert_eq!(
        relifted.canonicalize(),
        canon,
        "{label}: lift→emit→re-lift is not isomorphic"
    );

    // The ExactPreservation demonstrator: serialize(parse(text)) re-lifts isomorphically to
    // parse(text) — the round-trip on the raw source, not just the canonical form.
    let once = bridge.serialize(&grammar);
    let twice = bridge.parse(&once).expect("re-parse of a serialization");
    assert_eq!(
        twice.canonicalize(),
        grammar.canonicalize(),
        "{label}: serialize(parse(text)) is not isomorphic to parse(text)"
    );

    grammar
}

#[test]
fn gate3_turtle_grammar_round_trips_isomorphically() {
    let g = assert_gate3_isomorphism(TURTLE_EBNF, "turtle.ebnf");
    // The Turtle grammar is substantial: the round-trip is over the real thing.
    assert!(
        g.rules.len() >= 40,
        "turtle.ebnf should carry the full production set, got {}",
        g.rules.len()
    );
    assert_eq!(g.formalism, Formalism::Ebnf);
}

#[test]
fn gate3_gts_grammar_round_trips_isomorphically() {
    let g = assert_gate3_isomorphism(GTS_EBNF, "gts.ebnf");
    assert!(
        g.rules.len() >= 10,
        "gts.ebnf should carry the GTS surface productions, got {}",
        g.rules.len()
    );
    // The GTS triple-term production is present — the RDF-1.2 surface the codec interprets.
    assert!(
        g.rules.iter().any(|r| r.name == "tripleTerm"),
        "gts.ebnf must carry the triple-term production"
    );
}

#[test]
fn alternation_associativity_canonicalizes_equal() {
    let bridge = EbnfBridge;
    // `a | (b | c)` and `(a | b) | c` are the SAME grammar: alternation is associative, and the
    // canonical form flattens the nesting (mirroring LegPath::normalize's flatten discipline).
    let right = bridge.parse("r ::= a | (b | c)").expect("right-nested alt");
    let left = bridge.parse("r ::= (a | b) | c").expect("left-nested alt");
    assert_eq!(
        right.canonicalize(),
        left.canonicalize(),
        "alternation associativity must canonicalize equal"
    );
    // And both flatten to a single 3-way Alt.
    let RuleExpr::Alt(branches) = &right.canonicalize().rules[0].body else {
        panic!("expected a flattened Alt");
    };
    assert_eq!(branches.len(), 3, "nested alternation must flatten");
}

#[test]
fn nested_sequence_and_singleton_groups_canonicalize_away() {
    let bridge = EbnfBridge;
    // A singleton group and a nested sequence collapse: `a (b c)` == `a b c`.
    let grouped = bridge.parse("r ::= a (b c)").expect("grouped seq");
    let flat = bridge.parse("r ::= a b c").expect("flat seq");
    assert_eq!(grouped.canonicalize(), flat.canonicalize());
    // A singleton group unwraps: `(a)` == `a`.
    let wrapped = bridge.parse("r ::= (a)").expect("wrapped ref");
    let bare = bridge.parse("r ::= a").expect("bare ref");
    assert_eq!(wrapped.canonicalize(), bare.canonicalize());
}

#[test]
fn left_recursive_rule_round_trips() {
    let bridge = EbnfBridge;
    // A left-recursive production (the classic list rule) round-trips isomorphically — the
    // bridge lifts structure, so self-reference in the head position is no obstacle.
    let src = "list ::= list ',' item | item";
    let g = bridge.parse(src).expect("left-recursive rule lifts");
    let round = bridge
        .parse(&bridge.serialize(&g.canonicalize()))
        .expect("re-lift");
    assert_eq!(round.canonicalize(), g.canonicalize());
    // The rule genuinely references itself.
    let RuleExpr::Alt(branches) = &g.canonicalize().rules[0].body else {
        panic!("expected Alt");
    };
    let RuleExpr::Seq(first) = &branches[0] else {
        panic!("expected a Seq first branch");
    };
    assert_eq!(first[0], RuleExpr::Ref("list".to_owned()));
}

#[test]
fn carried_correspondence_is_exact_and_round_trip_holds() {
    let bridge = EbnfBridge;
    let lifted = bridge
        .lift(b"turtleDoc ::= statement*")
        .expect("a grammar lifts");
    // Exactness read off the carried correspondence — an isomorphism with discharged laws.
    assert!(
        is_exact_correspondence(&lifted.correspondence),
        "the grammar round-trip is an isomorphism with discharged laws"
    );
    // A grammar is not a Form-AST form.
    assert!(lifted.forms.is_empty(), "a grammar carries no forms");
    // The RDF projection is recorded as an exact ledger row.
    assert_eq!(lifted.ledger.len(), 1);
    assert!(lifted.ledger[0].is_rdf);
    // The decidable leg-level round-trip: put == get.invert().
    let (get, put) = grammar_leg_pair();
    assert!(exact_round_trip_holds(&get, &put));
    // Emit reproduces the canonical grammar text (surface round-trip).
    let emitted = bridge.emit(&lifted);
    assert_eq!(
        String::from_utf8(emitted).unwrap(),
        bridge.serialize(
            &bridge
                .to_grammar(b"turtleDoc ::= statement*")
                .unwrap()
                .canonicalize()
        )
    );
}

#[test]
fn hard_fail_on_unterminated_char_class() {
    let bridge = EbnfBridge;
    let diag = bridge
        .parse("r ::= [abc")
        .expect_err("an unterminated char class must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        diag.construct.contains("character class"),
        "the diagnostic must name the construct, got: {}",
        diag.construct
    );
}

#[test]
fn hard_fail_on_line_without_rule_separator() {
    let bridge = EbnfBridge;
    let diag = bridge
        .parse("this is not a rule")
        .expect_err("a line with no '::=' must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::SilentIngestDrop);
    assert!(diag.construct.contains("::="));
}

#[test]
fn hard_fail_on_non_utf8_input() {
    let diag = parse_grammar(&[0x41, 0xff, 0x42], Formalism::Ebnf)
        .expect_err("non-UTF-8 grammar bytes must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::NonUtf8Surface);
    assert!(diag.construct.contains("non-UTF-8"));
}

#[test]
fn grammar_to_ntriples_emits_grammar_and_one_rule_per_production_deterministically() {
    let bridge = EbnfBridge;
    let grammar = bridge
        .parse("turtleDoc ::= statement*\nstatement ::= directive | triples '.'")
        .expect("two-rule grammar lifts");
    let iri = "http://example.org/g";
    let a = grammar_to_ntriples(&grammar, iri);
    let b = grammar_to_ntriples(&grammar, iri);
    // Deterministic: byte-identical across runs.
    assert_eq!(a, b, "the N-Triples projection must be deterministic");
    let text = String::from_utf8(a).unwrap();
    // One lang:Grammar typing.
    assert_eq!(
        text.matches("<https://blackcatinformatics.ca/lang/Grammar>")
            .count(),
        1,
        "exactly one lang:Grammar typing"
    );
    // One lang:grammarFormalism edge, at the EBNF individual.
    assert!(text.contains("<https://blackcatinformatics.ca/lang/grammarFormalism> <https://blackcatinformatics.ca/lang/ebnfFormalism>"));
    // One lang:GrammarRule per production (two here).
    assert_eq!(
        text.matches("<https://blackcatinformatics.ca/lang/GrammarRule>")
            .count(),
        2,
        "one lang:GrammarRule per production"
    );
    // Each rule links back to the grammar.
    assert_eq!(
        text.matches("<https://blackcatinformatics.ca/lang/grammarRuleOf>")
            .count(),
        2
    );
}

#[test]
fn abnf_bridge_round_trips_isomorphically() {
    let bridge = AbnfBridge;
    // An RFC-5234 ABNF fragment exercising `/` alternation, prefix repetition (`*`, `1*`,
    // bounded `2*4`, exact `4`), optional `[...]`, grouping, quoted terminals, and `%x` numeric
    // terminals with a range.
    let src = "\
postal-code = 5DIGIT [ \"-\" 4DIGIT ]
token = 1*char
char = ALPHA / DIGIT / \"-\"
list = element *( \",\" element )
byte = %x00-FF
ALPHA = %x41-5A / %x61-7A
octet = 2*4HEXDIG
";
    let g = bridge.parse(src).expect("ABNF fragment lifts");
    assert_eq!(g.formalism, Formalism::Abnf);
    let round = bridge
        .parse(&bridge.serialize(&g.canonicalize()))
        .expect("ABNF re-lifts");
    assert_eq!(
        round.canonicalize(),
        g.canonicalize(),
        "ABNF lift→emit→re-lift is not isomorphic"
    );
    // The carried correspondence is exact.
    let lifted = bridge.lift(src.as_bytes()).expect("ABNF lifts");
    assert!(is_exact_correspondence(&lifted.correspondence));
}

#[test]
fn abnf_incremental_alternation_hard_fails() {
    let bridge = AbnfBridge;
    // `=/` (incremental alternation) is not modeled — it must hard-fail naming the construct,
    // never silently shadow the base rule.
    let diag = bridge
        .parse("rule =/ extra")
        .expect_err("ABNF '=/' must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::SilentIngestDrop);
    assert!(diag.construct.contains("=/"));
}

#[test]
fn abnf_star_and_ebnf_star_share_structure() {
    // Cross-formalism structural identity of the SAME construct: ABNF `*x` and EBNF `x*` lift to
    // the same Star body (formalism differs, but the body structure is identical).
    let ebnf = EbnfBridge
        .parse("r ::= x*")
        .expect("ebnf star")
        .canonicalize();
    let abnf = AbnfBridge
        .parse("r = *x")
        .expect("abnf star")
        .canonicalize();
    assert_eq!(
        ebnf.rules[0].body,
        GrammarRule {
            name: "r".to_owned(),
            body: RuleExpr::Star(Box::new(RuleExpr::Ref("x".to_owned()))),
        }
        .body
    );
    assert_eq!(abnf.rules[0].body, ebnf.rules[0].body);
}

#[test]
fn grammar_bridge_imports_no_rdf_parser_only_grammar_text() {
    // NO-SECOND-PARSER GATE (self-hosting realized semantically, not by rerouting a load-bearing
    // parse call site). The grammar bridge's source must not import or instantiate ANY RDF /
    // Turtle / GTS DOCUMENT parser: its only parser is the EBNF/ABNF GRAMMAR-TEXT parser, which
    // is not an RDF parser. The one RDF/GTS parser stack in the workspace is the external
    // `purrdf` crate; this test keeps the invariant honest by scanning the bridge source for any
    // reference to an RDF-document parser stack.
    let grammar_src = include_str!("../src/grammar.rs");
    // Scan CODE only — strip line comments so the invariant's own prose (which names `purrdf`
    // as the single sanctioned RDF parser) does not trip the gate. The gate is about what the
    // bridge IMPORTS and INSTANTIATES, not what it documents.
    let code: String = grammar_src
        .lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    for needle in [
        "purrdf",
        "native_codecs",
        "oxigraph",
        "gmeow_rdf",
        "parse_turtle",
        "parse_ntriples",
        "TurtleParser",
        "fn tokenize", // no hand-rolled RDF tokenizer
    ] {
        assert!(
            !code.contains(needle),
            "the grammar bridge must not reference an RDF/GTS document parser ('{needle}'): it \
             parses grammar TEXT, never RDF documents"
        );
    }
    // Positive: it DOES define the grammar-text parser.
    assert!(code.contains("fn parse_grammar"));
    assert!(code.contains("struct ExprParser"));
}

/// Strip Rust line comments (`//`, `///`, `//!`) and block comments (`/* … */`) from `src`,
/// leaving CODE only. String and char literals are respected so that a `//` inside a string
/// (e.g. an IRI like `"https://…"`) or a `/*` inside a literal is NOT mistaken for a comment —
/// that keeps the scan from silently dropping a line of real code (a false negative). The
/// invariant's own PROSE (which legitimately names `purrdf`, `oxigraph`, etc. when explaining
/// the single-sanctioned-parser rule) lives in comments, so stripping them is what lets the
/// gate scan intent (imports/instantiations) rather than documentation.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    #[derive(PartialEq)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        Str,
        Char,
    }
    let mut state = State::Code;
    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();
        match state {
            State::Code => {
                if b == b'/' && next == Some(b'/') {
                    state = State::LineComment;
                    i += 2;
                } else if b == b'/' && next == Some(b'*') {
                    state = State::BlockComment;
                    i += 2;
                } else if b == b'"' {
                    state = State::Str;
                    out.push('"');
                    i += 1;
                } else if b == b'\'' {
                    state = State::Char;
                    out.push('\'');
                    i += 1;
                } else {
                    out.push(b as char);
                    i += 1;
                }
            }
            State::LineComment => {
                if b == b'\n' {
                    state = State::Code;
                    out.push('\n');
                }
                i += 1;
            }
            State::BlockComment => {
                if b == b'*' && next == Some(b'/') {
                    state = State::Code;
                    i += 2;
                } else {
                    // Preserve newlines so line structure (and any diagnostics) stays sane.
                    if b == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
            State::Str => {
                out.push(b as char);
                if b == b'\\' {
                    // Escaped byte — copy it verbatim, do not let it end the string.
                    if let Some(n) = next {
                        out.push(n as char);
                        i += 2;
                        continue;
                    }
                } else if b == b'"' {
                    state = State::Code;
                }
                i += 1;
            }
            State::Char => {
                out.push(b as char);
                if b == b'\\' {
                    if let Some(n) = next {
                        out.push(n as char);
                        i += 2;
                        continue;
                    }
                } else if b == b'\'' {
                    state = State::Code;
                }
                i += 1;
            }
        }
    }
    out
}

/// The rival-parser needle set: identifiers that would indicate a SECOND / rival RDF-document
/// parser stack in the crate. `purrdf` and `native_codecs` are DELIBERATELY absent — they are the
/// ONE sanctioned parser stack and are legitimately used crate-wide (e.g. `ontolex.rs` lifts FROM
/// RDF via `purrdf::parse_dataset`). These needles are rival-crate names and parser TYPE names
/// that never appear in the sanctioned `purrdf` API surface.
const RIVAL_PARSER_NEEDLES: [&str; 11] = [
    "oxigraph",
    "rio_turtle",
    "rio_api",
    "rio_xml",
    "sophia",
    "rdftk",
    "TurtleParser",
    "NTriplesParser",
    "TriGParser",
    "RdfXmlParser",
    "hdt", // the HDT (Header-Dictionary-Triples) rival RDF stack
];

/// Return `Some(needle)` if `code` (comments already stripped) references a rival RDF-parser
/// stack; `None` if it is clean. Split out so the gate can be self-verified with a synthetic hit.
fn rival_parser_hit(code: &str) -> Option<&'static str> {
    RIVAL_PARSER_NEEDLES
        .into_iter()
        .find(|needle| code.contains(needle))
}

/// Recursively collect every `*.rs` file under `dir` in deterministic (sorted) order.
fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| {
            panic!(
                "lang-bridge src subdir {} is not readable: {e}",
                dir.display()
            )
        })
        .map(|entry| entry.expect("dir entry resolves").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_rival_rdf_parser_stack_anywhere_in_the_crate() {
    // CRATE-WIDE NO-SECOND-PARSER GATE. The single sanctioned RDF/GTS parser stack in the
    // workspace is `purrdf` / `native_codecs`; bridges may USE it (e.g. `ontolex.rs` lifts FROM
    // RDF via `purrdf::parse_dataset`), but no lang-bridge module may introduce a RIVAL parser.
    // Unlike the grammar-bridge-specific gate (which forbids even referencing `purrdf` because
    // the grammar bridge parses grammar TEXT, never documents), this gate is crate-wide and so
    // must NOT ban the sanctioned stack — only rival stacks. It enumerates every `.rs` file under
    // `src/` at test time, so a NEWLY-ADDED module is automatically covered.

    // Self-verification: the detector must actually fire on a rival needle — the gate is not
    // vacuous. (Run before scanning so a broken detector fails loudly regardless of src state.)
    assert_eq!(
        rival_parser_hit("let p = oxigraph::TurtleParser::new();"),
        Some("oxigraph"),
        "the rival-parser detector must fire on a synthetic rival needle"
    );
    // And it must NOT fire on legitimate sanctioned-parser use.
    assert_eq!(
        rival_parser_hit("let ds = purrdf::parse_dataset(&bytes, \"text/turtle\", None)?;"),
        None,
        "the sanctioned purrdf parser must never be treated as a rival stack"
    );

    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(
        src_dir.is_dir(),
        "lang-bridge src/ must exist to enforce the no-second-parser invariant, missing: {}",
        src_dir.display()
    );
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);
    // A missing/empty src is a HARD FAIL — the gate must never silently pass on nothing.
    assert!(
        !files.is_empty(),
        "lang-bridge src/ contained no .rs files to scan for a rival RDF parser stack: {}",
        src_dir.display()
    );

    for path in &files {
        let raw = std::fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("lang-bridge module {} is not readable: {e}", path.display())
        });
        let code = strip_comments(&raw);
        if let Some(needle) = rival_parser_hit(&code) {
            panic!(
                "module {} references a rival RDF parser stack ('{needle}'): the ONE sanctioned \
                 RDF/GTS parser stack is `purrdf`/`native_codecs` — bridges may USE it but must \
                 never introduce a SECOND parser",
                path.display()
            );
        }
    }
}
