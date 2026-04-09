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

use crate::datatype::memory_bm25vector::Bm25VectorInput;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
pub fn _vchord_bm25_bm25vector_operator_eq(lhs: Bm25VectorInput, rhs: Bm25VectorInput) -> bool {
    let (lhs, rhs) = (lhs.as_borrowed(), rhs.as_borrowed());
    let lhs = (lhs.indexes(), lhs.values());
    let rhs = (rhs.indexes(), rhs.values());
    lhs == rhs
}

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
pub fn _vchord_bm25_bm25vector_operator_neq(lhs: Bm25VectorInput, rhs: Bm25VectorInput) -> bool {
    let (lhs, rhs) = (lhs.as_borrowed(), rhs.as_borrowed());
    let lhs = (lhs.indexes(), lhs.values());
    let rhs = (rhs.indexes(), rhs.values());
    lhs != rhs
}
