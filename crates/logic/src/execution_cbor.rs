// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Internal primitives for the complete native execution receipt wire.
//! This is the selected receipt schema, not a general CBOR deserializer. Owners
//! implement their own records, sparse fields and variants beside private data.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::TermValue;
use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Write};

use purrdf_lex::cbor::head;

mod collections;
mod evidence;
mod records;
mod term;

pub(crate) const MAX_RECEIPT_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_RECEIPT_DEPTH: usize = 64;
/// V3 admission counts inline table definitions plus the logically expanded payload.
/// This is an encoding-work bound, not a measured process-memory bound.
pub(crate) const MAX_EXPANDED_RECEIPT_BYTES: usize = 512 * 1024 * 1024;

/// Typed leaf operations for the selected receipt wire; static record keys stay inline.
pub(crate) trait LeafWrite: Write {
    fn string(&mut self, value: &str) -> io::Result<()>;
    fn term(&mut self, value: &TermValue) -> io::Result<()>;
}

/// Explicit inline strategy used for table definitions and expanded diagnostic sizes.
pub(crate) struct InlineWriter<'a, W: Write + ?Sized>(pub(crate) &'a mut W);
impl<W: Write + ?Sized> Write for InlineWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl<W: Write + ?Sized> LeafWrite for InlineWriter<'_, W> {
    fn string(&mut self, value: &str) -> io::Result<()> {
        head::write_text(self.0, value)
    }
    fn term(&mut self, value: &TermValue) -> io::Result<()> {
        crate::term_serde::write_cbor(value, self.0)
    }
}

#[derive(Default)]
struct EncodingSize(usize);
impl Write for EncodingSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("native receipt size overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn text_size(value: &str) -> Result<usize> {
    head::head_len(value.len() as u64)
        .checked_add(value.len())
        .ok_or_else(|| Error::invalid(0, "native receipt text size overflow"))
}
fn term_size(value: &TermValue) -> Result<usize> {
    let mut size = EncodingSize::default();
    crate::term_serde::write_cbor(value, &mut size)
        .map_err(|error| Error::caused_by(0, "native receipt term size failed", error))?;
    Ok(size.0)
}

