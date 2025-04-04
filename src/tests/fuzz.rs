use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

use crate::datatype::Bm25VectorOutput;
use pgrx::{spi::SpiClient, Spi};
use rand::{seq::IndexedRandom, Rng, SeedableRng};

#[allow(unused)]
#[derive(Debug, Clone, Copy)]
enum Operation {
    Insert,
    Select,
    Delete,
    Vacuum,
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;

    #[pgrx::pg_test]
    fn test_random_operations() {
        Spi::connect_mut(|client| {
            test_random_operations_inner(client);
        });
    }
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

fn test_random_operations_inner(client: &mut SpiClient<'_>) {
    let seed = rand::rng().random_range(0..u64::MAX);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    println!("Seed: {}", seed); // for reproducibility

    client
        .update(
            r#"
        CREATE TABLE documents (
            id SERIAL PRIMARY KEY,
            embedding bm25vector
        );
        "#,
            None,
            &[],
        )
        .unwrap();

    // Insert initial documents
    for _ in 0..INIT_DOCUMENTS {
        let bm25vector = random_bm25vector(&mut rng);
        client
            .update(
                r#"INSERT INTO documents (embedding) VALUES ($1);"#,
                None,
                &[bm25vector.into()],
            )
            .unwrap();
    }

    // Create index
    client
        .update(
            r#"CREATE INDEX documents_embedding_bm25 ON documents USING bm25 (embedding bm25_ops);"#,
            None,
            &[],
        )
        .unwrap();

    client
        .update(
            r#"SET bm25_catalog.segment_growing_max_page_size = 1;"#,
            None,
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
}

fn random_bm25vector(rng: &mut impl Rng) -> Bm25VectorOutput {
    let ids = (0..DOCUMENT_LEN).map(|_| rng.random_range(0..DOCUMENT_MAX_TOKEN));
    Bm25VectorOutput::from_ids(ids)
}

fn fuzz_insert(client: &mut SpiClient<'_>, rng: &mut impl Rng) {
    let bm25vector = random_bm25vector(rng);
    client
        .update(
            r#"INSERT INTO documents (embedding) VALUES ($1);"#,
            None,
            &[bm25vector.into()],
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

fn fuzz_select(client: &mut SpiClient<'_>, rng: &mut impl Rng) {
    let query_vector = random_bm25vector(rng);
    let query_vector_clone = Bm25VectorOutput::new(query_vector.borrow());

    client
        .update("SET enable_seqscan = off", None, &[])
        .unwrap();
    client
        .update("SET bm25_catalog.enable_index = on", None, &[])
        .unwrap();
    client
        .update("SET bm25_catalog.bm25_limit = 200", None, &[])
        .unwrap();

    let restuple = client
        .select(
            r#"
            SELECT id, embedding <&> to_bm25query('documents_embedding_bm25', $1) AS rank
            FROM documents
            ORDER BY rank
            LIMIT 100"#,
            None,
            &[query_vector.into()],
        )
        .unwrap();
    let mut index_results: BTreeMap<OrderedFloat, BTreeSet<i32>> = BTreeMap::new();
    for row in restuple {
        let id: i32 = row.get(1).unwrap().unwrap();
        let rank: f32 = row.get(2).unwrap().unwrap();
        index_results
            .entry(OrderedFloat(rank))
            .or_default()
            .insert(id);
    }
    index_results.pop_last();

    client.update("SET enable_seqscan = on", None, &[]).unwrap();
    client
        .update("SET bm25_catalog.enable_index = off", None, &[])
        .unwrap();

    let restuple = client
        .select(
            r#"
            SELECT id, embedding <&> to_bm25query('documents_embedding_bm25', $1) AS rank
            FROM documents
            ORDER BY rank
            LIMIT 100"#,
            None,
            &[query_vector_clone.into()],
        )
        .unwrap();
    let mut seq_results: BTreeMap<OrderedFloat, BTreeSet<i32>> = BTreeMap::new();
    for row in restuple {
        let id: i32 = row.get(1).unwrap().unwrap();
        let rank: f32 = row.get(2).unwrap().unwrap();
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

fn fuzz_delete(client: &mut SpiClient<'_>, rng: &mut impl Rng) {
    let id = rng.random_range(1..INIT_DOCUMENTS) as i32;
    client
        .update(r#"DELETE FROM documents WHERE id = $1"#, None, &[id.into()])
        .unwrap();
}

fn fuzz_vacuum(client: &mut SpiClient<'_>, _rng: &mut impl Rng) {
    client
        .update(r#"VACUUM FULL documents"#, None, &[])
        .unwrap();
}
