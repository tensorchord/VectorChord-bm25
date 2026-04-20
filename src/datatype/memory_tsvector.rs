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

use crate::datatype::tsvector::TsVectorBorrowed;
use pgrx::datum::{FromDatum, IntoDatum};
use pgrx::pg_sys::{Datum, Oid};
use pgrx::pgrx_sql_entity_graph::metadata::*;
use std::marker::PhantomData;
use std::ptr::NonNull;

#[repr(C)]
pub struct TsVectorHeader {
    varlena: u32,
    len: u32,
    entries: [u32; 0],
}

impl TsVectorHeader {
    unsafe fn as_borrowed<'a>(this: NonNull<Self>) -> TsVectorBorrowed<'a> {
        unsafe {
            let this = this.as_ptr();
            let size = (cfg_select! {
                target_endian = "little" => {
                    ((*this).varlena >> 2) & 0x3FFFFFFF
                }
                target_endian = "big" => {
                    (*this).varlena & 0x3FFFFFFF
                }
            } as usize)
                .strict_sub(size_of::<u32>());
            let len = (*this).len as usize;
            let size_0 = size_of::<u32>().strict_mul(len);
            let size_1 = size.strict_sub(size_0);
            let ptr_0 = (*this).entries.as_ptr();
            let ptr_1 = ptr_0.add(len).cast::<u8>();
            let entries = std::slice::from_raw_parts(ptr_0, len);
            let bytes = std::slice::from_raw_parts(ptr_1, size_1);
            TsVectorBorrowed::new(entries, bytes)
        }
    }
}

pub struct TsVectorInput<'a>(NonNull<TsVectorHeader>, PhantomData<&'a ()>, bool);

impl TsVectorInput<'_> {
    unsafe fn from_ptr(p: NonNull<TsVectorHeader>) -> Self {
        let q = unsafe {
            NonNull::new(pgrx::pg_sys::pg_detoast_datum(p.cast().as_ptr()).cast()).unwrap()
        };
        TsVectorInput(q, PhantomData, p != q)
    }
    pub fn as_borrowed(&self) -> TsVectorBorrowed<'_> {
        unsafe { TsVectorHeader::as_borrowed(self.0) }
    }
}

impl Drop for TsVectorInput<'_> {
    fn drop(&mut self) {
        if self.2 {
            unsafe {
                pgrx::pg_sys::pfree(self.0.as_ptr().cast());
            }
        }
    }
}

pub struct TsVectorOutput(NonNull<TsVectorHeader>);

impl TsVectorOutput {
    unsafe fn from_ptr(p: NonNull<TsVectorHeader>) -> Self {
        let q = unsafe {
            NonNull::new(pgrx::pg_sys::pg_detoast_datum_copy(p.as_ptr().cast()).cast()).unwrap()
        };
        Self(q)
    }
    pub fn as_borrowed(&self) -> TsVectorBorrowed<'_> {
        unsafe { TsVectorHeader::as_borrowed(self.0) }
    }
    fn into_raw(self) -> *mut TsVectorHeader {
        let result = self.0.as_ptr();
        std::mem::forget(self);
        result
    }
}

impl Drop for TsVectorOutput {
    fn drop(&mut self) {
        unsafe {
            pgrx::pg_sys::pfree(self.0.as_ptr().cast());
        }
    }
}

// FromDatum

impl FromDatum for TsVectorInput<'_> {
    unsafe fn from_polymorphic_datum(datum: Datum, is_null: bool, _typoid: Oid) -> Option<Self> {
        if is_null {
            None
        } else {
            let ptr = NonNull::new(datum.cast_mut_ptr()).unwrap();
            unsafe { Some(Self::from_ptr(ptr)) }
        }
    }
}

impl FromDatum for TsVectorOutput {
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

impl IntoDatum for TsVectorOutput {
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

unsafe impl<'a> pgrx::datum::UnboxDatum for TsVectorInput<'a> {
    type As<'src>
        = TsVectorInput<'src>
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

unsafe impl pgrx::datum::UnboxDatum for TsVectorOutput {
    type As<'src> = TsVectorOutput;
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

unsafe impl SqlTranslatable for TsVectorInput<'_> {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("tsvector")))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("tsvector"))))
    }
}

unsafe impl SqlTranslatable for TsVectorOutput {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("tsvector")))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("tsvector"))))
    }
}

// ArgAbi

unsafe impl<'fcx> pgrx::callconv::ArgAbi<'fcx> for TsVectorInput<'fcx> {
    unsafe fn unbox_arg_unchecked(arg: pgrx::callconv::Arg<'_, 'fcx>) -> Self {
        let index = arg.index();
        unsafe {
            arg.unbox_arg_using_from_datum()
                .unwrap_or_else(|| panic!("argument {index} must not be null"))
        }
    }
}

// BoxAbi

unsafe impl pgrx::callconv::BoxRet for TsVectorOutput {
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
