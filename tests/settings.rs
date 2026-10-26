use neum::{Mode, Settings, settings::{RECENT, THRESHOLDS, startup_memory}};

#[test]
fn defaults_when_missing_or_broken() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("settings.json");
    assert_eq!(Settings::load_from(&p), Settings::default());
    std::fs::write(&p, "{ not json").unwrap();
    assert_eq!(Settings::load_from(&p), Settings::default());
    // fields it doesn't know are dropped, fields it misses are defaults
    std::fs::write(&p, r#"{"light": true, "whatever": 1}"#).unwrap();
    let s = Settings::load_from(&p);
    assert!(s.light);
    assert_eq!(s.threshold, 75);
}

#[test]
fn round_trip() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("deeper").join("settings.json");
    let mut s = Settings { light: true, mode: Mode::Bytes, threshold: 90, ..Default::default() };
    s.opened("/a");
    s.save_to(&p).unwrap();
    assert_eq!(Settings::load_from(&p), s);
    assert!(std::fs::read_to_string(&p).unwrap().contains(r#""mode": "bytes""#));
}

#[test]
fn saves_are_newest_first_and_capped() {
    let mut s = Settings::default();
    for i in 0..RECENT + 3 {
        s.opened(&format!("/m{i}"));
    }
    s.opened("/m5");
    assert_eq!(s.recent.len(), RECENT);
    assert_eq!(s.recent[0], "/m5");
    assert_eq!(s.memory, "/m5");
    assert_eq!(s.recent.iter().filter(|p| *p == "/m5").count(), 1);
    s.forget_save("/m5");
    assert!(!s.recent.contains(&"/m5".to_string()));
}

#[test]
fn thresholds_cycle() {
    let mut s = Settings::default();
    let mut seen = vec![];
    for _ in 0..THRESHOLDS.len() {
        s.threshold = s.next_threshold();
        seen.push(s.threshold);
    }
    assert_eq!(seen, [90, 100, 50, 75]);
    assert_eq!(s.threshold_fraction(), 0.75);
}

#[test]
fn which_memory_opens() {
    let mut s = Settings::default();
    // NEUM_MEMORY isn't set by the test runner; if a developer has it set, skip what it decides
    if std::env::var("NEUM_MEMORY").is_ok() {
        return;
    }
    assert_eq!(startup_memory(None, &s), "./memory");
    s.memory = "/last".into();
    assert_eq!(startup_memory(None, &s), "/last");
    assert_eq!(startup_memory(Some("/arg".into()), &s), "/arg");
}

#[test]
fn human_bytes() {
    assert_eq!(neum::human_bytes(0), "0 B");
    assert_eq!(neum::human_bytes(1023), "1023 B");
    assert_eq!(neum::human_bytes(1536), "1.5 KiB");
    assert_eq!(neum::human_bytes(5 * 1024 * 1024), "5.0 MiB");
}
