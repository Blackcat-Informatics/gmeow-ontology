// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The content-key fold for the hash-consed term DAG.
//!
//! # What a content key is
//!
//! A [`NodeData`]'s content key is a pure structural fold: a `String` that is the
//! persistent identity of the node.  Two nodes hash-cons to one [`NodeId`] exactly when
//! their content keys are byte-equal, so the key must be **injective** — structurally
//! distinct nodes MUST get distinct keys.  Bound occurrences are locally-nameless
//! de-Bruijn refs, so alpha-equivalent terms already share structure and thus share a
//! key; there is no name in the key to normalize.
//!
//! # Why length-prefixed (netstring) encoding, not bare separators
//!
//! [`crate::logic-compile`](gmeow_logic_compile)'s `ir::Formula::content_key` folds with
//! a bare `SEP='\u{0}'` separator, which is safe there because its leaves are already
//! sanitized IR tokens.  This DAG interns *arbitrary* IRIs and literal lexical forms as
//! leaves — bytes that can contain any separator, mimic a kind tag, or embed a NUL.  A
//! bare-separator scheme could then conflate two structurally-distinct terms.  So every
//! child fragment is emitted as a **netstring** `"{len}:{s}"` (byte length, colon,
//! bytes), exactly the injective length-prefixing that
//! [`crate::provenance::mint_nary_reifier`] deliberately chose: because a reader consumes
//! exactly `len` bytes, the fragment's content can never bleed into the framing, so the
//! encoding is injective in every child regardless of its bytes.  The DISCIPLINE (kind-tag
//! letters, de-Bruijn bound tokens) mirrors `ir.rs`; only the child framing differs.
//!
//! # Encoding
//!
//! Per node kind (`net(s) = "{s.len()}:{s}"`):
//!
//! - `Leaf(id)`             → `I`   `net(term_display(atom))`
//! - `Free(id)`             → `V`   `net("free_" + term_display(atom))`
//! - `Meta(m)`              → `M`   `net(m.index())`               (metavars are identity-bearing)
//! - `Bound{debruijn,slot}` → `B`   `net(debruijn)` `net(slot)`    (locally-nameless de-Bruijn)
//! - `App{op,args}`         → `APP` `net(key(op))` `net(count)` `net(key(argᵢ))…`
//! - `Binder{op,sorts,body}`→ `BIND``net(key(op))` `net(sorts.len)` `net(key(sortᵢ))…` `net(key(body))`
//!
//! The kind tags never collide: the leading byte partitions the kinds (`I`/`V`/`M`/`B`
//! vs `A` for `APP`), and the only shared leading byte — `B` — is disambiguated one byte
//! deeper (`Bound`'s next byte is a decimal digit from `net(debruijn)`; `Binder`'s is the
//! `I` of `BIND`).

use crate::physical::term_dag::{NodeData, TermDag};

/// Frame one fragment as an injective netstring `"{len}:{s}"` (byte length, colon, bytes).
///
/// Length-prefixed so a reader consumes exactly `len` bytes: the fragment's content can
/// never be confused with the surrounding framing, whatever bytes it holds.  Appended in
/// place to avoid a per-child temporary allocation.
#[inline]
fn push_netstring(out: &mut String, s: &str) {
    use std::fmt::Write as _;
    // `len` is the BYTE length (`str::len`), so multibyte content stays injective.
    let _ = write!(out, "{}:", s.len());
    out.push_str(s);
}

/// Frame a numeric ordinal (a de-Bruijn distance, slot, arity, or metavar index) as a
/// netstring over its decimal render — the injective encoding of an identity-bearing
/// number.
#[inline]
fn push_netstring_num(out: &mut String, n: usize) {
    let decimal = n.to_string();
    push_netstring(out, &decimal);
}

/// The content key for `data`, given a [`TermDag`] in which every child of `data` is
/// already interned (so each child's cached key and each atom's display resolve).
///
/// A pure `O(children)` fold: it reads children's cached keys (never re-folds a subtree)
/// and the leaf atoms' cached displays.  It never inspects the node being built, so it is
/// safe to call before the node is pushed.
pub(crate) fn content_key(dag: &TermDag, data: &NodeData) -> String {
    let mut out = String::new();
    match data {
        NodeData::Leaf(atom) => {
            out.push('I');
            push_netstring(&mut out, dag.atom_display(*atom));
        }
        NodeData::Free(atom) => {
            out.push('V');
            // The `free_` prefix keeps a free variable named `x` distinct from a leaf
            // IRI/literal whose display is `x`, mirroring `ir.rs`'s `free_<name>` token.
            let mut framed = String::with_capacity(5 + dag.atom_display(*atom).len());
            framed.push_str("free_");
            framed.push_str(dag.atom_display(*atom));
            push_netstring(&mut out, &framed);
        }
        NodeData::Meta(m) => {
            out.push('M');
            push_netstring_num(&mut out, m.index());
        }
        NodeData::Bound { debruijn, slot } => {
            out.push('B');
            push_netstring_num(&mut out, *debruijn as usize);
            push_netstring_num(&mut out, *slot as usize);
        }
        NodeData::App { op, args } => {
            out.push_str("APP");
            push_netstring(&mut out, dag.key(*op));
            push_netstring_num(&mut out, args.len());
            for arg in args.iter() {
                push_netstring(&mut out, dag.key(*arg));
            }
        }
        NodeData::Binder { op, sorts, body } => {
            out.push_str("BIND");
            push_netstring(&mut out, dag.key(*op));
            push_netstring_num(&mut out, sorts.len());
            for sort in sorts.iter() {
                push_netstring(&mut out, dag.key(*sort));
            }
            push_netstring(&mut out, dag.key(*body));
        }
    }
    out
}
