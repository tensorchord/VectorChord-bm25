-- vchord_bm25--0.2.0--0.2.1.sql

-- complain if script is sourced in psql, rather than via CREATE EXTENSION
\echo Use "ALTER EXTENSION vchord_bm25 UPDATE TO '0.2.1'" to load this file. \quit

/* <begin connected objects> */
-- src/datatype/operator_bm25vector.rs:3
-- vchord_bm25::datatype::operator_bm25vector::_bm25catalog_bm25vector_operator_eq
CREATE  FUNCTION "_bm25catalog_bm25vector_operator_eq"(
	"lhs" bm25vector, /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
	"rhs" bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
) RETURNS bool /* bool */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_bm25catalog_bm25vector_operator_eq_wrapper';
/* </end connected objects> */

/* <begin connected objects> */
-- src/datatype/operator_bm25vector.rs:8
-- vchord_bm25::datatype::operator_bm25vector::_bm25catalog_bm25vector_operator_neq
CREATE  FUNCTION "_bm25catalog_bm25vector_operator_neq"(
	"lhs" bm25vector, /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
	"rhs" bm25vector /* vchord_bm25::datatype::memory_bm25vector::Bm25VectorInput */
) RETURNS bool /* bool */
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c /* Rust */
AS 'MODULE_PATHNAME', '_bm25catalog_bm25vector_operator_neq_wrapper';
/* </end connected objects> */

CREATE OPERATOR = (
    PROCEDURE = _bm25catalog_bm25vector_operator_eq,
    LEFTARG = bm25vector,
    RIGHTARG = bm25vector,
    COMMUTATOR = =,
    NEGATOR = <>,
    RESTRICT = eqsel,
    JOIN = eqjoinsel
);

CREATE OPERATOR <> (
    PROCEDURE = _bm25catalog_bm25vector_operator_neq,
    LEFTARG = bm25vector,
    RIGHTARG = bm25vector,
    COMMUTATOR = <>,
    NEGATOR = =,
    RESTRICT = eqsel,
    JOIN = eqjoinsel
);