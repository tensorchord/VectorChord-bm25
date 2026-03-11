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
use pgrx::datum::Internal;
use pgrx::pg_sys::Oid;

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_bm25vector_send(vector: Bm25VectorInput<'_>) -> Vec<u8> {
    let vector = vector.as_borrowed();
    let mut stream = Vec::<u8>::new();
    stream.extend(vector.len().to_be_bytes());
    for &c in vector.indexes() {
        stream.extend(c.to_be_bytes());
    }
    for &c in vector.values() {
        stream.extend(c.to_be_bytes());
    }
    stream
}

#[pgrx::pg_extern(immutable, strict, parallel_safe)]
fn _vchord_bm25_bm25vector_recv(mut internal: Internal, oid: Oid, typmod: i32) -> Bm25VectorOutput {
    let _ = (oid, typmod);
    let buf = unsafe { internal.get_mut::<pgrx::pg_sys::StringInfoData>().unwrap() };

    let len = {
        assert!(buf.cursor < i32::MAX - 4 && buf.cursor + 4 <= buf.len);
        let raw = unsafe { buf.data.add(buf.cursor as _).cast::<[u8; 4]>().read() };
        buf.cursor += 4;
        u32::from_be_bytes(raw)
    };
    let indexes = {
        let mut result = Vec::new();
        for _ in 0..len {
            result.push({
                assert!(buf.cursor < i32::MAX - 4 && buf.cursor + 4 <= buf.len);
                let raw = unsafe { buf.data.add(buf.cursor as _).cast::<[u8; 4]>().read() };
                buf.cursor += 4;
                u32::from_be_bytes(raw)
            });
        }
        result
    };
    let values = {
        let mut result = Vec::new();
        for _ in 0..len {
            result.push({
                assert!(buf.cursor < i32::MAX - 4 && buf.cursor + 4 <= buf.len);
                let raw = unsafe { buf.data.add(buf.cursor as _).cast::<[u8; 4]>().read() };
                buf.cursor += 4;
                u32::from_be_bytes(raw)
            });
        }
        result
    };

    if let Some(vector) = Bm25VectorBorrowed::new_checked(&indexes, &values) {
        Bm25VectorOutput::new(vector)
    } else {
        pgrx::error!("detect data corruption");
    }
}
