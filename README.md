# VectorChord-BM25

A PostgreSQL extension for bm25 ranking algorithm. We implemented the Block-WeakAnd Algorithms for BM25 ranking inside PostgreSQL. This extension is currently in **alpha** stage and not recommended for production use. We're still iterating on the tokenizer API to support more configurations and languages. The interface may change in the future.

## Getting Started
For new users, we recommend using the Docker image to get started quickly.

```
docker run \
  --name vectorchord-demo \
  -e POSTGRES_PASSWORD=mysecretpassword \
  -p 5432:5432 \
  -d ghcr.io/tensorchord/vchord_bm25-postgres:pg17-v0.1.0
```

Then you can connect to the database using the psql command line tool. The default username is postgres, and the default password is mysecretpassword.

```
psql -h localhost -p 5432 -U postgres
```

Run the following SQL to ensure the extension is enabled.

```
CREATE EXTENSION IF NOT EXISTS vchord_bm25 CASCADE;
```
And make sure to add vchord_bm25.so to the shared_preload_libraries in postgresql.conf and add bm25_catalog to search path.
```
-- Add vchord-bm25 to shared_preload_libraries --
ALTER SYSTEM SET shared_preload_libraries = 'vchord_bm25.so';
ALTER SYSTEM SET search_path TO "$user", public, bm25_catalog;
```

## Usage

The extension is mainly composed by three parts, tokenizer, bm25vector and bm25vector index. The tokenizer is used to convert the text into a bm25vector, and the bm25vector is similar to a sparse vector, which stores the vocabulary id and frequency. The bm25vector index is used to speed up the search and ranking process.

To tokenize a text, you can use the `tokenize` function. The `tokenize` function takes two arguments, the text to tokenize and the tokenizer name. 

```sql
-- tokenize text with bert tokenizer
SELECT tokenize('A quick brown fox jumps over the lazy dog.', 'Bert');
-- Output: {2474:1, 2829:1, 3899:1, 4248:1, 4419:1, 5376:1, 5831:1}
-- The output is a bm25vector, 2474:1 means the word with id 2474 appears once in the text.
```

One thing special about bm25 score is that it depends on a global document frequency, which means the score of a word in a document depends on the frequency of the word in all documents. To calculate the bm25 score between a bm25vector and a query, you need had a document set first and then use the `<&>` operator.

```sql
-- Setup the document table
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    passage TEXT,
    embedding bm25vector
);

INSERT INTO documents (passage) VALUES
('PostgreSQL is a powerful, open-source object-relational database system. It has over 15 years of active development.'),
('Full-text search is a technique for searching in plain-text documents or textual database fields. PostgreSQL supports this with tsvector.'),
('BM25 is a ranking function used by search engines to estimate the relevance of documents to a given search query.'),
('PostgreSQL provides many advanced features like full-text search, window functions, and more.'),
('Search and ranking in databases are important in building effective information retrieval systems.'),
('The BM25 ranking algorithm is derived from the probabilistic retrieval framework.'),
('Full-text search indexes documents to allow fast text queries. PostgreSQL supports this through its GIN and GiST indexes.'),
('The PostgreSQL community is active and regularly improves the database system.'),
('Relational databases such as PostgreSQL can handle both structured and unstructured data.'),
('Effective search ranking algorithms, such as BM25, improve search results by understanding relevance.');
```

Then tokenize it 

```sql
UPDATE documents SET embedding = tokenize(passage, 'Bert');
```

Create the index on the bm25vector column so that we can collect the global document frequency.

```sql
CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);
```

Now we can calculate the BM25 score between the query and the vectors. Note that the bm25 score is negative, which means the higher the score, the more relevant the document is. We intentionally make it negative so that you can use the default order by to get the most relevant documents first.

```sql
-- to_bm25query(index_name, query, tokenizer_name)
-- <&> is the operator to compute the bm25 score
SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', 'PostgreSQL', 'Bert') AS bm25_score;
```

And you can use the order by to utilize the index to get the most relevant documents first and faster.
```sql
SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', 'PostgreSQL', 'Bert') AS rank
FROM documents
ORDER BY rank
LIMIT 10;
```


<!-- ## Performance Benchmark

