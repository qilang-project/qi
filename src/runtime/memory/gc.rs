//! Garbage Collection Implementation
//!
//! This module provides tracing garbage collection primitives for the Qi runtime.
//! The collector is an exact mark-and-sweep collector over an explicit object
//! graph maintained by the runtime.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use super::{MemoryResult};

/// Garbage collection strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcStrategy {
    /// Traditional mark-and-sweep algorithm
    MarkAndSweep,
    /// Reference-counting compatibility mode
    ReferenceCounting,
    /// Generational GC placeholder
    Generational,
}

/// Garbage collection configuration
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// GC strategy to use
    pub strategy: GcStrategy,
    /// Maximum pause time in milliseconds
    pub max_pause_time_ms: u64,
    /// Collection frequency threshold
    pub collection_threshold: f64,
    /// Enable incremental collection
    pub incremental: bool,
    /// Enable parallel collection
    pub parallel: bool,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            strategy: GcStrategy::MarkAndSweep,
            max_pause_time_ms: 100,
            collection_threshold: 0.8,
            incremental: false,
            parallel: false,
        }
    }
}

/// Garbage collection statistics
#[derive(Debug, Clone, Default)]
pub struct GcStats {
    pub collections_performed: u64,
    pub objects_collected: u64,
    pub bytes_collected: u64,
    pub avg_collection_time_ms: f64,
    pub max_collection_time_ms: u64,
    pub collection_efficiency: f64,
}

impl GcStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_collection(&mut self, objects: u64, bytes: u64, time_ms: f64) {
        self.collections_performed += 1;
        self.objects_collected += objects;
        self.bytes_collected += bytes;

        if self.collections_performed == 1 {
            self.avg_collection_time_ms = time_ms;
        } else {
            self.avg_collection_time_ms =
                (self.avg_collection_time_ms * (self.collections_performed - 1) as f64 + time_ms)
                    / self.collections_performed as f64;
        }

        self.max_collection_time_ms = self.max_collection_time_ms.max(time_ms as u64);

