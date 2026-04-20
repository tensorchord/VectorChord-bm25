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

use crate::datatype::memory_tsvector::TsVectorInput;
use crate::datatype::tsvector::cast_tsvector_to_document;
use crate::index::bm25::am::Reloption;
use crate::index::bm25::types::*;
use crate::index::fetcher::ctid_to_key;
use crate::index::storage::PostgresRelation;
use crate::index::traverse::{HeapTraverser, Traverser};
use std::ffi::CStr;
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum BuildPhaseCode {
    Initializing = 0,
    Build = 1,
}

pub struct BuildPhase(BuildPhaseCode, u16);

impl BuildPhase {
    pub const fn new(code: BuildPhaseCode, k: u16) -> Option<Self> {
        match (code, k) {
            (BuildPhaseCode::Initializing, 0) => Some(BuildPhase(code, k)),
            (BuildPhaseCode::Build, 0) => Some(BuildPhase(code, k)),
            _ => None,
        }
    }
    pub const fn name(self) -> &'static CStr {
        match self {
            BuildPhase(BuildPhaseCode::Initializing, k) => {
                static RAW: [&CStr; 1] = [c"initializing"];
                RAW[k as usize]
            }
            BuildPhase(BuildPhaseCode::Build, k) => {
                static RAW: [&CStr; 1] = [c"initializing index"];
                RAW[k as usize]
            }
        }
    }
    pub const fn from_code(code: BuildPhaseCode) -> Self {
        Self(code, 0)
    }
    pub const fn from_value(value: u32) -> Option<Self> {
        const INITIALIZING: u16 = BuildPhaseCode::Initializing as _;
        const BUILD: u16 = BuildPhaseCode::Build as _;
        let k = value as u16;
        match (value >> 16) as u16 {
            INITIALIZING => Self::new(BuildPhaseCode::Initializing, k),
            BUILD => Self::new(BuildPhaseCode::Build, k),
            _ => None,
        }
    }
    pub const fn into_value(self) -> u32 {
        (self.0 as u32) << 16 | (self.1 as u32)
    }
}

#[pgrx::pg_guard]
pub extern "C-unwind" fn ambuildphasename(x: i64) -> *mut core::ffi::c_char {
    if let Ok(x) = u32::try_from(x.wrapping_sub(1)) {
        if let Some(x) = BuildPhase::from_value(x) {
            x.name().as_ptr().cast_mut()
        } else {
            std::ptr::null_mut()
        }
    } else {
        std::ptr::null_mut()
    }
}

#[derive(Debug, Clone)]
struct PostgresReporter {
    _phantom: PhantomData<*mut ()>,
}

impl PostgresReporter {
    fn phase(&self, phase: BuildPhase) {
        unsafe {
            pgrx::pg_sys::pgstat_progress_update_param(
                pgrx::pg_sys::PROGRESS_CREATEIDX_SUBPHASE as _,
                (phase.into_value() as i64) + 1,
            );
        }
    }
    fn tuples_total(&self, tuples_total: u64) {
        unsafe {
            pgrx::pg_sys::pgstat_progress_update_param(
                pgrx::pg_sys::PROGRESS_CREATEIDX_TUPLES_TOTAL as _,
                tuples_total as _,
            );
        }
    }
    fn tuples_done(&self, tuples_done: u64) {
        unsafe {
            pgrx::pg_sys::pgstat_progress_update_param(
                pgrx::pg_sys::PROGRESS_CREATEIDX_TUPLES_DONE as _,
                tuples_done as _,
            );
        }
    }
}

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn ambuild(
    heap_relation: pgrx::pg_sys::Relation,
    index_relation: pgrx::pg_sys::Relation,
    index_info: *mut pgrx::pg_sys::IndexInfo,
) -> *mut pgrx::pg_sys::IndexBuildResult {
    use validator::Validate;
    let bm25_options = unsafe { options(index_relation) };
    if let Err(errors) = Validate::validate(&bm25_options) {
        pgrx::error!("error while validating options: {}", errors);
    }
    let reporter = PostgresReporter {
        _phantom: PhantomData,
    };
    reporter.tuples_total(unsafe { (*(*index_relation).rd_rel).reltuples as u64 });
    reporter.phase(BuildPhase::from_code(BuildPhaseCode::Build));
    let index = unsafe { PostgresRelation::new(index_relation) };
    let traverser = unsafe {
        HeapTraverser::new(
            heap_relation,
            index_relation,
            index_info,
            std::ptr::null_mut(),
        )
    };
    let mut collector = bm25::Collector::new();
    let mut indtuples = 0_u64;
    traverser.traverse(true, |tuple: &mut dyn crate::index::traverse::Tuple| {
        let ctid = tuple.id();
        let (values, is_nulls) = tuple.build();
        let value = unsafe { (!is_nulls.add(0).read()).then_some(values.add(0).read()) };
        let document = 'block: {
            use pgrx::datum::FromDatum;
            let Some(datum) = value else {
                break 'block None;
            };
            if datum.is_null() {
                break 'block None;
            }
            let vector = unsafe { TsVectorInput::from_datum(datum, false).unwrap() };
            Some(cast_tsvector_to_document(vector.as_borrowed()))
        };
        if let Some(document) = document {
            collector.push(&document, ctid_to_key(ctid));
            indtuples += 1;
            reporter.tuples_done(indtuples);
        }
    });
    let segment = collector.finish();
    bm25::build(bm25_options.index, &index, segment);
    unsafe { pgrx::pgbox::PgBox::<pgrx::pg_sys::IndexBuildResult>::alloc0().into_pg() }
}

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn ambuildempty(_index_relation: pgrx::pg_sys::Relation) {
    pgrx::error!("Unlogged indexes are not supported.");
}

unsafe fn options(index_relation: pgrx::pg_sys::Relation) -> Bm25IndexingOptions {
    let att = unsafe { &mut *(*index_relation).rd_att };
    #[cfg(any(feature = "pg14", feature = "pg15", feature = "pg16", feature = "pg17"))]
    let atts = unsafe { att.attrs.as_slice(att.natts as _) };
    #[cfg(feature = "pg18")]
    let atts = unsafe {
        let ptr = att
            .compact_attrs
            .as_ptr()
            .add(att.natts as _)
            .cast::<pgrx::pg_sys::FormData_pg_attribute>();
        std::slice::from_raw_parts(ptr, att.natts as _)
    };
    if atts.is_empty() {
        pgrx::error!("indexing on no columns is not supported");
    }
    if atts.len() != 1 {
        pgrx::error!("multicolumn index is not supported");
    }
    // get indexing options
    let indexing_options = {
        let reloption = unsafe { (*index_relation).rd_options as *const Reloption };
        let s = unsafe { Reloption::options(reloption, c"") }.to_string_lossy();
        match toml::from_str::<Bm25IndexingOptions>(&s) {
            Ok(p) => p,
            Err(e) => pgrx::error!("failed to parse options: {}", e),
        }
    };
    #[allow(clippy::let_and_return)]
    indexing_options
}
