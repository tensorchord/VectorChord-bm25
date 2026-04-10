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

use index::tuples::{Bool, MutChecker, Padding, RefChecker};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const ALIGN: usize = 8;
pub type Tag = u64;
const MAGIC: Tag = Tag::from_ne_bytes(*b"vchordbm");
const VERSION: u64 = 1;

#[inline(always)]
fn tag(source: &[u8]) -> Tag {
    assert!(source.len() >= size_of::<Tag>());
    #[allow(unsafe_code)]
    unsafe {
        source.as_ptr().cast::<Tag>().read_unaligned()
    }
}

pub trait Tuple: 'static {
    fn serialize(&self) -> Vec<u8>;
}

pub trait WithReader: Tuple {
    type Reader<'a>;
    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_>;
}

pub trait WithWriter: Tuple {
    type Writer<'a>;
    fn deserialize_mut(source: &mut [u8]) -> Self::Writer<'_>;
}

#[repr(C, align(8))]
#[derive(Debug, Clone, PartialEq, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct MetaTupleHeader {
    version: u64,
    k1: f64,
    b: f64,
    ptr_lock: u32,
    ptr_jump: u32,
}

pub struct MetaTuple {
    pub k1: f64,
    pub b: f64,
    pub ptr_lock: u32,
    pub ptr_jump: u32,
}

impl Tuple for MetaTuple {
    #[allow(clippy::match_single_binding)]
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        match self {
            MetaTuple {
                k1,
                b,
                ptr_lock,
                ptr_jump,
            } => {
                buffer.extend((MAGIC as Tag).to_ne_bytes());
                buffer.extend(
                    MetaTupleHeader {
                        version: VERSION,
                        k1: *k1,
                        b: *b,
                        ptr_jump: *ptr_jump,
                        ptr_lock: *ptr_lock,
                    }
                    .as_bytes(),
                );
            }
        }
        buffer
    }
}

