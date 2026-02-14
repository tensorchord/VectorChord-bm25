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

/// Term statistic segment is a **global** segment that stores the statistic for each terms.
/// Currently, it stores total count of documents which contains the term.
/// Used to calculate the inverse document frequency.
use crate::page::{VirtualPageReader, VirtualPageWriter};

use super::meta::MetaPageData;

pub struct TermStatReader {
    page_reader: VirtualPageReader,
    term_id_cnt: u32,
}

impl TermStatReader {
    pub unsafe fn new(index: pgrx::pg_sys::Relation, meta: &MetaPageData) -> Self {
        let page_reader = unsafe { VirtualPageReader::new(index, meta.term_stat_blkno) };
        Self {
            page_reader,
            term_id_cnt: meta.term_id_cnt,
        }
    }

    pub fn read(&self, term_id: u32) -> u32 {
        if term_id >= self.term_id_cnt {
            return 0;
        }

        let mut res: u32 = 0;
        self.page_reader.read_at(
            term_id * size_of::<u32>() as u32,
            bytemuck::bytes_of_mut(&mut res),
        );
        res
    }

    pub fn update(&self, term_id: u32, f: impl FnOnce(&mut u32)) {
        self.page_reader.update_at(
            term_id * size_of::<u32>() as u32,
            size_of::<u32>() as u32,
            |data| {
                f(bytemuck::from_bytes_mut(data));
            },
        );
    }
}

pub unsafe fn extend_term_id(
    index: pgrx::pg_sys::Relation,
    meta: &mut MetaPageData,
    term_id_cnt: u32,
) {
    let old_term_id_cnt = meta.term_id_cnt;
    if term_id_cnt <= old_term_id_cnt {
        return;
    }

    let mut page_writer = unsafe { VirtualPageWriter::open(index, meta.term_stat_blkno, false) };
    for _ in old_term_id_cnt..term_id_cnt {
        page_writer.write(&[0u8; size_of::<u32>()]);
    }
    meta.term_id_cnt = term_id_cnt;
}
