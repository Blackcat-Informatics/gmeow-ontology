// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Binding ownership in a shared CONSTRUCT template. Native alpha-renaming keeps
//! unrelated UNION branches from filling each other's output variables. Ground
//! template rows use a branch-bound predicate, preserving fresh template blanks.

use super::{LoweredLeg, native};
use crate::projections::get_leg::{Atom, Expr, Item, ProfileBinding, ProjectionCell};
use purrdf::sparql::{
    BlankNode, Expression, GraphPattern, NamedNodePattern, TermPattern, TriplePattern, Variable,
};
use std::collections::{BTreeMap, BTreeSet};

/// Generated roles are allocated before source and generated variables enter the
/// same algebra. The inventory includes every authored use, even an unbound
/// template variable or a variable occurring only inside an expression/guard.
pub(crate) struct HelperNames {
    pub(super) class: String,
    pub(super) bearer: String,
    pub(super) language: String,
    pub(super) variety: String,
    pub(super) internal_tag: String,
    pub(super) external_tag: String,
    pub(super) script: String,
    pub(super) final_value: String,
}

impl HelperNames {
    pub(crate) fn for_binding(cell: &ProjectionCell, binding: &ProfileBinding) -> Self {
        fn atom(atom: &Atom, used: &mut BTreeSet<String>) {
            used.insert(atom.subject_var.clone());
            used.extend(atom.predicate_var.iter().cloned());
            used.extend(atom.object_var.iter().cloned());
        }
        fn items(values: &[Item], used: &mut BTreeSet<String>) {
            for value in values {
                match value {
                    Item::Atom(value) => atom(value, used),
                    Item::Group(values) => items(values, used),
                }
            }
        }
        fn expression(value: &Expr, used: &mut BTreeSet<String>) {
            match value {
                Expr::Var(value) => {
                    used.insert(value.clone());
                }
                Expr::Op { args, .. } => {
                    for value in args {
                        expression(value, used);
                    }
                }
                Expr::ConstTerm(_) => {}
            }
        }
        fn fresh(used: &mut BTreeSet<String>, stem: &str) -> String {
            if used.insert(stem.to_owned()) {
                return stem.to_owned();
            }
            for index in 0..=used.len() {
                let candidate = format!("{stem}_{index}");
                if used.insert(candidate.clone()) {
                    return candidate;
                }
            }
            unreachable!("more distinct candidate names than occupied names")
        }
        let source = &cell.pattern;
        let mut used = BTreeSet::from([source.anchor.clone()]);
        used.extend(source.value.iter().cloned());
        items(&source.atoms, &mut used);
        for value in source
            .suppress_when
            .iter()
            .chain(&source.project_when)
            .chain(&source.exclude_when)
            .chain(&binding.template_atoms)
        {
            atom(value, &mut used);
        }
        for value in &source.filters {
            expression(value, &mut used);
        }
        for value in source.binds.iter().chain(&source.mints) {
            used.insert(value.var.clone());
            expression(&value.expr, &mut used);
        }
        Self {
            class: fresh(
                &mut used,
                &format!("{}Class", source.value.as_deref().unwrap_or("")),
            ),
            bearer: fresh(&mut used, "_supBearer"),
            language: fresh(&mut used, "_lang"),
            variety: fresh(&mut used, "_variety"),
            internal_tag: fresh(&mut used, "_intTag"),
            external_tag: fresh(&mut used, "_extTag"),
            script: fresh(&mut used, "_sc"),
            final_value: fresh(
                &mut used,
                &format!("_final_{}", source.value.as_deref().unwrap_or("")),
            ),
        }
    }
}

