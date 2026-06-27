// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native claim-view emission — GMEOW's internal
//! `generated/queries/observation-claim-view.rq` SPARQL CONSTRUCT.
//!
//! Unlike the standpoint projections ([`crate::standpoint_emit`], which re-express
//! GMEOW in *external* peer models) and the per-profile SPARQL projections
//! ([`crate::sparql_emit`]), this is an INTERNAL gmeow→gmeow view: it materialises
//! the legacy `gmeow:Observation` / `gmeow:StandpointClaim` query surface FROM the
//! canonical `gmeow:ClaimToken` layer, so generic "all observations about X"
//! consumers keep working after the proposition / claim-token / attitude /
//! evaluation separation. The unified observation is "a projected union view over
//! the four constructs — a convenience surface, generated, never the canonical
//! record" (the foundation). `gmeow:observedFeature` is back-filled from
//! `gmeow:expresses`; `gmeow:vantage` from the asserting agent
//! (`gmeow:wasAssociatedWith`). The output is byte-identical to the committed
//! `observation-claim-view.rq` (the parity gate).

use crate::sparql_emit::{prefix_block, GENERATED_BANNER};

/// The committed file name of the internal observation union view.
pub const CLAIM_VIEW_FILE: &str = "observation-claim-view.rq";

/// Emit the internal observation union view: a CONSTRUCT that materialises
/// `gmeow:Observation` / `gmeow:StandpointClaim` triples from each
/// `gmeow:ClaimToken`, back-filling `gmeow:observedFeature` from `gmeow:expresses`
/// and `gmeow:vantage` from the asserting agent. Suppressed tokens
/// (`gmeow:displayable false`) are excluded (Principle 10).
///
/// Takes no DSL input — it is a constant template-coded query — so it is
/// infallible, matching the individual standpoint emitters.
pub fn emit_claim_view() -> String {
    let body = "CONSTRUCT {\n\
         \x20   ?tok a gmeow:Observation , gmeow:StandpointClaim ;\n\
         \x20       gmeow:observedFeature ?prop ;\n\
         \x20       gmeow:vantage ?who .\n\
         }\n\
         WHERE {\n\
         \x20   ?tok a gmeow:ClaimToken ;\n\
         \x20       gmeow:expresses ?prop .\n\
         \x20   OPTIONAL { ?tok gmeow:wasAssociatedWith ?who }\n\
         \x20   FILTER NOT EXISTS { ?tok gmeow:displayable false }\n\
         }\n"
    .to_string();
    let header = format!(
        "# Projection: GMEOW claim-token layer → Observation / StandpointClaim union view. {GENERATED_BANNER}\n\
         # Internal gmeow→gmeow view: materialises the legacy gmeow:Observation /\n\
         # gmeow:StandpointClaim query surface from canonical gmeow:ClaimToken individuals,\n\
         # so generic \"all observations about X\" consumers keep working after the\n\
         # proposition / claim-token / attitude / evaluation separation. observedFeature\n\
         # is back-filled from gmeow:expresses; vantage from the asserting agent\n\
         # (gmeow:wasAssociatedWith). Suppressed tokens (gmeow:displayable false) drop out.\n"
    );
    format!("{header}{}\n\n{body}", prefix_block(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo root (two levels up from crates/slice).
    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn claim_view_matches_committed() {
        let text = emit_claim_view();
        let committed_path = repo_root()
            .join("generated")
            .join("queries")
            .join(CLAIM_VIEW_FILE);
        let committed = std::fs::read_to_string(&committed_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", committed_path.display()));
        assert_eq!(text, committed, "claim view drifted from committed");
    }

    #[test]
    fn claim_view_constructs_observation_surface_from_claim_tokens() {
        let text = emit_claim_view();
        // Reads the canonical layer...
        assert!(text.contains("?tok a gmeow:ClaimToken"));
        assert!(text.contains("gmeow:expresses ?prop"));
        // ...and materialises the legacy observation surface.
        assert!(text.contains("gmeow:Observation , gmeow:StandpointClaim"));
        assert!(text.contains("gmeow:observedFeature ?prop"));
        // Suppression is honoured.
        assert!(text.contains("FILTER NOT EXISTS { ?tok gmeow:displayable false }"));
    }
}
