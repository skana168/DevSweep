use devsweep::refresh::ActivationRefresh;
use std::time::Duration;

#[test]
fn activation_requires_a_return_after_thirty_seconds_and_manual_scan_resets_age() {
    let mut refresh = ActivationRefresh::new(Duration::ZERO, true);
    assert!(!refresh.activate(true, Duration::from_secs(30), false));
    assert!(!refresh.activate(false, Duration::from_secs(30), false));
    assert!(refresh.activate(true, Duration::from_secs(30), false));
    refresh.scanned(Duration::from_secs(30));
    refresh.activate(false, Duration::from_secs(59), false);
    assert!(!refresh.activate(true, Duration::from_secs(59), false));
    refresh.scanned(Duration::from_secs(60));
    refresh.activate(false, Duration::from_secs(89), false);
    assert!(!refresh.activate(true, Duration::from_secs(89), false));
    refresh.activate(false, Duration::from_secs(90), false);
    assert!(refresh.activate(true, Duration::from_secs(90), false));
}

#[test]
fn ongoing_scan_consumes_activation_without_requesting_a_follow_up() {
    let mut refresh = ActivationRefresh::new(Duration::ZERO, false);
    assert!(!refresh.activate(true, Duration::from_secs(30), true));
    assert!(!refresh.activate(true, Duration::from_secs(60), false));
    refresh.activate(false, Duration::from_secs(60), false);
    assert!(refresh.activate(true, Duration::from_secs(60), false));
}