/// The same cumulative expansion admission is used before production and hydration.
#[derive(Default)]
pub(crate) struct Expansion {
    used: usize,
}
impl Expansion {
    fn charge(&mut self, bytes: usize) -> Result<()> {
        let used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| Error::invalid(0, "native receipt expansion size overflow"))?;
        if used > MAX_EXPANDED_RECEIPT_BYTES {
            return Err(Error::invalid(
                0,
                "native receipt exceeds its 512 MiB expanded logical-encoding budget",
            ));
        }
        self.used = used;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Leaf {
    index: usize,
    bytes: usize,
}
/// First-use table order follows the existing deterministic typed record traversal.
#[derive(Default)]
pub(crate) struct Tables {
    strings: BTreeMap<String, Leaf>,
    terms: BTreeMap<TermValue, Leaf>,
    definition_bytes: usize,
    expanded_bytes: usize,
}
impl Tables {
    pub(crate) fn prepare(value: &impl ExecutionCbor, schema: &str) -> Result<Self> {
        let mut tables = Self::default();
        let mut expansion = Expansion::default();
        let mut compact_payload = EncodingSize::default();
        value
            .write_cbor(&mut Encoder {
                output: &mut compact_payload,
                tables: WriteTables::Collect(&mut tables),
                expansion: &mut expansion,
            })
            .map_err(|error| Error::caused_by(0, "native receipt table planning failed", error))?;
        let headers = head::head_len(4)
            .checked_add(text_size(schema)?)
            .and_then(|n| n.checked_add(head::head_len(tables.strings.len() as u64)))
            .and_then(|n| n.checked_add(head::head_len(tables.terms.len() as u64)))
            .ok_or_else(|| Error::invalid(0, "native receipt table header size overflow"))?;
        expansion.charge(headers)?;
        tables.definition_bytes = tables
            .definition_bytes
            .checked_add(headers)
            .ok_or_else(|| Error::invalid(0, "native receipt table definition size overflow"))?;
        tables.expanded_bytes = expansion.used;
        let compact_bytes = tables
            .definition_bytes
            .checked_add(compact_payload.0)
            .ok_or_else(|| Error::invalid(0, "native receipt compact size overflow"))?;
        if compact_bytes > MAX_RECEIPT_BYTES {
            return Err(Error::invalid(
                0,
                format!(
                    "native receipt compact encoding requires {compact_bytes} bytes, exceeding its 256 MiB publication bound; expanded logical encoding requires {} bytes",
                    tables.expanded_bytes
                ),
            ));
        }
        Ok(tables)
    }
    fn add_definition(&mut self, bytes: usize) -> io::Result<()> {
        let total = self
            .definition_bytes
            .checked_add(bytes)
            .ok_or_else(|| io::Error::other("native receipt table size overflow"))?;
        if total > MAX_RECEIPT_BYTES {
            return Err(io::Error::other(
                "native receipt tables exceed the 256 MiB publication bound",
            ));
        }
        self.definition_bytes = total;
        Ok(())
    }
    pub(crate) fn write<W: Write + ?Sized>(&self, output: &mut W) -> io::Result<()> {
        head::write_head(output, head::ARRAY, self.strings.len() as u64)?;
        let mut strings: Vec<_> = self.strings.iter().collect();
        strings.sort_by_key(|(_, leaf)| leaf.index);
        for (value, _) in strings {
            head::write_text(output, value)?;
        }
        head::write_head(output, head::ARRAY, self.terms.len() as u64)?;
        let mut terms: Vec<_> = self.terms.iter().collect();
        terms.sort_by_key(|(_, leaf)| leaf.index);
        for (value, _) in terms {
            crate::term_serde::write_cbor(value, output)?;
        }
        Ok(())
    }
    pub(crate) fn write_payload<W: Write + ?Sized>(
        &self,
        value: &impl ExecutionCbor,
        output: &mut W,
    ) -> io::Result<()> {
        let mut expansion = Expansion::default();
        expansion
            .charge(self.definition_bytes)
            .map_err(io::Error::other)?;
        value.write_cbor(&mut Encoder {
            output,
            tables: WriteTables::Compact(self),
            expansion: &mut expansion,
        })?;
        if expansion.used != self.expanded_bytes {
            return Err(io::Error::other(
                "native receipt changed after table planning",
            ));
        }
        Ok(())
    }
}

enum WriteTables<'a> {
    Collect(&'a mut Tables),
    Compact(&'a Tables),
}
struct Encoder<'a, W: Write + ?Sized> {
    output: &'a mut W,
    tables: WriteTables<'a>,
    expansion: &'a mut Expansion,
}
impl<W: Write + ?Sized> Write for Encoder<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.expansion
            .charge(bytes.len())
            .map_err(io::Error::other)?;
        self.output.write_all(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
impl<W: Write + ?Sized> LeafWrite for Encoder<'_, W> {
    fn string(&mut self, value: &str) -> io::Result<()> {
        let leaf = match &mut self.tables {
            WriteTables::Collect(tables) => {
                if let Some(leaf) = tables.strings.get(value) {
                    *leaf
                } else {
                    let bytes = text_size(value).map_err(io::Error::other)?;
                    // Charge the new owned table materialization before copying.
                    self.expansion.charge(bytes).map_err(io::Error::other)?;
                    tables.add_definition(bytes)?;
                    let leaf = Leaf {
                        index: tables.strings.len(),
                        bytes,
                    };
                    tables.strings.insert(value.to_owned(), leaf);
                    leaf
                }
            }
            WriteTables::Compact(tables) => *tables.strings.get(value).ok_or_else(|| {
                io::Error::other("native receipt string absent from selected table")
            })?,
        };
        self.expansion
            .charge(leaf.bytes)
            .map_err(io::Error::other)?;
        head::write_head(self.output, head::UNSIGNED, leaf.index as u64)
    }
    fn term(&mut self, value: &TermValue) -> io::Result<()> {
        let leaf = match &mut self.tables {
            WriteTables::Collect(tables) => {
                if let Some(leaf) = tables.terms.get(value) {
                    *leaf
                } else {
                    let bytes = term_size(value).map_err(io::Error::other)?;
                    self.expansion.charge(bytes).map_err(io::Error::other)?;
                    tables.add_definition(bytes)?;
                    let leaf = Leaf {
                        index: tables.terms.len(),
                        bytes,
                    };
                    tables.terms.insert(value.clone(), leaf);
                    leaf
                }
            }
            WriteTables::Compact(tables) => *tables.terms.get(value).ok_or_else(|| {
                io::Error::other("native receipt term absent from selected table")
            })?,
        };
        self.expansion
            .charge(leaf.bytes)
            .map_err(io::Error::other)?;
        head::write_head(self.output, head::UNSIGNED, leaf.index as u64)
    }
}

struct ReadTerm {
    value: Arc<TermValue>,
    bytes: usize,
    depth: usize,
}
struct ReadTables<'a> {
    strings: Vec<&'a str>,
    terms: Vec<ReadTerm>,
    next_string: usize,
    next_term: usize,
    used_strings: BTreeSet<usize>,
    used_terms: BTreeSet<usize>,
}
enum ReadLeaves<'a> {
    Definitions,
    Compact(ReadTables<'a>),
}

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
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()>;
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self>;
}

