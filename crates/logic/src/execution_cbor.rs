// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Internal primitives for the complete native execution receipt wire.
//! This is the selected receipt schema, not a general CBOR deserializer. Owners
//! implement their own records, sparse fields and variants beside private data.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Write};
use std::sync::Arc;

use purrdf_lex::cbor::head;

mod collections;
mod evidence;
mod records;
mod term;

pub(crate) const MAX_RECEIPT_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_RECEIPT_DEPTH: usize = 64;

/// A selected wire refusal, retained beneath the owning typed result diagnostic.
/// Integrators must keep that diagnostic kind as the direct downcastable source.
#[derive(Debug)]
pub(crate) struct Error {
    offset: usize,
    message: String,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl Error {
    pub(crate) fn invalid(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
            source: None,
        }
    }

    pub(crate) fn caused_by(
        offset: usize,
        message: &'static str,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self {
            offset,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native execution CBOR at byte {}: {}",
            self.offset, self.message
        )
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn StdError + 'static))
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Consume one typed visitor during the selected publication preflight.
/// This intentionally prevents structurally admitted but typed-unreadable output;
/// it does not replace the independent structural scan or the decoder's budget.
pub(crate) fn descend(remaining: usize) -> Result<usize> {
    remaining
        .checked_sub(1)
        .ok_or_else(|| Error::invalid(0, "typed receipt exceeds its recursion limit"))
}

/// One owner implements the complete existing wire without a temporary value tree.
/// Implementations must consume precisely one item and never normalize fields.
pub(crate) trait ExecutionCbor: Sized {
    /// Check the exact typed reader's visitor budget without allocating a value tree.
    fn check_depth(&self, remaining: usize) -> Result<()>;
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()>;
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self>;
}

/// A borrowed cursor with the old typed visitor budget, independently of encoding.
pub(crate) struct Reader<'a> {
    remaining: &'a [u8],
    length: usize,
    typed_depth: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(Error::invalid(0, "receipt exceeds its publication limit"));
        }
        Ok(Self {
            remaining: bytes,
            length: bytes.len(),
            typed_depth: 0,
        })
    }

    pub(crate) fn offset(&self) -> usize {
        self.length - self.remaining.len()
    }

    pub(crate) fn invalid(&self, message: impl Into<String>) -> Error {
        Error::invalid(self.offset(), message)
    }

    pub(crate) fn finish(self) -> Result<()> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(self.invalid("trailing receipt bytes"))
        }
    }

    /// Ciborium 0.2.2's recurse checks zero, decrements around the visitor and
    /// restores it even on Err. Option/Box/newtype visitors do not call recurse.
    fn visit<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.typed_depth >= MAX_RECEIPT_DEPTH {
            return Err(self.invalid("typed receipt exceeds its recursion limit"));
        }
        self.typed_depth += 1;
        let value = read(self);
        self.typed_depth -= 1;
        value
    }

    fn argument(&mut self, major: u8) -> Result<u64> {
        head::read_argument(&mut self.remaining, major)
            .ok_or_else(|| self.invalid("expected a complete definite receipt head"))
    }

    /// Reject impossible sizes before any owner allocates or iterates the length.
    /// Each item needs at least one byte; a map entry needs at least two.
    fn length(&mut self, major: u8, items_per_entry: u64) -> Result<usize> {
        let entries = self.argument(major)?;
        let items = entries
            .checked_mul(items_per_entry)
            .ok_or_else(|| self.invalid("receipt container size overflow"))?;
        if items > self.remaining.len() as u64 {
            return Err(self.invalid("receipt container exceeds its remaining bytes"));
        }
        usize::try_from(entries)
            .map_err(|error| Error::caused_by(self.offset(), "receipt length exceeds usize", error))
    }

    pub(crate) fn text(&mut self) -> Result<&'a str> {
        head::read_text(&mut self.remaining)
            .ok_or_else(|| self.invalid("expected complete UTF-8 receipt text"))
    }

    /// Selected records retain their emitted field order. Canonical replay remains
    /// mandatory at the outer boundary, including maps and sets built by readers.
    pub(crate) fn field(&mut self, expected: &'static str) -> Result<()> {
        if self.text()? == expected {
            Ok(())
        } else {
            Err(self.invalid(format!("expected receipt field {expected}")))
        }
    }

    pub(crate) fn array<T>(
        &mut self,
        read: impl FnOnce(&mut Self, usize) -> Result<T>,
    ) -> Result<T> {
        self.visit(|input| {
            let length = input.length(head::ARRAY, 1)?;
            read(input, length)
        })
    }

    pub(crate) fn map<T>(&mut self, read: impl FnOnce(&mut Self, usize) -> Result<T>) -> Result<T> {
        self.visit(|input| {
            let length = input.length(head::MAP, 2)?;
            read(input, length)
        })
    }

    /// Ciborium's deserialize_enum recurses for BOTH a text unit variant and a
    /// one-entry externally tagged map. Its map wrapper is that same visitor,
    /// not a second map visitor. Struct/tuple payloads then enter their own map
    /// or sequence visitor. Owners must match the expected tagged/unit form.
    pub(crate) fn variant<T>(
        &mut self,
        read: impl FnOnce(&mut Self, &'a str, bool) -> Result<T>,
    ) -> Result<T> {
        self.visit(|input| {
            let major = input.remaining.first().map(|byte| byte >> 5);
            let tagged = match major {
                Some(head::TEXT) => false,
                Some(head::MAP) => {
                    if input.length(head::MAP, 2)? != 1 {
                        return Err(input.invalid("expected one tagged receipt variant"));
                    }
                    true
                }
                _ => return Err(input.invalid("expected a text or tagged receipt variant")),
            };
            let name = input.text()?;
            read(input, name, tagged)
        })
    }

    fn unsigned(&mut self) -> Result<u64> {
        self.argument(head::UNSIGNED)
    }

    fn scalar(&mut self, byte: u8) -> bool {
        if self.remaining.first() == Some(&byte) {
            self.remaining = &self.remaining[1..];
            true
        } else {
            false
        }
    }
}

