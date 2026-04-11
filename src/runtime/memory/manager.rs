//! Memory Manager Implementation
//!
//! This module provides the core memory management functionality for the Qi runtime,
//! including allocation tracking, tracing GC integration, and exact deallocation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{AllocatorType, GarbageCollector, GcConfig, MemoryError, MemoryResult, MemoryUsage};

/// Main memory manager for the Qi runtime
#[derive(Debug)]
pub struct MemoryManager {
    usage: Arc<Mutex<MemoryUsage>>,
    allocation_strategy: AllocatorType,
    gc_config: GcConfig,
    max_memory_bytes: usize,
    allocations: Arc<Mutex<HashMap<*const u8, AllocationInfo>>>,
    gc: GarbageCollector,
    gc_threshold: f64,
}

/// Information about a memory allocation
#[derive(Debug, Clone)]
struct AllocationInfo {
    size: usize,
    align: usize,
    allocator_type: AllocatorType,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new(max_memory_mb: usize, gc_threshold: f64) -> MemoryResult<Self> {
        if max_memory_mb == 0 {
            return Err(MemoryError::AllocationFailed {
                requested: 0,
                available: 0,
            });
        }

        let max_memory_bytes = max_memory_mb * 1024 * 1024;
        let gc_config = GcConfig::default();

        Ok(Self {
            usage: Arc::new(Mutex::new(MemoryUsage::new())),
            allocation_strategy: AllocatorType::Hybrid,
            gc_config: gc_config.clone(),
            max_memory_bytes,
            allocations: Arc::new(Mutex::new(HashMap::new())),
            gc: GarbageCollector::new(gc_config),
            gc_threshold: gc_threshold.clamp(0.1, 0.95),
        })
    }

    /// Initialize the memory manager
    pub fn initialize(&mut self) -> MemoryResult<()> {
        *self.usage.lock().unwrap() = MemoryUsage::new();
        self.allocations.lock().unwrap().clear();
        self.gc.initialize()?;
        Ok(())
    }

    /// Allocate memory with specified size and strategy
    pub fn allocate(&mut self, size: usize, strategy: Option<AllocatorType>) -> MemoryResult<*mut u8> {
        if size == 0 {
            return Err(MemoryError::AllocationFailed {
                requested: size,
                available: self.get_available_memory(),
            });
        }

        let current_usage = self.usage.lock().unwrap().in_use;
        if current_usage + size > self.max_memory_bytes {
            return Err(MemoryError::OutOfMemory { size });
        }

        let allocator_type = strategy.unwrap_or(self.allocation_strategy);
        let align = 8;
        let ptr = unsafe { self.allocate_raw(size, align)? };

        let info = AllocationInfo {
            size,
            align,
            allocator_type,
        };

        self.allocations.lock().unwrap().insert(ptr as *const u8, info);
        self.gc.add_root(ptr as *const u8)?;
        self.gc.clear_references(ptr as *const u8)?;

        self.usage.lock().unwrap().record_allocation(size);

        if self.should_trigger_gc() {
            let _ = self.trigger_gc();
        }

        Ok(ptr)
    }

    /// Deallocate memory
    pub fn deallocate(&mut self, ptr: *mut u8) -> MemoryResult<()> {
        if ptr.is_null() {
            return Ok(());
        }

        let info = self
            .allocations
            .lock()
            .unwrap()
            .remove(&(ptr as *const u8))
            .ok_or(MemoryError::DeallocationFailed {
                address: ptr as *const u8,
            })?;

        unsafe { self.deallocate_raw_with_layout(ptr, info.size, info.align)? };
        self.gc.forget_object(ptr as *const u8)?;
        self.usage.lock().unwrap().record_deallocation(info.size);

        Ok(())
    }

    /// Trigger garbage collection
    pub fn trigger_gc(&mut self) -> MemoryResult<usize> {
        let heap: HashMap<*const u8, usize> = self
            .allocations
            .lock()
            .unwrap()
            .iter()
            .map(|(ptr, info)| (*ptr, info.size))
            .collect();

        let result = self.gc.collect(&heap)?;
        let mut freed_bytes = 0usize;

        for ptr in &result.collected_objects {
            if let Some(info) = self.allocations.lock().unwrap().remove(ptr) {
                unsafe { self.deallocate_raw_with_layout(*ptr as *mut u8, info.size, info.align)? };
                self.gc.forget_object(*ptr)?;
                freed_bytes += info.size;
                self.usage.lock().unwrap().record_deallocation(info.size);
            }
        }

        // Deallocation has already reduced `in_use`; here we only want to
        // count that a GC cycle happened.
        self.usage.lock().unwrap().record_gc(0);
        Ok(freed_bytes)
    }

    /// Check if garbage collection should be triggered based on memory usage
    pub fn should_collect(&self) -> bool {
        self.get_usage_ratio() > self.gc_threshold
    }

    /// Trigger garbage collection and return bytes freed
    pub fn collect(&mut self) -> MemoryResult<usize> {
        self.trigger_gc()
    }

    /// Register an allocation as a GC root
    pub fn add_root(&mut self, ptr: *mut u8) -> MemoryResult<()> {
        self.gc.add_root(ptr as *const u8)
    }

    /// Remove an allocation from the GC root set
    pub fn remove_root(&mut self, ptr: *mut u8) -> MemoryResult<()> {
        self.gc.remove_root(ptr as *const u8)
    }

    /// Add a reference edge from one heap object to another
    pub fn add_reference(&mut self, from: *mut u8, to: *mut u8) -> MemoryResult<()> {
        self.gc.add_reference(from as *const u8, to as *const u8)
    }

