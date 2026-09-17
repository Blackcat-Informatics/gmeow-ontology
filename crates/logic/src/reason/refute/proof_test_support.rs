// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated synthetic-family helpers; production uses the native joint engine.
use super::*;

impl ContextualConflict {
    /// Project only local memberships justified by this native proof.
    pub(crate) fn local_clashes(
        &self,
        rule: &str,
    ) -> BTreeSet<crate::reason::refute::NothingClash> {
        let premises = self
            .proof
            .premises()
            .iter()
            .map(|premise| {
                let subject = crate::facts::skolemize(&premise.subject);
                let object = crate::facts::skolemize(&premise.object);
                (
                    subject
                        .as_iri()
                        .expect("source subjects are resources")
                        .to_owned(),
                    premise.predicate.clone(),
                    crate::provenance::term_display(object.as_ref()),
                )
            })
            .collect::<Vec<_>>();
        self.proof
            .local_subjects()
            .into_iter()
            .map(|individual| crate::reason::refute::NothingClash {
                individual,
                world: self.world.clone(),
                rule_name: rule.to_owned(),
                premises: premises.clone(),
            })
            .collect()
    }
}
