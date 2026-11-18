//! Opt-in integration tests against work/ssh_fixture.py, never an external server.
use crate::{
    model::Profile,
    net::{self, HostKeyRequest, TrustBroker},
};
use std::{
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc},
    thread,
    time::Duration,
};

fn fixture() -> (Profile, String) {
    let port = std::env::var("NEXUS_TEST_PORT")
        .expect("start the loopback fixture and set NEXUS_TEST_PORT")
        .parse::<u16>()
        .unwrap();
    assert!(port > 1024, "fixture must use a dynamic unprivileged port");
    let fingerprint =
        std::env::var("NEXUS_TEST_FINGERPRINT").expect("NEXUS_TEST_FINGERPRINT required");
    assert!(fingerprint.starts_with("SHA256:"));
    (
        Profile {
            name: "Loopback integration fixture".into(),
            host: "127.0.0.1".into(),
            port,
            username: "testuser".into(),
            ..Default::default()
        },
        fingerprint,
    )
}

fn broker(expected: String, accept: bool) -> (TrustBroker, thread::JoinHandle<()>) {
    let (requests, rx) = mpsc::channel::<HostKeyRequest>();
    let thread = thread::spawn(move || {
        let request = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("SSH identity request");
        assert_eq!(
            request.fingerprint, expected,
            "fixture host identity must match out-of-band fingerprint"
        );
        assert!(request.endpoint.starts_with("[127.0.0.1]:"));
        request
            .reply
            .send(accept)
            .expect("reply to SSH identity request");
    });
    (TrustBroker { requests }, thread)
}

fn auth_attempts() -> u64 {
    let path = std::env::var("NEXUS_TEST_STATS").expect("NEXUS_TEST_STATS required");
    // Windows can briefly deny a read while the fixture atomically updates counters.
    for _ in 0..50 {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(stats) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(attempts) = stats["auth_attempts"].as_u64() {
                    return attempts;
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("fixture authentication statistics unavailable");
}

#[test]
#[ignore = "Requires explicitly started localhost SSH/SFTP fixture and NEXUS_TEST_* environment"]
fn loopback_host_key_rejection_precedes_password_authentication() {
    let (profile, fingerprint) = fixture();
    let before = auth_attempts();
    let (trust, verification) = broker(fingerprint, false);
    let result = net::connect(
        &profile,
        "fixture-password",
        &trust,
        &AtomicBool::new(false),
    );
    verification.join().unwrap();
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("Aucune authentification"));
    assert_eq!(
        auth_attempts(),
        before,
        "no password authentication may be sent before host identity is accepted"
    );
}

#[test]
#[ignore = "Requires explicitly started localhost SSH/SFTP fixture and NEXUS_TEST_* environment"]
fn loopback_wrong_password_is_rejected() {
    let (profile, fingerprint) = fixture();
    let (trust, verification) = broker(fingerprint, true);
    let result = net::connect(
        &profile,
        "deliberately-wrong-fixture-password",
        &trust,
        &AtomicBool::new(false),
    );
    verification.join().unwrap();
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("Authentification"));
}

#[test]
#[ignore = "Requires explicitly started localhost SSH/SFTP fixture and NEXUS_TEST_* environment"]
fn loopback_sftp_real_transfer_overwrite_rename_delete_and_cancel() {
    let (profile, fingerprint) = fixture();
    let (trust, verification) = broker(fingerprint, true);
    let session = net::connect(
        &profile,
        "fixture-password",
        &trust,
        &AtomicBool::new(false),
    )
    .expect("verified fixture connection");
    verification.join().unwrap();
    let local: PathBuf = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("loopback-integration")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&local).unwrap();
    crate::sftp::loopback_transfer_test(&session, &local).expect("SFTP protocol roundtrip");
    session
        .disconnect(None, "Integration test complete", None)
        .unwrap();
}
