use crate::guc::ENABLE_INDEX;

use super::{
    build::{ambuild, ambuildempty},
    insert::aminsert,
    options::amoptions,
    scan::{ambeginscan, amendscan, amgettuple, amrescan},
    vacuum::{ambulkdelete, amvacuumcleanup},
};

#[pgrx::pg_extern(sql = "\
CREATE FUNCTION _bm25_amhandler(internal) RETURNS index_am_handler
IMMUTABLE STRICT PARALLEL SAFE LANGUAGE c AS 'MODULE_PATHNAME', '@FUNCTION_NAME@';
")]
fn _bm25_amhandler(
    _fcinfo: pgrx::pg_sys::FunctionCallInfo,
) -> pgrx::PgBox<pgrx::pg_sys::IndexAmRoutine> {
    let mut amroutine = unsafe {
        pgrx::PgBox::<pgrx::pg_sys::IndexAmRoutine>::alloc_node(
            pgrx::pg_sys::NodeTag::T_IndexAmRoutine,
        )
    };

    amroutine.amcanorderbyop = true;
    amroutine.amoptionalkey = true;

    amroutine.ambuild = Some(ambuild);
    amroutine.ambuildempty = Some(ambuildempty);
    amroutine.aminsert = Some(aminsert);
    amroutine.ambulkdelete = Some(ambulkdelete);
    amroutine.amvacuumcleanup = Some(amvacuumcleanup);
    amroutine.amcostestimate = Some(amcostestimate);
    amroutine.amoptions = Some(amoptions);
    amroutine.amproperty = Some(ampropety);
    amroutine.amvalidate = Some(amvalidate);
    amroutine.ambeginscan = Some(ambeginscan);
    amroutine.amrescan = Some(amrescan);
    amroutine.amgettuple = Some(amgettuple);
    amroutine.amendscan = Some(amendscan);

    amroutine.into_pg_boxed()
}

#[pgrx::pg_guard]
pub extern "C-unwind" fn amvalidate(_opclass_oid: pgrx::pg_sys::Oid) -> bool {
    // TODO: Implement validation
    true
}

#[allow(clippy::too_many_arguments)]
#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn amcostestimate(
    _root: *mut pgrx::pg_sys::PlannerInfo,
    path: *mut pgrx::pg_sys::IndexPath,
    _loop_count: f64,
    index_startup_cost: *mut pgrx::pg_sys::Cost,
    index_total_cost: *mut pgrx::pg_sys::Cost,
    index_selectivity: *mut pgrx::pg_sys::Selectivity,
    index_correlation: *mut f64,
    index_pages: *mut f64,
) {
    if !ENABLE_INDEX.get() || ((*path).indexorderbys.is_null() && (*path).indexclauses.is_null()) {
        *index_startup_cost = f64::MAX;
        *index_total_cost = f64::MAX;
        *index_selectivity = 0.0;
        *index_correlation = 0.0;
        *index_pages = 0.0;
        return;
    }
    // TODO: Implement detailed cost estimation
    *index_startup_cost = 0.0;
    *index_total_cost = 0.0;
    *index_selectivity = 1.0;
    *index_correlation = 1.0;
    *index_pages = 0.0;
}

#[pgrx::pg_guard]
pub unsafe extern "C-unwind" fn ampropety(
    _index_oid: pgrx::pg_sys::Oid,
    attno: i32,
    prop: pgrx::pg_sys::IndexAMProperty::Type,
    _propname: *const std::os::raw::c_char,
    res: *mut bool,
    isnull: *mut bool,
) -> bool {
    if attno == 1 && prop == pgrx::pg_sys::IndexAMProperty::AMPROP_DISTANCE_ORDERABLE {
        *res = true;
        *isnull = false;
        return true;
    }
    false
}
