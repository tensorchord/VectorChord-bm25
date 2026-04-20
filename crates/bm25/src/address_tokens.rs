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

use crate::tape::BackwardTapeWriter;
use crate::tuples::{AddressTokensTuple, Edge, WithReader};
use crate::{Opaque, WIDTH};
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};
use std::cmp::Ordering;

// The `write` function imposes strict constraints on its input.
// * All tuples must be evenly distributed across either `n + 1` pages or `n` pages.
// * On each page, tuples must be contiguous and ordered, starting from `1`.
// * IDs must be ordered on all pages.

pub fn write<R: RelationWrite>(index: &R, elements: &[([u8; WIDTH], (u32, u16))]) -> (u32, u32, u32)
where
    R::Page: Page<Opaque = Opaque>,
{
    assert!(elements.is_sorted_by(|(l, (_, _)), (r, (_, _))| l < r));
    let mut tape = BackwardTapeWriter::<_, AddressTokensTuple>::create(index);
    let width_1 = AddressTokensTuple::fit(tape.freespace())
        .expect("implementation: a blank page cannot fit a single tuple") as u16;
    let mut buffer = Vec::new();
    for chunk in elements.chunk_by(|(_, (l, _)), (_, (r, _))| l == r) {
        for (index, (_, (_, element))) in chunk.iter().copied().enumerate() {
            if index + 1 != element as usize {
                panic!("tuples are not continuous");
            }
        }
        let n = chunk.len();
        buffer.push(Edge::new((chunk[n - 1].0, chunk[n - 1].1.0)));
    }
    let mut depth = 0_u32;
    while buffer.len() > 1 {
        depth += 1;
        for chunk in core::mem::take(&mut buffer).chunks(width_1 as usize) {
            let tuple = AddressTokensTuple {
                edges: chunk.to_vec(),
            };
            let key = chunk.last().expect("impossible").into_inner().0;
            buffer.push(Edge::new((key, tape.tape_put(tuple).0)));
            tape.tape_move();
        }
    }
    let start = buffer.first().map(|e| e.into_inner().1).unwrap_or(u32::MAX);
    let free = tape.into_head().id();
    (depth, start, free)
}

pub fn read<R: RelationRead>(
    index: &R,
    depth: u32,
    start: u32,
    token_id: [u8; WIDTH],
) -> Option<(R::ReadGuard<'_>, u16)> {
    if start == u32::MAX {
        return None;
    }
    let mut id = start;
    for _ in 0..depth {
        let address_guard = index.read(id);
        let address_bytes = address_guard.get(1).expect("data corruption");
        let address_tuple = AddressTokensTuple::deserialize_ref(address_bytes);
        let edges = address_tuple.edges();
        let pos = edges.partition_point(|edge| edge.into_inner().0 < token_id);
        if let Some(edge) = edges.get(pos) {
            id = edge.into_inner().1;
        } else {
            return None;
        }
    }
    let token_guard = index.read(id);
    let n = token_guard.len();
    let mut l = 1;
    let mut r = n + 1;
    while l < r {
        let i = u16::midpoint(l, r);
        let token_bytes = token_guard.get(i).expect("data corruption");
        let key = std::array::from_fn(|i| token_bytes[i]);
        match Ord::cmp(&key, &token_id) {
            Ordering::Less => l = i + 1,
            Ordering::Equal => return Some((token_guard, i)),
            Ordering::Greater => r = i,
        }
    }
    None
}
