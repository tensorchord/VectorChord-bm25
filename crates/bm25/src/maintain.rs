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

use crate::io::{MappingsWriter, RecordsWriter};
use crate::segment::{Mapping, Record};
use crate::tape::TapeReader;
use crate::tuples::*;
use crate::vector::Document;
use crate::{Opaque, WIDTH, compression};
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};

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

    let mut collector_0 = Collector0::new();

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
                collector_0.add_document((!bool::from(tuple.deleted())).then_some(tuple.payload()));
            }
            current = guard.get_opaque().next;
        }
    }

    let tempdir = tempfile::tempdir().expect("failed to create temporary directory");

    let mut collector_1 = collector_0.finish(crate::io::mappings_writer(tempdir.path(), 0));

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
                    collector_1.add_element(token.id, document_id, term_frequency);
                }
            }
        }
        assert!(tape_summaries.next(index).is_none(), "data corruption");
        assert!(tape_blocks.next(index).is_none(), "data corruption");
    }

    let (mut records_writer, mut mappings_writer) =
        collector_1.finish(crate::io::records_writer(tempdir.path(), 0));

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

struct Collector0 {
    records: Vec<Record>,
    relabel: Vec<u32>,
}

impl Collector0 {
    fn new() -> Self {
        Self {
            records: Vec::new(),
            relabel: Vec::new(),
        }
    }
    fn add_document(&mut self, payload: Option<[u16; 3]>) {
        if let Some(payload) = payload {
            let id = self.records.len() as u32;
            self.records.push(Record(0_u32, payload));
            self.relabel.push(id);
        } else {
            self.relabel.push(u32::MAX);
        }
    }
    fn finish(self, mappings_writer: MappingsWriter) -> Collector1 {
        Collector1 {
            records: self.records,
            relabel: self.relabel,
            mappings_writer,
        }
    }
}

struct Collector1 {
    records: Vec<Record>,
    relabel: Vec<u32>,
    mappings_writer: MappingsWriter,
}

impl Collector1 {
    fn add_element(&mut self, token_id: [u8; WIDTH], document_id: u32, term_frequency: u32) {
        let document_id = self.relabel[document_id as usize];
        if document_id == u32::MAX {
            return;
        }
        self.records[document_id as usize].0 =
            1u32.saturating_add(self.records[document_id as usize].0);
        self.mappings_writer
            .write(Mapping(token_id, document_id, term_frequency));
    }
    fn finish(self, mut records_writer: RecordsWriter) -> (RecordsWriter, MappingsWriter) {
        for record in self.records {
            records_writer.write(record);
        }
        (records_writer, self.mappings_writer)
    }
}
