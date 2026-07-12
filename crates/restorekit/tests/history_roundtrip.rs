#![cfg(feature = "history")]

use restorekit::history::{self, HistoryEntry};

// Exercises the real bundled-SQLite store end to end (open → migrate → insert →
// query → CSV export → clear) against a throwaway cache dir.
#[test]
fn record_list_export_clear_roundtrip() {
    let dir = std::env::temp_dir().join(format!("rk-hist-{}", std::process::id()));
    std::env::set_var("RESTOREKIT_CACHE_DIR", dir.join("firmwares"));

    history::clear().unwrap();

    let entry = HistoryEntry {
        serial_number: Some("C02XX1234567".into()),
        ecid: "0x77aa22bb44cc".into(),
        model_identifier: Some("Mac14,2".into()),
        name: "MacBook Air (M2, 2022)".into(), // contains a comma → must be CSV-quoted
        mode: "restore".into(),
        status: "restored".into(),
        timestamp_rfc3339: "2026-01-01T00:00:00Z".into(),
        obliteration: Some("confirmed".into()),
        checkpoints_json: Some(r#"["{\"CHECKPOINT_NAME\":\"format_effaceable_storage\"}"]"#.into()),
        checkpoints_raw: Some(r#"["<plist>…</plist>"]"#.into()),
    };
    history::record(&entry).unwrap();

    let all = history::list().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].serial_number.as_deref(), Some("C02XX1234567"));
    assert_eq!(all[0].name, "MacBook Air (M2, 2022)");
    assert_eq!(all[0].obliteration.as_deref(), Some("confirmed"));

    let csv = dir.join("out.csv");
    history::export_csv(&csv).unwrap();
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.starts_with("Timestamp,Serial,ECID,Model,Name,Mode,Status,Obliteration"));
    assert!(text.contains("\"MacBook Air (M2, 2022)\""));
    assert!(text.trim_end().ends_with("confirmed"));

    history::clear().unwrap();
    assert_eq!(history::list().unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}
