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

use bm25::vector::Bm25VectorBorrowed;
use pgrx::datum::{FromDatum, IntoDatum};
use pgrx::pg_sys::{Datum, Oid};
use pgrx::pgrx_sql_entity_graph::metadata::*;
use std::marker::PhantomData;
use std::ptr::NonNull;

#[repr(C)]
pub struct Bm25VectorHeader {
    varlena: u32,
    len: u32,
    elements: [u8; 0],
}

impl Bm25VectorHeader {
    fn size_of_by_len(len: u32) -> usize {
        if len > 65535 {
            panic!("vector is too large");
        }
        size_of::<Self>() + size_of::<u32>() * len as usize + size_of::<u32>() * len as usize
    }
    unsafe fn as_borrowed<'a>(this: NonNull<Self>) -> Bm25VectorBorrowed<'a> {
        unsafe {
            let this = this.as_ptr();
            let len = (&raw const (*this).len).read() as usize;
            Bm25VectorBorrowed::new(
                std::slice::from_raw_parts(
                    #[expect(clippy::erasing_op)]
                    (&raw const (*this).elements).cast::<u32>().add(0 * len),
                    len,
                ),
                std::slice::from_raw_parts(
                    (&raw const (*this).elements).cast::<u32>().add(1 * len),
                    len,
                ),
            )
        }
    }
}

pub struct Bm25VectorInput<'a>(NonNull<Bm25VectorHeader>, PhantomData<&'a ()>, bool);

impl Bm25VectorInput<'_> {
    unsafe fn from_ptr(p: NonNull<Bm25VectorHeader>) -> Self {
        let q = unsafe {
            NonNull::new(pgrx::pg_sys::pg_detoast_datum(p.cast().as_ptr()).cast()).unwrap()
        };
        unsafe {
            let varlena = q.cast::<u32>().read();
            #[cfg(target_endian = "big")]
            let size = varlena as usize;
            #[cfg(target_endian = "little")]
            let size = varlena as usize >> 2;
            let len = q.byte_add(4).cast::<u32>().read();
            assert_eq!(Bm25VectorHeader::size_of_by_len(len), size);
        }
        Bm25VectorInput(q, PhantomData, p != q)
    }
    pub fn as_borrowed(&self) -> Bm25VectorBorrowed<'_> {
        unsafe { Bm25VectorHeader::as_borrowed(self.0) }
    }
}

impl Drop for Bm25VectorInput<'_> {
    fn drop(&mut self) {
        if self.2 {
            unsafe {
                pgrx::pg_sys::pfree(self.0.as_ptr().cast());
            }
        }
    }
}

pub struct Bm25VectorOutput(NonNull<Bm25VectorHeader>);

impl Bm25VectorOutput {
    unsafe fn from_ptr(p: NonNull<Bm25VectorHeader>) -> Self {
        let q = unsafe {
            NonNull::new(pgrx::pg_sys::pg_detoast_datum_copy(p.as_ptr().cast()).cast()).unwrap()
        };
        unsafe {
            let varlena = q.cast::<u32>().read();
            #[cfg(target_endian = "big")]
            let size = varlena as usize;
            #[cfg(target_endian = "little")]
            let size = varlena as usize >> 2;
            let len = q.byte_add(4).cast::<u32>().read();
            assert_eq!(Bm25VectorHeader::size_of_by_len(len), size);
        }
        Self(q)
    }
    pub fn new(vector: Bm25VectorBorrowed<'_>) -> Self {
        unsafe {
            let len = vector.len();
            let size = Bm25VectorHeader::size_of_by_len(len);

            let ptr = pgrx::pg_sys::palloc0(size) as *mut Bm25VectorHeader;
            // SET_VARSIZE_4B
            #[cfg(target_endian = "big")]
            (&raw mut (*ptr).varlena).write((size as u32) & 0x3FFFFFFF);
            #[cfg(target_endian = "little")]
            (&raw mut (*ptr).varlena).write((size << 2) as u32);
            (&raw mut (*ptr).len).write(len);
            std::ptr::copy_nonoverlapping(
                vector.indexes().as_ptr(),
                #[expect(clippy::erasing_op)]
                (&raw mut (*ptr).elements)
                    .cast::<u32>()
                    .add(0 * len as usize),
                len as usize,
            );
            std::ptr::copy_nonoverlapping(
                vector.values().as_ptr(),
                (&raw mut (*ptr).elements)
                    .cast::<u32>()
                    .add(1 * len as usize),
                len as usize,
            );
            Self(NonNull::new(ptr).unwrap())
        }
    }
    pub fn as_borrowed(&self) -> Bm25VectorBorrowed<'_> {
        unsafe { Bm25VectorHeader::as_borrowed(self.0) }
    }
    fn into_raw(self) -> *mut Bm25VectorHeader {
        let result = self.0.as_ptr();
        std::mem::forget(self);
        result
    }
}

