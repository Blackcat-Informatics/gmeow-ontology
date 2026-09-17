// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed source metadata retained independently of any RDF carrier. Dynamic
//! attribution reads borrow the completed native stores and their exact receipts.

use super::*;
use crate::modal::composite::{
    Accessibility, AdmissionError, Context, Evidence, Frame, Successors, TemporalTrace,
};
use crate::modal::contextual::{
    AssessmentEvidence, ContextualInference, NativeEvidence, RdfFrame, atom_term, malformed,
};
use purrdf::{DatasetView, GraphMatch, TermValue};

#[derive(Clone)]
struct Claim {
    world: String,
    reifier: String,
    statement: (TermValue, String, TermValue),
    owners: BTreeSet<String>,
    statuses: BTreeSet<String>,
}

pub(super) struct PreparedFrame {
    pub(super) source_world: String,
    pub(super) basis_subjects: BTreeSet<TermValue>,
    pub(super) contexts: BTreeMap<String, Context>,
    closed: BTreeMap<String, bool>,
    edges: BTreeMap<(String, Accessibility), Successors>,
    traces: BTreeMap<String, Result<TemporalTrace, AdmissionError>>,
    claims: Vec<Claim>,
    base_basis: String,
}

pub(super) struct NativeFrame<'a> {
    prepared: &'a PreparedFrame,
    claims: BTreeMap<(String, TermValue, String, TermValue), Evidence>,
    inferences: BTreeMap<String, ContextualInference>,
    receipts: BTreeMap<String, NativeEvidence>,
    pub(super) support: BTreeSet<(String, crate::physical::WitnessStatement)>,
}

impl PreparedFrame {
    pub(super) fn new<D: DatasetView + ?Sized>(
        source: &RdfFrame<'_, D>,
    ) -> gmeow_errors::Result<Self> {
        let dataset = source.dataset;
        let source_graph = source
            .source_graph
            .map(|graph| crate::reason::dataset::native(dataset, graph));
        let selected_worlds = source
            .contexts
            .values()
            .map(|context| context.world.as_str())
            .collect::<BTreeSet<_>>();
        let mut claims = Vec::new();
        for row in dataset.reifier_quads() {
            let Some(graph) = row.g else {
                continue;
            };
            let world = crate::modal::contextual::iri(dataset, graph).map_err(diagnostic)?;
            if !selected_worlds.contains(world.as_str()) {
                continue;
            }
            let reifier = crate::modal::contextual::iri(dataset, row.s).map_err(diagnostic)?;
            let TermValue::Triple { s, p, o } = crate::reason::dataset::native(dataset, row.o)
            else {
                return Err(diagnostic(malformed(
                    "rdf:reifies must bind an RDF 1.2 triple term",
                )));
            };
            let predicate = p
                .as_iri()
                .ok_or_else(|| diagnostic(malformed("a claim predicate must be an IRI")))?
                .to_owned();
            let values = |property: &str| -> gmeow_errors::Result<BTreeSet<String>> {
                let Some(predicate) = dataset.term_id_by_value(&TermValue::iri(property)) else {
                    return Ok(BTreeSet::new());
                };
                gmeow_logic_compile::frontend::selected_source_statements(
                    dataset,
                    Some(row.s),
                    Some(predicate),
                    None,
                    GraphMatch::Named(graph),
                )
                .map(|row| crate::modal::contextual::iri(dataset, row.o).map_err(diagnostic))
                .collect()
            };
            claims.push(Claim {
                world,
                reifier,
                statement: (*s, predicate, *o),
                owners: values(OWNER)?,
                statuses: values(STATUS)?,
            });
        }
        Ok(Self {
            source_world: LogicalGraph::from_graph(source_graph).world()?,
            basis_subjects: source
                .journal_basis_subjects()
                .into_iter()
                .map(|subject| {
                    crate::facts::skolemize(&crate::reason::dataset::native(dataset, subject))
                        .into_owned()
                })
                .collect(),
            contexts: source.contexts.clone(),
            closed: source.evidence_closed.clone(),
            edges: source.edges.clone(),
            traces: source
                .contexts
                .keys()
                .map(|identity| (identity.clone(), source.temporal_trace(identity)))
                .collect(),
            claims,
            base_basis: source.basis_digest(),
        })
    }

    pub(super) fn read_worlds(&self) -> BTreeSet<String> {
        self.contexts
            .values()
            .map(|context| context.world.clone())
            .collect()
    }

