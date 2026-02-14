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

use std::num::NonZero;

use crate::page::{METAPAGE_BLKNO, page_read};
use crate::segment::meta::MetaPageData;
use crate::segment::term_stat::TermStatReader;
use crate::weight::bm25_score_batch;

use super::memory_bm25vector::{Bm25VectorInput, Bm25VectorOutput};

#[pgrx::pg_extern(stable, strict, parallel_safe)]
pub fn search_bm25query(
    target_vector: Bm25VectorInput,
    query: pgrx::composite_type!("bm25query"),
) -> f32 {
    let index_oid: pgrx::pg_sys::Oid = query
        .get_by_index(NonZero::new(1).unwrap())
        .unwrap()
        .unwrap();
    let query_vector: Bm25VectorOutput = query
        .get_by_index(NonZero::new(2).unwrap())
        .unwrap()
        .unwrap();
    let query_vector = query_vector.borrow();
    let target_vector = target_vector.borrow();

    let index =
        unsafe { pgrx::PgRelation::with_lock(index_oid, pgrx::pg_sys::AccessShareLock as _) };
    let meta = {
        let page = unsafe { page_read(index.as_ptr(), METAPAGE_BLKNO) };
        unsafe { &*(page.data().as_ptr() as *const MetaPageData) }
    };

    let term_stat_reader = unsafe { TermStatReader::new(index.as_ptr(), meta) };
    let avgdl = meta.avgdl();
    let scores = bm25_score_batch(
        meta.doc_cnt,
        avgdl,
        &term_stat_reader,
        target_vector,
        query_vector,
    );

    -scores
}
