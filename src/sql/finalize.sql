-- List of types

CREATE TYPE bm25vector (
    INPUT = _vchord_bm25_bm25vector_in,
    OUTPUT = _vchord_bm25_bm25vector_out,
    RECEIVE = _vchord_bm25_bm25vector_recv,
    SEND = _vchord_bm25_bm25vector_send,
    STORAGE = external
);

CREATE TYPE bm25query AS (
    index regclass,
    vector bm25vector
);

-- List of casts

CREATE CAST (int[] AS bm25vector)
    WITH FUNCTION _vchord_bm25_bm25vector_cast_intarray_bm25vector(int[]) AS ASSIGNMENT;

CREATE CAST (bm25vector AS int[])
    WITH FUNCTION _vchord_bm25_bm25vector_cast_bm25vector_intarray(bm25vector) AS ASSIGNMENT;

-- List of operators

CREATE OPERATOR = (
    PROCEDURE = _vchord_bm25_bm25vector_operator_eq,
    LEFTARG = bm25vector,
    RIGHTARG = bm25vector,
    COMMUTATOR = =,
    NEGATOR = <>,
	RESTRICT = eqsel,
    JOIN = eqjoinsel
);

CREATE OPERATOR <> (
    PROCEDURE = _vchord_bm25_bm25vector_operator_neq,
    LEFTARG = bm25vector,
    RIGHTARG = bm25vector,
    COMMUTATOR = <>,
    NEGATOR = =,
	RESTRICT = neqsel,
    JOIN = neqjoinsel
);

CREATE OPERATOR <&> (
    PROCEDURE = _bm25_evaluate,
    LEFTARG = bm25vector,
    RIGHTARG = bm25query
);

-- List of functions

CREATE FUNCTION bm25_amhandler(internal) RETURNS index_am_handler
IMMUTABLE STRICT PARALLEL SAFE LANGUAGE c AS 'MODULE_PATHNAME', '_bm25_amhandler_wrapper';

CREATE FUNCTION to_bm25query(regclass, bm25vector) RETURNS bm25query
IMMUTABLE PARALLEL SAFE LANGUAGE sql AS 'SELECT ROW($1, $2)::bm25query';

CREATE FUNCTION to_bm25query(regclass, int[]) RETURNS bm25query
IMMUTABLE PARALLEL SAFE LANGUAGE sql AS 'SELECT ROW($1, $2::bm25vector)::bm25query';

-- List of access methods

CREATE ACCESS METHOD bm25 TYPE INDEX HANDLER bm25_amhandler;

-- List of operator families

CREATE OPERATOR FAMILY bm25_ops USING bm25;

-- List of operator classes

CREATE OPERATOR CLASS bm25_ops FOR TYPE bm25vector USING bm25 FAMILY bm25_ops AS
    OPERATOR 1 <&>(bm25vector, bm25query) FOR ORDER BY float_ops;