We used datasets are from [xhluca/bm25-benchmarks](https://github.com/xhluca/bm25-benchmarks) and compare the results with ElasticSearch and Lucene. The QPS reflects the query efficiency with the index structure. And the NDCG@10 reflects the ranking quality of the search engine, which is totally based on the tokenizer. This means we can achieve the same ranking quality as ElasticSearch and Lucene if using the exact same tokenizer. 

### QPS Result

| Dataset          | VectorChord-BM25 | ElasticSearch |
| ---------------- | ---------------- | ------------- |
| trec-covid       | 28.38            | 27.31         |
| webis-touche2020 | 38.57            | 32.05         |

### NDCG@10 Result

| Dataset          | VectorChord-BM25 | ElasticSearch | Lucene |
| ---------------- | ---------------- | ------------- | ------ |
| trec-covid       | 67.67            | 68.80         | 61.0   |
| webis-touche2020 | 31.0             | 34.70         | 33.2   |

## Installation

1. Setup development environment.

You can follow the docs about [`pgvecto.rs`](https://docs.pgvecto.rs/developers/development.html).

2. Install the extension.

```sh
cargo pgrx install --sudo --release
```

3. Configure your PostgreSQL by modifying `search_path` to include the extension.

```sh
psql -U postgres -c 'ALTER SYSTEM SET search_path TO "$user", public, bm25_catalog'
# You need restart the PostgreSQL cluster to take effects.
sudo systemctl restart postgresql.service   # for vchord_bm25.rs running with systemd
```

4. Connect to the database and enable the extension.

```sql
DROP EXTENSION IF EXISTS vchord_bm25;
CREATE EXTENSION vchord_bm25;
``` -->

## Comparison to other solution in Postgres
PostgreSQL supports full-text search using the tsvector data type and GIN indexes. Text is transformed into a tsvector, which tokenizes content into standardized lexemes, and a GIN index accelerates searches—even on large text fields. However, PostgreSQL lacks modern relevance scoring methods like BM25; it retrieves all matching documents and re-ranks them using ts_rank, which is inefficient and can obscure the most relevant results.

ParadeDB is an alternative that functions as a full-featured PostgreSQL replacement for ElasticSearch. It offloads full-text search and filtering operations to Tantivy and includes BM25 among its features, though it uses a different query and filter syntax than PostgreSQL's native indexes.

In contrast, Vectorchord-bm25 focuses exclusively on BM25 ranking within PostgreSQL. We implemented the BM25 ranking algorithm Block WeakAnd from scratch and built it as a custom operator and index (similar to pgvector) to accelerate queries. It is designed to be lightweight and a more native and intuitive API for better full-text search and ranking in PostgreSQL.

## Limitation
- The index will return up to `bm25_catalog.bm25_limit` results to PostgreSQL. Users need to adjust the `bm25_catalog.bm25_limit` for more results when using larger limit values or stricter filter conditions.
- We currently have only tested against English. Other language can be supported with bpe tokenizer with larger vocab like tiktoken out of the box. Feel free to talk to us or raise issue if you need more language support.

## Reference

### Data Types

- `bm25vector`: A vector type for storing BM25 tokenized text.
- `bm25query`: A query type for BM25 ranking.

### Functions

- `create_tokenizer(tokenizer_name text, config text)`: Create a tokenizer with the given name and configuration.
- `create_unicode_tokenizer_and_trigger(tokenizer_name text, table_name text, source_column text, target_column text)`: Create a Unicode tokenizer and trigger function for the given table and columns. It will automatically build the tokenizer according to source_column and store the result in target_column.
- `drop_tokenizer(tokenizer_name text)`: Drop the tokenizer with the given name.
- `tokenize(content text, tokenizer_name text) RETURNS bm25vector`: Tokenize the content text into a BM25 vector. 
- `to_bm25query(index_name regclass, query text, tokenizer_name text) RETURNS bm25query`: Convert the input text into a BM25 query.
- `bm25vector <&> bm25query RETURNS float4`: Calculate the **negative** BM25 score between the BM25 vector and query.

For more information about tokenizer, check the [tokenizer](./tokenizer.md) document.

### GUCs

- `bm25_catalog.bm25_limit (integer)`: The maximum number of documents to return in a search. Default is 100, minimum is -1, and maximum is 65535. When set to -1, it will perform brute force search and return all documents with scores greater than 0.
- `bm25_catalog.enable_index (boolean)`: Whether to enable the bm25 index. Default is true.
- `bm25_catalog.segment_growing_max_page_size (integer)`: The maximum page count of the growing segment. When the size of the growing segment exceeds this value, the segment will be sealed into a read-only segment. Default is 1,000, minimum is 1, and maximum is 1,000,000.

## Contribution

- For new tokenizer, check the [tokenizer](./tokenizer.md#contribution) document.

## License

This software is licensed under a dual license model:

1. **GNU Affero General Public License v3 (AGPLv3)**: You may use, modify, and distribute this software under the terms of the AGPLv3.

2. **Elastic License v2 (ELv2)**: You may also use, modify, and distribute this software under the Elastic License v2, which has specific restrictions.

You may choose either license based on your needs. We welcome any commercial collaboration or support, so please email us <vectorchord-inquiry@tensorchord.ai> with any questions or requests regarding the licenses.