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

use crate::segment::Segment;
use crate::tape::TapeWriter;
use crate::tuples::{
    BlockTuple, DocumentTuple, JumpTuple, MetaTuple, Pointer, SummaryTuple, TokenTuple, VectorTuple,
};
use crate::types::Bm25IndexOptions;
use crate::{Opaque, compression, tf, tree};
use index::relation::{Page, RelationWrite};
use index::tuples::Bool;

pub fn build<R: RelationWrite>(bm25_options: Bm25IndexOptions, index: &R, builder: Segment)
where
    R::Page: Page<Opaque = Opaque>,
{
    let k1 = bm25_options.k1;
    let b = bm25_options.b;

    let documents = builder.documents;
    let tokens = builder.tokens;
    let sum_of_document_lengths = documents.iter().map(|&(norm, _)| norm as u64).sum();

    let mut meta = TapeWriter::<_, MetaTuple>::create(index);
    assert_eq!(meta.first(), 0);

    let avgdl = sum_of_document_lengths as f64 / documents.len() as f64;

    let mut tape_documents = TapeWriter::<_, DocumentTuple>::create(index);
    let mut map_documents = Vec::new();
    for (document_id, &(document_length, payload)) in documents.iter().enumerate() {
        map_documents.push((
            document_id as u32,
            tape_documents.push(DocumentTuple {
                id: document_id as u32,
                length: document_length,
                payload,
                deleted: Bool::FALSE,
            }),
        ));
    }
    let length = |i: u32| documents[i as usize].0;

    let mut tape_blocks = TapeWriter::<_, BlockTuple>::create(index);
    let mut tape_summaries = TapeWriter::<_, SummaryTuple>::create(index);
    let mut tape_tokens = TapeWriter::<_, TokenTuple>::create(index);
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
                id: token_id,
                number_of_documents,
                wand_document_length: token_wand.document_length(),
                wand_term_frequency: token_wand.term_frequency(),
                wptr_summaries: Pointer::new(wptr_summaries.expect("empty token")),
            }),
        ));
    }

    let tape_vectors = TapeWriter::<_, VectorTuple>::create(index);

    let mut tape_jump = TapeWriter::<_, JumpTuple>::create(index);
    let (root_documents, depth_documents, free_documents) = tree::write(index, &map_documents);
    let (root_tokens, depth_tokens, free_tokens) = tree::write(index, &map_tokens);
    let ptr_jump = tape_jump.push(JumpTuple {
        ptr_vectors: { tape_vectors }.first(),
        number_of_documents: documents.len() as _,
        number_of_tokens: tokens.len() as _,
        sum_of_document_lengths,
        root_documents,
        depth_documents,
        free_documents,
        root_tokens,
        depth_tokens,
        free_tokens,
        ptr_documents: { tape_documents }.first(),
        ptr_tokens: { tape_tokens }.first(),
        ptr_summaries: { tape_summaries }.first(),
        ptr_blocks: { tape_blocks }.first(),
    });
    assert_eq!(ptr_jump.1, 1);

    let tape_lock = TapeWriter::<_, ()>::create(index);

    meta.push(MetaTuple {
        k1,
        b,
        ptr_lock: { tape_lock }.first(),
        ptr_jump: ptr_jump.0,
    });
}

pub(crate) struct Wand {
    tf: f64,
    document_length: u32,
    term_frequency: u32,
}

impl Wand {
    pub(crate) fn new() -> Self {
        Self {
            tf: 0.0f64,
            document_length: u32::MAX,
            term_frequency: 0_u32,
        }
    }
    pub(crate) fn push(
        &mut self,
        k1: f64,
        b: f64,
        avgdl: f64,
        document_length: u32,
        term_frequency: u32,
    ) {
        let tf = tf(k1, b, avgdl, document_length, term_frequency);
        if self.tf < tf {
            self.tf = tf;
            self.document_length = document_length;
            self.term_frequency = term_frequency;
        }
    }
    pub(crate) fn extend(&mut self, other: &Self) {
        if self.tf < other.tf {
            self.tf = other.tf;
            self.document_length = other.document_length;
            self.term_frequency = other.term_frequency;
        }
    }
    pub(crate) fn document_length(&self) -> u32 {
        self.document_length
    }
    pub(crate) fn term_frequency(&self) -> u32 {
        self.term_frequency
    }
}
