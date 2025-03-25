use crate::datatype::Bm25VectorOutput;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_cast_array_to_bm25vector(
    array: pgrx::datum::Array<i32>,
    _typmod: i32,
    _explicit: bool,
) -> Bm25VectorOutput {
    Bm25VectorOutput::from_ids(array.iter().map(|x| x.unwrap().try_into().unwrap()))
}
