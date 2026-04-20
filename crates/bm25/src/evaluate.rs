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
use crate::vector::{Document, Query};
use crate::{Opaque, idf, tf, tree};
use index::relation::{Page, RelationRead};
use score::Score;
use std::convert::identity;

pub fn evaluate<R: RelationRead>(index: &R, document: &Document, query: &Query) -> Score
where
    R::Page: Page<Opaque = Opaque>,
{
    let document_length = document.length();

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

    let sum_of_document_lengths = jump_tuple.sum_of_document_lengths();
    let number_of_documents = jump_tuple.number_of_documents();
    let avgdl = sum_of_document_lengths as f64 / number_of_documents as f64;

    let mut cursor = 0_usize;

    let mut result = 0.0;
    for &key in query.iter() {
        let value = {
            while cursor < document.len() && document.as_slice()[cursor].key < key {
                cursor += 1;
            }
            if cursor < document.len() && document.as_slice()[cursor].key == key {
                document.as_slice()[cursor].value
            } else {
                continue;
            }
        };
        let Some(wptr_token) = tree::read(
            index,
            jump_tuple.root_tokens(),
            jump_tuple.depth_tokens(),
            key,
            identity,
        ) else {
            continue;
        };
        let token_guard = index.read(wptr_token.0);
        let token_bytes = token_guard.get(wptr_token.1).expect("data corruption");
        let token_tuple = TokenTuple::deserialize_ref(token_bytes);
        let term_frequency = value;
        let idf = idf(number_of_documents, token_tuple.number_of_documents());
        let tf = tf(document_length, term_frequency, k1, b, avgdl);
        result += idf * tf;
    }
    Score::from_f64(result)
}
