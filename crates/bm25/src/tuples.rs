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

use index::tuples::{Padding, RefChecker};
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

#[expect(dead_code)]
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
    wptr_segment: u32,
    _padding_0: [Padding; 4],
}

pub struct MetaTuple {
    pub k1: f64,
    pub b: f64,
    pub wptr_segment: u32,
}

impl Tuple for MetaTuple {
    #[allow(clippy::match_single_binding)]
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        match self {
            MetaTuple {
                k1,
                b,
                wptr_segment,
            } => {
                buffer.extend((MAGIC as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<MetaTupleHeader>()));
                // header
                buffer[size_of::<Tag>()..][..size_of::<MetaTupleHeader>()].copy_from_slice(
                    MetaTupleHeader {
                        version: VERSION,
                        k1: *k1,
                        b: *b,
                        wptr_segment: *wptr_segment,
                        _padding_0: Default::default(),
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
    fn deserialize_ref(source: &[u8]) -> MetaTupleReader<'_> {
        let tag = tag(source);
        match tag {
            MAGIC => {
                let checker = RefChecker::new(source);
                if VERSION != *checker.prefix::<u64>(size_of::<Tag>()) {
                    panic!(
                        "deserialization: bad version number; {}",
                        "after upgrading VectorChord, please use REINDEX to rebuild the index."
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
    pub fn wptr_segment(self) -> u32 {
        self.header.wptr_segment
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct IndexTupleHeader0 {
    pairs_s: u16,
    pairs_e: u16,
    _padding_0: [Padding; 4],
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct IndexTupleHeader1 {
    pairs_s: u16,
    pairs_e: u16,
    _padding_0: [Padding; 4],
}

pub enum IndexTuple {
    _0 { pairs: Vec<Index0> },
    _1 { pairs: Vec<Index1> },
}

impl Tuple for IndexTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        match self {
            IndexTuple::_0 { pairs } => {
                buffer.extend((0 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<IndexTupleHeader0>()));
                // pairs
                let pairs_s = buffer.len() as u16;
                buffer.extend(pairs.as_bytes());
                let pairs_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<IndexTupleHeader0>()].copy_from_slice(
                    IndexTupleHeader0 {
                        pairs_s,
                        pairs_e,
                        _padding_0: Default::default(),
                    }
                    .as_bytes(),
                );
            }
            IndexTuple::_1 { pairs } => {
                buffer.extend((1 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<IndexTupleHeader1>()));
                // pairs
                let pairs_s = buffer.len() as u16;
                buffer.extend(pairs.as_bytes());
                let pairs_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<IndexTupleHeader1>()].copy_from_slice(
                    IndexTupleHeader1 {
                        pairs_s,
                        pairs_e,
                        _padding_0: Default::default(),
                    }
                    .as_bytes(),
                );
            }
        }
        buffer
    }
}

impl WithReader for IndexTuple {
    type Reader<'a> = IndexTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> IndexTupleReader<'_> {
        let tag = tag(source);
        match tag {
            0 => {
                let checker = RefChecker::new(source);
                let header: &IndexTupleHeader0 = checker.prefix(size_of::<Tag>());
                let pairs: &[Index0] = checker.bytes(header.pairs_s, header.pairs_e);
                IndexTupleReader::_0(IndexTupleReader0 { header, pairs })
            }
            1 => {
                let checker = RefChecker::new(source);
                let header: &IndexTupleHeader1 = checker.prefix(size_of::<Tag>());
                let pairs: &[Index1] = checker.bytes(header.pairs_s, header.pairs_e);
                IndexTupleReader::_1(IndexTupleReader1 { header, pairs })
            }
            _ => panic!("deserialization: bad magic number"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IndexTupleReader<'a> {
    _0(IndexTupleReader0<'a>),
    _1(IndexTupleReader1<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct IndexTupleReader0<'a> {
    #[expect(dead_code)]
    header: &'a IndexTupleHeader0,
    pairs: &'a [Index0],
}

impl<'a> IndexTupleReader0<'a> {
    pub fn pairs(self) -> &'a [Index0] {
        self.pairs
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IndexTupleReader1<'a> {
    #[expect(dead_code)]
    header: &'a IndexTupleHeader1,
    pairs: &'a [Index1],
}

impl<'a> IndexTupleReader1<'a> {
    pub fn pairs(self) -> &'a [Index1] {
        self.pairs
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct SegmentTupleHeader {
    number_of_documents: u32,
    number_of_tokens: u32,
    sum_of_document_lengths: u64,
    iptr_documents: u32,
    iptr_tokens: u32,
    sptr_summaries: u32,
    sptr_blocks: u32,
}

pub struct SegmentTuple {
    pub number_of_documents: u32,
    pub number_of_tokens: u32,
    pub sum_of_document_lengths: u64,
    pub iptr_documents: u32,
    pub iptr_tokens: u32,
    pub sptr_summaries: u32,
    pub sptr_blocks: u32,
}

impl Tuple for SegmentTuple {
    fn serialize(&self) -> Vec<u8> {
        SegmentTupleHeader {
            number_of_documents: self.number_of_documents,
            number_of_tokens: self.number_of_tokens,
            sum_of_document_lengths: self.sum_of_document_lengths,
            iptr_documents: self.iptr_documents,
            iptr_tokens: self.iptr_tokens,
            sptr_summaries: self.sptr_summaries,
            sptr_blocks: self.sptr_blocks,
        }
        .as_bytes()
        .to_vec()
    }
}

impl WithReader for SegmentTuple {
    type Reader<'a> = SegmentTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> SegmentTupleReader<'_> {
        let checker = RefChecker::new(source);
        let header: &SegmentTupleHeader = checker.prefix(0_u16);
        SegmentTupleReader { header }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentTupleReader<'a> {
    header: &'a SegmentTupleHeader,
}

impl<'a> SegmentTupleReader<'a> {
    pub fn number_of_documents(self) -> u32 {
        self.header.number_of_documents
    }
    #[expect(dead_code)]
    pub fn number_of_tokens(self) -> u32 {
        self.header.number_of_tokens
    }
    pub fn sum_of_document_lengths(self) -> u64 {
        self.header.sum_of_document_lengths
    }
    pub fn iptr_documents(self) -> u32 {
        self.header.iptr_documents
    }
    pub fn iptr_tokens(self) -> u32 {
        self.header.iptr_tokens
    }
    #[expect(dead_code)]
    pub fn sptr_summaries(self) -> u32 {
        self.header.sptr_summaries
    }
    #[expect(dead_code)]
    pub fn sptr_blocks(self) -> u32 {
        self.header.sptr_blocks
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct DocumentTupleHeader {
    length: u32,
    payload: [u16; 3],
    _padding_0: [Padding; 6],
}

pub struct DocumentTuple {
    pub length: u32,
    pub payload: [u16; 3],
}

impl Tuple for DocumentTuple {
    fn serialize(&self) -> Vec<u8> {
        DocumentTupleHeader {
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

    fn deserialize_ref(source: &[u8]) -> DocumentTupleReader<'_> {
        let checker = RefChecker::new(source);
        let header: &DocumentTupleHeader = checker.prefix(0_u16);
        DocumentTupleReader { header }
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
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct TokenTupleHeader {
    number_of_documents: u32,
    wand_document_length: u32,
    wand_term_frequency: u32,
    wptr_summaries: Pointer,
    _padding_0: [Padding; 4],
}

pub struct TokenTuple {
    pub number_of_documents: u32,
    pub wand_document_length: u32,
    pub wand_term_frequency: u32,
    pub wptr_summaries: Pointer,
}

impl Tuple for TokenTuple {
    fn serialize(&self) -> Vec<u8> {
        TokenTupleHeader {
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

    fn deserialize_ref(source: &[u8]) -> TokenTupleReader<'_> {
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
        }
        .as_bytes()
        .to_vec()
    }
}

impl WithReader for SummaryTuple {
    type Reader<'a> = SummaryTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> SummaryTupleReader<'_> {
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

    fn deserialize_ref(source: &[u8]) -> BlockTupleReader<'_> {
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

#[repr(C)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    IntoBytes,
    FromBytes,
    Immutable,
    KnownLayout,
)]
pub struct Pointer {
    x: u32,
    y: u16,
    _padding_0: [Padding; 2],
}

impl Pointer {
    pub fn new((x, y): (u32, u16)) -> Self {
        Self {
            x,
            y,
            _padding_0: Default::default(),
        }
    }
    pub fn into_inner(self) -> (u32, u16) {
        (self.x, self.y)
    }
}

#[repr(C)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    IntoBytes,
    FromBytes,
    Immutable,
    KnownLayout,
)]
pub struct Index0 {
    key: u32,
    val: Pointer,
}

impl Index0 {
    pub fn new((key, val): (u32, Pointer)) -> Self {
        Self { key, val }
    }
    pub fn into_inner(self) -> (u32, Pointer) {
        (self.key, self.val)
    }
}

#[repr(C)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    IntoBytes,
    FromBytes,
    Immutable,
    KnownLayout,
)]
pub struct Index1 {
    key: u32,
    val: u32,
}

impl Index1 {
    pub fn new((key, val): (u32, u32)) -> Self {
        Self { key, val }
    }
    pub fn into_inner(self) -> (u32, u32) {
        (self.key, self.val)
    }
}