    /// Replace all outgoing references for an object
    pub fn set_references(&mut self, from: *mut u8, refs: Vec<*mut u8>) -> MemoryResult<()> {
        let refs: Vec<*const u8> = refs.into_iter().map(|ptr| ptr as *const u8).collect();
        self.gc.set_references(from as *const u8, refs)
    }

    /// Clear outgoing references for an object
    pub fn clear_references(&mut self, ptr: *mut u8) -> MemoryResult<()> {
        self.gc.clear_references(ptr as *const u8)
    }

    /// Get current memory usage in megabytes
    pub fn get_current_usage_mb(&self) -> f64 {
        self.usage.lock().unwrap().usage_mb()
    }

    /// Get total allocated bytes
    pub fn get_total_allocated(&self) -> usize {
        self.usage.lock().unwrap().total_allocated
    }

    /// Get currently in-use bytes
    pub fn get_in_use_bytes(&self) -> usize {
        self.usage.lock().unwrap().in_use
    }

    /// Get available memory bytes
    pub fn get_available_memory(&self) -> usize {
        self.max_memory_bytes.saturating_sub(self.get_in_use_bytes())
    }

    /// Get memory usage statistics
    pub fn get_usage(&self) -> MemoryUsage {
        self.usage.lock().unwrap().clone()
    }

    /// Set allocation strategy
    pub fn set_allocation_strategy(&mut self, strategy: AllocatorType) {
        self.allocation_strategy = strategy;
    }

    /// Get allocation strategy
    pub fn get_allocation_strategy(&self) -> AllocatorType {
        self.allocation_strategy
    }

    fn get_usage_ratio(&self) -> f64 {
        if self.max_memory_bytes == 0 {
            0.0
        } else {
            self.get_in_use_bytes() as f64 / self.max_memory_bytes as f64
        }
    }

    fn should_trigger_gc(&self) -> bool {
        self.should_collect()
    }

    unsafe fn allocate_raw(&self, size: usize, align: usize) -> MemoryResult<*mut u8> {
        let layout = std::alloc::Layout::from_size_align(size, align).map_err(|_| MemoryError::AllocationFailed {
            requested: size,
            available: self.get_available_memory(),
        })?;

        let ptr = std::alloc::alloc(layout);
        if ptr.is_null() {
            return Err(MemoryError::AllocationFailed {
                requested: size,
                available: self.get_available_memory(),
            });
        }

        Ok(ptr)
    }

    unsafe fn deallocate_raw_with_layout(&self, ptr: *mut u8, size: usize, align: usize) -> MemoryResult<()> {
        if ptr.is_null() {
            return Ok(());
        }

        let layout = std::alloc::Layout::from_size_align(size, align).map_err(|_| MemoryError::CorruptedMemory)?;
        std::alloc::dealloc(ptr, layout);
        Ok(())
    }
}

impl Drop for MemoryManager {
    fn drop(&mut self) {
        let remaining: Vec<(*const u8, AllocationInfo)> = self
            .allocations
            .lock()
            .unwrap()
            .drain()
            .collect();

        for (ptr, info) in remaining {
            let _ = self.gc.forget_object(ptr);
            let _ = unsafe { self.deallocate_raw_with_layout(ptr as *mut u8, info.size, info.align) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_manager_creation() {
        let manager = MemoryManager::new(1024, 0.8).unwrap();
        assert_eq!(manager.max_memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(manager.gc_threshold, 0.8);
    }

    #[test]
    fn test_memory_allocation_and_deallocation() {
        let mut manager = MemoryManager::new(1024, 0.8).unwrap();
        manager.initialize().unwrap();

        let ptr = manager.allocate(1024, None).unwrap();
        assert!(!ptr.is_null());
        assert_eq!(manager.get_usage().in_use, 1024);

        manager.deallocate(ptr).unwrap();
        assert_eq!(manager.get_usage().in_use, 0);
    }

    #[test]
    fn test_memory_limits() {
        let mut manager = MemoryManager::new(1, 0.8).unwrap();
        manager.initialize().unwrap();

        let result = manager.allocate(2 * 1024 * 1024, None);
        assert!(matches!(result.unwrap_err(), MemoryError::OutOfMemory { .. }));
    }

    #[test]
    fn test_gc_collects_unreachable_allocations() {
        let mut manager = MemoryManager::new(1024, 0.1).unwrap();
        manager.initialize().unwrap();

        let root = manager.allocate(64, None).unwrap();
        let child = manager.allocate(64, None).unwrap();
        let orphan = manager.allocate(64, None).unwrap();

        manager.add_reference(root, child).unwrap();
        manager.remove_root(child).unwrap();
        manager.remove_root(orphan).unwrap();

        let freed = manager.trigger_gc().unwrap();
        assert_eq!(freed, 64);
        assert_eq!(manager.get_usage().in_use, 128);

        manager.deallocate(root).unwrap();
        manager.deallocate(child).unwrap();
    }

    #[test]
    fn test_gc_collects_root_graph_when_root_removed() {
        let mut manager = MemoryManager::new(1024, 0.1).unwrap();
        manager.initialize().unwrap();

        let root = manager.allocate(64, None).unwrap();
        let child = manager.allocate(64, None).unwrap();

        manager.add_reference(root, child).unwrap();
        manager.remove_root(child).unwrap();
        manager.remove_root(root).unwrap();

        let freed = manager.trigger_gc().unwrap();
        assert_eq!(freed, 128);
        assert_eq!(manager.get_usage().in_use, 0);
    }
}
