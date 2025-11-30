use std::num::NonZeroU32;

use crate::{
    algorithm::block_encode::{BlockEncode, BlockEncodeTrait},
    page::{page_alloc_with_fsm, page_write, PageFlags, VirtualPageWriter},
    segment::{
        field_norm::{id_to_fieldnorm, FieldNormRead, FieldNormReader},
        meta::MetaPageData,
        posting::{PostingTermInfo, PostingTermMetaData},
        term_stat::TermStatReader,
    },
    weight::{idf, Bm25Weight},
};

use super::{
    serializer::PostingSerializer, writer::TFRecorder, InvertedWrite, PostingTermInfoReader,
    SkipBlock, SkipBlockFlags, COMPRESSION_BLOCK_SIZE,
};

pub struct InvertedAppender {
    index: pgrx::pg_sys::Relation,
    block_encode: BlockEncode,
    term_info_reader: PostingTermInfoReader,
    term_stat_reader: TermStatReader,
    term_id: u32,
    doc_cnt: u32,
    avgdl: f32,
    fieldnorm_reader: FieldNormReader,
}

impl InvertedAppender {
    pub fn new(index: pgrx::pg_sys::Relation, meta: &MetaPageData) -> Self {
        let block_encode = BlockEncode::new();
        let term_info_reader = PostingTermInfoReader::new(index, meta.sealed_segment);
        let term_stat_reader = TermStatReader::new(index, meta);
        let fieldnorm_reader = FieldNormReader::new(index, meta.field_norm_blkno);
        Self {
            index,
            block_encode,
            term_info_reader,
            term_stat_reader,
            term_id: 0,
            doc_cnt: meta.doc_cnt,
            avgdl: meta.avgdl(),
            fieldnorm_reader,
        }
    }
}

impl InvertedWrite for InvertedAppender {
    fn write(&mut self, recorder: Option<&TFRecorder>) {
        let Some(recorder) = recorder else {
            self.term_id += 1;
            return;
        };

        if recorder.doc_cnt() == 0 {
            self.term_id += 1;
            return;
        }

        let term_doc_cnt = self.term_stat_reader.read(self.term_id);
        let idf = idf(self.doc_cnt, term_doc_cnt);
        let weight = Bm25Weight::new(1, idf, self.avgdl);

        let term_info = self.term_info_reader.read(self.term_id);
        if term_info.meta_blkno == pgrx::pg_sys::InvalidBlockNumber {
            self.write_new_term_id(recorder, weight);
        } else {
            self.append_existing_term_id(recorder, weight, term_info.meta_blkno);
        }

        self.term_id += 1;
    }
}

impl InvertedAppender {
    fn write_new_term_id(&mut self, recorder: &TFRecorder, weight: Bm25Weight) {
        let mut serializer = PostingSerializer::new(self.index);
        serializer.new_term();

        let mut block_count = 0;
        let mut blockwand_tf = 0;
        let mut blockwand_fieldnorm_id = 0;
        let mut blockwand_score = 0.0;
        for (i, (docid, freq)) in recorder.iter().enumerate() {
            serializer.write_doc(docid, freq);

            let fieldnorm_id = self.fieldnorm_reader.read(docid);
            let len = id_to_fieldnorm(fieldnorm_id);
            let score = weight.score(len, freq);
            if score > blockwand_score {
                blockwand_tf = freq;
                blockwand_fieldnorm_id = fieldnorm_id;
                blockwand_score = score;
            }

            if (i + 1) % COMPRESSION_BLOCK_SIZE == 0 {
                serializer.flush_block(blockwand_tf, blockwand_fieldnorm_id);
                blockwand_tf = 0;
                blockwand_fieldnorm_id = 0;
                blockwand_score = 0.0;
                block_count += 1;
            }
        }

        let mut term_meta_guard = page_alloc_with_fsm(self.index, PageFlags::TERM_META, true);
        let term_meta_page = &mut *term_meta_guard;
        term_meta_page.header.pd_lower += std::mem::size_of::<PostingTermMetaData>() as u16;
        let term_meta: &mut PostingTermMetaData = term_meta_page.as_mut();

        let (unflushed_docids, unflushed_term_freqs) = serializer.unflushed_data();
        let unfulled_doc_cnt = unflushed_docids.len();
        assert!(unfulled_doc_cnt < 128);
        term_meta.unfulled_docid[..unfulled_doc_cnt].copy_from_slice(unflushed_docids);
        term_meta.unfulled_freq[..unfulled_doc_cnt].copy_from_slice(unflushed_term_freqs);
        if unfulled_doc_cnt != 0 {
            block_count += 1;
        }
        term_meta.block_count = block_count;

        serializer.close_term(&weight, &self.fieldnorm_reader, term_meta);

        self.term_info_reader.write(
            self.term_id,
            PostingTermInfo {
                meta_blkno: term_meta_guard.blkno(),
            },
        );
    }

