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

use crate::datatype::Bm25VectorInput;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _bm25catalog_bm25vector_operator_eq(lhs: Bm25VectorInput, rhs: Bm25VectorInput) -> bool {
    lhs.borrow() == rhs.borrow()
}

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _bm25catalog_bm25vector_operator_neq(lhs: Bm25VectorInput, rhs: Bm25VectorInput) -> bool {
    lhs.borrow() != rhs.borrow()
}
