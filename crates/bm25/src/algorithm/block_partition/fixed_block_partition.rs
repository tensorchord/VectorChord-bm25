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

use super::BlockPartitionTrait;

const BLOCK_SIZE: usize = 128;

pub struct FixedBlockPartition {
    scores: Vec<f32>,
    partitions: Vec<u32>,
    max_doc: Vec<u32>,
}

impl FixedBlockPartition {
    pub fn new() -> Self {
        Self {
            scores: Vec::new(),
            partitions: Vec::new(),
            max_doc: Vec::new(),
        }
    }
}

impl BlockPartitionTrait for FixedBlockPartition {
    fn partitions(&self) -> &[u32] {
        &self.partitions
    }

    fn max_doc(&self) -> &[u32] {
        &self.max_doc
    }

    fn add_doc(&mut self, score: f32) {
        self.scores.push(score);
    }

    fn reset(&mut self) {
        self.scores.clear();
        self.partitions.clear();
        self.max_doc.clear();
    }

    fn make_partitions(&mut self) {
        let doc_cnt = self.scores.len();
        let full_block_cnt = doc_cnt / BLOCK_SIZE;
        for i in 0..full_block_cnt {
            let start: u32 = (i * BLOCK_SIZE).try_into().unwrap();
            self.partitions.push(start + BLOCK_SIZE as u32 - 1);
            let max_doc: u32 = self.scores[start as usize..][..BLOCK_SIZE]
                .iter()
                .cloned()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap()
                .0
                .try_into()
                .unwrap();
            self.max_doc.push(max_doc + start);
        }
    }
}
