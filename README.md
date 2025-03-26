<div align="center">
<img src="https://github.com/tensorchord/marketing-material/raw/main/vectorchord/logo-light.svg" width="200" alt="VectorChord Logo">
<h4 align=center></h4>
</div>

<p align=center>
<a href="https://discord.gg/KqswhpVgdU"><img alt="discord invitation link" src="https://dcbadge.vercel.app/api/server/KqswhpVgdU?style=flat"></a>
<a href="https://twitter.com/TensorChord"><img src="https://img.shields.io/twitter/follow/tensorchord?style=social" alt="Twitter" /></a>
</p>

VectorChord-BM25 is a PostgreSQL extension for bm25 ranking algorithm. We implemented the Block-WeakAnd Algorithms for BM25 ranking inside PostgreSQL. This extension is currently in **alpha** stage and not recommended for production use. We're still iterating on the tokenizer API to support more configurations and languages. The interface may change in the future.

## Getting Started
For new users, we recommend using the Docker image to get started quickly.

```sh
docker run \
  --name vectorchord-demo \
  -e POSTGRES_PASSWORD=mysecretpassword \
  -p 5432:5432 \
  -d ghcr.io/tensorchord/vchord_bm25-postgres:pg17-v0.1.1
```

Once everything’s set up, you can connect to the database using the `psql` command line tool. The default username is `postgres`, and the default password is `mysecretpassword`. Here’s how to connect:

```sh
psql -h localhost -p 5432 -U postgres
```

After connecting, run the following SQL to make sure the extension is enabled:

```sql
CREATE EXTENSION IF NOT EXISTS pg_tokenizer CASCADE;  -- for tokenizer
CREATE EXTENSION IF NOT EXISTS vchord_bm25 CASCADE;   -- for bm25 ranking
```

Then, don’t forget to add `tokenizer_catalog` and `bm25_catalog` to the search path:

```sql
ALTER SYSTEM SET search_path TO "$user", public, tokenizer_catalog, bm25_catalog;
SELECT pg_reload_conf();
```

## Usage

The extension is mainly composed by three parts, tokenizer, bm25vector and bm25vector index. The tokenizer is used to convert the text into a bm25vector, and the bm25vector is similar to a sparse vector, which stores the vocabulary id and frequency. The bm25vector index is used to speed up the search and ranking process.

To tokenize a text, you can use the `tokenize` function. The `tokenize` function takes two arguments, the text to tokenize and the tokenizer name. 

