// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Sound bounds on a compiled value space, shared with datatype obligations.
//! Primitive XSD range decisions belong to PurRDF. Exact rational endpoints and
//! native literal identity remain in the common GMEOW value interpretation.

use super::{Facet, Node};
use crate::reason::value::{LiteralMeaning, LiteralValue};
use purrdf::xsd::{XsdDatatype, XsdValue, range, rational::Rational};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ValueSpaceBounds {
    lower: u128,
    upper: Option<u128>,
}

impl ValueSpaceBounds {
    const UNKNOWN: Self = Self {
        lower: 0,
        upper: None,
    };
    const INFINITE: Self = Self {
        lower: u128::MAX,
        upper: None,
    };

    fn exactly(count: u128) -> Self {
        Self {
            lower: count,
            upper: Some(count),
        }
    }

    pub(super) fn exact_count(self) -> Option<u128> {
        self.upper.filter(|upper| *upper == self.lower)
    }

    /// Prove capacity or impossibility for the requested finite count. An unknown
    /// answer is not a consistency certificate or an empty-space assertion.
    pub(crate) fn admits(self, count: u128) -> Option<bool> {
        if self.lower >= count {
            Some(true)
        } else if self.upper.is_some_and(|upper| upper < count) {
            Some(false)
        } else {
            None
        }
    }
}

pub(super) fn analyze(nodes: &[Node], root: usize) -> ValueSpaceBounds {
    // A restriction summarizes its complete base/facet chain in one primitive
    // call. Do not separately lower every prefix of that chain while deriving
    // the requested root's capacity. Other shared operands are visited once.
    let mut needed = std::collections::BTreeSet::new();
    let mut pending = vec![root];
    while let Some(index) = pending.pop() {
        if !needed.insert(index) {
            continue;
        }
        match &nodes[index] {
            Node::Complement(inner) => pending.push(*inner),
            Node::Intersection(members) | Node::Union(members) => pending.extend(members),
            Node::Named(_) | Node::Enumeration { .. } | Node::Restriction { .. } => {}
        }
    }
    let mut bounds = Vec::<ValueSpaceBounds>::with_capacity(nodes.len());
    // A proved infinite complement, not merely an unsupported membership.
    let mut infinite_outside = Vec::with_capacity(nodes.len());
    for node in nodes {
        let requested = needed.contains(&bounds.len());
        let (bound, outside) = match node {
            Node::Named(iri) => (
                if requested {
                    named(iri)
                } else {
                    ValueSpaceBounds::UNKNOWN
                },
                iri != "http://www.w3.org/2000/01/rdf-schema#Literal",
            ),
            Node::Enumeration { members, .. } => (
                if requested {
                    enumeration(members)
                } else {
                    ValueSpaceBounds::UNKNOWN
                },
                true,
            ),
            Node::Restriction { base, .. } => (
                if requested {
                    restriction(nodes, bounds.len())
                } else {
                    ValueSpaceBounds::UNKNOWN
                },
                infinite_outside[*base],
            ),
            Node::Complement(inner) => {
                let bound = if let Node::Complement(original) = &nodes[*inner] {
                    bounds[*original]
                } else if bounds[*inner].upper == Some(0) || infinite_outside[*inner] {
                    ValueSpaceBounds::INFINITE
                } else if is_universe(&nodes[*inner]) {
                    ValueSpaceBounds::exactly(0)
                } else {
                    ValueSpaceBounds::UNKNOWN
                };
                (bound, false)
            }
            Node::Intersection(members) => {
                let members: std::collections::BTreeSet<_> = members.iter().copied().collect();
                let bound = if members.is_empty() {
                    ValueSpaceBounds::INFINITE
                } else if members.len() == 1 {
                    bounds[*members.first().expect("nonempty")]
                } else {
                    ValueSpaceBounds {
                        lower: 0,
                        upper: members
                            .iter()
                            .filter_map(|index| bounds[*index].upper)
                            .min(),
                    }
                };
                (bound, members.iter().any(|index| infinite_outside[*index]))
            }
            Node::Union(members) => {
                let members: std::collections::BTreeSet<_> = members.iter().copied().collect();
                let bound = if members.len() == 1 {
                    bounds[*members.first().expect("one member")]
                } else {
                    ValueSpaceBounds {
                        lower: members
                            .iter()
                            .map(|index| bounds[*index].lower)
                            .max()
                            .unwrap_or(0),
                        upper: members
                            .iter()
                            .try_fold(0u128, |sum, index| sum.checked_add(bounds[*index].upper?)),
                    }
                };
                (bound, bound.upper.is_some())
            }
        };
        bounds.push(bound);
        infinite_outside.push(outside);
    }
    bounds[root]
}

fn is_universe(node: &Node) -> bool {
    matches!(node, Node::Named(iri) if iri == "http://www.w3.org/2000/01/rdf-schema#Literal")
        || matches!(node, Node::Intersection(members) if members.is_empty())
}

pub(super) fn named(iri: &str) -> ValueSpaceBounds {
    if matches!(
        iri,
        "http://www.w3.org/2002/07/owl#rational"
            | "http://www.w3.org/2002/07/owl#real"
            | "http://www.w3.org/2000/01/rdf-schema#Literal"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
    ) {
        return ValueSpaceBounds::INFINITE;
    }
    let Some(datatype) = XsdDatatype::from_iri(iri) else {
        return ValueSpaceBounds::UNKNOWN;
    };
    // PurRDF owns primitive XSD capacities. Authored math declarations are checked
    // against this same result; a second hand-maintained count table is unnecessary.
    match range::cardinality(&range::DataRange::Datatype(datatype)) {
        range::Cardinality::Exactly(count) => ValueSpaceBounds::exactly(u128::from(count)),
        range::Cardinality::Unbounded => ValueSpaceBounds::INFINITE,
        range::Cardinality::AtLeast(count) => ValueSpaceBounds {
            lower: u128::from(count),
            upper: None,
        },
        range::Cardinality::Undecided => {
            match range::satisfiability(&range::DataRange::Datatype(datatype)) {
                range::Satisfiability::Inhabited => ValueSpaceBounds {
                    lower: 1,
                    upper: None,
                },
                range::Satisfiability::Empty => ValueSpaceBounds::exactly(0),
                range::Satisfiability::Undecided => ValueSpaceBounds::UNKNOWN,
            }
        }
    }
}

