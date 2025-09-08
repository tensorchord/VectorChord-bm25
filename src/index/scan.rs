use std::num::NonZero;

use lending_iterator::LendingIterator;
use pgrx::{prelude::PgHeapTuple, FromDatum};

use crate::{
    algorithm::block_wand::{block_wand, block_wand_single, SealedScorer},
    datatype::{Bm25VectorBorrowed, Bm25VectorOutput},
    guc::{BM25_LIMIT, ENABLE_PREFILTER},
    page::{page_read, METAPAGE_BLKNO},
    segment::{
        delete::DeleteBitmapReader, field_norm::FieldNormReader, growing::GrowingSegmentReader,
        meta::MetaPageData, payload::PayloadReader, sealed::SealedSegmentReader,
        term_stat::TermStatReader,
    },
    utils::{loser_tree::LoserTree, topk_computer::TopKComputer},
    weight::{bm25_score_batch, idf, Bm25Weight},
};

pub enum Scanner {
    Initial {
        node: *mut pgrx::pg_sys::IndexScanState,
    },
    Waiting {
        node: *mut pgrx::pg_sys::IndexScanState,
        query_index: pgrx::PgRelation,
        query_vector: Bm25VectorOutput,
    },
    Scanned {
        node: *mut pgrx::pg_sys::IndexScanState,
        results: Vec<u64>,
    },
}

impl Scanner {
    fn node(&self) -> *mut pgrx::pg_sys::IndexScanState {
        match self {
            Scanner::Initial { node } => *node,
            Scanner::Waiting { node, .. } => *node,
            Scanner::Scanned { node, .. } => *node,
        }
    }

    pub fn set_node(&mut self, node: *mut pgrx::pg_sys::IndexScanState) {
        let n = match self {
            Scanner::Initial { node: n } => n,
            Scanner::Waiting { node: n, .. } => n,
            Scanner::Scanned { node: n, .. } => n,
        };
        *n = node;
    }
}

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn ambeginscan(
    index: pgrx::pg_sys::Relation,
    n_keys: std::os::raw::c_int,
    n_orderbys: std::os::raw::c_int,
) -> pgrx::pg_sys::IndexScanDesc {
    use pgrx::memcxt::PgMemoryContexts::CurrentMemoryContext;

    assert!(n_keys == 0, "it doesn't support WHERE clause");
    assert!(n_orderbys == 1, "it only supports one ORDER BY clause");
    let scan = pgrx::pg_sys::RelationGetIndexScan(index, n_keys, n_orderbys);
    (*scan).opaque = CurrentMemoryContext
        .leak_and_drop_on_delete(Scanner::Initial {
            node: std::ptr::null_mut(),
        })
        .cast();
    scan
}

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn amrescan(
    scan: pgrx::pg_sys::IndexScanDesc,
    _keys: pgrx::pg_sys::ScanKey,
    _n_keys: std::os::raw::c_int,
    orderbys: pgrx::pg_sys::ScanKey,
    _n_orderbys: std::os::raw::c_int,
) {
    assert!(!orderbys.is_null());
    std::ptr::copy(orderbys, (*scan).orderByData, (*scan).numberOfOrderBys as _);
    let data = (*scan).orderByData;
    let value = (*data).sk_argument;
    let is_null = ((*data).sk_flags & pgrx::pg_sys::SK_ISNULL as i32) != 0;
    let bm25_query = PgHeapTuple::from_datum(value, is_null).unwrap();
    let index_oid = bm25_query
        .get_by_index(NonZero::new(1).unwrap())
        .unwrap()
        .unwrap();
    let query_vector = bm25_query
        .get_by_index(NonZero::new(2).unwrap())
        .unwrap()
        .unwrap();

    let scanner = (*scan).opaque.cast::<Scanner>().as_mut().unwrap();
    *scanner = Scanner::Waiting {
        node: scanner.node(),
        query_index: pgrx::PgRelation::with_lock(index_oid, pgrx::pg_sys::AccessShareLock as _),
        query_vector,
    };
}

#[pgrx::pg_guard]
pub extern "C-unwind" fn amgettuple(
    scan: pgrx::pg_sys::IndexScanDesc,
    direction: pgrx::pg_sys::ScanDirection::Type,
) -> bool {
    if direction != pgrx::pg_sys::ScanDirection::ForwardScanDirection {
        pgrx::error!("bm25 index without a forward scan direction is not supported");
    }

    let scanner = unsafe { (*scan).opaque.cast::<Scanner>().as_mut().unwrap() };
    let results = match scanner {
        Scanner::Initial { .. } => return false,
        Scanner::Waiting {
            node,
            query_index,
            query_vector,
        } => {
            let results = scan_main(*node, query_index.as_ptr(), query_vector.borrow());
            *scanner = Scanner::Scanned {
                node: *node,
                results,
            };
            let Scanner::Scanned { results, .. } = scanner else {
                unreachable!()
            };
            results
        }
        Scanner::Scanned { results, .. } => results,
    };

    if let Some(tid) = results.pop() {
        unsafe {
            pgrx::itemptr::u64_to_item_pointer(tid, &mut (*scan).xs_heaptid);
            (*scan).xs_recheckorderby = false;
            (*scan).xs_recheck = false;
        }
        true
    } else {
        false
    }
}

