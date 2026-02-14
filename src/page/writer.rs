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

use std::ops::DerefMut;

use super::{PageFlags, PageWriteGuard, page_alloc_init_forknum, page_alloc_with_fsm, page_write};

pub struct PageWriterInitFork {
    relation: pgrx::pg_sys::Relation,
    flag: PageFlags,
    first_blkno: pgrx::pg_sys::BlockNumber,
    page: Option<PageWriteGuard>,
}

impl PageWriterInitFork {
    #[allow(dead_code)]
    pub unsafe fn new(relation: pgrx::pg_sys::Relation, flag: PageFlags) -> Self {
        Self {
            relation,
            flag,
            first_blkno: pgrx::pg_sys::InvalidBlockNumber,
            page: None,
        }
    }

    #[allow(dead_code)]
    pub fn finalize(self) -> pgrx::pg_sys::BlockNumber {
        self.first_blkno
    }

    #[allow(dead_code)]
    fn change_page(&mut self) {
        let mut old_page = self.page.take().unwrap();
        let new_page = unsafe { page_alloc_init_forknum(self.relation, self.flag) };
        old_page.opaque.next_blkno = new_page.blkno();
        self.page = Some(new_page);
    }

    #[allow(dead_code)]
    fn offset(&mut self) -> &mut u16 {
        let page = self.page.as_mut().unwrap().deref_mut();
        &mut page.header.pd_lower
    }

    #[allow(dead_code)]
    fn freespace_mut(&mut self) -> &mut [u8] {
        if self.page.is_none() {
            let page = unsafe { page_alloc_init_forknum(self.relation, self.flag) };
            self.first_blkno = page.blkno();
            self.page = Some(page);
        }
        self.page.as_mut().unwrap().deref_mut().freespace_mut()
    }

    #[allow(dead_code)]
    pub fn write(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let space = self.freespace_mut();
            let space_len = space.len();
            let len = space_len.min(data.len());
            space[..len].copy_from_slice(&data[..len]);
            *self.offset() += len as u16;
            if len == space_len {
                self.change_page();
            }
            data = &data[len..];
        }
    }
}

pub struct PageWriter {
    relation: pgrx::pg_sys::Relation,
    flag: PageFlags,
    skip_lock_rel: bool,
    first_blkno: pgrx::pg_sys::BlockNumber,
    page: Option<PageWriteGuard>,
}

impl PageWriter {
    pub unsafe fn new(
        relation: pgrx::pg_sys::Relation,
        flag: PageFlags,
        skip_lock_rel: bool,
    ) -> Self {
        Self {
            relation,
            flag,
            skip_lock_rel,
            first_blkno: pgrx::pg_sys::InvalidBlockNumber,
            page: None,
        }
    }

    #[allow(dead_code)]
    pub unsafe fn open(
        relation: pgrx::pg_sys::Relation,
        last_blkno: pgrx::pg_sys::BlockNumber,
        skip_lock_rel: bool,
    ) -> Self {
        let page = unsafe { page_write(relation, last_blkno) };
        Self {
            relation,
            flag: page.opaque.page_flag,
            skip_lock_rel,
            first_blkno: pgrx::pg_sys::InvalidBlockNumber,
            page: Some(page),
        }
    }
}

impl PageWriter {
    pub fn finalize(self) -> pgrx::pg_sys::BlockNumber {
        self.first_blkno
    }

    pub fn blkno(&self) -> pgrx::pg_sys::BlockNumber {
        self.page.as_ref().unwrap().blkno()
    }

    fn change_page(&mut self) {
        let mut old_page = self.page.take().unwrap();
        let new_page = unsafe { page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel) };
        old_page.opaque.next_blkno = new_page.blkno();
        self.page = Some(new_page);
    }

    fn offset(&mut self) -> &mut u16 {
        let page = self.page.as_mut().unwrap().deref_mut();
        &mut page.header.pd_lower
    }

    fn freespace_mut(&mut self) -> &mut [u8] {
        if self.page.is_none() {
            let page = unsafe { page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel) };
            self.first_blkno = page.blkno();
            self.page = Some(page);
        }
        self.page.as_mut().unwrap().deref_mut().freespace_mut()
    }

    pub fn write(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let space = self.freespace_mut();
            let space_len = space.len();
            let len = space_len.min(data.len());
            space[..len].copy_from_slice(&data[..len]);
            *self.offset() += len as u16;
            if len == space_len {
                self.change_page();
            }
            data = &data[len..];
        }
    }
}
