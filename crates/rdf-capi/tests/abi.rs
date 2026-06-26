// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests that exercise the `extern "C"` surface directly (the crate
//! exposes an `rlib` so the symbols link without dlopen). These are the primary
//! ABI suite — they call the exact C entry points with C-shaped inputs and
//! assert on status codes, out-params, and free ordering.

use std::ffi::CString;

use purrdf::buffer::{purrdf_buffer_data, purrdf_buffer_free, PurrdfBuffer};
use purrdf::error::{purrdf_error_code, purrdf_error_free, purrdf_error_message, PurrdfError};
use purrdf::handles::{
    purrdf_dataset_free, purrdf_dataset_quad_count, purrdf_dataset_term_count, PurrdfDataset,
};
use purrdf::parse::purrdf_parse;
use purrdf::serialize::purrdf_serialize;
use purrdf::status::{PurrdfAbiVersion, PurrdfStatus};
use purrdf::version::{purrdf_abi_version, PURRDF_ABI_MAJOR, PURRDF_ABI_MINOR, PURRDF_ABI_PATCH};

/// Parse a Turtle/N-Triples snippet, returning the owned dataset handle.
unsafe fn parse(media: &str, doc: &str) -> *mut PurrdfDataset {
    let media = CString::new(media).unwrap();
    let mut dataset: *mut PurrdfDataset = std::ptr::null_mut();
    let mut error: *mut PurrdfError = std::ptr::null_mut();
    let status = purrdf_parse(
        doc.as_ptr(),
        doc.len(),
        media.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        &mut dataset,
        &mut error,
    );
    assert_eq!(status, PurrdfStatus::Ok as i32, "parse should succeed");
    assert!(error.is_null());
    assert!(!dataset.is_null());
    dataset
}

unsafe fn buffer_bytes(buf: *const PurrdfBuffer) -> Vec<u8> {
    let mut ptr: *const u8 = std::ptr::null();
    let mut len: usize = 0;
    assert_eq!(
        purrdf_buffer_data(buf, &mut ptr, &mut len),
        PurrdfStatus::Ok as i32
    );
    std::slice::from_raw_parts(ptr, len).to_vec()
}

#[test]
fn abi_version_is_beta_0_1_0() {
    let mut version = PurrdfAbiVersion {
        major: 9,
        minor: 9,
        patch: 9,
    };
    let status = unsafe { purrdf_abi_version(&mut version) };
    assert_eq!(status, PurrdfStatus::Ok as i32);
    assert_eq!(version.major, PURRDF_ABI_MAJOR);
    assert_eq!(version.minor, PURRDF_ABI_MINOR);
    assert_eq!(version.patch, PURRDF_ABI_PATCH);
    assert_eq!((version.major, version.minor, version.patch), (0, 1, 0));
}

#[test]
fn abi_version_null_out_is_handled() {
    let status = unsafe { purrdf_abi_version(std::ptr::null_mut()) };
    assert_eq!(status, PurrdfStatus::NullPointer as i32);
}

#[test]
fn status_discriminants_are_frozen() {
    // The ABI is SemVer-frozen: these numbers must never change.
    assert_eq!(PurrdfStatus::Ok as i32, 0);
    assert_eq!(PurrdfStatus::NullPointer as i32, 1);
    assert_eq!(PurrdfStatus::InvalidUtf8 as i32, 2);
    assert_eq!(PurrdfStatus::CursorExhausted as i32, 9);
    assert_eq!(PurrdfStatus::GtsError as i32, 10);
    assert_eq!(PurrdfStatus::Panic as i32, 100);
}

#[test]
fn parse_counts_quads_and_terms() {
    unsafe {
        let dataset = parse("text/turtle", "<http://a> <http://b> <http://c> .");
        let mut quads: usize = 0;
        let mut terms: usize = 0;
        assert_eq!(
            purrdf_dataset_quad_count(dataset, &mut quads),
            PurrdfStatus::Ok as i32
        );
        assert_eq!(
            purrdf_dataset_term_count(dataset, &mut terms),
            PurrdfStatus::Ok as i32
        );
        assert_eq!(quads, 1);
        assert_eq!(terms, 3);
        purrdf_dataset_free(dataset);
    }
}

#[test]
fn serialize_round_trips_through_ntriples() {
    unsafe {
        let dataset = parse("text/turtle", "<http://a> <http://b> <http://c> .");
        let media = CString::new("application/n-triples").unwrap();
        let mut buffer: *mut PurrdfBuffer = std::ptr::null_mut();
        let mut dropped: usize = 999;
        let mut error: *mut PurrdfError = std::ptr::null_mut();
        let status = purrdf_serialize(
            dataset,
            media.as_ptr(),
            std::ptr::null(),
            &mut buffer,
            &mut dropped,
            &mut error,
        );
        assert_eq!(status, PurrdfStatus::Ok as i32);
        assert!(error.is_null());
        // N-Triples is star-capable: no statement rows dropped.
        assert_eq!(dropped, 0);
        let bytes = buffer_bytes(buffer);
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("<http://a>"));
        assert!(text.contains("<http://c>"));

        // Re-parse the serialized output; it must yield the same single quad.
        let reparsed = parse("application/n-triples", &text);
        let mut quads: usize = 0;
        purrdf_dataset_quad_count(reparsed, &mut quads);
        assert_eq!(quads, 1);

        purrdf_buffer_free(buffer);
        purrdf_dataset_free(reparsed);
        purrdf_dataset_free(dataset);
    }
}

#[test]
fn parse_rejects_malformed_turtle_without_aborting() {
    unsafe {
        let media = CString::new("text/turtle").unwrap();
        let doc = "<http://a> <http://b> @@@ not-valid";
        let mut dataset: *mut PurrdfDataset = std::ptr::null_mut();
        let mut error: *mut PurrdfError = std::ptr::null_mut();
        let status = purrdf_parse(
            doc.as_ptr(),
            doc.len(),
            media.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            &mut dataset,
            &mut error,
        );
        assert_eq!(status, PurrdfStatus::ParseError as i32);
        assert!(dataset.is_null());
        assert!(!error.is_null());
        assert_eq!(purrdf_error_code(error), PurrdfStatus::ParseError as i32);
        let msg = std::ffi::CStr::from_ptr(purrdf_error_message(error));
        assert!(!msg.to_bytes().is_empty());
        purrdf_error_free(error);
    }
}

#[test]
fn serialize_rejects_unknown_media_type() {
    unsafe {
        let dataset = parse("text/turtle", "<http://a> <http://b> <http://c> .");
        let media = CString::new("application/x-made-up").unwrap();
        let mut buffer: *mut PurrdfBuffer = std::ptr::null_mut();
        let mut error: *mut PurrdfError = std::ptr::null_mut();
        let status = purrdf_serialize(
            dataset,
            media.as_ptr(),
            std::ptr::null(),
            &mut buffer,
            std::ptr::null_mut(),
            &mut error,
        );
        assert_eq!(status, PurrdfStatus::UnsupportedFormat as i32);
        assert!(buffer.is_null());
        assert!(!error.is_null());
        purrdf_error_free(error);
        purrdf_dataset_free(dataset);
    }
}
