// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared identifier / text helpers for the developer-surface schema emitters.
//!
//! These are the deterministic building blocks BOTH the LinkML/TypeScript/GraphQL
//! renderer ([`crate::stages::schemas`]) and the SHACL-derived Pydantic package
//! emitter ([`crate::stages::pydantic`]) sanitize identifiers, quote Python
//! strings, order classes parent-before-child, and finish text buffers with.
//! Lifting them here keeps ONE copy of each rule so the two surfaces can never
//! drift (e.g. a reserved-word escape fixed in one place fixes both).

use std::collections::{BTreeMap, BTreeSet};

/// The bare local name of an IRI: the substring after the last `#` or `/`.
pub(crate) fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

/// Normalize a trailing-newline text buffer: collapse trailing blank lines to a
/// single terminating newline, and guarantee exactly one trailing newline.
pub(crate) fn finish_text(mut out: String) -> Vec<u8> {
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.into_bytes()
}

/// Quote a string as a Python string literal (Rust `Debug` produces a
/// double-quoted, backslash-escaped form that is valid Python).
pub(crate) fn py_string(s: &str) -> String {
    format!("{s:?}")
}

/// Sanitize an arbitrary token into a valid Python identifier, escaping the
/// small set of reserved words the surfaces can collide with. `fallback` is
/// used when the token reduces to nothing.
pub(crate) fn sanitize_identifier(raw: &str, fallback: &str) -> String {
    let mut out = String::new();
    for (i, ch) in raw.chars().enumerate() {
        let valid = ch == '_' || ch.is_ascii_alphanumeric();
        if valid {
            if i == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out = out.trim_matches('_').to_string();
    if out.is_empty() {
        fallback.to_string()
    } else if matches!(
        out.as_str(),
        "class" | "def" | "enum" | "from" | "import" | "None" | "True" | "False" | "type"
    ) {
        format!("{out}_")
    } else {
        out
    }
}

/// Sanitize an arbitrary token into a valid, upper-camel Python type name.
pub(crate) fn sanitize_type(raw: &str, fallback: &str) -> String {
    let ident = sanitize_identifier(raw, fallback);
    let mut chars = ident.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(ident.len());
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
            out
        }
        None => fallback.to_string(),
    }
}

/// Cycle-safe topological order (parent before child) over a class set.
///
/// `parents` maps every class name to `Some(parent)` when the class extends
/// another class in the SAME set, or `None` when it has no in-set parent. The
/// caller is responsible for only recording an in-set parent (a `None` for a
/// parent that is not a key), so this pass never needs to bounds-check. A
/// `temporary`/`permanent` marking makes a stray cycle terminate rather than
/// recurse forever.
pub(crate) fn class_render_order(parents: &BTreeMap<String, Option<String>>) -> Vec<String> {
    fn visit(
        name: &str,
        parents: &BTreeMap<String, Option<String>>,
        temporary: &mut BTreeSet<String>,
        permanent: &mut BTreeSet<String>,
        out: &mut Vec<String>,
    ) {
        if permanent.contains(name) || temporary.contains(name) {
            return;
        }
        temporary.insert(name.to_string());
        if let Some(Some(parent)) = parents.get(name)
            && parents.contains_key(parent)
        {
            visit(parent, parents, temporary, permanent, out);
        }
        temporary.remove(name);
        permanent.insert(name.to_string());
        out.push(name.to_string());
    }

    let mut out = Vec::with_capacity(parents.len());
    let mut temporary = BTreeSet::new();
    let mut permanent = BTreeSet::new();
    for name in parents.keys() {
        visit(name, parents, &mut temporary, &mut permanent, &mut out);
    }
    out
}
