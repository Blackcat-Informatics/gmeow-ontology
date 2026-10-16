// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Qualified object counts as ordinary first-order witness laws. Equality is
//! denotational; source names never imply a unique-name assumption.

use super::*;

const SAME_AS: &str = "https://blackcatinformatics.ca/logic/sameAs";

/// Independent authored bounds remain conjunctive for the same qualifier.
pub(super) const BOUNDS: [&str; 3] = [
    "minQualifiedCardinality",
    "maxQualifiedCardinality",
    "qualifiedCardinality",
];

/// Interpret the count through the native XSD integer value space. String
/// lexicals follow the existing GMEOW cardinality-field convention.
pub(super) fn count(dataset: &RdfDataset, id: TermId) -> Result<usize, ClassRefusal> {
    let invalid = || ClassRefusal::Malformed {
        code: "MALFORMED_CLASS_CARDINALITY",
        reason: "cardinality requires a nonnegative native integer count".into(),
    };
    let TermRef::Literal {
        lexical,
        datatype,
        language: None,
        direction: None,
    } = dataset.resolve(id)
    else {
        return Err(invalid());
    };
    let TermRef::Iri(iri) = dataset.resolve(datatype) else {
        return Err(invalid());
    };
    let datatype = if iri == XSD_STRING {
        purrdf::xsd::XsdDatatype::NonNegativeInteger
    } else {
        purrdf::xsd::XsdDatatype::from_iri(iri).ok_or_else(invalid)?
    };
    if datatype.integer_range().is_none() {
        return Err(invalid());
    }
    let parsed =
        purrdf::xsd::parse(lexical, datatype).map_err(|error| ClassRefusal::Malformed {
            code: "MALFORMED_CLASS_CARDINALITY",
            reason: format!("cardinality native integer interpretation failed: {error}"),
        })?;
    let purrdf::xsd::XsdValue::Integer { value, .. } = &parsed else {
        return Err(invalid());
    };
    if value.is_negative() {
        return Err(invalid());
    }
    parsed
        .as_i128()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(unrepresentable)
}

/// Refuse a valid mathematical count that cannot be represented by a materialized
/// witness formula on this target; never truncate it or emit a weaker bound.
pub(super) fn unrepresentable() -> ClassRefusal {
    ClassRefusal::Blocked {
        code: "CLASS_CARDINALITY_NOT_REPRESENTABLE",
        reason: "the qualified count cannot be represented by this target's witness formula".into(),
    }
}

/// At least zero successors is true without an unused qualifier binding.
pub(super) fn zero_minimum(subject: &Term) -> Formula {
    atom(SAME_AS, vec![subject.clone(), subject.clone()])
}

/// Require n pairwise unequal qualifying successors. Zero is a tautology;
/// negating this law for n+1 gives an upper bound without inventing equality.
pub(super) fn minimum(
    property: &str,
    subject: &Term,
    class: &Term,
    count: usize,
) -> Result<Formula, ClassRefusal> {
    if count == 0 {
        return Ok(zero_minimum(subject));
    }
    let size = count
        .checked_mul(count - 1)
        .and_then(|pairs| count.checked_mul(2)?.checked_add(pairs / 2))
        .ok_or_else(unrepresentable)?;
    std::alloc::Layout::array::<String>(count).map_err(|_| unrepresentable())?;
    std::alloc::Layout::array::<Formula>(size).map_err(|_| unrepresentable())?;
    let mut vars: Vec<String> = Vec::with_capacity(count);
    let mut conditions = Vec::with_capacity(size);
    for index in 0..count {
        let name = format!("class_count_{index}");
        let witness = Term::Var(name.clone());
        conditions.push(atom(property, vec![subject.clone(), witness.clone()]));
        conditions.push(atom(
            LOGIC_INSTANCE_OF,
            vec![witness.clone(), class.clone()],
        ));
        for previous in &vars {
            conditions.push(Formula::Not(Box::new(atom(
                SAME_AS,
                vec![Term::Var(previous.clone()), witness.clone()],
            ))));
        }
        vars.push(name);
    }
    Ok(Formula::Exists {
        vars,
        body: Box::new(Formula::And(conditions)),
    })
}
