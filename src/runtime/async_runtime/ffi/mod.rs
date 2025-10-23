//! FFI bindings for C syscalls
//!
//! This module provides the interface to low-level C syscalls for platform-specific
//! async I/O operations.

pub mod syscalls;

pub use syscalls::{EpollEvent, EventType, SyscallResult};

/// Platform-specific I/O event loop interface
pub trait IoEventLoop {
    /// Initialize the event loop
    fn initialize(&mut self) -> SyscallResult<()>;
    
    /// Register a file descriptor for monitoring
    fn register_fd(&mut self, fd: i32, events: EventType) -> SyscallResult<()>;
    
    /// Unregister a file descriptor
    fn unregister_fd(&mut self, fd: i32) -> SyscallResult<()>;
    
    /// Wait for events with optional timeout (in milliseconds)
    fn wait_events(&mut self, timeout_ms: i32) -> SyscallResult<Vec<EpollEvent>>;
    
    /// Cleanup and shutdown the event loop
    fn shutdown(&mut self) -> SyscallResult<()>;
}

#[cfg(target_os = "linux")]
pub type PlatformEventLoop = syscalls::LinuxEpoll;

#[cfg(target_os = "macos")]
pub type PlatformEventLoop = syscalls::MacOsKqueue;

#[cfg(target_os = "windows")]
pub type PlatformEventLoop = syscalls::WindowsIocp;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub type PlatformEventLoop = syscalls::GenericEventLoop;
