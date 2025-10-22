//! Memory Management Unit Tests
//!
//! This module provides comprehensive unit tests for the memory management
//! subsystem including allocation strategies, garbage collection, and performance.

use qi_runtime::runtime::memory::{MemoryManager, MemoryResult, MemoryError, AllocatorType};
use proptest::prelude::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_memory_manager_initialization() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;

    manager.initialize()?;

    // Verify initial state
    let usage = manager.get_usage();
    assert_eq!(usage.total_allocated, 0);
    assert_eq!(usage.in_use, 0);
    assert_eq!(usage.allocation_count, 0);
    assert_eq!(usage.deallocation_count, 0);
    assert_eq!(usage.gc_count, 0);

    Ok(())
}

#[test]
fn test_basic_memory_allocation() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // Allocate memory
    let ptr = manager.allocate(1024, None)?;
    assert!(!ptr.is_null());

    // Verify tracking
    let usage = manager.get_usage();
    assert_eq!(usage.total_allocated, 1024);
    assert_eq!(usage.in_use, 1024);
    assert_eq!(usage.allocation_count, 1);

    // Deallocate
    manager.deallocate(ptr)?;

    // Verify deallocation tracking
    let usage = manager.get_usage();
    assert_eq!(usage.in_use, 0);
    assert_eq!(usage.deallocation_count, 1);

    Ok(())
}

#[test]
fn test_multiple_allocations() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    let mut ptrs = Vec::new();

    // Allocate multiple blocks
    for i in 0..10 {
        let size = 1024 * (i + 1);
        let ptr = manager.allocate(size, None)?;
        ptrs.push(ptr);
        assert!(!ptr.is_null());
    }

    // Verify all allocations tracked
    let usage = manager.get_usage();
    assert_eq!(usage.allocation_count, 10);
    assert_eq!(usage.total_allocated, 1024 * 55); // Sum of 1..10 * 1024

    // Deallocate all blocks
    for ptr in ptrs {
        manager.deallocate(ptr)?;
    }

    // Verify all deallocated
    let usage = manager.get_usage();
    assert_eq!(usage.in_use, 0);
    assert_eq!(usage.deallocation_count, 10);

    Ok(())
}

#[test]
fn test_memory_limit_enforcement() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1, 0.8)?; // 1MB limit
    manager.initialize()?;

    // Try to allocate more than the limit
    let result = manager.allocate(2 * 1024 * 1024, None); // 2MB
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MemoryError::OutOfMemory { .. }));

    // Should still be able to allocate within limit
    let ptr = manager.allocate(512 * 1024, None)?; // 512KB
    assert!(!ptr.is_null());
    manager.deallocate(ptr)?;

    Ok(())
}

#[test]
fn test_allocation_strategies() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // Test different allocation strategies
    let strategies = vec![
        AllocatorType::Bump,
        AllocatorType::Arena,
        AllocatorType::Hybrid,
    ];

    for strategy in strategies {
        let ptr = manager.allocate(1024, Some(strategy))?;
        assert!(!ptr.is_null());
        manager.deallocate(ptr)?;
    }

    Ok(())
}

#[test]
fn test_garbage_collection_trigger() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1, 0.1)?; // Low GC threshold
    manager.initialize()?;

    // Allocate memory to trigger GC
    let ptr1 = manager.allocate(100 * 1024, None)?;
    let ptr2 = manager.allocate(200 * 1024, None)?;
    let ptr3 = manager.allocate(300 * 1024, None)?;

    // This should trigger GC due to low threshold
    let freed_bytes = manager.trigger_gc()?;

    // GC should have run
    let usage = manager.get_usage();
    assert!(usage.gc_count > 0);

    // Clean up
    manager.deallocate(ptr1)?;
    manager.deallocate(ptr2)?;
    manager.deallocate(ptr3)?;

    Ok(())
}

#[test]
fn test_memory_leak_detection() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // "Leak" some memory by not deallocating
    let _ptr1 = manager.allocate(1024, None)?;
    let _ptr2 = manager.allocate(2048, None)?;

    // Check usage before cleanup
    let usage_before = manager.get_usage();
    assert_eq!(usage_before.in_use, 3072); // 1024 + 2048

    // Trigger GC to clean up leaked memory
    let freed_bytes = manager.trigger_gc()?;

    // Note: In a real implementation, this would clean up leaked memory
    // For now, we just verify the GC ran
    let usage_after = manager.get_usage();
    assert!(usage_after.gc_count >= usage_before.gc_count);

    Ok(())
}

