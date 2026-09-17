// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One bounded signature analysis of a closed canonical sentence.

use gmeow_logic_compile::ir::{Formula, Term};

use super::*;

/// A closed formula retained in its canonical native IR. Transport reuses this
/// body and its symbol-role analysis; no derived formula or RDF serialization is
/// constructed. Variable and connective identity remains owned by the shared IR.
#[derive(Debug)]
pub struct SentenceBody {
    formula: Arc<Formula>,
    pub(super) symbols: Vec<(String, BTreeSet<SymbolRole>)>,
    nodes: usize,
    bytes: usize,
    depth: usize,
}

impl SentenceBody {
    pub fn new(
        formula: Arc<Formula>,
        limits: PresentationLimits,
    ) -> gmeow_errors::Result<Arc<Self>> {
        let mut analysis = Analysis {
            symbols: BTreeMap::new(),
            bound: Vec::new(),
            nodes: 0,
            bytes: 0,
            depth: 0,
            limits,
        };
        analysis.formula(&formula, 0)?;
        for iri in analysis.symbols.keys() {
            super::super::algebra_iri(iri)?;
        }
        Ok(Arc::new(Self {
            formula,
            symbols: analysis.symbols.into_iter().collect(),
            nodes: analysis.nodes,
            bytes: analysis.bytes,
            depth: analysis.depth,
        }))
    }
    pub fn formula(&self) -> &Arc<Formula> {
        &self.formula
    }
    pub fn symbols(&self) -> &[(String, BTreeSet<SymbolRole>)] {
        &self.symbols
    }
    pub(super) fn admit_limits(&self, limits: PresentationLimits) -> gmeow_errors::Result<()> {
        bounded(self.nodes, limits.max_formula_nodes, "formula nodes")?;
        bounded(self.bytes, limits.max_formula_bytes, "formula bytes")?;
        bounded(
            self.depth,
            limits.max_formula_depth.min(128),
            "formula depth",
        )
    }
    pub(super) fn key(&self, bindings: &[usize]) -> ContentKey {
        let names = self
            .symbols
            .iter()
            .zip(bindings)
            .map(|((name, _), index)| {
                (
                    name.as_str(),
                    format!("urn:gmeow:presentation:generator:{index}"),
                )
            })
            .collect();
        self.formula.content_key_with_symbols(&names)
    }
}

struct Analysis {
    symbols: BTreeMap<String, BTreeSet<SymbolRole>>,
    bound: Vec<String>,
    nodes: usize,
    bytes: usize,
    depth: usize,
    limits: PresentationLimits,
}

impl Analysis {
    fn node(&mut self, depth: usize) -> gmeow_errors::Result<()> {
        self.depth = self.depth.max(depth);
        bounded(
            depth,
            self.limits.max_formula_depth.min(128),
            "formula depth",
        )?;
        add_size(
            &mut self.nodes,
            1,
            self.limits.max_formula_nodes,
            "formula nodes",
        )
    }
    fn text(&mut self, text: &str) -> gmeow_errors::Result<()> {
        add_size(
            &mut self.bytes,
            text.len(),
            self.limits.max_formula_bytes,
            "formula bytes",
        )
    }
    fn symbol(&mut self, name: &str, role: SymbolRole) -> gmeow_errors::Result<()> {
        self.text(name)?;
        self.symbols.entry(name.into()).or_default().insert(role);
        bounded(
            self.symbols.len(),
            self.limits.max_symbols,
            "formula symbols",
        )
    }
    fn term(&mut self, term: &Term, depth: usize) -> gmeow_errors::Result<()> {
        self.node(depth)?;
        match term {
            Term::Var(name) | Term::SequenceMarker(name) => {
                self.text(name)?;
                if !self.bound.contains(name) {
                    return Err(error(
                        "finite presentation axioms and caveats must be closed sentences",
                    ));
                }
            }
            Term::Iri(iri) => self.symbol(iri, SymbolRole::Individual)?,
            Term::Literal(literal) => {
                self.text(&literal.lexical_form)?;
                if let Some(datatype) = &literal.datatype {
                    self.text(datatype)?;
                }
                if let Some(language) = &literal.language {
                    self.text(language)?;
                }
            }
            Term::App { symbol, args } => {
                if args.is_empty() {
                    return Err(error(
                        "nullary application is not an admitted presentation term",
                    ));
                }
                let role = if args
                    .iter()
                    .any(|term| matches!(term, Term::SequenceMarker(_)))
                {
                    SymbolRole::VariadicFunction
                } else {
                    SymbolRole::Function(args.len())
                };
                self.symbol(symbol, role)?;
                for arg in args {
                    self.term(arg, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn formula(&mut self, formula: &Formula, depth: usize) -> gmeow_errors::Result<()> {
        self.node(depth)?;
        match formula {
            Formula::Atom { relation, args } => {
                let Term::Iri(iri) = relation else {
                    return Err(error(
                        "presentation relation must be a named canonical symbol",
                    ));
                };
                self.node(depth + 1)?;
                let role = if args
                    .iter()
                    .any(|term| matches!(term, Term::SequenceMarker(_)))
                {
                    SymbolRole::VariadicRelation
                } else {
                    SymbolRole::Relation(args.len())
                };
                self.symbol(iri, role)?;
                for arg in args {
                    self.term(arg, depth + 1)?;
                }
            }
            Formula::Not(body) => self.formula(body, depth + 1)?,
            Formula::And(bodies) | Formula::Or(bodies) => {
                if bodies.len() < 2 {
                    return Err(error(
                        "presentation conjunction/disjunction requires at least two operands",
                    ));
                }
                for body in bodies {
                    self.formula(body, depth + 1)?;
                }
            }
            Formula::Implies(left, right) | Formula::Iff(left, right) => {
                self.formula(left, depth + 1)?;
                self.formula(right, depth + 1)?;
            }
            Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
                if vars.is_empty()
                    || vars.iter().any(|name| name.trim().is_empty())
                    || vars.iter().collect::<BTreeSet<_>>().len() != vars.len()
                {
                    return Err(error(
                        "presentation quantifier requires distinct nonempty binder names",
                    ));
                }
                for name in vars {
                    self.text(name)?;
                }
                let start = self.bound.len();
                self.bound.extend(vars.iter().cloned());
                self.formula(body, depth + 1)?;
                self.bound.truncate(start);
            }
        }
        Ok(())
    }
}
