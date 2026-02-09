use std::collections::VecDeque;

pub struct RollingWindow<T> {
    blocks: VecDeque<(u64, Vec<T>)>,
    max_blocks: usize,
    last_broadcast_slot: u64,
}

impl<T> RollingWindow<T> {
    pub fn new(max_blocks: usize) -> Self {
        Self {
            blocks: VecDeque::new(),
            max_blocks,
            last_broadcast_slot: 0,
        }
    }

    pub fn add(&mut self, slot: u64, item: T) {
        if let Some((last_slot, items)) = self.blocks.back_mut() {
            if *last_slot == slot {
                items.push(item);
                return;
            }
        }
        self.blocks.push_back((slot, vec![item]));
        if self.blocks.len() > self.max_blocks {
            self.blocks.pop_front();
        }
    }

    pub fn slot_range(&self) -> (u64, u64) {
        match (self.blocks.front(), self.blocks.back()) {
            (Some((f, _)), Some((l, _))) => (*f, *l),
            _ => (0, 0),
        }
    }

    pub fn should_broadcast(&mut self, slot: u64) -> bool {
        if self.blocks.len() >= self.max_blocks && slot != self.last_broadcast_slot {
            self.last_broadcast_slot = slot;
            return true;
        }
        false
    }

    pub fn blocks(&self) -> &VecDeque<(u64, Vec<T>)> {
        &self.blocks
    }
}

pub fn percentile(sorted: &[u64], percentile: u32) -> u64 {
    if sorted.is_empty() {
        0
    } else {
        let percentile = percentile.min(9_999) as usize;
        let idx = percentile * sorted.len() / 10_000;
        sorted.get(idx).copied().unwrap_or(0)
    }
}
