//! Disposable database for manual GUI checks. Refuses to overwrite any file.
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("specify a new temporary DB path");
    assert!(!std::path::Path::new(&path).exists());
    let db = uchi_pulse_hub::db::Database::open(path).unwrap();
    db.connection().execute_batch(
        "INSERT INTO families VALUES (1, 'お父さん', 1), (2, '花子', 1);
         INSERT INTO devices VALUES ('node-01', 'リビングのボタン', 'pico-w', '2026-09-05', '2026-09-05', 1);
         INSERT INTO actions VALUES (10, 'お父さんにご飯を知らせる', 'FAMILY', 1, 'ご飯です', 1);
         INSERT INTO action_state_changes VALUES (10, 'MEAL_NOTICE', 'ON');
         INSERT INTO action_notification_settings VALUES (10, 1, 'ご飯ができました');
         INSERT INTO action_notification_targets VALUES (10, 1);"
    ).unwrap();
}
