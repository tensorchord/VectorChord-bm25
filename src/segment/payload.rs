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

/// Payload segment is a global segment that stores the ctid of the documents.
/// doc_id -> ctid mapping
use crate::page::{PageFlags, VirtualPageReader, VirtualPageWriter};

pub struct PayloadWriter {
    pub buffer: Vec<u64>,
}

impl PayloadWriter {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn insert(&mut self, id: u64) {
        self.buffer.push(id);
    }

    pub fn serialize(&self, index: pgrx::pg_sys::Relation) -> pgrx::pg_sys::BlockNumber {
        let data = bytemuck::cast_slice(&self.buffer);
        let mut pager = VirtualPageWriter::new(index, PageFlags::PAYLOAD, true);
        pager.write(data);
        pager.finalize()
    }
}

pub struct PayloadReader(VirtualPageReader);

impl PayloadReader {
    pub fn new(index: pgrx::pg_sys::Relation, blkno: pgrx::pg_sys::BlockNumber) -> Self {
        Self(VirtualPageReader::new(index, blkno))
    }

    pub fn read(&self, doc_id: u32) -> u64 {
        let mut buf = [0u8; 8];
        self.0.read_at(doc_id * 8, &mut buf);
        u64::from_le_bytes(buf)
    }
}
