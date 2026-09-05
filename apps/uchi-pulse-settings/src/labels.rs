pub const STATES: &[(&str, &str)] = &[
    ("MEAL_NOTICE", "ご飯のお知らせ"),
    ("SNACK_NOTICE", "おやつのお知らせ"),
    ("HELP_NOTICE", "お手伝いのお願い"),
    ("ENTRY_PERMISSION", "部屋に入ってよいか"),
    ("MAILBOX", "ポストへの投函"),
];
pub fn state_label(key: &str) -> &str {
    STATES
        .iter()
        .find(|(code, _)| *code == key)
        .map(|(_, name)| *name)
        .unwrap_or(key)
}
pub fn values(state: &str) -> &'static [(&'static str, &'static str)] {
    match state {
        "ENTRY_PERMISSION" => &[
            ("UNSET", "未設定"),
            ("OK", "入室できます"),
            ("NG", "今は入室できません"),
            ("MEETING", "会議中です"),
        ],
        "MAILBOX" => &[("ON", "届いています"), ("OFF", "確認しました")],
        _ => &[("ON", "お知らせする"), ("OFF", "お知らせを解除する")],
    }
}
pub fn value_label<'a>(state: &str, key: &'a str) -> &'a str {
    values(state)
        .iter()
        .find(|(code, _)| *code == key)
        .map(|(_, name)| *name)
        .unwrap_or(key)
}
