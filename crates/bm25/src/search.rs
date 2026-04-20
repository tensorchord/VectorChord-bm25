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

use crate::bm25::{Cache, length_to_fieldnorm};
use crate::tape::TruncatedTapeReader;
use crate::tuples::*;
use crate::vector::{Element, Query};
use crate::{Opaque, WIDTH, address_documents, address_tokens, compression};
use always_equal::AlwaysEqual;
use index::relation::{Page, RelationRead};
use score::Score;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::iter::chain;
use std::num::NonZero;

pub fn search<R: RelationRead>(
    index: &R,
    k: NonZero<usize>,
    query: &Query,
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
    let ptr_jump = meta_tuple.ptr_jump();
    drop(meta_guard);

    let jump_guard = index.read(ptr_jump);
    let jump_bytes = jump_guard.get(1).expect("data corruption");
    let jump_tuple = JumpTuple::deserialize_ref(jump_bytes);

    let sum_of_document_lengths = jump_tuple.sum_of_document_lengths();
    let number_of_documents = jump_tuple.number_of_documents();
    let avgdl = sum_of_document_lengths as f64 / number_of_documents as f64;

    let mut tokens = Vec::new();
    for &key in query.iter() {
        let Some((token_guard, token_i)) = address_tokens::read(
            index,
            jump_tuple.depth_tokens(),
            jump_tuple.start_tokens(),
            key,
        ) else {
            continue;
        };
        let token_bytes = token_guard.get(token_i).expect("data corruption");
        let token_tuple = TokenTuple::deserialize_ref(token_bytes);
        tokens.push(Token {
            id: key,
            number_of_documents: token_tuple.number_of_documents(),
            wand_fieldnorm: token_tuple.wand_fieldnorm(),
            wand_term_frequency: token_tuple.wand_term_frequency(),
            wptr_summaries: token_tuple.wptr_summaries(),
            bm25: Cache::new(
                number_of_documents,
                token_tuple.number_of_documents(),
                k1,
                b,
                avgdl,
            ),
        });
    }

    let mut results = Results::<[u16; 3]>::new(k, 0.0);

    {
        let first = jump_tuple.ptr_vectors();
        assert!(first != u32::MAX);
        let mut elements = Vec::new();
        let mut current = first;
        while current != u32::MAX {
            let vector_guard = index.read(current);
            for i in 1..=vector_guard.len() {
                let vector_bytes = vector_guard.get(i).expect("data corruption");
                let vector_tuple = VectorTuple::deserialize_ref(vector_bytes);
                match vector_tuple {
                    VectorTupleReader::_2(_) => {
                        elements.clear();
                    }
                    VectorTupleReader::_1(vector_tuple) => {
                        elements.extend(vector_tuple.elements());
                    }
                    VectorTupleReader::_0(vector_tuple) => {
                        if !bool::from(vector_tuple.deleted()) {
                            let payload = vector_tuple.payload();
                            if filter(payload) {
                                elements.extend(vector_tuple.elements());
                                let document = std::mem::take(&mut elements);
                                let fieldnorm = length_to_fieldnorm(vector_tuple.length());
                                let mut result = 0.0;
                                for &Element { key, value } in document.iter() {
                                    if let Ok(i) = tokens.binary_search_by_key(&key, |t| t.id) {
                                        let token = &tokens[i];
                                        let term_frequency = value;
                                        result += token.bm25.evaluate(fieldnorm, term_frequency);
                                    }
                                }
                                results.push(result, payload);
                            }
                        }
                    }
                }
            }
            current = vector_guard.get_opaque().next;
        }
    }

    let mut cursors = Vec::new();
    for token in tokens {
        cursors.push(Box::new(Cursor::new(
            index,
            token.number_of_documents,
            token.wand_fieldnorm,
            token.wand_term_frequency,
            token.wptr_summaries,
            token.bm25,
        )));
    }

    let mut tail = Vec::<Box<Cursor>>::new();
    let mut head = BinaryHeap::from(cursors);
    'main: loop {
        let lead = 'lead: {
            let mut sum = 0.0f64;
            for cursor in tail.iter() {
                sum += cursor.token_upper_bound();
            }
            while let Some(cursor) = head.pop() {
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
        while let Some(cursor) =
            binary_heap_pop_if(&mut head, |cursor| document_id == cursor.document_id())
        {
            lead.push(cursor);
        }
        {
            let mut failures = tail.extract_if(.., |cursor| {
                cursor.seek_block(index, document_id);
                document_id < cursor.document_id()
            });
            if let Some(failure) = failures.next() {
                for cursor in lead {
                    head.push(cursor);
                }
                head.push(failure);
                for failure in failures {
                    head.push(failure);
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
                        head.push(cursor);
                    }
                    head.push(failure);
                    continue 'main;
                }
            };
            let (document_guard, document_i) = address_documents::read(
                index,
                jump_tuple.width_1_documents(),
                jump_tuple.width_0_documents(),
                jump_tuple.depth_documents(),
                jump_tuple.start_documents(),
                document_id,
            )
            .expect("data corruption");
            let document_bytes = document_guard.get(document_i).expect("data corruption");
            let document_tuple = DocumentTuple::deserialize_ref(document_bytes);
            let fieldnorm = document_tuple.fieldnorm();
            let payload = document_tuple.payload();
            if filter(payload) {
                let mut result = 0.0;
                for cursor in chain(tail.iter_mut(), lead.iter_mut()) {
                    let term_frequency = cursor.get(index);
                    result += cursor.bm25().evaluate(fieldnorm, term_frequency);
                }
                results.push(result, payload);
            }
            for mut cursor in chain(tail, lead) {
                cursor.seek(index, 1 + document_id);
                head.push(cursor);
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
                    .map(|cursor| cursor.document_id())
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
            head.push(cursor);
            for cursor in lead.into_iter() {
                head.push(cursor);
            }
        }
    }
    results.into_sorted_vec()
}

