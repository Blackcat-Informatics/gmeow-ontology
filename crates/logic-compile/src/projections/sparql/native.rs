// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Mapping-model admission into native query algebra. Rendering is an output exit.

use std::collections::BTreeMap;

use purrdf::sparql::{
    BaseDirection, Expression, Function, GraphPattern, GraphUpdateOperation, Literal, NamedNode,
    NamedNodePattern, QuadPattern, Query, QueryDataset, TermPattern, TriplePattern, Variable,
};

use crate::ingest::DslTerm;
use crate::projections::get_leg::{Atom, Expr, Item, RDF_TYPE};

pub(crate) fn error(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Sparql {
        detail: detail.into(),
    })
}

pub(crate) fn iri(value: &str) -> gmeow_errors::Result<NamedNode> {
    NamedNode::new(value.to_owned()).map_err(|why| error(why.to_string()))
}

pub(crate) fn named(value: &str) -> gmeow_errors::Result<TermPattern> {
    iri(value).map(TermPattern::NamedNode)
}

pub(crate) fn var(value: &str) -> TermPattern {
    TermPattern::Variable(Variable::new(value))
}

pub(crate) fn expr_var(value: &str) -> Expression {
    Expression::Variable(Variable::new(value))
}

pub(crate) fn triple(
    subject: TermPattern,
    predicate: &str,
    object: TermPattern,
) -> gmeow_errors::Result<TriplePattern> {
    Ok(TriplePattern {
        subject,
        predicate: NamedNodePattern::NamedNode(iri(predicate)?),
        object,
    })
}

pub(crate) fn bgp(patterns: Vec<TriplePattern>) -> GraphPattern {
    GraphPattern::Bgp { patterns }
}

pub(crate) fn join(left: GraphPattern, right: GraphPattern) -> GraphPattern {
    match (left, right) {
        (GraphPattern::Bgp { mut patterns }, GraphPattern::Bgp { patterns: other }) => {
            patterns.extend(other);
            bgp(patterns)
        }
        (GraphPattern::Bgp { patterns }, right) if patterns.is_empty() => right,
        (left, GraphPattern::Bgp { patterns }) if patterns.is_empty() => left,
        (left, right) => GraphPattern::Join {
            left: Box::new(left),
            right: Box::new(right),
        },
    }
}

pub(crate) fn filter(inner: GraphPattern, expr: Expression) -> GraphPattern {
    GraphPattern::Filter {
        expr,
        inner: Box::new(inner),
    }
}

pub(crate) fn extend(inner: GraphPattern, variable: &str, expression: Expression) -> GraphPattern {
    GraphPattern::Extend {
        inner: Box::new(inner),
        variable: Variable::new(variable),
        expression,
    }
}

pub(crate) fn optional(
    left: GraphPattern,
    right: GraphPattern,
    expression: Option<Expression>,
) -> GraphPattern {
    GraphPattern::LeftJoin {
        left: Box::new(left),
        right: Box::new(right),
        expression,
    }
}

pub(crate) fn object(
    atom: &Atom,
    vars: &BTreeMap<String, String>,
) -> gmeow_errors::Result<TermPattern> {
    if let Some(value) = &atom.object_var {
        return Ok(var(vars.get(value).unwrap_or(value)));
    }
    if let Some(value) = &atom.object_value {
        return named(value);
    }
    if let Some(literal) = &atom.object_literal {
        return literal_term(literal);
    }
    Err(error("mapping atom has no object"))
}

pub(crate) fn atom_triple(
    atom: &Atom,
    vars: &BTreeMap<String, String>,
) -> gmeow_errors::Result<TriplePattern> {
    if let Some(path) = &atom.path {
        let purrdf::sparql::PropertyPathExpression::NamedNode(predicate) = path else {
            return Err(error(
                "a composite property path cannot be a CONSTRUCT template predicate",
            ));
        };
        return Ok(TriplePattern {
            subject: var(vars.get(&atom.subject_var).unwrap_or(&atom.subject_var)),
            predicate: NamedNodePattern::NamedNode(predicate.clone()),
            object: object(atom, vars)?,
        });
    }
    let predicate = if let Some(value) = &atom.predicate_var {
        NamedNodePattern::Variable(Variable::new(value))
    } else {
        NamedNodePattern::NamedNode(iri(atom
            .predicate
            .as_deref()
            .ok_or_else(|| error("mapping atom has no predicate"))?)?)
    };
    Ok(TriplePattern {
        subject: var(vars.get(&atom.subject_var).unwrap_or(&atom.subject_var)),
        predicate,
        object: object(atom, vars)?,
    })
}

