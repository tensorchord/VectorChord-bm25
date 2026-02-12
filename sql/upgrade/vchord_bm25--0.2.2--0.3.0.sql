-- vchord_bm25--0.2.2--0.3.0.sql

CREATE FUNCTION "bm25_page_inspect"("index" regclass, "blkno" INT) RETURNS TEXT
STRICT LANGUAGE c AS 'MODULE_PATHNAME', 'bm25_page_inspect_wrapper';
