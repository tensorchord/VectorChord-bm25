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
// Copyright (c) 2025 TensorChord Inc.

use crate::page::{page_free, page_read};

pub mod builder;
pub mod delete;
pub mod field_norm;
pub mod growing;
pub mod meta;
pub mod payload;
pub mod posting;
pub mod sealed;
pub mod term_stat;

pub fn free_page_lists(index: pgrx::pg_sys::Relation, blkno: pgrx::pg_sys::BlockNumber) {
    let mut current_free_blkno = blkno;

    while current_free_blkno != pgrx::pg_sys::InvalidBlockNumber {
        let page = page_read(index, current_free_blkno);
        let next_blkno = page.opaque.next_blkno;
        page_free(index, current_free_blkno);
        current_free_blkno = next_blkno;
    }
}
