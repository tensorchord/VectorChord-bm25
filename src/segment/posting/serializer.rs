use crate::{
    page::{PageFlags, VirtualPageWriter},
    segment::{
        field_norm::{id_to_fieldnorm, FieldNormRead, MAX_FIELD_NORM},
        meta::MetaPageData,
    },
    token::vocab_len,
    utils::compress_block::BlockEncoder,
    weight::{idf, Bm25Weight},
};

use super::{PostingTermInfo, SkipBlock, COMPRESSION_BLOCK_SIZE};

pub struct InvertedSerializer<R: FieldNormRead> {
    postings_serializer: PostingSerializer<R>,
    term_info_serializer: PostingTermInfoSerializer,
    current_term_info: PostingTermInfo,
}

impl<R: FieldNormRead> InvertedSerializer<R> {
    pub fn new(
        index: pgrx::pg_sys::Relation,
        doc_cnt: u32,
        avgdl: f32,
        fieldnorm_reader: R,
    ) -> Self {
        let postings_serializer = PostingSerializer::new(index, doc_cnt, avgdl, fieldnorm_reader);
        let term_info_serializer = PostingTermInfoSerializer::new(index);
        Self {
            postings_serializer,
            term_info_serializer,
            current_term_info: PostingTermInfo::default(),
        }
    }

    pub fn new_term(&mut self, doc_count: u32) {
        self.current_term_info = PostingTermInfo {
            doc_count,
            postings_blkno: pgrx::pg_sys::InvalidBlockNumber,
        };
        if doc_count != 0 {
            self.postings_serializer.new_term(doc_count);
        }
    }

    pub fn write_doc(&mut self, doc_id: u32, freq: u32) {
        self.postings_serializer.write_doc(doc_id, freq);
    }

    pub fn close_term(&mut self, meta: &mut MetaPageData) {
        if self.current_term_info.doc_count != 0 {
            self.current_term_info.postings_blkno = self.postings_serializer.close_term(meta);
        }
        self.term_info_serializer.push(self.current_term_info);
    }

    /// return term_info_blkno
    pub fn finalize(self, meta: &mut MetaPageData) -> pgrx::pg_sys::BlockNumber {
        self.term_info_serializer.finalize(meta)
    }
}

struct PostingTermInfoSerializer {
    index: pgrx::pg_sys::Relation,
    term_infos: Vec<PostingTermInfo>,
}

impl PostingTermInfoSerializer {
    pub fn new(index: pgrx::pg_sys::Relation) -> Self {
        Self {
            index,
            term_infos: Vec::with_capacity(vocab_len() as usize),
        }
    }

    pub fn push(&mut self, term_info: PostingTermInfo) {
        self.term_infos.push(term_info);
    }

    pub fn finalize(self, meta: &mut MetaPageData) -> pgrx::pg_sys::BlockNumber {
        let mut pager = VirtualPageWriter::new(self.index, meta, PageFlags::POSTINGS, true);
        pager.write(bytemuck::cast_slice(&self.term_infos));
        pager.finalize()
    }
}

struct PostingSerializer<R: FieldNormRead> {
    index: pgrx::pg_sys::Relation,
    encoder: BlockEncoder,
    posting_write: Vec<u8>,
    last_doc_id: u32,
    // block buffer
    doc_ids: [u32; COMPRESSION_BLOCK_SIZE],
    term_freqs: [u32; COMPRESSION_BLOCK_SIZE],
    block_size: usize,
    // block wand helper
    skip_write: Vec<SkipBlock>,
    avg_dl: f32,
    doc_cnt: u32,
    bm25_weight: Option<Bm25Weight>,
    fieldnorm_reader: R,
}

impl<R: FieldNormRead> PostingSerializer<R> {
    pub fn new(
        index: pgrx::pg_sys::Relation,
        doc_cnt: u32,
        avg_dl: f32,
        fieldnorm_reader: R,
    ) -> Self {
        Self {
            index,
            encoder: BlockEncoder::new(),
            posting_write: Vec::new(),
            last_doc_id: 0,
            doc_ids: [0; COMPRESSION_BLOCK_SIZE],
            term_freqs: [0; COMPRESSION_BLOCK_SIZE],
            block_size: 0,
            skip_write: Vec::new(),
            avg_dl,
            doc_cnt,
            bm25_weight: None,
            fieldnorm_reader,
        }
    }

