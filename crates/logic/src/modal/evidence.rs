// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Evidence for evaluation over an admitted, completed finite predecessor.
//! This records that selected evaluation; it is not a certificate of unrestricted
//! source completeness, context merging, or an optimization rewrite.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    ATOM_OBJECT, ATOM_PREDICATE, ATOM_SUBJECT, DEONTICALLY_IDEAL, MODAL_COUNTEREXAMPLE_WORLD,
    MODAL_EVAL_WORLD, MODAL_NECESSITY_FAILS, MODAL_NECESSITY_HOLDS, MODAL_NECESSITY_UNDETERMINED,
    MODAL_POSSIBILITY_FAILS, MODAL_POSSIBILITY_HOLDS, MODAL_RULE_IRI, ModalFrame, ModalOp,
    NECESSARILY, OVER_ACCESSIBILITY, POSSIBLY, TYPED_ACCESSIBILITY, iri_binding, modal_err, n3,
};

/// An exact positive fact occurrence. Context is part of its identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModalPremise {
    pub context: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

impl ModalPremise {
    /// Context-qualified occurrence identity, distinct from an RDF triple reifier.
    #[must_use]
    pub fn occurrence_id(&self) -> String {
        occurrence_id(&self.context, &self.subject, &self.predicate, &self.object)
    }

    pub(crate) fn triple_id(&self) -> String {
        crate::provenance::reifier_from_strings(&self.subject, &self.predicate, &self.object)
    }
}

pub(crate) fn occurrence_id(context: &str, subject: &str, predicate: &str, object: &str) -> String {
    digest("modal-occurrence", &[context, subject, predicate, object])
}

fn digest(domain: &str, fields: &[&str]) -> String {
    let mut hash = Sha256::new();
    for field in std::iter::once(domain).chain(fields.iter().copied()) {
        hash.update((field.len() as u64).to_be_bytes());
        hash.update(field.as_bytes());
    }
    format!(
        "https://blackcatinformatics.ca/logic/evidence/{:x}",
        hash.finalize()
    )
}

/// Presence or absence of the selected atom at an explicitly reached world.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModalWorldEvidence {
    pub world: String,
    pub atom_present: bool,
}

/// The bounded input contract admitted by the caller before evaluating absence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ModalFrontier {
    /// The native predecessor completed; `worlds` exhausts the selected `(C,w0,R)`
    /// adjacency, with the atom lookup for every endpoint. No claim is made about
    /// an unselected program, a recursive modal stratum, or an external corpus.
    CompletedFinitePredecessor { worlds: Vec<ModalWorldEvidence> },
}

/// Context-preserving evidence for one modal conclusion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModalEvaluation {
    pub context: String,
    pub formula: String,
    pub operator: ModalOp,
    pub body: String,
    pub evaluation_world: String,
    pub accessibility_relation: String,
    pub atom_subject: String,
    pub atom_predicate: String,
    pub atom_object: String,
    pub frontier: ModalFrontier,
    pub conclusion_predicate: String,
    pub conclusion_object: String,
}

impl ModalEvaluation {
    pub(crate) fn from_frame(
        frame: &ModalFrame,
        worlds: Vec<ModalWorldEvidence>,
        predicate: &str,
        object: String,
    ) -> Self {
        Self {
            context: frame.context.clone(),
            formula: frame.formula.clone(),
            operator: frame.op,
            body: frame.body.clone(),
            evaluation_world: frame.w0.clone(),
            accessibility_relation: frame.relation.clone(),
            atom_subject: frame.atom_s.clone(),
            atom_predicate: frame.atom_p.clone(),
            atom_object: frame.atom_o.clone(),
            frontier: ModalFrontier::CompletedFinitePredecessor { worlds },
            conclusion_predicate: predicate.to_owned(),
            conclusion_object: object,
        }
    }

    /// Every positive premise, in deterministic frame/edge/body order. An absent
    /// atom appears only in the frontier, never in this list.
    #[must_use]
    pub fn positive_premises(&self) -> Vec<ModalPremise> {
        let mut out = Vec::new();
        let mut push = |context: &str, s: &str, p: &str, o: &str| {
            out.push(ModalPremise {
                context: context.to_owned(),
                subject: s.to_owned(),
                predicate: p.to_owned(),
                object: n3(o),
            })
        };
        let op = match self.operator {
            ModalOp::Box => NECESSARILY,
            ModalOp::Diamond => POSSIBLY,
        };
        push(&self.context, &self.formula, op, &self.body);
        push(
            &self.context,
            &self.formula,
            OVER_ACCESSIBILITY,
            &self.accessibility_relation,
        );
        push(
            &self.context,
            &self.formula,
            MODAL_EVAL_WORLD,
            &self.evaluation_world,
        );
        push(&self.context, &self.body, ATOM_SUBJECT, &self.atom_subject);
        push(
            &self.context,
            &self.body,
            ATOM_PREDICATE,
            &self.atom_predicate,
        );
        push(&self.context, &self.body, ATOM_OBJECT, &self.atom_object);
        let ModalFrontier::CompletedFinitePredecessor { worlds } = &self.frontier;
        for world in worlds {
            push(
                &self.context,
                &self.evaluation_world,
                &self.accessibility_relation,
                &world.world,
            );
            if world.atom_present {
                push(
                    &world.world,
                    &self.atom_subject,
                    &self.atom_predicate,
                    &self.atom_object,
                );
            }
        }
        out
    }

