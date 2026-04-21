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

use crate::datatype::memory_tsvector::{TsVectorInput, TsVectorOutput};
use crate::datatype::tsvector::{cast_tsvector_to_document, cast_tsvector_to_query};
use crate::index::storage::PostgresRelation;
use pgrx::pg_sys::Oid;
use pgrx_catalog::{PgAm, PgClass, PgClassRelkind};
use std::num::NonZero;

#[pgrx::pg_extern(stable, strict, parallel_safe)]
pub fn _bm25_evaluate(lhs: TsVectorInput, rhs: pgrx::composite_type!("bm25query")) -> f64 {
    let vector: TsVectorOutput = match rhs.get_by_index(NonZero::new(1).unwrap()) {
        Ok(Some(s)) => s,
        Ok(None) => pgrx::error!("bm25query contains a null vector"),
        Err(_) => unreachable!(),
    };
    let index: Oid = match rhs.get_by_index(NonZero::new(2).unwrap()) {
        Ok(Some(s)) => s,
        Ok(None) => pgrx::error!("bm25query contains a null index"),
        Err(_) => unreachable!(),
    };
    let pg_am = PgAm::search_amname(c"bm25").unwrap();
    let Some(pg_am) = pg_am.get() else {
        pgrx::error!("vchord_bm25 is not installed");
    };
    let pg_class = PgClass::search_reloid(index).unwrap();
    let Some(pg_class) = pg_class.get() else {
        pgrx::error!("the relation does not exist");
    };
    if pg_class.relkind() != PgClassRelkind::Index {
        pgrx::error!("the relation {:?} is not an index", pg_class.relname());
    }
    if pg_class.relam() != pg_am.oid() {
        pgrx::error!("the index {:?} is not a bm25 index", pg_class.relname());
    }
    let relation = Index::open(index, pgrx::pg_sys::AccessShareLock as _);
    let index = unsafe { PostgresRelation::new(relation.raw()) };
    let seed = bm25::seed::seed(&index);
    let lhs = cast_tsvector_to_document(&seed, lhs.as_borrowed());
    let rhs = cast_tsvector_to_query(&seed, vector.as_borrowed());
    let score = bm25::evaluate(&index, &lhs, &rhs);
    -score.to_f64()
}

struct Index {
    raw: *mut pgrx::pg_sys::RelationData,
    lockmode: pgrx::pg_sys::LOCKMODE,
}

impl Index {
    fn open(indexrelid: Oid, lockmode: pgrx::pg_sys::LOCKMASK) -> Self {
        Self {
            raw: unsafe { pgrx::pg_sys::index_open(indexrelid, lockmode) },
            lockmode,
        }
    }
    fn raw(&self) -> *mut pgrx::pg_sys::RelationData {
        self.raw
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        unsafe {
            pgrx::pg_sys::index_close(self.raw, self.lockmode);
        }
    }
}
