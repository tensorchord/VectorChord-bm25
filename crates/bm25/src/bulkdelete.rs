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
use crate::tuples::*;
use index::relation::{Page, RelationRead, RelationWrite};
use index::tuples::Bool;

pub fn bulkdelete<R: RelationRead + RelationWrite>(
    index: &R,
    check: impl Fn(),
    callback: impl Fn([u16; 3]) -> bool,
) where
    R::Page: Page<Opaque = Opaque>,
{
    let meta_guard = index.read(0);
    let meta_bytes = meta_guard.get(1).expect("data corruption");
    let meta_tuple = MetaTuple::deserialize_ref(meta_bytes);
    let ptr_lock = meta_tuple.ptr_lock();
    let ptr_jump = meta_tuple.ptr_jump();
    drop(meta_guard);

    let _lock_guard = index.read(ptr_lock);

    let jump_guard = index.read(ptr_jump);
    let jump_bytes = jump_guard.get(1).expect("data corruption");
    let jump_tuple = JumpTuple::deserialize_ref(jump_bytes);

    {
        let first = jump_tuple.ptr_vectors();
        assert!(first != u32::MAX);
        let mut current = first;
        while current != u32::MAX {
            check();
            let read = index.read(current);
            let flag = 'flag: {
                for i in 1..=read.len() {
                    let vector_bytes = read.get(i).expect("data corruption");
                    let vector_tuple = VectorTuple::deserialize_ref(vector_bytes);
                    if let VectorTupleReader::_0(vector_tuple) = vector_tuple {
                        if !bool::from(vector_tuple.deleted()) && callback(vector_tuple.payload()) {
                            break 'flag true;
                        }
                    }
                }
                false
            };
            if flag {
                drop(read);
                let mut write = index.write(current);
                for i in 1..=write.len() {
                    let vector_bytes = write.get_mut(i).expect("data corruption");
                    let vector_tuple = VectorTuple::deserialize_mut(vector_bytes);
                    if let VectorTupleWriter::_0(mut vector_tuple) = vector_tuple {
                        if !bool::from(*vector_tuple.deleted()) && callback(*vector_tuple.payload())
                        {
                            *vector_tuple.deleted() = Bool::TRUE;
                        }
                    }
                }
                current = write.get_opaque().next;
            } else {
                current = read.get_opaque().next;
            }
        }
    }

    {
        let first = jump_tuple.ptr_documents();
        assert!(first != u32::MAX);
        let mut current = first;
        while current != u32::MAX {
            check();
            let read = index.read(current);
            let flag = 'flag: {
                for i in 1..=read.len() {
                    let bytes = read.get(i).expect("data corruption");
                    let tuple = DocumentTuple::deserialize_ref(bytes);
                    if !bool::from(tuple.deleted()) && callback(tuple.payload()) {
                        break 'flag true;
                    }
                }
                false
            };
            if flag {
                drop(read);
                let mut write = index.write(current);
                for i in 1..=write.len() {
                    let bytes = write.get_mut(i).expect("data corruption");
                    let mut tuple = DocumentTuple::deserialize_mut(bytes);
                    if !bool::from(*tuple.deleted()) && callback(*tuple.payload()) {
                        *tuple.deleted() = Bool::TRUE;
                    }
                }
                current = write.get_opaque().next;
            } else {
                current = read.get_opaque().next;
            }
        }
    }
}
