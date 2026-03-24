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

use crate::tuples::*;
use crate::vector::Bm25VectorBorrowed;
use crate::{Opaque, compression, guide, idf, tf};
use always_equal::AlwaysEqual;
use core::f64;
use index::relation::{Page, RelationRead};
use score::Score;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, VecDeque};
use std::iter::chain;
use std::num::NonZero;

pub fn search<R: RelationRead>(
    index: &R,
    k: NonZero<usize>,
    query: Bm25VectorBorrowed<'_>,
    mut filter: impl FnMut([u16; 3]) -> bool,
) -> Vec<(Reverse<Score>, AlwaysEqual<[u16; 3]>)>
where
    R::Page: Page<Opaque = Opaque>,
{
    let meta_guard = index.read(0);
    let meta_bytes = meta_guard.get(1).expect("data corruption");
    let meta_tuple = MetaTuple::deserialize_ref(meta_bytes);
    let k1 = meta_tuple.k1();
    let b = meta_tuple.b();
    let ptr_segment = meta_tuple.wptr_segment();
    drop(meta_guard);

    let segment_guard = index.read(ptr_segment);
    let segment_bytes = segment_guard.get(1).expect("data corruption");
    let segment_tuple = SegmentTuple::deserialize_ref(segment_bytes);

    let sum_of_document_lengths = segment_tuple.sum_of_document_lengths();
    let number_of_documents = segment_tuple.number_of_documents();
    let avgdl = sum_of_document_lengths as f64 / number_of_documents as f64;

    let mut cursors = Vec::new();
    for &key in query.indexes() {
        let Some(token) = guide::read(index, segment_tuple.iptr_tokens(), key) else {
            continue;
        };
        let token_guard = index.read(token.0);
        let token_bytes = token_guard.get(token.1).expect("data corruption");
        let token_tuple = TokenTuple::deserialize_ref(token_bytes);
        cursors.push(Reverse(Box::new(Cursor::new(
            index,
            k1,
            b,
            number_of_documents,
            avgdl,
            key,
            token_tuple.number_of_documents(),
            token_tuple.wand_document_length(),
            token_tuple.wand_term_frequency(),
            token_tuple.wptr_summaries().into_inner(),
        ))));
    }

    let mut results = Results::<[u16; 3]>::new(k, 0.0);
    let mut tail = Vec::<Box<Cursor>>::new();
    let mut head = BinaryHeap::from(cursors);
    'main: loop {
        let lead = 'lead: {
            let mut sum = 0.0f64;
            for cursor in tail.iter() {
                sum += cursor.token_upper_bound();
            }
            while let Some(Reverse(cursor)) = head.pop() {
                if cursor.document_id() == u32::MAX {
                    break 'main;
                }
                if results.threshold() < sum + cursor.token_upper_bound() {
                    break 'lead cursor;
                } else {
                    sum += cursor.token_upper_bound();
                    tail.push(cursor);
                }
            }
            break 'main;
        };
        let document_id = lead.document_id();
        let mut lead = vec![lead];
        while let Some(Reverse(cursor)) = binary_heap_pop_if(&mut head, |Reverse(cursor)| {
            document_id == cursor.document_id()
        }) {
            lead.push(cursor);
        }
        {
            let mut failures = tail.extract_if(.., |cursor| {
                cursor.seek_block(index, document_id);
                document_id < cursor.document_id()
            });
            if let Some(failure) = failures.next() {
                for cursor in lead {
                    head.push(Reverse(cursor));
                }
                head.push(Reverse(failure));
                for failure in failures {
                    head.push(Reverse(failure));
                }
                continue 'main;
            }
        }
        let sum_of_block_upper_bounds = {
            let mut result = 0.0;
            for cursor in tail.iter() {
                result += cursor.block_upper_bound();
            }
            for cursor in lead.iter() {
                result += cursor.block_upper_bound();
            }
            result
        };
        if results.threshold() < sum_of_block_upper_bounds {
            {
                let mut failures = tail.extract_if(.., |cursor| {
                    cursor.seek(index, document_id);
                    document_id < cursor.document_id()
                });
                if let Some(failure) = failures.next() {
                    for cursor in lead {
                        head.push(Reverse(cursor));
                    }
                    head.push(Reverse(failure));
                    /*
                    for failure in failures {
                        head.push(Reverse(failure));
                    }
                    */
                    continue 'main;
                }
            };
            let document = guide::read(index, segment_tuple.iptr_documents(), document_id)
                .expect("data corruption");
            let document_guard = index.read(document.0);
            let document_bytes = document_guard.get(document.1).expect("data corruption");
            let document_tuple = DocumentTuple::deserialize_ref(document_bytes);
            let document_length = document_tuple.length();
            let payload = document_tuple.payload();
            if filter(payload) {
                let mut result = 0.0;
                for cursor in chain(tail.iter_mut(), lead.iter_mut()) {
                    let idf = idf(number_of_documents, cursor.token_number_of_documents());
                    let tf = tf(k1, b, avgdl, document_length, cursor.get(index));
                    result += idf * tf;
                }
                results.push(result, payload);
            }
            for mut cursor in chain(tail.into_iter(), lead.into_iter()) {
                cursor.seek(index, 1 + document_id);
                head.push(Reverse(cursor));
            }
            tail = Vec::new();
        } else {
            let min_of_block_max_document_ids = {
                let mut result = u32::MAX;
                for cursor in lead.iter() {
                    result = result.min(cursor.block_max_document_id());
                }
                for cursor in tail.iter() {
                    result = result.min(cursor.block_max_document_id());
                }
                result
            };
            let seek_document_id = std::cmp::min(
                1 + min_of_block_max_document_ids,
                head.peek()
                    .map(|Reverse(cursor)| cursor.document_id())
                    .unwrap_or(u32::MAX),
            );
            let mut cursor = {
                let array = [&mut lead, &mut tail];
                let mut max = f64::NEG_INFINITY;
                let mut argmax = (0, 0);
                for i in 0..array.len() {
                    for j in 0..array[i].len() {
                        if array[i][j].token_upper_bound() > max {
                            max = array[i][j].token_upper_bound();
                            argmax = (i, j);
                        }
                    }
                }
                array[argmax.0].remove(argmax.1)
            };
            cursor.seek(index, seek_document_id);
            head.push(Reverse(cursor));
            for cursor in lead.into_iter() {
                head.push(Reverse(cursor));
            }
        }
    }
    results.into_sorted_vec()
}

