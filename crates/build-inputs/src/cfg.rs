// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{Result, fail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Meta, Token, parse::Parser, punctuated::Punctuated};

/// Complete rustc cfg context for one selected Cargo unit. Known but absent axes
/// are false; a custom axis without a recorded owner is an admission failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CfgContext {
    pub flags: BTreeSet<String>,
    pub values: BTreeMap<String, BTreeSet<String>>,
    pub known: BTreeSet<String>,
}
impl CfgContext {
    pub fn from_rustc(output: &str, features: &[String], debug_assertions: bool) -> Result<Self> {
        let mut context = Self {
            flags: BTreeSet::new(),
            values: BTreeMap::new(),
            known: [
                "test",
                "doc",
                "doctest",
                "debug_assertions",
                "unix",
                "windows",
                "feature",
                "panic",
                "target_arch",
                "target_endian",
                "target_env",
                "target_family",
                "target_feature",
                "target_has_atomic",
                "target_os",
                "target_pointer_width",
                "target_vendor",
                "target_abi",
                "target_has_atomic_equal_alignment",
                "target_has_atomic_load_store",
                "target_thread_local",
                "sanitize",
                "proc_macro",
                "overflow_checks",
                "ub_checks",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        };
        for line in output.lines().filter(|line| !line.is_empty()) {
            let meta: Meta = syn::parse_str(line).map_err(fail)?;
            match meta {
                Meta::Path(path) => {
                    let name = ident(&path)?;
                    context.known.insert(name.clone());
                    context.flags.insert(name);
                }
                Meta::NameValue(pair) => {
                    let name = ident(&pair.path)?;
                    let value = string(&pair.value)?;
                    context.known.insert(name.clone());
                    context.values.entry(name).or_default().insert(value);
                }
                _ => return Err(fail("rustc --print cfg returned a compound expression")),
            }
        }
        context.flags.remove("test");
        context.flags.remove("doc");
        context.flags.remove("doctest");
        context.flags.remove("debug_assertions");
        if debug_assertions {
            context.flags.insert("debug_assertions".into());
        }
        context
            .values
            .insert("feature".into(), features.iter().cloned().collect());
        Ok(context)
    }
    pub(crate) fn evaluate(&self, meta: &Meta) -> Result<bool> {
        match meta {
            Meta::Path(path) => {
                let name = ident(path)?;
                self.require(&name)?;
                Ok(self.flags.contains(&name))
            }
            Meta::NameValue(pair) => {
                let name = ident(&pair.path)?;
                self.require(&name)?;
                let value = string(&pair.value)?;
                Ok(self
                    .values
                    .get(&name)
                    .is_some_and(|values| values.contains(&value)))
            }
            Meta::List(list) => {
                let args = Punctuated::<Meta, Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                    .map_err(fail)?;
                let values = args
                    .iter()
                    .map(|arg| self.evaluate(arg))
                    .collect::<Result<Vec<_>>>()?;
                match ident(&list.path)?.as_str() {
                    "all" => Ok(values.iter().all(|v| *v)),
                    "any" => Ok(values.iter().any(|v| *v)),
                    "not" if values.len() == 1 => Ok(!values[0]),
                    other => Err(fail(format!("unsupported cfg operator {other}"))),
                }
            }
        }
    }
    /// Canonical semantic ownership includes every non-test configuration branch.
    /// This is a symbolic source contract, never a replacement for Cargo's selected cfg.
    pub(crate) fn portable(&self, meta: &Meta) -> Result<Possible> {
        match meta {
            Meta::Path(path) => {
                let name = ident(path)?;
                self.require(&name)?;
                Ok(if matches!(name.as_str(), "test" | "doc" | "doctest") {
                    Possible::NO
                } else {
                    Possible::BOTH
                })
            }
            Meta::NameValue(pair) => {
                self.require(&ident(&pair.path)?)?;
                string(&pair.value)?;
                Ok(Possible::BOTH)
            }
            Meta::List(list) => {
                let args = Punctuated::<Meta, Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                    .map_err(fail)?;
                let values = args
                    .iter()
                    .map(|arg| self.portable(arg))
                    .collect::<Result<Vec<_>>>()?;
                match ident(&list.path)?.as_str() {
                    "all" => Ok(Possible {
                        yes: values.iter().all(|value| value.yes),
                        no: values.iter().any(|value| value.no),
                    }),
                    "any" => Ok(Possible {
                        yes: values.iter().any(|value| value.yes),
                        no: values.iter().all(|value| value.no),
                    }),
                    "not" if values.len() == 1 => Ok(Possible {
                        yes: values[0].no,
                        no: values[0].yes,
                    }),
                    other => Err(fail(format!("unsupported portable cfg operator {other}"))),
                }
            }
        }
    }
    fn require(&self, name: &str) -> Result<()> {
        if self.known.contains(name) {
            Ok(())
        } else {
            Err(fail(format!("custom cfg {name} has no selected owner")))
        }
    }
}
pub(crate) fn ident(path: &syn::Path) -> Result<String> {
    path.get_ident()
        .map(ToString::to_string)
        .ok_or_else(|| fail("cfg/path attribute requires one identifier"))
}
pub(crate) fn string(expr: &syn::Expr) -> Result<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(fail("expected a literal string")),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Possible {
    pub yes: bool,
    pub no: bool,
}
impl Possible {
    const NO: Self = Self {
        yes: false,
        no: true,
    };
    const BOTH: Self = Self {
        yes: true,
        no: true,
    };
}
