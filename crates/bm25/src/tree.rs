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
use crate::tape::BackwardTapeWriter;
use crate::tuples::{Edge, NodeTuple, WithReader};
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};
use std::cmp::Ordering;

pub fn write<const BITS: usize, T: Copy + Ord, R: RelationWrite>(
    index: &R,
    elements: &[(T, (u32, u16))],
    serialize: impl Fn(T) -> [u8; BITS],
) -> (u32, u32, u32)
where
    R::Page: Page<Opaque = Opaque>,
{
    debug_assert!(elements.is_sorted_by_key(|(x, (_, _))| x));
    let mut tape = BackwardTapeWriter::<_, NodeTuple<BITS>>::create(index);
    let mut buffer = Vec::new();
    for chunk in elements.chunk_by(|(_, (x, _)), (_, (y, _))| x == y) {
        debug_assert!(chunk.is_sorted_by_key(|(_, (_, x))| x));
        let n = chunk.len();
        buffer.push((chunk[n - 1].0, chunk[n - 1].1.0));
    }
    let mut depth = 0_u32;
    while buffer.len() > 1 {
        depth += 1;
        let remain = core::mem::take(&mut buffer);
        let mut remain = remain.as_slice();
        while !remain.is_empty() {
            let w = {
                let w = NodeTuple::<BITS>::fit(tape.freespace())
                    .expect("implementation: a blank page cannot fit a single tuple");
                if w == 0 {
                    panic!("implementation: a blank page cannot fit a single tuple");
                }
                w
            };
            let (left, right) = remain.split_at(std::cmp::min(w, remain.len()));
            let edges = left
                .iter()
                .copied()
                .map(|(key, value)| Edge::new((serialize(key), value)))
                .collect::<Vec<_>>();
            let key = left.last().unwrap().0;
            let wptr = tape.tape_put(NodeTuple { edges }).0;
            buffer.push((key, wptr));
            remain = right;
            tape.tape_move();
        }
    }
    let root = buffer.first().map(|&(_, wptr)| wptr).unwrap_or(u32::MAX);
    let free = tape.into_head().id();
    (root, depth, free)
}

pub fn read<const BITS: usize, T: Copy + Ord, R: RelationRead>(
    index: &R,
    mut root: u32,
    depth: u32,
    key: T,
    deserialize: impl Fn([u8; BITS]) -> T,
) -> Option<(u32, u16)> {
    if root == u32::MAX {
        return None;
    }
    for _ in 0..depth {
        let node_guard = index.read(root);
        let node_bytes = node_guard.get(1).expect("data corruption");
        let node_tuple = NodeTuple::deserialize_ref(node_bytes);
        let edges = node_tuple.edges();
        let pos = edges.partition_point(|edge| deserialize(edge.into_inner().0) < key);
        if let Some(edge) = edges.get(pos) {
            root = edge.into_inner().1;
        } else {
            return None;
        }
    }
    let leaf_guard = index.read(root);
    let n = leaf_guard.len();
    let mut l = 1;
    let mut r = n + 1;
    while l < r {
        let i = u16::midpoint(l, r);
        let leaf_bytes = leaf_guard.get(i).expect("data corruption");
        let leaf_key = deserialize(std::array::from_fn(|i| leaf_bytes[i]));
        match Ord::cmp(&leaf_key, &key) {
            Ordering::Less => l = i + 1,
            Ordering::Equal => return Some((root, i)),
            Ordering::Greater => r = i,
        }
    }
    None
}