pub(crate) fn atom_pattern(atom: &Atom) -> gmeow_errors::Result<GraphPattern> {
    if let Some(path) = &atom.path
        && !matches!(path, purrdf::sparql::PropertyPathExpression::NamedNode(_))
    {
        return Ok(GraphPattern::Path {
            subject: var(&atom.subject_var),
            path: path.clone(),
            object: object(atom, &BTreeMap::new())?,
        });
    }
    Ok(bgp(vec![atom_triple(atom, &BTreeMap::new())?]))
}

pub(crate) fn items(items: &[Item]) -> gmeow_errors::Result<GraphPattern> {
    let mut result = bgp(Vec::new());
    for item in items {
        result = match item {
            Item::Group(inner) => optional(result, self::items(inner)?, None),
            Item::Atom(atom) if atom.optional => optional(result, atom_pattern(atom)?, None),
            Item::Atom(atom) => join(result, atom_pattern(atom)?),
        };
    }
    Ok(result)
}

fn constant(term: &DslTerm) -> gmeow_errors::Result<Expression> {
    match term {
        DslTerm::Iri(value) => iri(value).map(Expression::NamedNode),
        DslTerm::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } => {
            if direction.is_some() && language.is_none() {
                return Err(error("directional constant requires a language tag"));
            }
            let literal = if let Some(language) = language {
                Literal::new_lang(
                    lexical_form.clone(),
                    language.clone(),
                    direction.map(|value| match value {
                        purrdf::RdfTextDirection::Ltr => BaseDirection::Ltr,
                        purrdf::RdfTextDirection::Rtl => BaseDirection::Rtl,
                    }),
                )
            } else if datatype.is_empty() {
                Literal::new_simple(lexical_form.clone())
            } else {
                Literal::new_typed(lexical_form.clone(), iri(datatype)?)
            };
            Ok(Expression::Literal(literal))
        }
        DslTerm::Triple { s, p, o } if s.as_iri().is_some() && p.as_iri().is_some() => {
            Ok(Expression::FunctionCall(
                Function::Triple,
                vec![constant(s)?, constant(p)?, constant(o)?],
            ))
        }
        DslTerm::Blank { .. } | DslTerm::Triple { .. } => Err(error(
            "source-scoped or invalid proposition constant requires an explicit query binding",
        )),
    }
}

pub(crate) fn literal_term(term: &DslTerm) -> gmeow_errors::Result<TermPattern> {
    match constant(term)? {
        Expression::Literal(literal) => Ok(TermPattern::Literal(literal)),
        _ => Err(error("mapping objectLiteral requires a literal")),
    }
}

pub(crate) fn expression(source: &Expr) -> gmeow_errors::Result<Expression> {
    use Expression as E;
    let Expr::Op { op, args } = source else {
        return match source {
            Expr::Var(value) => Ok(expr_var(value)),
            Expr::ConstTerm(value) => constant(value),
            Expr::Op { .. } => unreachable!(),
        };
    };
    let name = op
        .strip_prefix("https://blackcatinformatics.ca/gmeow/")
        .ok_or_else(|| {
            error(format!(
                "unsupported expression operator: {op} is outside the canonical GMEOW namespace"
            ))
        })?;
    let mut args = args
        .iter()
        .map(expression)
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let args_len = args.len();
    let arity = |expected: usize| -> gmeow_errors::Result<()> {
        if args_len == expected {
            Ok(())
        } else {
            Err(error(format!(
                "unsupported expression operator: {name} expects {expected} arguments, got {args_len}"
            )))
        }
    };
    let binary: Option<fn(Box<E>, Box<E>) -> E> = match name {
        "opAdd" => Some(E::Add),
        "opSub" => Some(E::Subtract),
        "opMul" => Some(E::Multiply),
        "opDiv" => Some(E::Divide),
        "opEq" => Some(E::Equal),
        "opLt" => Some(E::Less),
        "opGt" => Some(E::Greater),
        "opLe" => Some(E::LessOrEqual),
        "opGe" => Some(E::GreaterOrEqual),
        "opAnd" => Some(E::And),
        "opOr" => Some(E::Or),
        _ => None,
    };
    if let Some(binary) = binary {
        let mut values = args.into_iter();
        let first = values
            .next()
            .ok_or_else(|| error(format!("empty expression operator: {name}")))?;
        return Ok(values.fold(first, |left, right| binary(Box::new(left), Box::new(right))));
    }
    match name {
        "opNe" => {
            arity(2)?;
            Ok(E::Not(Box::new(E::Equal(
                Box::new(args.remove(0)),
                Box::new(args.remove(0)),
            ))))
        }
        "opNot" => {
            arity(1)?;
            Ok(E::Not(Box::new(args.remove(0))))
        }
        "opIn" => {
            if args.is_empty() {
                return Err(error("opIn requires at least one argument"));
            }
            Ok(E::In(Box::new(args.remove(0)), args))
        }
        "opIf" => {
            arity(3)?;
            Ok(E::If(
                Box::new(args.remove(0)),
                Box::new(args.remove(0)),
                Box::new(args.remove(0)),
            ))
        }
        "opBound" => {
            arity(1)?;
            match args.remove(0) {
                E::Variable(value) => Ok(E::Bound(value)),
                _ => Err(error("BOUND requires a variable")),
            }
        }
        "opCoalesce" => Ok(E::Coalesce(args)),
        _ => {
            let function = match name {
                "opConcat" => Function::Concat,
                "opStr" => Function::Str,
                "opIri" => Function::Iri,
                "opStrDatatype" => Function::StrDt,
                "opLang" => Function::Lang,
                "opLangMatches" => Function::LangMatches,
                "opStrLang" => Function::StrLang,
                "opDatatype" => Function::Datatype,
                "opSubstr" => Function::SubStr,
                "opReplace" => Function::Replace,
                "opUcase" => Function::UCase,
                "opLcase" => Function::LCase,
                "opStrBefore" => Function::StrBefore,
                "opStrAfter" => Function::StrAfter,
                "opStrLen" => Function::StrLen,
                "opContains" => Function::Contains,
                "opStrStarts" => Function::StrStarts,
                "opStrEnds" => Function::StrEnds,
                "opEncodeForUri" => Function::EncodeForUri,
                "opRegex" => Function::Regex,
                "opDecimal" => Function::Custom(iri("http://www.w3.org/2001/XMLSchema#decimal")?),
                _ => return Err(error(format!("unsupported expression operator: {name}"))),
            };
            Ok(E::FunctionCall(function, args))
        }
    }
}

