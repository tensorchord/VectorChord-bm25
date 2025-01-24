use std::num::NonZeroU32;

use crate::{
    algorithm::block_encode::{BlockDecode, BlockDecodeTrait},
    page::{bm25_page_size, page_read, VirtualPageReader},
    segment::{field_norm::id_to_fieldnorm, sealed::SealedSegmentData},
    weight::Bm25Weight,
};

use super::{PostingTermInfo, PostingTermMetaData, SkipBlock, SkipBlockFlags, TERMINATED_DOC};

pub struct PostingTermInfoReader {
    page_reader: VirtualPageReader,
    term_id_cnt: u32,
}

impl PostingTermInfoReader {
    pub fn new(index: pgrx::pg_sys::Relation, sealed_data: SealedSegmentData) -> Self {
        let page_reader = VirtualPageReader::new(index, sealed_data.term_info_blkno);
        Self {
            page_reader,
            term_id_cnt: sealed_data.term_id_cnt,
        }
    }

    pub fn read(&self, term_id: u32) -> PostingTermInfo {
        let mut res = PostingTermInfo::empty();
        if term_id >= self.term_id_cnt {
            return res;
        }
        self.page_reader.read_at(
            term_id * std::mem::size_of::<PostingTermInfo>() as u32,
            bytemuck::bytes_of_mut(&mut res),
        );
        res
    }

    pub fn write(&mut self, term_id: u32, info: PostingTermInfo) {
        assert!(term_id < self.term_id_cnt);
        self.page_reader.update_at(
            term_id * std::mem::size_of::<PostingTermInfo>() as u32,
            std::mem::size_of::<PostingTermInfo>() as u32,
            |data| {
                data.copy_from_slice(bytemuck::bytes_of(&info));
            },
        );
    }
}

pub struct PostingCursor {
    index: pgrx::pg_sys::Relation,
    block_decode: BlockDecode,
    // block reader
    block_page_reader: VirtualPageReader,
    block_page_id: u32,
    page_offset: u32,
    // skip info reader
    skip_info_page_id: u32,
    skip_info_offset: u32,
    decode_offset: u32,
    cur_skip_info: SkipBlock,
    // helper state
    block_decoded: bool,
    remain_block_cnt: u32,
    // unfulled block
    unfulled_docid: Box<[u32]>,
    unfulled_freq: Box<[u32]>,
    unfulled_offset: u32,
}

impl PostingCursor {
    pub fn new(index: pgrx::pg_sys::Relation, term_info: PostingTermInfo) -> Self {
        let PostingTermInfo { meta_blkno } = term_info;

        let term_meta_guard = page_read(index, meta_blkno);
        let block_decode = BlockDecode::new();
        let term_meta: &PostingTermMetaData = term_meta_guard.as_ref();
        let block_page_reader = VirtualPageReader::new(index, term_meta.block_data_blkno);
        let remain_block_cnt = term_meta.block_count;
        let unfulled_docid = term_meta.unfulled_docid[..term_meta.unfulled_doc_cnt as usize].into();
        let unfulled_freq = term_meta.unfulled_freq[..term_meta.unfulled_doc_cnt as usize].into();

        let mut this = Self {
            index,
            block_decode,
            block_page_reader,
            block_page_id: 0,
            page_offset: 0,
            skip_info_page_id: term_meta.skip_info_blkno,
            skip_info_offset: 0,
            decode_offset: 0,
            cur_skip_info: SkipBlock::default(),
            block_decoded: false,
            remain_block_cnt,
            unfulled_docid,
            unfulled_freq,
            unfulled_offset: u32::MAX,
        };

        this.update_skip_info();
        this
    }

    pub fn next_block(&mut self) -> bool {
        debug_assert!(!self.completed());
        self.remain_block_cnt -= 1;
        self.block_decoded = false;
        if self.completed() {
            return false;
        }

        self.decode_offset = self.cur_skip_info.last_doc;
        self.page_offset += self.cur_skip_info.size as u32;

        self.skip_info_offset += std::mem::size_of::<SkipBlock>() as u32;
        if self.skip_info_offset == bm25_page_size() as u32 {
            let page = page_read(self.index, self.skip_info_page_id);
            self.skip_info_page_id = page.opaque.next_blkno;
            self.skip_info_offset = 0;
        }
        self.update_skip_info();

        if self
            .cur_skip_info
            .flag
            .contains(SkipBlockFlags::PAGE_CHANGED)
        {
            self.block_page_id += 1;
            self.page_offset = 0;
        }

        true
    }

