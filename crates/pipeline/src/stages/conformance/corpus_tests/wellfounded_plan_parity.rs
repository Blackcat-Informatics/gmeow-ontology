// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The authenticated source plan must document the exact native materializer phases.
//! Its explicit iteration stays outside the certified acyclic DAG fragment. The
//! native runtime never interprets this source plan for scheduling (Principle 12).

use std::sync::OnceLock;

use super::super::wellfounded_plan::{CHANNEL, Observation, SOURCE};

#[test]
fn authored_phase_plan_matches_runtime_phase_order() {
    static OBSERVED: OnceLock<
        Result<super::source_artifact::Selected<Observation>, gmeow_errors::Diag>,
    > = OnceLock::new();
    let observed = super::source_artifact::get(&OBSERVED, CHANNEL);
    assert_eq!(observed.source_path, SOURCE);
    assert_eq!(
        observed.source_digest.len(),
        64,
        "exact original module identity"
    );
    assert!(
        observed
            .plan_types
            .contains("https://blackcatinformatics.ca/logic/Plan"),
        "logic:wellFoundedMaterializerPlan must be `a logic:Plan`"
    );
    assert_eq!(
        observed.body_roots.len(),
        1,
        "the authored plan must have exactly one program-tree root"
    );
    let walked = observed.walk.as_ref().expect("the exact original phase plan must have single-valued links and recognised transaction-program combinators");
    assert_eq!(
        walked.phases,
        gmeow_logic::WELL_FOUNDED_PHASES.to_vec(),
        "the authored logic:wellFoundedMaterializerPlan phase order must equal \
         gmeow_logic::WELL_FOUNDED_PHASES (the runtime twin)"
    );
    assert_eq!(
        walked.iterated,
        vec![gmeow_logic::WELL_FOUNDED_ITERATED_PHASE.to_string()],
        "the phase wrapped in a logic:Iteration must be exactly \
         gmeow_logic::WELL_FOUNDED_ITERATED_PHASE"
    );
}
