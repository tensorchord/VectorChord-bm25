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

use crate::Opaque;
use crate::tape::TapeWriter;
use crate::tuples::{Index0, Index1, IndexTuple, IndexTupleReader, Pointer, WithReader};
use index::relation::{Page, RelationRead, RelationWrite};

pub fn write<R: RelationWrite>(index: &R, guide: &[(u32, (u32, u16))]) -> u32
where
    R::Page: Page<Opaque = Opaque>,
{
    let mut tape = TapeWriter::<_, IndexTuple>::create(index, false);
    let mut buffer = Vec::new();
    for chunk in guide.chunks(666) {
        let key = chunk.last().unwrap().0;
        let pairs = chunk
            .iter()
            .map(|&(key, val)| Index0::new((key, Pointer::new(val))))
            .collect::<Vec<_>>();
        let id = tape.tape_put(IndexTuple::_0 { pairs }).0;
        tape.tape_move();
        buffer.push((key, id));
    }
    while buffer.len() > 1 {
        let guide = core::mem::take(&mut buffer);
        for block in guide.chunks(1000) {
            let key = block.last().unwrap().0;
            let pairs = block
                .iter()
                .map(|&(key, val)| Index1::new((key, val)))
                .collect::<Vec<_>>();
            let id = tape.tape_put(IndexTuple::_1 { pairs }).0;
            tape.tape_move();
            buffer.push((key, id));
        }
    }
    if !buffer.is_empty() {
        buffer[0].1
    } else {
        tape.tape_put(IndexTuple::_1 { pairs: Vec::new() }).0
    }
}

pub fn read<R: RelationRead>(index: &R, mut id: u32, key: u32) -> Option<(u32, u16)> {
    loop {
        assert_ne!(id, u32::MAX);
        let index_guard = index.read(id);
        let index_bytes = index_guard.get(1).expect("data corruption");
        let index_tuple = IndexTuple::deserialize_ref(index_bytes);
        match index_tuple {
            IndexTupleReader::_0(tuple) => {
                let pairs = tuple.pairs();
                let pos = pairs.binary_search_by_key(&key, |pair| pair.into_inner().0);
                if let Ok(pos) = pos {
                    return Some(pairs[pos].into_inner().1.into_inner());
                } else {
                    return None;
                }
            }
            IndexTupleReader::_1(tuple) => {
                let pairs = tuple.pairs();
                let pos = pairs.partition_point(|pair| pair.into_inner().0 < key);
                if let Some(pair) = pairs.get(pos) {
                    id = pair.into_inner().1;
                } else {
                    return None;
                }
            }
        }
    }
}
