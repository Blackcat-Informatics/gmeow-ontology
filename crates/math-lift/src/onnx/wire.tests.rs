// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn wire_error(bytes: &[u8]) -> String {
    let mut reader = Reader::new(bytes);
    let mut last = None;
    loop {
        match reader.next_tag() {
            Ok(None) => break,
            Ok(Some(tag)) => {
                last = Some(tag);
                if let Err(e) = reader.skip_field() {
                    return format!("{e}");
                }
            }
            Err(e) => return format!("{e}"),
        }
    }
    panic!("the byte stream decoded cleanly (last tag {last:?}), but a failure was required");
}

/// `(field_number << 3) | wire_type`.
fn tag(number: u32, wire: WireType) -> u8 {
    u8::try_from((number << 3) | u32::from(wire.code())).expect("small tag")
}

#[test]
fn a_varint_field_round_trips_its_value() {
    // field 1, varint, 300 = 0xAC 0x02.
    let bytes = [tag(1, WireType::Varint), 0xac, 0x02];
    let mut reader = Reader::new(&bytes);
    let t = reader.next_tag().expect("tag").expect("a field");
    assert_eq!(t.number, 1);
    assert_eq!(t.wire, WireType::Varint);
    assert_eq!(reader.read_varint().expect("value"), 300);
    assert!(reader.next_tag().expect("end").is_none());
}

#[test]
fn a_negative_int64_decodes_from_its_ten_byte_twos_complement_form() {
    let mut bytes = vec![tag(1, WireType::Varint)];
    #[expect(clippy::cast_sign_loss, reason = "constructing the wire form of -7")]
    let mut value = -7_i64 as u64;
    for _ in 0..9 {
        bytes.push(u8::try_from(value & 0x7f).expect("7 bits") | 0x80);
        value >>= 7;
    }
    bytes.push(u8::try_from(value & 0x7f).expect("7 bits"));
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    assert_eq!(reader.read_i64().expect("value"), -7);
}

#[test]
fn a_truncated_varint_is_a_hard_failure_with_its_offset() {
    let text = wire_error(&[tag(1, WireType::Varint), 0x80, 0x80]);
    assert!(text.contains("truncated"), "{text}");
    assert!(text.contains("byte offset 1"), "{text}");
}

#[test]
fn a_varint_longer_than_ten_bytes_is_a_hard_failure() {
    let mut bytes = vec![tag(1, WireType::Varint)];
    bytes.extend(std::iter::repeat_n(0x80_u8, 11));
    bytes.push(0x00);
    let text = wire_error(&bytes);
    assert!(text.contains("ten bytes"), "{text}");
}

#[test]
fn a_varint_whose_tenth_byte_overflows_sixty_four_bits_is_a_hard_failure() {
    let mut bytes = vec![tag(1, WireType::Varint)];
    bytes.extend(std::iter::repeat_n(0xff_u8, 9));
    bytes.push(0x7f);
    let text = wire_error(&bytes);
    assert!(text.contains("overflows 64 bits"), "{text}");
    assert!(text.contains("0x7f"), "{text}");
}

#[test]
fn a_length_running_past_the_buffer_end_is_a_hard_failure() {
    // field 2, length-delimited, claims 200 bytes but supplies three.
    let text = wire_error(&[tag(2, WireType::LengthDelimited), 200, 1, b'a', b'b']);
    assert!(
        text.contains("truncated or a nested message does not close"),
        "{text}"
    );
    assert!(
        text.contains("needs 200 byte(s) but only 2 remain"),
        "{text}"
    );
    assert!(text.contains("byte offset 3"), "{text}");
}

#[test]
fn a_nested_message_that_does_not_close_is_a_hard_failure() {
    // An outer message of two bytes whose inner field claims a 99-byte string.
    let inner = [tag(1, WireType::LengthDelimited), 99];
    let mut bytes = vec![tag(7, WireType::LengthDelimited), 2];
    bytes.extend_from_slice(&inner);
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    let mut nested = reader.read_message().expect("a nested message");
    let _ = nested.next_tag().expect("tag").expect("a field");
    let err = nested
        .read_string()
        .expect_err("the inner field overruns its parent");
    let text = format!("{err}");
    assert!(text.contains("does not close"), "{text}");
    // The offset is FILE-absolute, not nested-message-relative.
    assert!(text.contains("byte offset 4"), "{text}");
}

#[test]
fn the_deprecated_group_wire_types_are_rejected_by_name() {
    for code in [3_u8, 4_u8] {
        let text = wire_error(&[(1 << 3) | code, 0x00]);
        assert!(text.contains("deprecated group encoding"), "{text}");
        assert!(text.contains(&format!("wire type {code}")), "{text}");
    }
}