pub struct Results<T> {
    limit: NonZero<usize>,
    threshold: Score,
    internal: BinaryHeap<(Reverse<Score>, AlwaysEqual<T>)>,
}

impl<T> Results<T> {
    pub fn new(limit: NonZero<usize>, threshold: f64) -> Self {
        Self {
            limit,
            threshold: Score::from_f64(threshold),
            internal: BinaryHeap::new(),
        }
    }
    pub fn threshold(&self) -> f64 {
        self.threshold.to_f64()
    }
    pub fn push(&mut self, key: f64, value: T) {
        self.internal
            .push((Reverse(Score::from_f64(key)), AlwaysEqual(value)));
        if self.internal.len() > self.limit.get() {
            self.internal.pop();
        }
        if self.internal.len() == self.limit.get() {
            self.threshold = self.threshold.max(self.internal.peek().unwrap().0.0);
        }
    }
    pub fn into_sorted_vec(self) -> Vec<(Reverse<Score>, AlwaysEqual<T>)> {
        self.internal.into_sorted_vec()
    }
}

struct Cursor {
    k1: f64,
    b: f64,
    number_of_documents: u32,
    avgdl: f64,

    token_id: u32,
    token_number_of_documents: u32,
    token_upper_bound: f64,

    document_id: u32,
    position_in_block: u8,

    summary: Summary,
    block: Option<Block>,
    block_upper_bound: f64,

    incoming: Incoming,
}

impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        self.document_id == other.document_id
    }
}

impl Eq for Cursor {}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(&self.document_id, &other.document_id))
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.document_id, &other.document_id)
    }
}

