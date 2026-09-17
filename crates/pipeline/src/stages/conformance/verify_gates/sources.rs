// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit source-role inventory and single-statement positive controls.

use super::{Object, Repair, Source};

pub(super) fn all() -> Vec<Source> {
    vec![
        Source {
            path: "slices/grounding/logic/tests/counter-examples/advisory-laundered-into-an-authorization-proof.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/capability-gap-without-blocked-step.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/checkpoint-no-folded-identity.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/checkpoint-restored-under-a-drifted-fold.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/compensation-names-no-forward-effect.ttl",
            repairs: vec![Repair {
                name: "a_compensation_naming_its_forward_receipt_passes_on_verify",
                subject: "https://blackcatinformatics.ca/gmeow/examples/logic/tests/invoice901Refund",
                predicate: "https://blackcatinformatics.ca/logic/compensatesEffect",
                remove: None,
                insert: Object::Iri(
                    "https://blackcatinformatics.ca/gmeow/examples/logic/tests/invoice901ChargeReceipt",
                ),
            }],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/compensation-typed-as-its-forward-receipt.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/context-assembly-serving-no-enactment.ttl",
            repairs: vec![Repair {
                name: "an_assembly_naming_the_run_it_served_passes_on_verify",
                subject: "https://blackcatinformatics.ca/gmeow/examples/logic/tests/assemblyServingNobody",
                predicate: "https://blackcatinformatics.ca/logic/assemblyForEnactment",
                remove: None,
                insert: Object::Iri(
                    "https://blackcatinformatics.ca/gmeow/examples/logic/tests/adrReviewWeek13",
                ),
            }],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/frontier-cites-a-content-free-witness.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/frontier-closed-on-a-budget-cut-witness.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/frontier-closed-without-saturation-witness.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/journal-entry-establishing-no-head.ttl",
            repairs: vec![Repair {
                name: "a_journal_entry_naming_both_of_its_heads_passes_on_verify",
                subject: "https://blackcatinformatics.ca/gmeow/examples/logic/tests/journalEntry9",
                predicate: "https://blackcatinformatics.ca/logic/journalNewHead",
                remove: None,
                insert: Object::String(
                    "b3:3fb1a7c05e29d648b03c7a15f9e02d84c76b1350ae42f9d867b0c31de5a4028f",
                ),
            }],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/maintenance-goal-closed-by-one-good-week.ttl",
            repairs: vec![
                Repair {
                    name: "a_maintenance_goal_held_so_far_but_undetermined_passes_on_verify",
                    subject: "https://blackcatinformatics.ca/gmeow/examples/logic/tests/maintenanceGoalClosedByWeek12",
                    predicate: "https://blackcatinformatics.ca/logic/goalEvaluationStatus",
                    remove: Some(Object::Iri(
                        "https://blackcatinformatics.ca/logic/GoalEvaluationCompleted",
                    )),
                    insert: Object::Iri(
                        "https://blackcatinformatics.ca/logic/GoalEvaluationUndetermined",
                    ),
                },
                Repair {
                    name: "a_maintenance_goal_conclusively_violated_passes_on_verify",
                    subject: "https://blackcatinformatics.ca/gmeow/examples/logic/tests/maintenanceGoalClosedByWeek12",
                    predicate: "https://blackcatinformatics.ca/logic/satisfactionStatus",
                    remove: Some(Object::Iri(
                        "https://blackcatinformatics.ca/logic/Satisfied",
                    )),
                    insert: Object::Iri("https://blackcatinformatics.ca/logic/Violated"),
                },
            ],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/ocr-gap-remedied-for-a-different-step.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/pin-freezes-steps-its-method-never-yielded.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/prescription-version-not-content-addressed.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/prescription-version-revised-under-a-running-enactment.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/receipt-without-attempt.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/retry-licensed-by-a-verdictless-probe.ttl",
            repairs: vec![Repair {
                name: "a_reconciliation_result_carrying_its_verdict_passes_on_verify",
                subject: "https://blackcatinformatics.ca/gmeow/examples/logic/tests/invoice903ProbeResult",
                predicate: "https://blackcatinformatics.ca/logic/reconciliationVerdict",
                remove: None,
                insert: Object::Iri("https://blackcatinformatics.ca/logic/ReconciledNotCommitted"),
            }],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/unknown-outcome-retried-on-a-borrowed-licence.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/logic/tests/counter-examples/unknown-outcome-without-attempt.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/math/examples/gmn-dimension-roundtrip.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/math/tests/counter-examples/dimension-zero-denominator.ttl",
            repairs: vec![],
        },
        Source {
            path: "slices/grounding/math/tests/counter-examples/force-dimension-inhomogeneous.ttl",
            repairs: vec![],
        },
    ]
}