#[test]
fn an_unknown_wire_type_is_rejected() {
    for code in [6_u8, 7_u8] {
        let text = wire_error(&[(1 << 3) | code, 0x00]);
        assert!(text.contains("unknown protobuf wire type"), "{text}");
    }
}

#[test]
fn field_number_zero_is_rejected() {
    let text = wire_error(&[0x00, 0x00]);
    assert!(text.contains("field number 0"), "{text}");
}

#[test]
fn a_wrong_wire_type_on_a_known_field_is_rejected() {
    let bytes = [tag(1, WireType::Varint), 0x05];
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    let err = reader
        .read_string()
        .expect_err("field 1 is not a string here");
    assert!(
        format!("{err}").contains("where length-delimited was required"),
        "{err}"
    );
}

#[test]
fn a_non_utf8_string_field_is_rejected_rather_than_re_decoded() {
    let bytes = [tag(1, WireType::LengthDelimited), 2, 0xff, 0xfe];
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    let err = reader.read_string().expect_err("0xff 0xfe is not UTF-8");
    assert!(format!("{err}").contains("not valid UTF-8"), "{err}");
}

#[test]
fn skip_field_steps_over_every_wire_type_and_stays_in_sync() {
    let mut bytes = vec![tag(1, WireType::Varint), 0xac, 0x02];
    bytes.push(tag(2, WireType::Fixed64));
    bytes.extend_from_slice(&7_u64.to_le_bytes());
    bytes.push(tag(3, WireType::Fixed32));
    bytes.extend_from_slice(&1.5_f32.to_le_bytes());
    bytes.extend_from_slice(&[tag(4, WireType::LengthDelimited), 3, b'x', b'y', b'z']);
    bytes.extend_from_slice(&[tag(5, WireType::Varint), 0x2a]);

    let mut reader = Reader::new(&bytes);
    let mut seen = Vec::new();
    while let Some(t) = reader.next_tag().expect("a well-formed tag") {
        if t.number == 5 {
            seen.push(reader.read_varint().expect("the last value"));
        } else {
            reader.skip_field().expect("every wire type is skippable");
        }
    }
    assert_eq!(
        seen,
        vec![42],
        "skipping four fields kept the cursor in sync"
    );
    reader.finish().expect("fully consumed");
}

#[test]
fn a_fixed32_float_decodes_little_endian() {
    let mut bytes = vec![tag(1, WireType::Fixed32)];
    bytes.extend_from_slice(&(-2.5_f32).to_le_bytes());
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    assert!((reader.read_f32().expect("value") + 2.5).abs() < f32::EPSILON);
}

#[test]
fn a_truncated_fixed32_is_a_hard_failure() {
    let text = wire_error(&[tag(1, WireType::Fixed32), 0x00, 0x00]);
    assert!(text.contains("a fixed32 value"), "{text}");
}

#[test]
fn a_truncated_fixed64_is_a_hard_failure() {
    let text = wire_error(&[tag(1, WireType::Fixed64), 0x00, 0x00, 0x00]);
    assert!(text.contains("a fixed64 value"), "{text}");
}

#[test]
fn an_int32_field_outside_the_thirty_two_bit_range_is_rejected() {
    let mut bytes = vec![tag(1, WireType::Varint)];
    let mut value = u64::from(u32::MAX) + 9;
    while value >= 0x80 {
        bytes.push(u8::try_from(value & 0x7f).expect("7 bits") | 0x80);
        value >>= 7;
    }
    bytes.push(u8::try_from(value).expect("7 bits"));
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    let err = reader.read_i32().expect_err("out of int32 range");
    assert!(
        format!("{err}").contains("does not fit in 32 bits"),
        "{err}"
    );
}

#[test]
fn an_empty_buffer_is_an_empty_message_not_an_error() {
    let mut reader = Reader::new(&[]);
    assert!(reader.next_tag().expect("no tag").is_none());
    reader.finish().expect("an empty message is closed");
}

#[test]
#[should_panic(expected = "must be consumed with a read_* or skip_field")]
fn reading_two_tags_without_consuming_is_a_programming_error() {
    let bytes = [
        tag(1, WireType::Varint),
        0x01,
        tag(2, WireType::Varint),
        0x02,
    ];
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    let _ = reader.next_tag();
}

#[test]
fn a_packed_varint_run_reads_through_the_raw_path() {
    let bytes = [tag(8, WireType::LengthDelimited), 3, 1, 2, 3];
    let mut reader = Reader::new(&bytes);
    let _ = reader.next_tag().expect("tag").expect("a field");
    let mut packed = reader.read_message().expect("the packed payload");
    let mut values = Vec::new();
    while !packed.is_exhausted() {
        values.push(packed.read_raw_varint().expect("a packed varint"));
    }
    assert_eq!(values, vec![1, 2, 3]);
}
