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

use crate::build::Wand;
use crate::tape::TapeWriter;
use crate::tuples::*;
use crate::vector::Bm25VectorOwned;
use crate::{Opaque, Segment, compression, tree};
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

    let mut builder = Segment::new();

    let jump_guard = index.read(ptr_jump);
    let jump_bytes = jump_guard.get(1).expect("data corruption");
    let jump_tuple = JumpTuple::deserialize_ref(jump_bytes);

    let mut documents = Vec::new();

    {
        let first = jump_tuple.ptr_documents();
        assert!(first != u32::MAX);
        let mut current = first;
        while current != u32::MAX {
            let guard = index.read(current);
            for i in 1..=guard.len() {
                let bytes = guard.get(i).expect("data corruption");
                let tuple = DocumentTuple::deserialize_ref(bytes);
                if !bool::from(tuple.deleted()) {
                    documents.push(Some(builder.documents.len() as u32));
                    builder.documents.push((tuple.length(), tuple.payload()));
                } else {
                    documents.push(None);
                }
            }
            current = guard.get_opaque().next;
        }
    }

    {
        let first = jump_tuple.ptr_summaries();
        assert!(first != u32::MAX);
        let mut current = first;
        while current != u32::MAX {
            let guard = index.read(current);
            for i in 1..=guard.len() {
                let summary_bytes = guard.get(i).expect("data corruption");
                let summary_tuple = SummaryTuple::deserialize_ref(summary_bytes);
                let block_guard = index.read(summary_tuple.wptr_block().into_inner().0);
                let block_bytes = block_guard
                    .get(summary_tuple.wptr_block().into_inner().1)
                    .expect("data corruption");
                let block_tuple = BlockTuple::deserialize_ref(block_bytes);
                let document_ids = compression::decompress_document_ids(
                    summary_tuple.min_document_id(),
                    block_tuple.bitwidth_document_ids(),
                    block_tuple.compressed_document_ids(),
                );
                let term_frequencies = compression::decompress_term_frequencies(
                    block_tuple.bitwidth_term_frequencies(),
                    block_tuple.compressed_term_frequencies(),
                );
                let mut sequence = Vec::new();
                for i in 0..summary_tuple.number_of_documents() {
                    let old_document_id = document_ids[i as usize];
                    let term_frequency = term_frequencies[i as usize];
                    if let Some(document_id) = documents[old_document_id as usize] {
                        sequence.push((document_id, term_frequency));
                    }
                }
                if !sequence.is_empty() {
                    builder
                        .tokens
                        .entry(summary_tuple.token_id())
                        .or_default()
                        .extend(sequence);
                }
            }
            current = guard.get_opaque().next;
        }
    }

    let ptr_vectors = {
        let first = jump_tuple.ptr_vectors();
        assert!(first != u32::MAX);
        let mut indexes = Vec::new();
        let mut values = Vec::new();
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
                            indexes.clear();
                            values.clear();
                        }
                        VectorTupleReader::_1(vector_tuple) => {
                            indexes
                                .extend(vector_tuple.elements().iter().map(|p| p.into_inner().0));
                            values.extend(vector_tuple.elements().iter().map(|p| p.into_inner().1));
                        }
                        VectorTupleReader::_0(vector_tuple) => {
                            indexes
                                .extend(vector_tuple.elements().iter().map(|p| p.into_inner().0));
                            values.extend(vector_tuple.elements().iter().map(|p| p.into_inner().1));
                            if !bool::from(vector_tuple.deleted()) {
                                let document = Bm25VectorOwned::new(
                                    std::mem::take(&mut indexes),
                                    std::mem::take(&mut values),
                                );
                                builder.push(document.as_borrowed(), vector_tuple.payload());
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
                            indexes.clear();
                            values.clear();
                        }
                        VectorTupleReader::_1(vector_tuple) => {
                            indexes
                                .extend(vector_tuple.elements().iter().map(|p| p.into_inner().0));
                            values.extend(vector_tuple.elements().iter().map(|p| p.into_inner().1));
                        }
                        VectorTupleReader::_0(vector_tuple) => {
                            indexes
                                .extend(vector_tuple.elements().iter().map(|p| p.into_inner().0));
                            values.extend(vector_tuple.elements().iter().map(|p| p.into_inner().1));
                            if !bool::from(vector_tuple.deleted()) {
                                let document = Bm25VectorOwned::new(
                                    std::mem::take(&mut indexes),
                                    std::mem::take(&mut values),
                                );
                                builder.push(document.as_borrowed(), vector_tuple.payload());
                            }
                        }
                    }
                }
                current = read.get_opaque().next;
            }
        };
        let fresh = index.alloc(Opaque {
            next: u32::MAX,
            index: 0,
        });
        head.get_opaque_mut().next = fresh.id();
        fresh.id()
    };

    drop(jump_guard);

    let documents = builder.documents;
    let tokens = builder.tokens;
    let sum_of_document_lengths = documents.iter().map(|&(norm, _)| norm as u64).sum();

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

    let (root_documents, depth_documents, free_documents) = tree::write(index, &map_documents);
    let (root_tokens, depth_tokens, free_tokens) = tree::write(index, &map_tokens);
    *jump_tuple.ptr_vectors() = ptr_vectors;
    *jump_tuple.number_of_documents() = documents.len() as _;
    *jump_tuple.number_of_tokens() = tokens.len() as _;
    *jump_tuple.sum_of_document_lengths() = sum_of_document_lengths;
    *jump_tuple.root_documents() = root_documents;
    *jump_tuple.depth_documents() = depth_documents;
    *jump_tuple.free_documents() = free_documents;
    *jump_tuple.root_tokens() = root_tokens;
    *jump_tuple.depth_tokens() = depth_tokens;
    *jump_tuple.free_tokens() = free_tokens;
    *jump_tuple.ptr_documents() = { tape_documents }.first();
    *jump_tuple.ptr_tokens() = { tape_tokens }.first();
    *jump_tuple.ptr_summaries() = { tape_summaries }.first();
    *jump_tuple.ptr_blocks() = { tape_blocks }.first();

    drop(jump_guard);

    let mut list = Vec::new();
    for (first, end) in recycle {
        let mut current = first;
        while current != end && current != u32::MAX {
            list.push(current);
            current = index.read(current).get_opaque().next;
        }
    }
    index.bulkfree(&list);
}
