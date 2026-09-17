// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Finite recurrences in the shared evaluator, using the same evidence algebra,
//! memo table, proof minter, cancellation signal and judgment budget.

use super::{
    AdmissionError, EvaluationError, Evaluator, Evidence, Fold, Frame, NodeId, TemporalOperator,
    TemporalSelection, TemporalTrace,
};

impl<F: Frame> Evaluator<'_, F> {
    fn trace(
        &mut self,
        node: NodeId,
        context: &str,
        selection: TemporalSelection,
    ) -> Result<TemporalTrace, EvaluationError> {
        let mut trace = self.frame.temporal(context)?;
        if trace.points.first().map(|point| point.context.as_str()) != Some(context) {
            return Err(AdmissionError::Malformed(
                "temporal trace does not start at the selected context".into(),
            )
            .into());
        }
        self.temporal_prefixes
            .insert(trace.prefix.identity().to_owned(), trace.prefix.clone());
        if matches!(selection, TemporalSelection::Suffix) {
            // Walk only the unevaluated prefix of this suffix. Asking the RDF
            // adapter for the whole suffix at every nested node would still do
            // quadratic copying even if the judgment memo hits every time.
            while trace.points.len() >= 2 {
                self.cancellation_checkpoint()?;
                let last = &trace.points.last().expect("nonempty trace").context;
                if self.memo.contains_key(&(node, last.clone())) {
                    break;
                }
                let continuation = self.frame.temporal(last)?;
                if continuation.prefix != trace.prefix
                    || continuation.points.first().map(|point| &point.context) != Some(last)
                {
                    return Err(AdmissionError::Malformed(
                        "temporal continuation changes its admitted prefix".into(),
                    )
                    .into());
                }
                match continuation.points.as_slice() {
                    [_] => break,
                    [_, next] => trace.points.push(next.clone()),
                    _ => {
                        return Err(AdmissionError::Malformed(
                            "temporal continuation is not an immediate successor".into(),
                        )
                        .into());
                    }
                }
            }
        }
        Ok(trace)
    }

    /// Strong next at a finalized boundary is opposed. An open frontier has
    /// neither witness; absence of a committed successor does not finalize it.
    fn frontier(
        &mut self,
        node: NodeId,
        context: &str,
        trace: &TemporalTrace,
        weak: bool,
    ) -> Evidence {
        if !trace.prefix.finalized() {
            return Evidence::default();
        }
        let witness = self.infer(
            node,
            context,
            if weak {
                "finite-weak-next-boundary"
            } else {
                "finite-next-boundary"
            },
            vec![trace.prefix.identity().to_owned()],
        );
        Evidence {
            support: weak.then(|| witness.clone()),
            opposition: (!weak).then_some(witness),
            complete: true,
        }
    }

    pub(super) fn temporal(
        &mut self,
        node: NodeId,
        context: &str,
        operator: TemporalOperator,
        body: NodeId,
    ) -> Result<Evidence, EvaluationError> {
        let selection = if operator == TemporalOperator::Next {
            TemporalSelection::Next
        } else {
            TemporalSelection::Suffix
        };
        let trace = self.trace(node, context, selection)?;
        if operator == TemporalOperator::Next {
            return match trace.points.as_slice() {
                [_, next] => {
                    let child = self.evaluate(body, &next.context)?;
                    let mut witnesses = next.witnesses.clone();
                    witnesses.push(trace.prefix.identity().to_owned());
                    Ok(self.combine(node, context, Fold::And, vec![child], &witnesses))
                }
                [_] => Ok(self.frontier(node, context, &trace, false)),
                _ => Err(AdmissionError::Malformed(
                    "strong next requires exactly one current point and at most one successor"
                        .into(),
                )
                .into()),
            };
        }
        self.fold_suffix(
            node,
            context,
            &trace,
            |evaluator, selected, next, witnesses| {
                let child = evaluator.evaluate(body, selected)?;
                let fold = if operator == TemporalOperator::Globally {
                    Fold::And
                } else {
                    Fold::Or
                };
                Ok(evaluator.combine(node, selected, fold, vec![child, next], witnesses))
            },
            operator == TemporalOperator::Globally,
        )
    }

    pub(super) fn until(
        &mut self,
        node: NodeId,
        context: &str,
        left: NodeId,
        right: NodeId,
    ) -> Result<Evidence, EvaluationError> {
        let trace = self.trace(node, context, TemporalSelection::Suffix)?;
        self.fold_suffix(
            node,
            context,
            &trace,
            |evaluator, selected, next, witnesses| {
                let right = evaluator.evaluate(right, selected)?;
                let left = evaluator.evaluate(left, selected)?;
                let maintained =
                    evaluator.combine(node, selected, Fold::And, vec![left, next], witnesses);
                Ok(evaluator.combine(node, selected, Fold::Or, vec![right, maintained], witnesses))
            },
            false,
        )
    }

    /// Evaluate backwards without recursion proportional to journal length.
    /// Future judgments are committed once and reused by nested temporal nodes.
    /// The outer evaluator commits the selected root, so it is never double charged.
    fn fold_suffix(
        &mut self,
        node: NodeId,
        context: &str,
        trace: &TemporalTrace,
        mut fold: impl FnMut(&mut Self, &str, Evidence, &[String]) -> Result<Evidence, EvaluationError>,
        weak: bool,
    ) -> Result<Evidence, EvaluationError> {
        let mut next = None;
        for point in trace.points.iter().rev() {
            self.cancellation_checkpoint()?;
            let key = (node, point.context.clone());
            if let Some(evidence) = self.memo.get(&key) {
                next = Some(evidence.clone());
                continue;
            }
            self.checkpoint()?;
            self.admit_context(&point.context)?;
            let successor = next
                .take()
                .unwrap_or_else(|| self.frontier(node, &point.context, trace, weak));
            let mut witnesses = point.witnesses.clone();
            witnesses.push(trace.prefix.identity().to_owned());
            let evidence = fold(self, &point.context, successor, &witnesses)?;
            if point.context != context {
                self.checkpoint()?;
                self.governor.charge();
                self.memo.insert(key, evidence.clone());
            }
            next = Some(evidence);
        }
        next.ok_or_else(|| AdmissionError::Malformed("empty temporal suffix".into()).into())
    }
}