    /// Identity includes the assertion context, conclusion, selected frontier and
    /// presence/absence observations. Equal triples in different contexts differ.
    #[must_use]
    pub fn derivation_id(&self) -> String {
        let ModalFrontier::CompletedFinitePredecessor { worlds } = &self.frontier;
        let mut fields = vec![
            self.context.as_str(),
            &self.formula,
            match self.operator {
                ModalOp::Box => "box",
                ModalOp::Diamond => "diamond",
            },
            &self.body,
            &self.evaluation_world,
            &self.accessibility_relation,
            &self.atom_subject,
            &self.atom_predicate,
            &self.atom_object,
            &self.conclusion_predicate,
            &self.conclusion_object,
            "completed-finite-predecessor",
        ];
        for world in worlds {
            fields.push(&world.world);
            fields.push(if world.atom_present {
                "present"
            } else {
                "absent"
            });
        }
        digest("modal-evaluation-v1", &fields)
    }

    /// Check the intrinsic evaluation contract, without pretending a transported
    /// receipt independently proves that its original predecessor was complete.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        if !self.context.is_empty() && self.context != "default" && !self.context.starts_with("_:")
        {
            iri_binding(&self.context, "modal asserting context")?;
        }
        for value in [
            &self.formula,
            &self.body,
            &self.evaluation_world,
            &self.accessibility_relation,
            &self.atom_subject,
            &self.atom_predicate,
            &self.atom_object,
            &self.conclusion_predicate,
            &self.conclusion_object,
        ] {
            iri_binding(value, "modal evaluation evidence")?;
        }
        if !TYPED_ACCESSIBILITY.contains(&self.accessibility_relation.as_str()) {
            return Err(modal_err(
                "modal evidence names an unadmitted accessibility relation".to_owned(),
            ));
        }
        let ModalFrontier::CompletedFinitePredecessor { worlds } = &self.frontier;
        for world in worlds {
            iri_binding(&world.world, "modal frontier world")?;
        }
        if worlds.windows(2).any(|pair| pair[0].world >= pair[1].world) {
            return Err(modal_err(
                "modal frontier worlds must be sorted and unique".to_owned(),
            ));
        }
        let missing = worlds.iter().find(|world| !world.atom_present);
        let predicate = match self.operator {
            ModalOp::Box
                if worlds.is_empty() && self.accessibility_relation == DEONTICALLY_IDEAL =>
            {
                MODAL_NECESSITY_UNDETERMINED
            }
            ModalOp::Box if missing.is_some() => MODAL_NECESSITY_FAILS,
            ModalOp::Box => MODAL_NECESSITY_HOLDS,
            ModalOp::Diamond if worlds.iter().any(|world| world.atom_present) => {
                MODAL_POSSIBILITY_HOLDS
            }
            ModalOp::Diamond => MODAL_POSSIBILITY_FAILS,
        };
        let valid = if self.conclusion_predicate == MODAL_COUNTEREXAMPLE_WORLD {
            self.operator == ModalOp::Box
                && missing.is_some_and(|world| world.world == self.conclusion_object)
        } else {
            self.conclusion_predicate == predicate && self.conclusion_object == self.body
        };
        if !valid {
            return Err(modal_err(
                "modal conclusion does not follow from its contextual frontier".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_axiom(
        &self,
        axiom: &crate::reason::InferredAxiom,
    ) -> gmeow_errors::Result<()> {
        self.validate()?;
        let premises = self
            .positive_premises()
            .into_iter()
            .map(|p| (p.subject, p.predicate, p.object))
            .collect::<Vec<_>>();
        if axiom.is_edb
            || axiom.rule_name.as_deref() != Some(MODAL_RULE_IRI)
            || axiom.world != self.context
            || axiom.subject != self.formula
            || axiom.predicate != self.conclusion_predicate
            || axiom.object.as_iri() != Some(self.conclusion_object.as_str())
            || axiom.premises != premises
        {
            return Err(modal_err(
                "modal axiom does not match its contextual evaluation evidence".to_owned(),
            ));
        }
        Ok(())
    }

    /// Explicit RDF transport literal. Native execution never decodes this form;
    /// only the authenticated RDF import boundary uses it.
    pub(crate) fn to_wire(&self) -> String {
        format!(
            "gmeow-modal-evaluation-v1\0{}",
            serde_json::to_string(self)
                .expect("modal evidence contains only serializable native fields")
        )
    }

    pub(crate) fn from_wire(wire: &str) -> Option<gmeow_errors::Result<Self>> {
        wire.strip_prefix("gmeow-modal-evaluation-v1\0")
            .map(|json| {
                let evidence: Self = serde_json::from_str(json)
                    .map_err(|error| modal_err(format!("invalid modal evidence: {error}")))?;
                evidence.validate()?;
                Ok(evidence)
            })
    }
}
