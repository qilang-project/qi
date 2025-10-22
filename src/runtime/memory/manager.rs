//! Memory Manager Implementation
//!
//! This module provides the core memory management functionality for the Qi runtime,
//! including allocation strategies, garbage collection, and memory tracking.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use super::{MemoryResult, MemoryError, MemoryUsage, AllocatorType, GcConfig};

/// Main memory manager for the Qi runtime
#[derive(Debug)]
pub struct MemoryManager {
    /// Current memory usage statistics
    usage: Arc<Mutex<MemoryUsage>>,
    /// Allocation strategy configuration
    allocation_strategy: AllocatorType,
    /// Garbage collection configuration
    gc_config: GcConfig,
    /// Maximum memory limit in bytes
    max_memory_bytes: usize,
    /// Memory allocation tracking
    allocations: Arc<Mutex<HashMap<*const u8, AllocationInfo>>>,
    /// GC trigger threshold (0.0-1.0)
    gc_threshold: f64,
}

/// Information about a memory allocation
#[derive(Debug, Clone)]
struct AllocationInfo {
    /// Size of the allocation in bytes
    size: usize,
    /// When the allocation was made
    timestamp: std::time::Instant,
    /// Allocation type
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

        Ok(Self {
            usage: Arc::new(Mutex::new(MemoryUsage::new())),
            allocation_strategy: AllocatorType::Hybrid,
            gc_config: GcConfig::default(),
            max_memory_bytes,
            allocations: Arc::new(Mutex::new(HashMap::new())),
            gc_threshold: gc_threshold.clamp(0.1, 0.9),
        })
    }

    /// Initialize the memory manager
    pub fn initialize(&mut self) -> MemoryResult<()> {
        let mut usage = self.usage.lock().unwrap();
        *usage = MemoryUsage::new();

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

        // Check memory limits
        let current_usage = {
            let usage = self.usage.lock().unwrap();
            usage.in_use
        };

        if current_usage + size > self.max_memory_bytes {
            return Err(MemoryError::OutOfMemory { size });
        }

        // Perform allocation
        let allocator_type = strategy.unwrap_or(self.allocation_strategy);
        let ptr = unsafe { self.allocate_raw(size, allocator_type)? };

        // Track allocation
        let allocation_info = AllocationInfo {
            size,
            timestamp: std::time::Instant::now(),
            allocator_type,
        };

        {
            let mut allocations = self.allocations.lock().unwrap();
            allocations.insert(ptr as *const u8, allocation_info);
        }

        {
            let mut usage = self.usage.lock().unwrap();
            usage.record_allocation(size);
        }

        // Check if GC should be triggered
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

        // Get allocation info
        let allocation_info = {
            let mut allocations = self.allocations.lock().unwrap();
            allocations.remove(&(ptr as *const u8))
        };

        if let Some(info) = allocation_info {
            // Update statistics
            {
                let mut usage = self.usage.lock().unwrap();
                usage.record_deallocation(info.size);
            }

            // Perform actual deallocation
            unsafe { self.deallocate_raw(ptr, info.allocator_type)? };

            Ok(())
        } else {
            Err(MemoryError::DeallocationFailed { address: ptr as *const u8 })
        }
    }

    /// Trigger garbage collection
    pub fn trigger_gc(&mut self) -> MemoryResult<usize> {
        // For now, implement a simple GC that frees any leaked allocations
        // In a real implementation, this would be much more sophisticated

        let mut freed_bytes = 0;
        let current_time = std::time::Instant::now();

        // Find old allocations (older than 1 minute for demo)
        let old_allocations: Vec<*const u8> = {
            let allocations = self.allocations.lock().unwrap();
            allocations
                .iter()
                .filter(|(_, info)| current_time.duration_since(info.timestamp) > std::time::Duration::from_secs(60))
                .map(|(ptr, _)| *ptr)
                .collect()
        };

        // Free old allocations
        for ptr in old_allocations {
            if let Some(info) = {
                let mut allocations = self.allocations.lock().unwrap();
                allocations.remove(&ptr)
            } {
                unsafe { self.deallocate_raw(ptr as *mut u8, info.allocator_type)? };
                freed_bytes += info.size;

                let mut usage = self.usage.lock().unwrap();
                usage.record_deallocation(info.size);
            }
        }

        {
            let mut usage = self.usage.lock().unwrap();
            usage.record_gc(freed_bytes);
        }

        Ok(freed_bytes)
    }

    /// Get current memory usage in megabytes
    pub fn get_current_usage_mb(&self) -> f64 {
        let usage = self.usage.lock().unwrap();
        usage.usage_mb()
    }

    /// Get total allocated bytes
    pub fn get_total_allocated(&self) -> usize {
        let usage = self.usage.lock().unwrap();
        usage.total_allocated
    }

    /// Get currently in-use bytes
    pub fn get_in_use_bytes(&self) -> usize {
        let usage = self.usage.lock().unwrap();
        usage.in_use
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

    /// Check if garbage collection should be triggered
    fn should_trigger_gc(&self) -> bool {
        let usage = self.usage.lock().unwrap();
        let memory_ratio = usage.in_use as f64 / self.max_memory_bytes as f64;
        memory_ratio > self.gc_threshold
    }

    /// Low-level raw allocation
    unsafe fn allocate_raw(&self, size: usize, _strategy: AllocatorType) -> MemoryResult<*mut u8> {
        let layout = std::alloc::Layout::from_size_align(
            size,
            std::mem::align_of::<u8>()
        ).map_err(|_| MemoryError::AllocationFailed {
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

    /// Low-level raw deallocation
    unsafe fn deallocate_raw(&self, ptr: *mut u8, _strategy: AllocatorType) -> MemoryResult<()> {
        if ptr.is_null() {
            return Ok(());
        }

        // Note: In a real implementation, we'd need to track the original layout
        // For now, we'll use a simple approach
        let layout = std::alloc::Layout::from_size_align(
            1, // Minimum size
            std::mem::align_of::<u8>()
        ).unwrap(); // This should never fail for size 1

        std::alloc::dealloc(ptr, layout);
        Ok(())
    }
}

impl Drop for MemoryManager {
    fn drop(&mut self) {
        // Cleanup any remaining allocations
        let _ = self.trigger_gc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_manager_creation() {
        let manager = MemoryManager::new(1024, 0.8);
        assert!(manager.is_ok());

        let manager = manager.unwrap();
        assert_eq!(manager.max_memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(manager.gc_threshold, 0.8);
    }

    #[test]
    fn test_memory_allocation() {
        let mut manager = MemoryManager::new(1024, 0.8).unwrap();
        manager.initialize().unwrap();

        let ptr = manager.allocate(1024, None);
        assert!(ptr.is_ok());

        let ptr = ptr.unwrap();
        assert!(!ptr.is_null());

        let usage = manager.get_usage();
        assert_eq!(usage.total_allocated, 1024);
        assert_eq!(usage.in_use, 1024);
    }

    #[test]
    fn test_memory_deallocation() {
        let mut manager = MemoryManager::new(1024, 0.8).unwrap();
        manager.initialize().unwrap();

        let ptr = manager.allocate(1024, None).unwrap();
        let result = manager.deallocate(ptr);
        assert!(result.is_ok());

        let usage = manager.get_usage();
        assert_eq!(usage.in_use, 0);
        assert_eq!(usage.deallocation_count, 1);
    }

    #[test]
    fn test_memory_limits() {
        let mut manager = MemoryManager::new(1, 0.8).unwrap(); // 1 MB limit
        manager.initialize().unwrap();

        // Try to allocate more than the limit
        let result = manager.allocate(2 * 1024 * 1024, None); // 2 MB
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MemoryError::OutOfMemory { .. }));
    }

    #[test]
    fn test_zero_allocation() {
        let mut manager = MemoryManager::new(1024, 0.8).unwrap();
        manager.initialize().unwrap();

        let result = manager.allocate(0, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_null_deallocation() {
        let mut manager = MemoryManager::new(1024, 0.8).unwrap();
        manager.initialize().unwrap();

        let result = manager.deallocate(std::ptr::null_mut());
        assert!(result.is_ok());
    }

    #[test]
    fn test_gc_trigger() {
        let mut manager = MemoryManager::new(1, 0.1).unwrap(); // Low threshold
        manager.initialize().unwrap();

        // Allocate enough to trigger GC
        let ptr1 = manager.allocate(100 * 1024, None).unwrap();
        let ptr2 = manager.allocate(100 * 1024, None).unwrap();

        let freed = manager.trigger_gc();
        assert!(freed.is_ok());

        // Clean up
        let _ = manager.deallocate(ptr1);
        let _ = manager.deallocate(ptr2);
    }

    #[test]
    fn test_memory_usage_tracking() {
        let mut manager = MemoryManager::new(1024, 0.8).unwrap();
        manager.initialize().unwrap();

        assert_eq!(manager.get_current_usage_mb(), 0.0);

        let ptr = manager.allocate(1024 * 1024, None).unwrap(); // 1 MB
        assert_eq!(manager.get_current_usage_mb(), 1.0);

        let _ = manager.deallocate(ptr);
        assert_eq!(manager.get_current_usage_mb(), 0.0);
    }
}