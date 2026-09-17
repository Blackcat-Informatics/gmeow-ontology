// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_schema_values_reuse_interpretations_with_bounded_retention() {
    let mut cache = NativeValues::default();
    let source = TermValue::simple_literal("native schema value");
    let first = cache.literal(&source);
    let second = cache.literal(&source);
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    let oversized = TermValue::simple_literal("x".repeat(8 * 1024));
    cache.literal(&oversized);
    assert!(!cache.entries.contains_key(&oversized));
    for index in 0..130 {
        cache.literal(&TermValue::simple_literal(format!("value {index}")));
    }
    assert_eq!(cache.entries.len(), 128);
    let uncached = TermValue::simple_literal("uncached value");
    cache.literal(&uncached);
    assert!(!cache.entries.contains_key(&uncached));
    assert!(std::sync::Arc::ptr_eq(&first, &cache.literal(&source)));
    let count = TermValue::simple_literal("001");
    assert_eq!(cache.cardinality(&count), Some(1));
    assert_eq!(cache.cardinality(&count), Some(1));
    assert_eq!(cache.counts.len(), 1);
    let oversized_count = TermValue::simple_literal("0".repeat(8 * 1024));
    assert_eq!(cache.cardinality(&oversized_count), Some(0));
    assert!(!cache.counts.contains_key(&oversized_count));
    for index in 0..130 {
        assert_eq!(
            cache.cardinality(&TermValue::simple_literal(index.to_string())),
            Some(index)
        );
    }
    assert_eq!(cache.counts.len(), 128);
}
