// This software is licensed under a dual license model:
//
// GNU Affero General Public License v3 (AGPLv3): You may use, modify, and
// distribute this software under the terms of the AGPLv3.
//
// Elastic License v2 (ELv2): You may also use, modify, and distribute this
// software under the Elastic License v2, which has specific restrictions.
//
// We welcome any commercial collaboration or support. For inquiries
// regarding the licenses, please contact us at:
// vectorchord-inquiry@tensorchord.ai
//
// Copyright (c) 2025 TensorChord Inc.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

#[allow(unused)]
#[derive(Debug, Clone, Copy)]
enum Operation {
    Insert,
    Select,
    Delete,
    Vacuum,
}

const INIT_DOCUMENTS: u32 = 10000;
const DOCUMENT_MAX_TOKEN: u32 = 10000;
const DOCUMENT_LEN: u32 = 100;

const FUZZ_ITERATIONS: u32 = 500;
const FUZZ_OPERATIONS: [Operation; 3] = [
    Operation::Insert,
    Operation::Select,
    Operation::Delete,
    // Operation::Vacuum,
];

fn test(client: &mut postgres::Client) {
    let seed = rand::rng().random_range(0..u64::MAX);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    println!("Seed: {}", seed); // for reproducibility

    client
        .execute(
            r#"
        CREATE TABLE documents (
            id SERIAL PRIMARY KEY,
            embedding bm25vector
        );
        "#,
            &[],
        )
        .unwrap();

    // Insert initial documents
    for _ in 0..INIT_DOCUMENTS {
        let bm25vector = random_bm25vector(&mut rng);
        client
            .execute(
                r#"INSERT INTO documents (embedding) VALUES ($1::text::bm25vector);"#,
                &[&bm25vector],
            )
            .unwrap();
    }

    // Create index
    client
        .execute(
            r#"CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);"#,
            &[],
        )
        .unwrap();

    client
        .execute(
            r#"SET bm25_catalog.segment_growing_max_page_size = 1;"#,
            &[],
        )
        .unwrap();

    for _ in 0..FUZZ_ITERATIONS {
        let operation = FUZZ_OPERATIONS.choose(&mut rng).unwrap();
        match operation {
            Operation::Insert => fuzz_insert(client, &mut rng),
            Operation::Select => fuzz_select(client, &mut rng),
            Operation::Delete => fuzz_delete(client, &mut rng),
            Operation::Vacuum => fuzz_vacuum(client, &mut rng),
        }
    }

    client.execute(r#"DROP TABLE documents;"#, &[]).unwrap();
}

fn random_bm25vector(rng: &mut impl RngExt) -> String {
    pub fn from_ids(ids: impl Iterator<Item = u32>) -> String {
        use std::fmt::Write;
        let mut map: BTreeMap<u32, u32> = BTreeMap::new();
        for term_id in ids {
            *map.entry(term_id).or_insert(0) += 1;
        }
        let mut doc_len: u32 = 0;
        let mut indexes = Vec::with_capacity(map.len());
        let mut values = Vec::with_capacity(map.len());
        for (index, value) in map {
            indexes.push(index);
            values.push(value);
            doc_len = doc_len.checked_add(value).expect("overflow");
        }
        let mut buffer = String::new();
        buffer.push('{');
        let mut need_splitter = false;
        for (&index, &value) in indexes.iter().zip(values.iter()) {
            match need_splitter {
                false => {
                    write!(buffer, "{index}:{value}").unwrap();
                    need_splitter = true;
                }
                true => write!(buffer, ", {index}:{value}").unwrap(),
            }
        }
        buffer.push('}');
        buffer
    }
    let ids = (0..DOCUMENT_LEN).map(|_| rng.random_range(0..DOCUMENT_MAX_TOKEN));
    from_ids(ids)
}

fn fuzz_insert(client: &mut postgres::Client, rng: &mut impl RngExt) {
    let bm25vector = random_bm25vector(rng);
    client
        .execute(
            r#"INSERT INTO documents (embedding) VALUES ($1::text::bm25vector);"#,
            &[&bm25vector],
        )
        .unwrap();
}

#[derive(Clone, Copy)]
struct OrderedFloat(f32);
impl Debug for OrderedFloat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for OrderedFloat {}
impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn fuzz_select(client: &mut postgres::Client, rng: &mut impl RngExt) {
    let query_vector = random_bm25vector(rng);
    let query_vector_clone = query_vector.clone();

    client.execute("SET enable_seqscan = off", &[]).unwrap();
    client
        .execute("SET bm25_catalog.enable_index = on", &[])
        .unwrap();
    client
        .execute("SET bm25_catalog.bm25_limit = 200", &[])
        .unwrap();

    let restuple = client
        .query(
            r#"
            SELECT id, embedding <&> to_bm25query('documents_embedding_bm25', $1::text::bm25vector) AS rank
            FROM documents
            ORDER BY rank
            LIMIT 100"#,
            &[&query_vector],
        )
        .unwrap();
    let mut index_results: BTreeMap<OrderedFloat, BTreeSet<i32>> = BTreeMap::new();
    for row in restuple {
        let id: i32 = row.get::<_, i32>(0);
        let rank: f32 = row.get::<_, f32>(1);
        index_results
            .entry(OrderedFloat(rank))
            .or_default()
            .insert(id);
    }
    index_results.pop_last();

    client.execute("SET enable_seqscan = on", &[]).unwrap();
    client
        .execute("SET bm25_catalog.enable_index = off", &[])
        .unwrap();

    let restuple = client
        .query(
            r#"
            SELECT id, embedding <&> to_bm25query('documents_embedding_bm25', $1::text::bm25vector) AS rank
            FROM documents
            ORDER BY rank
            LIMIT 100"#,
            &[&query_vector_clone],
        )
        .unwrap();
    let mut seq_results: BTreeMap<OrderedFloat, BTreeSet<i32>> = BTreeMap::new();
    for row in restuple {
        let id: i32 = row.get::<_, i32>(0);
        let rank: f32 = row.get::<_, f32>(1);
        seq_results
            .entry(OrderedFloat(rank))
            .or_default()
            .insert(id);
    }
    seq_results.pop_last();

    let mut miss_cnt = 0;
    for (rank, seq_id) in &seq_results {
        let Some(index_id) = index_results.get(rank) else {
            miss_cnt += seq_id.len();
            continue;
        };
        for id in seq_id {
            if !index_id.contains(id) {
                miss_cnt += 1;
            }
        }
    }
    if miss_cnt > 10 {
        panic!(
            "Index and Seq results do not match\nindex_results: {:?}\nseq_results: {:?}",
            index_results, seq_results
        );
    }
}

fn fuzz_delete(client: &mut postgres::Client, rng: &mut impl RngExt) {
    let id = rng.random_range(1..INIT_DOCUMENTS) as i32;
    client
        .execute(r#"DELETE FROM documents WHERE id = $1"#, &[&id])
        .unwrap();
}

fn fuzz_vacuum(client: &mut postgres::Client, _rng: &mut impl RngExt) {
    client.execute(r#"VACUUM FULL documents"#, &[]).unwrap();
}

fn main() {
    let params = std::env::args().nth(1).unwrap();
    let mut client = postgres::Client::connect(&params, postgres::tls::NoTls).unwrap();
    client
        .execute(r#"SET search_path = "$user", public, bm25_catalog"#, &[])
        .unwrap();
    test(&mut client);
}
