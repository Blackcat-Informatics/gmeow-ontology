// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// `index()`/`from_index()` round-trip the 0-based slot ↔ 1-based niche at the
/// boundary values (the `+1` niche offset must be exact everywhere) for the
/// engine-only brands too — the arena crate pins its own brands.
#[test]
fn id_niche_offset_round_trips_at_boundaries() {
    for slot in [0usize, 1, (u32::MAX - 2) as usize] {
        let id = PredId::from_index(slot);
        assert_eq!(id.index(), slot, "slot {slot} must round-trip");
    }
    assert_eq!(PredId::from_index(0).index(), 0);
}

/// The `NonZeroU32` niche makes `Option<Id<C>>` pointer-width (no discriminant
/// word), for EVERY brand.
#[test]
fn id_option_is_pointer_width() {
    assert_eq!(
        std::mem::size_of::<Option<TermId>>(),
        std::mem::size_of::<TermId>(),
        "Option<TermId> must be niche-packed to TermId's width"
    );
    assert_eq!(std::mem::size_of::<TermId>(), std::mem::size_of::<u32>());
    assert_eq!(
        std::mem::size_of::<Option<PredId>>(),
        std::mem::size_of::<PredId>()
    );
    assert_eq!(
        std::mem::size_of::<Option<RowId>>(),
        std::mem::size_of::<RowId>()
    );
    assert_eq!(
        std::mem::size_of::<Option<RuleId>>(),
        std::mem::size_of::<RuleId>()
    );
    // A TermRef is exactly its wrapped TermId — the row-tuple argument handle adds
    // no width over the atomic handle it carries.
    assert_eq!(
        std::mem::size_of::<TermRef>(),
        std::mem::size_of::<TermId>()
    );
}

/// `Ord` is by raw index (mint order) — earlier-minted sorts first.
#[test]
fn id_ord_is_mint_order() {
    let a = PredId::from_index(0);
    let b = PredId::from_index(1);
    assert!(a < b, "mint order: slot 0 precedes slot 1");
    assert_eq!(a, PredId::from_index(0));
}
