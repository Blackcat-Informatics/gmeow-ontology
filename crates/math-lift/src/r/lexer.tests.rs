// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn kinds(src: &str) -> Vec<Tok> {
    lex(src)
        .expect("lexes")
        .into_iter()
        .map(|t| t.tok)
        .collect()
}

#[test]
fn comments_and_whitespace_carry_no_tokens() {
    assert_eq!(kinds("# a comment only"), Vec::<Tok>::new());
    assert_eq!(
        kinds("x # trailing\n"),
        vec![Tok::Ident("x".to_owned()), Tok::Newline]
    );
}

#[test]
fn strings_resolve_both_quote_styles_and_escapes() {
    assert_eq!(
        kinds(r#""a\tb" 'c\'d' "\u{263A}""#),
        vec![
            Tok::Str("a\tb".to_owned()),
            Tok::Str("c'd".to_owned()),
            Tok::Str("\u{263a}".to_owned()),
        ]
    );
}

#[test]
fn an_unterminated_string_is_a_positioned_hard_failure() {
    let err = lex("x <- \"oops").expect_err("must not lex");
    let text = format!("{err}");
    assert!(text.contains("line 1"), "{text}");
    assert!(text.contains("unterminated string literal"), "{text}");
}

#[test]
fn numbers_cover_decimals_exponents_and_the_integer_suffix() {
    assert_eq!(
        kinds("1 2.5 1e5 3L .5"),
        vec![
            Tok::Number {
                value: 1.0,
                integer: false,
                text: "1.0".to_owned()
            },
            Tok::Number {
                value: 2.5,
                integer: false,
                text: "2.5".to_owned()
            },
            Tok::Number {
                value: 100_000.0,
                integer: false,
                text: "100000.0".to_owned()
            },
            Tok::Number {
                value: 3.0,
                integer: true,
                text: "3.0".to_owned()
            },
            Tok::Number {
                value: 0.5,
                integer: false,
                text: "0.5".to_owned()
            },
        ]
    );
}

#[test]
fn a_scanned_number_never_renders_in_exponent_form() {
    assert_eq!(format_source_decimal(1e5), "100000.0");
    assert_eq!(format_source_decimal(1e-7), "0.0000001");
    assert_eq!(format_source_decimal(2.0), "2.0");
}

#[test]
fn the_literal_keywords_are_their_own_tokens() {
    assert_eq!(
        kinds("TRUE FALSE NA NULL NaN Inf"),
        vec![
            Tok::True,
            Tok::False,
            Tok::Na,
            Tok::Null,
            Tok::NotANumber,
            Tok::Infinity
        ]
    );
}

#[test]
fn assignment_arrows_fuse_only_on_adjacency() {
    assert_eq!(
        kinds("x <- 1\n"),
        vec![
            Tok::Ident("x".to_owned()),
            Tok::Op(Op::Assign),
            Tok::Number {
                value: 1.0,
                integer: false,
                text: "1.0".to_owned()
            },
            Tok::Newline,
        ]
    );
    assert_eq!(
        kinds("x < -1\n"),
        vec![
            Tok::Ident("x".to_owned()),
            Tok::Op(Op::Less),
            Tok::Op(Op::Minus),
            Tok::Number {
                value: 1.0,
                integer: false,
                text: "1.0".to_owned()
            },
            Tok::Newline,
        ]
    );
    assert_eq!(
        kinds("1 -> x\n"),
        vec![
            Tok::Number {
                value: 1.0,
                integer: false,
                text: "1.0".to_owned()
            },
            Tok::Op(Op::RightAssign),
            Tok::Ident("x".to_owned()),
            Tok::Newline,
        ]
    );
    assert_eq!(kinds("x <<- 1")[1], Tok::Op(Op::SuperAssign));
    assert_eq!(kinds("1 ->> x")[1], Tok::Op(Op::SuperRightAssign));
}

#[test]
fn a_newline_inside_parens_or_brackets_is_suppressed() {
    assert!(!kinds("f(a,\n b)").contains(&Tok::Newline));
    assert!(!kinds("x[1,\n 2]").contains(&Tok::Newline));
    // A brace block's newlines DO separate its statements.
    assert_eq!(
        kinds("{\na\n}")
            .iter()
            .filter(|t| **t == Tok::Newline)
            .count(),
        2
    );
}

#[test]
fn double_brackets_open_twice_so_the_close_pair_balances() {
    assert_eq!(
        kinds("x[[1]]\n"),
        vec![
            Tok::Ident("x".to_owned()),
            Tok::DoubleLBracket,
            Tok::Number {
                value: 1.0,
                integer: false,
                text: "1.0".to_owned()
            },
            Tok::RBracket,
            Tok::RBracket,
            Tok::Newline,
        ]
    );
    assert!(!kinds("x[[1,\n2]]").contains(&Tok::Newline));
}

#[test]
fn percent_operators_scan_as_one_token_with_their_text() {
    assert_eq!(
        kinds("a %>% b %in% c %/% d\n"),
        vec![
            Tok::Ident("a".to_owned()),
            Tok::Special("%>%".to_owned()),
            Tok::Ident("b".to_owned()),
            Tok::Special("%in%".to_owned()),
            Tok::Ident("c".to_owned()),
            Tok::Special("%/%".to_owned()),
            Tok::Ident("d".to_owned()),
            Tok::Newline,
        ]
    );
    let err = lex("a %oops\n").expect_err("must not lex");
    assert!(format!("{err}").contains("unterminated %"), "{err}");
}

#[test]
fn a_bare_dot_is_an_identifier_but_a_dotted_number_is_a_number() {
    assert_eq!(kinds(". x")[0], Tok::Ident(".".to_owned()));
    assert!(matches!(kinds(".5")[0], Tok::Number { .. }));
}

#[test]
fn a_backtick_name_scans_as_one_identifier() {
    assert_eq!(
        kinds("`my var` <- 1")[0],
        Tok::Ident("my var".to_owned()),
        "backtick names carry spaces"
    );
    assert!(lex("`unterminated").is_err());
}

#[test]
fn an_illegal_character_is_a_positioned_hard_failure() {
    let err = lex("x <- 1\ny \u{a7} 2").expect_err("must not lex");
    let text = format!("{err}");
    assert!(text.contains("line 2"), "{text}");
    assert!(text.contains("not part of R's grammar"), "{text}");
}

#[test]
fn namespace_and_pipe_operators_scan_at_full_length() {
    assert_eq!(kinds("stats::lm")[1], Tok::Op(Op::DoubleColon));
    assert_eq!(kinds("stats:::lm")[1], Tok::Op(Op::TripleColon));
    assert_eq!(kinds("a |> b")[1], Tok::Op(Op::NativePipe));
    assert_eq!(kinds("a:b")[1], Tok::Op(Op::Colon));
}
