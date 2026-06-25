// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The public parse entry point and the recursive-descent parser that turns a
//! SPARQL 1.1/1.2 query into the [`Query`] algebra.
//!
//! The parser translates *directly* into the W3C SPARQL algebra (§18.2) rather
//! than building a separate syntax tree: group graph patterns accumulate into
//! `Join`/`LeftJoin`/`Filter`/`Extend`/`Union`/`Minus`/`Graph`, solution
//! modifiers wrap the result as `Group`/`OrderBy`/`Project`/`Distinct`/`Slice`,
//! and aggregates are lifted to synthetic variables in a `Group` node (the
//! standard §18.2.4 mechanism). Anything outside the corpus-driven scope is a
//! hard [`ParseError::Unsupported`].

use std::collections::HashMap;

use crate::algebra::{
    AggregateExpression, AggregateFunction, Expression, Function, GraphPattern, OrderExpression,
    PropertyPathExpression, Query,
};
use crate::ast::{
    BaseDirection, BlankNode, GroundTerm, GroundTriple, Literal, NamedNode, NamedNodePattern,
    TermPattern, TriplePattern, Variable,
};
use crate::error::{ParseError, Result};
use crate::lexer::{tokenize, Spanned, Token};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// A reusable SPARQL query parser.
///
/// Mirrors the `spargebra::SparqlParser` surface the existing consumers call so
/// the port is mechanical: `SparqlParser::new().parse_query(text)`.
#[derive(Clone, Debug, Default)]
pub struct SparqlParser {
    base_iri: Option<String>,
}

impl SparqlParser {
    /// Construct a parser with no implicit base IRI.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an implicit base IRI used to resolve relative IRI references that
    /// appear before any in-query `BASE` declaration.
    pub fn with_base_iri(mut self, base_iri: impl Into<String>) -> Self {
        self.base_iri = Some(base_iri.into());
        self
    }

    /// Parse a SPARQL 1.1/1.2 query into the algebra.
    pub fn parse_query(&self, query: &str) -> Result<Query> {
        let tokens = tokenize(query)?;
        let mut p = Parser {
            tokens,
            pos: 0,
            prefixes: HashMap::new(),
            base: self.base_iri.clone(),
            agg_counter: 0,
        };
        let q = p.parse_query()?;
        p.expect_eof()?;
        Ok(q)
    }
}

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
    prefixes: HashMap<String, String>,
    base: Option<String>,
    agg_counter: usize,
}

