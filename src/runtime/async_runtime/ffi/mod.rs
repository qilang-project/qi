//! FFI bindings for C syscalls and async runtime interface
//!
//! This module provides the interface to low-level C syscalls for platform-specific
//! async I/O operations, as well as C interface functions for the async runtime.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::collections::HashMap;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::future::Future;
use std::pin::Pin;

pub mod syscalls;

pub use syscalls::{EpollEvent, EventType, SyscallResult};

/// Task handle for C interface
pub type TaskHandle = *mut c_void;

/// Simple task storage for C interface
static TASK_STORE: OnceLock<Mutex<HashMap<u64, Pin<Box<dyn Future<Output = *mut c_void> + Send + 'static>>>>> = OnceLock::new();
static mut NEXT_TASK_ID: u64 = 1;
static RUNTIME_INIT: OnceLock<()> = OnceLock::new();

/// Check if debug mode is enabled
fn debug_enabled() -> bool {
    std::env::var("QI_DEBUG").is_ok() || std::env::var("QI_DEBUG_RUNTIME").is_ok()
}

/// Initialize the async runtime for C interface
fn ensure_runtime_initialized() {
    RUNTIME_INIT.get_or_init(|| {
        // Initialize the task store
        let _ = TASK_STORE.set(Mutex::new(HashMap::new()));
    });
}

/// Create a new async task
#[no_mangle]
pub extern "C" fn qi_runtime_create_task(function_ptr: *const c_void, arg_count: i64) -> TaskHandle {
    ensure_runtime_initialized();
    if debug_enabled() {
        eprintln!("DEBUG: create_task called");
    }

    let task_id = unsafe {
        let id = NEXT_TASK_ID;
        NEXT_TASK_ID += 1;
        id
    };
    if debug_enabled() {
        eprintln!("DEBUG: task_id = {}", task_id);
    }

    // Convert function_ptr to usize to make it Send
    let function_addr = function_ptr as usize;

    // Create a future that calls the async function and returns its result
    let future = async move {
        if debug_enabled() {
            eprintln!("DEBUG: Inside async block");
        }
        // Call the async function and return its result
        unsafe {
            let func = std::mem::transmute::<usize, extern "C" fn() -> *const c_void>(function_addr);
            if debug_enabled() {
                eprintln!("DEBUG: About to call function");
            }
            let result = func();
            if debug_enabled() {
                eprintln!("DEBUG: Function returned {:?}", result);
            }

            // Allocate memory to store the result pointer so caller can load from it
            let result_ptr = Box::into_raw(Box::new(result)) as *mut c_void;
            if debug_enabled() {
                eprintln!("DEBUG: Returning result_ptr {:?}", result_ptr);
            }
            result_ptr
        }
    };

    // Store the future
    if let Some(store) = TASK_STORE.get() {
        if let Ok(mut store_guard) = store.lock() {
            store_guard.insert(task_id, Box::pin(future));
            if debug_enabled() {
                eprintln!("DEBUG: Future stored with task_id {}", task_id);
            }
        }
    }

    if debug_enabled() {
        eprintln!("DEBUG: create_task returning {}", task_id);
    }
    task_id as TaskHandle
}

/// Await the completion of an async task
#[no_mangle]
pub extern "C" fn qi_runtime_await(task: TaskHandle) -> *mut c_void {
    ensure_runtime_initialized();
    if debug_enabled() {
        eprintln!("DEBUG: await called with task {:?}", task);
    }

    let task_id = task as u64;

    // Try to get the future from the store
    if let Some(store) = TASK_STORE.get() {
        if debug_enabled() {
            eprintln!("DEBUG: Got task store");
        }
        if let Ok(mut store_guard) = store.lock() {
            if debug_enabled() {
                eprintln!("DEBUG: Locked store, contains {} tasks", store_guard.len());
            }
            if let Some(mut future) = store_guard.remove(&task_id) {
                if debug_enabled() {
                    eprintln!("DEBUG: Found future for task {}", task_id);
                }
                // Create a waker that does nothing - our futures are immediately ready
                // since they just wrap synchronous function calls
                use std::task::{RawWaker, RawWakerVTable, Waker};
                
                unsafe fn noop_clone(_: *const ()) -> RawWaker {
                    noop_raw_waker()
                }
                unsafe fn noop_wake(_: *const ()) {}
                unsafe fn noop_wake_by_ref(_: *const ()) {}
                unsafe fn noop_drop(_: *const ()) {}
                
                const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
                    noop_clone,
                    noop_wake,
                    noop_wake_by_ref,
                    noop_drop,
                );
                
                fn noop_raw_waker() -> RawWaker {
                    RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
                }
                
                let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
                let mut context = Context::from_waker(&waker);

                // Poll the future - it should be immediately ready since async {} blocks
                // without await points complete on first poll
                if debug_enabled() {
                    eprintln!("DEBUG: About to poll future");
                }
                match Pin::new(&mut future).poll(&mut context) {
                    Poll::Ready(result) => {
                        if debug_enabled() {
                            eprintln!("DEBUG: Future is Ready, returning result");
                        }
                        return result;
                    }
                    Poll::Pending => {
                        // This shouldn't happen for our simple futures, but handle it
                        if debug_enabled() {
                            eprintln!("Warning: Future returned Pending unexpectedly");
                        }
                        return std::ptr::null_mut();
                    }
                }
            } else {
                if debug_enabled() {
                    eprintln!("DEBUG: No future found for task {}", task_id);
                }
            }
        } else {
            if debug_enabled() {
                eprintln!("DEBUG: Failed to lock store");
            }
        }
    } else {
        if debug_enabled() {
            eprintln!("DEBUG: No task store");
        }
    }

    if debug_enabled() {
        eprintln!("DEBUG: await returning null");
    }
    std::ptr::null_mut()
}

/// Spawn an async task to start execution
#[no_mangle]
pub extern "C" fn qi_runtime_spawn_task(task: TaskHandle) -> i32 {
    ensure_runtime_initialized();

    // For now, just return success
    // In a real implementation, this would add the task to the executor
    0
}

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
