-- List of types

CREATE TYPE bm25query AS (
    vector tsvector,
    index regclass
);

-- List of operators

CREATE OPERATOR <&> (
    PROCEDURE = _bm25_evaluate,
    LEFTARG = tsvector,
    RIGHTARG = bm25query
);

-- List of functions

CREATE FUNCTION bm25_amhandler(internal) RETURNS index_am_handler
IMMUTABLE STRICT PARALLEL SAFE LANGUAGE c AS 'MODULE_PATHNAME', '_bm25_amhandler_wrapper';

CREATE FUNCTION to_bm25query(tsvector, regclass) RETURNS bm25query
IMMUTABLE PARALLEL SAFE LANGUAGE sql AS 'SELECT ROW($1, $2)::bm25query';

-- List of access methods

CREATE ACCESS METHOD bm25 TYPE INDEX HANDLER bm25_amhandler;

-- List of operator families

CREATE OPERATOR FAMILY bm25_ops USING bm25;

-- List of operator classes

CREATE OPERATOR CLASS bm25_ops FOR TYPE tsvector USING bm25 FAMILY bm25_ops AS
    OPERATOR 1 <&>(tsvector, bm25query) FOR ORDER BY float_ops;
