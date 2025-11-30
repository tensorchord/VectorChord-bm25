use std::num::NonZeroU32;

use crate::{
    algorithm::{
        block_encode::{BlockEncode, BlockEncodeTrait},
        block_partition::{BlockPartition, BlockPartitionTrait},
    },
    page::{page_alloc, PageFlags, PageWriter, VirtualPageWriter},
    segment::{
        field_norm::{id_to_fieldnorm, FieldNormRead},
        posting::SkipBlockFlags,
    },
    weight::{idf, Bm25Weight},
};

use super::{writer::TFRecorder, PostingTermInfo, PostingTermMetaData, SkipBlock};

pub trait InvertedWrite {
    fn write(&mut self, recorder: Option<&TFRecorder>);
}

pub struct InvertedSerializer<R: FieldNormRead> {
    index: pgrx::pg_sys::Relation,
    postings_serializer: PostingSerializer,
    term_info_serializer: PostingTermInfoSerializer,
    block_partition: BlockPartition,
    // block wand helper
    avgdl: f32,
    corpus_doc_cnt: u32,
    fieldnorm_reader: R,
}

impl<R: FieldNormRead> InvertedSerializer<R> {
    pub fn new(
        index: pgrx::pg_sys::Relation,
        corpus_doc_cnt: u32,
        avgdl: f32,
        fieldnorm_reader: R,
    ) -> Self {
        let postings_serializer = PostingSerializer::new(index);
        let term_info_serializer = PostingTermInfoSerializer::new(index);
        Self {
            index,
            postings_serializer,
            term_info_serializer,
            block_partition: BlockPartition::new(),
            avgdl,
            corpus_doc_cnt,
            fieldnorm_reader,
        }
    }

    /// return term_info_blkno
    pub fn finalize(self) -> pgrx::pg_sys::BlockNumber {
        self.term_info_serializer.finalize()
    }
}

impl<R: FieldNormRead> InvertedWrite for InvertedSerializer<R> {
    fn write(&mut self, recorder: Option<&TFRecorder>) {
        let Some(recorder) = recorder else {
            self.term_info_serializer.push(PostingTermInfo::empty());
            return;
        };

        let doc_cnt = recorder.doc_cnt();
        if doc_cnt == 0 {
            self.term_info_serializer.push(PostingTermInfo::empty());
            return;
        }

        let idf = idf(self.corpus_doc_cnt, doc_cnt);
        let bm25_weight = Bm25Weight::new(1, idf, self.avgdl);
        for (doc_id, tf) in recorder.iter() {
            let len = id_to_fieldnorm(self.fieldnorm_reader.read(doc_id));
            self.block_partition.add_doc(bm25_weight.score(len, tf));
        }
        self.block_partition.make_partitions();
        let partitions = self.block_partition.partitions();
        let max_doc = self.block_partition.max_doc();
        let mut block_count = 0;
        let mut blockwand_tf = 0;
        let mut blockwand_fieldnorm_id = 0;

        self.postings_serializer.new_term();
        for (i, (doc_id, freq)) in recorder.iter().enumerate() {
            self.postings_serializer.write_doc(doc_id, freq);
            if partitions.get(block_count).copied() == Some(i as u32) {
                self.postings_serializer
                    .flush_block(blockwand_tf, blockwand_fieldnorm_id);
                block_count += 1;
            }
            if max_doc.get(block_count).copied() == Some(i as u32) {
                blockwand_tf = freq;
                blockwand_fieldnorm_id = self.fieldnorm_reader.read(doc_id);
            }
        }
        assert!(block_count == partitions.len());
        self.block_partition.reset();

        let mut term_meta_guard = page_alloc(self.index, PageFlags::TERM_META, true);
        let term_meta: &mut PostingTermMetaData = term_meta_guard.init_mut();

        let (unflushed_docids, unflushed_term_freqs) = self.postings_serializer.unflushed_data();
        let unfulled_doc_cnt = unflushed_docids.len();
        assert!(unfulled_doc_cnt < 128);
        term_meta.unfulled_docid[..unfulled_doc_cnt].copy_from_slice(unflushed_docids);
        term_meta.unfulled_freq[..unfulled_doc_cnt].copy_from_slice(unflushed_term_freqs);
        if unfulled_doc_cnt != 0 {
            block_count += 1;
        }

        self.postings_serializer
            .close_term(&bm25_weight, &self.fieldnorm_reader, term_meta);
        term_meta.block_count = block_count.try_into().unwrap();

        self.term_info_serializer.push(PostingTermInfo {
            meta_blkno: term_meta_guard.blkno(),
        });
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
            term_infos: Vec::new(),
        }
    }

    pub fn push(&mut self, term_info: PostingTermInfo) {
        self.term_infos.push(term_info);
    }

    pub fn finalize(self) -> pgrx::pg_sys::BlockNumber {
        let mut pager = VirtualPageWriter::new(self.index, PageFlags::TERM_INFO, true);
        pager.write(bytemuck::cast_slice(&self.term_infos));
        pager.finalize()
    }
}

pub struct PostingSerializer {
    index: pgrx::pg_sys::Relation,
    // block encoder
    block_encode: BlockEncode,
    prev_block_last_doc_id: u32,
    // block buffer
    doc_ids: Vec<u32>,
    term_freqs: Vec<u32>,
    // skip info writer
    skip_info_writer: Option<PageWriter>,
    // block data writer
    block_data_writer: Option<VirtualPageWriter>,
}

