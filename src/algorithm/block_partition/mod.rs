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