    fn append_existing_term_id(
        &mut self,
        recorder: &TFRecorder,
        weight: Bm25Weight,
        meta_blkno: pgrx::pg_sys::BlockNumber,
    ) {
        let mut term_meta_guard = page_write(self.index, meta_blkno);
        let term_meta: &mut PostingTermMetaData = term_meta_guard.as_mut();

        let mut block_count = term_meta.block_count;
        let mut last_full_block_last_docid = term_meta.last_full_block_last_docid;
        let mut blockwand_tf = 0;
        let mut blockwand_fieldnorm_id = 0;
        let mut blockwand_score = 0.0;
        let mut unfulled_doc_cnt = 0;
        let mut block_data_writer = None;

        if let Some(skip_info) = term_meta.unfulled_skip_block {
            blockwand_tf = skip_info.blockwand_tf;
            blockwand_fieldnorm_id = skip_info.blockwand_fieldnorm_id;
            blockwand_score = weight.score(id_to_fieldnorm(blockwand_fieldnorm_id), blockwand_tf);
            unfulled_doc_cnt = skip_info.doc_cnt as usize;
            block_count -= 1;
        }

        for (docid, freq) in recorder.iter() {
            term_meta.unfulled_docid[unfulled_doc_cnt] = docid;
            term_meta.unfulled_freq[unfulled_doc_cnt] = freq;

            let fieldnorm_id = self.fieldnorm_reader.read(docid);
            let len = id_to_fieldnorm(fieldnorm_id);
            let score = weight.score(len, freq);
            if score > blockwand_score {
                blockwand_tf = freq;
                blockwand_fieldnorm_id = fieldnorm_id;
                blockwand_score = score;
            }

            unfulled_doc_cnt += 1;
            if unfulled_doc_cnt == 128 {
                let new_last_full_block_last_docid = docid;
                let data = self.block_encode.encode(
                    NonZeroU32::new(last_full_block_last_docid),
                    &mut term_meta.unfulled_docid,
                    &mut term_meta.unfulled_freq,
                );
                last_full_block_last_docid = new_last_full_block_last_docid;
                unfulled_doc_cnt = 0;
                block_count += 1;

                let block_data_writer =
                    init_block_data_writer(self.index, &mut block_data_writer, term_meta);
                let page_changed = block_data_writer.write_vectorized_no_cross(&[data]);
                let mut flag = SkipBlockFlags::empty();
                if page_changed {
                    flag |= SkipBlockFlags::PAGE_CHANGED;
                }
                let skip_info = SkipBlock {
                    last_doc: last_full_block_last_docid,
                    blockwand_tf,
                    doc_cnt: 128,
                    size: data.len().try_into().unwrap(),
                    blockwand_fieldnorm_id,
                    flag,
                };
                append_skip_info(self.index, term_meta, skip_info);
            }
        }

        if unfulled_doc_cnt != 0 {
            let skip_info = SkipBlock {
                last_doc: term_meta.unfulled_docid[unfulled_doc_cnt - 1],
                blockwand_tf,
                doc_cnt: unfulled_doc_cnt as u32,
                size: 0,
                blockwand_fieldnorm_id,
                flag: SkipBlockFlags::UNFULLED,
            };
            append_skip_info(self.index, term_meta, skip_info);
            block_count += 1;
        }
        term_meta.block_count = block_count;
        term_meta.last_full_block_last_docid = last_full_block_last_docid;
    }
}

fn append_skip_info(
    index: pgrx::pg_sys::Relation,
    term_meta: &mut PostingTermMetaData,
    skip_info: SkipBlock,
) {
    let mut guard = if term_meta.skip_info_last_blkno != pgrx::pg_sys::InvalidBlockNumber {
        page_write(index, term_meta.skip_info_last_blkno)
    } else {
        let guard = page_alloc_with_fsm(index, PageFlags::SKIP_INFO, false);
        term_meta.skip_info_blkno = guard.blkno();
        term_meta.skip_info_last_blkno = guard.blkno();
        guard
    };

    let mut freespace = guard.freespace_mut();
    if freespace.len() < std::mem::size_of::<SkipBlock>() {
        let new_skip_info_guard = page_alloc_with_fsm(index, PageFlags::SKIP_INFO, false);
        guard.opaque.next_blkno = new_skip_info_guard.blkno();
        term_meta.skip_info_last_blkno = new_skip_info_guard.blkno();
        guard = new_skip_info_guard;
        freespace = guard.freespace_mut();
    }
    freespace[..std::mem::size_of::<SkipBlock>()].copy_from_slice(bytemuck::bytes_of(&skip_info));
    guard.header.pd_lower += std::mem::size_of::<SkipBlock>() as u16;
}

fn init_block_data_writer<'a>(
    index: pgrx::pg_sys::Relation,
    state: &'a mut Option<VirtualPageWriter>,
    term_meta: &mut PostingTermMetaData,
) -> &'a mut VirtualPageWriter {
    if state.is_none() {
        if term_meta.block_data_blkno == pgrx::pg_sys::InvalidBlockNumber {
            let writer = VirtualPageWriter::new(index, PageFlags::BLOCK_DATA, false);
            term_meta.block_data_blkno = writer.first_blkno();
            *state = Some(writer);
        } else {
            let writer = VirtualPageWriter::open(index, term_meta.block_data_blkno, false);
            *state = Some(writer);
        }
    }

    state.as_mut().unwrap()
}
