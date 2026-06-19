// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `gts → nquads` transform (§14) — mirror of
//! `src/gmeow_tools/gts/nquads.py`.
//!
//! Serialises the folded base quads, plus reifier/annotation triples in the
//! RDF 1.2 reifying style (`<reifier> rdf:reifies <<( s p o )>>` and
//! `<reifier> p v`). Inline blobs are externalised by the caller; this module
//! emits the graph text only.

use crate::model::{Graph, TermKind};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Escape a literal lexical form for N-Triples (incl. all C0 control chars).
fn escape(lex: &str) -> String {
    let mut out = String::with_capacity(lex.len());
    for ch in lex.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Render a term-id as an N-Triples token.
fn render(g: &Graph, tid: usize) -> String {
    let t = &g.terms[tid];
    match t.kind {
        TermKind::Iri => format!("<{}>", t.value.as_deref().unwrap_or("")),
        TermKind::Bnode => match &t.value {
            Some(v) => format!("_:{v}"),
            None => format!("_:b{tid}"),
        },
        TermKind::Literal => {
            let lit = format!("\"{}\"", escape(t.value.as_deref().unwrap_or("")));
            if let Some(lang) = &t.lang {
                format!("{lit}@{lang}")
            } else if let Some(dt) = t.datatype {
                format!("{lit}^^{}", render(g, dt))
            } else {
                lit // plain literal == xsd:string (§7.1)
            }
        }
        // quoted triple (RDF 1.2 triple term), resolved through its reifier
        TermKind::Triple => match t.reifier.and_then(|rf| g.reifier(rf)) {
            Some((s, p, o)) => {
                format!("<<( {} {} {} )>>", render(g, s), render(g, p), render(g, o))
            }
            // degraded but syntactically valid: an unbound reifier becomes a
            // blank node
            None => format!("_:unbound_triple_{tid}"),
        },
    }
}

/// Serialise a folded [`Graph`] to N-Quads text.
pub fn to_nquads(g: &Graph) -> String {
    let mut lines: Vec<String> = Vec::new();
    for &(s, p, o, gname) in &g.quads {
        let triple = format!("{} {} {}", render(g, s), render(g, p), render(g, o));
        match gname {
            Some(gv) => lines.push(format!("{triple} {} .", render(g, gv))),
            None => lines.push(format!("{triple} .")),
        }
    }
    for &(rid, (s, p, o)) in &g.reifiers {
        let quoted = format!("<<( {} {} {} )>>", render(g, s), render(g, p), render(g, o));
        lines.push(format!("{} <{RDF_REIFIES}> {quoted} .", render(g, rid)));
    }
    for &(r, p, v) in &g.annotations {
        lines.push(format!(
            "{} {} {} .",
            render(g, r),
            render(g, p),
            render(g, v)
        ));
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}