#[pgrx::pg_guard]
pub extern "C-unwind" fn amendscan(scan: pgrx::pg_sys::IndexScanDesc) {
    let scanner = unsafe { (*scan).opaque.cast::<Scanner>().as_mut().unwrap() };
    let node = scanner.node();
    *scanner = Scanner::Initial { node };
}

// return top-k results
fn scan_main(
    scan_state: *mut pgrx::pg_sys::IndexScanState,
    index: pgrx::pg_sys::Relation,
    query_vector: Bm25VectorBorrowed,
) -> Vec<u64> {
    let limit = BM25_LIMIT.get();
    if limit == 0 {
        return Vec::new();
    }
    if limit == -1 {
        return brute_force_scan(index, query_vector);
    }

    let page = page_read(index, METAPAGE_BLKNO);
    let meta: &MetaPageData = page.as_ref();
    let avgdl = meta.avgdl();

    let mut computer = TopKComputer::new(BM25_LIMIT.get() as _);
    let delete_bitmap_reader = DeleteBitmapReader::new(index, meta.delete_bitmap_blkno);

    let term_stat_reader = TermStatReader::new(index, meta);
    if let Some(growing) = meta.growing_segment.as_ref() {
        let reader = GrowingSegmentReader::new(index, growing);
        let mut doc_id = meta.sealed_doc_id;
        let mut iter = reader.into_lending_iter(usize::MAX);
        while let Some(vector) = iter.next() {
            if !delete_bitmap_reader.is_delete(doc_id) {
                let score =
                    bm25_score_batch(meta.doc_cnt, avgdl, &term_stat_reader, vector, query_vector);
                computer.push(score, doc_id);
            }
            doc_id += 1;
        }
    }

    let fieldnorm_reader = FieldNormReader::new(index, meta.field_norm_blkno);
    let sealed_reader = SealedSegmentReader::new(index, meta.sealed_segment);
    let scorers = query_vector
        .indexes()
        .iter()
        .zip(query_vector.values())
        .filter_map(|(&term_id, &term_tf)| {
            sealed_reader.get_postings(term_id).map(|posting_reader| {
                let term_cnt = term_stat_reader.read(term_id);
                let idf = idf(meta.doc_cnt, term_cnt);
                let weight = Bm25Weight::new(term_tf, idf, avgdl);
                SealedScorer {
                    posting: posting_reader,
                    weight,
                    max_score: weight.max_score(),
                }
            })
        })
        .collect::<Vec<_>>();

    let payload_reader = PayloadReader::new(index, meta.payload_blkno);

    if ENABLE_PREFILTER.get() {
        let f = |doc_id| {
            let value = payload_reader.read(doc_id);
            let mut tid = pgrx::pg_sys::ItemPointerData::default();
            pgrx::itemptr::u64_to_item_pointer(value, &mut tid);
            unsafe { check(scan_state, &mut tid) }
        };
        if scorers.len() == 1 {
            block_wand_single(
                scorers.into_iter().next().unwrap(),
                &fieldnorm_reader,
                &delete_bitmap_reader,
                &mut computer,
                f,
            );
        } else {
            block_wand(
                scorers,
                &fieldnorm_reader,
                &delete_bitmap_reader,
                &mut computer,
                f,
            );
        }
    } else {
        let f = |_| true;
        if scorers.len() == 1 {
            block_wand_single(
                scorers.into_iter().next().unwrap(),
                &fieldnorm_reader,
                &delete_bitmap_reader,
                &mut computer,
                f,
            );
        } else {
            block_wand(
                scorers,
                &fieldnorm_reader,
                &delete_bitmap_reader,
                &mut computer,
                f,
            );
        }
    }

    computer
        .to_sorted_slice()
        .iter()
        .map(|(_, doc_id)| payload_reader.read(*doc_id))
        .collect()
}

