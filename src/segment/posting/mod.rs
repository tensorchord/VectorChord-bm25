mod append;
mod reader;
mod serializer;
mod writer;

use std::num::NonZero;

use bytemuck::{Pod, Zeroable};

pub use append::InvertedAppender;
pub use reader::{PostingCursor, PostingTermInfoReader};
pub use serializer::{InvertedSerializer, InvertedWrite};
pub use writer::InvertedWriter;

pub const TERMINATED_DOC: u32 = u32::MAX;

pub const COMPRESSION_BLOCK_SIZE: usize = 128;

#[derive(Clone, Copy)]
pub struct PostingTermInfo {
    pub meta_blkno: pgrx::pg_sys::BlockNumber,
}

impl PostingTermInfo {
    pub fn empty() -> Self {
        Self {
            meta_blkno: pgrx::pg_sys::InvalidBlockNumber,
        }
    }
}

unsafe impl Zeroable for PostingTermInfo {}
unsafe impl Pod for PostingTermInfo {}

pub struct PostingTermMetaData {
    pub skip_info_blkno: pgrx::pg_sys::BlockNumber,
    pub skip_info_last_blkno: pgrx::pg_sys::BlockNumber,
    pub block_data_blkno: pgrx::pg_sys::BlockNumber,
    pub block_count: u32,
    pub last_full_block_last_docid: Option<NonZero<u32>>,
    pub unfulled_doc_cnt: u32,
    pub unfulled_docid: [u32; 128],
    pub unfulled_freq: [u32; 128],
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct SkipBlockFlags: u8 {
        const UNFULLED = 1 << 0;
        const PAGE_CHANGED = 1 << 1;
    }
}

impl Default for SkipBlockFlags {
    fn default() -> Self {
        SkipBlockFlags::empty()
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct SkipBlock {
    last_doc: u32,
    blockwand_tf: u32,
    #[allow(dead_code)]
    doc_cnt: u32, // unused now
    size: u16,
    blockwand_fieldnorm_id: u8,
    flag: SkipBlockFlags,
}

unsafe impl Zeroable for SkipBlock {}
unsafe impl Pod for SkipBlock {}