impl WithReader for MetaTuple {
    type Reader<'a> = MetaTupleReader<'a>;
    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let tag = tag(source);
        match tag {
            MAGIC => {
                let checker = RefChecker::new(source);
                if VERSION != *checker.prefix::<u64>(size_of::<Tag>()) {
                    panic!(
                        "deserialization: bad version number; {}",
                        "after upgrading VectorChord-bm25, please use REINDEX to rebuild the index."
                    );
                }
                let header: &MetaTupleHeader = checker.prefix(size_of::<Tag>());
                MetaTupleReader { header }
            }
            _ => panic!("deserialization: bad magic number"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MetaTupleReader<'a> {
    header: &'a MetaTupleHeader,
}

impl<'a> MetaTupleReader<'a> {
    pub fn k1(self) -> f64 {
        self.header.k1
    }
    pub fn b(self) -> f64 {
        self.header.b
    }
    pub fn ptr_lock(self) -> u32 {
        self.header.ptr_lock
    }
    pub fn ptr_jump(self) -> u32 {
        self.header.ptr_jump
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct JumpTupleHeader {
    ptr_vectors: u32,
    number_of_documents: u32,
    sum_of_document_lengths: u64,
    root_documents: u32,
    depth_documents: u32,
    free_documents: u32,
    root_tokens: u32,
    depth_tokens: u32,
    free_tokens: u32,
    ptr_documents: u32,
    ptr_tokens: u32,
    ptr_summaries: u32,
    ptr_blocks: u32,
}

#[derive(Debug, Clone)]
pub struct JumpTuple {
    pub ptr_vectors: u32,
    pub number_of_documents: u32,
    pub sum_of_document_lengths: u64,
    pub root_documents: u32,
    pub depth_documents: u32,
    pub free_documents: u32,
    pub root_tokens: u32,
    pub depth_tokens: u32,
    pub free_tokens: u32,
    pub ptr_documents: u32,
    pub ptr_tokens: u32,
    pub ptr_summaries: u32,
    pub ptr_blocks: u32,
}

impl Tuple for JumpTuple {
    fn serialize(&self) -> Vec<u8> {
        JumpTupleHeader {
            ptr_vectors: self.ptr_vectors,
            number_of_documents: self.number_of_documents,
            sum_of_document_lengths: self.sum_of_document_lengths,
            root_documents: self.root_documents,
            depth_documents: self.depth_documents,
            free_documents: self.free_documents,
            root_tokens: self.root_tokens,
            depth_tokens: self.depth_tokens,
            free_tokens: self.free_tokens,
            ptr_documents: self.ptr_documents,
            ptr_tokens: self.ptr_tokens,
            ptr_summaries: self.ptr_summaries,
            ptr_blocks: self.ptr_blocks,
        }
        .as_bytes()
        .to_vec()
    }
}

impl WithReader for JumpTuple {
    type Reader<'a> = JumpTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let checker = RefChecker::new(source);
        let header: &JumpTupleHeader = checker.prefix(0_u16);
        JumpTupleReader { header }
    }
}

impl WithWriter for JumpTuple {
    type Writer<'a> = JumpTupleWriter<'a>;

    fn deserialize_mut(source: &mut [u8]) -> Self::Writer<'_> {
        let mut checker = MutChecker::new(source);
        let header: &mut JumpTupleHeader = checker.prefix(0_u16);
        JumpTupleWriter { header }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct JumpTupleReader<'a> {
    header: &'a JumpTupleHeader,
}

impl<'a> JumpTupleReader<'a> {
    pub fn ptr_vectors(self) -> u32 {
        self.header.ptr_vectors
    }
    pub fn number_of_documents(self) -> u32 {
        self.header.number_of_documents
    }
    pub fn sum_of_document_lengths(self) -> u64 {
        self.header.sum_of_document_lengths
    }
    pub fn root_documents(self) -> u32 {
        self.header.root_documents
    }
    pub fn depth_documents(self) -> u32 {
        self.header.depth_documents
    }
    pub fn root_tokens(self) -> u32 {
        self.header.root_tokens
    }
    pub fn depth_tokens(self) -> u32 {
        self.header.depth_tokens
    }
    pub fn ptr_documents(self) -> u32 {
        self.header.ptr_documents
    }
    pub fn ptr_summaries(self) -> u32 {
        self.header.ptr_summaries
    }
}

#[derive(Debug)]
pub struct JumpTupleWriter<'a> {
    header: &'a mut JumpTupleHeader,
}

impl<'a> JumpTupleWriter<'a> {
    pub fn ptr_vectors(&mut self) -> &mut u32 {
        &mut self.header.ptr_vectors
    }
    pub fn number_of_documents(&mut self) -> &mut u32 {
        &mut self.header.number_of_documents
    }
    pub fn sum_of_document_lengths(&mut self) -> &mut u64 {
        &mut self.header.sum_of_document_lengths
    }
    pub fn root_documents(&mut self) -> &mut u32 {
        &mut self.header.root_documents
    }
    pub fn depth_documents(&mut self) -> &mut u32 {
        &mut self.header.depth_documents
    }
    pub fn free_documents(&mut self) -> &mut u32 {
        &mut self.header.free_documents
    }
    pub fn root_tokens(&mut self) -> &mut u32 {
        &mut self.header.root_tokens
    }
    pub fn depth_tokens(&mut self) -> &mut u32 {
        &mut self.header.depth_tokens
    }
    pub fn free_tokens(&mut self) -> &mut u32 {
        &mut self.header.free_tokens
    }
    pub fn ptr_documents(&mut self) -> &mut u32 {
        &mut self.header.ptr_documents
    }
    pub fn ptr_tokens(&mut self) -> &mut u32 {
        &mut self.header.ptr_tokens
    }
    pub fn ptr_summaries(&mut self) -> &mut u32 {
        &mut self.header.ptr_summaries
    }
    pub fn ptr_blocks(&mut self) -> &mut u32 {
        &mut self.header.ptr_blocks
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct VectorTupleHeader0 {
    payload: [u16; 3],
    deleted: Bool,
    _padding_0: [Padding; 1],
    elements_s: u16,
    elements_e: u16,
    _padding_1: [Padding; 4],
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct VectorTupleHeader1 {
    elements_s: u16,
    elements_e: u16,
    _padding_0: [Padding; 4],
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct VectorTupleHeader2 {}

pub enum VectorTuple {
    _0 {
        payload: [u16; 3],
        deleted: Bool,
        elements: Vec<Element>,
    },
    _1 {
        elements: Vec<Element>,
    },
    _2 {},
}

impl Tuple for VectorTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        match self {
            VectorTuple::_0 {
                payload,
                deleted,
                elements,
            } => {
                buffer.extend((0 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<VectorTupleHeader0>()));
                // elements
                let elements_s = buffer.len() as u16;
                buffer.extend(elements.as_bytes());
                let elements_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<VectorTupleHeader0>()].copy_from_slice(
                    VectorTupleHeader0 {
                        payload: *payload,
                        deleted: *deleted,
                        elements_s,
                        elements_e,
                        _padding_0: Default::default(),
                        _padding_1: Default::default(),
                    }
                    .as_bytes(),
                );
            }
            VectorTuple::_1 { elements } => {
                buffer.extend((1 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<VectorTupleHeader1>()));
                // elements
                let elements_s = buffer.len() as u16;
                buffer.extend(elements.as_bytes());
                let elements_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<VectorTupleHeader1>()].copy_from_slice(
                    VectorTupleHeader1 {
                        elements_s,
                        elements_e,
                        _padding_0: Default::default(),
                    }
                    .as_bytes(),
                );
            }
            VectorTuple::_2 {} => {
                buffer.extend((2 as Tag).to_ne_bytes());
                buffer.extend(VectorTupleHeader2 {}.as_bytes());
            }
        }
        buffer
    }
}

impl VectorTuple {
    pub fn estimate_size_0(elements: usize) -> usize {
        let mut size = 0_usize;
        size += size_of::<Tag>();
        size += size_of::<VectorTupleHeader0>();
        size += (elements * size_of::<Element>()).next_multiple_of(ALIGN);
        size
    }
    pub fn fit_1(freespace: u16) -> Option<usize> {
        let mut freespace = freespace as isize;
        freespace &= !(ALIGN - 1) as isize;
        freespace -= size_of::<Tag>() as isize;
        freespace &= !(ALIGN - 1) as isize;
        freespace -= size_of::<VectorTupleHeader1>() as isize;
        freespace &= !(ALIGN - 1) as isize;
        if freespace >= 0 {
            Some(freespace as usize / size_of::<Element>())
        } else {
            None
        }
    }
}

impl WithReader for VectorTuple {
    type Reader<'a> = VectorTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let tag = tag(source);
        match tag {
            0 => {
                let checker = RefChecker::new(source);
                let header: &VectorTupleHeader0 = checker.prefix(size_of::<Tag>());
                let elements = checker.bytes(header.elements_s, header.elements_e);
                VectorTupleReader::_0(VectorTupleReader0 { header, elements })
            }
            1 => {
                let checker = RefChecker::new(source);
                let header: &VectorTupleHeader1 = checker.prefix(size_of::<Tag>());
                let elements = checker.bytes(header.elements_s, header.elements_e);
                VectorTupleReader::_1(VectorTupleReader1 { header, elements })
            }
            2 => {
                let checker = RefChecker::new(source);
                let header: &VectorTupleHeader2 = checker.prefix(size_of::<Tag>());
                VectorTupleReader::_2(VectorTupleReader2 { header })
            }
            _ => panic!("deserialization: bad magic number"),
        }
    }
}

impl WithWriter for VectorTuple {
    type Writer<'a> = VectorTupleWriter<'a>;

