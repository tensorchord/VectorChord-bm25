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

use crate::datatype::Bm25VectorOutput;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_cast_array_to_bm25vector(
    array: pgrx::datum::Array<i32>,
    _typmod: i32,
    _explicit: bool,
) -> Bm25VectorOutput {
    Bm25VectorOutput::from_ids(array.iter().map(|x| x.unwrap().try_into().unwrap()))
}