> Tokenizer part is completed by a separate extension [pg_tokenizer.rs](https://github.com/tensorchord/pg_tokenizer.rs), more details can be found [here](https://github.com/tensorchord/pg_tokenizer.rs/tree/main/docs).

```sql
-- create a tokenizer
SELECT create_tokenizer('bert', $$
model = "bert_base_uncased"  -- using pre-tarined model
$$);
-- tokenize text with bert tokenizer
SELECT tokenize('A quick brown fox jumps over the lazy dog.', 'bert');
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
UPDATE documents SET embedding = tokenize(passage, 'bert');
```

Create the index on the bm25vector column so that we can collect the global document frequency.

```sql
CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);
```

Now we can calculate the BM25 score between the query and the vectors. Note that the bm25 score is negative, which means the higher the score, the more relevant the document is. We intentionally make it negative so that you can use the default order by to get the most relevant documents first.

```sql
-- to_bm25query(index_name, query, tokenizer_name)
-- <&> is the operator to compute the bm25 score
SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', tokenize('PostgreSQL', 'bert')) AS bm25_score FROM documents;
```

And you can use the order by to utilize the index to get the most relevant documents first and faster.
```sql
SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', tokenize('PostgreSQL', 'bert')) AS rank
FROM documents
ORDER BY rank
LIMIT 10;
```

## More Examples

### Using custom model

You can also build a custom model based on your own corpus easily.

```sql
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    passage TEXT,
    embedding bm25vector
);

-- create a text analyzer to generate tokens that can be used to train the model
SELECT create_text_analyzer('text_analyzer1', $$
pre_tokenizer = "unicode_segmentation"  # split texts according to the Unicode Standard Annex #29
[[character_filters]]
to_lowercase = {}                       # convert all characters to lowercase
[[character_filters]]
unicode_normalization = "nfkd"          # normalize the text to Unicode Normalization Form KD
[[token_filters]]
skip_non_alphanumeric = {}              # skip tokens that all characters are not alphanumeric
[[token_filters]]
stopwords = "nltk_english"              # remove stopwords using the nltk dictionary
[[token_filters]]
stemmer = "english_porter2"             # stem tokens using the English Porter2 stemmer
$$);

-- create a model to generate embeddings from original passage
-- It'll train a model from passage column and store the embeddings in the embedding column
SELECT create_custom_model_tokenizer_and_trigger(
    tokenizer_name => 'tokenizer1',
    model_name => 'model1',
    text_analyzer_name => 'text_analyzer1',
    table_name => 'documents',
    source_column => 'passage',
    target_column => 'embedding'
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

CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);

SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', tokenize('PostgreSQL', 'tokenizer1')) AS rank
FROM documents
ORDER BY rank
LIMIT 10;
```

### Using jieba for Chinese text

For chinese text, you can use [`jieba`](https://github.com/messense/jieba-rs) pre-tokenizer to segment the text into words. And then train a custom model with segmented words.

```sql
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    passage TEXT,
    embedding bm25vector
);

-- create a text analyzer which uses jieba pre-tokenizer
SELECT create_text_analyzer('text_analyzer1', $$
[pre_tokenizer.jieba]
$$);

SELECT create_custom_model_tokenizer_and_trigger(
    tokenizer_name => 'tokenizer1',
    model_name => 'model1',
    text_analyzer_name => 'text_analyzer1',
    table_name => 'documents',
    source_column => 'passage',
    target_column => 'embedding'
);

INSERT INTO documents (passage) VALUES 
('红海早过了，船在印度洋面上开驶着，但是太阳依然不饶人地迟落早起，侵占去大部分的夜。'),
('夜仿佛纸浸了油变成半透明体；它给太阳拥抱住了，分不出身来，也许是给太阳陶醉了，所以夕照晚霞褪后的夜色也带着酡红。'),
('到红消醉醒，船舱里的睡人也一身腻汗地醒来，洗了澡赶到甲板上吹海风，又是一天开始。'),
('这是七月下旬，合中国旧历的三伏，一年最热的时候。在中国热得更比常年利害，事后大家都说是兵戈之象，因为这就是民国二十六年【一九三七年】。'),
('这条法国邮船白拉日隆子爵号（VicomtedeBragelonne）正向中国开来。'),
('早晨八点多钟，冲洗过的三等舱甲板湿意未干，但已坐满了人，法国人、德国流亡出来的犹太人、印度人、安南人，不用说还有中国人。'),
('海风里早含着燥热，胖人身体给炎风吹干了，上一层汗结的盐霜，仿佛刚在巴勒斯坦的死海里洗过澡。'),
('毕竟是清晨，人的兴致还没给太阳晒萎，烘懒，说话做事都很起劲。'),
('那几个新派到安南或中国租界当警察的法国人，正围了那年轻善撒娇的犹太女人在调情。'),
('俾斯麦曾说过，法国公使大使的特点，就是一句外国话不会讲；这几位警察并不懂德文，居然传情达意，引得犹太女人格格地笑，比他们的外交官强多了。'),
('这女人的漂亮丈夫，在旁顾而乐之，因为他几天来，香烟、啤酒、柠檬水沾光了不少。'),
('红海已过，不怕热极引火，所以等一会甲板上零星果皮、纸片、瓶塞之外，香烟头定又遍处皆是。'),
('法国人的思想是有名的清楚，他的文章也明白干净，但是他的做事，无不混乱、肮脏、喧哗，但看这船上的乱糟糟。'),
('这船，倚仗人的机巧，载满人的扰攘，寄满人的希望，热闹地行着，每分钟把沾污了人气的一小方小面，还给那无情、无尽、无际的大海。');

CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);

SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', tokenize('人', 'tokenizer1')) AS rank
FROM documents
ORDER BY rank
LIMIT 10;
```

### Using lindera for Japanese text

For Japanese text, you can use [`lindera`](https://github.com/lindera/lindera) model with its configuration.

> It requires extra compile flags. We don't enable it default, and you need to recompile it from source.

```sql
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    passage TEXT,
    embedding bm25vector
);

-- using lindera config to customize the tokenizer, see https://github.com/lindera/lindera
SELECT create_lindera_model('lindera_ipadic', $$
[segmenter]
mode = "normal"
  [segmenter.dictionary]
  kind = "ipadic"
[[character_filters]]
kind = "unicode_normalize"
  [character_filters.args]
  kind = "nfkc"
[[character_filters]]
kind = "japanese_iteration_mark"
  [character_filters.args]
  normalize_kanji = true
  normalize_kana = true
[[character_filters]]
kind = "mapping"
[character_filters.args.mapping]
"リンデラ" = "Lindera"
[[token_filters]]
kind = "japanese_compound_word"
  [token_filters.args]
  kind = "ipadic"
  tags = [ "名詞,数", "名詞,接尾,助数詞" ]
  new_tag = "名詞,数"
[[token_filters]]
kind = "japanese_number"
  [token_filters.args]
  tags = [ "名詞,数" ]
[[token_filters]]
kind = "japanese_stop_tags"
  [token_filters.args]
  tags = [
  "接続詞",
  "助詞",
  "助詞,格助詞",
  "助詞,格助詞,一般",
  "助詞,格助詞,引用",
  "助詞,格助詞,連語",
  "助詞,係助詞",
  "助詞,副助詞",
  "助詞,間投助詞",
  "助詞,並立助詞",
  "助詞,終助詞",
  "助詞,副助詞／並立助詞／終助詞",
  "助詞,連体化",
  "助詞,副詞化",
  "助詞,特殊",
  "助動詞",
  "記号",
  "記号,一般",
  "記号,読点",
  "記号,句点",
  "記号,空白",
  "記号,括弧閉",
  "その他,間投",
  "フィラー",
  "非言語音"
]
[[token_filters]]
kind = "japanese_katakana_stem"
  [token_filters.args]
  min = 3
[[token_filters]]
kind = "remove_diacritical_mark"
  [token_filters.args]
  japanese = false
$$);

SELECT create_tokenizer('lindera_ipadic', $$
model = "lindera_ipadic"
$$);

INSERT INTO documents (passage) VALUES 
('どこで生れたかとんと見当けんとうがつかぬ。'),
('何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。'),
('吾輩はここで始めて人間というものを見た。'),
('しかもあとで聞くとそれは書生という人間中で一番獰悪どうあくな種族であったそうだ。'),
('この書生というのは時々我々を捕つかまえて煮にて食うという話である。'),
('しかしその当時は何という考もなかったから別段恐しいとも思わなかった。'),
('ただ彼の掌てのひらに載せられてスーと持ち上げられた時何だかフワフワした感じがあったばかりである。'),
('掌の上で少し落ちついて書生の顔を見たのがいわゆる人間というものの見始みはじめであろう。'),
('この時妙なものだと思った感じが今でも残っている。'),
('第一毛をもって装飾されべきはずの顔がつるつるしてまるで薬缶やかんだ。'),
('その後ご猫にもだいぶ逢あったがこんな片輪かたわには一度も出会でくわした事がない。'),
('のみならず顔の真中があまりに突起している。'),
('そうしてその穴の中から時々ぷうぷうと煙けむりを吹く。'),
('どうも咽むせぽくて実に弱った。'),
('これが人間の飲む煙草たばこというものである事はようやくこの頃知った。');

UPDATE documents SET embedding = tokenize(passage, 'lindera_ipadic');

CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);

SELECT id, passage, embedding <&> to_bm25query('documents_embedding_bm25', tokenize('書生', 'lindera_ipadic')) AS rank
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

- `bm25vector`: A specialized vector type for storing BM25 tokenized text. Structured as a sparse vector, it stores token IDs and their corresponding frequencies. For example, `{1:2, 2:1}` indicates that token ID 1 appears twice and token ID 2 appears once in the document.
- `bm25query`: A query type for BM25 ranking.

### Functions

- `to_bm25query(index_name regclass, query_vector bm25vector) RETURNS bm25query`: Convert the input text into a BM25 query.
- `bm25vector <&> bm25query RETURNS float4`: Calculate the **negative** BM25 score between the BM25 vector and query.

### Casts

- `int[]::bm25vector (implicit)`: Cast an integer array to a BM25 vector. The integer array represents token IDs, and the cast aggregates duplicates into frequencies, ignoring token order. For example, `{1, 2, 1}` will be cast to `{1:2, 2:1}` (token ID 1 appears twice, token ID 2 appears once).

### GUCs

- `bm25_catalog.bm25_limit (integer)`: The maximum number of documents to return in a search. Default is 100, minimum is -1, and maximum is 65535. When set to -1, it will perform brute force search and return all documents with scores greater than 0.
- `bm25_catalog.enable_index (boolean)`: Whether to enable the bm25 index. Default is true.
- `bm25_catalog.segment_growing_max_page_size (integer)`: The maximum page count of the growing segment. When the size of the growing segment exceeds this value, the segment will be sealed into a read-only segment. Default is 4,096, minimum is 1, and maximum is 1,000,000.

## License

This software is licensed under a dual license model:

1. **GNU Affero General Public License v3 (AGPLv3)**: You may use, modify, and distribute this software under the terms of the AGPLv3.

2. **Elastic License v2 (ELv2)**: You may also use, modify, and distribute this software under the Elastic License v2, which has specific restrictions.

You may choose either license based on your needs. We welcome any commercial collaboration or support, so please email us <vectorchord-inquiry@tensorchord.ai> with any questions or requests regarding the licenses.
