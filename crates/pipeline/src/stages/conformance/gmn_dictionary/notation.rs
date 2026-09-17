// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Observe each surface of the selected dictionary's native glyph signature.

use std::collections::BTreeMap;

use gmeow_lang_bridge::{
    Formalism, GmnDictionary, Grammar, GrammarRule, IngestDiagnostic, parse_grammar,
    serialize_grammar,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Notation {
    pub production: String,
    pub canonical: Vec<GrammarRule>,
    pub views: BTreeMap<String, Result<Vec<GrammarRule>, IngestDiagnostic>>,
}

pub(super) fn record(dictionary: &GmnDictionary) -> Result<Notation, IngestDiagnostic> {
    let production = dictionary.glyph_registry().render_glyph_token_production();
    let canonical = parse_grammar(production.as_bytes(), Formalism::Ebnf)?
        .canonicalize()
        .rules;
    let mut views = BTreeMap::new();
    for formalism in [
        Formalism::Ebnf,
        Formalism::Abnf,
        Formalism::Gbnf,
        Formalism::Lark,
    ] {
        let view = Grammar {
            formalism,
            rules: canonical.clone(),
        };
        let serialized = serialize_grammar(&view);
        let reparsed = parse_grammar(serialized.as_bytes(), formalism)
            .map(|grammar| grammar.canonicalize().rules);
        views.insert(format!("{formalism:?}"), reparsed);
    }
    Ok(Notation {
        production,
        canonical,
        views,
    })
}
