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
use crate::segment::Collector0;
use crate::tape::{TapeReader, TapeWriter};
use crate::tuples::*;
use crate::vector::Document;
use crate::{Opaque, WIDTH, address_documents, address_tokens, compression};
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};
use index::tuples::Bool;

pub fn maintain<R: RelationRead + RelationWrite>(index: &R, _check: impl Fn())
where
    R::Page: Page<Opaque = Opaque>,
{
    let meta_guard = index.read(0);
    let meta_bytes = meta_guard.get(1).expect("data corruption");
    let meta_tuple = MetaTuple::deserialize_ref(meta_bytes);
    let k1 = meta_tuple.k1();
    let b = meta_tuple.b();
    let ptr_lock = meta_tuple.ptr_lock();
    let ptr_jump = meta_tuple.ptr_jump();
    drop(meta_guard);

    let _lock_guard = index.write(ptr_lock);

    let mut collector = Collector0::new();

    let jump_guard = index.read(ptr_jump);
    let jump_bytes = jump_guard.get(1).expect("data corruption");
    let jump_tuple = JumpTuple::deserialize_ref(jump_bytes);

    {
        let first = jump_tuple.ptr_documents();
        assert!(first != u32::MAX);
        let mut current = first;
        while current != u32::MAX {
            let guard = index.read(current);
            for i in 1..=guard.len() {
                let bytes = guard.get(i).expect("data corruption");
                let tuple = DocumentTuple::deserialize_ref(bytes);
                collector.add_document((!bool::from(tuple.deleted())).then_some(tuple.payload()));
            }
            current = guard.get_opaque().next;
        }
    }

    let mut collector = collector.finish();

    {
        let mut tape_tokens = TapeReader::new(jump_tuple.ptr_tokens(), |bytes| {
            let token_tuple = TokenTuple::deserialize_ref(bytes);
            Token {
                id: token_tuple.id(),
                number_of_documents: token_tuple.number_of_documents(),
            }
        });
        let mut tape_summaries = TapeReader::new(jump_tuple.ptr_summaries(), |bytes| {
            let summary_tuple = SummaryTuple::deserialize_ref(bytes);
            Summary {
                min_document_id: summary_tuple.min_document_id(),
                number_of_documents: summary_tuple.number_of_documents(),
            }
        });
        let mut tape_blocks = TapeReader::new(jump_tuple.ptr_blocks(), |bytes| {
            let block_tuple = BlockTuple::deserialize_ref(bytes);
            Block {
                metadata_document_ids: block_tuple.metadata_document_ids(),
                compressed_document_ids: block_tuple.compressed_document_ids().to_vec(),
                metadata_term_frequencies: block_tuple.metadata_term_frequencies(),
                compressed_term_frequencies: block_tuple.compressed_term_frequencies().to_vec(),
            }
        });
        while let Some(token) = tape_tokens.next(index) {
            for _ in 0..token.number_of_documents.div_ceil(128) {
                let summary = tape_summaries.next(index).expect("data corruption");
                let block = tape_blocks.next(index).expect("data corruption");
                let mut document_ids = compression::Decompressed::new();
                compression::decompress_document_ids(
                    summary.min_document_id,
                    block.metadata_document_ids,
                    &block.compressed_document_ids,
                    &mut document_ids,
                );
                let mut term_frequencies = compression::Decompressed::new();
                compression::decompress_term_frequencies(
                    block.metadata_term_frequencies,
                    &block.compressed_term_frequencies,
                    &mut term_frequencies,
                );
                for i in 0..summary.number_of_documents {
                    let document_id = document_ids.as_slice()[i as usize];
                    let term_frequency = term_frequencies.as_slice()[i as usize];
                    collector.add_element(token.id, document_id, term_frequency);
                }
            }
        }
        assert!(tape_summaries.next(index).is_none(), "data corruption");
        assert!(tape_blocks.next(index).is_none(), "data corruption");
    }

    let mut collector = collector.finish();

    let ptr_vectors = {
        let first = jump_tuple.ptr_vectors();
        assert!(first != u32::MAX);
        let mut state = None;
        let mut current = first;
        let mut head = loop {
            let read = index.read(current);
            if read.get_opaque().next == u32::MAX {
                drop(read);
                let write = index.write(current);
                for i in 1..=write.len() {
                    let vector_bytes = write.get(i).expect("data corruption");
                    let vector_tuple = VectorTuple::deserialize_ref(vector_bytes);
                    match vector_tuple {
                        VectorTupleReader::_2(_) => {
                            state = Some(Vec::new());
                        }
                        VectorTupleReader::_1(vector_tuple) => {
                            if let Some(internal) = state.as_mut() {
                                internal.extend(vector_tuple.elements());
                            } else {
                                panic!("data corruption");
                            }
                        }
                        VectorTupleReader::_0(vector_tuple) => {
                            if let Some(mut internal) = state.take() {
                                if !bool::from(vector_tuple.deleted()) {
                                    internal.extend(vector_tuple.elements());
                                    let document = Document::new(internal);
                                    collector.push(&document, vector_tuple.payload());
                                }
                            } else {
                                panic!("data corruption");
                            }
                        }
                    }
                }
                if write.get_opaque().next == u32::MAX {
                    break write;
                }
                current = write.get_opaque().next;
            } else {
                for i in 1..=read.len() {
                    let vector_bytes = read.get(i).expect("data corruption");
                    let vector_tuple = VectorTuple::deserialize_ref(vector_bytes);
                    match vector_tuple {
                        VectorTupleReader::_2(_) => {
                            state = Some(Vec::new());
                        }
                        VectorTupleReader::_1(vector_tuple) => {
                            if let Some(internal) = state.as_mut() {
                                internal.extend(vector_tuple.elements());
                            } else {
                                panic!("data corruption");
                            }
                        }
                        VectorTupleReader::_0(vector_tuple) => {
                            if let Some(mut internal) = state.take() {
                                if !bool::from(vector_tuple.deleted()) {
                                    internal.extend(vector_tuple.elements());
                                    let document = Document::new(internal);
                                    collector.push(&document, vector_tuple.payload());
                                }
                            } else {
                                panic!("data corruption");
                            }
                        }
                    }
                }
                current = read.get_opaque().next;
            }
        };
        let fresh = index.alloc(Opaque {
            next: u32::MAX,
            flags: 0,
        });
        head.get_opaque_mut().next = fresh.id();
        fresh.id()
    };

    drop(jump_guard);

    let segment = collector.finish();

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

    let mut jump_guard = index.write(ptr_jump);
    let jump_bytes = jump_guard.get_mut(1).expect("data corruption");
    let mut jump_tuple = JumpTuple::deserialize_mut(jump_bytes);

    let recycle = [
        (*jump_tuple.ptr_vectors(), ptr_vectors),
        (*jump_tuple.free_documents(), u32::MAX),
        (*jump_tuple.free_tokens(), u32::MAX),
        (*jump_tuple.ptr_documents(), u32::MAX),
        (*jump_tuple.ptr_tokens(), u32::MAX),
        (*jump_tuple.ptr_summaries(), u32::MAX),
        (*jump_tuple.ptr_blocks(), u32::MAX),
    ];

    let (width_1_documents, width_0_documents, depth_documents, start_documents, free_documents) =
        address_documents::write(index, &map_documents);
    let (depth_tokens, start_tokens, free_tokens) = address_tokens::write(index, &map_tokens);
    *jump_tuple.ptr_vectors() = ptr_vectors;
    *jump_tuple.number_of_documents() = segment.documents().len() as u32;
    *jump_tuple.sum_of_document_lengths() = sum_of_document_lengths;
    *jump_tuple.width_1_documents() = width_1_documents;
    *jump_tuple.width_0_documents() = width_0_documents;
    *jump_tuple.depth_documents() = depth_documents;
    *jump_tuple.start_documents() = start_documents;
    *jump_tuple.free_documents() = free_documents;
    *jump_tuple.depth_tokens() = depth_tokens;
    *jump_tuple.start_tokens() = start_tokens;
    *jump_tuple.free_tokens() = free_tokens;
    *jump_tuple.ptr_documents() = { tape_documents }.first();
    *jump_tuple.ptr_tokens() = { tape_tokens }.first();
    *jump_tuple.ptr_summaries() = { tape_summaries }.first();
    *jump_tuple.ptr_blocks() = { tape_blocks }.first();

    drop(jump_guard);

    for (first, end) in recycle {
        let mut current = first;
        while current != end && current != u32::MAX {
            let guard = index.write(current);
            let next = guard.get_opaque().next;
            index.free(guard);
            current = next;
        }
    }
    index.vacuum();
}

struct Token {
    id: [u8; WIDTH],
    number_of_documents: u32,
}

struct Summary {
    min_document_id: u32,
    number_of_documents: u8,
}

struct Block {
    metadata_document_ids: u8,
    compressed_document_ids: Vec<u8>,
    metadata_term_frequencies: u8,
    compressed_term_frequencies: Vec<u8>,
}