fn encoded(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Scope {
    prefix: String,
}

impl Scope {
    fn variable(&self, value: &mut Variable) {
        *value = Variable::new(format!("{}_v_{}", self.prefix, encoded(value.as_str())));
    }

    fn term(&self, value: &mut TermPattern, template: bool) {
        match value {
            TermPattern::Variable(value) => self.variable(value),
            TermPattern::BlankNode(value) => {
                let role = if template { "t" } else { "q" };
                *value = BlankNode::new(format!(
                    "{}_{role}_{}",
                    self.prefix,
                    encoded(value.as_str())
                ));
            }
            TermPattern::Triple(value) => self.triple(value, template),
            TermPattern::NamedNode(_) | TermPattern::Literal(_) => {}
        }
    }

    fn triple(&self, triple: &mut TriplePattern, template: bool) {
        self.term(&mut triple.subject, template);
        self.term(&mut triple.object, template);
        if let NamedNodePattern::Variable(value) = &mut triple.predicate {
            self.variable(value);
        }
    }

    fn expression(&self, expression: &mut Expression) -> gmeow_errors::Result<()> {
        use Expression as E;
        match expression {
            E::Variable(value) | E::Bound(value) => self.variable(value),
            E::Or(a, b)
            | E::And(a, b)
            | E::Equal(a, b)
            | E::SameTerm(a, b)
            | E::Greater(a, b)
            | E::GreaterOrEqual(a, b)
            | E::Less(a, b)
            | E::LessOrEqual(a, b)
            | E::Add(a, b)
            | E::Subtract(a, b)
            | E::Multiply(a, b)
            | E::Divide(a, b) => {
                self.expression(a)?;
                self.expression(b)?;
            }
            E::UnaryPlus(value) | E::UnaryMinus(value) | E::Not(value) => self.expression(value)?,
            E::In(value, args) => {
                self.expression(value)?;
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::If(a, b, c) => {
                self.expression(a)?;
                self.expression(b)?;
                self.expression(c)?;
            }
            E::Coalesce(args) | E::FunctionCall(_, args) => {
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::Exists(pattern) => self.pattern(pattern)?,
            E::NamedNode(_) | E::Literal(_) => {}
        }
        Ok(())
    }

    fn pattern(&self, pattern: &mut GraphPattern) -> gmeow_errors::Result<()> {
        use GraphPattern as P;
        match pattern {
            P::Bgp { patterns } => {
                for triple in patterns {
                    self.triple(triple, false);
                }
            }
            P::Path {
                subject, object, ..
            } => {
                self.term(subject, false);
                self.term(object, false);
            }
            P::Join { left, right } | P::Union { left, right } => {
                self.pattern(left)?;
                self.pattern(right)?;
            }
            P::LeftJoin {
                left,
                right,
                expression,
            } => {
                self.pattern(left)?;
                self.pattern(right)?;
                if let Some(expression) = expression {
                    self.expression(expression)?;
                }
            }
            P::Filter { expr, inner } => {
                self.expression(expr)?;
                self.pattern(inner)?;
            }
            P::Extend {
                inner,
                variable,
                expression,
            } => {
                self.pattern(inner)?;
                self.variable(variable);
                self.expression(expression)?;
            }
            P::Values { variables, .. } => {
                for value in variables {
                    self.variable(value);
                }
            }
            _ => {
                return Err(native::error(
                    "mapping ownership admission encountered a graph operator outside the mapping grammar",
                ));
            }
        }
        Ok(())
    }
}

fn term_has_variable(term: &TermPattern) -> bool {
    match term {
        TermPattern::Variable(_) => true,
        TermPattern::Triple(triple) => triple_has_variable(triple),
        TermPattern::NamedNode(_) | TermPattern::BlankNode(_) | TermPattern::Literal(_) => false,
    }
}

fn triple_has_variable(triple: &TriplePattern) -> bool {
    matches!(triple.predicate, NamedNodePattern::Variable(_))
        || term_has_variable(&triple.subject)
        || term_has_variable(&triple.object)
}

/// Assign one source binding's lexical names to a disjoint native scope. The same
/// owned result is shared by its isolated law and its profile query. Template
/// blanks remain templates; a fresh branch-bound predicate guards ground rows.
pub(super) fn admit(mut leg: LoweredLeg, key: &str) -> gmeow_errors::Result<LoweredLeg> {
    use sha2::{Digest, Sha256};
    let digest: String = Sha256::digest(key.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let scope = Scope {
        prefix: format!("m{digest}"),
    };
    scope.pattern(&mut leg.pattern)?;
    let mut guards = BTreeMap::new();
    for triple in &mut leg.template {
        scope.triple(triple, true);
        if !triple_has_variable(triple) {
            let NamedNodePattern::NamedNode(predicate) = &triple.predicate else {
                unreachable!("variable predicate detected above")
            };
            let next = guards.len();
            let (_, variable) = guards
                .entry(predicate.as_str().to_owned())
                .or_insert_with(|| {
                    (
                        predicate.clone(),
                        Variable::new(format!("{}_p_{next}", scope.prefix)),
                    )
                });
            triple.predicate = NamedNodePattern::Variable(variable.clone());
        }
    }
    for (_, (predicate, variable)) in guards {
        leg.pattern = GraphPattern::Extend {
            inner: Box::new(leg.pattern),
            variable,
            expression: Expression::NamedNode(predicate),
        };
    }
    Ok(leg)
}
