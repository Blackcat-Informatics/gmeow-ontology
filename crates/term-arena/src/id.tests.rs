// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// `index()`/`from_index()` round-trip the 0-based slot ↔ 1-based niche at the
/// boundary values (the `+1` niche offset must be exact everywhere).
#[test]
fn id_niche_offset_round_trips_at_boundaries() {
    for slot in [0usize, 1, (u32::MAX - 2) as usize] {
        let id = TermId::from_index(slot);
        assert_eq!(id.index(), slot, "slot {slot} must round-trip");
    }
    // Slot 0 is stored as NonZeroU32(1) — the niche is genuinely used.
    assert_eq!(TermId::from_index(0).index(), 0);
}

/// The `NonZeroU32` niche makes `Option<Id<C>>` pointer-width (no discriminant
/// word), for EVERY brand this crate mints.
#[test]
fn id_option_is_pointer_width() {
    assert_eq!(
        std::mem::size_of::<Option<TermId>>(),
        std::mem::size_of::<TermId>(),
        "Option<TermId> must be niche-packed to TermId's width"
    );
    assert_eq!(std::mem::size_of::<TermId>(), std::mem::size_of::<u32>());
    assert_eq!(
        std::mem::size_of::<Option<NodeId>>(),
        std::mem::size_of::<NodeId>()
    );
    assert_eq!(
        std::mem::size_of::<Option<MetaId>>(),
        std::mem::size_of::<MetaId>()
    );
}

/// `Ord` is by raw index (mint order) — earlier-minted sorts first.
#[test]
fn id_ord_is_mint_order() {
    let a = TermId::from_index(0);
    let b = TermId::from_index(1);
    assert!(a < b, "mint order: slot 0 precedes slot 1");
    assert_eq!(a, TermId::from_index(0));
}