impl PostingSerializer {
    pub fn new(index: pgrx::pg_sys::Relation) -> Self {
        Self {
            index,
            block_encode: BlockEncode::new(),
            prev_block_last_doc_id: 0,
            doc_ids: Vec::with_capacity(128),
            term_freqs: Vec::with_capacity(128),
            skip_info_writer: None,
            block_data_writer: None,
        }
    }

    pub fn new_term(&mut self) {
        if self.skip_info_writer.is_some()
            || self.block_data_writer.is_some()
            || self.prev_block_last_doc_id != 0
        {
            panic!("Writers are already initialized for the previous term. Call close_term() before starting a new term.");
        }
    }

    pub fn write_doc(&mut self, doc_id: u32, freq: u32) {
        self.doc_ids.push(doc_id);
        self.term_freqs.push(freq);
    }

    // return (skip_info_blkno, skip_info_last_blkno, block_data_blkno)
    pub fn close_term<R: FieldNormRead>(
        &mut self,
        bm25_weight: &Bm25Weight,
        fieldnorm_reader: &R,
        term_meta: &mut PostingTermMetaData,
    ) {
        if !self.doc_ids.is_empty() {
            let (blockwand_tf, blockwand_fieldnorm_id) = blockwand_max_calculate(
                &self.doc_ids,
                &self.term_freqs,
                bm25_weight,
                fieldnorm_reader,
            );
            let skip_block = SkipBlock {
                last_doc: self.last_doc(),
                doc_cnt: self.doc_cnt(),
                blockwand_tf,
                size: 0,
                blockwand_fieldnorm_id,
                flag: SkipBlockFlags::UNFULLED,
            };
            term_meta.unfulled_skip_block = Some(skip_block);
        }

        let [skip_info_last_blkno, skip_info_blkno, block_data_blkno] =
            match (self.skip_info_writer.take(), self.block_data_writer.take()) {
                (Some(skip_info_writer), Some(block_data_writer)) => {
                    let skip_info_last_blkno = skip_info_writer.blkno();
                    let skip_info_blkno = skip_info_writer.finalize();
                    let block_data_blkno = block_data_writer.finalize();
                    [skip_info_last_blkno, skip_info_blkno, block_data_blkno]
                }
                (None, None) => [pgrx::pg_sys::InvalidBlockNumber; 3],
                _ => {
                    panic!("Inconsistent state: only one of the writers is None")
                }
            };
        term_meta.last_full_block_last_docid = self.prev_block_last_doc_id;
        term_meta.skip_info_blkno = skip_info_blkno;
        term_meta.skip_info_last_blkno = skip_info_last_blkno;
        term_meta.block_data_blkno = block_data_blkno;

        self.doc_ids.clear();
        self.term_freqs.clear();
        self.prev_block_last_doc_id = 0;
    }

    pub fn flush_block(&mut self, blockwand_tf: u32, blockwand_fieldnorm_id: u8) {
        let offset = NonZeroU32::new(self.prev_block_last_doc_id);
        let prev_block_last_doc_id = self.last_doc();
        let doc_cnt = self.doc_cnt();

        self.init_writers();
        self.prev_block_last_doc_id = prev_block_last_doc_id;
        let data = self
            .block_encode
            .encode(offset, &mut self.doc_ids, &mut self.term_freqs);

        let block_data_writer = self.block_data_writer.as_mut().unwrap();
        let page_changed = block_data_writer.write_vectorized_no_cross(&[data]);

        let mut flag = SkipBlockFlags::empty();
        if page_changed {
            flag |= SkipBlockFlags::PAGE_CHANGED;
        }
        let skip_block = SkipBlock {
            last_doc: prev_block_last_doc_id,
            doc_cnt,
            blockwand_tf,
            size: data.len().try_into().unwrap(),
            blockwand_fieldnorm_id,
            flag,
        };
        let skip_info_writer = self.skip_info_writer.as_mut().unwrap();
        skip_info_writer.write(bytemuck::bytes_of(&skip_block));

        self.doc_ids.clear();
        self.term_freqs.clear();
    }

    pub fn unflushed_data(&self) -> (&[u32], &[u32]) {
        (&self.doc_ids, &self.term_freqs)
    }

    fn init_writers(&mut self) {
        match (&mut self.skip_info_writer, &mut self.block_data_writer) {
            (Some(_), Some(_)) => {}
            (skip_info_writer @ None, block_data_writer @ None) => {
                *skip_info_writer = Some(PageWriter::new(self.index, PageFlags::SKIP_INFO, true));
                *block_data_writer = Some(VirtualPageWriter::new(
                    self.index,
                    PageFlags::BLOCK_DATA,
                    true,
                ));
            }
            _ => panic!("Inconsistent state: only one of the writers is None"),
        }
    }

    fn doc_cnt(&self) -> u32 {
        self.doc_ids.len().try_into().unwrap()
    }

    fn last_doc(&self) -> u32 {
        *self.doc_ids.last().unwrap()
    }
}

fn blockwand_max_calculate<R: FieldNormRead>(
    docids: &[u32],
    freqs: &[u32],
    bm25_weight: &Bm25Weight,
    fieldnorm_reader: &R,
) -> (u32, u8) {
    let mut max_score = 0.0;
    let mut max_fieldnorm_id = 0;
    let mut max_tf = 0;
    for (&doc_id, &freq) in docids.iter().zip(freqs.iter()) {
        let fieldnorm_id = fieldnorm_reader.read(doc_id);
        let fieldnorm = id_to_fieldnorm(fieldnorm_id);
        let score = bm25_weight.score(fieldnorm, freq);
        if score > max_score {
            max_score = score;
            max_fieldnorm_id = fieldnorm_id;
            max_tf = freq;
        }
    }
    (max_tf, max_fieldnorm_id)
}