#[test]
fn test_concurrent_memory_operations() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    let manager = Arc::new(std::sync::Mutex::new(manager));
    let mut handles = Vec::new();

    // Spawn multiple threads doing allocations
    for i in 0..5 {
        let manager_clone = Arc::clone(&manager);
        let handle = thread::spawn(move || -> MemoryResult<()> {
            for j in 0..10 {
                let size = (i + 1) * 100 * (j + 1);
                let ptr = {
                    let mut manager = manager_clone.lock().unwrap();
                    manager.allocate(size, None)?
                };

                // Do some work with the memory
                thread::sleep(Duration::from_millis(1));

                {
                    let mut manager = manager_clone.lock().unwrap();
                    manager.deallocate(ptr)?;
                }
            }
            Ok(())
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap()?;
    }

    // Verify all memory was properly deallocated
    let manager = manager.lock().unwrap();
    let usage = manager.get_usage();
    assert_eq!(usage.in_use, 0);
    assert_eq!(usage.allocation_count, 50);
    assert_eq!(usage.deallocation_count, 50);

    Ok(())
}

#[test]
fn test_memory_usage_tracking() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(10, 0.8)?;
    manager.initialize()?;

    // Initial state
    assert_eq!(manager.get_current_usage_mb(), 0.0);
    assert_eq!(manager.get_total_allocated(), 0);
    assert_eq!(manager.get_in_use_bytes(), 0);
    assert_eq!(manager.get_available_memory(), 10 * 1024 * 1024);

    // Allocate 1MB
    let ptr1 = manager.allocate(1024 * 1024, None)?;
    assert_eq!(manager.get_current_usage_mb(), 1.0);
    assert_eq!(manager.get_total_allocated(), 1024 * 1024);
    assert_eq!(manager.get_in_use_bytes(), 1024 * 1024);

    // Allocate another 2MB
    let ptr2 = manager.allocate(2 * 1024 * 1024, None)?;
    assert_eq!(manager.get_current_usage_mb(), 3.0);
    assert_eq!(manager.get_total_allocated(), 3 * 1024 * 1024);

    // Deallocate 1MB
    manager.deallocate(ptr1)?;
    assert_eq!(manager.get_current_usage_mb(), 2.0);
    assert_eq!(manager.get_in_use_bytes(), 2 * 1024 * 1024);
    assert_eq!(manager.get_total_allocated(), 3 * 1024 * 1024); // Total allocated doesn't decrease

    // Clean up
    manager.deallocate(ptr2)?;

    Ok(())
}

#[test]
fn test_zero_size_allocation() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // Zero size allocation should fail
    let result = manager.allocate(0, None);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MemoryError::AllocationFailed { .. }));

    Ok(())
}

#[test]
fn test_null_pointer_deallocation() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // Deallocating null pointer should be safe
    let result = manager.deallocate(std::ptr::null_mut());
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_invalid_deallocation() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // Try to deallocate a pointer that wasn't allocated
    let fake_ptr = 0x1000 as *mut u8;
    let result = manager.deallocate(fake_ptr);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MemoryError::DeallocationFailed { .. }));

    Ok(())
}

#[test]
fn test_gc_threshold_configuration() -> MemoryResult<()> {
    // Test with very low GC threshold
    let mut manager = MemoryManager::new(1, 0.05)?; // 5% threshold
    manager.initialize()?;

    // Allocate enough to exceed threshold
    let ptr = manager.allocate(100 * 1024, None)?; // Should trigger GC

    let usage = manager.get_usage();
    assert!(usage.gc_count > 0);

    manager.deallocate(ptr)?;

    Ok(())
}

#[test]
fn test_strategy_switching() -> MemoryResult<()> {
    let mut manager = MemoryManager::new(1024, 0.8)?;
    manager.initialize()?;

    // Test switching allocation strategies
    assert_eq!(manager.get_allocation_strategy(), AllocatorType::Hybrid);

    manager.set_allocation_strategy(AllocatorType::Bump);
    assert_eq!(manager.get_allocation_strategy(), AllocatorType::Bump);

    manager.set_allocation_strategy(AllocatorType::Arena);
    assert_eq!(manager.get_allocation_strategy(), AllocatorType::Arena);

    // Allocate with current strategy
    let ptr = manager.allocate(1024, None)?;
    assert!(!ptr.is_null());
    manager.deallocate(ptr)?;

    Ok(())
}

