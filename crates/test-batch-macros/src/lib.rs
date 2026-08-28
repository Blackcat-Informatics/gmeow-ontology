// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only macros that preserve named contracts in one consolidated runner.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Expr, Item, ItemFn, ItemMod, ReturnType, parse_macro_input};

/// Register one ordinary zero-argument conformance contract with the consolidated
/// runner instead of exposing it as an independent libtest process.
#[proc_macro_attribute]
pub fn batch_test(_args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    if let Err(error) = validate_zero_argument_contract(&input) {
        return error.into_compile_error().into();
    }
    let name = &input.sig.ident;
    let registration = registration(name, quote!(1usize));

    quote! {
        #input
        #registration
    }
    .into()
}

/// Turn `rstest`-style `#[case::name(Case::...)]` rows into one registered contract.
///
/// Nextest normally invokes every parameterized row in a fresh process. Corpus-backed
/// conformance cases would therefore restore and parse the same authenticated bundle and
/// shape model hundreds of times. This macro retains the readable case table but emits a
/// single function that passes all named cases to `conformance_support::run_cases`.
/// The crate-level consolidated runner invokes that function in the same process as
/// every other conformance table.
#[proc_macro_attribute]
pub fn batch_cases(_args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let name = input.sig.ident;
    let visibility = input.vis;
    let mut retained_attributes = Vec::new();
    let mut cases = Vec::new();

    for (index, attribute) in input.attrs.into_iter().enumerate() {
        if is_case(&attribute) {
            let label = attribute
                .path()
                .segments
                .iter()
                .skip(1)
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let label = if label.is_empty() {
                format!("case_{}", index + 1)
            } else {
                label
            };
            match attribute.parse_args::<Expr>() {
                Ok(expression) => cases.push((label, expression)),
                Err(error) => return error.into_compile_error().into(),
            }
        } else {
            retained_attributes.push(attribute);
        }
    }

    if cases.is_empty() {
        return syn::Error::new_spanned(&name, "batch_cases requires at least one #[case(...)]")
            .into_compile_error()
            .into();
    }
    let logical_cases = cases.len();
    let registration = registration(&name, quote!(#logical_cases));
    let labels = cases.iter().map(|(label, _)| label);
    let expressions = cases.iter().map(|(_, expression)| expression);

    quote! {
        #(#retained_attributes)*
        #visibility fn #name() {
            crate::conformance_support::run_cases([
                #((#labels, #expressions)),*
            ]);
        }
        #registration
    }
    .into()
}

/// Consolidate every ordinary test in the native MCP test module into two runners.
///
/// Nextest executes each libtest identity in a fresh process. The MCP suite's immutable
/// view restores a 140 MiB authenticated pack and builds multi-gigabyte indexes, so one
/// process per assertion made the same setup dominate the required lane. This module
/// transform retains every named function and catches every panic independently while
/// running required and maint-heavy inventories in one process each. Explicitly ignored
/// tests remain ordinary libtest entries.
#[proc_macro_attribute]
pub fn batch_mcp_module(_args: TokenStream, item: TokenStream) -> TokenStream {
    let mut module = parse_macro_input!(item as ItemMod);
    let Some((_, items)) = module.content.as_mut() else {
        return syn::Error::new_spanned(module, "batch_mcp_module requires an inline test module")
            .into_compile_error()
            .into();
    };

    let mut registrations = Vec::new();
    for item in items.iter_mut() {
        let Item::Fn(function) = item else {
            continue;
        };
        let is_test = function
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"));
        let is_ignored = function
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("ignore"));
        if !is_test || is_ignored {
            continue;
        }
        if let Err(error) = validate_zero_argument_contract(function) {
            return error.into_compile_error().into();
        }
        function
            .attrs
            .retain(|attribute| !attribute.path().is_ident("test"));
        let cfg_attributes = function
            .attrs
            .iter()
            .filter(|attribute| {
                attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr")
            })
            .cloned()
            .collect::<Vec<_>>();
        let name = &function.sig.ident;
        let name_text = name.to_string();
        let maint_heavy = name_text.ends_with("_heavy_offgate")
            || name_text == "json_rpc_protocol_conformance_round_trip";
        registrations.push(quote! {
            #(#cfg_attributes)*
            inventory::submit! {
                crate::tests::RegisteredMcpContract {
                    module: module_path!(),
                    name: stringify!(#name),
                    run: #name,
                    maint_heavy: #maint_heavy,
                }
            }
        });
    }

    let attrs = module.attrs;
    let vis = module.vis;
    let unsafety = module.unsafety;
    let ident = module.ident;
    let items = module.content.expect("inline module").1;
    quote! {
        #(#attrs)*
        #vis #unsafety mod #ident {
            #(#items)*

            pub(crate) struct RegisteredMcpContract {
                pub module: &'static str,
                pub name: &'static str,
                pub run: fn(),
                pub maint_heavy: bool,
            }

            inventory::collect!(RegisteredMcpContract);
            #(#registrations)*

            fn run_registered_mcp_contracts(maint_heavy: bool) {
                let mut contracts = inventory::iter::<RegisteredMcpContract>
                    .into_iter()
                    .filter(|contract| contract.maint_heavy == maint_heavy)
                    .collect::<Vec<_>>();
                contracts.sort_by(|left, right| {
                    left.module
                        .cmp(right.module)
                        .then_with(|| left.name.cmp(right.name))
                });
                assert!(
                    !contracts.is_empty(),
                    "the selected consolidated MCP lane contains no contracts"
                );
                if !maint_heavy {
                    assert!(
                        contracts.len() >= 100,
                        "the required consolidated MCP inventory unexpectedly shrank to {} contracts",
                        contracts.len()
                    );
                }
                let selected = contracts.len();
                let mut failures = Vec::new();
                let mut timings = Vec::with_capacity(selected);
                for contract in contracts {
                    let started = std::time::Instant::now();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(contract.run));
                    timings.push((
                        format!("{}::{}", contract.module, contract.name),
                        started.elapsed(),
                    ));
                    if let Err(payload) = result {
                        let detail = payload
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_string()))
                            .unwrap_or_else(|| "non-string panic".to_string());
                        failures.push(format!("{}::{}: {detail}", contract.module, contract.name));
                    }
                }
                timings.sort_by(|left, right| right.1.cmp(&left.1));
                let elapsed = timings
                    .iter()
                    .fold(std::time::Duration::ZERO, |total, (_, duration)| total + *duration);
                eprintln!(
                    "consolidated MCP lane: {selected} contracts, {:.3}s aggregate contract time",
                    elapsed.as_secs_f64()
                );
                for (name, duration) in timings
                    .iter()
                    .filter(|(_, duration)| *duration >= std::time::Duration::from_millis(100))
                {
                    eprintln!("  {:>10.3}s  {name}", duration.as_secs_f64());
                }
                assert!(
                    failures.is_empty(),
                    "{} of {selected} consolidated MCP contract(s) failed:\n{}",
                    failures.len(),
                    failures.join("\n")
                );
            }

            #[test]
            fn required_mcp_contracts_share_one_authenticated_view() {
                run_registered_mcp_contracts(false);
            }

            #[test]
            fn maint_heavy_mcp_contracts_share_one_authenticated_view_heavy_offgate() {
                run_registered_mcp_contracts(true);
            }
        }
    }
    .into()
}

fn validate_zero_argument_contract(input: &ItemFn) -> syn::Result<()> {
    if input.sig.asyncness.is_some()
        || !input.sig.inputs.is_empty()
        || !input.sig.generics.params.is_empty()
        || !matches!(input.sig.output, ReturnType::Default)
    {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "batch_test requires a synchronous, non-generic fn() with no return value",
        ));
    }
    Ok(())
}

fn registration(
    name: &syn::Ident,
    logical_cases: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        inventory::submit! {
            crate::conformance_support::RegisteredConformanceContract {
                module: module_path!(),
                name: stringify!(#name),
                run: #name,
                logical_cases: #logical_cases,
            }
        }
    }
}

fn is_case(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .first()
        .is_some_and(|segment| segment.ident == "case")
}
