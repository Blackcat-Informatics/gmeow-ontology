// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Invocation-local read evidence shared by memoized formula reconstructions.

use std::cell::RefCell;
use std::collections::HashSet;
use std::mem::size_of;
use std::rc::Rc;
use std::sync::Arc;

use purrdf::QuadIds;
use purrdf_hash::fixed::FixedState;

pub(super) struct ReadTrace<Id> {
    rows: Vec<QuadIds<Id>>,
    children: Vec<Arc<Self>>,
    /// Conservative transitive charge also covers children pinned after eviction.
    pub(super) bytes: usize,
}

struct Frame<Id> {
    rows: Vec<QuadIds<Id>>,
    children: Vec<Arc<ReadTrace<Id>>>,
}

pub(crate) struct ReadRecorder<Id>(Rc<RefCell<Vec<Frame<Id>>>>);

impl<Id> Clone for ReadRecorder<Id> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<Id> Default for ReadRecorder<Id> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

impl<Id> ReadRecorder<Id> {
    pub(super) fn begin(&self) {
        self.0.borrow_mut().push(Frame {
            rows: Vec::new(),
            children: Vec::new(),
        });
    }

    pub(crate) fn row(&self, quad: QuadIds<Id>) {
        if let Some(frame) = self.0.borrow_mut().last_mut() {
            frame.rows.push(quad);
        }
    }

    pub(super) fn child(&self, trace: Arc<ReadTrace<Id>>) {
        if let Some(frame) = self.0.borrow_mut().last_mut() {
            frame.children.push(trace);
        }
    }

    pub(super) fn finish(&self) -> Arc<ReadTrace<Id>> {
        let frame = self
            .0
            .borrow_mut()
            .pop()
            .expect("every expansion starts one read frame");
        let bytes = size_of::<ReadTrace<Id>>()
            .saturating_add(2 * size_of::<usize>())
            .saturating_add(
                frame
                    .rows
                    .capacity()
                    .saturating_mul(size_of::<QuadIds<Id>>()),
            )
            .saturating_add(
                frame
                    .children
                    .capacity()
                    .saturating_mul(size_of::<Arc<ReadTrace<Id>>>()),
            )
            .saturating_add(
                frame
                    .children
                    .iter()
                    .fold(0usize, |total, child| total.saturating_add(child.bytes)),
            );
        Arc::new(ReadTrace {
            rows: frame.rows,
            children: frame.children,
            bytes,
        })
    }
}

pub(super) struct ReadRows<'a, Id> {
    pending: Vec<&'a ReadTrace<Id>>,
    rows: std::slice::Iter<'a, QuadIds<Id>>,
    seen: HashSet<*const ReadTrace<Id>, FixedState>,
}

impl<Id> ReadTrace<Id> {
    pub(super) fn rows(&self) -> ReadRows<'_, Id> {
        ReadRows {
            pending: vec![self],
            rows: [].iter(),
            seen: HashSet::with_hasher(FixedState::new()),
        }
    }
}

impl<Id: Copy> Iterator for ReadRows<'_, Id> {
    type Item = QuadIds<Id>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(row) = self.rows.next() {
                return Some(*row);
            }
            let trace = self.pending.pop()?;
            if !self.seen.insert(std::ptr::from_ref(trace)) {
                continue;
            }
            self.pending.extend(trace.children.iter().map(Arc::as_ref));
            self.rows = trace.rows.iter();
        }
    }
}
