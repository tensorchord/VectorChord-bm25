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
use crate::index::temp::tempdir;
use crate::index::traverse::{HeapTraverser, Traverser};
use std::ffi::{CStr, OsStr};
use std::marker::PhantomData;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum BuildPhaseCode {
    Initializing = 0,
    Scanning = 1,
    Writing = 2,
}

pub struct BuildPhase(BuildPhaseCode, u16);

impl BuildPhase {
    pub const fn new(code: BuildPhaseCode, k: u16) -> Option<Self> {
        match (code, k) {
            (BuildPhaseCode::Initializing, 0) => Some(BuildPhase(code, k)),
            (BuildPhaseCode::Scanning, 0) => Some(BuildPhase(code, k)),
            (BuildPhaseCode::Writing, 0) => Some(BuildPhase(code, k)),
            _ => None,
        }
    }
    pub const fn name(self) -> &'static CStr {
        match self {
            BuildPhase(BuildPhaseCode::Initializing, k) => {
                static RAW: [&CStr; 1] = [c"initializing"];
                RAW[k as usize]
            }
            BuildPhase(BuildPhaseCode::Scanning, k) => {
                static RAW: [&CStr; 1] = [c"scanning table"];
                RAW[k as usize]
            }
            BuildPhase(BuildPhaseCode::Writing, k) => {
                static RAW: [&CStr; 1] = [c"writing index structures"];
                RAW[k as usize]
            }
        }
    }
    pub const fn from_code(code: BuildPhaseCode) -> Self {
        Self(code, 0)
    }
    pub const fn from_value(value: u32) -> Option<Self> {
        const INITIALIZING: u16 = BuildPhaseCode::Initializing as _;
        const SCANNING: u16 = BuildPhaseCode::Scanning as _;
        const WRITING: u16 = BuildPhaseCode::Writing as _;
        let k = value as u16;
        match (value >> 16) as u16 {
            INITIALIZING => Self::new(BuildPhaseCode::Initializing, k),
            SCANNING => Self::new(BuildPhaseCode::Scanning, k),
            WRITING => Self::new(BuildPhaseCode::Writing, k),
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
    reporter.phase(BuildPhase::from_code(BuildPhaseCode::Scanning));
    let seed = bm25::seed::random();
    let tempdir = tempdir();
    let total = if let Some(leader) = unsafe {
        Bm25Leader::enter(
            c"bm25_parallel_build_main",
            heap_relation,
            index_relation,
            (*index_info).ii_Concurrent,
            seed,
            tempdir.path(),
        )
    } {
        unsafe {
            leader.wait();
            parallel_build(
                index_relation,
                heap_relation,
                index_info,
                leader.tablescandesc,
                leader.bm25shared,
                leader.path,
                |indtuples| {
                    reporter.tuples_done(indtuples);
                },
                || {
                    #[allow(clippy::needless_late_init)]
                    let order;
                    // enter the barrier
                    let shared = leader.bm25shared;
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    (*shared).nparticipants = leader.nparticipants as u32;
                    order = (*shared).barrier_enter_0 as u32;
                    (*shared).barrier_enter_0 += 1;
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableBroadcast(
                        &raw mut (*shared).condvar_barrier_enter_0,
                    );
                    // leave the barrier
                    let total = leader.nparticipants;
                    loop {
                        pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                        if (*shared).barrier_enter_0 == total {
                            pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                            break;
                        }
                        pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                        pgrx::pg_sys::ConditionVariableSleep(
                            &raw mut (*shared).condvar_barrier_enter_0,
                            pgrx::pg_sys::WaitEventIPC::WAIT_EVENT_PARALLEL_CREATE_INDEX_SCAN as _,
                        );
                    }
                    pgrx::pg_sys::ConditionVariableCancelSleep();
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    (*shared).barrier_leave_0 = true;
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableBroadcast(
                        &raw mut (*shared).condvar_barrier_leave_0,
                    );
                    order
                },
                |indtuples| {
                    reporter.tuples_done(indtuples);
                    reporter.tuples_total(indtuples);
                    // enter the barrier
                    let shared = leader.bm25shared;
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    (*shared).barrier_enter_1 += 1;
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableBroadcast(
                        &raw mut (*shared).condvar_barrier_enter_1,
                    );
                    // leave the barrier
                    let total = leader.nparticipants;
                    loop {
                        pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                        if (*shared).barrier_enter_1 == total {
                            pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                            break;
                        }
                        pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                        pgrx::pg_sys::ConditionVariableSleep(
                            &raw mut (*shared).condvar_barrier_enter_1,
                            pgrx::pg_sys::WaitEventIPC::WAIT_EVENT_PARALLEL_CREATE_INDEX_SCAN as _,
                        );
                    }
                    pgrx::pg_sys::ConditionVariableCancelSleep();
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    (*shared).barrier_leave_1 = true;
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableBroadcast(
                        &raw mut (*shared).condvar_barrier_leave_1,
                    );
                },
                || {
                    // enter the barrier
                    let shared = leader.bm25shared;
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    (*shared).barrier_enter_2 += 1;
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableBroadcast(
                        &raw mut (*shared).condvar_barrier_enter_2,
                    );
                    // leave the barrier
                    let total = leader.nparticipants;
                    loop {
                        pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                        if (*shared).barrier_enter_2 == total {
                            pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                            break;
                        }
                        pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                        pgrx::pg_sys::ConditionVariableSleep(
                            &raw mut (*shared).condvar_barrier_enter_2,
                            pgrx::pg_sys::WaitEventIPC::WAIT_EVENT_PARALLEL_CREATE_INDEX_SCAN as _,
                        );
                    }
                    pgrx::pg_sys::ConditionVariableCancelSleep();
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    (*shared).barrier_leave_2 = true;
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableBroadcast(
                        &raw mut (*shared).condvar_barrier_leave_2,
                    );
                },
            );
            leader.nparticipants as u32
        }
    } else {
        unsafe {
            sequential_build(
                index_relation,
                heap_relation,
                index_info,
                seed,
                tempdir.path(),
                |indtuples| {
                    reporter.tuples_done(indtuples);
                },
                || (),
                |indtuples| {
                    reporter.tuples_done(indtuples);
                    reporter.tuples_total(indtuples);
                },
                || (),
            );
            1
        }
    };
    reporter.phase(BuildPhase::from_code(BuildPhaseCode::Writing));
    let index = unsafe { PostgresRelation::new(index_relation) };
    let segment = bm25::io::readers(tempdir.path(), total);
    bm25::build(bm25_options.index, &index, seed, segment);
    unsafe { pgrx::pgbox::PgBox::<pgrx::pg_sys::IndexBuildResult>::alloc0().into_pg() }
}

#[repr(C)]
struct Bm25Shared {
    /* immutable state */
    heaprelid: pgrx::pg_sys::Oid,
    indexrelid: pgrx::pg_sys::Oid,
    isconcurrent: bool,
    seed: [u8; 32],

    /* locking */
    mutex: pgrx::pg_sys::slock_t,
    condvar_barrier_enter_0: pgrx::pg_sys::ConditionVariable,
    condvar_barrier_leave_0: pgrx::pg_sys::ConditionVariable,
    condvar_barrier_enter_1: pgrx::pg_sys::ConditionVariable,
    condvar_barrier_leave_1: pgrx::pg_sys::ConditionVariable,
    condvar_barrier_enter_2: pgrx::pg_sys::ConditionVariable,
    condvar_barrier_leave_2: pgrx::pg_sys::ConditionVariable,

    /* mutable state */
    barrier_enter_0: i32,
    nparticipants: u32,
    indtuples: u64,
    barrier_leave_0: bool,
    barrier_enter_1: i32,
    barrier_leave_1: bool,
    barrier_enter_2: i32,
    barrier_leave_2: bool,
}

fn is_mvcc_snapshot(snapshot: *mut pgrx::pg_sys::SnapshotData) -> bool {
    matches!(
        unsafe { (*snapshot).snapshot_type },
        pgrx::pg_sys::SnapshotType::SNAPSHOT_MVCC
            | pgrx::pg_sys::SnapshotType::SNAPSHOT_HISTORIC_MVCC
    )
}

struct Bm25Leader {
    pcxt: *mut pgrx::pg_sys::ParallelContext,
    nparticipants: i32,
    snapshot: pgrx::pg_sys::Snapshot,
    bm25shared: *mut Bm25Shared,
    tablescandesc: *mut pgrx::pg_sys::ParallelTableScanDescData,
    path: *mut u8,
}

impl Bm25Leader {
    pub unsafe fn enter(
        main: &'static CStr,
        heap_relation: pgrx::pg_sys::Relation,
        index_relation: pgrx::pg_sys::Relation,
        isconcurrent: bool,
        seed: [u8; 32],
        path: &Path,
    ) -> Option<Self> {
        unsafe fn compute_parallel_workers(
            heap_relation: pgrx::pg_sys::Relation,
            index_relation: pgrx::pg_sys::Relation,
        ) -> i32 {
            unsafe {
                if pgrx::pg_sys::plan_create_index_workers(
                    (*heap_relation).rd_id,
                    (*index_relation).rd_id,
                ) == 0
                {
                    return 0;
                }
                if !(*heap_relation).rd_options.is_null() {
                    let std_options = (*heap_relation)
                        .rd_options
                        .cast::<pgrx::pg_sys::StdRdOptions>();
                    std::cmp::min(
                        (*std_options).parallel_workers,
                        pgrx::pg_sys::max_parallel_maintenance_workers,
                    )
                } else {
                    pgrx::pg_sys::max_parallel_maintenance_workers
                }
            }
        }

        let request = unsafe { compute_parallel_workers(heap_relation, index_relation) };
        if request <= 0 {
            return None;
        }

        unsafe {
            pgrx::pg_sys::EnterParallelMode();
        }
        let pcxt = unsafe {
            pgrx::pg_sys::CreateParallelContext(c"vchord_bm25".as_ptr(), main.as_ptr(), request)
        };

        let snapshot = if isconcurrent {
            unsafe { pgrx::pg_sys::RegisterSnapshot(pgrx::pg_sys::GetTransactionSnapshot()) }
        } else {
            &raw mut pgrx::pg_sys::SnapshotAnyData
        };

        fn estimate_chunk(e: &mut pgrx::pg_sys::shm_toc_estimator, x: usize) {
            e.space_for_chunks += x.next_multiple_of(pgrx::pg_sys::ALIGNOF_BUFFER as _);
        }
        fn estimate_keys(e: &mut pgrx::pg_sys::shm_toc_estimator, x: usize) {
            e.number_of_keys += x;
        }
        let est_tablescandesc =
            unsafe { pgrx::pg_sys::table_parallelscan_estimate(heap_relation, snapshot) };
        unsafe {
            estimate_chunk(&mut (*pcxt).estimator, size_of::<Bm25Shared>());
            estimate_keys(&mut (*pcxt).estimator, 1);
            estimate_chunk(&mut (*pcxt).estimator, est_tablescandesc);
            estimate_keys(&mut (*pcxt).estimator, 1);
            let encoded_bytes = path.as_os_str().as_encoded_bytes();
            estimate_chunk(&mut (*pcxt).estimator, 8 + encoded_bytes.len());
            estimate_keys(&mut (*pcxt).estimator, 1);
        }

        unsafe {
            pgrx::pg_sys::InitializeParallelDSM(pcxt);
            if (*pcxt).seg.is_null() {
                if is_mvcc_snapshot(snapshot) {
                    pgrx::pg_sys::UnregisterSnapshot(snapshot);
                }
                pgrx::pg_sys::DestroyParallelContext(pcxt);
                pgrx::pg_sys::ExitParallelMode();
                return None;
            }
        }

        let bm25shared = unsafe {
            let bm25shared = pgrx::pg_sys::shm_toc_allocate((*pcxt).toc, size_of::<Bm25Shared>())
                .cast::<Bm25Shared>();
            bm25shared.write(Bm25Shared {
                heaprelid: (*heap_relation).rd_id,
                indexrelid: (*index_relation).rd_id,
                isconcurrent,
                seed,
                mutex: std::mem::zeroed(),
                condvar_barrier_enter_0: std::mem::zeroed(),
                condvar_barrier_leave_0: std::mem::zeroed(),
                condvar_barrier_enter_1: std::mem::zeroed(),
                condvar_barrier_leave_1: std::mem::zeroed(),
                condvar_barrier_enter_2: std::mem::zeroed(),
                condvar_barrier_leave_2: std::mem::zeroed(),
                barrier_enter_0: 0,
                nparticipants: 0,
                indtuples: 0,
                barrier_leave_0: false,
                barrier_enter_1: 0,
                barrier_leave_1: false,
                barrier_enter_2: 0,
                barrier_leave_2: false,
            });
            pgrx::pg_sys::SpinLockInit(&raw mut (*bm25shared).mutex);
            pgrx::pg_sys::ConditionVariableInit(&raw mut (*bm25shared).condvar_barrier_enter_0);
            pgrx::pg_sys::ConditionVariableInit(&raw mut (*bm25shared).condvar_barrier_leave_0);
            pgrx::pg_sys::ConditionVariableInit(&raw mut (*bm25shared).condvar_barrier_enter_1);
            pgrx::pg_sys::ConditionVariableInit(&raw mut (*bm25shared).condvar_barrier_leave_1);
            pgrx::pg_sys::ConditionVariableInit(&raw mut (*bm25shared).condvar_barrier_enter_2);
            pgrx::pg_sys::ConditionVariableInit(&raw mut (*bm25shared).condvar_barrier_leave_2);
            bm25shared
        };

        let tablescandesc = unsafe {
            let tablescandesc = pgrx::pg_sys::shm_toc_allocate((*pcxt).toc, est_tablescandesc)
                .cast::<pgrx::pg_sys::ParallelTableScanDescData>();
            pgrx::pg_sys::table_parallelscan_initialize(heap_relation, tablescandesc, snapshot);
            tablescandesc
        };

        let path = unsafe {
            let encoded_bytes = path.as_os_str().as_encoded_bytes();
            let x =
                pgrx::pg_sys::shm_toc_allocate((*pcxt).toc, 8 + encoded_bytes.len()).cast::<u8>();
            (x as *mut u64).write_unaligned(encoded_bytes.len() as _);
            std::ptr::copy(encoded_bytes.as_ptr(), x.add(8), encoded_bytes.len());
            x
        };

        unsafe {
            pgrx::pg_sys::shm_toc_insert((*pcxt).toc, 0xA000000000000001, bm25shared.cast());
            pgrx::pg_sys::shm_toc_insert((*pcxt).toc, 0xA000000000000002, tablescandesc.cast());
            pgrx::pg_sys::shm_toc_insert((*pcxt).toc, 0xA000000000000003, path.cast());
        }

        unsafe {
            pgrx::pg_sys::LaunchParallelWorkers(pcxt);
        }

        let nworkers_launched = unsafe { (*pcxt).nworkers_launched };

        unsafe {
            if nworkers_launched == 0 {
                pgrx::pg_sys::WaitForParallelWorkersToFinish(pcxt);
                if is_mvcc_snapshot(snapshot) {
                    pgrx::pg_sys::UnregisterSnapshot(snapshot);
                }
                pgrx::pg_sys::DestroyParallelContext(pcxt);
                pgrx::pg_sys::ExitParallelMode();
                return None;
            }
        }

        Some(Self {
            pcxt,
            nparticipants: nworkers_launched + 1,
            snapshot,
            tablescandesc,
            bm25shared,
            path,
        })
    }

    pub fn wait(&self) {
        unsafe {
            pgrx::pg_sys::WaitForParallelWorkersToAttach(self.pcxt);
        }
    }
}

impl Drop for Bm25Leader {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            unsafe {
                pgrx::pg_sys::WaitForParallelWorkersToFinish(self.pcxt);
                if is_mvcc_snapshot(self.snapshot) {
                    pgrx::pg_sys::UnregisterSnapshot(self.snapshot);
                }
                pgrx::pg_sys::DestroyParallelContext(self.pcxt);
                pgrx::pg_sys::ExitParallelMode();
            }
        }
    }
}

