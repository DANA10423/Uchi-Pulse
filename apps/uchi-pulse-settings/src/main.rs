mod appearance;
mod db;
mod labels;

use appearance::{ACCENT, BACKGROUND, BORDER, INK, MUTED};
use db::{
    ActionRecord, DeviceRecord, EventRecord, FamilyRecord, NotificationDestinationRecord,
    SqliteDatabase, StateChangeRecord,
};
use eframe::egui::{self, Color32, RichText, TextEdit};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Families,
    Devices,
    Actions,
    Destinations,
    Events,
}
impl Section {
    fn label(self) -> &'static str {
        match self {
            Self::Families => "家族",
            Self::Devices => "子機",
            Self::Actions => "ボタンの動作",
            Self::Destinations => "通知の届け先",
            Self::Events => "受信履歴",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Families => "一緒に使う人の名前を登録します。",
            Self::Devices => "ボタンやセンサーの名前と識別番号を登録します。",
            Self::Actions => "ボタンを押したときに、誰に何を知らせるかを設定します。",
            Self::Destinations => "家族ごとに、LINEなどの送信先を登録します。",
            Self::Events => "子機から受け取ったイベントを、新しい順に確認できます。",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Draft {
    Device(DeviceRecord),
    Family(FamilyRecord),
    Action(ActionRecord),
    Destination(NotificationDestinationRecord),
}
impl Draft {
    fn name(&self) -> &str {
        match self {
            Self::Device(v) => &v.name,
            Self::Family(v) => &v.display_name,
            Self::Action(v) => &v.action_name,
            Self::Destination(v) => &v.notification_type,
        }
    }
}
enum Intent {
    OpenPicker,
    OpenPath(PathBuf),
    Navigate(Section),
    Edit(Draft),
    New,
    Reload,
    Close,
}
enum Dialog {
    Discard(Intent),
    Delete,
}
#[derive(Default)]
struct Data {
    devices: Vec<DeviceRecord>,
    families: Vec<FamilyRecord>,
    actions: Vec<ActionRecord>,
    destinations: Vec<NotificationDestinationRecord>,
    events: Vec<EventRecord>,
}
impl Data {
    fn load(db: &SqliteDatabase) -> Result<Self, String> {
        Ok(Self {
            devices: db.list_devices()?,
            families: db.list_families()?,
            actions: db.list_actions()?,
            destinations: db.list_notification_destinations()?,
            events: db.list_events()?,
        })
    }
}
struct SettingsApp {
    db: Option<SqliteDatabase>,
    path: Option<PathBuf>,
    data: Data,
    section: Section,
    draft: Option<Draft>,
    original: Option<Draft>,
    creating: bool,
    query: String,
    status: String,
    error: bool,
    dialog: Option<Dialog>,
    state_type: String,
    path_input: String,
    font_found: bool,
    allow_close: bool,
}
impl SettingsApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let font_found = appearance::configure(&cc.egui_ctx);
        Self {
            db: None,
            path: None,
            data: Data::default(),
            section: Section::Families,
            draft: None,
            original: None,
            creating: false,
            query: String::new(),
            status: "設定ファイルを選んで始めましょう。".into(),
            error: false,
            dialog: None,
            state_type: "MEAL_NOTICE".into(),
            path_input: String::new(),
            font_found,
            allow_close: false,
        }
    }
    fn dirty(&self) -> bool {
        self.draft != self.original
    }
    fn message(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
    }
    fn request(&mut self, intent: Intent) {
        if self.dirty() {
            self.dialog = Some(Dialog::Discard(intent));
        } else {
            self.perform(intent);
        }
    }
    fn perform(&mut self, intent: Intent) {
        match intent {
            Intent::OpenPicker => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("親機の設定ファイルを選択")
                    .add_filter("SQLite データベース", &["db", "sqlite", "sqlite3"])
                    .add_filter("すべてのファイル", &["*"])
                    .pick_file()
                {
                    self.open(path);
                }
            }
            Intent::OpenPath(path) => self.open(path),
            Intent::Navigate(section) => {
                self.section = section;
                self.query.clear();
                self.clear_editor();
            }
            Intent::Edit(draft) => {
                self.original = Some(draft.clone());
                self.draft = Some(draft);
                self.creating = false;
            }
            Intent::New => self.new_record(),
            Intent::Reload => {
                if let Some(db) = &self.db {
                    match Data::load(db) {
                        Ok(data) => {
                            self.data = data;
                            self.clear_editor();
                            self.message("最新の設定を読み込みました。", false);
                        }
                        Err(e) => self.message(e, true),
                    }
                }
            }
            Intent::Close => self.allow_close = true,
        }
    }
    fn clear_editor(&mut self) {
        self.draft = None;
        self.original = None;
        self.creating = false;
    }
    fn open(&mut self, path: PathBuf) {
        let result =
            SqliteDatabase::open(&path).and_then(|db| Data::load(&db).map(|data| (db, data)));
        match result {
            Ok((db, data)) => {
                self.db = Some(db);
                self.data = data;
                self.path = Some(path);
                self.clear_editor();
                self.query.clear();
                self.message(
                    "設定ファイルを開きました。左から設定項目を選んでください。",
                    false,
                );
            }
            Err(e) => self.message(e, true),
        }
    }
    fn new_record(&mut self) {
        let draft = match self.section {
            Section::Devices => Draft::Device(DeviceRecord {
                enabled: true,
                device_type: "pico-w".into(),
                ..Default::default()
            }),
            Section::Families => Draft::Family(FamilyRecord {
                enabled: true,
                family_id: next_id(self.data.families.iter().map(|v| v.family_id)),
                ..Default::default()
            }),
            Section::Actions => Draft::Action(ActionRecord {
                enabled: true,
                target_type: "FAMILY".into(),
                action_id: next_id(self.data.actions.iter().map(|v| v.action_id)),
                ..Default::default()
            }),
            Section::Destinations => Draft::Destination(NotificationDestinationRecord {
                enabled: true,
                notification_type: "LINE".into(),
                ..Default::default()
            }),
            Section::Events => return,
        };
        self.original = Some(draft.clone());
        self.draft = Some(draft);
        self.creating = true;
    }
    fn save(&mut self) {
        let (Some(db), Some(draft)) = (&self.db, &self.draft) else {
            return;
        };
        if let Some(e) = self.validation() {
            self.message(e, true);
            return;
        }
        let result = match draft {
            Draft::Device(v) => db.save_device(v),
            Draft::Family(v) => db.save_family(v),
            Draft::Action(v) => db.save_action(v),
            Draft::Destination(v) => db.save_notification_destination(v).map(|id| {
                if let Some(Draft::Destination(v)) = &mut self.draft {
                    v.id = Some(id);
                }
            }),
        };
        match result {
            Ok(()) => {
                self.original = self.draft.clone();
                self.creating = false;
                match Data::load(self.db.as_ref().unwrap()) {
                    Ok(data) => {
                        self.data = data;
                        self.message(
                            "保存しました。変更を反映するには親機を再起動してください。",
                            false,
                        );
                    }
                    Err(e) => self.message(
                        format!("保存は完了しました。再読込に失敗しました：{e}"),
                        true,
                    ),
                }
            }
            Err(e) => self.message(e, true),
        }
    }
    fn validation(&self) -> Option<String> {
        let draft = self.draft.as_ref()?;
        let issue: Option<&str> = match draft {
            Draft::Device(v) if v.name.trim().is_empty() => Some("子機の名前を入力してください。"),
            Draft::Device(v) if v.device_id.trim().is_empty() => {
                Some("子機に設定した識別番号を入力してください。")
            }
            Draft::Device(v) if v.device_type.trim().is_empty() => {
                Some("子機の種類を入力してください。")
            }
            Draft::Device(v)
                if self.creating
                    && self.data.devices.iter().any(|x| x.device_id == v.device_id) =>
            {
                Some("この識別番号は登録済みです。一覧から選んで編集してください。")
            }
            Draft::Family(v) if v.display_name.trim().is_empty() => {
                Some("家族の名前を入力してください。")
            }
            Draft::Family(v)
                if self.creating
                    && self
                        .data
                        .families
                        .iter()
                        .any(|x| x.family_id == v.family_id) =>
            {
                Some("この管理番号は登録済みです。別の番号を指定してください。")
            }
            Draft::Action(v) if v.action_name.trim().is_empty() => {
                Some("動作の名前を入力してください。")
            }
            Draft::Action(v)
                if self.creating
                    && self.data.actions.iter().any(|x| x.action_id == v.action_id) =>
            {
                Some("この動作番号は登録済みです。一覧から選んで編集してください。")
            }
            Draft::Action(v)
                if v.target_type == "FAMILY"
                    && !self
                        .data
                        .families
                        .iter()
                        .any(|x| Some(x.family_id) == v.target_family_id) =>
            {
                Some("対象の家族を選んでください。家族が未登録の場合は、先に登録してください。")
            }
            Draft::Destination(v)
                if !self
                    .data
                    .families
                    .iter()
                    .any(|x| x.family_id == v.family_id) =>
            {
                Some("通知を受け取る家族を選んでください。")
            }
            Draft::Destination(v) if v.destination.trim().is_empty() => {
                Some("送信先ID・アドレスを入力してください。")
            }
            _ => None,
        };
        issue.map(str::to_owned)
    }
    fn delete(&mut self) {
        let (Some(db), Some(draft)) = (&self.db, &self.original) else {
            return;
        };
        let result = match draft {
            Draft::Device(v) => db.delete_device(&v.device_id),
            Draft::Family(v) => db.delete_family(v.family_id),
            Draft::Action(v) => db.delete_action(v.action_id),
            Draft::Destination(v) => {
                v.id.ok_or_else(|| "保存済みの届け先を選んでください。".into())
                    .and_then(|id| db.delete_notification_destination(id))
            }
        };
        match result {
            Ok(()) => {
                self.perform(Intent::Reload);
                if !self.error {
                    self.message("削除しました。", false);
                }
            }
            Err(e) => self.message(e, true),
        }
    }
    fn rows(&self) -> Vec<(String, String, Draft)> {
        match self.section {
            Section::Families => self
                .data
                .families
                .iter()
                .map(|v| {
                    (
                        v.display_name.clone(),
                        enabled(v.enabled).into(),
                        Draft::Family(v.clone()),
                    )
                })
                .collect(),
            Section::Devices => self
                .data
                .devices
                .iter()
                .map(|v| {
                    (
                        v.name.clone(),
                        format!("{} · {}", v.device_id, enabled(v.enabled)),
                        Draft::Device(v.clone()),
                    )
                })
                .collect(),
            Section::Actions => self
                .data
                .actions
                .iter()
                .map(|v| {
                    (
                        v.action_name.clone(),
                        format!("動作番号 {} · {}", v.action_id, enabled(v.enabled)),
                        Draft::Action(v.clone()),
                    )
                })
                .collect(),
            Section::Destinations => self
                .data
                .destinations
                .iter()
                .map(|v| {
                    (
                        self.family_name(v.family_id),
                        format!("{} · {}", v.notification_type, enabled(v.enabled)),
                        Draft::Destination(v.clone()),
                    )
                })
                .collect(),
            Section::Events => Vec::new(),
        }
    }
    fn family_name(&self, id: u32) -> String {
        family_name(&self.data.families, id)
    }
    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.add_space(44.0);
        ui.heading("おうちの「お知らせ」を設定しましょう");
        ui.label(RichText::new("家族やボタンの動作を、このアプリから管理できます。").color(MUTED));
        ui.add_space(20.0);
        card(ui, |ui| {
            ui.heading("まず、親機の設定ファイルを開きます");
            ui.label("親機が使っている uchi-pulse.db を選んでください。");
            ui.add_space(8.0);
            if primary(ui, "設定ファイルを選ぶ").clicked() {
                self.request(Intent::OpenPicker);
            }
            if ui.button("このフォルダの uchi-pulse.db を開く").clicked() {
                self.request(Intent::OpenPath(PathBuf::from("uchi-pulse.db")));
            }
            egui::CollapsingHeader::new("ファイルの場所を直接入力").show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.path_input)
                        .hint_text("/フォルダ/uchi-pulse.db")
                        .desired_width(f32::INFINITY),
                );
                if ui.button("このファイルを開く").clicked() {
                    self.request(Intent::OpenPath(PathBuf::from(self.path_input.trim())));
                }
            });
        });
        ui.add_space(20.0);
        for (title, detail) in [
            ("1  家族を登録", "一緒に使う人の名前を入力します。"),
            (
                "2  子機を登録",
                "ボタンやセンサーに、わかりやすい名前を付けます。",
            ),
            (
                "3  ボタンの動作を設定",
                "「ご飯のお知らせ」など、押したときの動作を選びます。",
            ),
        ] {
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(detail).color(MUTED));
        }
    }
    fn list(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(self.section.label()).strong().size(19.0));
        if primary(ui, "＋ 新しく登録").clicked() {
            self.request(Intent::New);
        }
        ui.add(
            TextEdit::singleline(&mut self.query)
                .hint_text("名前や番号で検索")
                .desired_width(f32::INFINITY),
        );
        let rows = self.rows();
        ui.small(format!("{}件登録されています", rows.len()));
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("records")
            .show(ui, |ui| {
                let mut count = 0;
                for (name, detail, draft) in rows {
                    if !format!("{name} {detail}")
                        .to_lowercase()
                        .contains(&self.query.to_lowercase())
                    {
                        continue;
                    }
                    count += 1;
                    let selected = !self.creating && self.original.as_ref() == Some(&draft);
                    if ui
                        .add_sized(
                            [ui.available_width(), 70.0],
                            egui::Button::new(
                                RichText::new(format!("{name}\n{detail}")).size(15.0),
                            )
                            .selected(selected),
                        )
                        .clicked()
                    {
                        self.request(Intent::Edit(draft));
                    }
                }
                if count == 0 {
                    ui.add_space(20.0);
                    ui.label(if self.query.is_empty() {
                        "まだ登録がありません。\n「＋ 新しく登録」から追加できます。"
                    } else {
                        "一致する項目がありません。"
                    });
                }
            });
    }
    fn editor(&mut self, ui: &mut egui::Ui) {
        if self.draft.is_none() {
            ui.add_space(40.0);
            ui.heading(self.section.label());
            ui.label(self.section.hint());
            ui.add_space(20.0);
            if primary(ui, "＋ 新しく登録").clicked() {
                self.request(Intent::New);
            }
            ui.label(
                RichText::new("登録済みの項目は、左の一覧から選んで編集できます。").color(MUTED),
            );
            return;
        }
        let dirty = self.dirty();
        let valid = self.validation();
        egui::Panel::bottom("editor_save")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(16))
            .show(ui, |ui| {
                if let Some(e) = &valid {
                    ui.colored_label(MUTED, e);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            valid.is_none() && (dirty || self.creating),
                            egui::Button::new(RichText::new("変更を保存").color(Color32::WHITE))
                                .fill(ACCENT),
                        )
                        .clicked()
                    {
                        self.save();
                    }
                    if ui
                        .add_enabled(dirty, egui::Button::new("変更を取り消す"))
                        .clicked()
                    {
                        if let Some(original) = self.original.clone() {
                            self.request(Intent::Edit(original));
                        }
                    }
                    ui.label(
                        RichText::new(if dirty {
                            "未保存の変更があります"
                        } else {
                            "変更はありません"
                        })
                        .small()
                        .color(MUTED),
                    );
                    if !self.creating
                        && ui
                            .button(RichText::new("削除…").color(Color32::from_rgb(160, 57, 54)))
                            .clicked()
                    {
                        self.dialog = Some(Dialog::Delete);
                    }
                });
            });
        egui::ScrollArea::vertical().id_salt(("form", self.section.label())).show(ui, |ui| {
            ui.heading(if self.creating { format!("{}を登録", self.section.label()) } else { format!("{}を編集", self.section.label()) });
            ui.label(RichText::new(self.section.hint()).color(MUTED));
            ui.add_space(8.0);
            let creating = self.creating;
            let families = &self.data.families;
            match self.draft.as_mut().unwrap() {
                Draft::Device(v) => card(ui, |ui| {
                    text_field(ui, "子機の名前 *", "例：リビングのボタン", &mut v.name);
                    ui.label("子機の識別番号 *");
                    ui.add_enabled(creating, TextEdit::singleline(&mut v.device_id).hint_text("例：node-01").desired_width(f32::INFINITY));
                    note(ui, "子機に設定した device_id と同じ文字列を入力します。保存後は変更できません。");
                    text_field(ui, "子機の種類 *", "例：pico-w、玄関ボタン", &mut v.device_type);
                    ui.checkbox(&mut v.enabled, "この子機を使う");
                }),
                Draft::Family(v) => card(ui, |ui| {
                    text_field(ui, "名前 *", "例：お父さん、花子", &mut v.display_name);
                    ui.checkbox(&mut v.enabled, "この家族を有効にする");
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("管理番号：{}", v.family_id)).small().color(MUTED));
                    note(ui, "管理番号は新規登録時に自動で割り当てます。");
                }),
                Draft::Action(v) => {
                    card(ui, |ui| {
                        text_field(ui, "動作の名前 *", "例：お父さんにご飯を知らせる", &mut v.action_name);
                        ui.label("動作番号");
                        ui.add_enabled(creating, egui::DragValue::new(&mut v.action_id).range(0..=u32::MAX));
                        note(ui, "子機のボタンにも、この番号（Action ID）を設定します。");
                        ui.checkbox(&mut v.enabled, "この動作を使う");
                    });
                    card(ui, |ui| {
                        ui.label(RichText::new("誰の状態を変えますか？").strong());
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut v.target_type, "FAMILY".into(), "家族ひとり");
                            ui.selectable_value(&mut v.target_type, "COMMON".into(), "家全体の共通状態");
                        });
                        if v.target_type == "FAMILY" {
                            family_combo(ui, "action_family", families, &mut v.target_family_id);
                        } else if v.target_type == "COMMON" { v.target_family_id = None; }
                        ui.separator();
                        ui.label(RichText::new("どんなお知らせにしますか？").strong());
                        note(ui, "複数追加できます。通知だけの動作なら、追加しなくても構いません。");
                        let mut remove = None;
                        for (i, change) in v.state_changes.iter_mut().enumerate() {
                            ui.push_id(i, |ui| { ui.horizontal_wrapped(|ui| {
                                ui.label(labels::state_label(&change.state_type));
                                egui::ComboBox::from_id_salt("change_value")
                                    .selected_text(labels::value_label(&change.state_type, &change.state_value))
                                    .show_ui(ui, |ui| {
                                        for (code, label) in labels::values(&change.state_type) {
                                            ui.selectable_value(&mut change.state_value, (*code).into(), *label);
                                        }
                                    });
                                if ui.button("外す").clicked() { remove = Some(i); }
                            }); });
                        }
                        if let Some(i) = remove { v.state_changes.remove(i); }
                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt("state_type").selected_text(labels::state_label(&self.state_type)).show_ui(ui, |ui| {
                                for (code, label) in labels::STATES { ui.selectable_value(&mut self.state_type, (*code).into(), *label); }
                            });
                            let exists = v.state_changes.iter().any(|v| v.state_type == self.state_type);
                            if ui.add_enabled(!exists, egui::Button::new("お知らせを追加")).clicked() {
                                v.state_changes.push(StateChangeRecord { state_type: self.state_type.clone(), state_value: labels::values(&self.state_type)[0].0.into() });
                            }
                        });
                    });
                    card(ui, |ui| {
                        ui.label(RichText::new("スマートフォンへの通知").strong());
                        ui.checkbox(&mut v.notification_enabled, "この動作で通知する");
                        note(ui, "ここでは設定を保存します。通知の実際の送信には親機の通知機能が必要です。");
                        text_field(ui, "通知メッセージ（任意）", "例：ご飯ができました", &mut v.notification_message);
                        ui.label("通知を受け取る家族（複数選択可）");
                        if families.is_empty() { note(ui, "「家族」から名前を登録すると選択できます。"); }
                        for family in families {
                            let mut checked = v.notification_targets.contains(&family.family_id);
                            if ui.checkbox(&mut checked, &family.display_name).changed() {
                                if checked { v.notification_targets.push(family.family_id); }
                                else { v.notification_targets.retain(|id| *id != family.family_id); }
                            }
                        }
                    });
                    card(ui, |ui| {
                        text_field(ui, "Web画面に出すメッセージ（任意）", "例：ご飯です", &mut v.web_message);
                        note(ui, "空欄ならメッセージを設定しません。");
                    });
                }
                Draft::Destination(v) => card(ui, |ui| {
                    ui.label("受け取る家族 *");
                    let mut selected = if v.family_id == 0 { None } else { Some(v.family_id) };
                    family_combo(ui, "destination_family", families, &mut selected);
                    v.family_id = selected.unwrap_or(0);
                    ui.label("通知サービス");
                    egui::ComboBox::from_id_salt("service").selected_text(&v.notification_type).show_ui(ui, |ui| {
                        for name in ["LINE", "Slack"] { ui.selectable_value(&mut v.notification_type, name.into(), name); }
                    });
                    text_field(ui, "送信先ID・アドレス *", "サービスで指定された送信先を入力", &mut v.destination);
                    note(ui, "通知サービスで取得した送信先を指定します。ここではテスト送信は行いません。");
                    ui.checkbox(&mut v.enabled, "この届け先を使う");
                }),
            }
            ui.add_space(12.0);
        });
    }
    fn history(&self, ui: &mut egui::Ui) {
        ui.heading("受信履歴");
        ui.label("最新500件を表示しています。");
        if self.data.events.is_empty() {
            ui.add_space(24.0);
            ui.label("まだ受信履歴がありません。");
            return;
        }
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("events").striped(true).show(ui, |ui| {
                for title in ["受信日時（DB記録値）", "子機", "イベント番号"] {
                    ui.strong(title);
                }
                ui.end_row();
                for v in &self.data.events {
                    ui.label(&v.received_at);
                    let name = self
                        .data
                        .devices
                        .iter()
                        .find(|d| d.device_id == v.device_id)
                        .map(|d| d.name.as_str())
                        .unwrap_or(&v.device_id);
                    ui.label(name);
                    ui.label(&v.event_id)
                        .on_hover_text(format!("記録番号 {}", v.id));
                    ui.end_row();
                }
            });
        });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.dialog.is_none() {
            return;
        }
        let deleting = matches!(self.dialog, Some(Dialog::Delete));
        let mut accept = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading(if deleting {
                "この登録を削除しますか？"
            } else {
                "保存していない変更があります"
            });
            if deleting {
                ui.label(format!(
                    "対象：{}",
                    self.original.as_ref().map(|d| d.name()).unwrap_or("")
                ));
                ui.label("関連する設定も削除されることがあります。履歴は削除しません。");
            } else {
                ui.label("変更を破棄して、次の操作に進みますか？");
            }
            ui.horizontal(|ui| {
                if ui.button("編集に戻る").clicked() {
                    cancel = true;
                }
                if ui
                    .button(if deleting {
                        "削除する"
                    } else {
                        "変更を破棄して進む"
                    })
                    .clicked()
                {
                    accept = true;
                }
            });
        });
        if cancel {
            self.dialog = None;
        }
        if accept {
            match self.dialog.take().unwrap() {
                Dialog::Delete => self.delete(),
                Dialog::Discard(intent) => {
                    self.draft = self.original.clone();
                    self.perform(intent);
                }
            }
        }
    }
}
impl eframe::App for SettingsApp {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.dialog.is_none() {
                self.dialog = Some(Dialog::Discard(Intent::Close));
            }
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Uchi-Pulse")
                            .size(24.0)
                            .strong()
                            .color(ACCENT),
                    );
                    ui.label(RichText::new("おうちの設定").size(19.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("設定ファイルを選ぶ").clicked() {
                            self.request(Intent::OpenPicker);
                        }
                        if self.db.is_some() && ui.button("再読込").clicked() {
                            self.request(Intent::Reload);
                        }
                    });
                });
                if let Some(path) = &self.path {
                    ui.label(
                        RichText::new(format!("編集中：{}", path.display()))
                            .small()
                            .color(MUTED),
                    );
                }
            });
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(14))
            .show(ui, |ui| {
                ui.colored_label(
                    if self.error {
                        Color32::from_rgb(165, 43, 40)
                    } else {
                        ACCENT
                    },
                    &self.status,
                );
                if self.db.is_some() {
                    note(
                        ui,
                        "設定の変更は親機を停止してから。保存後に親機を再起動してください。",
                    );
                }
                if !self.font_found {
                    ui.colored_label(
                        Color32::RED,
                        "Japanese font not found. Install Noto Sans CJK to display Japanese.",
                    );
                }
            });
        if self.db.is_some() {
            egui::Panel::left("nav")
                .resizable(false)
                .default_size(182.0)
                .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(16))
                .show(ui, |ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("設定メニュー").small().color(MUTED));
                    ui.add_space(8.0);
                    for section in [
                        Section::Families,
                        Section::Devices,
                        Section::Actions,
                        Section::Destinations,
                        Section::Events,
                    ] {
                        if ui
                            .add_sized(
                                [ui.available_width(), 44.0],
                                egui::Button::new(section.label())
                                    .selected(self.section == section),
                            )
                            .clicked()
                        {
                            self.request(Intent::Navigate(section));
                        }
                    }
                });
            if self.section != Section::Events {
                egui::Panel::left("list")
                    .resizable(true)
                    .default_size(260.0)
                    .min_size(220.0)
                    .max_size(380.0)
                    .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(16))
                    .show(ui, |ui| self.list(ui));
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(22))
            .show(ui, |ui| {
                if self.db.is_none() {
                    egui::ScrollArea::vertical().show(ui, |ui| self.welcome(ui));
                } else if self.section == Section::Events {
                    self.history(ui);
                } else {
                    self.editor(ui);
                }
            });
        self.dialogs(ui.ctx());
    }
}
fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(10)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
    ui.add_space(8.0);
}
fn primary<'a>(ui: &mut egui::Ui, text: &'a str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(ACCENT))
}
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(MUTED));
}
fn text_field(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String) {
    ui.label(RichText::new(label).color(INK));
    ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
}
fn enabled(value: bool) -> &'static str {
    if value { "利用中" } else { "無効" }
}
fn next_id(ids: impl Iterator<Item = u32>) -> u32 {
    let used: std::collections::HashSet<_> = ids.collect();
    (1..=u32::MAX).find(|id| !used.contains(id)).unwrap_or(0)
}
fn family_name(families: &[FamilyRecord], id: u32) -> String {
    families
        .iter()
        .find(|f| f.family_id == id)
        .map(|f| f.display_name.clone())
        .unwrap_or_else(|| format!("未登録の家族（{id}）"))
}
fn family_combo(ui: &mut egui::Ui, id: &str, families: &[FamilyRecord], value: &mut Option<u32>) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(
            value
                .map(|id| family_name(families, id))
                .unwrap_or_else(|| "家族を選んでください".into()),
        )
        .width(240.0)
        .show_ui(ui, |ui| {
            for f in families {
                ui.selectable_value(value, Some(f.family_id), &f.display_name);
            }
        });
    if families.is_empty() {
        note(ui, "先に「家族」メニューで名前を登録してください。");
    }
}
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Uchi-Pulse おうちの設定",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 850.0])
                .with_min_inner_size([1000.0, 700.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(SettingsApp::new(cc)))),
    )
}
