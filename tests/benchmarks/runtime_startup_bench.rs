//! Runtime Startup Performance Benchmarks
//!
//! This module provides benchmarks for runtime startup time to ensure
//! the <2s startup target is met consistently.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

use qi_compiler::runtime::{RuntimeEnvironment, RuntimeConfig};

fn benchmark_runtime_startup_default(c: &mut Criterion) {
    c.bench_function("runtime_startup_default", |b| {
        b.iter(|| {
            let config = RuntimeConfig::default();
            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);
            runtime.initialize().unwrap();
            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_startup_minimal_config(c: &mut Criterion) {
    c.bench_function("runtime_startup_minimal", |b| {
        b.iter(|| {
            let config = RuntimeConfig {
                max_memory_mb: 256,
                gc_threshold_percent: 0.9,
                io_buffer_size: 4096,
                network_timeout_ms: 60000,
                debug_mode: false,
                locale: "zh-CN".to_string(),
                enable_metrics: false,
            };
            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);
            runtime.initialize().unwrap();
            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_startup_debug_mode(c: &mut Criterion) {
    c.bench_function("runtime_startup_debug", |b| {
        b.iter(|| {
            let mut config = RuntimeConfig::default();
            config.debug_mode = true;
            config.enable_metrics = true;

            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);
            runtime.initialize().unwrap();
            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_startup_large_memory(c: &mut Criterion) {
    c.bench_function("runtime_startup_large_memory", |b| {
        b.iter(|| {
            let config = RuntimeConfig {
                max_memory_mb: 4096,
                gc_threshold_percent: 0.7,
                io_buffer_size: 16384,
                network_timeout_ms: 30000,
                debug_mode: false,
                locale: "zh-CN".to_string(),
                enable_metrics: true,
            };
            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);
            runtime.initialize().unwrap();
            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_startup_multiple_instances(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_startup_multiple");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("startup_5_instances", |b| {
        b.iter(|| {
            let mut ids = Vec::new();
            for _ in 0..5 {
                let config = RuntimeConfig::default();
                let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
                let mut runtime = black_box(runtime);
                runtime.initialize().unwrap();
                ids.push(runtime.id);
            }
            black_box(ids)
        })
    });

    group.bench_function("startup_10_instances", |b| {
        b.iter(|| {
            let mut ids = Vec::new();
            for _ in 0..10 {
                let config = RuntimeConfig::default();
                let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
                let mut runtime = black_box(runtime);
                runtime.initialize().unwrap();
                ids.push(runtime.id);
            }
            black_box(ids)
        })
    });

    group.finish();
}

fn benchmark_runtime_startup_with_subsystems(c: &mut Criterion) {
    c.bench_function("runtime_startup_with_subsystems", |b| {
        b.iter(|| {
            let config = RuntimeConfig::default();
            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);

            // Initialize all subsystems explicitly
            runtime.memory_manager.initialize().unwrap();
            runtime.file_system.initialize().unwrap();
            runtime.network_manager.initialize().unwrap();
            runtime.error_handler.initialize().unwrap();

            // Set state to ready
            runtime.state = qi_compiler::runtime::RuntimeState::Ready;

            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_creation_only(c: &mut Criterion) {
    c.bench_function("runtime_creation_only", |b| {
        b.iter(|| {
            let config = RuntimeConfig::default();
            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_initialization_only(c: &mut Criterion) {
    c.bench_function("runtime_initialization_only", |b| {
        b.iter_batched(
            || {
                let config = RuntimeConfig::default();
                RuntimeEnvironment::new(config).unwrap()
            },
            |mut runtime| {
                runtime.initialize().unwrap();
                black_box(runtime.id)
            },
            criterion::BatchSize::Small
        )
    });
}

fn benchmark_runtime_startup_chinese_locale(c: &mut Criterion) {
    c.bench_function("runtime_startup_chinese_locale", |b| {
        b.iter(|| {
            let mut config = RuntimeConfig::default();
            config.locale = "zh-CN".to_string();

            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);
            runtime.initialize().unwrap();
            black_box(runtime.id)
        })
    });
}

fn benchmark_runtime_startup_with_metrics_disabled(c: &mut Criterion) {
    c.bench_function("runtime_startup_no_metrics", |b| {
        b.iter(|| {
            let mut config = RuntimeConfig::default();
            config.enable_metrics = false;

            let runtime = RuntimeEnvironment::new(black_box(config)).unwrap();
            let mut runtime = black_box(runtime);
            runtime.initialize().unwrap();
            black_box(runtime.id)
        })
    });
}

criterion_group!(
    runtime_startup_benches,
    benchmark_runtime_startup_default,
    benchmark_runtime_startup_minimal_config,
    benchmark_runtime_startup_debug_mode,
    benchmark_runtime_startup_large_memory,
    benchmark_runtime_startup_multiple_instances,
    benchmark_runtime_startup_with_subsystems,
    benchmark_runtime_creation_only,
    benchmark_runtime_initialization_only,
    benchmark_runtime_startup_chinese_locale,
    benchmark_runtime_startup_with_metrics_disabled
);

criterion_main!(runtime_startup_benches);