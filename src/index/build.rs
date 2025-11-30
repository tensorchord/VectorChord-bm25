use pgrx::{itemptr::item_pointer_to_u64, FromDatum, PgMemoryContexts};

use crate::{
    datatype::Bm25VectorInput,
    page::{
        page_alloc, page_alloc_init_forknum, page_write, PageFlags, VirtualPageWriter,
        METAPAGE_BLKNO,
    },
    segment::{builder::IndexBuilder, meta::MetaPageData},
};

#[pgrx::pg_guard]
pub extern "C-unwind" fn ambuildempty(index: pgrx::pg_sys::Relation) {
    let mut meta_page = page_alloc_init_forknum(index, PageFlags::META);
    assert_eq!(meta_page.blkno(), METAPAGE_BLKNO);

    let meta: &mut MetaPageData = meta_page.init_mut();
    meta.field_norm_blkno = VirtualPageWriter::init_fork(index, PageFlags::FIELD_NORM);
    meta.term_stat_blkno = VirtualPageWriter::init_fork(index, PageFlags::TERM_STATISTIC);
    meta.payload_blkno = VirtualPageWriter::init_fork(index, PageFlags::PAYLOAD);
    meta.delete_bitmap_blkno = VirtualPageWriter::init_fork(index, PageFlags::DELETE);
}

struct BuildState {
    heap_tuples: usize,
    index_tuples: usize,
    index: pgrx::pg_sys::Relation,
    builder: IndexBuilder,
    memctx: PgMemoryContexts,
}

#[pgrx::pg_guard]
pub extern "C-unwind" fn ambuild(
    heap: pgrx::pg_sys::Relation,
    index: pgrx::pg_sys::Relation,
    index_info: *mut pgrx::pg_sys::IndexInfo,
) -> *mut pgrx::pg_sys::IndexBuildResult {
    {
        let metapage = page_alloc(index, PageFlags::META, true);
        assert_eq!(metapage.blkno(), METAPAGE_BLKNO);
    }

    let mut state = BuildState {
        heap_tuples: 0,
        index_tuples: 0,
        index,
        builder: IndexBuilder::new(),
        memctx: PgMemoryContexts::new("vchord_bm25_index_build"),
    };

    unsafe {
        pgrx::pg_sys::IndexBuildHeapScan(heap, index, index_info, Some(build_callback), &mut state);
    }
    state.builder.finalize_insert();
    write_down(&state);

    let mut result = unsafe { pgrx::PgBox::<pgrx::pg_sys::IndexBuildResult>::alloc() };
    result.heap_tuples = state.heap_tuples as f64;
    result.index_tuples = state.index_tuples as f64;

    result.into_pg()
}

#[pgrx::pg_guard]
pub extern "C-unwind" fn build_callback(
    _index: pgrx::pg_sys::Relation,
    ctid: pgrx::pg_sys::ItemPointer,
    datum: *mut pgrx::pg_sys::Datum,
    is_null: *mut bool,
    _tuple_is_alive: bool,
    state: *mut std::os::raw::c_void,
) {
    unsafe {
        let state = &mut *(state.cast::<BuildState>());
        state.memctx.reset();
        state.memctx.switch_to(|_| {
            let Some(vector) = Bm25VectorInput::from_datum(*datum, *is_null) else {
                return;
            };
            let id = item_pointer_to_u64(ctid.read());
            state.builder.insert(id, vector.borrow());
            state.index_tuples += 1;
        });
        state.memctx.reset();
        state.heap_tuples += 1;
    }
}

fn write_down(state: &BuildState) {
    let doc_cnt = state.builder.doc_cnt();
    let doc_term_cnt = state.builder.doc_term_cnt();
    let term_id_cnt = state.builder.term_id_cnt();

    let mut meta_page = page_write(state.index, METAPAGE_BLKNO);
    let meta: &mut MetaPageData = meta_page.init_mut();
    meta.doc_cnt = doc_cnt;
    meta.doc_term_cnt = doc_term_cnt;
    meta.term_id_cnt = term_id_cnt;
    meta.current_doc_id = doc_cnt;
    meta.sealed_doc_id = doc_cnt;

    // delete bitmap
    let mut delete_bitmap_writer = VirtualPageWriter::new(state.index, PageFlags::DELETE, true);
    for _ in 0..(doc_cnt.div_ceil(8)) {
        delete_bitmap_writer.write(&[0u8]);
    }
    let delete_bitmap_blkno = delete_bitmap_writer.finalize();

    // term stat
    let mut term_stat_writer = VirtualPageWriter::new(state.index, PageFlags::TERM_STATISTIC, true);
    for count in state.builder.term_stat() {
        term_stat_writer.write(bytemuck::bytes_of(&count));
    }
    let term_stat_blkno = term_stat_writer.finalize();

    let (payload_blkno, field_norm_blkno, sealed_data) = state.builder.serialize(state.index);

    meta.field_norm_blkno = field_norm_blkno;
    meta.payload_blkno = payload_blkno;
    meta.term_stat_blkno = term_stat_blkno;
    meta.delete_bitmap_blkno = delete_bitmap_blkno;
    meta.sealed_segment = sealed_data;
}
