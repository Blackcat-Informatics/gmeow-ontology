// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict projection grammar over PurRDF's native default-graph indexes.

use purrdf::{RdfDataset, TermId, TermRef};

use super::*;

/// Read the relational-core projection without copying its graph into another index.
/// Missing or ambiguous fields, malformed links and dishonest loss claims fail closed.
/// Cache hydration restores the complete typed payload separately; this is its independent
/// projection validator and the reader for external relational-core exports.
pub fn parse_relational_core(dataset: &RdfDataset) -> gmeow_errors::Result<RelationalCoreProgram> {
    let reader = Reader::new(dataset);
    let root = dataset
        .term_id_by_iri(&program_iri())
        .ok_or_else(|| term::error("missing relational-core program"))?;
    reader.class(root, &class_program())?;
    let source_iri = reader
        .optional(root, "sourceIri")?
        .map(|id| term::text(dataset, id).map(str::to_owned))
        .transpose()?;
    let residue = reader
        .values(root, "lossyDrop")
        .map(|id| term::text(dataset, id).map(str::to_owned))
        .collect::<gmeow_errors::Result<BTreeSet<_>>>()?;
    let facts = reader
        .values(root, "hasFact")
        .map(|node| reader.atom(node, &class_fact()))
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let rules = reader
        .values(root, "hasRule")
        .map(|node| reader.rule(node))
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let program = finalize(facts, rules, residue, source_iri);
    let preservation = reader.one(root, "hasPreservation")?;
    if dataset.term_id_by_iri(&program.preservation().iri()) != Some(preservation) {
        return Err(term::error(
            "hasPreservation disagrees with relational-core loss evidence",
        ));
    }
    Ok(program)
}

struct Reader<'a> {
    dataset: &'a RdfDataset,
    // Fixed grammar vocabulary, bounded independently of the graph's size.
    predicates: BTreeMap<&'static str, Option<TermId>>,
}

impl<'a> Reader<'a> {
    fn new(dataset: &'a RdfDataset) -> Self {
        let predicates = [
            "sourceIri",
            "lossyDrop",
            "hasPreservation",
            "hasFact",
            "hasRule",
            "rcSubject",
            "rcPredicate",
            "rcObject",
            "rcNegated",
            "rcHead",
            "rcHeadConjunct",
            "rcBody",
            "rcNumeric",
            "rcNumericOperator",
            "rcNumericLeft",
            "rcNumericRight",
            "rcNumericResult",
            "rcIndex",
            "rcDistinct",
            "rcDistinctLeft",
            "rcDistinctRight",
            "rcObjectLiteral",
        ]
        .into_iter()
        .map(|name| {
            (
                name,
                dataset.term_id_by_iri(&format!("{LOGIC_NAMESPACE}{name}")),
            )
        })
        .collect();
        Self {
            dataset,
            predicates,
        }
    }

    fn values(&self, subject: TermId, name: &'static str) -> impl Iterator<Item = TermId> + 'a {
        let dataset = self.dataset;
        self.predicates
            .get(name)
            .copied()
            .flatten()
            .into_iter()
            .flat_map(move |predicate| {
                crate::graphutil::default_graph_pattern(
                    dataset,
                    Some(subject),
                    Some(predicate),
                    None,
                )
                .map(|quad| quad.o)
            })
    }

    fn optional(&self, node: TermId, name: &'static str) -> gmeow_errors::Result<Option<TermId>> {
        let mut values = self.values(node, name);
        let value = values.next();
        if values.next().is_some() {
            return Err(term::error(format!(
                "ambiguous {name}: requires at most one value"
            )));
        }
        Ok(value)
    }

    fn one(&self, node: TermId, name: &'static str) -> gmeow_errors::Result<TermId> {
        self.optional(node, name)?
            .ok_or_else(|| term::error(format!("missing {name}")))
    }

    fn class(&self, node: TermId, class: &str) -> gmeow_errors::Result<()> {
        self.iri(node)?;
        let pair = self
            .dataset
            .term_id_by_iri(RDF_TYPE)
            .zip(self.dataset.term_id_by_iri(class));
        let Some((predicate, class_id)) = pair else {
            return Err(term::error(format!("missing required type <{class}>")));
        };
        if crate::graphutil::default_graph_pattern(
            self.dataset,
            Some(node),
            Some(predicate),
            Some(class_id),
        )
        .next()
        .is_none()
        {
            return Err(term::error(format!("missing required type <{class}>")));
        }
        Ok(())
    }

    fn iri(&self, node: TermId) -> gmeow_errors::Result<&'a str> {
        match self.dataset.resolve(node) {
            TermRef::Iri(value) => Ok(value),
            _ => Err(term::error("record link or predicate requires an IRI")),
        }
    }

