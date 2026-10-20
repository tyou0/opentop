//! Ring buffers of recent samples, kept alongside the snapshot so graph
//! widgets can draw history without owning sampling logic themselves.

use std::collections::{BTreeMap, VecDeque};

/// Fixed-capacity ring buffer of `f32` samples, oldest first.
#[derive(Debug, Clone)]
pub struct Ring {
    buf: VecDeque<f32>,
    cap: usize,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            buf: VecDeque::with_capacity(cap),
            cap,
        }
    }

    pub fn push(&mut self, value: f32) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(value);
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn latest(&self) -> Option<f32> {
        self.buf.back().copied()
    }

    pub fn max(&self) -> Option<f32> {
        self.buf.iter().copied().reduce(f32::max)
    }

    pub fn min(&self) -> Option<f32> {
        self.buf.iter().copied().reduce(f32::min)
    }

    pub fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        self.buf.iter().copied()
    }

    pub fn to_vec(&self) -> Vec<f32> {
        self.buf.iter().copied().collect()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

/// Per-interface network history.
#[derive(Debug, Clone)]
pub struct NetHistory {
    pub rx: Ring,
    pub tx: Ring,
}

/// Recent history for every metric that benefits from a graph.
#[derive(Debug, Clone)]
pub struct History {
    pub cpu_total: Ring,
    pub cpu_cores: Vec<Ring>,
    pub memory_used: Ring,
    pub memory_percent: Ring,
    pub swap_used: Ring,
    pub net: BTreeMap<String, NetHistory>,
    pub cpu_temp: Ring,
    pub gpu_temp: Ring,
    pub cpu_power: Ring,
    pub gpu_power: Ring,
    pub gpu_usage: Ring,
    pub battery_percent: Ring,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            cpu_total: Ring::new(capacity),
            cpu_cores: Vec::new(),
            memory_used: Ring::new(capacity),
            memory_percent: Ring::new(capacity),
            swap_used: Ring::new(capacity),
            net: BTreeMap::new(),
            cpu_temp: Ring::new(capacity),
            gpu_temp: Ring::new(capacity),
            cpu_power: Ring::new(capacity),
            gpu_power: Ring::new(capacity),
            gpu_usage: Ring::new(capacity),
            battery_percent: Ring::new(capacity),
            capacity: capacity.max(1),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn push_cpu(&mut self, total: f32, cores: &[f32]) {
        self.cpu_total.push(total);
        if self.cpu_cores.len() != cores.len() {
            self.cpu_cores = cores.iter().map(|_| Ring::new(self.capacity)).collect();
        }
        for (ring, value) in self.cpu_cores.iter_mut().zip(cores) {
            ring.push(*value);
        }
    }

    pub fn push_memory(&mut self, used_bytes: u64, percent: f32, swap_used_bytes: u64) {
        self.memory_used.push(used_bytes as f32);
        self.memory_percent.push(percent);
        self.swap_used.push(swap_used_bytes as f32);
    }

    pub fn push_net(&mut self, name: &str, rx_per_sec: f64, tx_per_sec: f64) {
        let entry = self
            .net
            .entry(name.to_string())
            .or_insert_with(|| NetHistory {
                rx: Ring::new(self.capacity),
                tx: Ring::new(self.capacity),
            });
        entry.rx.push(rx_per_sec as f32);
        entry.tx.push(tx_per_sec as f32);
    }

    pub fn push_temps(&mut self, cpu: Option<f32>, gpu: Option<f32>) {
        if let Some(v) = cpu {
            self.cpu_temp.push(v);
        }
        if let Some(v) = gpu {
            self.gpu_temp.push(v);
        }
    }

    pub fn push_power(&mut self, cpu: Option<f32>, gpu: Option<f32>, gpu_usage: Option<f32>) {
        if let Some(v) = cpu {
            self.cpu_power.push(v);
        }
        if let Some(v) = gpu {
            self.gpu_power.push(v);
        }
        if let Some(v) = gpu_usage {
            self.gpu_usage.push(v);
        }
    }

    pub fn push_battery(&mut self, percent: f32) {
        self.battery_percent.push(percent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_capacity_and_order() {
        let mut ring = Ring::new(3);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            ring.push(v);
        }
        assert_eq!(ring.to_vec(), vec![3.0, 4.0, 5.0]);
        assert_eq!(ring.latest(), Some(5.0));
        assert_eq!(ring.max(), Some(5.0));
        assert_eq!(ring.min(), Some(3.0));
    }

    #[test]
    fn cpu_cores_resize_on_core_count_change() {
        let mut history = History::new(4);
        history.push_cpu(10.0, &[1.0, 2.0]);
        history.push_cpu(20.0, &[3.0, 4.0, 5.0]);
        assert_eq!(history.cpu_cores.len(), 3);
        assert_eq!(history.cpu_cores[2].to_vec(), vec![5.0]);
        assert_eq!(history.cpu_total.to_vec(), vec![10.0, 20.0]);
    }
}
