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
// Copyright (c) 2025-2026 TensorChord Inc.

use index::relation::{
    Opaque, Page, PageGuard, Relation, RelationId, RelationPrefetch, RelationRead,
    RelationReadTypes, RelationWrite, RelationWriteTypes,
};
use std::marker::PhantomData;
use std::mem::{MaybeUninit, offset_of};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

#[repr(C, align(8))]
#[derive(Debug)]
pub struct PostgresPage<O> {
    header: pgrx::pg_sys::PageHeaderData,
    content: [u8; pgrx::pg_sys::BLCKSZ as usize - size_of::<pgrx::pg_sys::PageHeaderData>()],
    _opaque: PhantomData<fn(O) -> O>,
}

// It is a non-guaranteed detection.
// If `PageHeaderData` contains padding bytes, const-eval probably fails.
const _: () = {
    use pgrx::pg_sys::PageHeaderData as T;
    use std::mem::{transmute, zeroed};
    const _ZERO: &[u8; size_of::<T>()] = unsafe { transmute(&zeroed::<T>()) };
};

// Layout checks of header.
const _: () = {
    use pgrx::pg_sys::{MAXIMUM_ALIGNOF, PageHeaderData as T};
    assert!(size_of::<T>() == offset_of!(T, pd_linp));
    assert!(size_of::<T>() % MAXIMUM_ALIGNOF as usize == 0);
};

const _: () = assert!(align_of::<PostgresPage<()>>() == pgrx::pg_sys::MAXIMUM_ALIGNOF as usize);
const _: () = assert!(size_of::<PostgresPage<()>>() == pgrx::pg_sys::BLCKSZ as usize);

impl<O: Opaque> Page for PostgresPage<O> {
    type Opaque = O;
    fn get_opaque(&self) -> &O {
        assert!(self.header.pd_special as usize + size_of::<O>() == size_of::<Self>());
        unsafe { &*((self as *const _ as *const O).byte_add(self.header.pd_special as _)) }
    }
    fn get_opaque_mut(&mut self) -> &mut O {
        assert!(self.header.pd_special as usize + size_of::<O>() == size_of::<Self>());
        unsafe { &mut *((self as *mut _ as *mut O).byte_add(self.header.pd_special as _)) }
    }
    fn len(&self) -> u16 {
        use pgrx::pg_sys::{ItemIdData, PageHeaderData};
        assert!(self.header.pd_lower as usize <= size_of::<Self>());
        assert!(self.header.pd_upper as usize <= size_of::<Self>());
        let lower = self.header.pd_lower as usize;
        let upper = self.header.pd_upper as usize;
        assert!(offset_of!(PageHeaderData, pd_linp) <= lower && lower <= upper);
        ((lower - offset_of!(PageHeaderData, pd_linp)) / size_of::<ItemIdData>()) as u16
    }
    fn get(&self, i: u16) -> Option<&[u8]> {
        use pgrx::pg_sys::{ItemIdData, PageHeaderData};
        if i == 0 {
            return None;
        }
        assert!(self.header.pd_lower as usize <= size_of::<Self>());
        let lower = self.header.pd_lower as usize;
        assert!(offset_of!(PageHeaderData, pd_linp) <= lower);
        let n = ((lower - offset_of!(PageHeaderData, pd_linp)) / size_of::<ItemIdData>()) as u16;
        if i > n {
            return None;
        }
        let iid = unsafe { self.header.pd_linp.as_ptr().add((i - 1) as _).read() };
        let lp_off = iid.lp_off() as usize;
        let lp_len = iid.lp_len() as usize;
        match lp_flags(iid) {
            pgrx::pg_sys::LP_UNUSED => return None,
            pgrx::pg_sys::LP_NORMAL => (),
            pgrx::pg_sys::LP_REDIRECT => unimplemented!(),
            pgrx::pg_sys::LP_DEAD => unimplemented!(),
            _ => unreachable!(),
        }
        assert!(offset_of!(PageHeaderData, pd_linp) <= lp_off);
        assert!(lp_off <= size_of::<Self>());
        assert!(lp_len <= size_of::<Self>());
        assert!(lp_off + lp_len <= size_of::<Self>());
        unsafe {
            let ptr = (self as *const Self).cast::<u8>().add(lp_off as _);
            Some(std::slice::from_raw_parts(ptr, lp_len as _))
        }
    }
    fn get_mut(&mut self, i: u16) -> Option<&mut [u8]> {
        use pgrx::pg_sys::{ItemIdData, PageHeaderData};
        if i == 0 {
            return None;
        }
        assert!(self.header.pd_lower as usize <= size_of::<Self>());
        let lower = self.header.pd_lower as usize;
        assert!(offset_of!(PageHeaderData, pd_linp) <= lower);
        let n = ((lower - offset_of!(PageHeaderData, pd_linp)) / size_of::<ItemIdData>()) as u16;
        if i > n {
            return None;
        }
        let iid = unsafe { self.header.pd_linp.as_ptr().add((i - 1) as _).read() };
        let lp_off = iid.lp_off() as usize;
        let lp_len = iid.lp_len() as usize;
        match lp_flags(iid) {
            pgrx::pg_sys::LP_UNUSED => return None,
            pgrx::pg_sys::LP_NORMAL => (),
            pgrx::pg_sys::LP_REDIRECT => unimplemented!(),
            pgrx::pg_sys::LP_DEAD => unimplemented!(),
            _ => unreachable!(),
        }
        assert!(offset_of!(PageHeaderData, pd_linp) <= lp_off);
        assert!(lp_off <= size_of::<Self>());
        assert!(lp_len <= size_of::<Self>());
        assert!(lp_off + lp_len <= size_of::<Self>());
        unsafe {
            let ptr = (self as *mut Self).cast::<u8>().add(lp_off as _);
            Some(std::slice::from_raw_parts_mut(ptr, lp_len as _))
        }
    }
    fn alloc(&mut self, data: &[u8]) -> Option<u16> {
        unsafe {
            let i = pgrx::pg_sys::PageAddItemExtended(
                (self as *const Self).cast_mut().cast(),
                data.as_ptr().cast_mut().cast(),
                data.len(),
                0,
                0,
            );
            if i == 0 { None } else { Some(i) }
        }
    }
    fn free(&mut self, i: u16) {
        unsafe {
            pgrx::pg_sys::PageIndexTupleDeleteNoCompact((self as *mut Self).cast(), i);
        }
    }
    fn freespace(&self) -> u16 {
        unsafe { pgrx::pg_sys::PageGetFreeSpace((self as *const Self).cast_mut().cast()) as u16 }
    }
    fn clear(&mut self, opaque: O) {
        unsafe {
            page_init(self, opaque);
        }
    }
}

