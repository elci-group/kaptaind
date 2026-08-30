//! Pure supervisor planning benchmarks. They deliberately perform no network
//! access, filesystem mutation, or process creation.

use kaptaind::supervisor::model::{
    project_id_for_path, DesiredState, ProjectSpec, SupervisorSnapshot, WorkerObservation,
};
use kaptaind::supervisor::reconcile::plan_reconciliation;
use kaptaind::supervisor::store::AtomicSnapshotStore;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    divan::main();
}

fn fixture(
    size: usize,
    running_stride: usize,
) -> (SupervisorSnapshot, BTreeMap<String, WorkerObservation>) {
    let mut snapshot = SupervisorSnapshot::default();
    let mut observations = BTreeMap::new();
    for index in 0..size {
        let path = PathBuf::from(format!("/benchmark/project-{index:05}"));
        let project_id = project_id_for_path(&path);
        snapshot.projects.insert(
            project_id.clone(),
            ProjectSpec {
                project_id: project_id.clone(),
                config: path.join("kaptaind.toml"),
                path,
                desired_state: DesiredState::Enabled,
                health_port: 10_000u16.saturating_add((index % 50_000) as u16),
                revision: 1,
                updated_at_ms: 1,
                source: "benchmark".to_string(),
            },
        );
        observations.insert(
            project_id,
            if index % running_stride == 0 {
                WorkerObservation::Running {
                    pid: (index + 100) as u32,
                }
            } else {
                WorkerObservation::Stopped
            },
        );
    }
    (snapshot, observations)
}

#[divan::bench(args = [10usize, 100usize, 1_000usize, 10_000usize])]
fn plan_noop(size: usize) {
    let (snapshot, mut observations) = fixture(size, 1);
    for observation in observations.values_mut() {
        *observation = WorkerObservation::Running { pid: 42 };
    }
    divan::black_box(plan_reconciliation(&snapshot, &observations, 4));
}

#[divan::bench(args = [10usize, 100usize, 1_000usize, 10_000usize])]
fn plan_mixed(size: usize) {
    let (snapshot, observations) = fixture(size, 3);
    divan::black_box(plan_reconciliation(&snapshot, &observations, 4));
}

#[divan::bench(args = [10usize, 100usize, 1_000usize, 10_000usize])]
fn validate_snapshot(bencher: divan::Bencher, size: usize) {
    bencher
        .with_inputs(|| fixture(size, 1).0)
        .bench_refs(|snapshot| snapshot.validate().expect("valid benchmark snapshot"));
}

#[divan::bench(args = [10usize, 100usize, 1_000usize, 10_000usize])]
fn serialize_snapshot(bencher: divan::Bencher, size: usize) {
    bencher
        .with_inputs(|| fixture(size, 1).0)
        .bench_refs(|snapshot| serde_json::to_vec(snapshot).expect("serialize snapshot"));
}

#[divan::bench(args = [10usize, 100usize, 1_000usize, 10_000usize])]
fn load_snapshot(bencher: divan::Bencher, size: usize) {
    bencher
        .with_inputs(|| {
            let directory = tempfile::tempdir().expect("temporary benchmark directory");
            let store = AtomicSnapshotStore::new(directory.path().join("supervisor-state.json"));
            store.save(&fixture(size, 1).0).expect("persist fixture");
            (directory, store)
        })
        .bench_refs(|(_directory, store)| store.load().expect("load snapshot"));
}

#[divan::bench(args = [1_000usize])]
fn persist_snapshot_atomically(bencher: divan::Bencher, size: usize) {
    bencher
        .with_inputs(|| {
            let directory = tempfile::tempdir().expect("temporary benchmark directory");
            let store = AtomicSnapshotStore::new(directory.path().join("supervisor-state.json"));
            (directory, store, fixture(size, 1).0)
        })
        .bench_refs(|(_directory, store, snapshot)| {
            store.save(snapshot).expect("atomically persist snapshot")
        });
}