fn brute_force_scan(index: pgrx::pg_sys::Relation, query_vector: Bm25VectorBorrowed) -> Vec<u64> {
    let mut results = Vec::new();

    let page = page_read(index, METAPAGE_BLKNO);
    let meta: &MetaPageData = page.as_ref();
    let avgdl = meta.avgdl();

    let delete_bitmap_reader = DeleteBitmapReader::new(index, meta.delete_bitmap_blkno);

    let term_stat_reader = TermStatReader::new(index, meta);
    if let Some(growing) = meta.growing_segment.as_ref() {
        let reader = GrowingSegmentReader::new(index, growing);
        let mut doc_id = meta.sealed_doc_id;
        let mut iter = reader.into_lending_iter(usize::MAX);
        while let Some(vector) = iter.next() {
            if !delete_bitmap_reader.is_delete(doc_id) {
                let score =
                    bm25_score_batch(meta.doc_cnt, avgdl, &term_stat_reader, vector, query_vector);
                results.push((score, doc_id));
            }
            doc_id += 1;
        }
    }

    let fieldnorm_reader = FieldNormReader::new(index, meta.field_norm_blkno);
    let sealed_reader = SealedSegmentReader::new(index, meta.sealed_segment);

    struct Cmp(f32, u32);
    impl PartialEq for Cmp {
        fn eq(&self, other: &Self) -> bool {
            self.1.eq(&other.1)
        }
    }
    impl Eq for Cmp {}
    impl PartialOrd for Cmp {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Cmp {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.1.cmp(&other.1)
        }
    }

    let iters = query_vector
        .indexes()
        .iter()
        .zip(query_vector.values())
        .filter_map(|(&term_id, &term_tf)| {
            sealed_reader.get_postings(term_id).map(|posting_reader| {
                let term_cnt = term_stat_reader.read(term_id);
                let idf = idf(meta.doc_cnt, term_cnt);
                let weight = Bm25Weight::new(term_tf, idf, avgdl);
                SealedScorer {
                    posting: posting_reader,
                    weight,
                    max_score: weight.max_score(),
                }
                .into_iter(&fieldnorm_reader, &delete_bitmap_reader)
                .map(|(a, b)| Cmp(a, b))
            })
        })
        .collect::<Vec<_>>();
    let loser_tree = LoserTree::new(iters);

    let mut cur_docid = None;
    let mut cur_score = 0.;
    for Cmp(score, docid) in loser_tree {
        if Some(docid) != cur_docid {
            if let Some(docid) = cur_docid {
                results.push((cur_score, docid));
            }
            cur_docid = Some(docid);
            cur_score = 0.;
        }
        cur_score += score;
    }
    if let Some(docid) = cur_docid {
        results.push((cur_score, docid));
    }

    results.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let payload_reader = PayloadReader::new(index, meta.payload_blkno);
    results
        .into_iter()
        .map(|(_, doc_id)| payload_reader.read(doc_id))
        .collect()
}

unsafe fn execute_boolean_qual(
    state: *mut pgrx::pg_sys::ExprState,
    econtext: *mut pgrx::pg_sys::ExprContext,
) -> bool {
    use pgrx::PgMemoryContexts;
    if state.is_null() {
        return true;
    }
    assert!((*state).flags & pgrx::pg_sys::EEO_FLAG_IS_QUAL as u8 != 0);
    let mut is_null = true;
    pgrx::pg_sys::MemoryContextReset((*econtext).ecxt_per_tuple_memory);
    let ret = PgMemoryContexts::For((*econtext).ecxt_per_tuple_memory)
        .switch_to(|_| (*state).evalfunc.unwrap()(state, econtext, &mut is_null));
    assert!(!is_null);
    bool::from_datum(ret, is_null).unwrap()
}

unsafe fn check_quals(node: *mut pgrx::pg_sys::IndexScanState) -> bool {
    let slot = (*node).ss.ss_ScanTupleSlot;
    let econtext = (*node).ss.ps.ps_ExprContext;
    (*econtext).ecxt_scantuple = slot;
    if (*node).ss.ps.qual.is_null() {
        return true;
    }
    let state = (*node).ss.ps.qual;
    let econtext = (*node).ss.ps.ps_ExprContext;
    execute_boolean_qual(state, econtext)
}

unsafe fn check_mvcc(
    node: *mut pgrx::pg_sys::IndexScanState,
    p: pgrx::pg_sys::ItemPointer,
) -> bool {
    let scan_desc = (*node).iss_ScanDesc;
    let heap_fetch = (*scan_desc).xs_heapfetch;
    let index_relation = (*heap_fetch).rel;
    let rd_tableam = (*index_relation).rd_tableam;
    let snapshot = (*scan_desc).xs_snapshot;
    let index_fetch_tuple = (*rd_tableam).index_fetch_tuple.unwrap();
    let mut all_dead = false;
    let slot = (*node).ss.ss_ScanTupleSlot;
    let mut heap_continue = false;
    let found = index_fetch_tuple(
        heap_fetch,
        p,
        snapshot,
        slot,
        &mut heap_continue,
        &mut all_dead,
    );
    if found {
        return true;
    }
    while heap_continue {
        let found = index_fetch_tuple(
            heap_fetch,
            p,
            snapshot,
            slot,
            &mut heap_continue,
            &mut all_dead,
        );
        if found {
            return true;
        }
    }
    false
}

unsafe fn check(node: *mut pgrx::pg_sys::IndexScanState, p: pgrx::pg_sys::ItemPointer) -> bool {
    if node.is_null() {
        return true;
    }
    if !check_mvcc(node, p) {
        return false;
    }
    if !check_quals(node) {
        return false;
    }
    true
}