    pub(super) fn bind<'a>(
        &'a self,
        worlds: &BTreeMap<&str, NativeWorldSnapshot<'_>>,
    ) -> gmeow_errors::Result<NativeFrame<'a>> {
        let mut frame = self.unvisited();
        for claim in &self.claims {
            let snapshot = worlds.get(claim.world.as_str()).ok_or_else(|| {
                diagnostic(malformed(
                    "selected contextual evidence world has no native store",
                ))
            })?;
            let input = snapshot.input();
            if input.world() != claim.world {
                return Err(diagnostic(malformed(
                    "contextual evidence snapshot belongs to a different world",
                )));
            }
            let mut values =
                |property: &str,
                 asserted: &BTreeSet<String>|
                 -> gmeow_errors::Result<BTreeMap<String, Option<NativeEvidence>>> {
                    let mut values: BTreeMap<_, _> = asserted
                        .iter()
                        .cloned()
                        .map(|value| (value, None))
                        .collect();
                    for index in input.store.facts_for_predicate(property) {
                        let fact = &input.store.facts()[*index];
                        if fact.subject.as_iri() != Some(claim.reifier.as_str()) {
                            continue;
                        }
                        let value = fact.object.as_iri().ok_or_else(|| {
                            diagnostic(malformed("derived attribution metadata must be IRI-valued"))
                        })?;
                        frame
                            .support
                            .insert((claim.world.clone(), WitnessStatement::from(fact)));
                        if values.contains_key(value) {
                            continue;
                        }
                        let row = input
                            .rows
                            .iter()
                            .filter(|row| {
                                row.subject == fact.subject
                                    && row.predicate == fact.predicate
                                    && row.object == fact.object
                            })
                            .min_by(|left, right| left.derivation_id.cmp(&right.derivation_id))
                            .ok_or_else(|| {
                                diagnostic(malformed(
                                    "attribution fact has no native source or committed derivation",
                                ))
                            })?;
                        if row.rule_iri == crate::provenance::ASSERT_RULE_IRI
                            || row.source_quad_ids.is_empty()
                        {
                            return Err(diagnostic(malformed(
                                "derived attribution metadata requires its native firing rule and premises",
                            )));
                        }
                        values.insert(
                            value.to_owned(),
                            Some(native_receipt(input.world(), row, value)?),
                        );
                    }
                    Ok(values)
                };
            let owners = values(OWNER, &claim.owners)?;
            let contexts = self
                .contexts
                .iter()
                .filter(|(_, context)| {
                    context.world == claim.world && owners.contains_key(&context.standpoint)
                })
                .collect::<Vec<_>>();
            if contexts.is_empty() {
                continue;
            }
            let statuses = values(STATUS, &claim.statuses)?;
            if statuses.len() != 1 {
                return Err(diagnostic(malformed(
                    "an attributed claim requires exactly one support status",
                )));
            }
            let (polarity, status_receipt) =
                statuses.first_key_value().expect("one admitted status");
            let (support, opposition) = match polarity.strip_prefix(crate::modal::GMEOW_NS) {
                Some("supportSupported") => (true, false),
                Some("supportOpposed") => (false, true),
                Some("supportBoth") => (true, true),
                Some("supportNeither") => (false, false),
                _ => {
                    return Err(diagnostic(malformed(
                        "unrecognized attributed support status",
                    )));
                }
            };
            frame.support.insert((
                claim.world.clone(),
                WitnessStatement {
                    subject: TermValue::iri(&claim.reifier),
                    predicate: REIFIES.into(),
                    object: TermValue::Triple {
                        s: Box::new(claim.statement.0.clone()),
                        p: Box::new(TermValue::iri(&claim.statement.1)),
                        o: Box::new(claim.statement.2.clone()),
                    },
                },
            ));
            for (identity, context) in contexts {
                let mut antecedents = vec![claim.reifier.clone()];
                for receipt in [
                    owners[&context.standpoint].as_ref(),
                    status_receipt.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    antecedents.push(receipt.identity().to_owned());
                    frame
                        .receipts
                        .insert(receipt.identity().to_owned(), receipt.clone());
                }
                let witness = if antecedents.len() == 1 {
                    claim.reifier.clone()
                } else {
                    antecedents.push(identity.clone());
                    antecedents.sort();
                    antecedents.dedup();
                    let rule = "https://blackcatinformatics.ca/logic/rule/contextual-attribution";
                    let address = crate::provenance::mint_derivation_id(
                        rule,
                        &antecedents.iter().map(String::as_str).collect::<Vec<_>>(),
                    );
                    frame.inferences.insert(
                        address.clone(),
                        ContextualInference {
                            identity: address.clone(),
                            rule: rule.into(),
                            context: identity.clone(),
                            antecedents,
                        },
                    );
                    address
                };
                let evidence = frame
                    .claims
                    .entry((
                        identity.clone(),
                        claim.statement.0.clone(),
                        claim.statement.1.clone(),
                        claim.statement.2.clone(),
                    ))
                    .or_default();
                for (present, slot) in [
                    (support, &mut evidence.support),
                    (opposition, &mut evidence.opposition),
                ] {
                    if present && slot.as_ref().is_none_or(|prior| witness < *prior) {
                        *slot = Some(witness.clone());
                    }
                }
            }
        }
        Ok(frame)
    }

    /// A zero-allowance assessment visits no evidence or temporal position. Its
    /// stop record binds the admitted source without reading an unfinished writer.
    pub(super) fn unvisited(&self) -> NativeFrame<'_> {
        NativeFrame {
            prepared: self,
            claims: BTreeMap::new(),
            inferences: BTreeMap::new(),
            receipts: BTreeMap::new(),
            support: BTreeSet::new(),
        }
    }
}

