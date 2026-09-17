// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compiled inventory contracts for nested MCP registration and feature selection.

use std::sync::atomic::{AtomicUsize, Ordering};

static EXECUTED: AtomicUsize = AtomicUsize::new(0);

mod tests {
    pub struct RegisteredMcpContract {
        pub module: &'static str,
        pub name: &'static str,
        pub run: fn(),
        pub maint_heavy: bool,
    }

    inventory::collect!(RegisteredMcpContract);
}

mod nested {
    use super::{EXECUTED, Ordering};

    #[gmeow_test_batch_macros::batch_mcp_test]
    #[cfg(test)]
    fn required_contract() {
        EXECUTED.fetch_or(1, Ordering::SeqCst);
    }

    #[gmeow_test_batch_macros::batch_mcp_test]
    fn breadth_contract_heavy_offgate() {
        EXECUTED.fetch_or(2, Ordering::SeqCst);
    }

    #[gmeow_test_batch_macros::batch_mcp_test]
    fn json_rpc_protocol_conformance_round_trip() {
        EXECUTED.fetch_or(4, Ordering::SeqCst);
    }

    // These intentionally unresolvable bodies prove that both feature-gated
    // functions and their inventory references disappear before name resolution.
    #[gmeow_test_batch_macros::batch_mcp_test]
    #[cfg(not(test))]
    fn disabled_contract() {
        disabled_dependency_must_not_be_resolved();
    }

    #[gmeow_test_batch_macros::batch_mcp_test]
    #[cfg_attr(test, cfg(not(test)))]
    fn conditionally_disabled_contract() {
        disabled_dependency_must_not_be_resolved();
    }
}

#[test]
fn nested_contracts_preserve_names_lanes_execution_and_feature_gates() {
    let mut contracts = inventory::iter::<tests::RegisteredMcpContract>
        .into_iter()
        .collect::<Vec<_>>();
    contracts.sort_by_key(|contract| contract.name);
    assert_eq!(
        contracts
            .iter()
            .map(|contract| (contract.name, contract.maint_heavy))
            .collect::<Vec<_>>(),
        [
            ("breadth_contract_heavy_offgate", true),
            ("json_rpc_protocol_conformance_round_trip", true),
            ("required_contract", false),
        ]
    );
    for contract in contracts {
        assert_eq!(contract.module, concat!(module_path!(), "::nested"));
        (contract.run)();
    }
    assert_eq!(EXECUTED.load(Ordering::SeqCst), 7);
}
