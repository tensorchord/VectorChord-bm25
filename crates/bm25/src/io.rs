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

use crate::segment::{Mapping, Record, Segment};
use crate::vector::{Document, Element};
use always_equal::AlwaysEqual;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::iter::Peekable;
use std::path::Path;
use zerocopy::{FromZeros, IntoBytes};

pub fn handle_io_error<T>(result: std::io::Result<T>) -> T {
    result.expect("IO error occurred during external sorting")
}

fn not_found_is_okay(result: std::io::Result<File>) -> Option<File> {
    if let Err(e) = result.as_ref() {
        if e.kind() == std::io::ErrorKind::NotFound {
            return None;
        }
    }
    Some(handle_io_error(result))
}

pub struct RecordsWriter {
    file: BufWriter<File>,
    len: usize,
}

impl RecordsWriter {
    pub fn create(file: File) -> Self {
        Self {
            file: BufWriter::with_capacity(64 * 1024, file),
            len: 0,
        }
    }
    pub fn write(&mut self, element: Record) {
        handle_io_error(self.file.write_all(element.as_bytes()));
        self.len = self.len.checked_add(1).expect("too many documents");
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn flush(&mut self) {
        handle_io_error(self.file.flush());
    }
}

pub struct MappingsWriter {
    file: Box<dyn FnMut() -> File>,
    capacity: usize,
    buffer: Vec<Mapping>,
}

impl MappingsWriter {
    pub fn create(file: impl FnMut() -> File + 'static, memory_limit: usize) -> Self {
        let capacity = (memory_limit / size_of::<Mapping>()).max(1);
        Self {
            file: Box::new(file),
            capacity,
            buffer: Vec::with_capacity(capacity),
        }
    }
    pub fn write(&mut self, element: Mapping) {
        if self.buffer.len() >= self.capacity {
            self.flush();
        }
        self.buffer.push(element);
    }
    pub fn flush(&mut self) {
        if !self.buffer.is_empty() {
            self.buffer.sort_unstable();
            let bytes = self.buffer.as_slice().as_bytes();
            handle_io_error((self.file)().write_all(bytes));
            self.buffer.clear();
        }
    }
}

pub struct RecordsReader {
    stream: Peekable<Box<dyn Iterator<Item = BufReader<File>>>>,
}

impl RecordsReader {
    pub fn open(iter: impl Iterator<Item = File> + 'static) -> Self {
        let stream: Box<dyn Iterator<Item = _>> =
            Box::new(iter.map(|file| BufReader::with_capacity(64 * 1024, file)));
        Self {
            stream: stream.peekable(),
        }
    }
}

impl Iterator for RecordsReader {
    type Item = Record;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let file = self.stream.peek_mut()?;
            if handle_io_error(file.fill_buf()).is_empty() {
                let _ = self.stream.next();
                continue;
            }
            let mut element = Record::new_zeroed();
            handle_io_error(file.read_exact(element.as_mut_bytes()));
            return Some(element);
        }
    }
}

pub struct MappingsReader {
    collection: BinaryHeap<(Reverse<Mapping>, AlwaysEqual<(u32, BufReader<File>)>)>,
}

impl MappingsReader {
    pub fn open(iter: impl Iterator<Item = (u32, File)>) -> Self {
        let mut collection = Vec::new();
        for (offset, file) in iter {
            let mut reader = BufReader::with_capacity(64 * 1024, file);
            if !handle_io_error(reader.fill_buf()).is_empty() {
                let mut element = Mapping::new_zeroed();
                handle_io_error(reader.read_exact(element.as_mut_bytes()));
                element.1 += offset;
                collection.push((Reverse(element), AlwaysEqual((offset, reader))));
            }
        }
        Self {
            collection: BinaryHeap::from(collection),
        }
    }
}

impl Iterator for MappingsReader {
    type Item = Mapping;

    fn next(&mut self) -> Option<Self::Item> {
        let (Reverse(result), AlwaysEqual((offset, mut reader))) = self.collection.pop()?;
        if !handle_io_error(reader.fill_buf()).is_empty() {
            let collection = &mut self.collection;
            let mut element = Mapping::new_zeroed();
            handle_io_error(reader.read_exact(element.as_mut_bytes()));
            element.1 += offset;
            collection.push((Reverse(element), AlwaysEqual((offset, reader))));
        }
        Some(result)
    }
}