impl Cursor {
    fn new<R: RelationRead>(
        index: &R,
        k1: f64,
        b: f64,
        number_of_documents: u32,
        avgdl: f64,
        token_id: u32,
        token_number_of_documents: u32,
        token_wand_document_length: u32,
        token_wand_term_frequency: u32,
        ptr_summaries: (u32, u16),
    ) -> Self
    where
        R::Page: Page<Opaque = Opaque>,
    {
        let token_upper_bound = {
            let idf = idf(number_of_documents, token_number_of_documents);
            let document_length = token_wand_document_length;
            let term_frequency = token_wand_term_frequency;
            let tf = tf(k1, b, avgdl, document_length, term_frequency);
            idf * tf
        };
        let mut incoming = Incoming::new(index, token_id, ptr_summaries);
        let summary = incoming.next(index, token_id).unwrap_or(Summary {
            min_document_id: u32::MAX,
            max_document_id: u32::MAX,
            number_of_documents: 1,
            wand_document_length: u32::MAX,
            wand_term_frequency: 0_u32,
            ptr_block: None,
        });
        let block_upper_bound = {
            let idf = idf(number_of_documents, token_number_of_documents);
            let document_length = summary.wand_document_length;
            let term_frequency = summary.wand_term_frequency;
            let tf = tf(k1, b, avgdl, document_length, term_frequency);
            idf * tf
        };
        Cursor {
            k1,
            b,
            number_of_documents,
            avgdl,
            token_id,
            token_number_of_documents,
            token_upper_bound,
            document_id: summary.min_document_id,
            position_in_block: 0,
            summary,
            block: None,
            block_upper_bound,
            incoming,
        }
    }
    fn document_id(&self) -> u32 {
        self.document_id
    }
    fn token_number_of_documents(&self) -> u32 {
        self.token_number_of_documents
    }
    fn block_max_document_id(&self) -> u32 {
        self.summary.max_document_id
    }
    fn token_upper_bound(&self) -> f64 {
        self.token_upper_bound
    }
    fn block_upper_bound(&self) -> f64 {
        self.block_upper_bound
    }
    fn seek<R: RelationRead>(&mut self, index: &R, document_id: u32)
    where
        R::Page: Page<Opaque = Opaque>,
    {
        self.seek_block(index, document_id);
        if document_id <= self.summary.min_document_id {
            self.document_id = self.summary.min_document_id;
            self.position_in_block = 0;
            return;
        }
        if document_id == self.summary.max_document_id {
            self.document_id = self.summary.max_document_id;
            self.position_in_block = self.summary.number_of_documents as u8 - 1;
            return;
        }
        let block = Block::get(
            &mut self.block,
            index,
            self.summary.min_document_id,
            self.summary.ptr_block,
        );
        (self.document_id, self.position_in_block) = 'a: {
            for i in 0..self.summary.number_of_documents {
                if block.document_ids[i as usize] >= document_id {
                    break 'a (block.document_ids[i as usize], i as u8);
                }
            }
            unreachable!()
        };
    }
    fn seek_block<R: RelationRead>(&mut self, index: &R, document_id: u32)
    where
        R::Page: Page<Opaque = Opaque>,
    {
        assert!(document_id < u32::MAX);
        debug_assert!(document_id >= self.document_id);
        while self.summary.max_document_id < document_id {
            self.summary = self.incoming.next(index, self.token_id).unwrap_or(Summary {
                min_document_id: u32::MAX,
                max_document_id: u32::MAX,
                number_of_documents: 1,
                wand_document_length: u32::MAX,
                wand_term_frequency: 0,
                ptr_block: None,
            });
            self.block = None;
            self.block_upper_bound = {
                let idf = idf(self.number_of_documents, self.token_number_of_documents);
                let document_length = self.summary.wand_document_length;
                let term_frequency = self.summary.wand_term_frequency;
                let tf = tf(self.k1, self.b, self.avgdl, document_length, term_frequency);
                idf * tf
            };
            self.document_id = self.summary.min_document_id;
            self.position_in_block = 0;
        }
    }
    fn get<R: RelationRead>(&mut self, index: &R) -> u32
    where
        R::Page: Page<Opaque = Opaque>,
    {
        let block = Block::get(
            &mut self.block,
            index,
            self.summary.min_document_id,
            self.summary.ptr_block,
        );
        block.term_frequencies[self.position_in_block as usize]
    }
}

