// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONNX parse tier, layer 0: a hand-rolled protobuf wire decoder.
//!
//! `.onnx` is a serialized `onnx.ModelProto`, so reading one means reading protobuf. This
//! module is that reader, written by hand rather than generated.
//!
//! # Why hand-rolled
//!
//! A code-generated decoder would drag `prost`/`protobuf` (and a `protoc`-shaped build
//! step) into a crate whose whole point is that it links no runtime and builds for wasm.
//! The wire format is four live types and a varint; the *field numbers* are the part that
//! matters, and those live in [`super::model`] where a reviewer can check them against the
//! `onnx.proto` schema. Generated code would hide them in a build artifact.
//!
//! # Every malformation is a hard failure with a byte offset
//!
//! `MATHEMATICS-RUNTIME.md`'s ingestion rules forbid a degraded parse, and a protobuf
//! decoder is exactly where "degraded" sneaks in: the format is self-describing enough that
//! a truncated buffer *looks* like a short message and an over-long varint *looks* like a
//! big number. Nothing here truncates, saturates, or silently stops:
//!
//! | condition | outcome |
//! |---|---|
//! | buffer ends mid-varint | [`OnnxWire`] |
//! | varint longer than 10 bytes | [`OnnxWire`] |
//! | varint whose 10th byte overflows 64 bits | [`OnnxWire`] |
//! | a length-delimited length running past the buffer end | [`OnnxWire`] |
//! | a nested message whose fields do not close inside it | [`OnnxWire`] |
//! | wire type 3 / 4 (the deprecated groups) | [`OnnxWire`] |
//! | wire type 6 / 7 (undefined) | [`OnnxWire`] |
//! | field number 0 (illegal) | [`OnnxWire`] |
//! | a known field carrying the wrong wire type | [`OnnxWire`] |
//! | a `string` field that is not valid UTF-8 | [`OnnxWire`] |
//!
//! Every message carries the **absolute** byte offset within the whole `.onnx` file, not an
//! offset within the sub-message being decoded, so a diagnostic points at a byte a human can
//! find with a hex editor. That is what a [`Reader`]'s private `base` field is for.
//!
//! # The tag/value protocol
//!
//! A caller reads a [`Tag`], then consumes its value with exactly one `read_*` or
//! [`Reader::skip_field`]:
//!
//! ```text
//! while let Some(tag) = reader.next_tag()? {
//!     match tag.number {
//!         1 => name = reader.read_string()?.to_owned(),
//!         _ => reader.skip_field()?,
//!     }
//! }
//! ```
//!
//! The two-step shape is what makes `skip_field` real: an unknown field number is *skipped
//! by wire type*, which is the only way to stay in sync with a producer that wrote fields
//! this subset does not model. Forgetting to consume would desynchronize the stream, so the
//! reader tracks the outstanding tag and treats a second [`Reader::next_tag`] before a
//! consume as a programming error (a panic), never as silent corruption of the decode.

use gmeow_errors::Diag;

use crate::error::OnnxWire;

/// The protobuf wire types this decoder accepts.
///
/// Types 3 and 4 (`StartGroup`/`EndGroup`) are the deprecated group encoding and are
/// rejected by name rather than lumped in with "unknown", because a producer emitting them
/// is emitting a *legal but obsolete* encoding this decoder deliberately does not implement
/// — a distinction the diagnostic should carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireType {
    /// 0 — a base-128 varint (`int32`, `int64`, `uint*`, `bool`, `enum`).
    Varint,
    /// 1 — eight little-endian bytes (`fixed64`, `sfixed64`, `double`).
    Fixed64,
    /// 2 — a varint length followed by that many bytes (`string`, `bytes`, embedded
    /// messages, and every packed repeated field).
    LengthDelimited,
    /// 5 — four little-endian bytes (`fixed32`, `sfixed32`, `float`).
    Fixed32,
}

impl WireType {
    /// The wire type's numeric code, as it appears in the low three bits of a tag.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Varint => 0,
            Self::Fixed64 => 1,
            Self::LengthDelimited => 2,
            Self::Fixed32 => 5,
        }
    }

    /// The protobuf spelling used in diagnostics.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Varint => "varint",
            Self::Fixed64 => "fixed64",
            Self::LengthDelimited => "length-delimited",
            Self::Fixed32 => "fixed32",
        }
    }
}