impl Drop for Bm25VectorOutput {
    fn drop(&mut self) {
        unsafe {
            pgrx::pg_sys::pfree(self.0.as_ptr().cast());
        }
    }
}

// FromDatum

impl FromDatum for Bm25VectorInput<'_> {
    unsafe fn from_polymorphic_datum(datum: Datum, is_null: bool, _typoid: Oid) -> Option<Self> {
        if is_null {
            None
        } else {
            let ptr = NonNull::new(datum.cast_mut_ptr()).unwrap();
            unsafe { Some(Self::from_ptr(ptr)) }
        }
    }
}

impl FromDatum for Bm25VectorOutput {
    unsafe fn from_polymorphic_datum(datum: Datum, is_null: bool, _typoid: Oid) -> Option<Self> {
        if is_null {
            None
        } else {
            let ptr = NonNull::new(datum.cast_mut_ptr()).unwrap();
            unsafe { Some(Self::from_ptr(ptr)) }
        }
    }
}

// IntoDatum

impl IntoDatum for Bm25VectorOutput {
    fn into_datum(self) -> Option<Datum> {
        Some(Datum::from(self.into_raw()))
    }

    fn type_oid() -> Oid {
        Oid::INVALID
    }

    fn is_compatible_with(_: Oid) -> bool {
        true
    }
}

// UnboxDatum

unsafe impl<'a> pgrx::datum::UnboxDatum for Bm25VectorInput<'a> {
    type As<'src>
        = Bm25VectorInput<'src>
    where
        'a: 'src;
    #[inline]
    unsafe fn unbox<'src>(datum: pgrx::datum::Datum<'src>) -> Self::As<'src>
    where
        Self: 'src,
    {
        let datum = datum.sans_lifetime();
        let ptr = NonNull::new(datum.cast_mut_ptr()).unwrap();
        unsafe { Self::from_ptr(ptr) }
    }
}

unsafe impl pgrx::datum::UnboxDatum for Bm25VectorOutput {
    type As<'src> = Bm25VectorOutput;
    #[inline]
    unsafe fn unbox<'src>(datum: pgrx::datum::Datum<'src>) -> Self::As<'src>
    where
        Self: 'src,
    {
        let datum = datum.sans_lifetime();
        let ptr = NonNull::new(datum.cast_mut_ptr()).unwrap();
        unsafe { Self::from_ptr(ptr) }
    }
}

// SqlTranslatable

unsafe impl SqlTranslatable for Bm25VectorInput<'_> {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("bm25vector")))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("bm25vector"))))
    }
}

unsafe impl SqlTranslatable for Bm25VectorOutput {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("bm25vector")))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("bm25vector"))))
    }
}

// ArgAbi

unsafe impl<'fcx> pgrx::callconv::ArgAbi<'fcx> for Bm25VectorInput<'fcx> {
    unsafe fn unbox_arg_unchecked(arg: pgrx::callconv::Arg<'_, 'fcx>) -> Self {
        let index = arg.index();
        unsafe {
            arg.unbox_arg_using_from_datum()
                .unwrap_or_else(|| panic!("argument {index} must not be null"))
        }
    }
}

// BoxRet

unsafe impl pgrx::callconv::BoxRet for Bm25VectorOutput {
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        match self.into_datum() {
            Some(datum) => unsafe { fcinfo.return_raw_datum(datum) },
            None => fcinfo.return_null(),
        }
    }
}