struct Block {
    document_ids: Vec<u32>,
    term_frequencies: Vec<u32>,
}

impl Block {
    fn get<'this, R: RelationRead>(
        this: &'this mut Option<Self>,
        index: &R,
        min_document_id: u32,
        ptr_block: Option<(u32, u16)>,
    ) -> &'this Self
    where
        R::Page: Page<Opaque = Opaque>,
    {
        this.get_or_insert_with(|| {
            let block = ptr_block.expect("cursor has reached its end");
            let block_guard = index.read(block.0);
            let block_bytes = block_guard.get(block.1).expect("data corruption");
            let block_tuple = BlockTuple::deserialize_ref(block_bytes);
            let document_ids = compression::decompress_document_ids(
                min_document_id,
                block_tuple.bitwidth_document_ids(),
                block_tuple.compressed_document_ids(),
            );
            let term_frequencies = compression::decompress_term_frequencies(
                block_tuple.bitwidth_term_frequencies(),
                block_tuple.compressed_term_frequencies(),
            );
            Self {
                document_ids,
                term_frequencies,
            }
        })
    }
}

struct Incoming {
    buffer: VecDeque<Summary>,
    next: u32,
}

impl Incoming {
    fn new<R: RelationRead>(index: &R, token_id: u32, ptr_summaries: (u32, u16)) -> Self
    where
        R::Page: Page<Opaque = Opaque>,
    {
        let mut buffered = VecDeque::new();
        let incoming = 'incoming: {
            let summary_guard = index.read(ptr_summaries.0);
            for j in ptr_summaries.1..=summary_guard.len() {
                let summary_bytes = summary_guard.get(j).expect("data corruption");
                let summary_tuple = SummaryTuple::deserialize_ref(summary_bytes);
                if summary_tuple.token_id() != token_id {
                    break 'incoming u32::MAX;
                }
                buffered.push_back(Summary {
                    min_document_id: summary_tuple.min_document_id(),
                    max_document_id: summary_tuple.max_document_id(),
                    number_of_documents: summary_tuple.number_of_documents(),
                    wand_term_frequency: summary_tuple.wand_term_frequency(),
                    wand_document_length: summary_tuple.wand_document_length(),
                    ptr_block: Some(summary_tuple.wptr_block().into_inner()),
                });
            }
            summary_guard.get_opaque().next
        };
        Self {
            buffer: buffered,
            next: incoming,
        }
    }
    fn next<R: RelationRead>(&mut self, index: &R, token_id: u32) -> Option<Summary>
    where
        R::Page: Page<Opaque = Opaque>,
    {
        while self.buffer.is_empty() && self.next != u32::MAX {
            self.next = 'incoming: {
                let summary_guard = index.read(self.next);
                for j in 1..=summary_guard.len() {
                    let summary_bytes = summary_guard.get(j).expect("data corruption");
                    let summary_tuple = SummaryTuple::deserialize_ref(summary_bytes);
                    if summary_tuple.token_id() != token_id {
                        break 'incoming u32::MAX;
                    }
                    self.buffer.push_back(Summary {
                        min_document_id: summary_tuple.min_document_id(),
                        max_document_id: summary_tuple.max_document_id(),
                        number_of_documents: summary_tuple.number_of_documents(),
                        wand_term_frequency: summary_tuple.wand_term_frequency(),
                        wand_document_length: summary_tuple.wand_document_length(),
                        ptr_block: Some(summary_tuple.wptr_block().into_inner()),
                    });
                }
                summary_guard.get_opaque().next
            };
        }
        self.buffer.pop_front()
    }
}

struct Summary {
    min_document_id: u32,
    max_document_id: u32,
    number_of_documents: u32,
    wand_document_length: u32,
    wand_term_frequency: u32,
    ptr_block: Option<(u32, u16)>,
}

fn binary_heap_pop_if<T: Ord>(
    this: &mut BinaryHeap<T>,
    predicate: impl FnOnce(&T) -> bool,
) -> Option<T> {
    let top = this.peek()?;
    if predicate(top) { this.pop() } else { None }
}
