/* <begin connected objects> */
/*
This file is auto generated by pgrx.

The ordering of items is not stable, it is driven by a dependency graph.
*/
/* </end connected objects> */

/* <begin connected objects> */
-- src/lib.rs:13
-- bootstrap
CREATE TYPE bm25vector;
CREATE TYPE bm25query;
/* </end connected objects> */

/* <begin connected objects> */
-- src/index/am.rs:11
-- vchord_bm25::index::am::_bm25_amhandler
CREATE FUNCTION _bm25_amhandler(internal) RETURNS index_am_handler
IMMUTABLE STRICT PARALLEL SAFE LANGUAGE c AS 'MODULE_PATHNAME', '_bm25_amhandler_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/text_bm25vector.rs:136
-- vchord_bm25::datatype::text_bm25vector::_bm25catalog_bm25vector_in
CREATE  FUNCTION "_bm25catalog_bm25vector_in"(
	"input" cstring, /* &core::ffi::c_str::CStr */
	"_oid" oid, /* pgrx_pg_sys::submodules::oids::Oid */
	"_typmod" INT /* i32 */
) RETURNS bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorOutput */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_bm25catalog_bm25vector_in_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/text_bm25vector.rs:145
-- vchord_bm25::datatype::text_bm25vector::_bm25catalog_bm25vector_out
CREATE  FUNCTION "_bm25catalog_bm25vector_out"(
	"vector" bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
) RETURNS cstring /* alloc::ffi::c_str::CString */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_bm25catalog_bm25vector_out_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/binary_bm25vector.rs:30
-- vchord_bm25::datatype::binary_bm25vector::_bm25catalog_bm25vector_recv
CREATE  FUNCTION "_bm25catalog_bm25vector_recv"(
	"internal" internal, /* pgrx::datum::internal::Internal */
	"_oid" oid, /* pgrx_pg_sys::submodules::oids::Oid */
	"_typmod" INT /* i32 */
) RETURNS bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorOutput */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_bm25catalog_bm25vector_recv_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/binary_bm25vector.rs:12
-- vchord_bm25::datatype::binary_bm25vector::_bm25catalog_bm25vector_send
CREATE  FUNCTION "_bm25catalog_bm25vector_send"(
	"vector" bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
) RETURNS bytea /* vchord_bm25::datatype::bytea::Bytea */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_bm25catalog_bm25vector_send_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/cast.rs:3
-- vchord_bm25::datatype::cast::_vchord_bm25_cast_array_to_bm25vector
CREATE  FUNCTION "_vchord_bm25_cast_array_to_bm25vector"(
	"array" INT[], /* pgrx::datum::array::Array<i32> */
	"_typmod" INT, /* i32 */
	"_explicit" bool /* bool */
) RETURNS bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorOutput */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_vchord_bm25_cast_array_to_bm25vector_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/functions.rs:11
-- vchord_bm25::datatype::functions::search_bm25query
CREATE  FUNCTION "search_bm25query"(
	"target_vector" bm25vector, /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
	"query" bm25query /* pgrx::heap_tuple::PgHeapTuple<pgrx::pgbox::AllocatedByRust> */
) RETURNS real /* f32 */
STRICT STABLE PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', 'search_bm25query_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/lib.rs:14
-- finalize
CREATE TYPE bm25vector (
    INPUT = _bm25catalog_bm25vector_in,
    OUTPUT = _bm25catalog_bm25vector_out,
    RECEIVE = _bm25catalog_bm25vector_recv,
    SEND = _bm25catalog_bm25vector_send,
    STORAGE = EXTERNAL,
    INTERNALLENGTH = VARIABLE,
    ALIGNMENT = double
);

CREATE CAST (int[] AS bm25vector)
    WITH FUNCTION _vchord_bm25_cast_array_to_bm25vector(int[], integer, boolean) AS IMPLICIT;

CREATE TYPE bm25query AS (
    index_oid regclass,
    query_vector bm25vector
);

CREATE FUNCTION to_bm25query(index_oid regclass, query_vector bm25vector) RETURNS bm25query
    IMMUTABLE STRICT PARALLEL SAFE LANGUAGE sql AS $$
        SELECT index_oid, query_vector;
    $$;

CREATE ACCESS METHOD bm25 TYPE INDEX HANDLER _bm25_amhandler;
COMMENT ON ACCESS METHOD bm25 IS 'vchord bm25 index access method';

CREATE OPERATOR pg_catalog.<&> (
    PROCEDURE = search_bm25query,
    LEFTARG = bm25vector,
    RIGHTARG = bm25query
);

CREATE OPERATOR FAMILY bm25_ops USING bm25;

CREATE OPERATOR CLASS bm25_ops FOR TYPE bm25vector USING bm25 FAMILY bm25_ops AS
    OPERATOR 1 pg_catalog.<&>(bm25vector, bm25query) FOR ORDER BY float_ops;
/* </end connected objects> */

