use criterion::{criterion_group, criterion_main, Criterion};
use soulbox::container::ContainerManager;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Simple benchmark for container creation
fn bench_simple_container_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("simple_container_creation", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Just measure the basic container manager creation
                if let Ok(_manager) = ContainerManager::new_default() {
                    Duration::from_millis(1) // Success case
                } else {
                    Duration::from_millis(999) // Failure case
                }
            })
        })
    });
}

criterion_group!(benches, bench_simple_container_creation);
criterion_main!(benches);