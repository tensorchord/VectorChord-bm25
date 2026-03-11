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

use pgrx::pg_sys::{Datum, ItemPointerData};

pub trait Tuple {
    fn id(&mut self) -> ItemPointerData;
    fn build(&mut self) -> (*const Datum, *const bool);
}

pub trait Traverse {
    fn next<T: Tuple>(&mut self, tuple: T);
}

impl<F: FnMut(&mut dyn Tuple)> Traverse for F {
    fn next<T: Tuple>(&mut self, mut tuple: T) {
        self(&mut tuple);
    }
}

pub trait Traverser {
    fn traverse<T: Traverse>(&self, progress: bool, traverse: T);
}

#[derive(Debug, Clone)]
pub struct HeapTraverser {
    heap_relation: pgrx::pg_sys::Relation,
    index_relation: pgrx::pg_sys::Relation,
    index_info: *mut pgrx::pg_sys::IndexInfo,
    scan: *mut pgrx::pg_sys::TableScanDescData,
}

impl HeapTraverser {
    pub unsafe fn new(
        heap_relation: pgrx::pg_sys::Relation,
        index_relation: pgrx::pg_sys::Relation,
        index_info: *mut pgrx::pg_sys::IndexInfo,
        scan: *mut pgrx::pg_sys::TableScanDescData,
    ) -> Self {
        Self {
            heap_relation,
            index_relation,
            index_info,
            scan,
        }
    }
}

impl Drop for HeapTraverser {
    fn drop(&mut self) {}
}

impl Traverser for HeapTraverser {
    fn traverse<T: Traverse>(&self, progress: bool, mut traverse: T) {
        unsafe {
            use pgrx::pg_sys::ffi::pg_guard_ffi_boundary;
            let table_am = (*self.heap_relation).rd_tableam;
            if table_am.is_null() {
                panic!("unknown heap access method");
            }
            let index_build_range_scan = (*table_am)
                .index_build_range_scan
                .expect("unsupported heap access method");
            #[allow(ffi_unwind_calls, reason = "protected by pg_guard_ffi_boundary")]
            pg_guard_ffi_boundary(|| {
                index_build_range_scan(
                    self.heap_relation,
                    self.index_relation,
                    self.index_info,
                    true,
                    false,
                    progress,
                    0,
                    pgrx::pg_sys::InvalidBlockNumber,
                    Some(callback::<T>),
                    (&raw mut traverse).cast(),
                    self.scan,
                )
            });
        }
    }
}

struct HeapTuple {
    id: ItemPointerData,
    values: *const Datum,
    is_nulls: *const bool,
}

impl Tuple for HeapTuple {
    fn id(&mut self) -> ItemPointerData {
        self.id
    }

    fn build(&mut self) -> (*const Datum, *const bool) {
        (self.values, self.is_nulls)
    }
}

#[pgrx::pg_guard]
unsafe extern "C-unwind" fn callback<T: Traverse>(
    _index_relation: pgrx::pg_sys::Relation,
    ctid: pgrx::pg_sys::ItemPointer,
    values: *mut Datum,
    is_null: *mut bool,
    _tuple_is_alive: bool,
    state: *mut core::ffi::c_void,
) {
    let state = unsafe { &mut *state.cast::<T>() };

    state.next(HeapTuple {
        id: unsafe { *ctid },
        values: values.cast_const(),
        is_nulls: is_null.cast_const(),
    });
}
