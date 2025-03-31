-- vchord_bm25--0.1.1--0.2.0.sql

-- complain if script is sourced in psql, rather than via CREATE EXTENSION
\echo Use "ALTER EXTENSION vchord_bm25 UPDATE TO '0.2.0'" to load this file. \quit

-- Drop the tokenizers table and any associated triggers
DROP TABLE IF EXISTS bm25_catalog.tokenizers CASCADE;

-- Drop old functions related to tokenization
DROP FUNCTION IF EXISTS unicode_tokenizer_split(TEXT, bytea);
DROP FUNCTION IF EXISTS create_unicode_tokenizer_and_trigger(TEXT, TEXT, TEXT, TEXT);
DROP FUNCTION IF EXISTS tokenize(TEXT, TEXT);
DROP FUNCTION IF EXISTS create_tokenizer(TEXT, TEXT);
DROP FUNCTION IF EXISTS drop_tokenizer(TEXT);
DROP FUNCTION IF EXISTS unicode_tokenizer_set_target_column_trigger();
DROP FUNCTION IF EXISTS unicode_tokenizer_insert_trigger();
DROP FUNCTION IF EXISTS to_bm25query(regclass, TEXT, TEXT);

-- Create the new cast function and cast
CREATE FUNCTION _vchord_bm25_cast_array_to_bm25vector(
    "array" INT[],
    "_typmod" INT,
    "_explicit" bool
) RETURNS bm25vector
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE c
AS 'MODULE_PATHNAME', '_vchord_bm25_cast_array_to_bm25vector_wrapper';

CREATE CAST (int[] AS bm25vector)
    WITH FUNCTION _vchord_bm25_cast_array_to_bm25vector(int[], integer, boolean) AS IMPLICIT;

-- Update the to_bm25query function
CREATE OR REPLACE FUNCTION to_bm25query(index_oid regclass, query_vector bm25vector)
RETURNS bm25query
IMMUTABLE STRICT PARALLEL SAFE
LANGUAGE sql
AS $$
    SELECT index_oid, query_vector;
$$;
