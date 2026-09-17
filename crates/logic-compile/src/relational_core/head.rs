// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Positive conjunctive heads with lexically scoped existential witnesses.
//!
//! `B(x) → ∃z. (P(x,z) ∧ Q(z))` is one tuple-generating dependency. It must
//! never become independent clauses that choose unrelated witnesses. No Skolem
//! function syntax or second clausifier is needed: the native chase already owns
//! witness invention and its termination admission.

use super::*;
use std::collections::BTreeMap;

pub(super) fn lower(
    head: &Formula,
    body: Vec<RcAtom>,
    numeric: Vec<RcNumeric>,
) -> Result<RcRule, &'static str> {
    let mut bound = body_bound_vars(&body);
    bound.extend(
        numeric
            .iter()
            .filter_map(RcNumeric::output_var)
            .map(str::to_owned),
    );
    let mut used = bound.clone();
    collect_names(head, &mut used);
    let mut lowering = Head {
        bound,
        used,
        scope: Vec::new(),
        next_witness: 0,
        reifiers: BTreeMap::new(),
        atoms: Vec::new(),
    };
    lowering.visit(head)?;
    let mut atoms = lowering.atoms.into_iter();
    let head = atoms.next().ok_or("empty positive head conjunction")?;
    Ok(RcRule {
        numeric,
        head,
        head_conjuncts: atoms.collect(),
        body,
        distinct_pairs: Vec::new(),
    })
}

struct Head {
    bound: BTreeSet<String>,
    used: BTreeSet<String>,
    /// Authored binder name and its fresh, unsigilled native name, innermost last.
    scope: Vec<(String, String)>,
    next_witness: usize,
    reifiers: BTreeMap<String, RcTerm>,
    atoms: Vec<RcAtom>,
}

impl Head {
    fn visit(&mut self, formula: &Formula) -> Result<(), &'static str> {
        match formula {
            Formula::Exists { vars, body } => {
                let depth = self.scope.len();
                for name in vars {
                    let fresh = loop {
                        let candidate = format!("existH{}", self.next_witness);
                        self.next_witness += 1;
                        if self.used.insert(format!("?{candidate}")) {
                            break candidate;
                        }
                    };
                    self.scope.push((name.clone(), fresh));
                }
                self.visit(body)?;
                self.scope.truncate(depth);
                Ok(())
            }
            Formula::And(parts) => {
                if parts.is_empty() {
                    return Err("empty positive head conjunction");
                }
                for part in parts {
                    self.visit(part)?;
                }
                Ok(())
            }
            Formula::Atom { relation, args } => {
                let terms = args.iter().map(|term| self.rename(term)).collect();
                self.atom(relation.clone(), terms)
            }
            _ => Err("head is not a positive existential conjunction of atoms"),
        }
    }

    fn rename(&self, term: &Term) -> Term {
        match term {
            Term::Var(name) => self
                .scope
                .iter()
                .rev()
                .find(|(authored, _)| authored == name)
                .map_or_else(|| term.clone(), |(_, fresh)| Term::Var(fresh.clone())),
            // Applications remain unsupported by the shared atom converter. Renaming
            // their arguments still preserves lexical scope at this legalization seam.
            Term::App { symbol, args } => Term::App {
                symbol: symbol.clone(),
                args: args.iter().map(|arg| self.rename(arg)).collect(),
            },
            _ => term.clone(),
        }
    }

    fn atom(&mut self, relation: Term, args: Vec<Term>) -> Result<(), &'static str> {
        let Term::Iri(rel) = &relation else {
            return Err("non-IRI relation in atom");
        };
        if NumericOperator::from_iri(rel).is_some() {
            return Err("interpreted numeric relation cannot be asserted in a head");
        }
        let terms = args
            .iter()
            .map(|term| formula_term_to_rc(term, true))
            .collect::<Result<Vec<_>, _>>()?;
        for term in &terms {
            if let RcTerm::Var(name) = term
                && !self.bound.contains(name)
                && !self
                    .scope
                    .iter()
                    .any(|(_, fresh)| name == &format!("?{fresh}"))
            {
                return Err("head variable not bound by the body or an explicit existential");
            }
        }
        if args.len() < 3 {
            self.atoms.extend(formula_atom_to_rc_atoms(
                &Formula::Atom { relation, args },
                AtomPosition::Head,
            )?);
            return Ok(());
        }
        let mut key = rel.clone();
        for term in &terms {
            key.push('\u{1f}');
            key.push_str(&term.key());
        }
        let reifier = if let Some(reifier) = self.reifiers.get(&key) {
            reifier.clone()
        } else {
            let base = format!("?naryH{}", sha256_12(&key));
            let mut name = base.clone();
            let mut suffix = 0;
            while !self.used.insert(name.clone()) {
                name = format!("{base}v{suffix}");
                suffix += 1;
            }
            let reifier = RcTerm::Var(name);
            self.reifiers.insert(key, reifier.clone());
            reifier
        };
        self.atoms.push(RcAtom {
            subject: reifier.clone(),
            predicate: instance_of_iri(),
            object: RcTerm::Iri(rel.clone()),
            negated: false,
        });
        self.atoms
            .extend(terms.into_iter().enumerate().map(|(i, object)| RcAtom {
                subject: reifier.clone(),
                predicate: nary_arg_iri(i),
                object,
                negated: false,
            }));
        Ok(())
    }
}

fn collect_names(formula: &Formula, names: &mut BTreeSet<String>) {
    match formula {
        Formula::Atom { relation, args } => {
            collect_term_names(relation, names);
            for arg in args {
                collect_term_names(arg, names);
            }
        }
        Formula::Exists { vars, body } | Formula::Forall { vars, body } => {
            names.extend(vars.iter().map(|name| format!("?{name}")));
            collect_names(body, names);
        }
        Formula::And(parts) | Formula::Or(parts) => {
            for part in parts {
                collect_names(part, names);
            }
        }
        Formula::Not(body) => collect_names(body, names),
        Formula::Implies(left, right) | Formula::Iff(left, right) => {
            collect_names(left, names);
            collect_names(right, names);
        }
    }
}

fn collect_term_names(term: &Term, names: &mut BTreeSet<String>) {
    match term {
        Term::Var(name) | Term::SequenceMarker(name) => {
            names.insert(format!("?{name}"));
        }
        Term::App { args, .. } => {
            for arg in args {
                collect_term_names(arg, names);
            }
        }
        Term::Iri(_) | Term::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests;