struct Results<T> {
    limit: NonZero<usize>,
    threshold: Score,
    internal: BinaryHeap<(Reverse<Score>, AlwaysEqual<T>)>,
}

impl<T> Results<T> {
    fn new(limit: NonZero<usize>, threshold: f64) -> Self {
        Self {
            limit,
            threshold: Score::from_f64(threshold),
            internal: BinaryHeap::new(),
        }
    }
    fn threshold(&self) -> f64 {
        self.threshold.to_f64()
    }
    fn push(&mut self, key: f64, value: T) {
        self.internal
            .push((Reverse(Score::from_f64(key)), AlwaysEqual(value)));
        if self.internal.len() > self.limit.get() {
            self.internal.pop();
        }
        if self.internal.len() == self.limit.get() {
            self.threshold = self.threshold.max(self.internal.peek().unwrap().0.0);
        }
    }
    fn into_sorted_vec(self) -> Vec<(Reverse<Score>, AlwaysEqual<T>)> {
        self.internal.into_sorted_vec()
    }
}

struct Cursor {
    bm25: Cache,
    token_upper_bound: f64,

    document_id: u32,
    position_in_block: u8,

    incoming: TruncatedTapeReader<Summary>,

    summary: Summary,
    block_upper_bound: f64,
    filled: bool,
    block: Block,
}

impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&other.document_id, &self.document_id)
    }
}

impl Eq for Cursor {}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(&other.document_id, &self.document_id))
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&other.document_id, &self.document_id)
    }
}