impl ExecutionCbor for String {
    /// Scalars do not enter a typed deserialization visitor.
    fn check_depth(&self, _remaining: usize) -> Result<()> {
        Ok(())
    }
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(out, self)
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.text().map(str::to_owned)
    }
}

impl ExecutionCbor for bool {
    /// Scalars do not enter a typed deserialization visitor.
    fn check_depth(&self, _remaining: usize) -> Result<()> {
        Ok(())
    }
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::SIMPLE, if *self { 21 } else { 20 })
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        if input.scalar(0xf4) {
            Ok(false)
        } else if input.scalar(0xf5) {
            Ok(true)
        } else {
            Err(input.invalid("expected a receipt boolean"))
        }
    }
}

macro_rules! unsigned {
    ($($type:ty),* $(,)?) => {$(
        impl ExecutionCbor for $type {
            /// Scalars do not enter a typed deserialization visitor.
            fn check_depth(&self, _remaining: usize) -> Result<()> { Ok(()) }
            fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
                head::write_head(out, head::UNSIGNED, *self as u64)
            }
            fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
                Self::try_from(input.unsigned()?)
                    .map_err(|error| Error::caused_by(input.offset(), "receipt integer exceeds its target width", error))
            }
        }
    )*};
}
unsigned!(u8, u32, usize);

impl ExecutionCbor for u64 {
    fn check_depth(&self, _remaining: usize) -> Result<()> {
        Ok(())
    }

    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::UNSIGNED, *self)
    }

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.unsigned()
    }
}

