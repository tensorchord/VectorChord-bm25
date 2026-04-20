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

use crate::Opaque;
use crate::tape::TapeWriter;
use crate::tuples::{JumpTuple, MetaTuple, VectorTuple, WithReader};
use crate::vector::Document;
use index::relation::{Page, RelationRead, RelationWrite};
use index::tuples::Bool;

pub fn insert<R: RelationRead + RelationWrite>(index: &R, document: &Document, payload: [u16; 3])
where
    R::Page: Page<Opaque = Opaque>,
{
    let document_length = document.length();

    let meta_guard = index.read(0);
    let meta_bytes = meta_guard.get(1).expect("data corruption");
    let meta_tuple = MetaTuple::deserialize_ref(meta_bytes);
    let ptr_jump = meta_tuple.ptr_jump();
    drop(meta_guard);

    let jump_guard = index.read(ptr_jump);
    let jump_bytes = jump_guard.get(1).expect("data corruption");
    let jump_tuple = JumpTuple::deserialize_ref(jump_bytes);

    let first = jump_tuple.ptr_vectors();
    let mut current = first;
    let head = loop {
        let read = index.read(current);
        if read.get_opaque().next == u32::MAX {
            drop(read);
            let write = index.write(current);
            if write.get_opaque().next == u32::MAX {
                break write;
            }
            current = write.get_opaque().next;
        } else {
            current = read.get_opaque().next;
        }
    };

    let mut tape = TapeWriter::from_guard(index, head);
    tape.push(VectorTuple::_2 {});
    let mut remain = document.as_slice();
    loop {
        let freespace = tape.freespace();
        if VectorTuple::estimate_size_0(remain.len()) <= freespace as usize {
            tape.tape_put(VectorTuple::_0 {
                deleted: Bool::FALSE,
                payload,
                length: document_length,
                elements: remain.to_vec(),
            });
            break;
        }
        if let Some(w) = VectorTuple::fit_1(freespace) {
            let (left, right) = remain.split_at(std::cmp::min(w, remain.len()));
            tape.tape_put(VectorTuple::_1 {
                elements: left.to_vec(),
            });
            remain = right;
        } else {
            tape.tape_move();
        }
    }
}
