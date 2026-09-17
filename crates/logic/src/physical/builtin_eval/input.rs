// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed input decoding for the one native builtin algebra.

use std::borrow::Cow;

use purrdf::TermValue;

use super::{Operand, Value, parse_typed_value, parse_value_surface};
use crate::query_ir::QTerm;

/// Reference-oracle text or an already typed native substitution value.
pub(super) enum Binding<'a> {
    Surface(Cow<'a, str>),
    Native(&'a TermValue),
}

impl<'a> Binding<'a> {
    fn numeric(&self) -> Option<Value> {
        match self {
            Self::Surface(surface) => parse_value_surface(surface),
            Self::Native(TermValue::Literal {
                lexical_form,
                datatype,
                language: None,
                direction: None,
            }) => parse_typed_value(lexical_form, datatype),
            Self::Native(_) => None,
        }
    }

    fn iri(self) -> Option<Cow<'a, str>> {
        match self {
            Self::Native(TermValue::Iri(iri)) => Some(Cow::Borrowed(iri)),
            Self::Native(_) => None,
            Self::Surface(Cow::Borrowed(surface)) => bare_iri(surface).map(Cow::Borrowed),
            Self::Surface(Cow::Owned(surface)) => {
                bare_iri(&surface).map(|iri| Cow::Owned(iri.to_owned()))
            }
        }
    }
}

fn bare_iri(surface: &str) -> Option<&str> {
    surface.strip_prefix('<')?.strip_suffix('>')
}

/// Resolve numeric binding mode without conflating absent and nonnumeric values.
pub(super) fn resolve_operand<'a>(
    term: &QTerm,
    lookup: &impl Fn(&str) -> Option<Binding<'a>>,
) -> Operand {
    let value = match term {
        QTerm::Num(n) => return Operand::Bound(Value::Int(*n)),
        QTerm::Var(name) => match lookup(name) {
            Some(binding) => binding.numeric(),
            None => return Operand::Unbound,
        },
        QTerm::Const(surface) => parse_value_surface(surface),
        QTerm::Struct(_) | QTerm::Triple { .. } => None,
    };
    value.map_or(Operand::NonNumeric, Operand::Bound)
}

/// Borrow an IRI directly from the substitution or constant. Only an owned
/// reference-oracle surface requires an owned result; native probes allocate none.
pub(super) fn resolve_iri_operand<'a>(
    term: &'a QTerm,
    lookup: &impl Fn(&str) -> Option<Binding<'a>>,
) -> Option<Cow<'a, str>> {
    match term {
        QTerm::Const(surface) => bare_iri(surface).map(Cow::Borrowed),
        QTerm::Var(name) => lookup(name)?.iri(),
        QTerm::Num(_) | QTerm::Struct(_) | QTerm::Triple { .. } => None,
    }
}

#[cfg(test)]
mod tests;
