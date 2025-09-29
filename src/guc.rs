use pgrx::{GucContext, GucFlags, GucRegistry, GucSetting};

pub static BM25_LIMIT: GucSetting<i32> = GucSetting::<i32>::new(100);
pub static ENABLE_INDEX: GucSetting<bool> = GucSetting::<bool>::new(true);
pub static SEGMENT_GROWING_MAX_PAGE_SIZE: GucSetting<i32> = GucSetting::<i32>::new(4096);
pub static ENABLE_PREFILTER: GucSetting<bool> = GucSetting::<bool>::new(true);

pub fn init() {
    GucRegistry::define_int_guc(
        c"bm25_catalog.bm25_limit",
        c"bm25 query limit",
        c"The maximum number of documents to return in a search",
        &BM25_LIMIT,
        -1,
        65535,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"bm25_catalog.enable_index",
        c"Whether to enable the bm25 index",
        c"Whether to enable the bm25 index",
        &ENABLE_INDEX,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"bm25_catalog.segment_growing_max_page_size",
        c"bm25 growing segment max page size",
        c"The maximum page count of the growing segment. When the size of the growing segment exceeds this value, the segment will be sealed into a read-only segment.",
        &SEGMENT_GROWING_MAX_PAGE_SIZE,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"bm25_catalog.enable_prefilter",
        c"Whether to enable the prefilter",
        c"Whether to enable the prefilter for bm25 queries. If enabled, the prefilter will be used to filter out documents that do not match the query before scoring.",
        &ENABLE_PREFILTER,
        GucContext::Userset,
        GucFlags::default(),
    );
}
