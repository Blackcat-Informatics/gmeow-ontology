// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn yaml_scalar_plain_when_safe() {
    assert_eq!(yaml_scalar("Dog"), "Dog");
    // A colon NOT followed by a space is a legal plain scalar (CURIEs, IRIs).
    assert_eq!(yaml_scalar("gmeow:Dog"), "gmeow:Dog");
    assert_eq!(
        yaml_scalar("https://example.org/Dog"),
        "https://example.org/Dog"
    );
    // Multi-dot version strings are not numbers — stay plain.
    assert_eq!(yaml_scalar("1.0.0"), "1.0.0");
    // Unicode is emitted directly (allow_unicode).
    assert_eq!(yaml_scalar("café"), "café");
}

#[test]
fn yaml_scalar_single_quotes_plain_unsafe() {
    assert_eq!(yaml_scalar(""), "''");
    assert_eq!(yaml_scalar("yes"), "'yes'");
    assert_eq!(yaml_scalar("Null"), "'Null'");
    assert_eq!(yaml_scalar("42"), "'42'");
    // Exponent / non-finite floats would resolve to numbers if left plain.
    assert_eq!(yaml_scalar("1e3"), "'1e3'");
    assert_eq!(yaml_scalar("inf"), "'inf'");
    assert_eq!(yaml_scalar("nan"), "'nan'");
    // Indicator-led, mid-string `: `, and trailing `:`.
    assert_eq!(yaml_scalar("- leading"), "'- leading'");
    assert_eq!(yaml_scalar("key: value"), "'key: value'");
    assert_eq!(yaml_scalar("trailing:"), "'trailing:'");
    // An apostrophe is legal mid-plain-scalar — it needs no quoting on its own.
    assert_eq!(yaml_scalar("it's"), "it's");
    // But when the value is single-quoted for another reason, it doubles.
    assert_eq!(yaml_scalar("key: it's"), "'key: it''s'");
}

#[test]
fn yaml_scalar_quotes_sexagesimal() {
    // YAML 1.1 folds these to int/float (e.g. `12:30` → 750) unless quoted.
    assert_eq!(yaml_scalar("12:30"), "'12:30'");
    assert_eq!(yaml_scalar("1:2:3"), "'1:2:3'");
    assert_eq!(yaml_scalar("12:30:00"), "'12:30:00'");
    assert_eq!(yaml_scalar("12:30.5"), "'12:30.5'");
    // A non-numeric head or an out-of-range base-60 field is NOT sexagesimal.
    assert_eq!(yaml_scalar("a:b:c"), "a:b:c");
    assert_eq!(yaml_scalar("12:99"), "12:99");
    // A leading underscore is not a valid base-60 first field — stays plain.
    assert_eq!(yaml_scalar("_12:30"), "_12:30");
}

#[test]
fn yaml_scalar_quotes_yaml11_number_forms() {
    // Radix integers and underscore digit groups: a YAML 1.1 reader folds these
    // to integers/floats, but Rust's `f64::parse` rejects them, so a bare
    // emission would silently change type on read.
    for n in ["0x1f", "0b101", "0o17", "1_000", "3_000.5"] {
        assert_eq!(yaml_scalar(n), format!("'{n}'"), "{n} must be quoted");
    }
    // Special-float spellings (`.inf` / `.nan` family, any case).
    for n in [".inf", "+.inf", "-.inf", ".nan", ".NaN"] {
        assert_eq!(yaml_scalar(n), format!("'{n}'"), "{n} must be quoted");
    }
    // Single-character booleans `y` / `n` resolve to bool in YAML 1.1.
    for n in ["y", "n", "Y", "N"] {
        assert_eq!(yaml_scalar(n), format!("'{n}'"), "{n} must be quoted");
    }
    // Underscored identifiers / CURIE locals are NOT numbers — stay plain.
    assert_eq!(yaml_scalar("has_part"), "has_part");
    assert_eq!(yaml_scalar("P1_2"), "P1_2");
}

#[test]
fn yaml_scalar_quotes_timestamps() {
    // YAML 1.1 resolves a `YYYY-M-D` lead to a timestamp, not a string.
    assert_eq!(yaml_scalar("2001-12-14"), "'2001-12-14'");
    assert_eq!(
        yaml_scalar("2001-12-14T10:00:00Z"),
        "'2001-12-14T10:00:00Z'"
    );
    assert_eq!(yaml_scalar("2001-1-1 10:00:00"), "'2001-1-1 10:00:00'");
    // A dotted version string is not a date, and a year with a non-date tail
    // is not a timestamp lead — both stay plain.
    assert_eq!(yaml_scalar("1.0.0"), "1.0.0");
    assert_eq!(yaml_scalar("2001-mixed"), "2001-mixed");
}

#[test]
fn yaml_scalar_double_quotes_control_chars() {
    // A multi-line definition is ENCODED (formerly a hard build failure).
    assert_eq!(yaml_scalar("line one\nline two"), "\"line one\\nline two\"");
    assert_eq!(yaml_scalar("a\tb"), "\"a\\tb\"");
    assert_eq!(yaml_scalar("a\rb"), "\"a\\rb\"");
    // A bell (U+0007) escapes as \x07.
    assert_eq!(yaml_scalar("a\u{7}b"), "\"a\\x07b\"");
    // Double quotes and backslashes escape inside the double-quoted form.
    assert_eq!(yaml_scalar("say \"hi\"\nbye"), "\"say \\\"hi\\\"\\nbye\"");
}