/// An invocation-owned accumulator shared by cell laws and profile output.
#[derive(Default)]
pub(crate) struct LegBuilder {
    template: Vec<TriplePattern>,
    branches: Vec<GraphPattern>,
}

impl LegBuilder {
    pub(crate) fn push(&mut self, template: &[TriplePattern], pattern: &GraphPattern) {
        for triple in template {
            if !self.template.contains(triple) {
                self.template.push(triple.clone());
            }
        }
        if !self.branches.contains(pattern) {
            self.branches.push(pattern.clone());
        }
    }

    pub(crate) fn finish(self) -> Option<Query> {
        if self.branches.is_empty() {
            return None;
        }
        fn balanced(mut values: Vec<GraphPattern>) -> GraphPattern {
            if values.len() == 1 {
                return values.remove(0);
            }
            let right = values.split_off(values.len() / 2);
            GraphPattern::Union {
                left: Box::new(balanced(values)),
                right: Box::new(balanced(right)),
            }
        }
        Some(Query::Construct {
            template: self
                .template
                .into_iter()
                .map(|triple| QuadPattern {
                    triple,
                    graph: None,
                })
                .collect(),
            pattern: balanced(self.branches),
            dataset: QueryDataset::default(),
            base_iri: None,
            version: None,
        })
    }
}

/// Use PurRDF's native template and WHERE serializer. Its public Modify renderer
/// shares those exact SPARQL productions with CONSTRUCT; only the fixed head token
/// changes here. No update is executed and no emitted text is parsed internally.
pub(crate) fn emit(query: Query) -> gmeow_errors::Result<String> {
    let Query::Construct {
        template,
        pattern,
        dataset,
        base_iri,
        version,
    } = query
    else {
        return Err(error("mapping emission requires a CONSTRUCT carrier"));
    };
    if dataset != QueryDataset::default() || base_iri.is_some() || version.is_some() {
        return Err(error(
            "mapping emission requires the declared default dataset without a prologue override",
        ));
    }
    let output = GraphUpdateOperation::DeleteInsert {
        delete: Vec::new(),
        insert: template,
        with: None,
        using: Vec::new(),
        pattern: Box::new(pattern),
    }
    .to_string();
    let body = output
        .strip_prefix("INSERT ")
        .ok_or_else(|| error("native template serializer did not emit the required INSERT head"))?;
    Ok(format!("CONSTRUCT {body}\n"))
}

pub(crate) fn class_template(
    anchor: &str,
    class: TermPattern,
) -> gmeow_errors::Result<Vec<TriplePattern>> {
    Ok(vec![triple(var(anchor), RDF_TYPE, class)?])
}
