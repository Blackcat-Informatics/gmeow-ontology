// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded reuse of committed judgments inside the existing physical evaluator.
//! A journal append changes temporal suffixes. Non-temporal subprograms can
//! retain their exact proof DAG when the admitted context evidence is unchanged.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AdmissionError, AssessmentAnchor, Evaluator, Evidence, Frame, Inference, Instruction, NodeId,
    Program,
};

#[derive(Debug, Default)]
pub(in crate::modal) struct JudgmentCache {
    origin: Option<(String, String)>,
    pub(super) memo: BTreeMap<(NodeId, String), Evidence>,
    pub(super) inferences: BTreeMap<String, Inference>,
    pub(super) anchors: BTreeMap<String, AssessmentAnchor>,
    pub(super) context_anchors: BTreeMap<String, String>,
}

impl JudgmentCache {
    pub(super) fn validate_program(&self, program: &Program) -> Result<(), AdmissionError> {
        if let Some((formula, context)) = &self.origin
            && (formula != &program.formula_key.to_string() || context != &program.selected_context)
        {
            return Err(AdmissionError::Malformed(
                "monitor cache belongs to a different compiled formula or selected context".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn retain(
        program: &Program,
        evaluator: &Evaluator<'_, impl Frame>,
        capacity: usize,
    ) -> Self {
        if capacity == 0 {
            return Self::default();
        }
        let mut stable = Vec::with_capacity(program.instructions.len());
        for instruction in &program.instructions {
            let reusable = match instruction {
                Instruction::Atom { .. } => true,
                Instruction::AtContext { body, .. } | Instruction::Not(body) => stable[body.0],
                Instruction::And(children) | Instruction::Or(children) => {
                    children.iter().all(|child| stable[child.0])
                }
                Instruction::Implies(left, right) | Instruction::Iff(left, right) => {
                    stable[left.0] && stable[right.0]
                }
                Instruction::Modal { axis, body, .. } => {
                    axis.iri() != crate::modal::TYPED_ACCESSIBILITY[3] && stable[body.0]
                }
                Instruction::Temporal { .. } | Instruction::Until { .. } => false,
            };
            stable.push(reusable);
        }
        let memo: BTreeMap<_, _> = evaluator
            .memo
            .iter()
            .filter(|((node, _), _)| stable[node.0])
            .take(capacity)
            .map(|(key, evidence)| (key.clone(), evidence.clone()))
            .collect();
        // A retained judgment carries every reachable proof and anchor. Merely
        // retaining the truth coordinates would erase the evidence on a cache hit.
        let mut pending = memo
            .values()
            .flat_map(|evidence| evidence.support.iter().chain(&evidence.opposition))
            .cloned()
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        let mut inferences = BTreeMap::new();
        let mut anchors = BTreeMap::new();
        let mut contexts = memo
            .keys()
            .map(|(_, context)| context.clone())
            .collect::<BTreeSet<_>>();
        while let Some(identity) = pending.pop() {
            if !seen.insert(identity.clone()) {
                continue;
            }
            if let Some(inference) = evaluator.inferences.get(&identity) {
                pending.extend(inference.antecedents.iter().cloned());
                inferences.insert(identity.clone(), inference.clone());
            }
            if let Some(anchor) = evaluator.anchors.get(&identity) {
                contexts.insert(anchor.context.clone());
                anchors.insert(identity, anchor.clone());
            }
        }
        Self {
            origin: Some((
                program.formula_key.to_string(),
                program.selected_context.clone(),
            )),
            memo,
            inferences,
            anchors,
            context_anchors: contexts
                .into_iter()
                .map(|context| {
                    let digest = evaluator.context_anchors[&context].clone();
                    (context, digest)
                })
                .collect(),
        }
    }
}