unsafe fn page_init<O: Opaque>(this: *mut PostgresPage<O>, opaque: O) {
    unsafe {
        use pgrx::pg_sys::{BLCKSZ, PageHeaderData, PageInit};
        PageInit(this.cast(), BLCKSZ as usize, size_of::<O>());
        assert_eq!(
            (*this.cast::<PageHeaderData>()).pd_special as usize + size_of::<O>(),
            size_of::<PostgresPage<O>>()
        );
        this.cast::<O>()
            .byte_add(size_of::<PostgresPage<O>>() - size_of::<O>())
            .write(opaque);
    }
}

pub struct PostgresBufferReadGuard<Opaque> {
    buf: i32,
    page: NonNull<PostgresPage<Opaque>>,
    id: u32,
}

impl<Opaque> PageGuard for PostgresBufferReadGuard<Opaque> {
    fn id(&self) -> u32 {
        self.id
    }
}

impl<Opaque> Deref for PostgresBufferReadGuard<Opaque> {
    type Target = PostgresPage<Opaque>;

    fn deref(&self) -> &PostgresPage<Opaque> {
        unsafe { self.page.as_ref() }
    }
}

impl<Opaque> Drop for PostgresBufferReadGuard<Opaque> {
    fn drop(&mut self) {
        unsafe {
            pgrx::pg_sys::UnlockReleaseBuffer(self.buf);
        }
    }
}

pub struct PostgresBufferWriteGuard<O: Opaque> {
    buf: i32,
    page: NonNull<PostgresPage<O>>,
    state: *mut pgrx::pg_sys::GenericXLogState,
    id: u32,
}

impl<O: Opaque> PageGuard for PostgresBufferWriteGuard<O> {
    fn id(&self) -> u32 {
        self.id
    }
}

impl<O: Opaque> Deref for PostgresBufferWriteGuard<O> {
    type Target = PostgresPage<O>;

