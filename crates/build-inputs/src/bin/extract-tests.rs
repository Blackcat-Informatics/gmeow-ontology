// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Offline migration utility. Input source paths come from an explicitly reviewed
//! production inventory; planning never modifies sources, and apply requires the
//! exact saved plan with no unresolved semantic blockers.

#[path = "../extract.rs"]
mod extract;

use purrdf_lex::json::{
    self,
    record::{self, ToJson},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: extract-tests plan|native-plan|selected-plan|apply|verify-format ROOT INPUT_JSON OUTPUT_JSON".into());
    }
    let root = std::path::Path::new(&args[1]).canonicalize()?;
    if args[0] == "native-plan" {
        let sources = gmeow_build_inputs::native_extraction_paths(&root)?;
        std::fs::write(&args[2], json::write_pretty(&sources.to_json()))?;
        let plan = extract::plan(&root, &sources)?;
        std::fs::write(&args[3], json::write_pretty(&plan.to_json()))?;
    } else if args[0] == "selected-plan" {
        let selection = purrdf_lex::json::record::from_slice(&std::fs::read(&args[2])?)?;
        let sources = gmeow_build_inputs::production_extraction_paths(&root, &selection)?;
        let plan = extract::plan(&root, &sources)?;
        std::fs::write(&args[3], json::write_pretty(&plan.to_json()))?;
    } else if args[0] == "plan" {
        let sources: Vec<String> = record::from_slice(&std::fs::read(&args[2])?)?;
        let plan = extract::plan(&root, &sources)?;
        std::fs::write(&args[3], json::write_pretty(&plan.to_json()))?;
    } else if args[0] == "apply" {
        let plan: extract::ExtractionPlan = record::from_slice(&std::fs::read(&args[2])?)?;
        extract::apply(&root, &plan)?;
        std::fs::write(&args[3], json::write_pretty(&plan.preservation.to_json()))?;
    } else if args[0] == "verify-format" {
        let plan: extract::ExtractionPlan = record::from_slice(&std::fs::read(&args[2])?)?;
        let evidence = extract::verify_format(&root, &plan)?;
        std::fs::write(&args[3], json::write_pretty(&evidence.to_json()))?;
    } else {
        return Err("expected plan, native-plan, selected-plan, apply or verify-format".into());
    }
    Ok(())
}
