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

use pgrx::datum::IntoDatum;
use pgrx::pg_sys::{Datum, Oid, bytea};
use pgrx::pgrx_sql_entity_graph::metadata::*;

#[repr(transparent)]
pub struct Bytea(*mut bytea);

impl Bytea {
    pub fn new(x: *mut bytea) -> Self {
        Self(x)
    }
}

impl IntoDatum for Bytea {
    fn into_datum(self) -> Option<Datum> {
        if !self.0.is_null() {
            Some(Datum::from(self.0))
        } else {
            None
        }
    }

    fn type_oid() -> Oid {
        pgrx::pg_sys::BYTEAOID
    }
}

unsafe impl SqlTranslatable for Bytea {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("bytea")))
    }

    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("bytea"))))
    }
}

unsafe impl pgrx::callconv::BoxRet for Bytea {
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        unsafe { fcinfo.return_raw_datum(Datum::from(self.0 as *mut ())) }
    }
}
