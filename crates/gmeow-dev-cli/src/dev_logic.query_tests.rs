// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::resolve_query;

const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

#[test]
fn counterfactual_depth_refusal_does_not_build_an_unused_base_snapshot() {
    // The quoted-triple object is deliberately outside the snapshot reifier
    // contract. A depth-zero counterfactual returns before it needs any base
    // snapshot; the plain-query preparation path must not run speculatively.
    let nquads = "<https://ex/s> <https://ex/meta> \
                      <<( <https://ex/qs> <https://ex/qp> <https://ex/qo> )>> \
                      <http://world/base> .\n";
    let program = ":- prefix(ex, 'https://ex/').\n\
                       :- counterfactual('http://world/cf', 'http://world/base').\n\
                       :- depth_budget(0).\n\
                       :- assume(ex:p2(ex:s, ex:o2)).\n\
                       ?- ex:p(ex:s, Z).\n";

    let (answers, status) = resolve_query(nquads, program, HORN_PROFILE, None, None, None).unwrap();
    assert!(answers.is_empty());
    assert_eq!(status, "incomplete");
}