impl Cursor {
    fn new<R: RelationRead>(
        index: &R,
        token_number_of_documents: u32,
        token_wand_fieldnorm: u8,
        token_wand_term_frequency: u32,
        wptr_summaries: (u32, u16),
        bm25: Cache,
    ) -> Self
    where
        R::Page: Page<Opaque = Opaque>,
    {
        let token_upper_bound = bm25.evaluate(token_wand_fieldnorm, token_wand_term_frequency);
        let mut incoming = TruncatedTapeReader::new(
            index,
            wptr_summaries,
            |bytes| {
                let summary_tuple = SummaryTuple::deserialize_ref(bytes);
                Summary {
                    min_document_id: summary_tuple.min_document_id(),
                    max_document_id: summary_tuple.max_document_id(),
                    number_of_documents: summary_tuple.number_of_documents(),
                    wand_fieldnorm: summary_tuple.wand_fieldnorm(),
                    wand_term_frequency: summary_tuple.wand_term_frequency(),
                    wptr_block: summary_tuple.wptr_block().into_inner(),
                }
            },
            token_number_of_documents.div_ceil(128),
        );
        let summary = next_summary(&mut incoming, index);
        let block_upper_bound = bm25.evaluate(summary.wand_fieldnorm, summary.wand_term_frequency);
        Cursor {
            bm25,
            token_upper_bound,
            document_id: summary.min_document_id,
            position_in_block: 0,
            summary,
            block_upper_bound,
            filled: false,
            block: Block {
                document_ids: compression::Decompressed::new(),
                term_frequencies: compression::Decompressed::new(),
            },
            incoming,
        }
    }
    fn bm25(&self) -> &Cache {
        &self.bm25
    }
    fn token_upper_bound(&self) -> f64 {
        self.token_upper_bound
    }
    fn document_id(&self) -> u32 {
        self.document_id
    }
    fn block_max_document_id(&self) -> u32 {
        self.summary.max_document_id
    }
    fn block_upper_bound(&self) -> f64 {
        self.block_upper_bound
    }
    fn seek_block<R: RelationRead>(&mut self, index: &R, document_id: u32)
    where
        R::Page: Page<Opaque = Opaque>,
    {
        assert!(document_id < u32::MAX);
        debug_assert!(document_id >= self.document_id);
        if document_id <= self.summary.max_document_id {
            return;
        }
        while self.summary.max_document_id < document_id {
            self.summary = next_summary(&mut self.incoming, index);
        }
        self.document_id = self.summary.min_document_id;
        self.position_in_block = 0;
        self.block_upper_bound = self.bm25().evaluate(
            self.summary.wand_fieldnorm,
            self.summary.wand_term_frequency,
        );
        self.filled = false;
    }
    fn seek<R: RelationRead>(&mut self, index: &R, document_id: u32)
    where
        R::Page: Page<Opaque = Opaque>,
    {
        self.seek_block(index, document_id);
        if document_id <= self.document_id {
            return;
        }
        if document_id == self.summary.max_document_id {
            self.document_id = self.summary.max_document_id;
            self.position_in_block = self.summary.number_of_documents - 1;
            return;
        }
        if !self.filled {
            fill_block(
                &mut self.block,
                index,
                self.summary.min_document_id,
                self.summary.wptr_block,
            );
            self.filled = true;
        }
        (self.document_id, self.position_in_block) = {
            let document_ids = self.block.document_ids.as_slice();
            let i = if document_id == self.document_id + 1 {
                self.position_in_block + 1
            } else {
                let start = self.position_in_block + 1;
                let (Ok(delta) | Err(delta)) =
                    document_ids[start as usize..].binary_search(&document_id);
                start + delta as u8
            };
            (document_ids[i as usize], i)
        };
    }
    fn get<R: RelationRead>(&mut self, index: &R) -> u32
    where
        R::Page: Page<Opaque = Opaque>,
    {
        if !self.filled {
            fill_block(
                &mut self.block,
                index,
                self.summary.min_document_id,
                self.summary.wptr_block,
            );
            self.filled = true;
        }
        self.block.term_frequencies.as_slice()[self.position_in_block as usize]
    }
}

fn next_summary<R: RelationRead>(incoming: &mut TruncatedTapeReader<Summary>, index: &R) -> Summary
where
    R::Page: Page<Opaque = Opaque>,
{
    incoming.next(index).unwrap_or(Summary {
        min_document_id: u32::MAX,
        max_document_id: u32::MAX,
        number_of_documents: 1,
        wand_fieldnorm: u8::MAX,
        wand_term_frequency: 0_u32,
        wptr_block: (u32::MAX, 0),
    })
}

fn fill_block<R: RelationRead>(
    block: &mut Block,
    index: &R,
    min_document_id: u32,
    ptr_block: (u32, u16),
) {
    let block_guard = index.read(ptr_block.0);
    let block_bytes = block_guard.get(ptr_block.1).expect("data corruption");
    let block_tuple = BlockTuple::deserialize_ref(block_bytes);
    compression::decompress_document_ids(
        min_document_id,
        block_tuple.bitwidth_document_ids(),
        block_tuple.compressed_document_ids(),
        &mut block.document_ids,
    );
    compression::decompress_term_frequencies(
        block_tuple.bitwidth_term_frequencies(),
        block_tuple.compressed_term_frequencies(),
        &mut block.term_frequencies,
    );
}

struct Token {
    id: [u8; WIDTH],
    number_of_documents: u32,
    wand_fieldnorm: u8,
    wand_term_frequency: u32,
    wptr_summaries: (u32, u16),
    bm25: Cache,
}

struct Summary {
    min_document_id: u32,
    max_document_id: u32,
    number_of_documents: u8,
    wand_fieldnorm: u8,
    wand_term_frequency: u32,
    wptr_block: (u32, u16),
}

struct Block {
    document_ids: compression::Decompressed,
    term_frequencies: compression::Decompressed,
}

// Emulate unstable library feature `binary_heap_pop_if`.
// See https://github.com/rust-lang/rust/issues/151828.

fn binary_heap_pop_if<T: Ord>(
    this: &mut BinaryHeap<T>,
    predicate: impl FnOnce(&T) -> bool,
) -> Option<T> {
    let top = this.peek()?;
    if predicate(top) { this.pop() } else { None }
}
