// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_lang_bridge::registry::{LangProjectionInput, registry};

#[test]
fn native_reparse_is_bound_to_the_actual_emitted_bytes() {
    let source = GrammarSource::parse("tiny", b"r ::= 'a' | 'b'\n").unwrap();
    let input = LangProjectionInput {
        grammars: vec![source],
        ..Default::default()
    };
    let target = registry()
        .into_iter()
        .find(|target| target.name() == "ebnf")
        .unwrap();
    let mut emissions = target.emit(&input).unwrap();
    let mut observations = Observations::new(&input.grammars);
    observations.record("ebnf", &emissions[0]).unwrap();
    let observed = &observations.grammars["tiny"];
    assert_eq!(
        observed.emissions["ebnf"][0].ebnf_reparsed[0],
        Ok(observed.canonical.clone())
    );
    emissions[0].artifacts[0].bytes.push(b' ');
    assert!(
        observations.record("ebnf", &emissions[0]).is_err(),
        "equivalent text still has a different byte identity"
    );
    emissions[0].grammar_reparse = None;
    assert!(
        observations.record("ebnf", &emissions[0]).is_err(),
        "a selected grammar cannot omit its native result"
    );
}