    fn deref(&self) -> &PostgresPage<O> {
        unsafe { self.page.as_ref() }
    }
}

impl<O: Opaque> DerefMut for PostgresBufferWriteGuard<O> {
    fn deref_mut(&mut self) -> &mut PostgresPage<O> {
        unsafe { self.page.as_mut() }
    }
}

impl<O: Opaque> Drop for PostgresBufferWriteGuard<O> {
    fn drop(&mut self) {
        unsafe {
            if std::thread::panicking() {
                pgrx::pg_sys::GenericXLogAbort(self.state);
            } else {
                pgrx::pg_sys::GenericXLogFinish(self.state);
            }
            pgrx::pg_sys::UnlockReleaseBuffer(self.buf);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PostgresRelation<Opaque> {
    raw: pgrx::pg_sys::Relation,
    _phantom: PhantomData<fn(Opaque) -> Opaque>,
}

impl<Opaque> PostgresRelation<Opaque> {
    pub unsafe fn new(raw: pgrx::pg_sys::Relation) -> Self {
        Self {
            raw,
            _phantom: PhantomData,
        }
    }
}

impl<O: Opaque> Relation for PostgresRelation<O> {
    type Page = PostgresPage<O>;
}

impl<O: Opaque> RelationId for PostgresRelation<O> {
    fn id(&self) -> u32 {
        unsafe {
            let oid = (*self.raw).rd_id;
            debug_assert!(oid != pgrx::pg_sys::Oid::INVALID);
            oid.to_u32()
        }
    }
}

impl<O: Opaque> RelationReadTypes for PostgresRelation<O> {
    type ReadGuard<'a> = PostgresBufferReadGuard<O>;
}

impl<O: Opaque> RelationRead for PostgresRelation<O> {
    fn read(&self, id: u32) -> Self::ReadGuard<'_> {
        assert!(id != u32::MAX, "no such page");
        unsafe {
            use pgrx::pg_sys::{
                BUFFER_LOCK_SHARE, BufferGetPage, ForkNumber, LockBuffer, ReadBufferExtended,
                ReadBufferMode,
            };
            let buf = ReadBufferExtended(
                self.raw,
                ForkNumber::MAIN_FORKNUM,
                id,
                ReadBufferMode::RBM_NORMAL,
                std::ptr::null_mut(),
            );
            LockBuffer(buf, BUFFER_LOCK_SHARE as _);
            let page = NonNull::new(BufferGetPage(buf).cast()).expect("failed to get page");
            PostgresBufferReadGuard { buf, page, id }
        }
    }
}

impl<O: Opaque> RelationWriteTypes for PostgresRelation<O> {
    type WriteGuard<'a> = PostgresBufferWriteGuard<O>;
}

impl<O: Opaque> RelationWrite for PostgresRelation<O> {
    fn write(&self, id: u32) -> PostgresBufferWriteGuard<O> {
        assert!(id != u32::MAX, "no such page");
        unsafe {
            use pgrx::pg_sys::{
                BUFFER_LOCK_EXCLUSIVE, ForkNumber, GenericXLogRegisterBuffer, GenericXLogStart,
                LockBuffer, ReadBufferExtended, ReadBufferMode,
            };
            let buf = ReadBufferExtended(
                self.raw,
                ForkNumber::MAIN_FORKNUM,
                id,
                ReadBufferMode::RBM_NORMAL,
                std::ptr::null_mut(),
            );
            LockBuffer(buf, BUFFER_LOCK_EXCLUSIVE as _);
            let state = GenericXLogStart(self.raw);
            let page = NonNull::new(
                GenericXLogRegisterBuffer(state, buf, 0).cast::<MaybeUninit<PostgresPage<O>>>(),
            )
            .expect("failed to get page");
            PostgresBufferWriteGuard {
                buf,
                page: page.cast(),
                state,
                id,
            }
        }
    }
    fn alloc(&self, opaque: <Self::Page as Page>::Opaque) -> PostgresBufferWriteGuard<O> {
        unsafe {
            use pgrx::pg_sys::{
                GENERIC_XLOG_FULL_IMAGE, GenericXLogRegisterBuffer, GenericXLogStart,
            };
            let buf = loop {
                use pgrx::pg_sys::{
                    BUFFER_LOCK_UNLOCK, BufferGetPage, ConditionalLockBuffer, LockBuffer,
                    PageIsNew, ReadBuffer, ReleaseBuffer,
                };
                let blkno = pgrx::pg_sys::GetFreeIndexPage(self.raw);
                if blkno == pgrx::pg_sys::InvalidBlockNumber {
                    break None;
                }
                let buf = ReadBuffer(self.raw, blkno);
                if ConditionalLockBuffer(buf) {
                    let page = BufferGetPage(buf);
                    if PageIsNew(page) {
                        break Some(buf);
                    }
                    let page = page.cast::<PostgresPage<O>>();
                    if (*page).get_opaque().is_deleted() {
                        break Some(buf);
                    }
                    LockBuffer(buf, BUFFER_LOCK_UNLOCK as _);
                }
                ReleaseBuffer(buf);
            };
            let buf = if let Some(buf) = buf {
                buf
            } else {
                #[cfg(any(feature = "pg14", feature = "pg15"))]
                {
                    use pgrx::pg_sys::{
                        BUFFER_LOCK_EXCLUSIVE, ExclusiveLock, ForkNumber, LockBuffer,
                        LockRelationForExtension, ReadBufferExtended, ReadBufferMode,
                        UnlockRelationForExtension,
                    };
                    LockRelationForExtension(self.raw, ExclusiveLock as _);
                    let buf = ReadBufferExtended(
                        self.raw,
                        ForkNumber::MAIN_FORKNUM,
                        u32::MAX,
                        ReadBufferMode::RBM_NORMAL,
                        std::ptr::null_mut(),
                    );
                    UnlockRelationForExtension(self.raw, ExclusiveLock as _);
                    LockBuffer(buf, BUFFER_LOCK_EXCLUSIVE as _);
                    buf
                }
                #[cfg(any(feature = "pg16", feature = "pg17", feature = "pg18"))]
                {
                    use pgrx::pg_sys::{
                        BufferManagerRelation, ExtendBufferedFlags, ExtendBufferedRel, ForkNumber,
                    };
                    let bmr = BufferManagerRelation {
                        rel: self.raw,
                        smgr: std::ptr::null_mut(),
                        relpersistence: 0,
                    };
                    ExtendBufferedRel(
                        bmr,
                        ForkNumber::MAIN_FORKNUM,
                        std::ptr::null_mut(),
                        ExtendBufferedFlags::EB_LOCK_FIRST as _,
                    )
                }
            };
            let state = GenericXLogStart(self.raw);
            let mut page = NonNull::new(
                GenericXLogRegisterBuffer(state, buf, GENERIC_XLOG_FULL_IMAGE as _)
                    .cast::<MaybeUninit<PostgresPage<O>>>(),
            )
            .expect("failed to get page");
            page_init(page.as_mut().as_mut_ptr(), opaque);
            PostgresBufferWriteGuard {
                buf,
                page: page.cast(),
                state,
                id: pgrx::pg_sys::BufferGetBlockNumber(buf),
            }
        }
    }
    fn free(&self, mut guard: Self::WriteGuard<'_>) {
        guard.get_opaque_mut().set_deleted();
        unsafe {
            pgrx::pg_sys::RecordFreeIndexPage(self.raw, guard.id());
        }
    }
    fn vacuum(&self) {
        unsafe {
            pgrx::pg_sys::IndexFreeSpaceMapVacuum(self.raw);
        }
    }
}

impl<O: Opaque> RelationPrefetch for PostgresRelation<O> {
    fn prefetch(&self, id: u32) {
        assert!(id != u32::MAX, "no such page");
        unsafe {
            use pgrx::pg_sys::PrefetchBuffer;
            PrefetchBuffer(self.raw, 0, id);
        }
    }
}

#[inline(always)]
fn lp_flags(x: pgrx::pg_sys::ItemIdData) -> u32 {
    let x: u32 = unsafe { std::mem::transmute(x) };
    (x >> 15) & 0b11
}

// Emulate unstable library feature `box_vec_non_null`.
// See https://github.com/rust-lang/rust/issues/130364.

#[allow(dead_code)]
#[must_use]
fn box_into_non_null<T>(b: Box<T>) -> NonNull<T> {
    unsafe { NonNull::new_unchecked(Box::into_raw(b)) }
}
