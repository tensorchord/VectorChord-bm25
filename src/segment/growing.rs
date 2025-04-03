use std::{io::Read, num::NonZero};

use lending_iterator::{lending_iterator::LendingIteratorඞItem, LendingIterator, HKT};

use crate::{
    datatype::{Bm25VectorBorrowed, Bm25VectorHeader, Bm25VectorInput},
    guc::SEGMENT_GROWING_MAX_PAGE_SIZE,
    page::{
        bm25_page_size, page_alloc_with_fsm, page_append_item, page_get_item, page_get_item_id,
        page_get_max_offset_number, page_read, page_write, PageFlags, PageReadGuard, PageReader,
        PageWriter,
    },
};

use super::meta::MetaPageData;

/// store bm25vector
#[derive(Debug, Clone, Copy)]
pub struct GrowingSegmentData {
    pub first_blkno: NonZero<u32>,
    pub last_blkno: pgrx::pg_sys::BlockNumber,
    pub growing_full_page_count: u32,
}

pub struct GrowingSegmentReader {
    index: pgrx::pg_sys::Relation,
    blkno: pgrx::pg_sys::BlockNumber,
}

impl GrowingSegmentReader {
    pub fn new(index: pgrx::pg_sys::Relation, data: &GrowingSegmentData) -> Self {
        Self {
            index,
            blkno: data.first_blkno.get(),
        }
    }

    pub fn into_lending_iter(
        self,
        page_limit: usize,
    ) -> impl LendingIterator + for<'a> LendingIteratorඞItem<'a, T = Bm25VectorBorrowed<'a>> {
        struct TmpState {
            index: pgrx::pg_sys::Relation,
            blkno: pgrx::pg_sys::BlockNumber,
            page: Option<PageReadGuard>,
            redirect_data: Vec<u8>,
            offset: u16,
            count: u16,
            page_limit: usize,
        }

        impl TmpState {
            fn page(&self) -> &PageReadGuard {
                self.page.as_ref().unwrap()
            }
        }

        let GrowingSegmentReader { index, blkno } = self;
        let page = page_read(index, blkno);
        let count = page_get_max_offset_number(&page);
        let state = TmpState {
            index,
            blkno,
            page: Some(page),
            redirect_data: Vec::new(),
            offset: 1,
            count,
            page_limit: page_limit - 1,
        };

        lending_iterator::from_fn::<HKT!(Bm25VectorBorrowed<'_>), _, _>(state, |state| {
            if state.blkno == pgrx::pg_sys::InvalidBlockNumber {
                return None;
            }
            if state.offset > state.count {
                state.blkno = state.page().opaque.next_blkno;
                if state.blkno == pgrx::pg_sys::InvalidBlockNumber || state.page_limit == 0 {
                    state.page = None;
                    return None;
                }
                state.page = Some(page_read(state.index, state.blkno));
                state.offset = 1;
                state.count = page_get_max_offset_number(state.page());
                state.page_limit -= 1;
            }
            let offset = state.offset;
            state.offset += 1;
            let item_id = page_get_item_id(state.page(), offset);

            if item_id.lp_flags() == pgrx::pg_sys::LP_REDIRECT {
                state.redirect_data.clear();
                let first_blkno: &u32 = page_get_item(state.page(), item_id);
                let mut reader = PageReader::new(state.index, *first_blkno);
                reader.read_to_end(&mut state.redirect_data).unwrap();
                let item: &Bm25VectorHeader = unsafe { &*state.redirect_data.as_ptr().cast() };
                return Some(item.borrow());
            }

            let item: &Bm25VectorHeader = page_get_item(state.page(), item_id);
            Some(item.borrow())
        })
    }
}

pub struct SealingTask {
    pub page_count: u32,
}

/// - if no growing segment, create one
/// - append to the last page
/// - if growing segment is full, seal it
///
/// return (first_blkno, growing_full_page_count) if growing segment is full
pub fn growing_segment_insert(
    index: pgrx::pg_sys::Relation,
    meta: &mut MetaPageData,
    bm25vector: &Bm25VectorInput,
) -> Option<SealingTask> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(bm25vector.to_bytes());
    let mut redirect = false;

    if buf.len() > bm25_page_size() - std::mem::size_of::<pgrx::pg_sys::ItemIdData>() {
        let mut writer = PageWriter::new(index, PageFlags::GROWING_REDIRECT, false);
        writer.write(&buf);
        let first_blkno = writer.finalize();
        buf.clear();
        buf.extend_from_slice(&first_blkno.to_le_bytes());
        redirect = true;
    }

    let Some(growing_segment) = &mut meta.growing_segment else {
        let mut page = page_alloc_with_fsm(index, PageFlags::GROWING, false);
        meta.growing_segment = Some(GrowingSegmentData {
            first_blkno: NonZero::new(page.blkno()).unwrap(),
            last_blkno: page.blkno(),
            growing_full_page_count: 0,
        });
        let success = page_append_item(&mut page, &buf, redirect);
        assert!(success);
        return None;
    };

    let mut page = page_write(index, growing_segment.last_blkno);
    if !page_append_item(&mut page, &buf, redirect) {
        let mut new_page = page_alloc_with_fsm(index, PageFlags::GROWING, false);
        let success = page_append_item(&mut new_page, &buf, redirect);
        assert!(success);
        page.opaque.next_blkno = new_page.blkno();
        growing_segment.last_blkno = new_page.blkno();
        growing_segment.growing_full_page_count += 1;
        if growing_segment.growing_full_page_count >= SEGMENT_GROWING_MAX_PAGE_SIZE.get() as u32 {
            return Some(SealingTask {
                page_count: growing_segment.growing_full_page_count,
            });
        }
    }
    None
}