impl ExecutionCbor for u128 {
    /// Scalars do not enter a typed deserialization visitor.
    fn check_depth(&self, _remaining: usize) -> Result<()> {
        Ok(())
    }
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        if let Ok(value) = u64::try_from(*self) {
            return head::write_head(out, head::UNSIGNED, value);
        }
        let bytes = self.to_be_bytes();
        let start = self.leading_zeros() as usize / 8;
        head::write_head(out, head::TAG, 2)?;
        head::write_bytes(out, &bytes[start..])
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        if input
            .remaining
            .first()
            .is_some_and(|byte| byte >> 5 == head::UNSIGNED)
        {
            return input.unsigned().map(u128::from);
        }
        // Ciborium's integer() consumes tag 2 and its byte string without recurse.
        if input.argument(head::TAG)? != 2 {
            return Err(input.invalid("expected the selected positive bignum tag"));
        }
        let bytes = head::read_bytes(&mut input.remaining)
            .ok_or_else(|| input.invalid("expected complete receipt bignum bytes"))?;
        if !(9..=16).contains(&bytes.len()) || bytes.first() == Some(&0) {
            return Err(input.invalid("receipt bignum is outside its emitted u128 form"));
        }
        Ok(bytes
            .iter()
            .fold(0_u128, |value, byte| (value << 8) | u128::from(*byte)))
    }
}

impl<T: ExecutionCbor> ExecutionCbor for Option<T> {
    /// Option is transparent to Ciborium's typed visitor counter.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        match self {
            Some(value) => value.check_depth(remaining),
            None => Ok(()),
        }
    }
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Some(value) => value.write_cbor(out),
            None => head::write_head(out, head::SIMPLE, 22),
        }
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        if input.scalar(0xf6) {
            Ok(None)
        } else {
            T::read_cbor(input).map(Some)
        }
    }
}

macro_rules! transparent {
    ($($type:ident),* $(,)?) => {$(
        impl<T: ExecutionCbor> ExecutionCbor for $type<T> {

            /// Box and Arc are transparent, including recursive execution evidence.
            fn check_depth(&self, remaining: usize) -> Result<()> {
                self.as_ref().check_depth(remaining)
            }
            fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> { self.as_ref().write_cbor(out) }
            fn read_cbor(input: &mut Reader<'_>) -> Result<Self> { T::read_cbor(input).map(Self::new) }
        }
    )*};
}
transparent!(Box, Arc);

impl<K: ExecutionCbor + Ord, V: ExecutionCbor> ExecutionCbor for BTreeMap<K, V> {
    /// The map visitor stays open while each key and value is decoded.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        for (key, value) in self {
            key.check_depth(remaining)?;
            value.check_depth(remaining)?;
        }
        Ok(())
    }
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, self.len() as u64)?;
        for (key, value) in self {
            key.write_cbor(out)?;
            value.write_cbor(out)?;
        }
        Ok(())
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            let mut values = BTreeMap::new();
            for _ in 0..length {
                let key = K::read_cbor(input)?;
                if values.contains_key(&key) {
                    return Err(input.invalid("duplicate receipt map key"));
                }
                values.insert(key, V::read_cbor(input)?);
            }
            Ok(values)
        })
    }
}

// Retained digest arrays use an integer sequence, never a CBOR byte string.
impl<const N: usize> ExecutionCbor for [u8; N] {
    /// Fixed integer arrays enter one sequence; their u8 items add no visitor.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        descend(remaining).map(|_| ())
    }
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::ARRAY, N as u64)?;
        for value in self {
            value.write_cbor(out)?;
        }
        Ok(())
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.array(|input, length| {
            if length != N {
                return Err(input.invalid("wrong fixed receipt array length"));
            }
            let mut values = [0; N];
            for value in &mut values {
                *value = u8::read_cbor(input)?;
            }
            Ok(values)
        })
    }
}