fn enumeration(members: &[std::sync::Arc<LiteralValue>]) -> ValueSpaceBounds {
    let mut unique: Vec<&LiteralValue> = Vec::new();
    let mut unknown = false;
    for member in members {
        if matches!(member.meaning, LiteralMeaning::Opaque(_)) {
            unknown = true;
            continue;
        }
        if !unique
            .iter()
            .any(|prior| member.same_value(prior) == Some(true))
        {
            unique.push(member);
        }
    }
    if unknown {
        ValueSpaceBounds::UNKNOWN
    } else {
        ValueSpaceBounds::exactly(unique.len() as u128)
    }
}

fn restriction(nodes: &[Node], root: usize) -> ValueSpaceBounds {
    let mut index = root;
    let mut facets = Vec::new();
    while let Node::Restriction {
        base,
        facets: current,
    } = &nodes[index]
    {
        facets.extend(current);
        index = *base;
    }
    let Node::Named(iri) = &nodes[index] else {
        return ValueSpaceBounds::UNKNOWN;
    };
    if matches!(
        iri.as_str(),
        "http://www.w3.org/2002/07/owl#rational"
            | "http://www.w3.org/2002/07/owl#real"
            | "http://www.w3.org/2001/XMLSchema#decimal"
    ) {
        return dense(&facets, iri.ends_with("#decimal"));
    }
    let Some(base) = XsdDatatype::from_iri(iri) else {
        return ValueSpaceBounds::UNKNOWN;
    };
    let Some(facets) = facets
        .into_iter()
        .map(purrdf_facet)
        .collect::<Option<Vec<_>>>()
    else {
        return ValueSpaceBounds::UNKNOWN;
    };
    let range = range::DataRange::Restriction { base, facets };
    match range::cardinality(&range) {
        range::Cardinality::Exactly(count) => ValueSpaceBounds::exactly(u128::from(count)),
        range::Cardinality::Unbounded => ValueSpaceBounds::INFINITE,
        range::Cardinality::AtLeast(count) => ValueSpaceBounds {
            lower: u128::from(count),
            upper: None,
        },
        range::Cardinality::Undecided => match range::satisfiability(&range) {
            range::Satisfiability::Empty => ValueSpaceBounds::exactly(0),
            range::Satisfiability::Inhabited => ValueSpaceBounds {
                lower: 1,
                upper: None,
            },
            range::Satisfiability::Undecided => ValueSpaceBounds::UNKNOWN,
        },
    }
}

fn purrdf_facet(facet: &Facet) -> Option<range::Facet> {
    Some(match facet {
        Facet::Ordered { kind, bound } => {
            let value = match &bound.meaning {
                LiteralMeaning::Rational(value) if value.denominator() == 1 => XsdValue::Integer {
                    value: value.numerator(),
                    datatype: XsdDatatype::Integer,
                },
                LiteralMeaning::Native(value) => value.clone(),
                _ => return None,
            };
            match kind {
                0 => range::Facet::MinInclusive(value),
                1 => range::Facet::MaxInclusive(value),
                2 => range::Facet::MinExclusive(value),
                3 => range::Facet::MaxExclusive(value),
                _ => unreachable!("admitted order facet"),
            }
        }
        Facet::Length { kind, bound } => {
            let count = u64::try_from(*bound).ok()?;
            match kind {
                4 => range::Facet::Length(count),
                5 => range::Facet::MinLength(count),
                6 => range::Facet::MaxLength(count),
                _ => unreachable!("admitted length facet"),
            }
        }
        Facet::Unsupported => return None,
    })
}

fn dense(facets: &[&Facet], decimal: bool) -> ValueSpaceBounds {
    let mut lower: Option<(Rational, bool)> = None;
    let mut upper: Option<(Rational, bool)> = None;
    for facet in facets {
        let Facet::Ordered { kind, bound } = facet else {
            return ValueSpaceBounds::UNKNOWN;
        };
        let LiteralMeaning::Rational(value) = bound.meaning else {
            return ValueSpaceBounds::UNKNOWN;
        };
        let strict = *kind >= 2;
        let (side, minimum) = if *kind == 0 || *kind == 2 {
            (&mut lower, true)
        } else {
            (&mut upper, false)
        };
        match side {
            Some((prior, prior_strict)) if *prior == value => *prior_strict |= strict,
            Some((prior, _)) if (minimum && *prior > value) || (!minimum && *prior < value) => {}
            _ => *side = Some((value, strict)),
        }
    }
    match (lower, upper) {
        (Some((lo, lo_strict)), Some((hi, hi_strict))) if lo >= hi => {
            if lo > hi || lo_strict || hi_strict {
                return ValueSpaceBounds::exactly(0);
            }
            let singleton = LiteralValue {
                meaning: LiteralMeaning::Rational(lo),
            };
            ValueSpaceBounds::exactly(u128::from(
                !decimal
                    || singleton.named_datatype("http://www.w3.org/2001/XMLSchema#decimal")
                        == Some(true),
            ))
        }
        _ => ValueSpaceBounds::INFINITE,
    }
}