#[pgrx::pg_guard]
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn bm25_parallel_build_main(
    _seg: *mut pgrx::pg_sys::dsm_segment,
    toc: *mut pgrx::pg_sys::shm_toc,
) {
    let _ = rand::rng().reseed();
    let bm25shared = unsafe {
        pgrx::pg_sys::shm_toc_lookup(toc, 0xA000000000000001, false).cast::<Bm25Shared>()
    };
    let tablescandesc = unsafe {
        pgrx::pg_sys::shm_toc_lookup(toc, 0xA000000000000002, false)
            .cast::<pgrx::pg_sys::ParallelTableScanDescData>()
    };
    let path = unsafe {
        pgrx::pg_sys::shm_toc_lookup(toc, 0xA000000000000003, false)
            .cast::<u8>()
            .cast_const()
    };
    let heap_lockmode;
    let index_lockmode;
    if unsafe { !(*bm25shared).isconcurrent } {
        heap_lockmode = pgrx::pg_sys::ShareLock as pgrx::pg_sys::LOCKMODE;
        index_lockmode = pgrx::pg_sys::AccessExclusiveLock as pgrx::pg_sys::LOCKMODE;
    } else {
        heap_lockmode = pgrx::pg_sys::ShareUpdateExclusiveLock as pgrx::pg_sys::LOCKMODE;
        index_lockmode = pgrx::pg_sys::RowExclusiveLock as pgrx::pg_sys::LOCKMODE;
    }
    let heap = unsafe { pgrx::pg_sys::table_open((*bm25shared).heaprelid, heap_lockmode) };
    let index = unsafe { pgrx::pg_sys::index_open((*bm25shared).indexrelid, index_lockmode) };
    let index_info = unsafe { pgrx::pg_sys::BuildIndexInfo(index) };
    unsafe {
        (*index_info).ii_Concurrent = (*bm25shared).isconcurrent;
    }

    unsafe {
        parallel_build(
            index,
            heap,
            index_info,
            tablescandesc,
            bm25shared,
            path,
            |_| (),
            || {
                #[allow(clippy::needless_late_init)]
                let order;
                // enter the barrier
                let shared = bm25shared;
                pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                order = (*shared).barrier_enter_0 as u32;
                (*shared).barrier_enter_0 += 1;
                pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                pgrx::pg_sys::ConditionVariableBroadcast(
                    &raw mut (*shared).condvar_barrier_enter_0,
                );
                // leave the barrier
                loop {
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    if (*shared).barrier_leave_0 {
                        pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                        break;
                    }
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableSleep(
                        &raw mut (*shared).condvar_barrier_leave_0,
                        pgrx::pg_sys::WaitEventIPC::WAIT_EVENT_PARALLEL_CREATE_INDEX_SCAN as _,
                    );
                }
                pgrx::pg_sys::ConditionVariableCancelSleep();
                order
            },
            |_| {
                // enter the barrier
                let shared = bm25shared;
                pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                (*shared).barrier_enter_1 += 1;
                pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                pgrx::pg_sys::ConditionVariableBroadcast(
                    &raw mut (*shared).condvar_barrier_enter_1,
                );
                // leave the barrier
                loop {
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    if (*shared).barrier_leave_1 {
                        pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                        break;
                    }
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableSleep(
                        &raw mut (*shared).condvar_barrier_leave_1,
                        pgrx::pg_sys::WaitEventIPC::WAIT_EVENT_PARALLEL_CREATE_INDEX_SCAN as _,
                    );
                }
                pgrx::pg_sys::ConditionVariableCancelSleep();
            },
            || {
                // enter the barrier
                let shared = bm25shared;
                pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                (*shared).barrier_enter_2 += 1;
                pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                pgrx::pg_sys::ConditionVariableBroadcast(
                    &raw mut (*shared).condvar_barrier_enter_2,
                );
                // leave the barrier
                loop {
                    pgrx::pg_sys::SpinLockAcquire(&raw mut (*shared).mutex);
                    if (*shared).barrier_leave_2 {
                        pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                        break;
                    }
                    pgrx::pg_sys::SpinLockRelease(&raw mut (*shared).mutex);
                    pgrx::pg_sys::ConditionVariableSleep(
                        &raw mut (*shared).condvar_barrier_leave_2,
                        pgrx::pg_sys::WaitEventIPC::WAIT_EVENT_PARALLEL_CREATE_INDEX_SCAN as _,
                    );
                }
                pgrx::pg_sys::ConditionVariableCancelSleep();
            },
        );
    }

    unsafe {
        pgrx::pg_sys::index_close(index, index_lockmode);
        pgrx::pg_sys::table_close(heap, heap_lockmode);
    }
}

