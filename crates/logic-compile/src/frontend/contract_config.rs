// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The contract-configuration nodes of a source graph, split by role, so that contract
//! configuration is kept out of the domain axiom set without one node masking another.

use super::*;

/// The meta-configuration nodes whose facet-config triples must be kept out of the domain
/// axiom set, split by the role a node plays: a node that is configuration in one role is
/// not configuration where it appears in the other.
#[derive(Debug, Default)]
pub(super) struct ContractConfigSubjects {
    /// Subjects typed `logic:ReasoningContract` or `logic:ReasoningPreset`: the owners of the
    /// facet predicates, `logic:closureEntry` and `logic:complexityClass`.
    owners: BTreeSet<String>,
    /// Closure entries: subjects typed `logic:ClosureEntry`, and every object of
    /// `logic:closureEntry` on an owner whether or not it is typed. They
    /// own `logic:closureKey`, `logic:closureValue` and `logic:closureIndividual`.
    entries: BTreeSet<String>,
}

impl ContractConfigSubjects {
    /// Whether the row `subject <logic:p_local> _` is configuration of `subject`: the closure
    /// entry predicates belong to an entry, every other reserved predicate to an owner.
    pub(super) fn holds_predicate(&self, p_local: &str, subject: &str) -> bool {
        if matches!(p_local, "closureKey" | "closureValue" | "closureIndividual") {
            self.entries.contains(subject)
        } else {
            self.owners.contains(subject)
        }
    }

    /// Whether the row `subject rdf:type <logic:class_local>` declares `subject` in its
    /// configuration role: `logic:ClosureEntry` declares an entry, the other classes an owner.
    pub(super) fn declares_class(&self, class_local: &str, subject: &str) -> bool {
        if class_local == "ClosureEntry" {
            self.entries.contains(subject)
        } else {
            self.owners.contains(subject)
        }
    }
}

/// Collect the IRIs / blank-node ids of every contract / preset owner and of every closure
/// entry, each in its own role (see [`ContractConfigSubjects`]).
///
/// An object of `logic:closureEntry` on a contract or preset is a closure entry whether or not
/// it is typed `logic:ClosureEntry`.
pub(super) fn collect_contract_config_subjects(store: &RdfDataset) -> ContractConfigSubjects {
    let mut subjects = ContractConfigSubjects::default();
    let mut owners: Vec<Subject> = Vec::new();
    for class_local in ["ReasoningContract", "ReasoningPreset"] {
        owners.extend(subjects_of_structural_class(
            store,
            &Node::iri(logic_iri(class_local)),
        ));
    }
    for entry in subjects_of_structural_class(store, &Node::iri(logic_iri("ClosureEntry"))) {
        subjects.entries.insert(subject_str(&entry));
    }
    let closure_entry = nn(&logic_iri("closureEntry"));
    for owner in &owners {
        subjects.owners.insert(subject_str(owner));
        for entry in objects(store, owner, &closure_entry) {
            if let Some(entry) = term_as_subject(&entry) {
                subjects.entries.insert(subject_str(&entry));
            }
        }
    }
    subjects
}
