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

use crate::segment::Wand;
use crate::tape::TapeWriter;
use crate::tuples::{
    BlockTuple, DocumentTuple, JumpTuple, MetaTuple, Pointer, SummaryTuple, TokenTuple, VectorTuple,
};
use crate::types::Bm25IndexOptions;
use crate::{Opaque, Segment, compression, length_to_fieldnorm, tree};
use index::relation::{Page, RelationWrite};
use index::tuples::Bool;
use std::convert::identity;

pub fn build<R: RelationWrite>(bm25_options: Bm25IndexOptions, index: &R, segment: Segment)
where
    R::Page: Page<Opaque = Opaque>,
{
    let k1 = bm25_options.k1;
    let b = bm25_options.b;

    let mut meta = TapeWriter::<_, MetaTuple>::create(index);
    assert_eq!(meta.first(), 0);

    let sum_of_document_lengths = segment
        .documents()
        .iter()
        .map(|&(document_length, _)| document_length as u64)
        .sum();

    let avgdl = sum_of_document_lengths as f64 / segment.documents().len() as f64;

    let mut map_documents = Vec::new();
    let mut tape_documents = TapeWriter::<_, DocumentTuple>::create(index);
    for (document_id, &(document_length, payload)) in segment.documents().iter().enumerate() {
        map_documents.push((
            document_id as u32,
            tape_documents.push(DocumentTuple {
                id: document_id as u32,
                fieldnorm: length_to_fieldnorm(document_length),
                payload,
                deleted: Bool::FALSE,
            }),
        ));
    }

    let mut map_tokens = Vec::new();
    let mut tape_tokens = TapeWriter::<_, TokenTuple>::create(index);
    let mut tape_summaries = TapeWriter::<_, SummaryTuple>::create(index);
    let mut tape_blocks = TapeWriter::<_, BlockTuple>::create(index);
    for token in segment.tokens() {
        let mut token_wand = Wand::new();
        let mut wptr_summaries = (tape_summaries.first(), 1);
        for (ordinal, block) in token.blocks().enumerate() {
            let (bitwidth_document_ids, compressed_document_ids) =
                compression::compress_document_ids(block.min_document_id(), &block.document_ids());
            let (bitwidth_term_frequencies, compressed_term_frequencies) =
                compression::compress_term_frequencies(&block.term_frequencies());
            let wptr_block = tape_blocks.push(BlockTuple {
                bitwidth_document_ids,
                bitwidth_term_frequencies,
                compressed_document_ids,
                compressed_term_frequencies,
            });
            let mut block_wand = Wand::new();
            for &(_, document_id, term_frequency) in block.internal() {
                let (document_length, _) = segment.documents()[document_id as usize];
                block_wand.push(
                    k1,
                    b,
                    avgdl,
                    length_to_fieldnorm(document_length),
                    term_frequency,
                );
            }
            token_wand.extend(&block_wand);
            let wptr_summary = tape_summaries.push(SummaryTuple {
                min_document_id: block.min_document_id(),
                max_document_id: block.max_document_id(),
                number_of_documents: block.number_of_documents(),
                wand_fieldnorm: block_wand.fieldnorm(),
                wand_term_frequency: block_wand.term_frequency(),
                wptr_block: Pointer::new(wptr_block),
            });
            if ordinal == 0 {
                wptr_summaries = wptr_summary;
            }
        }
        map_tokens.push((
            token.id(),
            tape_tokens.push(TokenTuple {
                id: token.id(),
                number_of_documents: token.number_of_documents(),
                wand_fieldnorm: token_wand.fieldnorm(),
                wand_term_frequency: token_wand.term_frequency(),
                wptr_summaries,
            }),
        ));
    }

    let tape_vectors = TapeWriter::<_, VectorTuple>::create(index);

    let mut tape_jump = TapeWriter::<_, JumpTuple>::create(index);
    let (root_documents, depth_documents, free_documents) =
        tree::write(index, &map_documents, u32::to_ne_bytes);
    let (root_tokens, depth_tokens, free_tokens) = tree::write(index, &map_tokens, identity);
    let ptr_jump = tape_jump.push(JumpTuple {
        ptr_vectors: { tape_vectors }.first(),
        number_of_documents: segment.documents().len() as u32,
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
