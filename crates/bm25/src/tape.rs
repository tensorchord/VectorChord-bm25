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
use crate::tuples::*;
use index::relation::{Page, PageGuard, RelationWrite};
use std::marker::PhantomData;

pub struct TapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
{
    head: R::WriteGuard<'a>,
    first: u32,
    index: &'a R,
    tracking_freespace: bool,
    _phantom: PhantomData<fn(T) -> T>,
}

impl<'a, R, T> TapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
    R::Page: Page<Opaque = Opaque>,
{
    pub fn create(index: &'a R, tracking_freespace: bool) -> Self {
        let head = index.extend(
            Opaque {
                next: u32::MAX,
                _padding_0: Default::default(),
            },
            tracking_freespace,
        );
        let first = head.id();
        Self {
            head,
            first,
            index,
            tracking_freespace,
            _phantom: PhantomData,
        }
    }
    pub fn first(&self) -> u32 {
        self.first
    }
    #[expect(dead_code)]
    pub fn freespace(&self) -> u16 {
        self.head.freespace()
    }
    pub fn tape_move(&mut self) {
        if self.head.len() == 0 {
            panic!("implementation: a clear page cannot accommodate a single tuple");
        }
        let next = self.index.extend(
            Opaque {
                next: u32::MAX,
                _padding_0: Default::default(),
            },
            self.tracking_freespace,
        );
        self.head.get_opaque_mut().next = next.id();
        self.head = next;
    }
}

impl<'a, R, T> TapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
    R::Page: Page<Opaque = Opaque>,
    T: Tuple,
{
    pub fn push(&mut self, x: T) -> (u32, u16) {
        let bytes = T::serialize(&x);
        if let Some(i) = self.head.alloc(&bytes) {
            (self.head.id(), i)
        } else {
            let next = self.index.extend(
                Opaque {
                    next: u32::MAX,
                    _padding_0: Default::default(),
                },
                self.tracking_freespace,
            );
            self.head.get_opaque_mut().next = next.id();
            self.head = next;
            if let Some(i) = self.head.alloc(&bytes) {
                (self.head.id(), i)
            } else {
                panic!("implementation: a free page cannot accommodate a single tuple")
            }
        }
    }
    pub fn tape_put(&mut self, x: T) -> (u32, u16) {
        let bytes = T::serialize(&x);
        if let Some(i) = self.head.alloc(&bytes) {
            (self.head.id(), i)
        } else {
            panic!("implementation: a free page cannot accommodate a single tuple")
        }
    }
}
