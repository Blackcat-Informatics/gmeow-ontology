// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The stable operational `ReasoningSession` façade.
//!
//! This submodule tree promotes the crate-internal incremental maintenance engine
//! ([`crate::cost::IncrementalForwardSession`], a DBSP-style signed-Z-set maintainer
//! over *finite positive binary Datalog*) into a **stable public session contract** an
//! external runtime consumer can pin against. It binds seven identities into one
//! content-addressed [`SessionIdentity`], applies content-addressed [`SessionDelta`]s
//! that reference an *authorized* workspace commit (adding facts + retiring active
//! state, **never** becoming an authority writer), supports content-addressed /
//! integrity-checked / identity-gated [`Checkpoint`]s, classifies every operation into
//! six disjoint [`OperationOutcome`]s, and is crash/replay safe via a hash-linked
//! [`TransitionEntry`] journal (a delta can never be applied twice).
//!
//! # The seven bound identities ([`SessionIdentity`])
//!
//! 1. **Published data-generation** — a `urn:blake3:` content address of the authorized
//!    EDB facts (minted with the shared framed-BLAKE3 discipline).
//! 2. **Rule/program digest + slice digest** — from the canonical [`LogicProgram`].
//! 3. **`ReasoningContract`** (carries the resource policy) — pinned on
//!    [`gmeow_logic_compile::ir::ReasoningContract::content_digest`].
//! 4. **Engine implementation/version** — the whole
//!    [`crate::runtime::EngineContract::current`] descriptor.
//! 5. **Tuple-annotation algebra** — the [`crate::annotation::AnnotationContract`]
//!    canonical key.
//! 6. **Supported incremental fragment** — the fixed string naming finite positive
//!    binary Datalog (the set `apply` maintains as `Applied`; every other fragment is
//!    refused/routed).
//! 7. **Resource policy** — folded via the `ReasoningContract` content digest (facet 3).
//!
//! All seven are framed into one `descriptor_hash`, so a mismatch on ANY axis is a
//! detectable identity drift when restoring a checkpoint.
//!
//! # Total, disjoint outcomes
//!
//! The `apply`-family methods are **total**: they never panic and always return an
//! [`OperationOutcome`]. Unsupported fragments hard-refuse ([`OperationOutcome::UnsupportedFragment`])
//! or route to a labelled rebuild ([`OperationOutcome::RequiresFullRebuild`]), never a
//! silent approximation.
//!
//! # Semver governance
//!
//! Every public type below is `#[non_exhaustive]`; construction is only via the
//! provided constructors. Adding a variant, field, or outcome is therefore an additive
//! (minor) change. The `-v1` domain tags are the semver-stable serialization contract.

mod checkpoint;
mod delta;
mod facade;
mod identity;
mod journal;
mod outcome;

pub use checkpoint::Checkpoint;
pub use delta::{SessionDelta, Suppression};
pub use facade::{PagedCompositionMetrics, ReasoningSession};
pub use identity::SessionIdentity;
pub use journal::TransitionEntry;
pub use outcome::{
    FragmentDisposition, IncompleteCause, IntegrityFault, OperationOutcome, OutcomeTag,
    RebuildReason, UnsupportedFragment,
};

/// The source-contract identity minted into every resident session's
/// data-generation [`crate::seam::WorldSourceIdentity`].
///
/// It names the authorized-EDB content-address discipline (a `urn:blake3:` generation
/// framed with the shared domain-tagged BLAKE3 hasher), distinct from the snapshot
/// source contract used by the paged/derived world source.
pub(crate) const SESSION_SOURCE_CONTRACT: &str =
    "https://blackcatinformatics.ca/logic/session/authorized-edb-generation-v1";

/// The fixed name of the incrementally-certified fragment the façade maintains:
/// finite positive binary Datalog (exactly what
/// [`crate::cost::IncrementalForwardSession::prepare`] accepts). Every `Applied`
/// operation is a genuine incremental maintenance over this fragment; every other
/// fragment (negation, chase, stable-model) is refused/routed.
pub(crate) const CERTIFIED_FRAGMENT: &str =
    "https://blackcatinformatics.ca/logic/session/fragment/FinitePositiveBinaryDatalog";
