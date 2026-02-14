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

use fixed_block_partition::FixedBlockPartition;

mod fixed_block_partition;

pub trait BlockPartitionTrait {
    fn partitions(&self) -> &[u32];
    fn max_doc(&self) -> &[u32];
    fn add_doc(&mut self, score: f32);
    fn reset(&mut self);
    fn make_partitions(&mut self);
}

pub type BlockPartition = FixedBlockPartition;
