use crate::segment::{
    meta::MetaPageData,
    posting::{PostingTermMetaData, SkipBlock},
};

use super::{page_read, PageFlags};

#[pgrx::pg_extern]
fn bm25_page_inspect(index: pgrx::PgRelation, blkno: i32) -> String {
    let page = page_read(index.as_ptr(), blkno.try_into().unwrap());
    match page.opaque.page_flag {
        PageFlags::META => {
            let meta_page: &MetaPageData = page.as_ref();
            format!("Meta Page:\n{:#?}", meta_page)
        }
        PageFlags::PAYLOAD => {
            let data: &[u64] = bytemuck::cast_slice(page.data());
            format!("Payload Page ({} entries):\n{:?}", data.len(), data)
        }
        PageFlags::FIELD_NORM => {
            let data: &[u8] = page.data();
            format!("Field Norm Page ({} entries):\n{:?}", data.len(), data)
        }
        PageFlags::TERM_STATISTIC => {
            let data: &[u32] = bytemuck::cast_slice(page.data());
            format!("Term Statistic Page ({} entries):\n{:?}", data.len(), data)
        }
        PageFlags::TERM_INFO => {
            let data: &[u32] = bytemuck::cast_slice(page.data());
            format!("Term Info Page ({} entries):\n{:?}", data.len(), data)
        }
        PageFlags::TERM_META => {
            let term_meta: &PostingTermMetaData = page.as_ref();
            format!("Term Meta Page:\n{:#?}", term_meta)
        }
        PageFlags::SKIP_INFO => {
            let data: &[SkipBlock] = bytemuck::cast_slice(page.data());
            format!("Skip Info Page ({} entries):\n{:?}", data.len(), data)
        }
        PageFlags::BLOCK_DATA => {
            let data: &[u8] = page.data();
            format!("Block Data Page ({} bytes):\n{:02X?}", data.len(), data)
        }
        PageFlags::GROWING => {
            let data: &[u8] = page.data();
            format!("Delete Bitmap Page ({} bytes):\n{:02X?}", data.len(), data)
        }
        PageFlags::GROWING_REDIRECT => {
            let data: &[u8] = page.data();
            format!(
                "Delete Bitmap Redirect Page ({} bytes):\n{:02X?}",
                data.len(),
                data
            )
        }
        PageFlags::DELETE => {
            let data: &[u8] = page.data();
            format!("Delete Bitmap Page ({} bytes):\n{:02X?}", data.len(), data)
        }
        PageFlags::VIRTUAL_INODE => {
            let data: &[u32] = bytemuck::cast_slice(page.data());
            format!("Virtual Inode Page ({} entries):\n{:?}", data.len(), data)
        }
        PageFlags::FREE => "Free Page".to_string(),
        _ => {
            let data: &[u8] = page.data();
            format!(
                "Unknown Page Flag {:?} ({} bytes):\n{:02X?}",
                page.opaque.page_flag,
                data.len(),
                data
            )
        }
    }
}
