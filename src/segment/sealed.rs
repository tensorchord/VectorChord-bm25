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
// Copyright (c) 2025 TensorChord Inc.

use crate::datatype::Bm25VectorBorrowed;
use crate::page::VirtualPageWriter;

use super::field_norm::FieldNormRead;
use super::posting::{
    InvertedSerializer, InvertedWriter, PostingCursor, PostingTermInfo, PostingTermInfoReader,
};

#[derive(Debug, Clone, Copy)]
pub struct SealedSegmentData {
    pub term_info_blkno: u32,
    pub term_id_cnt: u32,
}

#[allow(dead_code)]
pub struct SealedSegmentWriter {
    writer: InvertedWriter,
}

impl SealedSegmentWriter {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            writer: InvertedWriter::new(),
        }
    }

    #[allow(dead_code)]
    pub fn insert(&mut self, doc_id: u32, vector: Bm25VectorBorrowed) {
        self.writer.insert(doc_id, vector);
    }

    #[allow(dead_code)]
    pub fn finalize_insert(&mut self) {
        self.writer.finalize();
    }

    #[allow(dead_code)]
    pub fn serialize<R: FieldNormRead>(&self, s: &mut InvertedSerializer<R>) {
        self.writer.serialize(s);
    }
}

pub struct SealedSegmentReader {
    index: pgrx::pg_sys::Relation,
    term_info_reader: PostingTermInfoReader,
    term_id_cnt: u32,
}

impl SealedSegmentReader {
    pub unsafe fn new(index: pgrx::pg_sys::Relation, sealed_data: SealedSegmentData) -> Self {
        let term_info_reader = unsafe { PostingTermInfoReader::new(index, sealed_data) };
        Self {
            index,
            term_info_reader,
            term_id_cnt: sealed_data.term_id_cnt,
        }
    }

    pub fn get_postings(&self, term_id: u32) -> Option<PostingCursor> {
        if term_id >= self.term_id_cnt {
            return None;
        }

        let term_info = self.term_info_reader.read(term_id);
        if term_info.meta_blkno == pgrx::pg_sys::InvalidBlockNumber {
            return None;
        }
        unsafe { Some(PostingCursor::new(self.index, term_info)) }
    }
}

pub unsafe fn extend_sealed_term_id(
    index: pgrx::pg_sys::Relation,
    sealed_data: &mut SealedSegmentData,
    term_id_cnt: u32,
) {
    if sealed_data.term_id_cnt >= term_id_cnt {
        return;
    }
    let mut page_writer =
        unsafe { VirtualPageWriter::open(index, sealed_data.term_info_blkno, false) };
    for _ in sealed_data.term_id_cnt..term_id_cnt {
        page_writer.write(bytemuck::bytes_of(&PostingTermInfo::empty()));
    }
    sealed_data.term_id_cnt = term_id_cnt;
}
