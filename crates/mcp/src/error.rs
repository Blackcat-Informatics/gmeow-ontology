// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! MCP diagnostic kinds.
//!
//! Every defect on the agent-facing tool surface is a HARD failure
//! (no-optionality): a snapshot that will not read, a query that is not the SELECT
//! the tool demanded, a term that resolves ambiguously, a memory append that will
//! not commit, a tool name nothing dispatches. Each is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `mcp.*` code
//! namespace, so the MCP surface reports on the same content-bound substrate as
//! every other crate rather than raising bare strings.
//!
//! The four [`crate::extension`] kinds are the *totality* guards on the tool and
//! resource surface. They exist because the surface is now assembled from two
//! independent halves — the consumer builtins declared here and whatever a host
//! crate (`gmeow-mcp-dev`) registers on top — and a name that is advertised but not
//! dispatchable, or dispatched but not registered, or registered twice, is a wiring
//! defect that must be named and refused, never absorbed into a silent no-op.
//!
//! [`SegmentNotLoaded`] is the one kind that is NOT a defect: it is the tiered
//! deployment's routing signal, saying "this tool is real, advertised, and dispatchable
//! — its implementation just lives in a segment you have not loaded yet". It is typed
//! separately from [`UnknownTool`] precisely so a client can tell deferral from absence
//! without parsing prose.
//!
//! [`MCP_DIAG_CODES`] and [`register_all`] are this crate's single, complete
//! catalog.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A hard defect raised by the MCP server surface (snapshot decode, query
    /// dispatch, memory access, or transaction append).
    pub struct Mcp { message: String }
    code = "mcp.error";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "mcp error: {}", message;
}

define_diag_kind! {
    /// A consumer query matched a bare local name in more than one namespace on the
    /// MCP surface — a HARD fail (no silent namespace precedence), the twin of the
    /// shippable-CLI `gmeow-cli.describe.ambiguous`. Minted DISTINCT from the generic
    /// unknown-term [`Mcp`] so an ambiguous term is greppable as its own code. The
    /// message names the query and lists the sorted candidate CURIEs the caller must
    /// disambiguate between.
    pub struct McpAmbiguousTerm { message: String }
    code = "mcp.ambiguous-term";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", message;
}

define_diag_kind! {
    /// `tools/call` named a tool that nothing in the assembled surface dispatches.
    /// Raised by the TOTAL dispatch in [`crate::extension::Surface::dispatch_tool`]:
    /// there is no fallthrough arm and no silent no-op, so a consumer asking for a
    /// dev-only tool, or a client asking for a tool that does not exist, gets one
    /// named refusal.
    pub struct UnknownTool { name: String }
    code = "mcp.unknown-tool";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown tool: {}", name;
    failure_class = "https://blackcatinformatics.ca/logic/UnadvertisedToolInvocation";
}

define_diag_kind! {
    /// `resources/read` named a resource URI that nothing in the assembled surface
    /// serves. The resource twin of [`UnknownTool`].
    pub struct UnknownResource { uri: String }
    code = "mcp.unknown-resource";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown resource: {}", uri;
    failure_class = "https://blackcatinformatics.ca/logic/UnadvertisedResourceRead";
}

define_diag_kind! {
    /// Two registrations claimed the same tool name or the same resource URI while
    /// the surface was being assembled. A HARD fail at construction: last-writer-wins
    /// would make the advertised descriptor and the dispatched handler silently
    /// disagree, so the server refuses to start instead. The message names the
    /// colliding key.
    pub struct DuplicateRegistration { key: String }
    code = "mcp.duplicate-registration";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "duplicate MCP registration: {}", key;
    failure_class = "https://blackcatinformatics.ca/logic/DuplicateToolRegistration";
}

define_diag_kind! {
    /// A registration is not well-formed: its descriptor carries no string `name`
    /// (tool) / `uri` (resource) / `mimeType` (resource), or the builtin descriptor
    /// list and the builtin handler list are not in bijection at the same index. All
    /// of these mean "advertised but not dispatchable" (or the converse), which the
    /// totality contract forbids.
    pub struct InvalidRegistration { message: String }
    code = "mcp.invalid-registration";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "invalid MCP registration: {}", message;
    failure_class = "https://blackcatinformatics.ca/logic/BrokenToolRegistration";
}

