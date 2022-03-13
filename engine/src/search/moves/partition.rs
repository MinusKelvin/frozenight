use arrayvec::ArrayVec;

use super::ScoredMove;

#[derive(Debug)]
pub struct PartitionedMoveList {
    move_list: ArrayVec<ScoredMove, 218>,
    partitioned: usize
}

impl PartitionedMoveList {
    pub fn new() -> Self {
        Self {
            move_list: ArrayVec::new(),
            partitioned: 0
        }
    }

    pub fn new_partition(&mut self, mut builder: impl FnMut(PartitionBuilder)) -> Partition {
        builder(PartitionBuilder(self));
        self.build_partition()
    }

    pub fn new_partition_from_slice(&mut self, slice: &[ScoredMove]) -> Partition {
        self.move_list.try_extend_from_slice(slice).unwrap();
        self.build_partition()
    }
    
    fn build_partition(&mut self) -> Partition {
        let start = self.partitioned;
        let end = self.move_list.len();
        pdqsort::sort_by_key(&mut self.move_list[start..end], |(_, s)| std::cmp::Reverse(*s));
        self.partitioned = end;
        Partition {
            start,
            yielded: 0,
            end
        }
    }

    pub fn yield_from_partition(&mut self, partition: &mut Partition) -> Option<&ScoredMove> {
        let moves = &mut self.move_list[(partition.start + partition.yielded)..partition.end];
        let mv = moves.first();
        if mv.is_some() {
            partition.yielded += 1;
        }
        mv        
    }

    pub fn yielded_from_partition(&self, partition: &Partition) -> &[ScoredMove] {
        &self.move_list[partition.start..(partition.start + partition.yielded)]
    }
}

pub struct PartitionBuilder<'m>(&'m mut PartitionedMoveList);

impl PartitionBuilder<'_> {
    pub fn push(&mut self, mv: ScoredMove) {
        self.0.move_list.push(mv);
    }
}

#[derive(Debug, Clone)]
pub struct Partition {
    start: usize,
    yielded: usize,
    end: usize
}