    pub fn new_term(&mut self, doc_count: u32) {
        let idf = idf(self.doc_cnt, doc_count);
        self.bm25_weight = Some(Bm25Weight::new(1, idf, self.avg_dl));
    }

    pub fn write_doc(&mut self, doc_id: u32, freq: u32) {
        self.doc_ids[self.block_size] = doc_id;
        self.term_freqs[self.block_size] = freq;
        self.block_size += 1;
        if self.block_size == COMPRESSION_BLOCK_SIZE {
            self.flush_block();
        }
    }

    pub fn close_term(&mut self, meta: &mut MetaPageData) -> pgrx::pg_sys::BlockNumber {
        if self.block_size > 0 {
            if self.block_size == COMPRESSION_BLOCK_SIZE {
                self.flush_block();
            } else {
                self.flush_block_unfull();
            }
        }
        let mut pager = VirtualPageWriter::new(self.index, meta, PageFlags::POSTINGS, true);
        pager.write(bytemuck::cast_slice(self.skip_write.as_slice()));
        let mut offset = 0;
        for skip in self.skip_write.iter().take(self.skip_write.len() - 1) {
            let len = skip.block_size();
            pager.write_no_cross(&self.posting_write[offset..][..len]);
            offset += len;
        }
        pager.write_no_cross(&self.posting_write[offset..]);
        let blkno = pager.finalize();
        self.last_doc_id = 0;
        self.bm25_weight = None;
        self.posting_write.clear();
        self.skip_write.clear();
        blkno
    }

    fn flush_block(&mut self) {
        assert!(self.block_size == COMPRESSION_BLOCK_SIZE);

        let (blockwand_tf, blockwand_fieldnorm_id) = self.block_wand();

        // doc_id
        let (docid_bits, docid_block) = self
            .encoder
            .compress_block_sorted(&self.doc_ids[..self.block_size], self.last_doc_id);
        self.posting_write.extend_from_slice(docid_block);
        self.last_doc_id = self.doc_ids[self.block_size - 1];

        // term_freq
        for i in 0..self.block_size {
            self.term_freqs[i] -= 1;
        }
        let (tf_bits, term_freq_block) = self
            .encoder
            .compress_block_unsorted(&self.term_freqs[..self.block_size]);
        self.posting_write.extend_from_slice(term_freq_block);

        self.skip_write.push(SkipBlock {
            last_doc: self.last_doc_id,
            docid_bits,
            tf_bits,
            blockwand_tf,
            blockwand_fieldnorm_id,
            reserved: 0,
        });

        self.block_size = 0;
    }

    fn flush_block_unfull(&mut self) {
        assert!(self.block_size > 0);

        let (blockwand_tf, blockwand_fieldnorm_id) = self.block_wand();

        // doc_id
        let docid_block = self
            .encoder
            .compress_vint_sorted(&self.doc_ids[..self.block_size], self.last_doc_id);
        self.posting_write.extend_from_slice(docid_block);
        self.last_doc_id = self.doc_ids[self.block_size - 1];

        // term_freq
        for i in 0..self.block_size {
            self.term_freqs[i] -= 1;
        }
        let term_freq_block = self
            .encoder
            .compress_vint_unsorted(&self.term_freqs[..self.block_size]);
        self.posting_write.extend_from_slice(term_freq_block);

        self.skip_write.push(SkipBlock {
            last_doc: self.last_doc_id,
            docid_bits: 0,
            tf_bits: 0,
            blockwand_tf,
            blockwand_fieldnorm_id,
            reserved: 0,
        });

        self.block_size = 0;
    }

    fn block_wand(&self) -> (u32, u8) {
        let mut blockwand_tf = MAX_FIELD_NORM;
        let mut blockwand_fieldnorm_id = u8::MAX;
        let mut blockwand_max = 0.0f32;
        let bm25_weight = self.bm25_weight.as_ref().expect("no bm25 weight");
        for i in 0..self.block_size {
            let doc_id = self.doc_ids[i];
            let tf = self.term_freqs[i];
            let fieldnorm_id = self.fieldnorm_reader.read(doc_id);
            let len = id_to_fieldnorm(fieldnorm_id);
            let bm25_score = bm25_weight.score(len, tf);
            if bm25_score > blockwand_max {
                blockwand_max = bm25_score;
                blockwand_tf = tf;
                blockwand_fieldnorm_id = fieldnorm_id;
            }
        }
        (blockwand_tf, blockwand_fieldnorm_id)
    }
}