pub fn records_writer(dir: impl AsRef<Path>, code: u32) -> RecordsWriter {
    let dir = dir.as_ref();
    let filename = format!("records.{code:08x}");
    let file = handle_io_error(File::create_new(dir.join(filename)));
    RecordsWriter::create(file)
}

pub fn mappings_writer(dir: impl AsRef<Path>, code: u32) -> MappingsWriter {
    let dir = dir.as_ref().to_path_buf();
    let mut number = 0_u32;
    let file = move || {
        let filename = format!("mappings.{code:08x}.{number:08x}");
        number = number.checked_add(1).expect("written too many files");
        handle_io_error(File::create_new(dir.join(filename)))
    };
    MappingsWriter::create(file, 64 * 1024 * 1024)
}

pub fn write(
    records_writer: &mut RecordsWriter,
    mappings_writer: &mut MappingsWriter,
    document: &Document,
    payload: [u16; 3],
) {
    let document_id = u32::try_from(records_writer.len()).expect("too many documents");
    if document_id == u32::MAX {
        panic!("too many documents");
    }
    records_writer.write(Record(document.length(), payload));
    for &Element { key, value } in document.iter() {
        mappings_writer.write(Mapping(key, document_id, value));
    }
}

pub fn locally_merge(dir: impl AsRef<Path>, code: u32) {
    let dir = dir.as_ref();
    let (mut start, mut end) = (0_u32, 'end: {
        for number in 0..=u32::MAX {
            let filename = format!("mappings.{code:08x}.{number:08x}");
            if !handle_io_error(std::fs::exists(dir.join(filename))) {
                break 'end number;
            }
        }
        panic!("read too many files");
    });
    while start + 1 < end {
        let pivot = start.saturating_add(32).min(end);
        let iter = (start..pivot).flat_map(move |number| {
            let offset = 0_u32;
            let filename = format!("mappings.{code:08x}.{number:08x}");
            let file = handle_io_error(File::open(dir.join(filename)));
            Some((offset, file))
        });
        let reader = MappingsReader::open(iter);
        let file = {
            let filename = format!("mappings.{code:08x}.{end:08x}");
            handle_io_error(File::create_new(dir.join(filename)))
        };
        let mut writer = BufWriter::with_capacity(64 * 1024, file);
        for mapping in reader {
            handle_io_error(writer.write_all(mapping.as_bytes()));
        }
        handle_io_error(writer.flush());
        let _ = writer.into_inner();
        for number in start..pivot {
            let filename = format!("mappings.{code:08x}.{number:08x}");
            handle_io_error(std::fs::remove_file(dir.join(filename)));
        }
        (start, end) = (pivot, end.checked_add(1).expect("written too many files"));
    }
    if start < end {
        let old_filename = format!("mappings.{code:08x}.{start:08x}");
        let new_filename = format!("mappings.{code:08x}");
        handle_io_error(std::fs::rename(
            dir.join(old_filename),
            dir.join(new_filename),
        ));
    }
}

pub fn readers(dir: impl AsRef<Path>, total: u32) -> Segment<RecordsReader, MappingsReader> {
    let dir = dir.as_ref().to_path_buf();
    let offsets = {
        let mut offsets = Vec::with_capacity(total as usize);
        let mut offset = 0_u32;
        for code in 0..total {
            offsets.push(offset);
            let filename = format!("records.{code:08x}");
            let len = handle_io_error(std::fs::metadata(dir.join(filename))).len();
            if !len.is_multiple_of(size_of::<Record>() as u64) {
                panic!("IO error occurred during external sorting: data corruption")
            }
            let num = u32::try_from(len / size_of::<Record>() as u64).expect("too many documents");
            offset = offset.checked_add(num).expect("too many documents");
        }
        offsets
    };
    let records_reader = {
        let dir = dir.clone();
        let iter = (0..total).flat_map(move |code| {
            let filename = format!("records.{code:08x}");
            not_found_is_okay(File::open(dir.join(filename)))
        });
        RecordsReader::open(iter)
    };
    let mappings_reader = {
        let iter = (0..total).flat_map(|code| {
            let offset = offsets[code as usize];
            let filename = format!("mappings.{code:08x}");
            let file = not_found_is_okay(File::open(dir.join(filename)))?;
            Some((offset, file))
        });
        MappingsReader::open(iter)
    };
    Segment {
        records: records_reader,
        mappings: mappings_reader,
    }
}
