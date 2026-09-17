// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

/// The VARIANT (per-exact-adornment) magic-sets transformation — the `#[cfg(test)]`
/// byte-identity reference the production [`magic_transform`] is checked against.
///
/// Mints a SEPARATE magic predicate per distinct demanded adornment (no subsumptive collapse):
/// this is the pre-upgrade demand keying, kept only as the A/B oracle proving the subsumptive
/// collapse leaves the answer set byte-identical. It is NOT a production path (greenfield: one
/// production demand rewrite, the subsumptive one).
pub(super) fn magic_transform_variant(
    rules: &[EvalRule],
    goal: &EvalAtom,
    goal_adorn: BindingPattern,
) -> MagicProgram {
    let idb: BTreeSet<String> = rules
        .iter()
        .map(|r| r.head.predicate.as_str().to_owned())
        .collect();

    let mut out: Vec<EvalRule> = Vec::new();
    let mut seeds: Vec<EvalAtom> = Vec::new();

    // (1) Seed: the goal's magic fact (none for an ff goal). Every unconditional demand
    //     rule below is likewise lifted into the seed set as control state.
    if let Some(s) = magic_seed_atom(goal, goal_adorn) {
        seeds.push(s);
    }

    // The full variant-keyed (pred → exact adornments) demand set.
    let demanded = demand_fixpoint(rules, &idb, goal, goal_adorn);

    // (2) Modified rules + (3) magic rules, for every demanded (head_pred, exact adorn).
    for (head_pred, codes) in &demanded {
        for adorn_code in codes {
            let head_adorn = BindingPattern::from_code(adorn_code);
            for (ri, r) in rules
                .iter()
                .enumerate()
                .filter(|(_, r)| r.head.predicate.as_str() == head_pred.as_str())
            {
                let mut bound = head_bound_vars(&r.head, head_adorn);

                let head_guard = magic_guard_atom(&r.head, head_adorn);
                let mut mod_body: Vec<EvalAtom> = Vec::new();
                if let Some(guard) = &head_guard {
                    mod_body.push(guard.clone());
                }

                let mut prefix: Vec<EvalAtom> = Vec::new();
                for (bi, atom) in r.body.iter().enumerate() {
                    if idb.contains(atom.predicate.as_str()) {
                        let a = adorn_atom(atom, &bound);
                        if let Some(magic_head) = magic_guard_atom(atom, a) {
                            let mut mbody: Vec<EvalAtom> = Vec::new();
                            if let Some(hg) = &head_guard {
                                mbody.push(hg.clone());
                            }
                            mbody.extend(prefix.iter().cloned());
                            let iri = format!(
                                "{}::magic/{}/{}#{ri}.{bi}",
                                atom.predicate.as_str(),
                                a.code(),
                                head_pred
                            );
                            emit_or_seed(magic_head, mbody, iri, &mut out, &mut seeds);
                        }
                    }
                    mod_body.push(atom.clone());
                    if atom.negated {
                        if negated_atom_fully_bound(atom, &bound) {
                            prefix.push(atom.clone());
                        }
                    } else {
                        prefix.push(atom.clone());
                        bind_atom_vars(atom, &mut bound);
                    }
                }

                let iri = format!("{}::mod/{}#{ri}", r.head.predicate.as_str(), adorn_code);
                if mod_body.is_empty() && r.builtins.is_empty() {
                    seeds.push(r.head.clone());
                } else {
                    let mut modified = rule(r.head.clone(), mod_body, iri);
                    modified.builtins = r.builtins.clone();
                    out.push(modified);
                }
            }
        }
    }

    // The variant mirrors the production transform exactly: unconditional demand-control
    // rules are lifted, while semantic NAF-only and builtin-only rules remain executable.
    assert!(
        out.iter()
            .all(|r| !r.body.is_empty() || !r.builtins.is_empty()),
        "magic_transform_variant must lift every unconditional demand-control rule into the seed set"
    );
    // Identical order-preserving end-of-transform dedup as `magic_transform` — required so
    // the A/B byte-identity oracle test comparing the two transforms' seed sets holds.
    let mut seen = std::collections::HashSet::new();
    seeds.retain(|s| seen.insert(format!("{s:?}")));
    MagicProgram { rules: out, seeds }
}
