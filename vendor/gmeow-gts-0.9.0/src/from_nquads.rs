// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `nquads -> gts` transform: the inverse of the §14 fold projection.
//!
//! This parser accepts the N-Quads(-star) text emitted by [`crate::nquads`]
//! and builds a canonical GTS segment with the shared writer semantics. Blobs,
//! suppressions, and opaque frames are not expressible in N-Quads and are
//! intentionally out of scope, matching the Python reference implementation.

use std::collections::HashMap;
use std::fmt;

use crate::model::{Quad, Term, TermKind, Triple3};
use crate::writer::Writer;

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Raised when N-Quads(-star) input is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NQuadsParseError {
    detail: String,
}

impl NQuadsParseError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for NQuadsParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for NQuadsParseError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Atom {
    kind: TermKind,
    value: String,
    lang: Option<String>,
    datatype: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TripleNode {
    s: Box<Node>,
    p: Box<Node>,
    o: Box<Node>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Atom(Atom),
    Triple(TripleNode),
}

struct Tokenizer<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn skip_ws(&mut self) {
        while matches!(self.text.as_bytes().get(self.pos), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip_ws();
        self.pos >= self.text.len() || self.text.as_bytes()[self.pos] == b'.'
    }

    fn node(&mut self) -> Result<Node, NQuadsParseError> {
        self.skip_ws();
        if self.pos >= self.text.len() {
            return Err(NQuadsParseError::new(format!(
                "unexpected end of line: {:?}",
                self.text
            )));
        }
        if self.text[self.pos..].starts_with("<<(") {
            return self.quoted_triple().map(Node::Triple);
        }
        match self.peek_char() {
            Some('<') => Ok(Node::Atom(Atom {
                kind: TermKind::Iri,
                value: self.iri()?,
                lang: None,
                datatype: None,
            })),
            Some('_') => Ok(Node::Atom(Atom {
                kind: TermKind::Bnode,
                value: self.bnode()?,
                lang: None,
                datatype: None,
            })),
            Some('"') => self.literal().map(Node::Atom),
            _ => Err(NQuadsParseError::new(format!(
                "unexpected token at {} in {:?}",
                self.pos, self.text
            ))),
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn iri(&mut self) -> Result<String, NQuadsParseError> {
        if self.text.as_bytes().get(self.pos) != Some(&b'<') {
            return Err(NQuadsParseError::new(format!("bad IRI in {:?}", self.text)));
        }
        let start = self.pos + 1;
        let rel = self.text[start..]
            .find('>')
            .ok_or_else(|| NQuadsParseError::new(format!("unterminated IRI in {:?}", self.text)))?;
        let end = start + rel;
        self.pos = end + 1;
        Ok(self.text[start..end].to_string())
    }

    fn bnode(&mut self) -> Result<String, NQuadsParseError> {
        if !self.text[self.pos..].starts_with("_:") {
            return Err(NQuadsParseError::new(format!(
                "bad blank node in {:?}",
                self.text
            )));
        }
        self.pos += 2;
        let start = self.pos;
        while self.pos < self.text.len() && !matches!(self.text.as_bytes()[self.pos], b' ' | b'\t')
        {
            self.pos += 1;
        }
        Ok(self.text[start..self.pos].to_string())
    }

    fn literal(&mut self) -> Result<Atom, NQuadsParseError> {
        if self.bump_char() != Some('"') {
            return Err(NQuadsParseError::new(format!(
                "bad literal in {:?}",
                self.text
            )));
        }
        let mut value = String::new();
        loop {
            let Some(ch) = self.bump_char() else {
                return Err(NQuadsParseError::new(format!(
                    "unterminated literal in {:?}",
                    self.text
                )));
            };
            match ch {
                '\\' => value.push(self.escape()?),
                '"' => break,
                _ => value.push(ch),
            }
        }

        let mut lang = None;
        let mut datatype = None;
        if self.text.as_bytes().get(self.pos) == Some(&b'@') {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.text.len()
                && !matches!(self.text.as_bytes()[self.pos], b' ' | b'\t')
            {
                self.pos += 1;
            }
            lang = Some(self.text[start..self.pos].to_string());
        } else if self.text[self.pos..].starts_with("^^") {
            self.pos += 2;
            self.skip_ws();
            datatype = Some(self.iri()?);
        }

        Ok(Atom {
            kind: TermKind::Literal,
            value,
            lang,
            datatype,
        })
    }

    fn escape(&mut self) -> Result<char, NQuadsParseError> {
        let Some(ch) = self.bump_char() else {
            return Err(NQuadsParseError::new(format!(
                "bad escape at end of {:?}",
                self.text
            )));
        };
        match ch {
            '\\' => Ok('\\'),
            '"' => Ok('"'),
            'b' => Ok('\u{0008}'),
            'f' => Ok('\u{000c}'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            'u' | 'U' => {
                let width = if ch == 'u' { 4 } else { 8 };
                let end = self.pos + width;
                if end > self.text.len() || !self.text.is_char_boundary(end) {
                    return Err(NQuadsParseError::new(format!(
                        "short or invalid unicode escape in {:?}",
                        self.text
                    )));
                }
                let raw = &self.text[self.pos..end];
                if !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(NQuadsParseError::new(format!(
                        "bad unicode escape \\{ch}{raw} in {:?}",
                        self.text
                    )));
                }
                self.pos += width;
                let code = u32::from_str_radix(raw, 16).map_err(|e| {
                    NQuadsParseError::new(format!("bad unicode escape \\{ch}{raw}: {e}"))
                })?;
                char::from_u32(code).ok_or_else(|| {
                    NQuadsParseError::new(format!("invalid unicode scalar \\{ch}{raw}"))
                })
            }
            other => Err(NQuadsParseError::new(format!(
                "unsupported escape \\{other} in {:?}",
                self.text
            ))),
        }
    }

    fn quoted_triple(&mut self) -> Result<TripleNode, NQuadsParseError> {
        self.pos += 3;
        let s = self.node()?;
        let p = self.node()?;
        let o = self.node()?;
        self.skip_ws();
        if !self.text[self.pos..].starts_with(")>>") {
            return Err(NQuadsParseError::new(format!(
                "unterminated quoted triple in {:?}",
                self.text
            )));
        }
        self.pos += 3;
        Ok(TripleNode {
            s: Box::new(s),
            p: Box::new(p),
            o: Box::new(o),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TermKey {
    Atom {
        kind: TermKind,
        value: String,
        lang: Option<String>,
        datatype: Option<String>,
    },
    Triple(usize, usize, usize),
}

struct Interner {
    ids: HashMap<TermKey, usize>,
    terms: Vec<Term>,
}

impl Interner {
    fn new() -> Self {
        Self {
            ids: HashMap::new(),
            terms: Vec::new(),
        }
    }

    fn atom(&mut self, atom: &Atom) -> usize {
        let key = TermKey::Atom {
            kind: atom.kind,
            value: atom.value.clone(),
            lang: atom.lang.clone(),
            datatype: atom.datatype.clone(),
        };
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let datatype = if atom.kind == TermKind::Literal {
            atom.datatype.as_ref().map(|iri| {
                self.atom(&Atom {
                    kind: TermKind::Iri,
                    value: iri.clone(),
                    lang: None,
                    datatype: None,
                })
            })
        } else {
            None
        };
        let id = self.terms.len();
        self.terms.push(Term {
            kind: atom.kind,
            value: Some(atom.value.clone()),
            datatype,
            lang: atom.lang.clone(),
            reifier: None,
        });
        self.ids.insert(key, id);
        id
    }

    fn node(&mut self, node: &Node, reifiers: &mut Vec<(usize, Triple3)>) -> usize {
        match node {
            Node::Atom(atom) => self.atom(atom),
            Node::Triple(triple) => {
                let s = self.node(&triple.s, reifiers);
                let p = self.node(&triple.p, reifiers);
                let o = self.node(&triple.o, reifiers);
                let key = TermKey::Triple(s, p, o);
                if let Some(id) = self.ids.get(&key) {
                    return *id;
                }
                let id = self.terms.len();
                self.terms.push(Term {
                    kind: TermKind::Triple,
                    value: None,
                    datatype: None,
                    lang: None,
                    reifier: Some(id),
                });
                self.ids.insert(key, id);
                set_reifier(reifiers, id, (s, p, o));
                id
            }
        }
    }
}

fn set_reifier(reifiers: &mut Vec<(usize, Triple3)>, rid: usize, spo: Triple3) {
    if let Some((_, existing)) = reifiers.iter_mut().find(|(r, _)| *r == rid) {
        *existing = spo;
    } else {
        reifiers.push((rid, spo));
    }
}

fn validate_statement(nodes: &[Node], line: &str) -> Result<(), NQuadsParseError> {
    let is_iri = |node: &Node| {
        matches!(
            node,
            Node::Atom(Atom {
                kind: TermKind::Iri,
                ..
            })
        )
    };
    let is_bnode = |node: &Node| {
        matches!(
            node,
            Node::Atom(Atom {
                kind: TermKind::Bnode,
                ..
            })
        )
    };
    let is_literal = |node: &Node| {
        matches!(
            node,
            Node::Atom(Atom {
                kind: TermKind::Literal,
                ..
            })
        )
    };
    let is_triple = |node: &Node| matches!(node, Node::Triple(_));

    if !(is_iri(&nodes[0]) || is_bnode(&nodes[0]) || is_triple(&nodes[0])) {
        return Err(NQuadsParseError::new(format!(
            "invalid subject term: {line:?}"
        )));
    }
    if !is_iri(&nodes[1]) {
        return Err(NQuadsParseError::new(format!(
            "predicate must be IRI: {line:?}"
        )));
    }
    if !(is_iri(&nodes[2]) || is_bnode(&nodes[2]) || is_literal(&nodes[2]) || is_triple(&nodes[2]))
    {
        return Err(NQuadsParseError::new(format!(
            "invalid object term: {line:?}"
        )));
    }
    if let Some(graph_name) = nodes.get(3) {
        if !(is_iri(graph_name) || is_bnode(graph_name)) {
            return Err(NQuadsParseError::new(format!(
                "invalid graph name term: {line:?}"
            )));
        }
    }
    Ok(())
}

/// Parse N-Quads(-star) text into a canonical GTS file.
pub fn from_nquads(text: &str) -> Result<Vec<u8>, NQuadsParseError> {
    let mut statements: Vec<Vec<Node>> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut tokenizer = Tokenizer::new(line);
        let mut nodes = Vec::new();
        while !tokenizer.at_end() {
            nodes.push(tokenizer.node()?);
        }
        if !(nodes.len() == 3 || nodes.len() == 4) {
            return Err(NQuadsParseError::new(format!(
                "expected 3 or 4 terms, got {}: {:?}",
                nodes.len(),
                line
            )));
        }
        validate_statement(&nodes, line)?;
        statements.push(nodes);
    }

    let mut interner = Interner::new();
    let mut reifiers: Vec<(usize, Triple3)> = Vec::new();
    let mut quads: Vec<Quad> = Vec::new();

    for nodes in &statements {
        let s = &nodes[0];
        let p = &nodes[1];
        let o = &nodes[2];
        let gname = nodes.get(3);

        if let (Node::Atom(subject), Node::Atom(predicate), Node::Triple(object), None) =
            (s, p, o, gname)
        {
            if predicate.value == RDF_REIFIES {
                let rid = interner.atom(subject);
                let ss = interner.node(&object.s, &mut reifiers);
                let pp = interner.node(&object.p, &mut reifiers);
                let oo = interner.node(&object.o, &mut reifiers);
                set_reifier(&mut reifiers, rid, (ss, pp, oo));
                continue;
            }
        }

        let sid = interner.node(s, &mut reifiers);
        let pid = interner.node(p, &mut reifiers);
        let oid = interner.node(o, &mut reifiers);
        let gid = gname.map(|node| interner.node(node, &mut reifiers));
        quads.push((sid, pid, oid, gid));
    }

    let mut writer = Writer::new("dist");
    if !interner.terms.is_empty() {
        writer.add_terms(&interner.terms);
    }
    if !quads.is_empty() {
        writer.add_quads(&quads);
    }
    if !reifiers.is_empty() {
        writer.add_reifies(&reifiers);
    }
    Ok(writer.to_bytes())
}
