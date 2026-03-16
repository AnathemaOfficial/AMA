use ama_core::slime::*;
use std::sync::Arc;
use std::thread;

#[test]
fn authorizes_valid_domain() {
    let auth = test_authorizer(10000);
    let verdict = auth.try_reserve(&"fs.write.workspace".into(), 10);
    assert!(matches!(verdict, SlimeVerdict::Authorized));
}

#[test]
fn rejects_unknown_domain() {
    let auth = test_authorizer(10000);
    let verdict = auth.try_reserve(&"unknown.domain".into(), 1);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

#[test]
fn rejects_disabled_domain() {
    let auth = test_authorizer_with_disabled(10000);
    let verdict = auth.try_reserve(&"fs.write.workspace".into(), 1);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

#[test]
fn rejects_over_per_action_limit() {
    let auth = test_authorizer(10000);
    let verdict = auth.try_reserve(&"fs.write.workspace".into(), 101);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

#[test]
fn capacity_exhaustion() {
    let auth = test_authorizer(100);
    assert!(matches!(auth.try_reserve(&"fs.read.workspace".into(), 50), SlimeVerdict::Authorized));
    assert!(matches!(auth.try_reserve(&"fs.read.workspace".into(), 50), SlimeVerdict::Authorized));
    assert!(matches!(auth.try_reserve(&"fs.read.workspace".into(), 1), SlimeVerdict::Impossible));
}

#[test]
fn capacity_never_exceeds_max_concurrent() {
    let auth = Arc::new(test_authorizer(1000));
    let mut handles = vec![];

    for _ in 0..100 {
        let auth = Arc::clone(&auth);
        handles.push(thread::spawn(move || {
            auth.try_reserve(&"fs.read.workspace".into(), 10)
        }));
    }

    let authorized_count: usize = handles.into_iter()
        .map(|h| h.join().unwrap())
        .filter(|v| matches!(v, SlimeVerdict::Authorized))
        .count();

    assert_eq!(authorized_count, 100);
    assert_eq!(auth.capacity_used(), 1000);
}

#[test]
fn check_only_does_not_consume_capacity() {
    let auth = test_authorizer(100);
    let verdict = auth.check_only(&"fs.write.workspace".into(), 50);
    assert!(matches!(verdict, SlimeVerdict::Authorized));
    assert_eq!(auth.capacity_used(), 0);
}

#[test]
fn check_only_reports_impossible_when_full() {
    let auth = test_authorizer(10);
    auth.try_reserve(&"fs.read.workspace".into(), 10);
    let verdict = auth.check_only(&"fs.read.workspace".into(), 1);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

fn test_authorizer(max_cap: u64) -> P0Authorizer {
    P0Authorizer::new(max_cap, vec![
        ("fs.write.workspace".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 100 }),
        ("fs.read.workspace".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 500 }),
        ("proc.exec.bounded".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 50 }),
        ("net.out.http".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 200 }),
    ])
}

fn test_authorizer_with_disabled(max_cap: u64) -> P0Authorizer {
    P0Authorizer::new(max_cap, vec![
        ("fs.write.workspace".into(), DomainPolicy { enabled: false, max_magnitude_per_action: 100 }),
    ])
}
