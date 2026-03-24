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

use crate::tape::TapeWriter;
use crate::tuples::{
    BlockTuple, DocumentTuple, MetaTuple, Pointer, SegmentTuple, SummaryTuple, TokenTuple,
};
use crate::types::Bm25IndexOptions;
use crate::vector::Bm25VectorBorrowed;
use crate::{Opaque, compression, guide, tf};
use index::relation::{Page, RelationWrite};
use std::collections::BTreeMap;
use std::iter::zip;

pub struct Segment {
    documents: Vec<(u32, [u16; 3])>,
    tokens: BTreeMap<u32, Vec<(u32, u32)>>,
    sum_of_document_lengths: u64,
}

impl Segment {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            tokens: BTreeMap::new(),
            sum_of_document_lengths: 0,
        }
    }
    pub fn push(&mut self, document: Bm25VectorBorrowed<'_>, payload: [u16; 3]) {
        let i = self.documents.len();
        let Ok(i) = u32::try_from(i) else {
            panic!("number of documents exceeds {}", u32::MAX - 1);
        };
        let norm = document.norm();
        self.documents.push((norm, payload));
        for (&key, &val) in zip(document.indexes(), document.values()) {
            self.tokens.entry(key).or_default().push((i, val));
        }
        self.sum_of_document_lengths += norm as u64;
    }
}

impl Default for Segment {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build<R: RelationWrite>(bm25_options: Bm25IndexOptions, index: &R, builder: Segment)
where
    R::Page: Page<Opaque = Opaque>,
{
    let k1 = bm25_options.k1;
    let b = bm25_options.b;

    let documents = builder.documents;
    let tokens = builder.tokens;
    let sum_of_document_lengths = builder.sum_of_document_lengths;

    let mut meta = TapeWriter::<_, MetaTuple>::create(index, false);
    assert_eq!(meta.first(), 0);

    let avgdl = sum_of_document_lengths as f64 / documents.len() as f64;

    let mut tape_documents = TapeWriter::<_, DocumentTuple>::create(index, false);
    let mut map_documents = Vec::new();
    for (document_id, &(document_length, payload)) in documents.iter().enumerate() {
        map_documents.push((
            document_id as u32,
            tape_documents.push(DocumentTuple {
                length: document_length,
                payload,
            }),
        ));
    }
    let length = |i: u32| documents[i as usize].0;

    let mut tape_blocks = TapeWriter::<_, BlockTuple>::create(index, false);
    let mut tape_summaries = TapeWriter::<_, SummaryTuple>::create(index, false);
    let mut tape_tokens = TapeWriter::<_, TokenTuple>::create(index, false);
    let mut map_tokens = Vec::new();
    for (&token_id, val) in tokens.iter() {
        let number_of_documents: u32 = val.len() as u32;
        let mut token_wand = Wand::new();
        let mut wptr_summaries = None;
        for block in val.chunks(128) {
            let min_document_id = block.first().unwrap().0;
            let max_document_id = block.last().unwrap().0;
            let number_of_documents = block.len() as u32;
            let document_ids = block.iter().map(|&(x, _)| x).collect::<Vec<_>>();
            let term_frequencies = block.iter().map(|&(_, x)| x).collect::<Vec<_>>();
            let (bitwidth_document_ids, compressed_document_ids) =
                compression::compress_document_ids(min_document_id, &document_ids);
            let (bitwidth_term_frequencies, compressed_term_frequencies) =
                compression::compress_term_frequencies(&term_frequencies);
            let wptr_block = tape_blocks.push(BlockTuple {
                bitwidth_document_ids,
                bitwidth_term_frequencies,
                compressed_document_ids,
                compressed_term_frequencies,
            });
            let mut block_wand = Wand::new();
            for &(document_id, term_frequency) in block {
                block_wand.push(k1, b, avgdl, length(document_id), term_frequency);
            }
            token_wand.extend(&block_wand);
            let wptr = tape_summaries.push(SummaryTuple {
                token_id,
                min_document_id,
                max_document_id,
                number_of_documents,
                wand_document_length: block_wand.document_length(),
                wand_term_frequency: block_wand.term_frequency(),
                wptr_block: Pointer::new(wptr_block),
            });
            wptr_summaries.get_or_insert(wptr);
        }
        map_tokens.push((
            token_id,
            tape_tokens.push(TokenTuple {
                number_of_documents,
                wand_document_length: token_wand.document_length(),
                wand_term_frequency: token_wand.term_frequency(),
                wptr_summaries: Pointer::new(wptr_summaries.expect("empty token")),
            }),
        ));
    }

    let mut tape_segments = TapeWriter::<_, SegmentTuple>::create(index, false);
    let wptr_segment = tape_segments.push(SegmentTuple {
        number_of_documents: documents.len() as _,
        number_of_tokens: tokens.len() as _,
        sum_of_document_lengths,
        iptr_documents: guide::write(index, &map_documents),
        iptr_tokens: guide::write(index, &map_tokens),
        sptr_summaries: { tape_summaries }.first(),
        sptr_blocks: { tape_blocks }.first(),
    });
    assert_eq!(wptr_segment.1, 1);

    meta.push(MetaTuple {
        k1,
        b,
        wptr_segment: wptr_segment.0,
    });
}

struct Wand {
    tf: f64,
    document_length: u32,
    term_frequency: u32,
}

impl Wand {
    fn new() -> Self {
        Self {
            tf: 0.0f64,
            document_length: u32::MAX,
            term_frequency: 0_u32,
        }
    }
    fn push(&mut self, k1: f64, b: f64, avgdl: f64, document_length: u32, term_frequency: u32) {
        let tf = tf(k1, b, avgdl, document_length, term_frequency);
        if self.tf < tf {
            self.tf = tf;
            self.document_length = document_length;
            self.term_frequency = term_frequency;
        }
    }
    fn extend(&mut self, other: &Self) {
        if self.tf < other.tf {
            self.tf = other.tf;
            self.document_length = other.document_length;
            self.term_frequency = other.term_frequency;
        }
    }
    fn document_length(&self) -> u32 {
        self.document_length
    }
    fn term_frequency(&self) -> u32 {
        self.term_frequency
    }
}
