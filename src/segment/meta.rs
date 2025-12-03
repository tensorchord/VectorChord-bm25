use super::{growing::GrowingSegmentData, sealed::SealedSegmentData};

pub const META_VERSION: u32 = 1;

#[derive(Debug)]
pub struct MetaPageData {
    pub version: u32,
    pub doc_cnt: u32,
    pub doc_term_cnt: u64,
    pub term_id_cnt: u32,
    pub sealed_doc_id: u32,
    pub current_doc_id: u32,
    pub field_norm_blkno: u32,
    pub payload_blkno: u32,
    pub term_stat_blkno: u32,
    pub delete_bitmap_blkno: u32,
    pub growing_segment: Option<GrowingSegmentData>,
    pub sealed_segment: SealedSegmentData,
}

impl Default for MetaPageData {
    fn default() -> Self {
        MetaPageData {
            version: META_VERSION,
            doc_cnt: 0,
            doc_term_cnt: 0,
            term_id_cnt: 0,
            field_norm_blkno: pgrx::pg_sys::InvalidBlockNumber,
            payload_blkno: pgrx::pg_sys::InvalidBlockNumber,
            term_stat_blkno: pgrx::pg_sys::InvalidBlockNumber,
            delete_bitmap_blkno: pgrx::pg_sys::InvalidBlockNumber,
            current_doc_id: 0,
            sealed_doc_id: 0,
            growing_segment: None,
            sealed_segment: SealedSegmentData {
                term_info_blkno: pgrx::pg_sys::InvalidBlockNumber,
                term_id_cnt: 0,
            },
        }
    }
}

impl MetaPageData {
    pub fn avgdl(&self) -> f32 {
        self.doc_term_cnt as f32 / self.doc_cnt as f32
    }
}