unsafe fn parallel_build(
    index_relation: pgrx::pg_sys::Relation,
    heap_relation: pgrx::pg_sys::Relation,
    index_info: *mut pgrx::pg_sys::IndexInfo,
    tablescandesc: *mut pgrx::pg_sys::ParallelTableScanDescData,
    bm25shared: *mut Bm25Shared,
    path: *const u8,
    mut callback: impl FnMut(u64),
    sync_0: impl FnOnce() -> u32,
    sync_1: impl FnOnce(u64),
    sync_2: impl FnOnce(),
) {
    let seed = unsafe { (*bm25shared).seed };
    let path: &Path = unsafe {
        let len = (path as *const u64).read_unaligned();
        let bytes = std::slice::from_raw_parts(path.add(8), len as _);
        OsStr::from_encoded_bytes_unchecked(bytes).as_ref()
    };

    let order = sync_0();
    let mut records_writer = bm25::io::records_writer(path, order);
    let mut mappings_writer = bm25::io::mappings_writer(path, order);

    let scan = unsafe { pgrx::pg_sys::table_beginscan_parallel(heap_relation, tablescandesc) };
    let traverser = unsafe { HeapTraverser::new(heap_relation, index_relation, index_info, scan) };

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
            Some(cast_tsvector_to_document(&seed, vector.as_borrowed()))
        };
        if let Some(document) = document {
            bm25::io::write(
                &mut records_writer,
                &mut mappings_writer,
                &document,
                ctid_to_key(ctid),
            );
        }
        unsafe {
            let indtuples;
            {
                pgrx::pg_sys::SpinLockAcquire(&raw mut (*bm25shared).mutex);
                (*bm25shared).indtuples += 1;
                indtuples = (*bm25shared).indtuples;
                pgrx::pg_sys::SpinLockRelease(&raw mut (*bm25shared).mutex);
            }
            callback(indtuples);
        }
    });

    sync_1(unsafe {
        // It may not be accurate, but it is acceptable.
        let indtuples;
        {
            pgrx::pg_sys::SpinLockAcquire(&raw mut (*bm25shared).mutex);
            indtuples = (*bm25shared).indtuples;
            pgrx::pg_sys::SpinLockRelease(&raw mut (*bm25shared).mutex);
        }
        indtuples
    });

    records_writer.flush();
    mappings_writer.flush();
    drop(records_writer);
    drop(mappings_writer);
    bm25::io::locally_merge(path, order);

    sync_2();
}