macro_rules! tuples {
    ($(($length:expr; $( $type:ident : $index:tt ),+)),* $(,)?) => {$(
        impl<$( $type: ExecutionCbor ),+> ExecutionCbor for ($( $type, )+) {

            /// A tuple is one sequence whose fields share its remaining budget.
            fn check_depth(&self, remaining: usize) -> Result<()> {
                let remaining = descend(remaining)?;
                $(self.$index.check_depth(remaining)?;)+
                Ok(())
            }
            fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
                head::write_head(out, head::ARRAY, $length)?;
                $(self.$index.write_cbor(out)?;)+
                Ok(())
            }
            fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
                input.array(|input, length| {
                    if length != $length as usize { return Err(input.invalid("wrong receipt tuple length")); }
                    Ok(($( $type::read_cbor(input)?, )+))
                })
            }
        }
    )*};
}
tuples!((2; A:0, B:1), (3; A:0, B:1, C:2));

/// The old encoder's IgnoredAny pass has a STRUCTURAL budget of 64 open maps or
/// arrays. Unlike typed decoding, a text enum name adds no depth; a recognized
/// positive bignum is a scalar and its tag adds no depth either. Keep this pass
/// separate: at the boundary the old encoder can emit a directional literal
/// whose extra typed enum visitor makes the old typed decoder refuse it.
/// This walks only the closed writer's output, without allocating a value tree
/// or implementing unsupported float, indefinite or foreign-tag mechanisms.
pub(crate) fn check_structure(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_RECEIPT_BYTES {
        return Err(Error::invalid(0, "receipt exceeds its publication limit"));
    }
    let mut input = bytes;
    // Index zero is a synthetic one-item frame, not a CBOR container.
    let mut remaining = [0_u64; MAX_RECEIPT_DEPTH + 1];
    remaining[0] = 1;
    let mut depth = 0;
    loop {
        if remaining[depth] == 0 {
            if depth == 0 {
                break;
            }
            depth -= 1;
            continue;
        }
        remaining[depth] -= 1;
        let offset = bytes.len() - input.len();
        let invalid = |message| Error::invalid(offset, message);
        let major = input
            .first()
            .map(|byte| byte >> 5)
            .ok_or_else(|| invalid("truncated receipt structure"))?;
        match major {
            head::UNSIGNED => {
                head::read_argument(&mut input, major)
                    .ok_or_else(|| invalid("invalid receipt integer"))?;
            }
            head::BYTES => {
                head::read_bytes(&mut input).ok_or_else(|| invalid("invalid receipt bytes"))?;
            }
            head::TEXT => {
                head::read_text(&mut input).ok_or_else(|| invalid("invalid receipt text"))?;
            }
            head::ARRAY | head::MAP => {
                if depth >= MAX_RECEIPT_DEPTH {
                    return Err(invalid("receipt exceeds its structural bound"));
                }
                let count = head::read_argument(&mut input, major)
                    .ok_or_else(|| invalid("invalid receipt container"))?;
                let count = count
                    .checked_mul(if major == head::MAP { 2 } else { 1 })
                    .ok_or_else(|| invalid("receipt container size overflow"))?;
                if count > input.len() as u64 {
                    return Err(invalid("receipt container exceeds its remaining bytes"));
                }
                if count != 0 {
                    depth += 1;
                    remaining[depth] = count;
                }
            }
            head::TAG => {
                if head::read_argument(&mut input, head::TAG) != Some(2) {
                    return Err(invalid("foreign receipt tag"));
                }
                let value = head::read_bytes(&mut input)
                    .ok_or_else(|| invalid("invalid receipt bignum"))?;
                if value.len() > 16 {
                    return Err(invalid("receipt bignum exceeds u128"));
                }
            }
            head::SIMPLE if matches!(input.first(), Some(0xf4..=0xf6)) => {
                input = &input[1..];
            }
            _ => return Err(invalid("unsupported kind in emitted receipt structure")),
        }
    }
    if input.is_empty() {
        Ok(())
    } else {
        Err(Error::invalid(
            bytes.len() - input.len(),
            "trailing receipt structure",
        ))
    }
}
