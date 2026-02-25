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

use lending_iterator::LendingIterator;

use crate::page::{BM25_PAGE_SIZE, METAPAGE_BLKNO, page_read, page_write};
use crate::segment::delete::DeleteBitmapReader;
use crate::segment::field_norm::{FieldNormRead, FieldNormReader};
use crate::segment::growing::GrowingSegmentReader;
use crate::segment::meta::MetaPageData;
use crate::segment::payload::PayloadReader;
use crate::segment::sealed::SealedSegmentReader;
use crate::segment::term_stat::TermStatReader;

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn ambulkdelete(
    info: *mut pgrx::pg_sys::IndexVacuumInfo,
    stats: *mut pgrx::pg_sys::IndexBulkDeleteResult,
    callback: pgrx::pg_sys::IndexBulkDeleteCallback,
    callback_state: *mut std::os::raw::c_void,
) -> *mut pgrx::pg_sys::IndexBulkDeleteResult {
    use pgrx::pg_sys::ffi::pg_guard_ffi_boundary;
    unsafe {
        let mut callback = {
            let callback = callback.unwrap();
            let mut item: pgrx::pg_sys::ItemPointerData = Default::default();
            move |p: u64| {
                pgrx::itemptr::u64_to_item_pointer(p, &mut item);
                #[allow(ffi_unwind_calls, reason = "protected by pg_guard_ffi_boundary")]
                pg_guard_ffi_boundary(|| callback(&mut item, callback_state))
            }
        };

        let mut stats = stats;
        if stats.is_null() {
            stats = pgrx::pg_sys::palloc0(size_of::<pgrx::pg_sys::IndexBulkDeleteResult>()).cast();
        }
        let stats = stats.as_mut().unwrap();

        let index = (*info).index;
        let mut metapage = page_write(index, METAPAGE_BLKNO);
        let meta: &mut MetaPageData = metapage.as_mut();
        let payload_reader = PayloadReader::new(index, meta.payload_blkno);
        let field_norm_reader = FieldNormReader::new(index, meta.field_norm_blkno);
        let mut delete_bitmap_reader = DeleteBitmapReader::new(index, meta.delete_bitmap_blkno);

        for i in 0..meta.current_doc_id {
            if i % BM25_PAGE_SIZE as u32 == 0 {
                #[cfg(any(
                    feature = "pg14",
                    feature = "pg15",
                    feature = "pg16",
                    feature = "pg17"
                ))]
                pgrx::pg_sys::vacuum_delay_point();
                #[cfg(feature = "pg18")]
                pgrx::pg_sys::vacuum_delay_point(false);
            }
            if delete_bitmap_reader.is_delete(i) {
                continue;
            }
            let tid = payload_reader.read(i);
            if callback(tid) {
                delete_bitmap_reader.delete(i);
                meta.doc_cnt -= 1;
                meta.doc_term_cnt -= field_norm_reader.read(i) as u64;
                stats.tuples_removed += 1.0;
            } else {
                stats.num_index_tuples += 1.0;
            }
        }

        stats
    }
}

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn amvacuumcleanup(
    info: *mut pgrx::pg_sys::IndexVacuumInfo,
    stats: *mut pgrx::pg_sys::IndexBulkDeleteResult,
) -> *mut pgrx::pg_sys::IndexBulkDeleteResult {
    unsafe {
        if stats.is_null() {
            return std::ptr::null_mut();
        }

        (*stats).num_pages = pgrx::pg_sys::RelationGetNumberOfBlocksInFork(
            (*info).index,
            pgrx::pg_sys::ForkNumber::MAIN_FORKNUM,
        );

        if (*stats).tuples_removed == 0.0 {
            return stats;
        }

        let index = (*info).index;

        let metapage = page_read(index, METAPAGE_BLKNO);
        let meta: &MetaPageData = metapage.as_ref();
        let term_id_cnt = meta.term_id_cnt;
        let mut term_stats = (0..term_id_cnt).map(|_| 0u32).collect::<Vec<_>>();
        let delete_bitmap_reader = DeleteBitmapReader::new(index, meta.delete_bitmap_blkno);

        if let Some(growing) = meta.growing_segment.as_ref() {
            let reader = GrowingSegmentReader::new(index, growing);
            let mut doc_id = meta.sealed_doc_id;
            let mut iter = reader.into_lending_iter(usize::MAX);
            while let Some(vector) = iter.next() {
                if !delete_bitmap_reader.is_delete(doc_id) {
                    for &idx in vector.indexes() {
                        term_stats[idx as usize] += 1;
                    }
                }
                doc_id += 1;
            }
        }

        let sealed_reader = SealedSegmentReader::new(index, meta.sealed_segment);
        for i in 0..meta.sealed_segment.term_id_cnt {
            let Some(mut posting) = sealed_reader.get_postings(i) else {
                continue;
            };
            loop {
                posting.decode_block();
                loop {
                    let doc_id = posting.docid();
                    if !delete_bitmap_reader.is_delete(doc_id) {
                        term_stats[i as usize] += 1;
                    }
                    if !posting.next_doc() {
                        break;
                    }
                }
                if !posting.next_block() {
                    break;
                }
            }
        }

        let mut metapage = metapage.upgrade(index);
        let meta: &mut MetaPageData = metapage.as_mut();
        let term_stat_reader = TermStatReader::new(index, meta);
        for i in 0..term_id_cnt {
            term_stat_reader.update(i, |tf| {
                *tf = term_stats[i as usize];
            });
        }

        stats
    }
}