    fn deserialize_mut(source: &mut [u8]) -> Self::Writer<'_> {
        let tag = tag(source);
        match tag {
            0 => {
                let mut checker = MutChecker::new(source);
                let header: &mut VectorTupleHeader0 = checker.prefix(size_of::<Tag>());
                VectorTupleWriter::_0(VectorTupleWriter0 { header })
            }
            1 => {
                let mut checker = MutChecker::new(source);
                let header: &mut VectorTupleHeader1 = checker.prefix(size_of::<Tag>());
                let elements = checker.bytes(header.elements_s, header.elements_e);
                VectorTupleWriter::_1(VectorTupleWriter1 { header, elements })
            }
            2 => {
                let mut checker = MutChecker::new(source);
                let header: &mut VectorTupleHeader2 = checker.prefix(size_of::<Tag>());
                VectorTupleWriter::_2(VectorTupleWriter2 { header })
            }
            _ => panic!("deserialization: bad magic number"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VectorTupleReader<'a> {
    _0(VectorTupleReader0<'a>),
    _1(VectorTupleReader1<'a>),
    #[allow(dead_code)]
    _2(VectorTupleReader2<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct VectorTupleReader0<'a> {
    header: &'a VectorTupleHeader0,
    elements: &'a [Element],
}

impl<'a> VectorTupleReader0<'a> {
    pub fn deleted(self) -> Bool {
        self.header.deleted
    }
    pub fn payload(self) -> [u16; 3] {
        self.header.payload
    }
    pub fn elements(self) -> &'a [Element] {
        self.elements
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VectorTupleReader1<'a> {
    #[allow(dead_code)]
    header: &'a VectorTupleHeader1,
    elements: &'a [Element],
}

impl<'a> VectorTupleReader1<'a> {
    pub fn elements(self) -> &'a [Element] {
        self.elements
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VectorTupleReader2<'a> {
    #[allow(dead_code)]
    header: &'a VectorTupleHeader2,
}

pub enum VectorTupleWriter<'a> {
    _0(VectorTupleWriter0<'a>),
    #[allow(dead_code)]
    _1(VectorTupleWriter1<'a>),
    #[allow(dead_code)]
    _2(VectorTupleWriter2<'a>),
}

#[derive(Debug)]
pub struct VectorTupleWriter0<'a> {
    header: &'a mut VectorTupleHeader0,
}

impl<'a> VectorTupleWriter0<'a> {
    pub fn deleted(&mut self) -> &mut Bool {
        &mut self.header.deleted
    }
    pub fn payload(&mut self) -> &mut [u16; 3] {
        &mut self.header.payload
    }
}

#[derive(Debug)]
pub struct VectorTupleWriter1<'a> {
    #[allow(dead_code)]
    header: &'a mut VectorTupleHeader1,
    elements: &'a mut [Element],
}

impl<'a> VectorTupleWriter1<'a> {
    #[allow(dead_code)]
    pub fn elements(self) -> &'a [Element] {
        self.elements
    }
}

#[derive(Debug)]
pub struct VectorTupleWriter2<'a> {
    #[allow(dead_code)]
    header: &'a mut VectorTupleHeader2,
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct NodeTupleHeader {
    edges_s: u16,
    edges_e: u16,
    _padding_0: [Padding; 4],
}

pub struct NodeTuple {
    pub edges: Vec<Edge>,
}

impl Tuple for NodeTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        buffer.extend(std::iter::repeat_n(0, size_of::<NodeTupleHeader>()));
        // edges
        let edges_s = buffer.len() as u16;
        buffer.extend(self.edges.as_bytes());
        let edges_e = buffer.len() as u16;
        while buffer.len() % ALIGN != 0 {
            buffer.push(0);
        }
        // header
        buffer[..size_of::<NodeTupleHeader>()].copy_from_slice(
            NodeTupleHeader {
                edges_s,
                edges_e,
                _padding_0: Default::default(),
            }
            .as_bytes(),
        );
        buffer
    }
}

impl NodeTuple {
    pub fn fit(freespace: u16) -> Option<usize> {
        let mut freespace = freespace as isize;
        freespace &= !(ALIGN - 1) as isize;
        freespace -= size_of::<NodeTupleHeader>() as isize;
        freespace &= !(ALIGN - 1) as isize;
        if freespace >= 0 {
            Some(freespace as usize / size_of::<Edge>())
        } else {
            None
        }
    }
}

impl WithReader for NodeTuple {
    type Reader<'a> = NodeTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let checker = RefChecker::new(source);
        let header: &NodeTupleHeader = checker.prefix(0_u16);
        let edges: &[Edge] = checker.bytes(header.edges_s, header.edges_e);
        NodeTupleReader { header, edges }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NodeTupleReader<'a> {
    #[expect(dead_code)]
    header: &'a NodeTupleHeader,
    edges: &'a [Edge],
}

impl<'a> NodeTupleReader<'a> {
    pub fn edges(self) -> &'a [Edge] {
        self.edges
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct DocumentTupleHeader {
    id: u32,
    deleted: Bool,
    _padding_0: [Padding; 1],
    payload: [u16; 3],
    length: u32,
}

pub struct DocumentTuple {
    pub id: u32,
    pub deleted: Bool,
    pub length: u32,
    pub payload: [u16; 3],
}

impl Tuple for DocumentTuple {
    fn serialize(&self) -> Vec<u8> {
        DocumentTupleHeader {
            id: self.id,
            deleted: self.deleted,
            length: self.length,
            payload: self.payload,
            _padding_0: Default::default(),
        }
        .as_bytes()
        .to_vec()
    }
}

impl WithReader for DocumentTuple {
    type Reader<'a> = DocumentTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let checker = RefChecker::new(source);
        let header: &DocumentTupleHeader = checker.prefix(0_u16);
        DocumentTupleReader { header }
    }
}

impl WithWriter for DocumentTuple {
    type Writer<'a> = DocumentTupleWriter<'a>;

    fn deserialize_mut(source: &mut [u8]) -> Self::Writer<'_> {
        let mut checker = MutChecker::new(source);
        let header: &mut DocumentTupleHeader = checker.prefix(0_u16);
        DocumentTupleWriter { header }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DocumentTupleReader<'a> {
    header: &'a DocumentTupleHeader,
}

impl<'a> DocumentTupleReader<'a> {
    pub fn length(self) -> u32 {
        self.header.length
    }
    pub fn payload(self) -> [u16; 3] {
        self.header.payload
    }
    pub fn deleted(self) -> Bool {
        self.header.deleted
    }
}

#[derive(Debug)]
pub struct DocumentTupleWriter<'a> {
    header: &'a mut DocumentTupleHeader,
}

impl<'a> DocumentTupleWriter<'a> {
    pub fn payload(&mut self) -> &mut [u16; 3] {
        &mut self.header.payload
    }
    pub fn deleted(&mut self) -> &mut Bool {
        &mut self.header.deleted
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct TokenTupleHeader {
    id: u32,
    number_of_documents: u32,
    wand_document_length: u32,
    wand_term_frequency: u32,
    wptr_summaries: Pointer,
    _padding_0: [Padding; 2],
}

pub struct TokenTuple {
    pub id: u32,
    pub number_of_documents: u32,
    pub wand_document_length: u32,
    pub wand_term_frequency: u32,
    pub wptr_summaries: Pointer,
}

impl Tuple for TokenTuple {
    fn serialize(&self) -> Vec<u8> {
        TokenTupleHeader {
            id: self.id,
            number_of_documents: self.number_of_documents,
            wand_document_length: self.wand_document_length,
            wand_term_frequency: self.wand_term_frequency,
            wptr_summaries: self.wptr_summaries,
            _padding_0: Default::default(),
        }
        .as_bytes()
        .to_vec()
    }
}

impl WithReader for TokenTuple {
    type Reader<'a> = TokenTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let checker = RefChecker::new(source);
        let header: &TokenTupleHeader = checker.prefix(0_u16);
        TokenTupleReader { header }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenTupleReader<'a> {
    header: &'a TokenTupleHeader,
}

impl<'a> TokenTupleReader<'a> {
    pub fn number_of_documents(self) -> u32 {
        self.header.number_of_documents
    }
    pub fn wand_document_length(self) -> u32 {
        self.header.wand_document_length
    }
    pub fn wand_term_frequency(self) -> u32 {
        self.header.wand_term_frequency
    }
    pub fn wptr_summaries(self) -> Pointer {
        self.header.wptr_summaries
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct SummaryTupleHeader {
    token_id: u32,
    min_document_id: u32,
    max_document_id: u32,
    number_of_documents: u32,
    wand_document_length: u32,
    wand_term_frequency: u32,
    wptr_block: Pointer,
    _padding_0: [Padding; 2],
}

pub struct SummaryTuple {
    pub token_id: u32,
    pub min_document_id: u32,
    pub max_document_id: u32,
    pub number_of_documents: u32,
    pub wand_document_length: u32,
    pub wand_term_frequency: u32,
    pub wptr_block: Pointer,
}

impl Tuple for SummaryTuple {
    fn serialize(&self) -> Vec<u8> {
        SummaryTupleHeader {
            token_id: self.token_id,
            min_document_id: self.min_document_id,
            max_document_id: self.max_document_id,
            number_of_documents: self.number_of_documents,
            wand_document_length: self.wand_document_length,
            wand_term_frequency: self.wand_term_frequency,
            wptr_block: self.wptr_block,
            _padding_0: Default::default(),
        }
        .as_bytes()
        .to_vec()
    }
}

impl WithReader for SummaryTuple {
    type Reader<'a> = SummaryTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let checker = RefChecker::new(source);
        let header: &SummaryTupleHeader = checker.prefix(0_u16);
        SummaryTupleReader { header }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SummaryTupleReader<'a> {
    header: &'a SummaryTupleHeader,
}

impl<'a> SummaryTupleReader<'a> {
    pub fn token_id(self) -> u32 {
        self.header.token_id
    }
    pub fn min_document_id(self) -> u32 {
        self.header.min_document_id
    }
    pub fn max_document_id(self) -> u32 {
        self.header.max_document_id
    }
    pub fn number_of_documents(self) -> u32 {
        self.header.number_of_documents
    }
    pub fn wand_document_length(self) -> u32 {
        self.header.wand_document_length
    }
    pub fn wand_term_frequency(self) -> u32 {
        self.header.wand_term_frequency
    }
    pub fn wptr_block(self) -> Pointer {
        self.header.wptr_block
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct BlockTupleHeader {
    bitwidth_document_ids: u8,
    bitwidth_term_frequencies: u8,
    compressed_document_ids_s: u16,
    compressed_document_ids_e: u16,
    compressed_term_frequencies_s: u16,
    compressed_term_frequencies_e: u16,
    _padding_0: [Padding; 6],
}

pub struct BlockTuple {
    pub bitwidth_document_ids: u8,
    pub bitwidth_term_frequencies: u8,
    pub compressed_document_ids: Vec<u8>,
    pub compressed_term_frequencies: Vec<u8>,
}

impl Tuple for BlockTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        buffer.extend(std::iter::repeat_n(0, size_of::<BlockTupleHeader>()));
        // compressed_document_ids
        let compressed_document_ids_s = buffer.len() as u16;
        buffer.extend(self.compressed_document_ids.as_bytes());
        let compressed_document_ids_e = buffer.len() as u16;
        while buffer.len() % ALIGN != 0 {
            buffer.push(0);
        }
        // compressed_term_frequencies
        let compressed_term_frequencies_s = buffer.len() as u16;
        buffer.extend(self.compressed_term_frequencies.as_bytes());
        let compressed_term_frequencies_e = buffer.len() as u16;
        while buffer.len() % ALIGN != 0 {
            buffer.push(0);
        }
        // header
        buffer[..size_of::<BlockTupleHeader>()].copy_from_slice(
            BlockTupleHeader {
                bitwidth_document_ids: self.bitwidth_document_ids,
                bitwidth_term_frequencies: self.bitwidth_term_frequencies,
                compressed_document_ids_s,
                compressed_document_ids_e,
                compressed_term_frequencies_s,
                compressed_term_frequencies_e,
                _padding_0: Default::default(),
            }
            .as_bytes(),
        );
        buffer
    }
}

impl WithReader for BlockTuple {
    type Reader<'a> = BlockTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> Self::Reader<'_> {
        let checker = RefChecker::new(source);
        let header: &BlockTupleHeader = checker.prefix(0_u16);
        let compressed_document_ids = checker.bytes(
            header.compressed_document_ids_s,
            header.compressed_document_ids_e,
        );
        let compressed_term_frequencies = checker.bytes(
            header.compressed_term_frequencies_s,
            header.compressed_term_frequencies_e,
        );
        BlockTupleReader {
            header,
            compressed_document_ids,
            compressed_term_frequencies,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlockTupleReader<'a> {
    header: &'a BlockTupleHeader,
    compressed_document_ids: &'a [u8],
    compressed_term_frequencies: &'a [u8],
}

impl<'a> BlockTupleReader<'a> {
    pub fn bitwidth_document_ids(self) -> u8 {
        self.header.bitwidth_document_ids
    }
    pub fn bitwidth_term_frequencies(self) -> u8 {
        self.header.bitwidth_term_frequencies
    }
    pub fn compressed_document_ids(self) -> &'a [u8] {
        self.compressed_document_ids
    }
    pub fn compressed_term_frequencies(self) -> &'a [u8] {
        self.compressed_term_frequencies
    }
}

#[repr(C, packed(2))]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct Pointer {
    x: u32,
    y: u16,
}

impl Pointer {
    pub fn new((x, y): (u32, u16)) -> Self {
        Self { x, y }
    }
    pub fn into_inner(self) -> (u32, u16) {
        (self.x, self.y)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct Element {
    index: u32,
    value: u32,
}

impl Element {
    pub fn new((index, value): (u32, u32)) -> Self {
        Self { index, value }
    }
    pub fn into_inner(self) -> (u32, u32) {
        (self.index, self.value)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct Edge {
    key: u32,
    value: u32,
}

impl Edge {
    pub fn new((key, value): (u32, u32)) -> Self {
        Self { key, value }
    }
    pub fn into_inner(self) -> (u32, u32) {
        (self.key, self.value)
    }
}
