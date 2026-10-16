// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Syntactic head possibilities over the already prepared native theory.
//!
//! Runtime pruning and syntactic discharge have different premises. Authored heads
//! remain possible here even when the current input cannot satisfy their bodies.
//! The expanded metadata is run-local; no second executable or source compiler runs.

use std::sync::Arc;
use std::time::Instant;

use super::{
    JointInput, JointTemplate, ProducerEffect, StatementPattern, ValueFlow, producer_effects,
    property_flow, seminaive_err,
};
use crate::physical::WriterInventory;

pub(super) struct SyntacticPlan {
    identity: [u8; 32],
    flow: ValueFlow,
    effects: Vec<ProducerEffect>,
}

impl SyntacticPlan {
    fn new(template: &JointTemplate) -> gmeow_errors::Result<Self> {
        // Keep the original compiled ordinary/chase layouts and every native
        // schema law. Runtime applicability can omit a law whose selector is
        // created only by a currently nonfiring authored metadata head.
        let mut flows = template.core_flows.to_vec();
        let mut effects = template.effects[..flows.len()].to_vec();
        let schemas = crate::reason::schema_laws();
        flows.extend(schemas.iter().map(property_flow));
        effects.extend(producer_effects(&[], &[], schemas)?);
        flows.extend(template.families.iter().map(|arm| arm.flow.clone()));
        effects.extend(template.families.iter().map(|arm| arm.effect.clone()));
        let protected = template
            .definitions
            .as_ref()
            .map_or_else(Vec::new, |contract| contract.patterns.clone());
        let mut observed = protected.clone();
        observed.extend(crate::modal::native::definition_vocabulary());
        observed.extend(crate::contextual::native::definition_vocabulary());
        Ok(Self {
            identity: crate::physical::metadata_identity(
                "gmeow-native-syntactic-plan-v1",
                &(&flows, &effects, template.semantics, &observed, &protected),
            ),
            flow: ValueFlow::with_conditioned_observations(
                &flows,
                &effects,
                template.semantics,
                &observed,
                &protected,
            ),
            effects,
        })
    }
}

impl JointInput<'_> {
    fn syntactic_plan(&self) -> gmeow_errors::Result<Arc<SyntacticPlan>> {
        let mut retained = self
            .syntactic
            .lock()
            .map_err(|_| seminaive_err("syntactic writer-analysis lock poisoned"))?;
        if let Some(plan) = retained.as_ref() {
            return Ok(Arc::clone(plan));
        }
        let plan = Arc::new(SyntacticPlan::new(&self.template)?);
        *retained = Some(Arc::clone(&plan));
        Ok(plan)
    }

    /// Analyze the complete selected world's possible writes once, without
    /// executing a rule, changing a source fact or rebuilding a source program.
    /// Unknown predicate cells remain explicit in the resulting inventory.
    pub(crate) fn writer_inventory(&self, world: &str) -> gmeow_errors::Result<WriterInventory> {
        let started = Instant::now();
        let facts = self.facts.get(world).ok_or_else(|| {
            seminaive_err("syntactic writer analysis has no admitted source world")
        })?;
        let plan = self.syntactic_plan()?;
        let mut declared = Vec::new();
        for index in self.template.authored_ordinary.clone() {
            declared.extend(
                self.template.core_flows[index]
                    .heads
                    .iter()
                    .map(StatementPattern::statement),
            );
        }
        let mut enabled = vec![true; plan.effects.len()];
        let producer_start = self.template.rules.keys().len();
        for (index, producer) in self.template.producers.iter().enumerate() {
            let owned = producer.owns_world(world)?;
            enabled[producer_start + index] = owned;
            if owned {
                declared.extend(producer.rule.head.iter().map(StatementPattern::atom));
            }
        }
        let external: Vec<_> = self
            .contextual_effects
            .iter()
            .filter(|effect| effect.owner == world)
            .collect();
        declared.extend(
            external
                .iter()
                .flat_map(|effect| effect.effect.writes.iter().cloned()),
        );
        let rows = || {
            facts.iter().chain(
                self.possible
                    .iter()
                    .filter(|(owner, _)| owner == world)
                    .map(|(_, fact)| fact),
            )
        };
        // Source selections and immutable-list reasoning see this world's own
        // possible metadata writers, including heads whose bodies never fire.
        let flow = plan
            .flow
            .with_additional_observations(declared.iter())
            .with_source_operators(rows());
        let mut summary = flow.summarize(rows());
        flow.seed_patterns(&mut summary, declared.iter());
        let effects = flow.refine_enabled(&plan.effects, &summary, &enabled);
        let mut inventory = WriterInventory::new(plan.identity);
        inventory.requires_source_admission = facts.iter().any(|fact| {
            !crate::reason::dl::construct_families(&fact.predicate, &fact.subject, &fact.object)
                .is_empty()
        });
        for (index, effect) in effects.iter().enumerate() {
            if !enabled[index] {
                continue;
            }
            inventory
                .producers
                .push(plan.effects[index].name().to_owned());
            for write in &effect.writes {
                inventory.include(&flow, write);
            }
        }
        // A declared head is syntactically possible irrespective of current
        // body satisfaction; the refined effects above cover its consequences.
        for head in &declared {
            inventory.include(&flow, head);
        }
        inventory.producers.extend(
            external
                .iter()
                .map(|effect| effect.effect.name().to_owned()),
        );
        tracing::info!(
            target: "pipeline_phase",
            stage = "stage-reason",
            phase = "syntactic-writer-analysis",
            event = "end",
            world,
            input_rows = facts.len(),
            producers = inventory.producers.len(),
            possible_predicates = inventory.predicates.len(),
            unbounded = inventory.unbounded,
            elapsed_ms = started.elapsed().as_millis(),
            "source-bound syntactic writer inventory",
        );
        Ok(inventory)
    }
}
