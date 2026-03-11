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
use pgrx::pg_sys::Oid;
use std::ffi::{CStr, CString};
use std::iter::zip;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_bm25vector_in(input: &CStr, oid: Oid, typmod: i32) -> Bm25VectorOutput {
    let _ = (oid, typmod);
    let mut input = input.to_bytes().iter();
    let mut indexes = Vec::<u32>::new();
    let mut values = Vec::<u32>::new();
    {
        loop {
            let Some(c) = input.next().copied() else {
                pgrx::error!("incorrect vector")
            };
            match c {
                b' ' => (),
                b'{' => break,
                _ => pgrx::error!("incorrect vector"),
            }
        }
    }
    {
        let mut s = Result::<String, String>::Ok("".to_string());
        loop {
            let Some(c) = input.next().copied() else {
                pgrx::error!("incorrect vector")
            };
            s = match (s, c) {
                (s, b' ') => s,
                (Ok(s), c @ (b'0'..=b'9')) => {
                    let mut x = s;
                    x.push(c as char);
                    Ok(x)
                }
                (Err(s), c @ (b'0'..=b'9')) => {
                    let mut x = s;
                    x.push(c as char);
                    Err(x)
                }
                (Ok(s), b':') => {
                    indexes.push(s.parse().expect("failed to parse number"));
                    Err("".to_string())
                }
                (Err(s), b',') => {
                    values.push(s.parse().expect("failed to parse number"));
                    Ok("".to_string())
                }
                (Ok(s), b'}') if s.is_empty() => {
                    break;
                }
                (Err(s), b'}') => {
                    values.push(s.parse().expect("failed to parse number"));
                    break;
                }
                _ => pgrx::error!("incorrect vector"),
            };
        }
    }
    if let Some(x) = Bm25VectorBorrowed::new_checked(&indexes, &values) {
        Bm25VectorOutput::new(x)
    } else {
        pgrx::error!("incorrect vector");
    }
}

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_bm25vector_out(vector: Bm25VectorInput<'_>) -> CString {
    let vector = vector.as_borrowed();
    let mut buffer = String::new();
    buffer.push('{');
    let mut iterator = zip(vector.indexes(), vector.values());
    if let Some((key, value)) = iterator.next() {
        buffer.push_str(format!("{key}:{value}").as_str());
    }
    for (key, value) in iterator {
        buffer.push_str(format!(", {key}:{value}").as_str());
    }
    buffer.push('}');
    CString::new(buffer).unwrap()
}
