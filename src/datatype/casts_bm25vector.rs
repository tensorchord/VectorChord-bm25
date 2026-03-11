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

use crate::datatype::memory_bm25vector::{Bm25VectorInput, Bm25VectorOutput};
use bm25::vector::Bm25VectorBorrowed;
use pgrx::datum::Array;
use std::iter::zip;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_bm25vector_cast_intarray_bm25vector(input: Array<'_, i32>) -> Bm25VectorOutput {
    let mut input = input.as_slice().expect("input contains nulls").to_vec();
    input.sort_unstable();
    let mut indexes = Vec::new();
    let mut values = Vec::new();
    for x in input {
        if indexes.last().copied() == Some(x as u32) {
            *values.last_mut().unwrap() += 1;
        } else {
            indexes.push(x as u32);
            values.push(1_u32);
        }
    }
    let vector = Bm25VectorBorrowed::new(&indexes, &values);
    Bm25VectorOutput::new(vector)
}

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_bm25vector_cast_bm25vector_intarray(input: Bm25VectorInput<'_>) -> Vec<i32> {
    let vector = input.as_borrowed();
    zip(vector.indexes(), vector.values())
        .flat_map(|(&index, &value)| std::iter::repeat_n(index as i32, value as usize))
        .collect::<Vec<i32>>()
}
