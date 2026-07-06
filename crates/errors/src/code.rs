// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Interned, registry-checked finding codes.
//!
//! A [`Code`] is a one-word [`NonZeroU32`] handle into a [`CodeRegistry`]. The
//! registry is the enumeration authority: a code string must be *registered*
//! before it can be [`intern`](CodeRegistry::intern)ed, and interning an
//! unregistered string is a HARD FAIL — no silent default, no ad-hoc code. The
//! code space is **open**: any `&'static str` can be registered, so later work
//! (e.g. loss-ledger preservation rungs) adds codes without a closed enum to rip
//! out.
//!
//! The numeric value of a `Code` is an in-process handle only — it depends on
//! registration order and is **never serialized**. Everything that must be stable
//! across processes (fingerprints, IRIs) keys on the code *string*, exactly as
//! [`DiagRef`](crate::ledger) handles are never serialized either.

use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroU32;
use std::sync::{LazyLock, RwLock};

/// A one-word handle to a registered finding code. `Option<Code>` is also one
/// word thanks to the `NonZeroU32` niche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code(NonZeroU32);

/// The error returned when a code string is not present in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCode(pub String);

impl fmt::Display for UnknownCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unregistered finding code `{}`; every code must be declared in the registry before use",
            self.0
        )
    }
}

impl std::error::Error for UnknownCode {}

/// The enumeration authority for finding codes. Registration is idempotent and
/// append-only; codes are `&'static str` (the `pub const` catalog entries and the
/// string literals passed to [`define_diag_kind!`](crate::define_diag_kind)).
#[derive(Debug, Default)]
pub struct CodeRegistry {
    by_str: HashMap<&'static str, Code>,
    // Index `Code.0.get() - 1`. Owns the canonical `&'static` spelling.
    by_id: Vec<&'static str>,
}

impl CodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a code string, returning its handle. Idempotent: registering an
    /// already-known string returns the existing handle. Overflow of the
    /// `NonZeroU32` handle space is a HARD FAIL (panic), never a wrap.
    pub fn register(&mut self, code: &'static str) -> Code {
        if let Some(&existing) = self.by_str.get(code) {
            return existing;
        }
        let next = u32::try_from(self.by_id.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(NonZeroU32::new)
            .expect("code registry handle space exhausted");
        let handle = Code(next);
        self.by_id.push(code);
        self.by_str.insert(code, handle);
        handle
    }

    /// Seed a batch of codes (e.g. a slice's `ValidationRule` catalog).
    pub fn seed(&mut self, codes: &[&'static str]) {
        for &code in codes {
            self.register(code);
        }
    }

    /// Resolve a code string to its handle. HARD FAIL ([`UnknownCode`]) if the
    /// string was never registered.
    pub fn intern(&self, code: &str) -> Result<Code, UnknownCode> {
        self.by_str
            .get(code)
            .copied()
            .ok_or_else(|| UnknownCode(code.to_owned()))
    }

    /// Whether a code string is registered.
    pub fn contains(&self, code: &str) -> bool {
        self.by_str.contains_key(code)
    }

    /// The canonical `&'static` spelling of a handle.
    pub fn as_str(&self, code: Code) -> &'static str {
        self.by_id[code.0.get() as usize - 1]
    }

    /// Number of distinct codes registered.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// The process-wide registry. Codes register here at startup (slice catalogs via
/// [`register_code`]/[`seed_codes`], and [`define_diag_kind!`](crate::define_diag_kind)
/// types at first use); `diag!`-style emission interns against it at the exit
/// boundary.
static GLOBAL: LazyLock<RwLock<CodeRegistry>> = LazyLock::new(|| RwLock::new(CodeRegistry::new()));

/// Access the process-wide registry.
pub fn global_registry() -> &'static RwLock<CodeRegistry> {
    &GLOBAL
}

/// Register a code in the process-wide registry (idempotent).
pub fn register_code(code: &'static str) -> Code {
    global_registry()
        .write()
        .expect("code registry poisoned")
        .register(code)
}

/// Seed a batch of codes into the process-wide registry.
pub fn seed_codes(codes: &[&'static str]) {
    global_registry()
        .write()
        .expect("code registry poisoned")
        .seed(codes);
}

/// Resolve a code string against the process-wide registry. HARD FAIL if the
/// string was never registered.
pub fn intern_code(code: &str) -> Result<Code, UnknownCode> {
    global_registry()
        .read()
        .expect("code registry poisoned")
        .intern(code)
}

/// The canonical spelling of a handle from the process-wide registry.
pub fn code_str(code: Code) -> &'static str {
    global_registry()
        .read()
        .expect("code registry poisoned")
        .as_str(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_code_is_one_word() {
        assert_eq!(
            std::mem::size_of::<Option<Code>>(),
            std::mem::size_of::<u32>()
        );
    }

    #[test]
    fn register_then_intern_round_trips() {
        let mut reg = CodeRegistry::new();
        let c = reg.register("shacl.nonconforming");
        assert_eq!(reg.intern("shacl.nonconforming").unwrap(), c);
        assert_eq!(reg.as_str(c), "shacl.nonconforming");
    }

    #[test]
    fn register_is_idempotent() {
        let mut reg = CodeRegistry::new();
        let a = reg.register("discipline/stereotype");
        let b = reg.register("discipline/stereotype");
        assert_eq!(a, b);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn unregistered_code_is_a_hard_fail() {
        let reg = CodeRegistry::new();
        assert_eq!(
            reg.intern("bogus.unregistered"),
            Err(UnknownCode("bogus.unregistered".to_owned()))
        );
    }

    #[test]
    fn seed_registers_a_batch() {
        let mut reg = CodeRegistry::new();
        reg.seed(&["a.one", "b.two", "c.three"]);
        assert_eq!(reg.len(), 3);
        assert!(reg.contains("b.two"));
    }

    #[test]
    fn open_space_admits_arbitrary_static_codes() {
        // Phase-4 preservation rungs (or any new family) fit without a closed enum.
        let mut reg = CodeRegistry::new();
        let rung = reg.register("preservation.rung.section-retraction");
        assert_eq!(reg.as_str(rung), "preservation.rung.section-retraction");
    }

    #[test]
    fn global_registry_interns_after_registration() {
        // A code unique to this test so it cannot collide with other tests that
        // share the process-global registry.
        register_code("test.code.global-roundtrip");
        assert!(intern_code("test.code.global-roundtrip").is_ok());
        assert!(intern_code("test.code.never-registered-xyz").is_err());
    }
}