    fn scalar(
        &self,
        node: TermId,
        name: &'static str,
        datatype: &str,
    ) -> gmeow_errors::Result<purrdf::xsd::XsdValue> {
        let value = self.one(node, name)?;
        let TermRef::Literal {
            lexical,
            datatype: kind,
            language: None,
            direction: None,
        } = self.dataset.resolve(value)
        else {
            return Err(term::error(format!(
                "{name} requires a typed scalar literal"
            )));
        };
        if self.dataset.term_id_by_iri(datatype) != Some(kind) {
            return Err(term::error(format!("{name} requires <{datatype}>")));
        }
        purrdf::xsd::parse_by_iri(lexical, datatype)
            .map_err(term::error)?
            .ok_or_else(|| term::error("unknown relational-core scalar datatype"))
    }

    fn atom(&self, node: TermId, class: &str) -> gmeow_errors::Result<RcAtom> {
        self.class(node, class)?;
        let subject = term::decode(self.dataset, self.one(node, "rcSubject")?)?;
        let predicate = self.iri(self.one(node, "rcPredicate")?)?.to_owned();
        let object = term::decode(self.dataset, self.one(node, "rcObject")?)?;
        if self.optional(node, "rcObjectLiteral")?.is_some() {
            return Err(term::error(
                "retired rcObjectLiteral marker cannot accompany typed term records",
            ));
        }
        let purrdf::xsd::XsdValue::Boolean(negated) = self.scalar(
            node,
            "rcNegated",
            "http://www.w3.org/2001/XMLSchema#boolean",
        )?
        else {
            return Err(term::error("rcNegated requires a boolean"));
        };
        Ok(RcAtom {
            subject,
            predicate,
            object,
            negated,
        })
    }

    fn ordered_nodes(
        &self,
        rule: TermId,
        field: &'static str,
        label: &str,
    ) -> gmeow_errors::Result<Vec<TermId>> {
        let mut indexed = self
            .values(rule, field)
            .map(|node| {
                self.iri(node)?;
                let purrdf::xsd::XsdValue::Integer { value, .. } =
                    self.scalar(node, "rcIndex", "http://www.w3.org/2001/XMLSchema#integer")?
                else {
                    return Err(term::error("rcIndex requires an integer"));
                };
                let index = usize::try_from(value).map_err(term::error)?;
                Ok((index, node))
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        indexed.sort_unstable_by_key(|(index, _)| *index);
        if indexed
            .iter()
            .enumerate()
            .any(|(position, (index, _))| *index != position)
        {
            return Err(term::error(format!(
                "non-contiguous or duplicate {label} rcIndex"
            )));
        }
        Ok(indexed.into_iter().map(|(_, atom)| atom).collect())
    }

    fn ordered(
        &self,
        rule: TermId,
        field: &'static str,
        label: &str,
    ) -> gmeow_errors::Result<Vec<RcAtom>> {
        self.ordered_nodes(rule, field, label)?
            .into_iter()
            .map(|node| self.atom(node, &class_atom()))
            .collect()
    }

    fn numeric(&self, node: TermId) -> gmeow_errors::Result<RcNumeric> {
        self.class(node, &iri("RelationalCoreNumeric"))?;
        let operator = self.iri(self.one(node, "rcNumericOperator")?)?;
        let operator = NumericOperator::from_iri(operator)
            .ok_or_else(|| term::error("unknown numeric operator"))?;
        let call = RcNumeric {
            operator,
            left: term::decode(self.dataset, self.one(node, "rcNumericLeft")?)?,
            right: term::decode(self.dataset, self.one(node, "rcNumericRight")?)?,
            result: self
                .optional(node, "rcNumericResult")?
                .map(|value| term::decode(self.dataset, value))
                .transpose()?,
        };
        call.validate().map_err(term::error)?;
        Ok(call)
    }

    fn rule(&self, node: TermId) -> gmeow_errors::Result<RcRule> {
        self.class(node, &class_rule())?;
        let head = self.atom(self.one(node, "rcHead")?, &class_atom())?;
        let head_conjuncts = self.ordered(node, "rcHeadConjunct", "head-conjunct")?;
        let body = self.ordered(node, "rcBody", "body")?;
        let numeric = self
            .ordered_nodes(node, "rcNumeric", "numeric")?
            .into_iter()
            .map(|node| self.numeric(node))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let scheduled =
            numeric::schedule(numeric.clone(), body_bound_vars(&body)).map_err(term::error)?;
        if scheduled != numeric {
            return Err(term::error("noncanonical numeric binding schedule"));
        }
        let mut distinct_pairs = self
            .values(node, "rcDistinct")
            .map(|record| {
                self.iri(record)?;
                let left =
                    term::text(self.dataset, self.one(record, "rcDistinctLeft")?)?.to_owned();
                let right =
                    term::text(self.dataset, self.one(record, "rcDistinctRight")?)?.to_owned();
                Ok((left, right))
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        distinct_pairs.sort();
        Ok(RcRule {
            numeric,
            head,
            head_conjuncts,
            body,
            distinct_pairs,
        })
    }
}