define_diag_kind! {
    /// `tools/call` named a tool the surface DOES advertise and DOES dispatch, but whose
    /// implementation lives in an engine segment this deployment has not loaded — the
    /// tiered browser console's lean core asking for a reasoning-segment tool.
    ///
    /// This is the deferral signal, and it is deliberately DISTINCT from
    /// [`UnknownTool`]: "not here yet" and "does not exist" are different facts and a
    /// caller must be able to tell them apart mechanically. The code is stable
    /// (`mcp.segment-not-loaded`) and the payload names BOTH the tool asked for and the
    /// segment that provides it, so a host can load exactly that segment and re-dispatch
    /// the identical frame. It is therefore never a refusal and never a degraded answer:
    /// it is a routing instruction that makes the call slower, not weaker.
    ///
    /// Raised ONLY when the deployment's [`SegmentSet`](crate::SegmentSet) excludes the
    /// segment. A build carrying the segment can never produce it, which is why the
    /// native surface and `gmeow-mcp-dev` are unaffected.
    ///
    /// `tool` is the DISPATCH KEY the frame asked for: a `tools/call` name, or a
    /// `resources/read` URI. Both halves of the surface are partitioned by segment — the
    /// reasoning image defers all five resources back to core exactly as core defers the
    /// reasoning tools forward — and a caller routes on the same two fields either
    /// way, so one signal serves both rather than two codes a host would have to learn.
    pub struct SegmentNotLoaded { tool: String, segment: String }
    code = "mcp.segment-not-loaded";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "`{}` is served by the `{}` engine segment, which this deployment has not loaded; load that segment and re-dispatch the same frame", tool, segment;
    failure_class = "https://blackcatinformatics.ca/logic/UnloadedEngineSegment";
}

define_diag_kind! {
    /// A runtime store asked to be primed with a dictionary id the LOADED bundle pins no
    /// bytes for.
    ///
    /// The bundle is the dictionary's distribution channel: `gmeow.gts` pins every
    /// declared dictionary in its segment header, so a consumer priming its own store
    /// reads the exact bytes the build trained. An id that names no bytes is a HARD FAIL
    /// — there is no weaker unprimed store to fall back to, because the store's OWN
    /// header is what makes it decodable, and writing it unprimed would silently discard
    /// the density the dictionary exists to provide.
    pub struct MediumUnpinnedStoreDictionary { detail: String }
    code = "mcp.medium.unpinned-store-dictionary";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: unpinned store dictionary — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumUnpinnedStoreDictionary";
}

/// The complete MCP diagnostic-code catalog, in registration order.
///
/// TOTAL over the crate regardless of which cargo features are selected — the catalog
/// is the diagnostic THEORY, and a lean deployment is a reduced deployment, not a
/// reduced theory (exactly as its [`TOOL_COUNT`](crate::TOOL_COUNT)-tool surface stays
/// total). `mcp.segment-not-loaded`
/// is therefore listed and interned on every build, including the ones that can never
/// raise it.
pub const MCP_DIAG_CODES: &[&str] = &[
    Mcp::CODE,
    McpAmbiguousTerm::CODE,
    UnknownTool::CODE,
    UnknownResource::CODE,
    DuplicateRegistration::CODE,
    InvalidRegistration::CODE,
    SegmentNotLoaded::CODE,
    MediumUnpinnedStoreDictionary::CODE,
];

/// Eagerly intern every MCP diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Mcp::register(),
        McpAmbiguousTerm::register(),
        UnknownTool::register(),
        UnknownResource::register(),
        DuplicateRegistration::register(),
        InvalidRegistration::register(),
        SegmentNotLoaded::register(),
        MediumUnpinnedStoreDictionary::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_mcp_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            MCP_DIAG_CODES.len(),
            "register_all() and MCP_DIAG_CODES must enumerate the same kinds"
        );
        for code in MCP_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "mcp code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = MCP_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            MCP_DIAG_CODES.len(),
            "duplicate mcp diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(
            distinct_handles.len(),
            handles.len(),
            "two mcp diagnostic kinds interned to the same code handle"
        );
    }
}
