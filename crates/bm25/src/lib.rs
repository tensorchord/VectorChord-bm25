// This software is licensed under a dual license model:
//
// GNU Affero General Public License v3 (AGPLv3): You may use, modify, and
// distribute this software under the terms of the AGPLv3.
//
// Elastic License v2 (ELv2): You may also use, modify, and distribute this
// software under the Elastic License v2, which has specific restrictions.
//
// We welcome any commercial collaboration or support. For inquiries
// regarding the licenses, please contact us at:
// vectorchord-inquiry@tensorchord.ai
//
// Copyright (c) 2025-2026 TensorChord Inc.

mod address_documents;
mod address_tokens;
mod bm25;
mod build;
mod bulkdelete;
mod compression;
mod evaluate;
mod insert;
mod maintain;
mod search;
mod segment;
mod tape;
mod tuples;

pub mod seed;
pub mod types;
pub mod vector;

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const WIDTH: usize = 16;

const _: () = assert!(WIDTH <= 32);

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct Opaque {
    pub next: u32,
    pub flags: u32,
}

#[allow(unsafe_code)]
unsafe impl index::relation::Opaque for Opaque {
    fn is_deleted(&self) -> bool {
        const B: u32 = 1 << 0;
        self.flags & B != 0
    }
    fn set_deleted(&mut self) {
        const B: u32 = 1 << 0;
        self.flags |= B;
    }
}

pub use build::build;
pub use bulkdelete::bulkdelete;
pub use evaluate::evaluate;
pub use insert::insert;
pub use maintain::maintain;
pub use search::search;
pub use segment::{Collector, Segment};
