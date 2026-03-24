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
// Copyright (c) 2025-2026 TensorChord Inc.

use std::collections::BTreeMap;
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
const FUZZ_OPERATIONS: [Operation; 1] = [
    // Operation::Insert,
    Operation::Select,
    // Operation::Delete,
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
        ) WITH (autovacuum_enabled = off);
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

fn fuzz_select(client: &mut postgres::Client, rng: &mut impl RngExt) {
    let query_vector = random_bm25vector(rng);

    client.execute("SET enable_seqscan = off", &[]).unwrap();
    client.execute("SET bm25.enable_scan = on", &[]).unwrap();
    client.execute("SET \"bm25.limit\" = 200", &[]).unwrap();

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
    let mut index_results: Vec<i32> = Vec::new();
    let mut index_result_ranks: Vec<f64> = Vec::new();
    for row in restuple {
        index_results.push(row.get::<_, i32>(0));
        index_result_ranks.push(row.get::<_, f64>(1));
    }

    client.execute("SET enable_seqscan = on", &[]).unwrap();
    client.execute("SET bm25.enable_scan = off", &[]).unwrap();

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
    let mut seq_results: Vec<i32> = Vec::new();
    let mut seq_result_ranks: Vec<f64> = Vec::new();
    for row in restuple {
        seq_results.push(row.get::<_, i32>(0));
        seq_result_ranks.push(row.get::<_, f64>(1));
    }

    if distance(&index_results, &seq_results) > 10 {
        println!("Query = {query_vector}");
        println!("Index =");
        for (id, rank) in std::iter::zip(index_results, index_result_ranks) {
            println!("  | {id} | {rank}");
        }
        println!("Seq =");
        for (id, rank) in std::iter::zip(seq_results, seq_result_ranks) {
            println!("  | {id} | {rank}");
        }
        panic!("Index and Seq results do not match");
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

fn distance<T: Eq>(a: &[T], b: &[T]) -> usize {
    use std::cmp::min;

    let (a, b) = if a.len() <= b.len() { (a, b) } else { (b, a) };

    if a.is_empty() {
        return b.len();
    }

    let mut f = (0..a.len() + 1).collect::<Vec<usize>>();
    let mut last;

    for (j, y) in b.iter().enumerate() {
        (last, f[0]) = (f[0], j + 1);
        for (i, x) in a.iter().enumerate() {
            let value = min(f[i + 1] + 1, min(f[i] + 1, last + (x != y) as usize));
            (last, f[i + 1]) = (f[i + 1], value);
        }
    }

    f[a.len()]
}

fn main() {
    let params = std::env::args().nth(1).unwrap();
    let mut client = postgres::Client::connect(&params, postgres::tls::NoTls).unwrap();
    test(&mut client);
}
