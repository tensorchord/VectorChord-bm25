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

use pgrx::guc::{GucContext, GucFlags, GucRegistry, GucSetting};
use std::ffi::CStr;

static BM25_ENABLE_SCAN: GucSetting<bool> = GucSetting::<bool>::new(true);

static BM25_LIMIT: GucSetting<i32> = GucSetting::<i32>::new(0);

static mut BM25_LIMIT_CONFIG: *mut pgrx::pg_sys::config_generic = core::ptr::null_mut();

static BM25_PREFILTER: GucSetting<bool> = GucSetting::<bool>::new(false);

pub fn init() {
    GucRegistry::define_bool_guc(
        c"bm25.enable_scan",
        c"`enable_scan` argument of bm25",
        c"`enable_scan` argument of bm25",
        &BM25_ENABLE_SCAN,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"bm25.limit",
        c"`limit` argument of bm25",
        c"`limit` argument of bm25",
        &BM25_LIMIT,
        0,
        65535,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"bm25.prefilter",
        c"`prefilter` argument of bm25",
        c"`prefilter` argument of bm25",
        &BM25_PREFILTER,
        GucContext::Userset,
        GucFlags::default(),
    );
    unsafe {
        #[cfg(feature = "pg14")]
        pgrx::pg_sys::EmitWarningsOnPlaceholders(c"bm25".as_ptr());
        #[cfg(any(feature = "pg15", feature = "pg16", feature = "pg17", feature = "pg18"))]
        pgrx::pg_sys::MarkGUCPrefixReserved(c"bm25".as_ptr());
    }
    assert!(crate::is_main());
    let targets = vec![(c"bm25.limit", &raw mut BM25_LIMIT_CONFIG)];
    #[cfg(any(feature = "pg14", feature = "pg15"))]
    unsafe {
        let len = pgrx::pg_sys::GetNumConfigOptions() as usize;
        let arr = pgrx::pg_sys::get_guc_variables();
        let mut sources = (0..len).map(|i| arr.add(i).read());
        debug_assert!(targets.is_sorted_by(|(a, _), (b, _)| guc_name_compare(a, b).is_le()));
        for (name, ptr) in targets {
            *ptr = loop {
                if let Some(source) = sources.next() {
                    if !(*source).name.is_null() && CStr::from_ptr((*source).name) == name {
                        break source;
                    } else {
                        continue;
                    }
                } else {
                    pgrx::error!("failed to find GUC {name:?}");
                }
            };
            assert!(check(*ptr, name), "failed to find GUC {name:?}");
        }
    }
    #[cfg(any(feature = "pg16", feature = "pg17", feature = "pg18"))]
    unsafe {
        use pgrx::pg_sys::PGERROR;
        for (name, ptr) in targets {
            *ptr = pgrx::pg_sys::find_option(name.as_ptr(), false, false, PGERROR as _);
            assert!(check(*ptr, name), "failed to find GUC {name:?}");
        }
    }
}

unsafe fn check(p: *mut pgrx::pg_sys::config_generic, name: &CStr) -> bool {
    if p.is_null() {
        return false;
    }
    if unsafe { (*p).flags } & pgrx::pg_sys::GUC_CUSTOM_PLACEHOLDER as core::ffi::c_int != 0 {
        return false;
    }
    if unsafe { (*p).name }.is_null() {
        return false;
    }
    if unsafe { CStr::from_ptr((*p).name) != name } {
        return false;
    }
    true
}

pub fn bm25_enable_scan() -> bool {
    BM25_ENABLE_SCAN.get()
}

pub fn bm25_limit(index: pgrx::pg_sys::Relation) -> u32 {
    fn parse(x: i32) -> u32 {
        x as u32
    }
    assert!(crate::is_main());
    const DEFAULT: i32 = 0;
    if unsafe { (*BM25_LIMIT_CONFIG).source } != pgrx::pg_sys::GucSource::PGC_S_DEFAULT {
        let value = BM25_LIMIT.get();
        parse(value)
    } else {
        use crate::index::bm25::am::Reloption;
        let value = unsafe { Reloption::limit((*index).rd_options as _, DEFAULT) };
        parse(value)
    }
}

pub fn bm25_prefilter() -> bool {
    BM25_PREFILTER.get()
}

#[allow(dead_code)]
fn guc_name_compare(a: &CStr, b: &CStr) -> std::cmp::Ordering {
    let (a, b) = (a.to_bytes_with_nul(), b.to_bytes_with_nul());
    let mut i = 0;
    while a[i] != 0 && b[i] != 0 {
        let a = a[i].to_ascii_lowercase();
        let b = b[i].to_ascii_lowercase();
        if a != b {
            return Ord::cmp(&a, &b);
        }
        i += 1;
    }
    if b[i] != 0 {
        std::cmp::Ordering::Less
    } else if a[i] != 0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}
