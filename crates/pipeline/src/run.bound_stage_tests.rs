// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The implementation the authored pipeline binds to stage `id` under the default
/// registry, so a test reads the declarations the production stage reports.
///
/// # Panics
/// When the authored pipeline does not validate or bind, or has no stage `id`.
pub(crate) fn bound_production_stage(id: &str) -> Arc<dyn crate::node::Stage> {
    let spec = full_spec();
    let graph = spec.validate().expect("the authored pipeline validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("the authored pipeline binds");
    bound
        .into_iter()
        .find(|stage| stage.id() == id)
        .unwrap_or_else(|| panic!("the authored pipeline has no stage {id}"))
}