    pub fn next_doc(&mut self) -> bool {
        debug_assert!(self.block_decoded);
        if self.is_in_unfulled_block() {
            self.unfulled_offset += 1;
            debug_assert!(self.unfulled_offset <= self.unfulled_doc_cnt());
            if self.unfulled_offset == self.unfulled_doc_cnt() {
                return false;
            }
            true
        } else {
            self.block_decode.next()
        }
    }

    pub fn next_with_auto_decode(&mut self) -> bool {
        if self.completed() {
            return false;
        }
        if self.next_doc() {
            return true;
        }
        if self.next_block() {
            self.decode_block();
            true
        } else {
            false
        }
    }

    pub fn shallow_seek(&mut self, docid: u32) -> bool {
        if self.completed() {
            return false;
        }
        let prev_docid = self.docid();
        while self.last_doc_in_block() < docid {
            if !self.next_block() {
                debug_assert!(prev_docid == self.docid());
                return false;
            }
        }
        debug_assert!(prev_docid == self.docid());
        true
    }

    pub fn seek(&mut self, docid: u32) -> u32 {
        if self.completed() {
            self.unfulled_offset = self.unfulled_doc_cnt();
            return TERMINATED_DOC;
        }
        if !self.shallow_seek(docid) {
            return TERMINATED_DOC;
        }
        if !self.block_decoded {
            self.decode_block();
        }

        if self.is_in_unfulled_block() {
            self.unfulled_offset = self
                .unfulled_docid
                .partition_point(|&d| d < docid)
                .try_into()
                .unwrap();
            debug_assert!(self.unfulled_offset < self.unfulled_doc_cnt());
        } else {
            let incomplete = self.block_decode.seek(docid);
            debug_assert!(incomplete);
        }
        debug_assert!(self.docid() >= docid);
        self.docid()
    }

    pub fn decode_block(&mut self) {
        debug_assert!(!self.completed());
        if self.block_decoded {
            return;
        }
        self.block_decoded = true;
        if self.is_in_unfulled_block() {
            self.unfulled_offset = 0;
            return;
        }

        let skip = &self.cur_skip_info;
        let page = page_read(
            self.index,
            self.block_page_reader.get_block_id(self.block_page_id),
        );
        self.block_decode.decode(
            &page.data()[self.page_offset as usize..][..skip.size as usize],
            NonZeroU32::new(self.decode_offset),
        );
    }

    pub fn docid(&self) -> u32 {
        if self.completed() && self.unfulled_offset == self.unfulled_doc_cnt() {
            return TERMINATED_DOC;
        }
        if self.is_in_unfulled_block() && self.unfulled_offset != u32::MAX {
            return self.unfulled_docid[self.unfulled_offset as usize];
        }
        debug_assert!(self.block_decode.docid() <= self.last_doc_in_block());
        self.block_decode.docid()
    }

    pub fn freq(&self) -> u32 {
        debug_assert!(!self.completed());
        debug_assert!(self.block_decoded);
        if self.is_in_unfulled_block() {
            return self.unfulled_freq[self.unfulled_offset as usize];
        }
        self.block_decode.freq()
    }

    pub fn block_max_score(&self, weight: &Bm25Weight) -> f32 {
        if self.completed() {
            return 0.0;
        }
        let len = id_to_fieldnorm(self.cur_skip_info.blockwand_fieldnorm_id);
        weight.score(len, self.cur_skip_info.blockwand_tf)
    }

    pub fn last_doc_in_block(&self) -> u32 {
        if self.completed() {
            return TERMINATED_DOC;
        }
        self.cur_skip_info.last_doc
    }

    pub fn completed(&self) -> bool {
        self.remain_block_cnt == 0
    }

    fn update_skip_info(&mut self) {
        let page = page_read(self.index, self.skip_info_page_id);
        let skip_info = *bytemuck::from_bytes(
            &page.data()[self.skip_info_offset as usize..][..std::mem::size_of::<SkipBlock>()],
        );
        self.cur_skip_info = skip_info;
    }

    fn unfulled_doc_cnt(&self) -> u32 {
        self.unfulled_docid.len() as u32
    }

    fn is_in_unfulled_block(&self) -> bool {
        !self.unfulled_docid.is_empty() && self.remain_block_cnt <= 1
    }
}