/// One field header: its number and the wire type of the value that follows.
///
/// `#[must_use]`: dropping a tag without consuming its value leaves the reader pointing at
/// a value byte rather than at the next tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Tag {
    /// The protobuf field number (always non-zero — zero is rejected at parse time).
    pub number: u32,
    /// The wire type of this field's value.
    pub wire: WireType,
    /// The absolute byte offset of the tag within the whole `.onnx` file.
    pub offset: usize,
}

/// A cursor over one protobuf message body.
///
/// A nested message is decoded by a *fresh* `Reader` over the parent's sub-slice
/// ([`Reader::read_message`]), which is what bounds the nesting: a nested field can never
/// read a byte the enclosing length did not cover, so "a nested message that does not
/// close" surfaces as a length running past this reader's end.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    /// The absolute offset of `buf[0]` within the whole file, so diagnostics carry a
    /// file-relative byte position rather than a sub-message-relative one.
    base: usize,
    /// The wire type of the tag read but not yet consumed.
    pending: Option<WireType>,
}

impl<'a> Reader<'a> {
    /// A reader over a whole `.onnx` byte stream.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            base: 0,
            pending: None,
        }
    }

    /// The absolute offset of the next unread byte.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.base + self.pos
    }

    /// Whether every byte of this message body has been consumed.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.pos >= self.buf.len()
    }

    /// How many bytes remain in this message body.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn fail(&self, detail: String) -> Diag {
        Diag::of_kind(OnnxWire { detail })
    }

    /// Read the next field header, or report the end of the message.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a malformed tag varint, a zero field number, or a wire type this
    /// decoder rejects (the deprecated groups 3/4, or the undefined 6/7).
    ///
    /// # Panics
    ///
    /// Panics when the previous tag's value was never consumed — a programming error in a
    /// decoder in this crate, never something a byte stream can provoke.
    pub fn next_tag(&mut self) -> gmeow_errors::Result<Option<Tag>> {
        assert!(
            self.pending.is_none(),
            "a protobuf tag's value must be consumed with a read_* or skip_field before the \
             next tag is read"
        );
        if self.is_exhausted() {
            return Ok(None);
        }
        let offset = self.offset();
        let tag = self.read_raw_varint()?;
        let number = u32::try_from(tag >> 3).map_err(|_| {
            self.fail(format!(
                "protobuf field number {} at byte offset {offset} exceeds the 32-bit field-number \
                 space; the byte stream is not a well-formed ONNX ModelProto",
                tag >> 3
            ))
        })?;
        if number == 0 {
            return Err(self.fail(format!(
                "protobuf field number 0 at byte offset {offset}: zero is not a legal field \
                 number, so this byte stream is not a well-formed protobuf message"
            )));
        }
        let code = u8::try_from(tag & 0b111).unwrap_or(u8::MAX);
        let wire = match code {
            0 => WireType::Varint,
            1 => WireType::Fixed64,
            2 => WireType::LengthDelimited,
            5 => WireType::Fixed32,
            3 | 4 => {
                return Err(self.fail(format!(
                    "protobuf wire type {code} (the deprecated group encoding) on field {number} \
                     at byte offset {offset}: groups were removed from proto3 and no ONNX \
                     producer emits them, so this decoder rejects them rather than guessing at \
                     the group's extent"
                )));
            }
            other => {
                return Err(self.fail(format!(
                    "unknown protobuf wire type {other} on field {number} at byte offset \
                     {offset}: wire types 6 and 7 are undefined, so the byte stream cannot be \
                     resynchronized and is not a well-formed ONNX ModelProto"
                )));
            }
        };
        self.pending = Some(wire);
        Ok(Some(Tag {
            number,
            wire,
            offset,
        }))
    }

    /// Take the outstanding tag's wire type, checking it is the one the caller expects.
    fn take_pending(&mut self, expected: WireType, what: &str) -> gmeow_errors::Result<()> {
        let wire = self
            .pending
            .take()
            .expect("a value is read only after its tag");
        if wire != expected {
            return Err(self.fail(format!(
                "a {what} field at byte offset {} carries protobuf wire type {} where {} was \
                 required; a field number decoded with the wrong wire type means the byte stream \
                 is not the ONNX ModelProto schema it claims to be",
                self.offset(),
                wire.name(),
                expected.name()
            )));
        }
        Ok(())
    }

    /// Discard the outstanding tag's value, whatever its wire type.
    ///
    /// This is how an unknown field number stays in sync: protobuf guarantees a value can be
    /// stepped over from its wire type alone, so a producer that wrote fields outside this
    /// subset (a `TrainingInfoProto`, a `FunctionProto`, a tensor payload) costs nothing but
    /// still has its bytes bounds-checked.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] when the value is truncated or its length runs past the buffer end.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn skip_field(&mut self) -> gmeow_errors::Result<()> {
        let wire = self
            .pending
            .take()
            .expect("a value is skipped only after its tag");
        match wire {
            WireType::Varint => {
                self.read_raw_varint()?;
            }
            WireType::Fixed64 => {
                self.take(8, "a fixed64 value")?;
            }
            WireType::Fixed32 => {
                self.take(4, "a fixed32 value")?;
            }
            WireType::LengthDelimited => {
                self.take_length_delimited()?;
            }
        }
        Ok(())
    }

    /// Read a varint-encoded value (`int32`, `int64`, `uint64`, `bool`, an `enum`).
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch or a malformed varint.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_varint(&mut self) -> gmeow_errors::Result<u64> {
        self.take_pending(WireType::Varint, "varint")?;
        self.read_raw_varint()
    }

    /// Read a protobuf `int64`.
    ///
    /// Negative `int64`s are transmitted as their two's-complement 64-bit pattern in a
    /// ten-byte varint, so the reinterpretation is the decoding — not a lossy cast.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch or a malformed varint.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_i64(&mut self) -> gmeow_errors::Result<i64> {
        let raw = self.read_varint()?;
        // Not a lossy cast: protobuf `int64` IS the two's-complement bit pattern.
        #[allow(clippy::cast_possible_wrap)]
        let signed = raw as i64;
        Ok(signed)
    }

    /// Read a protobuf `int32`.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch, a malformed varint, or a value outside the
    /// 32-bit range — which is a malformed stream, never something to truncate.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_i32(&mut self) -> gmeow_errors::Result<i32> {
        let offset = self.offset();
        let wide = self.read_i64()?;
        i32::try_from(wide).map_err(|_| {
            self.fail(format!(
                "protobuf int32 field at byte offset {offset} decoded to {wide}, which does not \
                 fit in 32 bits; narrowing it would silently corrupt the value, so the stream is \
                 rejected"
            ))
        })
    }

    /// Read a protobuf `float` (wire type 5, IEEE-754 little-endian).
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch or a truncated value.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_f32(&mut self) -> gmeow_errors::Result<f32> {
        self.take_pending(WireType::Fixed32, "float")?;
        self.read_raw_f32()
    }

    /// Read a length-delimited byte run.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch, a malformed length varint, or a length running
    /// past the end of this message body.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_bytes(&mut self) -> gmeow_errors::Result<&'a [u8]> {
        self.take_pending(WireType::LengthDelimited, "length-delimited")?;
        Ok(self.take_length_delimited()?.0)
    }

    /// Read a protobuf `string`.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch, a bad length, or bytes that are not valid
    /// UTF-8 — proto3 `string` is defined to be UTF-8, and re-reading it under a guessed
    /// encoding would be exactly the degraded parse the ingestion rules forbid.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_string(&mut self) -> gmeow_errors::Result<&'a str> {
        self.take_pending(WireType::LengthDelimited, "string")?;
        let (bytes, offset) = self.take_length_delimited()?;
        std::str::from_utf8(bytes).map_err(|e| {
            self.fail(format!(
                "a protobuf string field at byte offset {offset} is not valid UTF-8 (invalid byte \
                 sequence at +{}); proto3 defines `string` as UTF-8, and re-decoding it under a \
                 guessed encoding would be a degraded parse",
                e.valid_up_to()
            ))
        })
    }

    /// Read a nested message as its own bounded [`Reader`].
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a wire-type mismatch, a malformed length varint, or a length running
    /// past the end of this message body.
    ///
    /// # Panics
    ///
    /// Panics when there is no outstanding tag.
    pub fn read_message(&mut self) -> gmeow_errors::Result<Reader<'a>> {
        self.take_pending(WireType::LengthDelimited, "embedded message")?;
        let (bytes, offset) = self.take_length_delimited()?;
        Ok(Reader {
            buf: bytes,
            pos: 0,
            base: offset,
            pending: None,
        })
    }

    /// Read a varint directly, with no tag outstanding — the packed-repeated payload path.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] on a truncated varint, one longer than ten bytes, or one whose tenth
    /// byte carries bits beyond the 64-bit value space.
    pub fn read_raw_varint(&mut self) -> gmeow_errors::Result<u64> {
        let start = self.offset();
        let mut value: u64 = 0;
        let mut index = 0_usize;
        loop {
            let Some(byte) = self.buf.get(self.pos).copied() else {
                return Err(self.fail(format!(
                    "a protobuf varint beginning at byte offset {start} is truncated: the buffer \
                     ends after {index} continuation byte(s) with the high bit still set. A \
                     truncated varint is never read as the bytes that did arrive"
                )));
            };
            self.pos += 1;
            // The tenth byte is the last one a 64-bit varint may have, and it may carry only
            // bit 63 (value 0 or 1). A continuation bit there means the encoding runs long;
            // a larger value means it overflows. The two are distinguished because they are
            // different malformations, and a reader should say which it found.
            if index == 9 {
                if byte & 0x80 != 0 {
                    return Err(self.fail(format!(
                        "a protobuf varint beginning at byte offset {start} runs past ten bytes \
                         without a terminating byte; a 64-bit varint is at most ten bytes, so \
                         the byte stream is not a well-formed protobuf message"
                    )));
                }
                if byte > 1 {
                    return Err(self.fail(format!(
                        "a protobuf varint beginning at byte offset {start} overflows 64 bits: \
                         its tenth byte is 0x{byte:02x}, which carries value bits above bit 63"
                    )));
                }
            }
            value |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            index += 1;
        }
    }

    /// Read four little-endian bytes as an IEEE-754 `f32`, with no tag outstanding.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] when fewer than four bytes remain.
    pub fn read_raw_f32(&mut self) -> gmeow_errors::Result<f32> {
        let bytes = self.take(4, "a float value")?;
        let mut word = [0_u8; 4];
        word.copy_from_slice(bytes);
        Ok(f32::from_le_bytes(word))
    }

    /// Require that this message body is fully consumed.
    ///
    /// # Errors
    ///
    /// [`OnnxWire`] when trailing bytes remain — reachable only from a caller that stops
    /// reading fields early, which no decoder in this crate does.
    pub fn finish(&self) -> gmeow_errors::Result<()> {
        if self.is_exhausted() {
            return Ok(());
        }
        Err(self.fail(format!(
            "{} trailing byte(s) remain at byte offset {} after the last decoded field; a \
             protobuf message body is exactly its fields",
            self.remaining(),
            self.offset()
        )))
    }

    /// Consume exactly `count` bytes, or fail with the offset the run began at.
    fn take(&mut self, count: usize, what: &str) -> gmeow_errors::Result<&'a [u8]> {
        let start = self.offset();
        let end = self.pos.checked_add(count).ok_or_else(|| {
            self.fail(format!(
                "{what} at byte offset {start} declares a length that overflows the address space"
            ))
        })?;
        if end > self.buf.len() {
            return Err(self.fail(format!(
                "{what} at byte offset {start} needs {count} byte(s) but only {} remain before \
                 the end of the enclosing message; the byte stream is truncated or a nested \
                 message does not close",
                self.remaining()
            )));
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Read a length varint and the run of bytes it covers, returning the run's absolute
    /// offset alongside it.
    fn take_length_delimited(&mut self) -> gmeow_errors::Result<(&'a [u8], usize)> {
        let start = self.offset();
        let length = self.read_raw_varint()?;
        let count = usize::try_from(length).map_err(|_| {
            self.fail(format!(
                "a length-delimited protobuf field at byte offset {start} declares a length of \
                 {length} bytes, which exceeds this platform's address space"
            ))
        })?;
        let offset = self.offset();
        let slice = self.take(count, "a length-delimited field")?;
        Ok((slice, offset))
    }
}

#[path = "wire.tests.rs"]
#[cfg(test)]
mod tests;
