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

use crate::bm25::{Wand, length_to_fieldnorm};
use crate::tape::TapeWriter;
use crate::tuples::*;
use crate::types::Bm25IndexOptions;
use crate::{Opaque, Segment, address_documents, address_tokens, compression};
use index::relation::{Page, RelationWrite};
use index::tuples::Bool;

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
    for &(document_length, payload) in segment.documents().iter() {
        map_documents.push(tape_documents.push(DocumentTuple {
            fieldnorm: length_to_fieldnorm(document_length),
            payload,
            deleted: Bool::FALSE,
        }));
    }

    let mut map_tokens = Vec::new();
    let mut tape_tokens = TapeWriter::<_, TokenTuple>::create(index);
    let mut tape_summaries = TapeWriter::<_, SummaryTuple>::create(index);
    let mut tape_blocks = TapeWriter::<_, BlockTuple>::create(index);
    for token in segment.tokens() {
        let mut token_wand = Wand::new();
        let mut wptr_summaries = (tape_summaries.first(), 1);
        for (ordinal, block) in token.blocks().enumerate() {
            let (metadata_document_ids, compressed_document_ids) =
                compression::compress_document_ids(block.min_document_id(), &block.document_ids());
            let (metadata_term_frequencies, compressed_term_frequencies) =
                compression::compress_term_frequencies(&block.term_frequencies());
            let wptr_block = tape_blocks.push(BlockTuple {
                metadata_document_ids,
                compressed_document_ids,
                metadata_term_frequencies,
                compressed_term_frequencies,
            });
            let mut block_wand = Wand::new();
            for &(_, document_id, term_frequency) in block.internal() {
                let (document_length, _) = segment.documents()[document_id as usize];
                block_wand.push(
                    length_to_fieldnorm(document_length),
                    term_frequency,
                    k1,
                    b,
                    avgdl,
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
    let (width_1_documents, width_0_documents, depth_documents, start_documents, free_documents) =
        address_documents::write(index, &map_documents);
    let (depth_tokens, start_tokens, free_tokens) = address_tokens::write(index, &map_tokens);
    let ptr_jump = tape_jump.push(JumpTuple {
        ptr_vectors: { tape_vectors }.first(),
        number_of_documents: segment.documents().len() as u32,
        sum_of_document_lengths,
        width_1_documents,
        width_0_documents,
        depth_documents,
        start_documents,
        free_documents,
        depth_tokens,
        start_tokens,
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
