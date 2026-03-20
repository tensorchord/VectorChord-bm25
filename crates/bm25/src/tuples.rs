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
use u48::U48;
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
struct U32IndexTupleHeader0 {
    pairs_s: u16,
    pairs_e: u16,
    _padding_0: [Padding; 4],
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct U32IndexTupleHeader1 {
    pairs_s: u16,
    pairs_e: u16,
    _padding_0: [Padding; 4],
}

pub enum U32IndexTuple {
    _0 { pairs: Vec<U32Index0> },
    _1 { pairs: Vec<U32Index1> },
}

impl Tuple for U32IndexTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        match self {
            U32IndexTuple::_0 { pairs } => {
                buffer.extend((0 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<U32IndexTupleHeader0>()));
                // pairs
                let pairs_s = buffer.len() as u16;
                buffer.extend(pairs.as_bytes());
                let pairs_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<U32IndexTupleHeader0>()].copy_from_slice(
                    U32IndexTupleHeader0 {
                        pairs_s,
                        pairs_e,
                        _padding_0: Default::default(),
                    }
                    .as_bytes(),
                );
            }
            U32IndexTuple::_1 { pairs } => {
                buffer.extend((1 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<U32IndexTupleHeader1>()));
                // pairs
                let pairs_s = buffer.len() as u16;
                buffer.extend(pairs.as_bytes());
                let pairs_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<U32IndexTupleHeader1>()].copy_from_slice(
                    U32IndexTupleHeader1 {
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

impl WithReader for U32IndexTuple {
    type Reader<'a> = U32IndexTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> U32IndexTupleReader<'_> {
        let tag = tag(source);
        match tag {
            0 => {
                let checker = RefChecker::new(source);
                let header: &U32IndexTupleHeader0 = checker.prefix(size_of::<Tag>());
                let pairs: &[U32Index0] = checker.bytes(header.pairs_s, header.pairs_e);
                U32IndexTupleReader::_0(U32IndexTupleReader0 { header, pairs })
            }
            1 => {
                let checker = RefChecker::new(source);
                let header: &U32IndexTupleHeader1 = checker.prefix(size_of::<Tag>());
                let pairs: &[U32Index1] = checker.bytes(header.pairs_s, header.pairs_e);
                U32IndexTupleReader::_1(U32IndexTupleReader1 { header, pairs })
            }
            _ => panic!("deserialization: bad magic number"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum U32IndexTupleReader<'a> {
    _0(U32IndexTupleReader0<'a>),
    _1(U32IndexTupleReader1<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct U32IndexTupleReader0<'a> {
    #[expect(dead_code)]
    header: &'a U32IndexTupleHeader0,
    pairs: &'a [U32Index0],
}

impl<'a> U32IndexTupleReader0<'a> {
    pub fn pairs(self) -> &'a [U32Index0] {
        self.pairs
    }
}

#[derive(Debug, Clone, Copy)]
pub struct U32IndexTupleReader1<'a> {
    #[expect(dead_code)]
    header: &'a U32IndexTupleHeader1,
    pairs: &'a [U32Index1],
}

impl<'a> U32IndexTupleReader1<'a> {
    pub fn pairs(self) -> &'a [U32Index1] {
        self.pairs
    }
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct U48IndexTupleHeader0 {
    pairs_s: u16,
    pairs_e: u16,
    _padding_0: [Padding; 4],
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct U48IndexTupleHeader1 {
    pairs_s: u16,
    pairs_e: u16,
    _padding_0: [Padding; 4],
}

pub enum U48IndexTuple {
    _0 { pairs: Vec<U48Index0> },
    _1 { pairs: Vec<U48Index1> },
}

impl Tuple for U48IndexTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        match self {
            U48IndexTuple::_0 { pairs } => {
                buffer.extend((0 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<U48IndexTupleHeader0>()));
                // pairs
                let pairs_s = buffer.len() as u16;
                buffer.extend(pairs.as_bytes());
                let pairs_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<U48IndexTupleHeader0>()].copy_from_slice(
                    U48IndexTupleHeader0 {
                        pairs_s,
                        pairs_e,
                        _padding_0: Default::default(),
                    }
                    .as_bytes(),
                );
            }
            U48IndexTuple::_1 { pairs } => {
                buffer.extend((1 as Tag).to_ne_bytes());
                buffer.extend(std::iter::repeat_n(0, size_of::<U48IndexTupleHeader1>()));
                // pairs
                let pairs_s = buffer.len() as u16;
                buffer.extend(pairs.as_bytes());
                let pairs_e = buffer.len() as u16;
                while buffer.len() % ALIGN != 0 {
                    buffer.push(0);
                }
                // header
                buffer[size_of::<Tag>()..][..size_of::<U48IndexTupleHeader1>()].copy_from_slice(
                    U48IndexTupleHeader1 {
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

impl WithReader for U48IndexTuple {
    type Reader<'a> = U48IndexTupleReader<'a>;

    fn deserialize_ref(source: &[u8]) -> U48IndexTupleReader<'_> {
        let tag = tag(source);
        match tag {
            0 => {
                let checker = RefChecker::new(source);
                let header: &U48IndexTupleHeader0 = checker.prefix(size_of::<Tag>());
                let pairs: &[U48Index0] = checker.bytes(header.pairs_s, header.pairs_e);
                U48IndexTupleReader::_0(U48IndexTupleReader0 { header, pairs })
            }
            1 => {
                let checker = RefChecker::new(source);
                let header: &U48IndexTupleHeader1 = checker.prefix(size_of::<Tag>());
                let pairs: &[U48Index1] = checker.bytes(header.pairs_s, header.pairs_e);
                U48IndexTupleReader::_1(U48IndexTupleReader1 { header, pairs })
            }
            _ => panic!("deserialization: bad magic number"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum U48IndexTupleReader<'a> {
    _0(U48IndexTupleReader0<'a>),
    _1(U48IndexTupleReader1<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct U48IndexTupleReader0<'a> {
    #[expect(dead_code)]
    header: &'a U48IndexTupleHeader0,
    pairs: &'a [U48Index0],
}

impl<'a> U48IndexTupleReader0<'a> {
    pub fn pairs(self) -> &'a [U48Index0] {
        self.pairs
    }
}

#[derive(Debug, Clone, Copy)]
pub struct U48IndexTupleReader1<'a> {
    #[expect(dead_code)]
    header: &'a U48IndexTupleHeader1,
    pairs: &'a [U48Index1],
}

impl<'a> U48IndexTupleReader1<'a> {
    pub fn pairs(self) -> &'a [U48Index1] {
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
    _padding_0: [Padding; 4],
}

pub struct DocumentTuple {
    pub length: u32,
}

impl Tuple for DocumentTuple {
    fn serialize(&self) -> Vec<u8> {
        DocumentTupleHeader {
            length: self.length,
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
}

#[repr(C, align(8))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
struct TokenTupleHeader {
    number_of_documents: u32,
    wand_document_length: u32,
    wand_term_frequency: u32,
    wptr_summaries: Pointer,
    _padding_0: [Padding; 6],
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
    min_document_id: U48,
    max_document_id: U48,
    number_of_documents: u32,
    wand_document_length: u32,
    wand_term_frequency: u32,
    wptr_block: Pointer,
    _padding_0: [Padding; 6],
}

pub struct SummaryTuple {
    pub token_id: u32,
    pub min_document_id: U48,
    pub max_document_id: U48,
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
    pub fn min_document_id(self) -> U48 {
        self.header.min_document_id
    }
    pub fn max_document_id(self) -> U48 {
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
    bitwidth_document_ids_0: u8,
    bitwidth_document_ids_1: u8,
    bitwidth_term_frequencies: u8,
    _padding_0: [Padding; 1],
    compressed_document_ids_0_s: u16,
    compressed_document_ids_0_e: u16,
    compressed_document_ids_1_s: u16,
    compressed_document_ids_1_e: u16,
    compressed_term_frequencies_s: u16,
    compressed_term_frequencies_e: u16,
}

pub struct BlockTuple {
    pub bitwidth_document_ids_0: u8,
    pub bitwidth_document_ids_1: u8,
    pub bitwidth_term_frequencies: u8,
    pub compressed_document_ids_0: Vec<u8>,
    pub compressed_document_ids_1: Vec<u8>,
    pub compressed_term_frequencies: Vec<u8>,
}

impl Tuple for BlockTuple {
    fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        buffer.extend(std::iter::repeat_n(0, size_of::<BlockTupleHeader>()));
        // compressed_document_ids_0
        let compressed_document_ids_0_s = buffer.len() as u16;
        buffer.extend(self.compressed_document_ids_0.as_bytes());
        let compressed_document_ids_0_e = buffer.len() as u16;
        while buffer.len() % ALIGN != 0 {
            buffer.push(0);
        }
        // compressed_document_ids_1
        let compressed_document_ids_1_s = buffer.len() as u16;
        buffer.extend(self.compressed_document_ids_1.as_bytes());
        let compressed_document_ids_1_e = buffer.len() as u16;
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
                bitwidth_document_ids_0: self.bitwidth_document_ids_0,
                bitwidth_document_ids_1: self.bitwidth_document_ids_1,
                bitwidth_term_frequencies: self.bitwidth_term_frequencies,
                compressed_document_ids_0_s,
                compressed_document_ids_0_e,
                compressed_document_ids_1_s,
                compressed_document_ids_1_e,
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
        let compressed_document_ids_0 = checker.bytes(
            header.compressed_document_ids_0_s,
            header.compressed_document_ids_0_e,
        );
        let compressed_document_ids_1 = checker.bytes(
            header.compressed_document_ids_1_s,
            header.compressed_document_ids_1_e,
        );
        let compressed_term_frequencies = checker.bytes(
            header.compressed_term_frequencies_s,
            header.compressed_term_frequencies_e,
        );
        BlockTupleReader {
            header,
            compressed_document_ids_0,
            compressed_document_ids_1,
            compressed_term_frequencies,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlockTupleReader<'a> {
    header: &'a BlockTupleHeader,
    compressed_document_ids_0: &'a [u8],
    compressed_document_ids_1: &'a [u8],
    compressed_term_frequencies: &'a [u8],
}

impl<'a> BlockTupleReader<'a> {
    pub fn bitwidth_document_ids_0(self) -> u8 {
        self.header.bitwidth_document_ids_0
    }
    pub fn bitwidth_document_ids_1(self) -> u8 {
        self.header.bitwidth_document_ids_1
    }
    pub fn bitwidth_term_frequencies(self) -> u8 {
        self.header.bitwidth_term_frequencies
    }
    pub fn compressed_document_ids_0(self) -> &'a [u8] {
        self.compressed_document_ids_0
    }
    pub fn compressed_document_ids_1(self) -> &'a [u8] {
        self.compressed_document_ids_1
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
pub struct Pointer(U48);

impl Pointer {
    pub fn new((x, y): (u32, u16)) -> Self {
        Self(U48::from_pair((x, y)))
    }
    pub fn into_inner(self) -> (u32, u16) {
        self.0.to_pair()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct U32Index0 {
    key: [u8; size_of::<u32>()],
    val: Pointer,
}

impl U32Index0 {
    pub fn new((key, val): (u32, Pointer)) -> Self {
        Self {
            key: u32::to_ne_bytes(key),
            val,
        }
    }
    pub fn into_inner(self) -> (u32, Pointer) {
        (u32::from_ne_bytes(self.key), self.val)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct U32Index1 {
    key: u32,
    val: u32,
}

impl U32Index1 {
    pub fn new((key, val): (u32, u32)) -> Self {
        Self { key, val }
    }
    pub fn into_inner(self) -> (u32, u32) {
        (self.key, self.val)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct U48Index0 {
    key: U48,
    val: U48,
}

impl U48Index0 {
    pub fn new((key, val): (U48, Pointer)) -> Self {
        Self {
            key,
            val: U48::from_pair(val.into_inner()),
        }
    }
    pub fn into_inner(self) -> (U48, Pointer) {
        (self.key, Pointer::new(self.val.to_pair()))
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct U48Index1 {
    key: U48,
    val: [u8; size_of::<u32>()],
}

impl U48Index1 {
    pub fn new((key, val): (U48, u32)) -> Self {
        Self {
            key,
            val: val.to_ne_bytes(),
        }
    }
    pub fn into_inner(self) -> (U48, u32) {
        (self.key, u32::from_ne_bytes(self.val))
    }
}
