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
use crate::tuples::Tuple;
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};
use std::collections::VecDeque;
use std::marker::PhantomData;

pub struct TapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
{
    head: R::WriteGuard<'a>,
    first: u32,
    index: &'a R,
    _phantom: PhantomData<fn(T) -> T>,
}

impl<'a, R, T> TapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
    R::Page: Page<Opaque = Opaque>,
{
    pub fn create(index: &'a R) -> Self {
        let head = index.alloc(Opaque {
            next: u32::MAX,
            flags: 0,
        });
        let first = head.id();
        Self {
            head,
            first,
            index,
            _phantom: PhantomData,
        }
    }
    pub fn from_guard(index: &'a R, head: R::WriteGuard<'a>) -> Self {
        let first = head.id();
        Self {
            head,
            first,
            index,
            _phantom: PhantomData,
        }
    }
    pub fn first(&self) -> u32 {
        self.first
    }
    pub fn freespace(&self) -> u16 {
        self.head.freespace()
    }
    pub fn tape_move(&mut self) {
        if self.head.len() == 0 {
            panic!("implementation: a clear page cannot accommodate a single tuple");
        }
        let next = self.index.alloc(Opaque {
            next: u32::MAX,
            flags: 0,
        });
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
            let next = self.index.alloc(Opaque {
                next: u32::MAX,
                flags: 0,
            });
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

pub struct BackwardTapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
{
    head: R::WriteGuard<'a>,
    index: &'a R,
    _phantom: PhantomData<fn(T) -> T>,
}

impl<'a, R, T> BackwardTapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
    R::Page: Page<Opaque = Opaque>,
{
    pub fn create(index: &'a R) -> Self {
        let head = index.alloc(Opaque {
            next: u32::MAX,
            flags: 0,
        });
        Self {
            head,
            index,
            _phantom: PhantomData,
        }
    }
    pub fn into_head(self) -> R::WriteGuard<'a> {
        self.head
    }
    pub fn freespace(&self) -> u16 {
        self.head.freespace()
    }
    pub fn tape_move(&mut self) {
        if self.head.len() == 0 {
            panic!("implementation: a clear page cannot accommodate a single tuple");
        }
        self.head = self.index.alloc(Opaque {
            next: self.head.id(),
            flags: 0,
        });
    }
}

impl<'a, R, T> BackwardTapeWriter<'a, R, T>
where
    R: RelationWrite + 'a,
    R::Page: Page<Opaque = Opaque>,
    T: Tuple,
{
    pub fn tape_put(&mut self, x: T) -> (u32, u16) {
        let bytes = T::serialize(&x);
        if let Some(i) = self.head.alloc(&bytes) {
            (self.head.id(), i)
        } else {
            panic!("implementation: a free page cannot accommodate a single tuple")
        }
    }
}

pub struct TapeReader<T> {
    buffer: VecDeque<T>,
    next: u32,
    deserialize: fn(&[u8]) -> T,
}

impl<T> TapeReader<T> {
    pub fn new(first: u32, deserialize: fn(&[u8]) -> T) -> Self {
        Self {
            buffer: VecDeque::new(),
            next: first,
            deserialize,
        }
    }
    pub fn next<R: RelationRead>(&mut self, index: &R) -> Option<T>
    where
        R::Page: Page<Opaque = Opaque>,
    {
        while self.buffer.is_empty() && self.next != u32::MAX {
            self.next = {
                let guard = index.read(self.next);
                for j in 1..=guard.len() {
                    let bytes = guard.get(j).expect("data corruption");
                    let tuple = (self.deserialize)(bytes);
                    self.buffer.push_back(tuple);
                }
                guard.get_opaque().next
            };
        }
        self.buffer.pop_front()
    }
}

pub struct TruncatedTapeReader<T> {
    buffer: VecDeque<T>,
    next: u32,
    deserialize: fn(&[u8]) -> T,
    count: u32,
}

impl<T> TruncatedTapeReader<T> {
    pub fn new<R: RelationRead>(
        index: &R,
        first: (u32, u16),
        deserialize: fn(&[u8]) -> T,
        mut count: u32,
    ) -> Self
    where
        R::Page: Page<Opaque = Opaque>,
    {
        let mut buffer = VecDeque::new();
        let next = {
            let guard = index.read(first.0);
            for j in first.1..=guard.len() {
                if count == 0 {
                    break;
                }
                let bytes = guard.get(j).expect("data corruption");
                let tuple = deserialize(bytes);
                buffer.push_back(tuple);
                count -= 1;
            }
            guard.get_opaque().next
        };
        Self {
            buffer,
            next,
            deserialize,
            count,
        }
    }
    pub fn next<R: RelationRead>(&mut self, index: &R) -> Option<T>
    where
        R::Page: Page<Opaque = Opaque>,
    {
        while self.count != 0 && self.buffer.is_empty() && self.next != u32::MAX {
            self.next = {
                let guard = index.read(self.next);
                for j in 1..=guard.len() {
                    if self.count == 0 {
                        break;
                    }
                    let bytes = guard.get(j).expect("data corruption");
                    let tuple = (self.deserialize)(bytes);
                    self.buffer.push_back(tuple);
                    self.count -= 1;
                }
                guard.get_opaque().next
            };
        }
        self.buffer.pop_front()
    }
}
