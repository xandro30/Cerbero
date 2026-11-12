//! T015: SC-003 for the resolution (research R2). The three refusals, and a refusal by the first
//! or by the last of ten references, are not told apart by their time: the difference between
//! the medians of two groups does not exceed the larger difference between the two halves of
//! each, in the order they were sent, plus 1 ms. It has a file of its own, so no other test runs
//! at the same time.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use common::*;
use hecate_application::identity::settings::IdentitySettings;
use hecate_application::testing::CaRig;
use hecate_infrastructure::testing::TestComponent;
use tokio::sync::Semaphore;

const PER_GROUP: usize = 1_000;
const AT_ONCE: usize = 8;
const FLOOR: StdDuration = StdDuration::from_millis(50);
const MARGIN: StdDuration = StdDuration::from_millis(1);
const UNRESOLVABLE: &str = "secret://hermes/app/old";

async fn enrolled(rig: &CaRig, dir: &tempfile::TempDir, addr: SocketAddr) -> TestComponent {
    let component = TestComponent::open(&dir.path().join("hermes"), rig.clock.clone()).unwrap();
    let token = rig
        .identities
        .tokens_create("hermes")
        .unwrap()
        .token
        .to_string();
    let reply = component.enroll(addr, &token).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    component
}

/// The order of the requests: a xorshift with a fixed seed, so that a failure can be repeated.
fn shuffled<T>(mut items: Vec<T>, mut seed: u64) -> Vec<T> {
    for i in (1..items.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = usize::try_from(seed % (i as u64 + 1)).unwrap();
        items.swap(i, j);
    }
    items
}

fn median(mut times: Vec<StdDuration>) -> StdDuration {
    times.sort();
    times[times.len() / 2]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_refusals_are_not_told_apart_by_their_time() {
    let rig = ca_rig_with(IdentitySettings {
        refusal_floor: FLOOR,
        ..IdentitySettings::default()
    });
    let (supervisor, _, addr) = listening_api(&rig).await;
    let keys: Vec<String> = (0..9)
        .map(|n| format!("secret://hermes/app/key_{n}"))
        .collect();
    for key in &keys {
        store_secret(&rig, key, "a value");
    }
    store_secret(&rig, "secret://hades/app/key", "a value of hades");
    store_secret(&rig, UNRESOLVABLE, "an old value");
    revoke_secret(&rig, UNRESOLVABLE, 1);
    let dir = tempfile::tempdir().unwrap();
    let hermes = Arc::new(enrolled(&rig, &dir, addr).await);
    let first_fails: Vec<String> = std::iter::once(UNRESOLVABLE.to_string())
        .chain(keys.iter().cloned())
        .collect();
    let last_fails: Vec<String> = keys
        .iter()
        .cloned()
        .chain(std::iter::once(UNRESOLVABLE.to_string()))
        .collect();
    let groups = Arc::new([
        vec!["secret://hades/app/key".to_string()],
        vec!["secret://hermes/app/missing".to_string()],
        vec![UNRESOLVABLE.to_string()],
        first_fails,
        last_fails,
    ]);
    let schedule = shuffled(
        (0..groups.len())
            .flat_map(|group| std::iter::repeat_n(group, PER_GROUP))
            .collect(),
        0x5eed_0006,
    );
    let permits = Arc::new(Semaphore::new(AT_ONCE));
    let mut tasks = Vec::with_capacity(schedule.len());
    for (order, group) in schedule.into_iter().enumerate() {
        let (permits, hermes, groups) = (permits.clone(), hermes.clone(), groups.clone());
        tasks.push(tokio::spawn(async move {
            let _permit = permits.acquire_owned().await.unwrap();
            let references: Vec<&str> = groups[group].iter().map(String::as_str).collect();
            let started = Instant::now();
            let reply = hermes.resolve(addr, &references).await.unwrap();
            let elapsed = started.elapsed();
            assert_eq!(reply.status, 404, "{}", reply.body);
            (order, group, elapsed)
        }));
    }
    let mut times: Vec<Vec<(usize, StdDuration)>> = vec![Vec::new(); groups.len()];
    for task in tasks {
        let (order, group, elapsed) = task.await.unwrap();
        times[group].push((order, elapsed));
    }
    // Each group: its median and the difference between the medians of its two halves.
    let stats: Vec<(StdDuration, StdDuration)> = times
        .into_iter()
        .map(|mut group| {
            group.sort_by_key(|(order, _)| *order);
            let all: Vec<StdDuration> = group.iter().map(|(_, time)| *time).collect();
            let (first, second) = all.split_at(all.len() / 2);
            let halves = median(first.to_vec()).abs_diff(median(second.to_vec()));
            (median(all), halves)
        })
        .collect();
    for (a, b) in [(0, 1), (0, 2), (1, 2), (3, 4)] {
        let gap = stats[a].0.abs_diff(stats[b].0);
        let allowed = stats[a].1.max(stats[b].1) + MARGIN;
        assert!(
            gap <= allowed,
            "groups {a} and {b}: {gap:?} > {allowed:?} ({stats:?})"
        );
    }
    supervisor.shutdown().await;
}