fn native_receipt(
    world: &str,
    row: &crate::rule_ir::DerivedRow,
    object: &str,
) -> gmeow_errors::Result<NativeEvidence> {
    let subject = row
        .subject
        .as_iri()
        .ok_or_else(|| diagnostic(malformed("derived attribution subject must be an IRI")))?;
    // Joint rows receive their presentation graph only when the completed world
    // is drained. Evidence created during execution belongs to the authenticated
    // snapshot world, including when that presentation field is still empty.
    if !row.graph.is_empty() && row.graph != world {
        return Err(diagnostic(malformed(
            "derived attribution row belongs to a different world",
        )));
    }
    let receipt = crate::explain::AxiomReceipt {
        row: crate::explain::Row {
            modal_evaluation: None,
            graph: world.to_owned(),
            subject: subject.into(),
            predicate: row.predicate.clone(),
            obj: crate::provenance::term_display(&row.object),
            derivation_id: row.derivation_id.clone(),
            rule_iri: crate::explain::canonical_rule_iri(&row.rule_iri),
            source_quad_ids: row.source_quad_ids.clone(),
        },
        raw_rule_identity: row.rule_iri.clone(),
    };
    let conclusion = crate::explain::reifier_from_row(&receipt.row);
    let identity = crate::provenance::mint_derivation_id(
        "https://blackcatinformatics.ca/logic/rule/contextual-native-evidence",
        &[world, &conclusion, &receipt.row.derivation_id],
    );
    Ok(NativeEvidence {
        identity,
        receipt,
        object: object.into(),
    })
}

impl NativeFrame<'_> {
    pub(super) fn basis_digest(&self) -> String {
        let records = self
            .claims
            .iter()
            .map(|((context, s, p, o), evidence)| {
                (
                    context,
                    s.to_canonical_bytes(),
                    p,
                    o.to_canonical_bytes(),
                    &evidence.support,
                    &evidence.opposition,
                )
            })
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&(&self.prepared.base_basis, records))
            .expect("native contextual basis serializes");
        blake3::hash(&bytes).to_hex().to_string()
    }
}
impl AssessmentEvidence for NativeFrame<'_> {
    fn attribution_inferences(&self) -> &BTreeMap<String, ContextualInference> {
        &self.inferences
    }
    fn native_evidence(&self) -> &BTreeMap<String, NativeEvidence> {
        &self.receipts
    }
}
impl Frame for NativeFrame<'_> {
    fn context(&self, identity: &str) -> Result<&Context, AdmissionError> {
        self.prepared
            .contexts
            .get(identity)
            .ok_or_else(|| malformed(format!("missing attributed context {identity}")))
    }
    fn atom(
        &self,
        context: &str,
        relation: &str,
        subject: &gmeow_logic_compile::ir::Term,
        object: &gmeow_logic_compile::ir::Term,
    ) -> Result<Evidence, AdmissionError> {
        self.context(context)?;
        let mut evidence = self
            .claims
            .get(&(
                context.into(),
                atom_term(subject)?,
                relation.into(),
                atom_term(object)?,
            ))
            .cloned()
            .unwrap_or_default();
        evidence.complete = self.prepared.closed[context];
        Ok(evidence)
    }
    fn successors(&self, context: &str, axis: Accessibility) -> Result<Successors, AdmissionError> {
        self.prepared
            .edges
            .get(&(context.into(), axis))
            .cloned()
            .ok_or_else(|| {
                malformed(format!(
                    "missing selected successor inventory for {context} over {}",
                    axis.iri()
                ))
            })
    }
    fn temporal(&self, context: &str) -> Result<TemporalTrace, AdmissionError> {
        self.prepared
            .traces
            .get(context)
            .cloned()
            .ok_or_else(|| malformed(format!("missing attributed context {context}")))?
    }
}