        if time_ms > 0.0 {
            self.collection_efficiency = bytes as f64 / time_ms;
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Result of a garbage collection operation
#[derive(Debug, Clone)]
pub struct GcResult {
    pub objects_collected: u64,
    pub bytes_collected: u64,
    pub cycle_number: u64,
    pub strategy_used: GcStrategy,
    pub collected_objects: Vec<*const u8>,
}

impl GcResult {
    pub fn is_success(&self) -> bool {
        self.objects_collected > 0 || self.bytes_collected > 0
    }

    pub fn efficiency(&self) -> f64 {
        if self.cycle_number == 0 {
            0.0
        } else {
            self.bytes_collected as f64 / self.cycle_number as f64
        }
    }
}

/// Garbage collector implementation
#[derive(Debug)]
pub struct GarbageCollector {
    config: GcConfig,
    stats: Arc<Mutex<GcStats>>,
    roots: Arc<Mutex<HashSet<*const u8>>>,
    references: Arc<Mutex<HashMap<*const u8, HashSet<*const u8>>>>,
    marked_objects: Arc<Mutex<HashSet<*const u8>>>,
    current_cycle: u64,
}

impl GarbageCollector {
    pub fn new(config: GcConfig) -> Self {
        Self {
            config,
            stats: Arc::new(Mutex::new(GcStats::new())),
            roots: Arc::new(Mutex::new(HashSet::new())),
            references: Arc::new(Mutex::new(HashMap::new())),
            marked_objects: Arc::new(Mutex::new(HashSet::new())),
            current_cycle: 0,
        }
    }

    pub fn initialize(&mut self) -> MemoryResult<()> {
        self.stats.lock().unwrap().reset();
        self.roots.lock().unwrap().clear();
        self.references.lock().unwrap().clear();
        self.marked_objects.lock().unwrap().clear();
        self.current_cycle = 0;
        Ok(())
    }

    pub fn add_root(&self, ptr: *const u8) -> MemoryResult<()> {
        if !ptr.is_null() {
            self.roots.lock().unwrap().insert(ptr);
        }
        Ok(())
    }

    pub fn remove_root(&self, ptr: *const u8) -> MemoryResult<()> {
        self.roots.lock().unwrap().remove(&ptr);
        Ok(())
    }

    pub fn set_references(&self, ptr: *const u8, refs: Vec<*const u8>) -> MemoryResult<()> {
        if ptr.is_null() {
            return Ok(());
        }

        let mut refs_map = self.references.lock().unwrap();
        let entry = refs_map.entry(ptr).or_insert_with(HashSet::new);
        entry.clear();
        for r in refs {
            if !r.is_null() {
                entry.insert(r);
            }
        }
        Ok(())
    }

    pub fn add_reference(&self, from: *const u8, to: *const u8) -> MemoryResult<()> {
        if from.is_null() || to.is_null() {
            return Ok(());
        }
        self.references
            .lock()
            .unwrap()
            .entry(from)
            .or_insert_with(HashSet::new)
            .insert(to);
        Ok(())
    }

    pub fn clear_references(&self, ptr: *const u8) -> MemoryResult<()> {
        self.references.lock().unwrap().remove(&ptr);
        Ok(())
    }

    pub fn forget_object(&self, ptr: *const u8) -> MemoryResult<()> {
        self.remove_root(ptr)?;
        self.clear_references(ptr)?;
        let mut refs_map = self.references.lock().unwrap();
        for refs in refs_map.values_mut() {
            refs.remove(&ptr);
        }
        Ok(())
    }

    pub fn roots_snapshot(&self) -> HashSet<*const u8> {
        self.roots.lock().unwrap().clone()
    }

    pub fn references_snapshot(&self) -> HashMap<*const u8, HashSet<*const u8>> {
        self.references.lock().unwrap().clone()
    }

    pub fn get_stats(&self) -> GcStats {
        self.stats.lock().unwrap().clone()
    }

    pub fn current_cycle(&self) -> u64 {
        self.current_cycle
    }

    pub fn collect(&mut self, heap: &HashMap<*const u8, usize>) -> MemoryResult<GcResult> {
        let start_time = std::time::Instant::now();
        self.current_cycle += 1;

        self.marked_objects.lock().unwrap().clear();
        self.mark_phase()?;

        let collected_objects = self.sweep_candidates(heap);
        let bytes_collected: u64 = collected_objects
            .iter()
            .filter_map(|ptr| heap.get(ptr).copied())
            .map(|size| size as u64)
            .sum();

        let elapsed = start_time.elapsed();
        let time_ms = elapsed.as_millis() as f64;

        let result = GcResult {
            objects_collected: collected_objects.len() as u64,
            bytes_collected,
            cycle_number: self.current_cycle,
            strategy_used: self.config.strategy,
            collected_objects,
        };

        self.stats
            .lock()
            .unwrap()
            .record_collection(result.objects_collected, result.bytes_collected, time_ms);

        Ok(result)
    }

    fn mark_phase(&self) -> MemoryResult<()> {
        let roots: Vec<*const u8> = self.roots.lock().unwrap().iter().copied().collect();
        let mut marked = self.marked_objects.lock().unwrap();
        for root in roots {
            self.mark_object(root, &mut marked);
        }
        Ok(())
    }

    fn mark_object(&self, obj: *const u8, marked: &mut HashSet<*const u8>) {
        if obj.is_null() || marked.contains(&obj) {
            return;
        }

        marked.insert(obj);

        let children: Vec<*const u8> = self
            .references
            .lock()
            .unwrap()
            .get(&obj)
            .map(|refs| refs.iter().copied().collect())
            .unwrap_or_default();

        for child in children {
            self.mark_object(child, marked);
        }
    }

    fn sweep_candidates(&self, heap: &HashMap<*const u8, usize>) -> Vec<*const u8> {
        let marked = self.marked_objects.lock().unwrap();
        let roots = self.roots.lock().unwrap();

        heap.keys()
            .copied()
            .filter(|ptr| !marked.contains(ptr) && !roots.contains(ptr))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gc_config_default() {
        let config = GcConfig::default();
        assert_eq!(config.strategy, GcStrategy::MarkAndSweep);
        assert_eq!(config.max_pause_time_ms, 100);
        assert_eq!(config.collection_threshold, 0.8);
    }

    #[test]
    fn test_gc_creation_and_init() {
        let mut gc = GarbageCollector::new(GcConfig::default());
        assert_eq!(gc.current_cycle(), 0);
        gc.initialize().unwrap();
        assert_eq!(gc.current_cycle(), 0);
    }

    #[test]
    fn test_root_and_reference_graph() {
        let gc = GarbageCollector::new(GcConfig::default());
        let a = 0x1000 as *const u8;
        let b = 0x2000 as *const u8;

        gc.add_root(a).unwrap();
        gc.add_reference(a, b).unwrap();

        assert!(gc.roots_snapshot().contains(&a));
        assert!(gc.references_snapshot().get(&a).unwrap().contains(&b));
    }

    #[test]
    fn test_mark_and_sweep_collects_unreachable() {
        let mut gc = GarbageCollector::new(GcConfig::default());
        gc.initialize().unwrap();

        let a = 0x1000 as *const u8;
        let b = 0x2000 as *const u8;
        let c = 0x3000 as *const u8;

        gc.add_root(a).unwrap();
        gc.add_reference(a, b).unwrap();

        let mut heap = HashMap::new();
        heap.insert(a, 64);
        heap.insert(b, 64);
        heap.insert(c, 64);

        let result = gc.collect(&heap).unwrap();
        assert_eq!(result.objects_collected, 1);
        assert_eq!(result.bytes_collected, 64);
        assert_eq!(result.collected_objects, vec![c]);
    }

    #[test]
    fn test_forget_object() {
        let gc = GarbageCollector::new(GcConfig::default());
        let a = 0x1000 as *const u8;
        let b = 0x2000 as *const u8;

        gc.add_root(a).unwrap();
        gc.add_reference(a, b).unwrap();
        gc.forget_object(b).unwrap();

        let refs = gc.references_snapshot();
        assert!(!refs.get(&a).map(|s| s.contains(&b)).unwrap_or(false));
    }
}