unsafe fn sequential_build(
    index_relation: pgrx::pg_sys::Relation,
    heap_relation: pgrx::pg_sys::Relation,
    index_info: *mut pgrx::pg_sys::IndexInfo,
    seed: [u8; 32],
    path: &Path,
    mut callback: impl FnMut(u64),
    sync_0: impl FnOnce(),
    sync_1: impl FnOnce(u64),
    sync_2: impl FnOnce(),
) {
    sync_0();
    let mut records_writer = bm25::io::records_writer(path, 0);
    let mut mappings_writer = bm25::io::mappings_writer(path, 0);
    let traverser = unsafe {
        HeapTraverser::new(
            heap_relation,
            index_relation,
            index_info,
            std::ptr::null_mut(),
        )
    };
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
            Some(cast_tsvector_to_document(&seed, vector.as_borrowed()))
        };
        if let Some(document) = document {
            bm25::io::write(
                &mut records_writer,
                &mut mappings_writer,
                &document,
                ctid_to_key(ctid),
            );
        }
        {
            indtuples += 1;
            callback(indtuples);
        }
    });

    sync_1(indtuples);

    records_writer.flush();
    mappings_writer.flush();
    drop(records_writer);
    drop(mappings_writer);
    bm25::io::locally_merge(path, 0);

    sync_2();
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
