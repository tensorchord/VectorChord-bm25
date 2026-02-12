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

use crate::page::BM25_PAGE_SIZE;

use super::{PageReadGuard, page_read, page_write};

pub struct ContinuousPageReader<T> {
    index: pgrx::pg_sys::Relation,
    start_blkno: pgrx::pg_sys::BlockNumber,
    phantom: std::marker::PhantomData<T>,
}

impl<T: Copy> ContinuousPageReader<T> {
    const PAGE_COUNT: u32 = {
        assert!(align_of::<T>() <= 8);
        (BM25_PAGE_SIZE / size_of::<T>()) as u32
    };

    pub fn new(index: pgrx::pg_sys::Relation, start_blkno: pgrx::pg_sys::BlockNumber) -> Self {
        Self {
            index,
            start_blkno,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn read(&self, idx: u32) -> T {
        let blkno_offset = idx / Self::PAGE_COUNT;
        let blkno = self.start_blkno + blkno_offset as pgrx::pg_sys::BlockNumber;
        let offset = (idx % Self::PAGE_COUNT) as usize;
        let page = page_read(self.index, blkno);
        unsafe { page.data().as_ptr().cast::<T>().add(offset).read() }
    }

    pub fn update(&self, idx: u32, f: impl FnOnce(&mut T)) {
        let blkno_offset = idx / Self::PAGE_COUNT;
        let blkno = self.start_blkno + blkno_offset as pgrx::pg_sys::BlockNumber;
        let offset = (idx % Self::PAGE_COUNT) as usize;
        let mut page = page_write(self.index, blkno);
        let data = page.data_mut();
        let ptr = unsafe { data.as_mut_ptr().cast::<T>().add(offset) };
        f(unsafe { &mut *ptr });
    }
}

pub struct PageReader {
    index: pgrx::pg_sys::Relation,
    blkno: pgrx::pg_sys::BlockNumber,
    inner: Option<PageReadGuard>,
    offset: usize,
}

impl PageReader {
    pub fn new(index: pgrx::pg_sys::Relation, blkno: pgrx::pg_sys::BlockNumber) -> Self {
        Self {
            index,
            blkno,
            inner: None,
            offset: 0,
        }
    }

    pub fn blkno(&self) -> pgrx::pg_sys::BlockNumber {
        self.blkno
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl std::io::Read for PageReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.blkno == pgrx::pg_sys::InvalidBlockNumber {
            return Ok(0);
        }
        let inner = self
            .inner
            .get_or_insert_with(|| page_read(self.index, self.blkno));

        let data = &inner.data()[self.offset..];
        let to_read = std::cmp::min(buf.len(), data.len());
        buf[..to_read].copy_from_slice(&data[..to_read]);
        self.offset += to_read;
        if to_read == data.len() {
            self.blkno = inner.opaque.next_blkno;
            self.offset = 0;
            self.inner = None;
        }
        Ok(to_read)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        if self.blkno == pgrx::pg_sys::InvalidBlockNumber {
            return Ok(0);
        }
        let mut blkno = self.blkno;
        self.blkno = pgrx::pg_sys::InvalidBlockNumber;
        let mut inner = self
            .inner
            .take()
            .unwrap_or_else(|| page_read(self.index, blkno));
        let mut read_len = 0;
        loop {
            let data = &inner.data()[self.offset..];
            buf.extend_from_slice(data);
            read_len += data.len();
            blkno = inner.opaque.next_blkno;
            self.offset = 0;
            if blkno == pgrx::pg_sys::InvalidBlockNumber {
                break;
            } else {
                inner = page_read(self.index, blkno);
            }
        }

        Ok(read_len)
    }
}