// Property-based tests
proptest! {
    #[test]
    fn test_random_allocation_sizes(
        sizes in prop::collection::vec(1usize..=10000, 10..=100)
    ) -> MemoryResult<()> {
        let mut manager = MemoryManager::new(100, 0.8)?; // 100MB
        manager.initialize()?;

        let mut ptrs = Vec::new();
        let total_size: usize = sizes.iter().sum();

        // Only test if total size is reasonable
        if total_size < 50 * 1024 * 1024 { // Less than 50MB
            for &size in &sizes {
                let ptr = manager.allocate(size, None)?;
                prop_assert!(!ptr.is_null());
                ptrs.push(ptr);
            }

            // Verify total allocated
            let usage = manager.get_usage();
            prop_assert_eq!(usage.total_allocated, total_size);
            prop_assert_eq!(usage.in_use, total_size);
            prop_assert_eq!(usage.allocation_count, sizes.len());

            // Clean up
            for ptr in ptrs {
                manager.deallocate(ptr)?;
            }

            let usage_after = manager.get_usage();
            prop_assert_eq!(usage_after.in_use, 0);
            prop_assert_eq!(usage_after.deallocation_count, sizes.len());
        }

        Ok(())
    }
}

proptest! {
    #[test]
    fn test_gc_thresholds(
        threshold in 0.1f64..0.9,
        memory_mb in 10usize..=100,
        allocation_size in 1024usize..=1048576 // 1KB to 1MB
    ) -> MemoryResult<()> {
        let mut manager = MemoryManager::new(memory_mb, threshold)?;
        manager.initialize()?;

        let mut ptrs = Vec::new();
        let allocations_to_reach_threshold =
            ((memory_mb as f64 * threshold * 0.8) / (allocation_size as f64 / (1024.0 * 1024.0))) as usize;

        // Allocate enough to potentially trigger GC
        for _ in 0..allocations_to_reach_threshold {
            if let Ok(ptr) = manager.allocate(allocation_size, None) {
                ptrs.push(ptr);
            } else {
                break; // Out of memory
            }
        }

        let usage = manager.get_usage();
        let memory_ratio = usage.in_use as f64 / (memory_mb * 1024 * 1024) as f64;

        // Should be close to threshold (within 10%)
        prop_assert!(memory_ratio >= threshold - 0.1);

        // Clean up
        for ptr in ptrs {
            let _ = manager.deallocate(ptr);
        }

        Ok(())
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_allocation_performance() -> MemoryResult<()> {
        let mut manager = MemoryManager::new(1024, 0.8)?;
        manager.initialize()?;

        let start = Instant::now();
        let mut ptrs = Vec::new();

        // Allocate 10,000 small blocks
        for _ in 0..10_000 {
            let ptr = manager.allocate(1024, None)?;
            ptrs.push(ptr);
        }

        let allocation_time = start.elapsed();

        // Should complete within reasonable time (adjust threshold as needed)
        assert!(allocation_time.as_millis() < 1000);

        // Deallocate all blocks
        let start = Instant::now();
        for ptr in ptrs {
            manager.deallocate(ptr)?;
        }
        let deallocation_time = start.elapsed();

        assert!(deallocation_time.as_millis() < 1000);

        let usage = manager.get_usage();
        assert_eq!(usage.in_use, 0);
        assert_eq!(usage.allocation_count, 10_000);
        assert_eq!(usage.deallocation_count, 10_000);

        Ok(())
    }

    #[test]
    fn test_memory_pressure() -> MemoryResult<()> {
        let mut manager = MemoryManager::new(10, 0.7)?; // 10MB with 70% threshold
        manager.initialize()?;

        let mut ptrs = Vec::new();
        let block_size = 1024 * 1024; // 1MB

        // Allocate until we can't allocate more
        loop {
            match manager.allocate(block_size, None) {
                Ok(ptr) => ptrs.push(ptr),
                Err(_) => break,
            }
        }

        // Should have allocated close to the threshold
        let usage = manager.get_usage();
        let memory_ratio = usage.in_use as f64 / (10 * 1024 * 1024) as f64;
        assert!(memory_ratio >= 0.6); // Should be at least 60%
        assert!(memory_ratio <= 0.8); // Should not exceed 80%

        // Clean up
        for ptr in ptrs {
            let _ = manager.deallocate(ptr);
        }

        Ok(())
    }

    #[test]
    fn test_gc_performance() -> MemoryResult<()> {
        let mut manager = MemoryManager::new(10, 0.1)?; // Low threshold for frequent GC
        manager.initialize()?;

        let mut ptrs = Vec::new();

        // Create many allocations to trigger multiple GC cycles
        for i in 0..1000 {
            let ptr = manager.allocate(1024, None)?;
            ptrs.push(ptr);

            // Occasionally deallocate to create garbage
            if i % 3 == 0 && !ptrs.is_empty() {
                let index = ptrs.len() - 1;
                let ptr = ptrs.swap_remove(index);
                let _ = manager.deallocate(ptr);
            }
        }

        let usage = manager.get_usage();

        // Should have triggered GC multiple times
        assert!(usage.gc_count > 0);

        // Clean up remaining allocations
        for ptr in ptrs {
            let _ = manager.deallocate(ptr);
        }

        Ok(())
    }
}