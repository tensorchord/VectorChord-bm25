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
use crate::segment::{Mapping, Record, Segment};
use crate::tape::TapeWriter;
use crate::tuples::*;
use crate::{Opaque, address_documents, address_tokens, compression};
use index::relation::{Page, RelationWrite};
use index::tuples::Bool;

pub struct Flushed {
    pub number_of_documents: u32,
    pub sum_of_document_lengths: u64,
    pub width_1_documents: u16,
    pub width_0_documents: u16,
    pub depth_documents: u32,
    pub start_documents: u32,
    pub free_documents: u32,
    pub depth_tokens: u32,
    pub start_tokens: u32,
    pub free_tokens: u32,
    pub ptr_documents: u32,
    pub ptr_tokens: u32,
    pub ptr_summaries: u32,
    pub ptr_blocks: u32,
}

pub fn flush<R: RelationWrite, D: IntoIterator<Item = Record>, M: IntoIterator<Item = Mapping>>(
    k1: f64,
    b: f64,
    index: &R,
    segment: Segment<D, M>,
) -> Flushed
where
    R::Page: Page<Opaque = Opaque>,
{
    let mut number_of_documents = 0_u32;
    let mut sum_of_document_lengths = 0_u64;
    let mut fieldnorms = Vec::new();
    let mut map_documents = Vec::new();
    let mut tape_documents = TapeWriter::<_, DocumentTuple>::create(index);
    for Record(document_length, payload) in segment.records.into_iter() {
        number_of_documents += 1;
        sum_of_document_lengths += document_length as u64;
        let fieldnorm = length_to_fieldnorm(document_length);
        fieldnorms.push(fieldnorm);
        map_documents.push(tape_documents.push(DocumentTuple {
            fieldnorm,
            payload,
            deleted: Bool::FALSE,
        }));
    }

    let avgdl = sum_of_document_lengths as f64 / number_of_documents as f64;

    let mut mappings = segment.mappings.into_iter().peekable();
    let mut map_tokens = Vec::new();
    let mut tape_tokens = TapeWriter::<_, TokenTuple>::create(index);
    let mut tape_summaries = TapeWriter::<_, SummaryTuple>::create(index);
    let mut tape_blocks = TapeWriter::<_, BlockTuple>::create(index);
    while let Some(token_id) = mappings.peek().map(|&Mapping(token_id, ..)| token_id) {
        let mut token_number_of_documents = 0_u32;
        let mut token_wand = Wand::new();
        let mut wptr_summaries = (tape_summaries.first(), 1);
        let mut ordinal = 0_usize;
        while Some(token_id) == mappings.peek().map(|&Mapping(token_id, ..)| token_id) {
            let block = {
                let func = |Mapping(i, ..): &Mapping| &token_id == i;
                let mut internal = Vec::with_capacity(128);
                for _ in 0..128 {
                    if let Some(Mapping(_, document_id, term_frequency)) = mappings.next_if(func) {
                        internal.push((document_id, term_frequency));
                    } else {
                        break;
                    }
                }
                Block { internal }
            };
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
            for &(document_id, term_frequency) in block.internal() {
                block_wand.push(
                    fieldnorms[document_id as usize],
                    term_frequency,
                    k1,
                    b,
                    avgdl,
                );
            }
            token_number_of_documents += block.number_of_documents() as u32;
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
            ordinal += 1;
        }
        map_tokens.push((
            token_id,
            tape_tokens.push(TokenTuple {
                id: token_id,
                number_of_documents: token_number_of_documents,
                wand_fieldnorm: token_wand.fieldnorm(),
                wand_term_frequency: token_wand.term_frequency(),
                wptr_summaries,
            }),
        ));
    }

    let (width_1_documents, width_0_documents, depth_documents, start_documents, free_documents) =
        address_documents::write(index, &map_documents);
    let (depth_tokens, start_tokens, free_tokens) = address_tokens::write(index, &map_tokens);

    Flushed {
        number_of_documents,
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
    }
}

struct Block {
    internal: Vec<(u32, u32)>,
}

impl Block {
    fn min_document_id(&self) -> u32 {
        self.internal[0].0
    }
    fn max_document_id(&self) -> u32 {
        let n = self.internal.len();
        self.internal[n - 1].0
    }
    fn number_of_documents(&self) -> u8 {
        self.internal.len() as u8
    }
    fn internal(&self) -> &[(u32, u32)] {
        self.internal.as_slice()
    }
    fn document_ids(&self) -> Vec<u32> {
        self.internal
            .iter()
            .map(|&(document_id, _)| document_id)
            .collect()
    }
    fn term_frequencies(&self) -> Vec<u32> {
        self.internal
            .iter()
            .map(|&(_, term_frequency)| term_frequency)
            .collect()
    }
}