impl Parser {
    // ── token cursor ─────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|s| &s.token)
    }

    fn peek2(&self) -> Option<&Token> {
        self.tokens.get(self.pos + 1).map(|s| &s.token)
    }

    fn span(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|s| s.start)
            .unwrap_or_else(|| self.tokens.last().map(|s| s.end).unwrap_or(0))
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).map(|s| s.token.clone());
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn at(&self, t: &Token) -> bool {
        self.peek() == Some(t)
    }

    fn eat(&mut self, t: &Token) -> bool {
        if self.at(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: &Token) -> Result<()> {
        if self.eat(t) {
            Ok(())
        } else {
            Err(ParseError::syntax(
                format!("expected {t:?}, found {:?}", self.peek()),
                self.span(),
            ))
        }
    }

    /// Is the current token the keyword `kw` (case-insensitive `Word`)?
    fn peek_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w.eq_ignore_ascii_case(kw))
    }

    fn peek2_kw(&self, kw: &str) -> bool {
        matches!(self.peek2(), Some(Token::Word(w)) if w.eq_ignore_ascii_case(kw))
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.peek_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: &str) -> Result<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(ParseError::syntax(
                format!("expected keyword {kw}, found {:?}", self.peek()),
                self.span(),
            ))
        }
    }

    fn expect_eof(&self) -> Result<()> {
        if self.pos >= self.tokens.len() {
            Ok(())
        } else {
            Err(ParseError::syntax(
                format!("unexpected trailing token {:?}", self.peek()),
                self.span(),
            ))
        }
    }

    // ── prologue + query form ────────────────────────────────────────────────

    fn parse_query(&mut self) -> Result<Query> {
        self.parse_prologue()?;
        let base_iri = self.base.clone().map(NamedNode::new_unchecked);
        if self.peek_kw("SELECT") {
            self.parse_select(base_iri)
        } else if self.peek_kw("CONSTRUCT") {
            self.parse_construct(base_iri)
        } else if self.peek_kw("ASK") {
            self.parse_ask(base_iri)
        } else if self.peek_kw("DESCRIBE") {
            self.parse_describe(base_iri)
        } else {
            Err(ParseError::syntax(
                "expected SELECT, CONSTRUCT, ASK or DESCRIBE",
                self.span(),
            ))
        }
    }

    fn parse_prologue(&mut self) -> Result<()> {
        loop {
            if self.eat_kw("BASE") {
                let iri = self.expect_iriref()?;
                self.base = Some(iri);
            } else if self.eat_kw("PREFIX") {
                let (prefix, _) = self.expect_pname_ns()?;
                let iri = self.expect_iriref()?;
                self.prefixes.insert(prefix, iri);
            } else {
                break;
            }
        }
        Ok(())
    }

    fn expect_iriref(&mut self) -> Result<String> {
        match self.bump() {
            Some(Token::Iri(s)) => Ok(self.resolve_iri(&s)),
            other => Err(ParseError::syntax(
                format!("expected IRIREF, found {other:?}"),
                self.span(),
            )),
        }
    }

    /// Expect a `prefix:` namespace token (PNAME_NS), i.e. an empty local part.
    fn expect_pname_ns(&mut self) -> Result<(String, String)> {
        match self.bump() {
            Some(Token::PrefixedName(p, l)) => Ok((p, l)),
            other => Err(ParseError::syntax(
                format!("expected prefix declaration, found {other:?}"),
                self.span(),
            )),
        }
    }

    fn resolve_iri(&self, s: &str) -> String {
        match &self.base {
            Some(base) if !is_absolute_iri(s) => match gmeow_iri::parse(base) {
                Ok(base_iri) => gmeow_iri::Iri::resolve(&base_iri, s)
                    .map(|r| r.as_str().to_owned())
                    .unwrap_or_else(|_| s.to_owned()),
                Err(_) => s.to_owned(),
            },
            _ => s.to_owned(),
        }
    }

    fn resolve_prefixed(&self, prefix: &str, local: &str) -> Result<NamedNode> {
        match self.prefixes.get(prefix) {
            Some(ns) => Ok(NamedNode::new_unchecked(format!("{ns}{local}"))),
            None => Err(ParseError::syntax(
                format!("undeclared prefix {prefix:?}"),
                self.span(),
            )),
        }
    }

    // ── query forms ──────────────────────────────────────────────────────────

    fn parse_select(&mut self, base_iri: Option<NamedNode>) -> Result<Query> {
        self.expect_kw("SELECT")?;
        let distinct = self.eat_kw("DISTINCT");
        let reduced = !distinct && self.eat_kw("REDUCED");

        // Projection: `*` or a list of Var / (Expr AS Var).
        let mut star = false;
        let mut projected: Vec<Variable> = Vec::new();
        let mut select_exprs: Vec<(Variable, Expression)> = Vec::new();
        let mut aggregates: Vec<(Variable, AggregateExpression)> = Vec::new();
        if self.eat(&Token::Star) {
            star = true;
        } else {
            loop {
                if let Some(Token::Variable(_)) = self.peek() {
                    projected.push(self.expect_var()?);
                } else if self.at(&Token::LParen) {
                    self.expect(&Token::LParen)?;
                    let expr = self.parse_expression_lifting_aggs(&mut aggregates)?;
                    self.expect_kw("AS")?;
                    let var = self.expect_var()?;
                    self.expect(&Token::RParen)?;
                    projected.push(var.clone());
                    select_exprs.push((var, expr));
                } else {
                    break;
                }
            }
            if projected.is_empty() {
                return Err(ParseError::syntax("empty SELECT projection", self.span()));
            }
        }

        // Optional dataset clause (FROM / FROM NAMED) — out of scope.
        self.reject_dataset_clause()?;

        self.eat_kw("WHERE");
        let where_pat = self.parse_group_graph_pattern()?;

        let modifiers = self.parse_solution_modifiers(&mut aggregates)?;

        // Build the algebra (§18.2.4 ordering).
        let mut p = where_pat;
        let has_group = !modifiers.group_by.is_empty() || !aggregates.is_empty();
        if has_group {
            p = GraphPattern::Group {
                inner: Box::new(p),
                variables: modifiers.group_by.clone(),
                aggregates,
            };
        }
        for expr in modifiers.having {
            p = GraphPattern::Filter {
                expr,
                inner: Box::new(p),
            };
        }
        for (var, expr) in select_exprs {
            p = GraphPattern::Extend {
                inner: Box::new(p),
                variable: var,
                expression: expr,
            };
        }
        if !modifiers.order_by.is_empty() {
            p = GraphPattern::OrderBy {
                inner: Box::new(p),
                expression: modifiers.order_by,
            };
        }
        let variables = if star {
            visible_variables(&p)
        } else {
            projected
        };
        p = GraphPattern::Project {
            inner: Box::new(p),
            variables,
        };
        if distinct {
            p = GraphPattern::Distinct { inner: Box::new(p) };
        } else if reduced {
            p = GraphPattern::Reduced { inner: Box::new(p) };
        }
        if modifiers.offset.is_some() || modifiers.limit.is_some() {
            p = GraphPattern::Slice {
                inner: Box::new(p),
                start: modifiers.offset.unwrap_or(0),
                length: modifiers.limit,
            };
        }
        Ok(Query::Select {
            pattern: p,
            base_iri,
        })
    }

    fn parse_construct(&mut self, base_iri: Option<NamedNode>) -> Result<Query> {
        self.expect_kw("CONSTRUCT")?;
        // Long form: CONSTRUCT { template } WHERE { ... }
        self.expect(&Token::LBrace)?;
        let template = self.parse_construct_template()?;
        self.expect(&Token::RBrace)?;
        self.reject_dataset_clause()?;
        self.eat_kw("WHERE");
        let where_pat = self.parse_group_graph_pattern()?;
        let mut aggregates = Vec::new();
        let modifiers = self.parse_solution_modifiers(&mut aggregates)?;
        if !aggregates.is_empty() || !modifiers.group_by.is_empty() {
            return Err(ParseError::unsupported("aggregation in CONSTRUCT"));
        }
        let mut p = where_pat;
        if !modifiers.order_by.is_empty() {
            p = GraphPattern::OrderBy {
                inner: Box::new(p),
                expression: modifiers.order_by,
            };
        }
        if modifiers.offset.is_some() || modifiers.limit.is_some() {
            p = GraphPattern::Slice {
                inner: Box::new(p),
                start: modifiers.offset.unwrap_or(0),
                length: modifiers.limit,
            };
        }
        Ok(Query::Construct {
            template,
            pattern: p,
            base_iri,
        })
    }

    fn parse_ask(&mut self, base_iri: Option<NamedNode>) -> Result<Query> {
        self.expect_kw("ASK")?;
        self.reject_dataset_clause()?;
        self.eat_kw("WHERE");
        let pattern = self.parse_group_graph_pattern()?;
        let mut aggregates = Vec::new();
        let _ = self.parse_solution_modifiers(&mut aggregates)?;
        Ok(Query::Ask { pattern, base_iri })
    }

    fn parse_describe(&mut self, base_iri: Option<NamedNode>) -> Result<Query> {
        self.expect_kw("DESCRIBE")?;
        let mut targets = Vec::new();
        if self.eat(&Token::Star) {
            // DESCRIBE * — no explicit targets.
        } else {
            loop {
                match self.peek() {
                    Some(Token::Variable(_)) => {
                        targets.push(NamedNodePattern::Variable(self.expect_var()?))
                    }
                    Some(Token::Iri(_)) | Some(Token::PrefixedName(_, _)) => {
                        targets.push(NamedNodePattern::NamedNode(self.expect_iri_node()?))
                    }
                    _ => break,
                }
            }
            if targets.is_empty() {
                return Err(ParseError::syntax("DESCRIBE needs a target", self.span()));
            }
        }
        self.reject_dataset_clause()?;
        let pattern = if self.eat_kw("WHERE") || self.at(&Token::LBrace) {
            self.parse_group_graph_pattern()?
        } else {
            GraphPattern::Bgp { patterns: vec![] }
        };
        let mut aggregates = Vec::new();
        let _ = self.parse_solution_modifiers(&mut aggregates)?;
        Ok(Query::Describe {
            pattern,
            targets,
            base_iri,
        })
    }

    fn reject_dataset_clause(&mut self) -> Result<()> {
        if self.peek_kw("FROM") {
            return Err(ParseError::unsupported("FROM / dataset clause"));
        }
        Ok(())
    }

    fn parse_construct_template(&mut self) -> Result<Vec<TriplePattern>> {
        // A bag of triples (TriplesTemplate): subject predicate-object lists,
        // `.`-separated. Simple predicates only (paths are not valid here).
        let mut triples = Vec::new();
        while !self.at(&Token::RBrace) {
            let subject = self.parse_term_pattern()?;
            self.parse_predicate_object_list(&subject, &mut triples, &mut Vec::new())?;
            if !self.eat(&Token::Dot) {
                break;
            }
        }
        Ok(triples)
    }

    // ── group graph pattern → algebra (§18.2.2) ──────────────────────────────

    fn parse_group_graph_pattern(&mut self) -> Result<GraphPattern> {
        self.expect(&Token::LBrace)?;

        // A sub-SELECT group: `{ SELECT ... }`.
        if self.peek_kw("SELECT") {
            let sub = self.parse_select(None)?;
            self.expect(&Token::RBrace)?;
            return match sub {
                Query::Select { pattern, .. } => Ok(pattern),
                _ => unreachable!("parse_select yields Query::Select"),
            };
        }

        let mut g = GraphPattern::Bgp { patterns: vec![] };
        let mut filters: Vec<Expression> = Vec::new();

        loop {
            if self.at(&Token::RBrace) {
                break;
            } else if self.at(&Token::LBrace) {
                let mut node = self.parse_group_graph_pattern()?;
                while self.eat_kw("UNION") {
                    let right = self.parse_group_graph_pattern()?;
                    node = GraphPattern::Union {
                        left: Box::new(node),
                        right: Box::new(right),
                    };
                }
                g = join(g, node);
            } else if self.eat_kw("OPTIONAL") {
                let inner = self.parse_group_graph_pattern()?;
                let (right, expression) = split_trailing_filter(inner);
                g = GraphPattern::LeftJoin {
                    left: Box::new(g),
                    right: Box::new(right),
                    expression,
                };
            } else if self.eat_kw("MINUS") {
                let right = self.parse_group_graph_pattern()?;
                g = GraphPattern::Minus {
                    left: Box::new(g),
                    right: Box::new(right),
                };
            } else if self.eat_kw("GRAPH") {
                let name = self.parse_var_or_iri_name()?;
                let inner = self.parse_group_graph_pattern()?;
                g = join(
                    g,
                    GraphPattern::Graph {
                        name,
                        inner: Box::new(inner),
                    },
                );
            } else if self.eat_kw("SERVICE") {
                let silent = self.eat_kw("SILENT");
                let name = self.parse_var_or_iri_name()?;
                let inner = self.parse_group_graph_pattern()?;
                g = join(
                    g,
                    GraphPattern::Service {
                        name,
                        inner: Box::new(inner),
                        silent,
                    },
                );
            } else if self.eat_kw("FILTER") {
                filters.push(self.parse_constraint()?);
            } else if self.eat_kw("BIND") {
                self.expect(&Token::LParen)?;
                let expression = self.parse_expression()?;
                self.expect_kw("AS")?;
                let variable = self.expect_var()?;
                self.expect(&Token::RParen)?;
                g = GraphPattern::Extend {
                    inner: Box::new(g),
                    variable,
                    expression,
                };
            } else if self.peek_kw("VALUES") {
                let values = self.parse_inline_data()?;
                g = join(g, values);
            } else if self.eat(&Token::Dot) {
                // statement separator between blocks
            } else {
                // A triples block (BGP / path patterns).
                let block = self.parse_triples_block()?;
                g = join(g, block);
            }
        }

        self.expect(&Token::RBrace)?;
        for expr in filters {
            g = GraphPattern::Filter {
                expr,
                inner: Box::new(g),
            };
        }
        Ok(g)
    }

    /// Parse a run of triples (subject + predicate-object lists) into a BGP and
    /// any complex property-path `Path` nodes, joined together.
    fn parse_triples_block(&mut self) -> Result<GraphPattern> {
        let mut triples: Vec<TriplePattern> = Vec::new();
        let mut paths: Vec<GraphPattern> = Vec::new();
        loop {
            let subject = self.parse_term_pattern()?;
            self.parse_predicate_object_list(&subject, &mut triples, &mut paths)?;
            if !self.eat(&Token::Dot) {
                break;
            }
            // After a `.`, stop if the block ends (`}` or a keyword/brace).
            if self.at(&Token::RBrace) || self.block_boundary() {
                break;
            }
        }
        let mut g = GraphPattern::Bgp { patterns: triples };
        for path in paths {
            g = join(g, path);
        }
        Ok(g)
    }

    /// True when the next token starts a non-triples element of a group.
    fn block_boundary(&self) -> bool {
        self.at(&Token::LBrace)
            || self.peek_kw("OPTIONAL")
            || self.peek_kw("MINUS")
            || self.peek_kw("GRAPH")
            || self.peek_kw("SERVICE")
            || self.peek_kw("FILTER")
            || self.peek_kw("BIND")
            || self.peek_kw("VALUES")
    }

    fn parse_predicate_object_list(
        &mut self,
        subject: &TermPattern,
        triples: &mut Vec<TriplePattern>,
        paths: &mut Vec<GraphPattern>,
    ) -> Result<()> {
        loop {
            // Verb = VarOrIri | path. A bare variable predicate is a simple
            // triple predicate, not a property path.
            let verb = if let Some(Token::Variable(_)) = self.peek() {
                Verb::Simple(NamedNodePattern::Variable(self.expect_var()?))
            } else {
                let path = self.parse_path()?;
                match simple_predicate(&path) {
                    Some(pred) => Verb::Simple(pred),
                    None => Verb::Path(path),
                }
            };
            // object list
            loop {
                let object = self.parse_term_pattern()?;
                match &verb {
                    Verb::Simple(pred) => triples.push(TriplePattern {
                        subject: subject.clone(),
                        predicate: pred.clone(),
                        object,
                    }),
                    Verb::Path(path) => paths.push(GraphPattern::Path {
                        subject: subject.clone(),
                        path: path.clone(),
                        object,
                    }),
                }
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
            if !self.eat(&Token::Semicolon) {
                break;
            }
            // allow a trailing `;` before `.`/`}`
            if self.at(&Token::Dot) || self.at(&Token::RBrace) || self.block_boundary() {
                break;
            }
        }
        Ok(())
    }

    // ── property paths (§18.1.7 / §9) ────────────────────────────────────────

    fn parse_path(&mut self) -> Result<PropertyPathExpression> {
        self.parse_path_alternative()
    }

    fn parse_path_alternative(&mut self) -> Result<PropertyPathExpression> {
        let mut left = self.parse_path_sequence()?;
        while self.eat(&Token::Pipe) {
            let right = self.parse_path_sequence()?;
            left = PropertyPathExpression::Alternative(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_path_sequence(&mut self) -> Result<PropertyPathExpression> {
        let mut left = self.parse_path_elt_or_inverse()?;
        while self.eat(&Token::Slash) {
            let right = self.parse_path_elt_or_inverse()?;
            left = PropertyPathExpression::Sequence(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_path_elt_or_inverse(&mut self) -> Result<PropertyPathExpression> {
        if self.eat(&Token::Caret) {
            Ok(PropertyPathExpression::Reverse(Box::new(
                self.parse_path_elt()?,
            )))
        } else {
            self.parse_path_elt()
        }
    }

    fn parse_path_elt(&mut self) -> Result<PropertyPathExpression> {
        let primary = self.parse_path_primary()?;
        Ok(match self.peek() {
            Some(Token::Star) => {
                self.pos += 1;
                PropertyPathExpression::ZeroOrMore(Box::new(primary))
            }
            Some(Token::Plus) => {
                self.pos += 1;
                PropertyPathExpression::OneOrMore(Box::new(primary))
            }
            Some(Token::Question) => {
                self.pos += 1;
                PropertyPathExpression::ZeroOrOne(Box::new(primary))
            }
            _ => primary,
        })
    }

    fn parse_path_primary(&mut self) -> Result<PropertyPathExpression> {
        if self.peek_kw("a") && matches!(self.peek(), Some(Token::Word(w)) if w == "a") {
            self.pos += 1;
            return Ok(PropertyPathExpression::NamedNode(NamedNode::new_unchecked(
                RDF_TYPE,
            )));
        }
        match self.peek() {
            Some(Token::Iri(_)) | Some(Token::PrefixedName(_, _)) => {
                Ok(PropertyPathExpression::NamedNode(self.expect_iri_node()?))
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let inner = self.parse_path()?;
                self.expect(&Token::RParen)?;
                Ok(inner)
            }
            Some(Token::Bang) => {
                self.pos += 1;
                self.parse_negated_property_set()
            }
            other => Err(ParseError::syntax(
                format!("expected a property path, found {other:?}"),
                self.span(),
            )),
        }
    }

    fn parse_negated_property_set(&mut self) -> Result<PropertyPathExpression> {
        let mut nodes = Vec::new();
        if self.eat(&Token::LParen) {
            loop {
                nodes.push(self.parse_path_one_in_set()?);
                if !self.eat(&Token::Pipe) {
                    break;
                }
            }
            self.expect(&Token::RParen)?;
        } else {
            nodes.push(self.parse_path_one_in_set()?);
        }
        Ok(PropertyPathExpression::NegatedPropertySet(nodes))
    }

    fn parse_path_one_in_set(&mut self) -> Result<NamedNode> {
        let _inverse = self.eat(&Token::Caret);
        if matches!(self.peek(), Some(Token::Word(w)) if w == "a") {
            self.pos += 1;
            return Ok(NamedNode::new_unchecked(RDF_TYPE));
        }
        self.expect_iri_node()
    }

    // ── terms ────────────────────────────────────────────────────────────────

    fn parse_term_pattern(&mut self) -> Result<TermPattern> {
        match self.peek() {
            Some(Token::Variable(_)) => Ok(TermPattern::Variable(self.expect_var()?)),
            Some(Token::Iri(_)) | Some(Token::PrefixedName(_, _)) => {
                Ok(TermPattern::NamedNode(self.expect_iri_node()?))
            }
            Some(Token::BlankNodeLabel(_)) => {
                let Some(Token::BlankNodeLabel(l)) = self.bump() else {
                    unreachable!()
                };
                Ok(TermPattern::BlankNode(BlankNode::new(l)))
            }
            Some(Token::Anon) => {
                self.pos += 1;
                Ok(TermPattern::BlankNode(BlankNode::new("")))
            }
            Some(Token::StringLit(_))
            | Some(Token::Integer(_))
            | Some(Token::Decimal(_))
            | Some(Token::Double(_)) => Ok(TermPattern::Literal(self.parse_literal()?)),
            Some(Token::Word(w)) if w == "true" || w == "false" => {
                let b = matches!(self.bump(), Some(Token::Word(w)) if w == "true");
                Ok(TermPattern::Literal(Literal::new_typed(
                    if b { "true" } else { "false" },
                    NamedNode::new_unchecked(XSD_BOOLEAN),
                )))
            }
            Some(Token::TripleOpen) => {
                let t = self.parse_quoted_triple()?;
                Ok(TermPattern::Triple(Box::new(t)))
            }
            other => Err(ParseError::syntax(
                format!("expected an RDF term, found {other:?}"),
                self.span(),
            )),
        }
    }

    /// `<<( s p o )>>` or `<< s p o >>` (RDF 1.2 quoted triple / triple term).
    fn parse_quoted_triple(&mut self) -> Result<TriplePattern> {
        self.expect(&Token::TripleOpen)?;
        let parens = self.eat(&Token::LParen);
        let subject = self.parse_term_pattern()?;
        let predicate = self.parse_predicate_name()?;
        let object = self.parse_term_pattern()?;
        if parens {
            self.expect(&Token::RParen)?;
        }
        self.expect(&Token::TripleClose)?;
        Ok(TriplePattern {
            subject,
            predicate,
            object,
        })
    }

    /// A predicate in a triple position: an IRI, `a`, or a variable.
    fn parse_predicate_name(&mut self) -> Result<NamedNodePattern> {
        if matches!(self.peek(), Some(Token::Word(w)) if w == "a") {
            self.pos += 1;
            return Ok(NamedNodePattern::NamedNode(NamedNode::new_unchecked(
                RDF_TYPE,
            )));
        }
        match self.peek() {
            Some(Token::Variable(_)) => Ok(NamedNodePattern::Variable(self.expect_var()?)),
            _ => Ok(NamedNodePattern::NamedNode(self.expect_iri_node()?)),
        }
    }

    fn parse_var_or_iri_name(&mut self) -> Result<NamedNodePattern> {
        match self.peek() {
            Some(Token::Variable(_)) => Ok(NamedNodePattern::Variable(self.expect_var()?)),
            _ => Ok(NamedNodePattern::NamedNode(self.expect_iri_node()?)),
        }
    }

    fn parse_literal(&mut self) -> Result<Literal> {
        match self.bump() {
            Some(Token::Integer(s)) => {
                Ok(Literal::new_typed(s, NamedNode::new_unchecked(XSD_INTEGER)))
            }
            Some(Token::Decimal(s)) => {
                Ok(Literal::new_typed(s, NamedNode::new_unchecked(XSD_DECIMAL)))
            }
            Some(Token::Double(s)) => {
                Ok(Literal::new_typed(s, NamedNode::new_unchecked(XSD_DOUBLE)))
            }
            Some(Token::StringLit(s)) => {
                if let Some(Token::LangTag(_)) = self.peek() {
                    let Some(Token::LangTag(tag)) = self.bump() else {
                        unreachable!()
                    };
                    let (lang, dir) = split_lang_dir(&tag);
                    Ok(Literal::new_lang(s, lang, dir))
                } else if self.eat(&Token::HatHat) {
                    let dt = self.expect_iri_node()?;
                    Ok(Literal::new_typed(s, dt))
                } else {
                    Ok(Literal::new_simple(s))
                }
            }
            other => Err(ParseError::syntax(
                format!("expected a literal, found {other:?}"),
                self.span(),
            )),
        }
    }

    fn expect_var(&mut self) -> Result<Variable> {
        match self.bump() {
            Some(Token::Variable(n)) => Ok(Variable::new(n)),
            other => Err(ParseError::syntax(
                format!("expected a variable, found {other:?}"),
                self.span(),
            )),
        }
    }

    fn expect_iri_node(&mut self) -> Result<NamedNode> {
        match self.bump() {
            Some(Token::Iri(s)) => Ok(NamedNode::new_unchecked(self.resolve_iri(&s))),
            Some(Token::PrefixedName(p, l)) => self.resolve_prefixed(&p, &l),
            other => Err(ParseError::syntax(
                format!("expected an IRI, found {other:?}"),
                self.span(),
            )),
        }
    }

    // ── VALUES / inline data ─────────────────────────────────────────────────

    fn parse_inline_data(&mut self) -> Result<GraphPattern> {
        self.expect_kw("VALUES")?;
        let mut variables = Vec::new();
        let mut bindings = Vec::new();
        if self.eat(&Token::LParen) {
            // VALUES ( ?a ?b ) { ( v v ) ... }
            while let Some(Token::Variable(_)) = self.peek() {
                variables.push(self.expect_var()?);
            }
            self.expect(&Token::RParen)?;
            self.expect(&Token::LBrace)?;
            while self.eat(&Token::LParen) {
                let mut row = Vec::new();
                while !self.at(&Token::RParen) {
                    row.push(self.parse_data_cell()?);
                }
                self.expect(&Token::RParen)?;
                bindings.push(row);
            }
            self.expect(&Token::RBrace)?;
        } else {
            // VALUES ?a { v v ... }
            variables.push(self.expect_var()?);
            self.expect(&Token::LBrace)?;
            while !self.at(&Token::RBrace) {
                bindings.push(vec![self.parse_data_cell()?]);
            }
            self.expect(&Token::RBrace)?;
        }
        Ok(GraphPattern::Values {
            variables,
            bindings,
        })
    }

    fn parse_data_cell(&mut self) -> Result<Option<GroundTerm>> {
        if matches!(self.peek(), Some(Token::Word(w)) if w.eq_ignore_ascii_case("UNDEF")) {
            self.pos += 1;
            return Ok(None);
        }
        Ok(Some(self.parse_ground_term()?))
    }

    fn parse_ground_term(&mut self) -> Result<GroundTerm> {
        match self.peek() {
            Some(Token::Iri(_)) | Some(Token::PrefixedName(_, _)) => {
                Ok(GroundTerm::NamedNode(self.expect_iri_node()?))
            }
            Some(Token::TripleOpen) => {
                let t = self.parse_ground_triple()?;
                Ok(GroundTerm::Triple(Box::new(t)))
            }
            Some(Token::Word(w)) if w == "true" || w == "false" => {
                let b = matches!(self.bump(), Some(Token::Word(w)) if w == "true");
                Ok(GroundTerm::Literal(Literal::new_typed(
                    if b { "true" } else { "false" },
                    NamedNode::new_unchecked(XSD_BOOLEAN),
                )))
            }
            _ => Ok(GroundTerm::Literal(self.parse_literal()?)),
        }
    }

    fn parse_ground_triple(&mut self) -> Result<GroundTriple> {
        self.expect(&Token::TripleOpen)?;
        let parens = self.eat(&Token::LParen);
        let subject = self.parse_ground_term()?;
        let predicate = self.expect_iri_node()?;
        let object = self.parse_ground_term()?;
        if parens {
            self.expect(&Token::RParen)?;
        }
        self.expect(&Token::TripleClose)?;
        Ok(GroundTriple {
            subject,
            predicate,
            object,
        })
    }

    // ── solution modifiers ───────────────────────────────────────────────────

    fn parse_solution_modifiers(
        &mut self,
        aggregates: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Modifiers> {
        let mut m = Modifiers::default();
        if self.eat_kw("GROUP") {
            self.expect_kw("BY")?;
            loop {
                if let Some(Token::Variable(_)) = self.peek() {
                    m.group_by.push(self.expect_var()?);
                } else if self.at(&Token::LParen) {
                    // (Expr AS ?v) grouping — bind then group by ?v (rare; reject)
                    return Err(ParseError::unsupported("expression in GROUP BY"));
                } else {
                    break;
                }
            }
        }
        if self.eat_kw("HAVING") {
            loop {
                let expr = self.parse_having_constraint(aggregates)?;
                m.having.push(expr);
                if !self.at(&Token::LParen) {
                    break;
                }
            }
        }
        if self.eat_kw("ORDER") {
            self.expect_kw("BY")?;
            loop {
                let cond = if self.eat_kw("ASC") {
                    self.expect(&Token::LParen)?;
                    let e = self.parse_expression()?;
                    self.expect(&Token::RParen)?;
                    OrderExpression::Asc(e)
                } else if self.eat_kw("DESC") {
                    self.expect(&Token::LParen)?;
                    let e = self.parse_expression()?;
                    self.expect(&Token::RParen)?;
                    OrderExpression::Desc(e)
                } else if self.order_key_ahead() {
                    OrderExpression::Asc(self.parse_primary_expression()?)
                } else {
                    break;
                };
                m.order_by.push(cond);
            }
        }
        // LIMIT / OFFSET in either order.
        loop {
            if self.eat_kw("LIMIT") {
                m.limit = Some(self.expect_integer()?);
            } else if self.eat_kw("OFFSET") {
                m.offset = Some(self.expect_integer()?);
            } else {
                break;
            }
        }
        Ok(m)
    }

    fn order_key_ahead(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::Variable(_)) | Some(Token::LParen) | Some(Token::Iri(_))
        ) || matches!(self.peek(), Some(Token::Word(w)) if is_builtin_function(w))
    }

    fn expect_integer(&mut self) -> Result<usize> {
        match self.bump() {
            Some(Token::Integer(s)) => s
                .parse::<usize>()
                .map_err(|_| ParseError::syntax(format!("bad integer {s:?}"), self.span())),
            other => Err(ParseError::syntax(
                format!("expected an integer, found {other:?}"),
                self.span(),
            )),
        }
    }

    // ── expressions ──────────────────────────────────────────────────────────

    /// FILTER constraint: a bracketted expression, a built-in call, or a
    /// function call (§ Constraint).
    fn parse_constraint(&mut self) -> Result<Expression> {
        if self.at(&Token::LParen) {
            self.pos += 1;
            let e = self.parse_expression()?;
            self.expect(&Token::RParen)?;
            Ok(e)
        } else {
            self.parse_primary_expression()
        }
    }

    fn parse_having_constraint(
        &mut self,
        aggregates: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        self.expect(&Token::LParen)?;
        let e = self.parse_expression_lifting_aggs(aggregates)?;
        self.expect(&Token::RParen)?;
        Ok(e)
    }

    fn parse_expression(&mut self) -> Result<Expression> {
        let mut sink = Vec::new();
        let e = self.parse_or(&mut sink)?;
        if !sink.is_empty() {
            return Err(ParseError::unsupported(
                "aggregate outside GROUP BY / SELECT / HAVING context",
            ));
        }
        Ok(e)
    }

    fn parse_expression_lifting_aggs(
        &mut self,
        aggregates: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        self.parse_or(aggregates)
    }

    fn parse_or(&mut self, aggs: &mut Vec<(Variable, AggregateExpression)>) -> Result<Expression> {
        let mut left = self.parse_and(aggs)?;
        while self.eat(&Token::Or) {
            let right = self.parse_and(aggs)?;
            left = Expression::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self, aggs: &mut Vec<(Variable, AggregateExpression)>) -> Result<Expression> {
        let mut left = self.parse_relational(aggs)?;
        while self.eat(&Token::And) {
            let right = self.parse_relational(aggs)?;
            left = Expression::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_relational(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        let left = self.parse_additive(aggs)?;
        let op = match self.peek() {
            Some(Token::Eq) => Some("="),
            Some(Token::NotEq) => Some("!="),
            Some(Token::Lt) => Some("<"),
            Some(Token::Gt) => Some(">"),
            Some(Token::LtEq) => Some("<="),
            Some(Token::GtEq) => Some(">="),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += 1;
            let right = self.parse_additive(aggs)?;
            let (l, r) = (Box::new(left), Box::new(right));
            return Ok(match op {
                "=" => Expression::Equal(l, r),
                "!=" => Expression::Not(Box::new(Expression::Equal(l, r))),
                "<" => Expression::Less(l, r),
                ">" => Expression::Greater(l, r),
                "<=" => Expression::LessOrEqual(l, r),
                _ => Expression::GreaterOrEqual(l, r),
            });
        }
        if self.peek_kw("IN") {
            self.pos += 1;
            let list = self.parse_expression_list(aggs)?;
            return Ok(Expression::In(Box::new(left), list));
        }
        if self.peek_kw("NOT") && self.peek2_kw("IN") {
            self.pos += 2;
            let list = self.parse_expression_list(aggs)?;
            return Ok(Expression::Not(Box::new(Expression::In(
                Box::new(left),
                list,
            ))));
        }
        Ok(left)
    }

    fn parse_additive(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        let mut left = self.parse_multiplicative(aggs)?;
        loop {
            if self.eat(&Token::Plus) {
                let right = self.parse_multiplicative(aggs)?;
                left = Expression::Add(Box::new(left), Box::new(right));
            } else if self.eat(&Token::Minus) {
                let right = self.parse_multiplicative(aggs)?;
                left = Expression::Subtract(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        let mut left = self.parse_unary(aggs)?;
        loop {
            if self.eat(&Token::Star) {
                let right = self.parse_unary(aggs)?;
                left = Expression::Multiply(Box::new(left), Box::new(right));
            } else if self.eat(&Token::Slash) {
                let right = self.parse_unary(aggs)?;
                left = Expression::Divide(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        if self.eat(&Token::Bang) {
            Ok(Expression::Not(Box::new(self.parse_unary(aggs)?)))
        } else if self.eat(&Token::Plus) {
            Ok(Expression::UnaryPlus(Box::new(self.parse_unary(aggs)?)))
        } else if self.eat(&Token::Minus) {
            Ok(Expression::UnaryMinus(Box::new(self.parse_unary(aggs)?)))
        } else {
            self.parse_primary_with_aggs(aggs)
        }
    }

    fn parse_primary_expression(&mut self) -> Result<Expression> {
        let mut sink = Vec::new();
        let e = self.parse_primary_with_aggs(&mut sink)?;
        if !sink.is_empty() {
            return Err(ParseError::unsupported("aggregate in this position"));
        }
        Ok(e)
    }

    fn parse_primary_with_aggs(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        match self.peek() {
            Some(Token::LParen) => {
                self.pos += 1;
                let e = self.parse_or(aggs)?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            Some(Token::Variable(_)) => Ok(Expression::Variable(self.expect_var()?)),
            Some(Token::Iri(_)) | Some(Token::PrefixedName(_, _)) => self.parse_iri_or_function(),
            Some(Token::StringLit(_))
            | Some(Token::Integer(_))
            | Some(Token::Decimal(_))
            | Some(Token::Double(_)) => Ok(Expression::Literal(self.parse_literal()?)),
            Some(Token::TripleOpen) => Err(ParseError::unsupported(
                "quoted triple in expression position",
            )),
            Some(Token::Word(w)) => {
                let w = w.clone();
                if w == "true" || w == "false" {
                    self.pos += 1;
                    Ok(Expression::Literal(Literal::new_typed(
                        if w == "true" { "true" } else { "false" },
                        NamedNode::new_unchecked(XSD_BOOLEAN),
                    )))
                } else {
                    self.parse_builtin_or_aggregate(&w, aggs)
                }
            }
            other => Err(ParseError::syntax(
                format!("expected an expression, found {other:?}"),
                self.span(),
            )),
        }
    }

    fn parse_iri_or_function(&mut self) -> Result<Expression> {
        let node = self.expect_iri_node()?;
        if self.at(&Token::LParen) {
            let args = self.parse_arg_list(&mut Vec::new())?;
            Ok(Expression::FunctionCall(Function::Custom(node), args))
        } else {
            Ok(Expression::NamedNode(node))
        }
    }

    fn parse_builtin_or_aggregate(
        &mut self,
        name: &str,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        let upper = name.to_ascii_uppercase();
        // Aggregates lift to a synthetic Group variable.
        if let Some(func) = aggregate_function(&upper) {
            return self.parse_aggregate(func, aggs);
        }
        match upper.as_str() {
            "BOUND" => {
                self.pos += 1;
                self.expect(&Token::LParen)?;
                let v = self.expect_var()?;
                self.expect(&Token::RParen)?;
                Ok(Expression::Bound(v))
            }
            "IF" => {
                self.pos += 1;
                let args = self.parse_arg_list(aggs)?;
                expect_arity(&args, 3, "IF", self.span())?;
                let mut it = args.into_iter();
                Ok(Expression::If(
                    Box::new(it.next().unwrap()),
                    Box::new(it.next().unwrap()),
                    Box::new(it.next().unwrap()),
                ))
            }
            "COALESCE" => {
                self.pos += 1;
                Ok(Expression::Coalesce(self.parse_arg_list(aggs)?))
            }
            "EXISTS" => {
                self.pos += 1;
                Ok(Expression::Exists(Box::new(
                    self.parse_group_graph_pattern()?,
                )))
            }
            "NOT" => {
                self.pos += 1;
                self.expect_kw("EXISTS")?;
                Ok(Expression::Not(Box::new(Expression::Exists(Box::new(
                    self.parse_group_graph_pattern()?,
                )))))
            }
            "SAMETERM" => {
                self.pos += 1;
                let args = self.parse_arg_list(aggs)?;
                expect_arity(&args, 2, "sameTerm", self.span())?;
                let mut it = args.into_iter();
                Ok(Expression::SameTerm(
                    Box::new(it.next().unwrap()),
                    Box::new(it.next().unwrap()),
                ))
            }
            _ => {
                if let Some(func) = builtin_function(&upper) {
                    self.pos += 1;
                    let args = self.parse_arg_list(aggs)?;
                    Ok(Expression::FunctionCall(func, args))
                } else {
                    Err(ParseError::unsupported(format!(
                        "function or keyword {name}"
                    )))
                }
            }
        }
    }

    fn parse_aggregate(
        &mut self,
        func: AggregateFunction,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Expression> {
        self.pos += 1; // function name
        self.expect(&Token::LParen)?;
        let agg = if self.eat(&Token::Star) {
            // COUNT(*)
            AggregateExpression::CountStar { distinct: false }
        } else {
            let distinct = self.eat_kw("DISTINCT");
            let inner = self.parse_expression()?;
            let separator = if let AggregateFunction::GroupConcat { .. } = func {
                self.parse_optional_separator()?
            } else {
                None
            };
            let function = match func {
                AggregateFunction::GroupConcat { .. } => {
                    AggregateFunction::GroupConcat { separator }
                }
                other => other,
            };
            AggregateExpression::FunctionCall {
                function,
                expression: Box::new(inner),
                distinct,
            }
        };
        self.expect(&Token::RParen)?;
        let synth = self.fresh_agg_var();
        aggs.push((synth.clone(), agg));
        Ok(Expression::Variable(synth))
    }

    fn parse_optional_separator(&mut self) -> Result<Option<String>> {
        if self.eat(&Token::Semicolon) {
            self.expect_kw("SEPARATOR")?;
            self.expect(&Token::Eq)?;
            match self.bump() {
                Some(Token::StringLit(s)) => Ok(Some(s)),
                other => Err(ParseError::syntax(
                    format!("expected SEPARATOR string, found {other:?}"),
                    self.span(),
                )),
            }
        } else {
            Ok(None)
        }
    }

    fn fresh_agg_var(&mut self) -> Variable {
        let v = Variable::new(format!("__gmeow_agg_{}", self.agg_counter));
        self.agg_counter += 1;
        v
    }

    fn parse_arg_list(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Vec<Expression>> {
        self.expect(&Token::LParen)?;
        let mut args = Vec::new();
        if self.eat(&Token::Star) {
            // e.g. COUNT(*) handled elsewhere; a bare `*` here is invalid.
            return Err(ParseError::syntax(
                "unexpected '*' in argument list",
                self.span(),
            ));
        }
        if !self.at(&Token::RParen) {
            self.eat_kw("DISTINCT");
            loop {
                args.push(self.parse_or(aggs)?);
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        Ok(args)
    }

    fn parse_expression_list(
        &mut self,
        aggs: &mut Vec<(Variable, AggregateExpression)>,
    ) -> Result<Vec<Expression>> {
        self.expect(&Token::LParen)?;
        let mut list = Vec::new();
        if !self.at(&Token::RParen) {
            loop {
                list.push(self.parse_or(aggs)?);
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        Ok(list)
    }
}

/// A parsed predicate: a simple verb (IRI/`a`/variable) yielding a triple, or a
/// complex property path yielding a `GraphPattern::Path`.
enum Verb {
    Simple(NamedNodePattern),
    Path(PropertyPathExpression),
}

#[derive(Default)]
struct Modifiers {
    group_by: Vec<Variable>,
    having: Vec<Expression>,
    order_by: Vec<OrderExpression>,
    limit: Option<usize>,
    offset: Option<usize>,
}

// ── free helpers ─────────────────────────────────────────────────────────────

/// Join two patterns, merging adjacent BGPs and absorbing the empty pattern (the
/// identity table `Z`) on either side so a group that opens with a non-triple
/// element (`UNION`, a property path, …) is not wrapped in a vacuous `Join`.
fn join(left: GraphPattern, right: GraphPattern) -> GraphPattern {
    if is_empty_bgp(&left) {
        return right;
    }
    if is_empty_bgp(&right) {
        return left;
    }
    match (left, right) {
        (GraphPattern::Bgp { mut patterns }, GraphPattern::Bgp { patterns: r }) => {
            patterns.extend(r);
            GraphPattern::Bgp { patterns }
        }
        (l, r) => GraphPattern::Join {
            left: Box::new(l),
            right: Box::new(r),
        },
    }
}

fn is_empty_bgp(p: &GraphPattern) -> bool {
    matches!(p, GraphPattern::Bgp { patterns } if patterns.is_empty())
}

/// If a property path is length-1 (a single predicate), return it as a triple
/// predicate; complex paths return `None` (they become `GraphPattern::Path`).
fn simple_predicate(path: &PropertyPathExpression) -> Option<NamedNodePattern> {
    match path {
        PropertyPathExpression::NamedNode(n) => Some(NamedNodePattern::NamedNode(n.clone())),
        _ => None,
    }
}

/// Lift a trailing `Filter` out of an `OPTIONAL` body so it becomes the
/// `LeftJoin` join condition (§18.2.2.3 "filter-in-optional").
fn split_trailing_filter(p: GraphPattern) -> (GraphPattern, Option<Expression>) {
    match p {
        GraphPattern::Filter { expr, inner } => (*inner, Some(expr)),
        other => (other, None),
    }
}

/// Collect the in-scope variables of a pattern in first-appearance order
/// (used for `SELECT *` projection).
fn visible_variables(p: &GraphPattern) -> Vec<Variable> {
    let mut out = Vec::new();
    collect_vars(p, &mut out);
    out
}

fn push_var(v: &Variable, out: &mut Vec<Variable>) {
    if !out.contains(v) {
        out.push(v.clone());
    }
}

fn collect_term_vars(t: &TermPattern, out: &mut Vec<Variable>) {
    match t {
        TermPattern::Variable(v) => push_var(v, out),
        TermPattern::Triple(tp) => {
            collect_term_vars(&tp.subject, out);
            if let NamedNodePattern::Variable(v) = &tp.predicate {
                push_var(v, out);
            }
            collect_term_vars(&tp.object, out);
        }
        _ => {}
    }
}

fn collect_triple_vars(tp: &TriplePattern, out: &mut Vec<Variable>) {
    collect_term_vars(&tp.subject, out);
    if let NamedNodePattern::Variable(v) = &tp.predicate {
        push_var(v, out);
    }
    collect_term_vars(&tp.object, out);
}

fn collect_vars(p: &GraphPattern, out: &mut Vec<Variable>) {
    match p {
        GraphPattern::Bgp { patterns } => {
            for tp in patterns {
                collect_triple_vars(tp, out);
            }
        }
        GraphPattern::Path {
            subject, object, ..
        } => {
            collect_term_vars(subject, out);
            collect_term_vars(object, out);
        }
        GraphPattern::Join { left, right }
        | GraphPattern::Union { left, right }
        | GraphPattern::Lateral { left, right }
        | GraphPattern::Minus { left, right } => {
            collect_vars(left, out);
            collect_vars(right, out);
        }
        GraphPattern::LeftJoin { left, right, .. } => {
            collect_vars(left, out);
            collect_vars(right, out);
        }
        GraphPattern::Filter { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. } => collect_vars(inner, out),
        GraphPattern::Graph { name, inner } | GraphPattern::Service { name, inner, .. } => {
            if let NamedNodePattern::Variable(v) = name {
                push_var(v, out);
            }
            collect_vars(inner, out);
        }
        GraphPattern::Extend {
            inner, variable, ..
        } => {
            collect_vars(inner, out);
            push_var(variable, out);
        }
        GraphPattern::Values { variables, .. } => {
            for v in variables {
                push_var(v, out);
            }
        }
        GraphPattern::Project { variables, .. } => {
            for v in variables {
                push_var(v, out);
            }
        }
        GraphPattern::Group {
            variables,
            aggregates,
            ..
        } => {
            for v in variables {
                push_var(v, out);
            }
            for (v, _) in aggregates {
                push_var(v, out);
            }
        }
    }
}

fn is_absolute_iri(s: &str) -> bool {
    // A scheme followed by ':' — RFC-3986 §3.1 (cheap prefix test).
    let mut chars = s.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for (_, c) in chars {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
            return false;
        }
    }
    false
}

/// Split a lang tag into the language and an optional RDF 1.2 base direction
/// (`en--ltr` → (`en`, Ltr)).
fn split_lang_dir(tag: &str) -> (String, Option<BaseDirection>) {
    if let Some((lang, dir)) = tag.split_once("--") {
        let dir = match dir.to_ascii_lowercase().as_str() {
            "ltr" => Some(BaseDirection::Ltr),
            "rtl" => Some(BaseDirection::Rtl),
            _ => None,
        };
        (lang.to_owned(), dir)
    } else {
        (tag.to_owned(), None)
    }
}

fn expect_arity(args: &[Expression], n: usize, name: &str, at: usize) -> Result<()> {
    if args.len() == n {
        Ok(())
    } else {
        Err(ParseError::syntax(
            format!("{name} expects {n} arguments, got {}", args.len()),
            at,
        ))
    }
}

fn aggregate_function(upper: &str) -> Option<AggregateFunction> {
    Some(match upper {
        "COUNT" => AggregateFunction::Count,
        "SUM" => AggregateFunction::Sum,
        "AVG" => AggregateFunction::Avg,
        "MIN" => AggregateFunction::Min,
        "MAX" => AggregateFunction::Max,
        "SAMPLE" => AggregateFunction::Sample,
        "GROUP_CONCAT" => AggregateFunction::GroupConcat { separator: None },
        _ => return None,
    })
}

fn is_builtin_function(name: &str) -> bool {
    builtin_function(&name.to_ascii_uppercase()).is_some()
}

fn builtin_function(upper: &str) -> Option<Function> {
    Some(match upper {
        "STR" => Function::Str,
        "LANG" => Function::Lang,
        "LANGMATCHES" => Function::LangMatches,
        "DATATYPE" => Function::Datatype,
        "IRI" => Function::Iri,
        "URI" => Function::Uri,
        "BNODE" => Function::BNode,
        "RAND" => Function::Rand,
        "ABS" => Function::Abs,
        "CEIL" => Function::Ceil,
        "FLOOR" => Function::Floor,
        "ROUND" => Function::Round,
        "CONCAT" => Function::Concat,
        "SUBSTR" => Function::SubStr,
        "STRLEN" => Function::StrLen,
        "REPLACE" => Function::Replace,
        "UCASE" => Function::UCase,
        "LCASE" => Function::LCase,
        "ENCODE_FOR_URI" => Function::EncodeForUri,
        "CONTAINS" => Function::Contains,
        "STRSTARTS" => Function::StrStarts,
        "STRENDS" => Function::StrEnds,
        "STRBEFORE" => Function::StrBefore,
        "STRAFTER" => Function::StrAfter,
        "YEAR" => Function::Year,
        "MONTH" => Function::Month,
        "DAY" => Function::Day,
        "HOURS" => Function::Hours,
        "MINUTES" => Function::Minutes,
        "SECONDS" => Function::Seconds,
        "TIMEZONE" => Function::Timezone,
        "TZ" => Function::Tz,
        "NOW" => Function::Now,
        "UUID" => Function::Uuid,
        "STRUUID" => Function::StrUuid,
        "MD5" => Function::Md5,
        "SHA1" => Function::Sha1,
        "SHA256" => Function::Sha256,
        "SHA384" => Function::Sha384,
        "SHA512" => Function::Sha512,
        "STRLANG" => Function::StrLang,
        "STRDT" => Function::StrDt,
        "ISIRI" => Function::IsIri,
        "ISURI" => Function::IsUri,
        "ISBLANK" => Function::IsBlank,
        "ISLITERAL" => Function::IsLiteral,
        "ISNUMERIC" => Function::IsNumeric,
        "REGEX" => Function::Regex,
        "TRIPLE" => Function::Triple,
        "SUBJECT" => Function::Subject,
        "PREDICATE" => Function::Predicate,
        "OBJECT" => Function::Object,
        "ISTRIPLE" => Function::IsTriple,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const GM: &str =
        "PREFIX gmeow: <https://x/>\nPREFIX rdf: <http://r/>\nPREFIX rdfs: <http://s/>\n";

    fn parse(q: &str) -> Query {
        SparqlParser::new().parse_query(q).expect("parse")
    }

    fn select_pattern(q: &str) -> GraphPattern {
        match parse(q) {
            Query::Select { pattern, .. } => pattern,
            other => panic!("expected SELECT, got {other:?}"),
        }
    }

    /// Strip the outer `Project` wrapper to reach the WHERE algebra.
    fn unproject(p: GraphPattern) -> GraphPattern {
        match p {
            GraphPattern::Project { inner, .. } => *inner,
            other => other,
        }
    }

    #[test]
    fn quoted_triple_with_variable_predicate() {
        // The RDF-1.2 codec shape: `?r rdf:reifies <<( ?s ?p ?o )>>`.
        let q = format!("{GM}SELECT ?r WHERE {{ ?r rdf:reifies <<( ?s ?p ?o )>> . }}");
        let where_pat = unproject(select_pattern(&q));
        let GraphPattern::Bgp { patterns } = where_pat else {
            panic!("expected BGP, got {where_pat:?}");
        };
        assert_eq!(patterns.len(), 1);
        let TermPattern::Triple(inner) = &patterns[0].object else {
            panic!(
                "object should be a quoted triple, got {:?}",
                patterns[0].object
            );
        };
        assert_eq!(
            inner.predicate,
            NamedNodePattern::Variable(Variable::new("p"))
        );
        assert_eq!(inner.subject, TermPattern::Variable(Variable::new("s")));
    }

    #[test]
    fn optional_lifts_trailing_filter_to_leftjoin() {
        let q = format!(
            "{GM}SELECT ?a WHERE {{ ?a a gmeow:T . OPTIONAL {{ ?a gmeow:p ?b . FILTER(?b != ?a) }} }}"
        );
        let where_pat = unproject(select_pattern(&q));
        let GraphPattern::LeftJoin { expression, .. } = where_pat else {
            panic!("expected LeftJoin, got {where_pat:?}");
        };
        assert!(expression.is_some(), "FILTER should lift into the LeftJoin");
    }

    #[test]
    fn union_of_two_groups() {
        let q = format!("{GM}SELECT ?a WHERE {{ {{ ?a a gmeow:X }} UNION {{ ?a a gmeow:Y }} }}");
        let where_pat = unproject(select_pattern(&q));
        assert!(
            matches!(where_pat, GraphPattern::Union { .. }),
            "got {where_pat:?}"
        );
    }

    #[test]
    fn bind_becomes_extend() {
        let q = format!("{GM}SELECT ?k WHERE {{ ?a a gmeow:T . BIND(\"x\" AS ?k) }}");
        let where_pat = unproject(select_pattern(&q));
        let GraphPattern::Extend { variable, .. } = where_pat else {
            panic!("expected Extend, got {where_pat:?}");
        };
        assert_eq!(variable, Variable::new("k"));
    }

    #[test]
    fn property_path_zero_or_more() {
        let q = format!("{GM}SELECT ?x WHERE {{ ?x rdfs:subClassOf* gmeow:C . }}");
        let where_pat = unproject(select_pattern(&q));
        let GraphPattern::Path { path, .. } = where_pat else {
            panic!("expected Path, got {where_pat:?}");
        };
        assert!(matches!(path, PropertyPathExpression::ZeroOrMore(_)));
    }

    #[test]
    fn sequence_path_with_star() {
        // `owl:members/rdf:rest*/rdf:first` — Sequence containing a ZeroOrMore.
        let q = format!("{GM}SELECT ?x WHERE {{ ?d gmeow:members/rdf:rest*/rdf:first ?x . }}");
        let where_pat = unproject(select_pattern(&q));
        assert!(
            matches!(
                where_pat,
                GraphPattern::Path {
                    path: PropertyPathExpression::Sequence(..),
                    ..
                }
            ),
            "got {where_pat:?}"
        );
    }

    #[test]
    fn filter_not_exists() {
        let q = format!(
            "{GM}SELECT ?a WHERE {{ ?a a gmeow:T . FILTER NOT EXISTS {{ ?a gmeow:bad ?x }} }}"
        );
        let where_pat = unproject(select_pattern(&q));
        let GraphPattern::Filter { expr, .. } = where_pat else {
            panic!("expected Filter, got {where_pat:?}");
        };
        assert!(matches!(expr, Expression::Not(inner) if matches!(*inner, Expression::Exists(_))));
    }

    #[test]
    fn group_by_with_count_aggregate() {
        let q = format!(
            "{GM}SELECT ?m (COUNT(?c) AS ?n) WHERE {{ ?c gmeow:vantage ?m . }} GROUP BY ?m"
        );
        let where_pat = unproject(select_pattern(&q));
        // After §18.2: ... Extend(?n = synth) over Group{aggregates:[(synth, COUNT ?c)]}.
        let GraphPattern::Extend {
            inner, variable, ..
        } = where_pat
        else {
            panic!("expected Extend, got {where_pat:?}");
        };
        assert_eq!(variable, Variable::new("n"));
        let GraphPattern::Group {
            variables,
            aggregates,
            ..
        } = *inner
        else {
            panic!("expected Group under Extend");
        };
        assert_eq!(variables, vec![Variable::new("m")]);
        assert_eq!(aggregates.len(), 1);
        assert!(matches!(
            aggregates[0].1,
            AggregateExpression::FunctionCall {
                function: AggregateFunction::Count,
                ..
            }
        ));
    }

    #[test]
    fn filter_in_list() {
        let q =
            format!("{GM}SELECT ?p WHERE {{ ?f gmeow:pol ?p . FILTER(?p IN (gmeow:a, gmeow:b)) }}");
        let where_pat = unproject(select_pattern(&q));
        let GraphPattern::Filter { expr, .. } = where_pat else {
            panic!("expected Filter, got {where_pat:?}");
        };
        let Expression::In(_, list) = expr else {
            panic!("expected IN, got {expr:?}");
        };
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn construct_form() {
        let q = format!("{GM}CONSTRUCT {{ ?s a gmeow:Out }} WHERE {{ ?s a gmeow:In }}");
        let Query::Construct { template, .. } = parse(&q) else {
            panic!("expected CONSTRUCT");
        };
        assert_eq!(template.len(), 1);
    }

    #[test]
    fn distinct_and_order_by_and_slice() {
        let q =
            format!("{GM}SELECT DISTINCT ?a WHERE {{ ?a a gmeow:T }} ORDER BY ?a LIMIT 5 OFFSET 2");
        let p = select_pattern(&q);
        // Distinct wraps Project; Slice is the outermost? Order: Project → Distinct → Slice.
        let GraphPattern::Slice {
            inner,
            start,
            length,
        } = p
        else {
            panic!("expected Slice outermost, got {p:?}");
        };
        assert_eq!(start, 2);
        assert_eq!(length, Some(5));
        assert!(matches!(*inner, GraphPattern::Distinct { .. }));
    }

    #[test]
    fn select_star_collects_visible_vars() {
        let q = format!("{GM}SELECT * WHERE {{ ?a gmeow:p ?b . }}");
        let GraphPattern::Project { variables, .. } = select_pattern(&q) else {
            panic!("expected Project");
        };
        assert_eq!(variables, vec![Variable::new("a"), Variable::new("b")]);
    }

    #[test]
    fn from_clause_is_unsupported() {
        let q = format!("{GM}SELECT ?a FROM <http://g/> WHERE {{ ?a a gmeow:T }}");
        let err = SparqlParser::new().parse_query(&q).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn undeclared_prefix_is_syntax_error() {
        let err = SparqlParser::new()
            .parse_query("SELECT ?a WHERE { ?a a nope:T }")
            .unwrap_err();
        assert!(matches!(err, ParseError::Syntax { .. }), "got {err:?}");
    }

    #[test]
    fn trailing_tokens_rejected() {
        let q =
            format!("{GM}SELECT ?a WHERE {{ ?a a gmeow:T }} SELECT ?b WHERE {{ ?b a gmeow:U }}");
        assert!(SparqlParser::new().parse_query(&q).is_err());
    }
}
