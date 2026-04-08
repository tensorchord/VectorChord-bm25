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

use crate::tuples::{JumpTuple, MetaTuple, TokenTuple, WithReader};
use crate::vector::Bm25VectorBorrowed;
use crate::{Opaque, idf, tf, tree};
use index::relation::{Page, RelationRead};
use score::Score;

pub fn evaluate<R: RelationRead>(
    index: &R,
    document: Bm25VectorBorrowed<'_>,
    query: Bm25VectorBorrowed<'_>,
) -> Score
where
    R::Page: Page<Opaque = Opaque>,
{
    let meta_guard = index.read(0);
    let meta_bytes = meta_guard.get(1).expect("data corruption");
    let meta_tuple = MetaTuple::deserialize_ref(meta_bytes);
    let k1 = meta_tuple.k1();
    let b = meta_tuple.b();
    let ptr_jump = meta_tuple.ptr_jump();
    drop(meta_guard);

    let jump_guard = index.read(ptr_jump);
    let jump_bytes = jump_guard.get(1).expect("data corruption");
    let jump_tuple = JumpTuple::deserialize_ref(jump_bytes);

    let document_length = document.norm();

    let sum_of_document_lengths = jump_tuple.sum_of_document_lengths();
    let number_of_documents = jump_tuple.number_of_documents();
    let avgdl = sum_of_document_lengths as f64 / number_of_documents as f64;

    let mut result = 0.0;
    for (key, value) in meet(document, query) {
        let Some(token) = tree::read(
            index,
            jump_tuple.root_tokens(),
            jump_tuple.depth_tokens(),
            key,
        ) else {
            continue;
        };
        let token_guard = index.read(token.0);
        let token_bytes = token_guard.get(token.1).expect("data corruption");
        let token_tuple = TokenTuple::deserialize_ref(token_bytes);
        let token_number_of_documents = token_tuple.number_of_documents();
        let idf = idf(number_of_documents, token_number_of_documents);
        let tf = tf(k1, b, avgdl, document_length, value);
        result += idf * tf;
    }
    Score::from_f64(result)
}

fn meet(
    document: Bm25VectorBorrowed<'_>,
    query: Bm25VectorBorrowed<'_>,
) -> impl Iterator<Item = (u32, u32)> {
    let (indexes, values, filter) = (document.indexes(), document.values(), query.indexes());
    let (mut i, mut j) = (0_usize, 0_usize);
    core::iter::from_fn(move || {
        while i < indexes.len() && j < filter.len() {
            let cmp = Ord::cmp(&indexes[i], &filter[j]);
            let next = (i + cmp.is_le() as usize, j + cmp.is_ge() as usize);
            let result = (indexes[i], values[i]);
            (i, j) = next;
            if cmp.is_eq() {
                return Some(result);
            }
        }
        None
    })
}
