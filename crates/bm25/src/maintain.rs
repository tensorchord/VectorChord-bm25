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

use crate::io::{MappingsWriter, RecordsWriter, handle_io_error};
use crate::segment::{Mapping, Record};
use crate::tape::TapeReader;
use crate::tuples::*;
use crate::vector::Document;
use crate::{Opaque, WIDTH, compression};
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};
use std::fs::File;
use std::io::BufWriter;
use zerocopy::{FromBytes, IntoBytes};

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

    let tempdir = handle_io_error(tempfile::tempdir());

    let mut relabel = BufWriter::with_capacity(16 * 1024, handle_io_error(tempfile::tempfile()));
    let mut records_writer = crate::io::records_writer(tempdir.path(), 0);

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
                add_document(
                    &mut relabel,
                    &mut records_writer,
                    (!bool::from(tuple.deleted())).then_some(tuple.payload()),
                );
            }
            current = guard.get_opaque().next;
        }
    }

    let relabel = handle_io_error(relabel.into_inner().map_err(|e| e.into_error()));
    let relabel_memmap = if handle_io_error(relabel.metadata()).len() != 0 {
        #[allow(unsafe_code)]
        Some(unsafe { handle_io_error(memmap2::Mmap::map(&relabel)) })
    } else {
        None
    };
    let relabel_slice: &[u32] = if let Some(memmap) = relabel_memmap.as_ref() {
        FromBytes::ref_from_bytes(memmap).expect("failed to read memory map")
    } else {
        &[]
    };
    records_writer.flush();
    let mut records_memmap = {
        let file = records_writer.get_ref();
        if handle_io_error(file.metadata()).len() != 0 {
            #[allow(unsafe_code)]
            Some(unsafe { handle_io_error(memmap2::MmapMut::map_mut(file)) })
        } else {
            None
        }
    };
    let records_slice: &mut [Record] = if let Some(memmap) = records_memmap.as_mut() {
        FromBytes::mut_from_bytes(memmap.as_mut()).expect("failed to read memory map")
    } else {
        &mut []
    };
    let mut mappings_writer = crate::io::mappings_writer(tempdir.path(), 0);

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
                    add_element(
                        relabel_slice,
                        records_slice,
                        &mut mappings_writer,
                        token.id,
                        document_id,
                        term_frequency,
                    );
                }
            }
        }
        assert!(tape_summaries.next(index).is_none(), "data corruption");
        assert!(tape_blocks.next(index).is_none(), "data corruption");
    }

    drop(records_memmap);
    drop(relabel_memmap);
    drop(relabel);

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
                                    crate::io::write(
                                        &mut records_writer,
                                        &mut mappings_writer,
                                        &document,
                                        vector_tuple.payload(),
                                    );
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
                                    crate::io::write(
                                        &mut records_writer,
                                        &mut mappings_writer,
                                        &document,
                                        vector_tuple.payload(),
                                    );
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

    records_writer.flush();
    mappings_writer.flush();
    drop(records_writer);
    drop(mappings_writer);
    crate::io::locally_merge(tempdir.path(), 0);

    let segment = crate::io::readers(tempdir.path(), 1);
    let flushed = crate::flush::flush(k1, b, index, segment);

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

    *jump_tuple.ptr_vectors() = ptr_vectors;
    *jump_tuple.number_of_documents() = flushed.number_of_documents;
    *jump_tuple.sum_of_document_lengths() = flushed.sum_of_document_lengths;
    *jump_tuple.width_1_documents() = flushed.width_1_documents;
    *jump_tuple.width_0_documents() = flushed.width_0_documents;
    *jump_tuple.depth_documents() = flushed.depth_documents;
    *jump_tuple.start_documents() = flushed.start_documents;
    *jump_tuple.free_documents() = flushed.free_documents;
    *jump_tuple.depth_tokens() = flushed.depth_tokens;
    *jump_tuple.start_tokens() = flushed.start_tokens;
    *jump_tuple.free_tokens() = flushed.free_tokens;
    *jump_tuple.ptr_documents() = flushed.ptr_documents;
    *jump_tuple.ptr_tokens() = flushed.ptr_tokens;
    *jump_tuple.ptr_summaries() = flushed.ptr_summaries;
    *jump_tuple.ptr_blocks() = flushed.ptr_blocks;

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

fn add_document(
    relabel: &mut BufWriter<File>,
    records_writer: &mut RecordsWriter,
    payload: Option<[u16; 3]>,
) {
    use std::io::Write;
    let label = if let Some(payload) = payload {
        records_writer.write(Record(0_u32, payload))
    } else {
        u32::MAX
    };
    handle_io_error(relabel.write_all(label.as_bytes()));
}

fn add_element(
    relabel_slice: &[u32],
    records_slice: &mut [Record],
    mappings_writer: &mut MappingsWriter,
    token_id: [u8; WIDTH],
    document_id: u32,
    term_frequency: u32,
) {
    let document_id = {
        let label = relabel_slice[document_id as usize];
        if label != u32::MAX { label } else { return }
    };
    {
        let Record(mut length, payload) = records_slice[document_id as usize];
        length = length.saturating_add(1);
        records_slice[document_id as usize] = Record(length, payload);
    }
    mappings_writer.write(Mapping(token_id, document_id, term_frequency));
}
