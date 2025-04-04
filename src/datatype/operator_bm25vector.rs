use crate::datatype::Bm25VectorInput;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _bm25catalog_bm25vector_operator_eq(lhs: Bm25VectorInput, rhs: Bm25VectorInput) -> bool {
    lhs.borrow() == rhs.borrow()
}

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _bm25catalog_bm25vector_operator_neq(lhs: Bm25VectorInput, rhs: Bm25VectorInput) -> bool {
    lhs.borrow() != rhs.borrow()
}