/// A borrowed cursor with the old typed visitor budget, independently of encoding.
pub(crate) struct Reader<'a> {
    remaining: &'a [u8],
    length: usize,
    typed_depth: usize,
    expansion: Expansion,
    leaves: ReadLeaves<'a>,
}

impl<'a> Reader<'a> {
    pub(crate) fn for_definitions(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(Error::invalid(0, "receipt exceeds its publication limit"));
        }
        Ok(Self {
            remaining: bytes,
            length: bytes.len(),
            typed_depth: 0,
            expansion: Expansion::default(),
            leaves: ReadLeaves::Definitions,
        })
    }

    /// Table definitions use inline retained terms; payload leaves require indexes.
    pub(crate) fn definitions(&self) -> bool {
        matches!(self.leaves, ReadLeaves::Definitions)
    }

    pub(crate) fn read_tables(&mut self) -> Result<()> {
        if !self.definitions() {
            return Err(self.invalid("native receipt tables already selected"));
        }
        let strings = self.array(|input, count| {
            let mut strings = Vec::new();
            let mut seen = BTreeSet::new();
            for _ in 0..count {
                // Raw text is charged before exposing it; its bytes borrow the wire.
                let text = input.text()?;
                if !seen.insert(text) {
                    return Err(input.invalid("duplicate native receipt string table entry"));
                }
                strings.push(text);
            }
            Ok(strings)
        })?;
        let terms = self.array(|input, count| {
            let mut terms = Vec::new();
            let mut seen = BTreeSet::new();
            for _ in 0..count {
                // The definition strategy charges raw fields before each owned copy
                // and every recursive retained-term hydration.
                let value = TermValue::read_cbor(input)?;
                let bytes = term_size(&value)?;
                let depth = term::required_depth(&value, MAX_RECEIPT_DEPTH)?;
                let value = Arc::new(value);
                if !seen.insert(Arc::clone(&value)) {
                    return Err(input.invalid("duplicate native receipt term table entry"));
                }
                terms.push(ReadTerm {
                    value,
                    bytes,
                    depth,
                });
            }
            Ok(terms)
        })?;
        self.leaves = ReadLeaves::Compact(ReadTables {
            strings,
            terms,
            next_string: 0,
            next_term: 0,
            used_strings: BTreeSet::new(),
            used_terms: BTreeSet::new(),
        });
        Ok(())
    }

    /// A wire-local index is replaced by its complete logical leaf cost, not added
    /// to that cost. Static heads, keys and variants are charged by ordinary reads.
    fn reference(&mut self) -> Result<usize> {
        let index = head::read_argument(&mut self.remaining, head::UNSIGNED)
            .ok_or_else(|| self.invalid("expected native receipt table index"))?;
        usize::try_from(index).map_err(|error| {
            Error::caused_by(self.offset(), "receipt table index exceeds usize", error)
        })
    }

    pub(crate) fn string_value(&mut self) -> Result<String> {
        if self.definitions() {
            return self.text().map(str::to_owned);
        }
        let index = self.reference()?;
        let ReadLeaves::Compact(tables) = &self.leaves else {
            unreachable!("selected table strategy")
        };
        let value = tables
            .strings
            .get(index)
            .ok_or_else(|| self.invalid("native receipt string index is outside its table"))?;
        let bytes = text_size(value)?;
        self.expansion.charge(bytes)?;
        let ReadLeaves::Compact(tables) = &mut self.leaves else {
            unreachable!("selected table strategy")
        };
        if !tables.used_strings.contains(&index) {
            if index != tables.next_string {
                return Err(Error::invalid(
                    self.length - self.remaining.len(),
                    "noncanonical native receipt string table order",
                ));
            }
            tables.next_string += 1;
            tables.used_strings.insert(index);
        }
        Ok(tables.strings[index].to_owned())
    }

    pub(crate) fn term_value(&mut self) -> Result<TermValue> {
        let index = self.reference()?;
        let ReadLeaves::Compact(tables) = &self.leaves else {
            return Err(self.invalid("native receipt compact term strategy is not selected"));
        };
        let value = tables
            .terms
            .get(index)
            .ok_or_else(|| self.invalid("native receipt term index is outside its table"))?;
        if value.depth > MAX_RECEIPT_DEPTH - self.typed_depth {
            return Err(self.invalid("typed receipt term exceeds its occurrence recursion limit"));
        }
        self.expansion.charge(value.bytes)?;
        let ReadLeaves::Compact(tables) = &mut self.leaves else {
            unreachable!("selected table strategy")
        };
        if !tables.used_terms.contains(&index) {
            if index != tables.next_term {
                return Err(Error::invalid(
                    self.length - self.remaining.len(),
                    "noncanonical native receipt term table order",
                ));
            }
            tables.next_term += 1;
            tables.used_terms.insert(index);
        }
        // Charge the complete recursively expanded value once before this clone;
        // its leaves are not read or charged a second time.
        Ok(tables.terms[index].value.as_ref().clone())
    }

    fn bytes(&mut self) -> Result<&'a [u8]> {
        let before = self.remaining.len();
        let value = head::read_bytes(&mut self.remaining)
            .ok_or_else(|| self.invalid("expected complete native receipt byte string"))?;
        self.expansion.charge(before - self.remaining.len())?;
        Ok(value)
    }

    pub(crate) fn offset(&self) -> usize {
        self.length - self.remaining.len()
    }

    pub(crate) fn invalid(&self, message: impl Into<String>) -> Error {
        Error::invalid(self.offset(), message)
    }

    pub(crate) fn finish(self) -> Result<()> {
        if let ReadLeaves::Compact(tables) = &self.leaves
            && (tables.used_strings.len() != tables.strings.len()
                || tables.used_terms.len() != tables.terms.len())
        {
            return Err(self.invalid("unused native receipt table entry"));
        }
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
        let before = self.remaining.len();
        let value = head::read_argument(&mut self.remaining, major)
            .ok_or_else(|| self.invalid("expected a complete definite receipt head"))?;
        self.expansion.charge(before - self.remaining.len())?;
        Ok(value)
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
        let before = self.remaining.len();
        let value = head::read_text(&mut self.remaining)
            .ok_or_else(|| self.invalid("expected complete UTF-8 receipt text"))?;
        self.expansion.charge(before - self.remaining.len())?;
        Ok(value)
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

    fn scalar(&mut self, byte: u8) -> Result<bool> {
        if self.remaining.first() == Some(&byte) {
            self.expansion.charge(1)?;
            self.remaining = &self.remaining[1..];
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl ExecutionCbor for String {
    /// Scalars do not enter a typed deserialization visitor.
    fn check_depth(&self, _remaining: usize) -> Result<()> {
        Ok(())
    }
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        out.string(self)
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.string_value()
    }
}

impl ExecutionCbor for bool {
    /// Scalars do not enter a typed deserialization visitor.
    fn check_depth(&self, _remaining: usize) -> Result<()> {
        Ok(())
    }
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::SIMPLE, if *self { 21 } else { 20 })
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        if input.scalar(0xf4)? {
            Ok(false)
        } else if input.scalar(0xf5)? {
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
            fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(&self, out: &mut W) -> io::Result<()> {
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

    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
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
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
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
        let bytes = input.bytes()?;
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
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Some(value) => value.write_cbor(out),
            None => head::write_head(out, head::SIMPLE, 22),
        }
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        if input.scalar(0xf6)? {
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
            fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(&self, out: &mut W) -> io::Result<()> { self.as_ref().write_cbor(out) }
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
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
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

// V3 fixed digests use one definite byte string, preserving every original byte.
impl<const N: usize> ExecutionCbor for [u8; N] {
    fn check_depth(&self, remaining: usize) -> Result<()> {
        descend(remaining).map(|_| ())
    }
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_bytes(out, self)
    }
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.visit(|input| {
            input
                .bytes()?
                .try_into()
                .map_err(|_| input.invalid("wrong fixed receipt byte-string length"))
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
            fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(&self, out: &mut W) -> io::Result<()> {
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
