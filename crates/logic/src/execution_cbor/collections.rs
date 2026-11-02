// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared sequence emission and visitor accounting with distinct collection admission.

use std::collections::BTreeSet;
use std::io;

use purrdf_lex::cbor::head;

use super::{ExecutionCbor, Reader, Result, descend};

// Both owners expose the same borrowed sequence wire and typed visitor count.
// Admission stays in each impl: vectors retain duplicates, while sets refuse them.
macro_rules! sequence_methods {
    () => {
        /// The sequence enters one visitor even when it has no elements.
        fn check_depth(&self, remaining: usize) -> Result<()> {
            let remaining = descend(remaining)?;
            for value in self {
                value.check_depth(remaining)?;
            }
            Ok(())
        }

        /// Stream recorded element order without an intermediate collection.
        fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
            &self,
            out: &mut W,
        ) -> io::Result<()> {
            head::write_head(out, head::ARRAY, self.len() as u64)?;
            for value in self {
                value.write_cbor(out)?;
            }
            Ok(())
        }
    };
}

impl<T: ExecutionCbor> ExecutionCbor for Vec<T> {
    sequence_methods!();

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.array(|input, length| {
            let mut values = Vec::new();
            for _ in 0..length {
                values.push(T::read_cbor(input)?);
            }
            Ok(values)
        })
    }
}

impl<T: ExecutionCbor + Ord> ExecutionCbor for BTreeSet<T> {
    sequence_methods!();

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.array(|input, length| {
            let mut values = BTreeSet::new();
            for _ in 0..length {
                if !values.insert(T::read_cbor(input)?) {
                    return Err(input.invalid("duplicate receipt set member"));
                }
            }
            Ok(values)
        })
    }
}
