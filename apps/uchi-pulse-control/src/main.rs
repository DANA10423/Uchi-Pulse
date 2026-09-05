use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use eframe::egui::{self, Color32, RichText, TextEdit, TextStyle};
use serde_json::{Value, json};
use uchi_pulse_common::cdc::CdcStatus;

use uchi_pulse_control::db::{
    ActionRecord, DeviceRecord, EventRecord, FamilyRecord, NotificationDestinationRecord,
    SqliteDatabase, StateChangeRecord,
};
use uchi_pulse_control::protocol::{
    WorkerCommand, WorkerEvent, build_request, parse_response, spawn_serial_worker,
};

const COMMANDS: &[(&str, &str)] = &[
    ("get_info", "情報取得"),
    ("get_config", "設定取得"),
    ("get_status", "状態取得"),
    ("get_inputs", "入力状態"),
    ("get_outputs", "出力状態"),
];
const HUB_COMMANDS: &[(&str, &str)] = &[
    ("get_info", "情報取得"),
    ("get_config", "設定取得"),
    ("get_status", "状態取得"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EndpointKind {
    Node,
    Hub,
    Database,
}

impl EndpointKind {
    fn label(self) -> &'static str {
        match self {
            Self::Node => "子機",
            Self::Hub => "親機",
            Self::Database => "SQLite",
        }
    }

    fn commands(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Node => COMMANDS,
            Self::Hub => HUB_COMMANDS,
            Self::Database => &[],
        }
    }
}

struct Connection {
    path: String,
    opened: bool,
    command_tx: Sender<WorkerCommand>,
    event_rx: Receiver<WorkerEvent>,
}

struct LogEntry {
    direction: &'static str,
    text: String,
}

#[derive(Clone, Copy)]
enum ConfirmationAction {
    FactoryReset,
    Reboot,
}

impl ConfirmationAction {
    fn command(self) -> &'static str {
        match self {
            Self::FactoryReset => "factory_reset",
            Self::Reboot => "reboot",
        }
    }

    fn title(self, endpoint: EndpointKind) -> &'static str {
        match (self, endpoint) {
            (Self::FactoryReset, _) => "設定を初期化しますか？",
            (Self::Reboot, EndpointKind::Node) => "子機を再起動しますか？",
            (Self::Reboot, EndpointKind::Hub) => "親機の再起動要求を送信しますか？",
            (Self::Reboot, EndpointKind::Database) => "",
        }
    }
}

struct ControlApp {
    endpoint: EndpointKind,
    ports: Vec<String>,
    selected_port: String,
    connection: Option<Connection>,
    config_text: String,
    log: Vec<LogEntry>,
    status: String,
    font_info: String,
    request_number: u64,
    confirmation: Option<ConfirmationAction>,
    database_path: String,
    database: Option<SqliteDatabase>,
    database_section: DatabaseSection,
    devices: Vec<DeviceRecord>,
    families: Vec<FamilyRecord>,
    actions: Vec<ActionRecord>,
    notification_destinations: Vec<NotificationDestinationRecord>,
    events: Vec<EventRecord>,
    selected_device: Option<String>,
    selected_family: Option<u32>,
    selected_action: Option<u32>,
    selected_notification_destination: Option<i64>,
    device_form: DeviceRecord,
    family_form: FamilyRecord,
    action_form: ActionRecord,
    notification_destination_form: NotificationDestinationRecord,
    state_type_input: String,
    state_value_input: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DatabaseSection {
    Devices,
    Families,
    Actions,
    Destinations,
    Events,
}

impl DatabaseSection {
    fn label(self) -> &'static str {
        match self {
            Self::Devices => "子機",
            Self::Families => "家族",
            Self::Actions => "Action",
            Self::Destinations => "通知先",
            Self::Events => "履歴",
        }
    }
}

impl ControlApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self {
            endpoint: EndpointKind::Node,
            ports: Vec::new(),
            selected_port: String::new(),
            connection: None,
            config_text: default_config_text(),
            log: Vec::new(),
            status: "未接続".to_owned(),
            font_info: "日本語フォントを検索中…".to_owned(),
            request_number: 0,
            confirmation: None,
            database_path: "./uchi-pulse.db".to_owned(),
            database: None,
            database_section: DatabaseSection::Devices,
            devices: Vec::new(),
            families: Vec::new(),
            actions: Vec::new(),
            notification_destinations: Vec::new(),
            events: Vec::new(),
            selected_device: None,
            selected_family: None,
            selected_action: None,
            selected_notification_destination: None,
            device_form: DeviceRecord::default(),
            family_form: FamilyRecord::default(),
            action_form: ActionRecord::default(),
            notification_destination_form: NotificationDestinationRecord {
                notification_type: "LINE".to_owned(),
                enabled: true,
                ..NotificationDestinationRecord::default()
            },
            state_type_input: "MEAL_NOTICE".to_owned(),
            state_value_input: "ON".to_owned(),
        };
        app.font_info = install_japanese_font(&context.egui_ctx)
            .map(|path| format!("日本語フォント: {path}"))
            .unwrap_or_else(|| "日本語フォント未検出（標準フォントを使用）".to_owned());
        app.refresh_ports();
        app
    }

    fn refresh_ports(&mut self) {
        match serialport::available_ports() {
            Ok(ports) => {
                self.ports = ports.into_iter().map(|port| port.port_name).collect();
                self.ports.sort();
                if !self.ports.contains(&self.selected_port) {
                    self.selected_port = self.ports.first().cloned().unwrap_or_default();
                }
                self.status = format!("{}ポート検出", self.ports.len());
            }
            Err(error) => {
                self.ports.clear();
                self.selected_port.clear();
                self.status = format!("ポート検出失敗: {error}");
            }
        }
    }

    fn connected(&self) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|connection| connection.opened)
    }

    fn add_log(&mut self, direction: &'static str, text: impl Into<String>) {
        self.log.push(LogEntry {
            direction,
            text: text.into(),
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn connect(&mut self) {
        if self.connected() {
            return;
        }
        if self.selected_port.is_empty() {
            self.status = "接続するCDCポートを選択してください".to_owned();
            return;
        }

        let path = self.selected_port.clone();
        let (command_tx, event_rx) = spawn_serial_worker(path.clone());
        self.connection = Some(Connection {
            path: path.clone(),
            opened: false,
            command_tx,
            event_rx,
        });
        self.status = format!("{path}へ接続中…");
        self.add_log("SYS", format!("CDC接続開始: {path}"));
    }

    fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.command_tx.send(WorkerCommand::Close);
            self.add_log("SYS", format!("CDC切断: {}", connection.path));
        }
        self.status = "未接続".to_owned();
    }

    fn set_endpoint(&mut self, endpoint: EndpointKind) {
        if self.endpoint == endpoint {
            return;
        }
        if self.connection.is_some() {
            self.status = "対象を変更する前にCDC接続を切断してください".to_owned();
            return;
        }
        self.endpoint = endpoint;
        self.config_text = match endpoint {
            EndpointKind::Node => default_config_text(),
            EndpointKind::Hub => default_hub_config_text(),
            EndpointKind::Database => String::new(),
        };
        self.status = format!("{}用の設定画面に切り替えました", endpoint.label());
    }

    fn open_database(&mut self) {
        let path = self.database_path.trim();
        if path.is_empty() {
            self.status = "SQLite DBファイルのパスを入力してください".to_owned();
            return;
        }
        match SqliteDatabase::open(path) {
            Ok(database) => {
                self.database = Some(database);
                self.status = format!("SQLiteを開きました: {path}");
                self.refresh_database_data();
            }
            Err(error) => self.status = error,
        }
    }

    fn close_database(&mut self) {
        self.database = None;
        self.devices.clear();
        self.families.clear();
        self.actions.clear();
        self.notification_destinations.clear();
        self.events.clear();
        self.status = "SQLiteを閉じました".to_owned();
    }

    fn refresh_database_data(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        let devices = database.list_devices();
        let families = database.list_families();
        let actions = database.list_actions();
        let notification_destinations = database.list_notification_destinations();
        let events = database.list_events();
        match (
            devices,
            families,
            actions,
            notification_destinations,
            events,
        ) {
            (Ok(devices), Ok(families), Ok(actions), Ok(notification_destinations), Ok(events)) => {
                self.devices = devices;
                self.families = families;
                self.actions = actions;
                self.notification_destinations = notification_destinations;
                self.events = events;
                self.status = "SQLiteの設定を読み込みました".to_owned();
            }
            (Err(error), _, _, _, _)
            | (_, Err(error), _, _, _)
            | (_, _, Err(error), _, _)
            | (_, _, _, Err(error), _)
            | (_, _, _, _, Err(error)) => self.status = error,
        }
    }

    fn select_database_section(&mut self, section: DatabaseSection) {
        self.database_section = section;
        self.selected_device = None;
        self.selected_family = None;
        self.selected_action = None;
        self.selected_notification_destination = None;
    }

    fn new_device(&mut self) {
        self.selected_device = None;
        self.device_form = DeviceRecord {
            device_type: "pico-w".to_owned(),
            enabled: true,
            ..DeviceRecord::default()
        };
    }

    fn new_family(&mut self) {
        self.selected_family = None;
        self.family_form = FamilyRecord {
            enabled: true,
            ..FamilyRecord::default()
        };
    }

    fn new_action(&mut self) {
        self.selected_action = None;
        self.action_form = ActionRecord {
            target_type: "FAMILY".to_owned(),
            enabled: true,
            ..ActionRecord::default()
        };
        self.state_type_input = "MEAL_NOTICE".to_owned();
        self.state_value_input = "ON".to_owned();
    }

    fn new_notification_destination(&mut self) {
        self.selected_notification_destination = None;
        self.notification_destination_form = NotificationDestinationRecord {
            notification_type: "LINE".to_owned(),
            enabled: true,
            ..NotificationDestinationRecord::default()
        };
    }

    fn save_device(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.save_device(&self.device_form) {
            Ok(()) => {
                self.status = "子機設定を保存しました".to_owned();
                self.refresh_database_data();
                self.selected_device = Some(self.device_form.device_id.clone());
            }
            Err(error) => self.status = error,
        }
    }

    fn delete_selected_device(&mut self) {
        let Some(device_id) = self.selected_device.as_deref() else {
            self.status = "削除する子機を選択してください".to_owned();
            return;
        };
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.delete_device(device_id) {
            Ok(()) => {
                self.status = "子機設定を削除しました".to_owned();
                self.new_device();
                self.refresh_database_data();
            }
            Err(error) => self.status = error,
        }
    }

    fn save_family(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.save_family(&self.family_form) {
            Ok(()) => {
                self.status = "家族設定を保存しました".to_owned();
                self.refresh_database_data();
                self.selected_family = Some(self.family_form.family_id);
            }
            Err(error) => self.status = error,
        }
    }

    fn delete_selected_family(&mut self) {
        let Some(family_id) = self.selected_family else {
            self.status = "削除する家族を選択してください".to_owned();
            return;
        };
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.delete_family(family_id) {
            Ok(()) => {
                self.status = "家族設定を削除しました".to_owned();
                self.new_family();
                self.refresh_database_data();
            }
            Err(error) => self.status = error,
        }
    }

    fn save_action(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.save_action(&self.action_form) {
            Ok(()) => {
                self.status = "Action設定を保存しました".to_owned();
                self.refresh_database_data();
                self.selected_action = Some(self.action_form.action_id);
            }
            Err(error) => self.status = error,
        }
    }

    fn delete_selected_action(&mut self) {
        let Some(action_id) = self.selected_action else {
            self.status = "削除するActionを選択してください".to_owned();
            return;
        };
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.delete_action(action_id) {
            Ok(()) => {
                self.status = "Action設定を削除しました".to_owned();
                self.new_action();
                self.refresh_database_data();
            }
            Err(error) => self.status = error,
        }
    }

    fn save_notification_destination(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.save_notification_destination(&self.notification_destination_form) {
            Ok(()) => {
                self.status = "通知先を保存しました".to_owned();
                self.refresh_database_data();
                self.selected_notification_destination = self.notification_destination_form.id;
            }
            Err(error) => self.status = error,
        }
    }

    fn delete_selected_notification_destination(&mut self) {
        let Some(id) = self.selected_notification_destination else {
            self.status = "削除する通知先を選択してください".to_owned();
            return;
        };
        let Some(database) = self.database.as_ref() else {
            self.status = "先にSQLiteを開いてください".to_owned();
            return;
        };
        match database.delete_notification_destination(id) {
            Ok(()) => {
                self.status = "通知先を削除しました".to_owned();
                self.new_notification_destination();
                self.refresh_database_data();
            }
            Err(error) => self.status = error,
        }
    }

    fn send_simple_command(&mut self, command: &'static str) {
        self.send_command(command, json!({}));
    }

    fn send_set_config(&mut self) {
        let params = match serde_json::from_str::<Value>(&self.config_text) {
            Ok(value) if value.is_object() => value,
            Ok(_) => {
                self.status = "設定JSONはオブジェクトで指定してください".to_owned();
                return;
            }
            Err(error) => {
                self.status = format!("設定JSONの解析失敗: {error}");
                return;
            }
        };
        self.send_command("set_config", params);
    }

    fn send_command(&mut self, command: &str, params: Value) {
        let Some(connection) = self.connection.as_ref() else {
            self.status = format!("{}に接続してください", self.endpoint.label());
            return;
        };
        if !connection.opened {
            self.status = "CDCポートを開いています。少し待ってください".to_owned();
            return;
        }

        self.request_number = self.request_number.saturating_add(1);
        let line = match build_request(command, params, self.request_number) {
            Ok(line) => line,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let display = line.trim_end_matches('\n').to_owned();
        if let Err(error) = connection.command_tx.send(WorkerCommand::Send(line)) {
            self.status = format!("CDC送信失敗: {error}");
            return;
        }
        self.add_log("TX", pretty_json(&display));
        self.status = format!("{command} を送信しました");
    }

    fn poll_events(&mut self) {
        let mut events = Vec::new();
        if let Some(connection) = self.connection.as_ref() {
            loop {
                match connection.event_rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }

        for event in events {
            match event {
                WorkerEvent::Opened => {
                    if let Some(connection) = self.connection.as_mut() {
                        connection.opened = true;
                        self.status = format!("接続中: {}", connection.path);
                    }
                    self.add_log("SYS", "CDCポートを開きました");
                }
                WorkerEvent::Line(line) => self.handle_response(&line),
                WorkerEvent::Error(error) => {
                    self.status = error.clone();
                    self.add_log("ERR", error);
                }
                WorkerEvent::Closed => {
                    self.status = "未接続".to_owned();
                    self.add_log("SYS", "CDCポートを閉じました");
                }
            }
        }
    }

    fn handle_response(&mut self, line: &str) {
        self.add_log("RX", pretty_json(line));
        let response = match parse_response(line) {
            Ok(response) => response,
            Err(error) => {
                self.status = error;
                return;
            }
        };

        match response.status {
            CdcStatus::Ok => {
                self.status = format!("応答成功: {}", response.request_id);
                if let Some(data) = response.data.as_ref()
                    && (data.get("gpio_inputs").is_some() || data.get("families").is_some())
                {
                    self.config_text = serde_json::to_string_pretty(data).unwrap_or_default();
                }
            }
            CdcStatus::Error => {
                if let Some(error) = response.error {
                    self.status = format!("{:?}: {}", error.code, error.message);
                } else {
                    self.status = "CDCエラー応答を受信しました".to_owned();
                }
            }
        }
    }

    fn show_confirmation(&mut self, context: &egui::Context) {
        let Some(action) = self.confirmation else {
            return;
        };
        egui::Window::new("確認")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(action.title(self.endpoint));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("キャンセル").clicked() {
                        self.confirmation = None;
                    }
                    if ui.button("実行").clicked() {
                        self.confirmation = None;
                        self.send_simple_command(action.command());
                    }
                });
            });
    }
}

impl eframe::App for ControlApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
        if self.connection.is_some() {
            context.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("connection_toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Uchi-Pulse Settings").strong());
                ui.separator();
                ui.label("対象");
                egui::ComboBox::from_id_salt("endpoint")
                    .selected_text(self.endpoint.label())
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.endpoint == EndpointKind::Node, "子機")
                            .clicked()
                        {
                            self.set_endpoint(EndpointKind::Node);
                        }
                        if ui
                            .selectable_label(self.endpoint == EndpointKind::Hub, "親機")
                            .clicked()
                        {
                            self.set_endpoint(EndpointKind::Hub);
                        }
                        if ui
                            .selectable_label(self.endpoint == EndpointKind::Database, "SQLite")
                            .clicked()
                        {
                            self.set_endpoint(EndpointKind::Database);
                        }
                    });
                if self.endpoint == EndpointKind::Database {
                    ui.separator();
                    ui.label("DBパス");
                    ui.add(
                        TextEdit::singleline(&mut self.database_path)
                            .desired_width(280.0)
                            .hint_text("例: ./uchi-pulse.db"),
                    );
                    if ui.button("開く").clicked() {
                        self.open_database();
                    }
                    if self.database.is_some() && ui.button("閉じる").clicked() {
                        self.close_database();
                    }
                } else {
                    ui.separator();
                    ui.label("ポート");
                    egui::ComboBox::from_id_salt("serial_port")
                        .selected_text(if self.selected_port.is_empty() {
                            "未選択"
                        } else {
                            &self.selected_port
                        })
                        .show_ui(ui, |ui| {
                            for port in self.ports.clone() {
                                ui.selectable_value(&mut self.selected_port, port.clone(), port);
                            }
                        });
                    if ui.button("更新").clicked() {
                        self.refresh_ports();
                    }
                    if self.connection.is_some() {
                        if ui.button("切断").clicked() {
                            self.disconnect();
                        }
                    } else if ui.button("接続").clicked() {
                        self.connect();
                    }
                }
                ui.separator();
                ui.small(&self.font_info);
                ui.separator();
                let color = if self.connected() {
                    Color32::LIGHT_GREEN
                } else {
                    Color32::GRAY
                };
                ui.colored_label(color, &self.status);
            });
        });

        if self.endpoint == EndpointKind::Database {
            self.show_database_ui(ui);
            return;
        }

        egui::Panel::left("commands")
            .resizable(true)
            .default_size(150.0)
            .min_size(120.0)
            .max_size(320.0)
            .show(ui, |ui| {
                ui.heading("コマンド");
                ui.add_space(8.0);
                let connected = self.connected();
                for &(command, label) in self.endpoint.commands() {
                    if ui
                        .add_enabled(connected, egui::Button::new(label))
                        .clicked()
                    {
                        self.send_simple_command(command);
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(connected, egui::Button::new("設定保存"))
                    .clicked()
                {
                    self.send_set_config();
                }
                if ui
                    .add_enabled(connected, egui::Button::new("設定初期化"))
                    .clicked()
                {
                    self.confirmation = Some(ConfirmationAction::FactoryReset);
                }
                if ui
                    .add_enabled(connected, egui::Button::new("再起動"))
                    .clicked()
                {
                    self.confirmation = Some(ConfirmationAction::Reboot);
                }
                ui.separator();
                if ui.button("ログ消去").clicked() {
                    self.log.clear();
                }
            });

        let editor_max_height = (ui.available_height() - 160.0).max(180.0);
        egui::Panel::top("configuration_editor")
            .resizable(true)
            .show_separator_line(true)
            .default_size(360.0)
            .min_size(180.0)
            .max_size(editor_max_height)
            .show(ui, |ui| {
                ui.heading(format!("{}設定", self.endpoint.label()));
                ui.label(format!(
                    "設定取得で{}の現在値を読み込み、JSONを編集して設定保存できます。",
                    self.endpoint.label()
                ));
                ui.add_space(6.0);
                ui.group(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("config_editor")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                TextEdit::multiline(&mut self.config_text)
                                    .frame(egui::Frame::NONE)
                                    .font(TextStyle::Monospace)
                                    .desired_rows(16)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("通信ログ");
            egui::ScrollArea::vertical()
                .id_salt("communication_log")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for entry in &self.log {
                        let color = match entry.direction {
                            "TX" => Color32::LIGHT_BLUE,
                            "RX" => Color32::LIGHT_GREEN,
                            "ERR" => Color32::LIGHT_RED,
                            _ => Color32::GRAY,
                        };
                        ui.colored_label(
                            color,
                            RichText::new(format!("[{}] {}", entry.direction, entry.text))
                                .monospace(),
                        );
                    }
                });
        });

        self.show_confirmation(ui.ctx());
    }
}

impl ControlApp {
    fn show_database_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("親機設定");
            ui.separator();
            for section in [
                DatabaseSection::Devices,
                DatabaseSection::Families,
                DatabaseSection::Actions,
                DatabaseSection::Destinations,
                DatabaseSection::Events,
            ] {
                if ui
                    .selectable_label(self.database_section == section, section.label())
                    .clicked()
                {
                    self.select_database_section(section);
                }
            }
            if ui
                .add_enabled(self.database.is_some(), egui::Button::new("再読込"))
                .clicked()
            {
                self.refresh_database_data();
            }
        });
        ui.colored_label(
            Color32::YELLOW,
            "親機を停止してから設定を変更してください。保存後は親機を再起動してください。",
        );
        ui.separator();

        if self.database.is_none() {
            ui.heading("SQLiteデータベースを開いてください");
            ui.label("画面上部のDBパスを指定して「開く」を押してください。");
            return;
        }

        match self.database_section {
            DatabaseSection::Devices => self.show_device_editor(ui),
            DatabaseSection::Families => self.show_family_editor(ui),
            DatabaseSection::Actions => self.show_action_editor(ui),
            DatabaseSection::Destinations => self.show_destination_editor(ui),
            DatabaseSection::Events => self.show_event_viewer(ui),
        }
    }

    fn show_device_editor(&mut self, ui: &mut egui::Ui) {
        let devices = self.devices.clone();
        let mut save = false;
        let mut delete = false;
        ui.columns(2, |columns| {
            columns[0].heading("子機一覧");
            if columns[0].button("新規").clicked() {
                self.new_device();
            }
            for device in &devices {
                let selected = self.selected_device.as_deref() == Some(device.device_id.as_str());
                if columns[0]
                    .selectable_label(
                        selected,
                        format!(
                            "{}{}",
                            device.name,
                            if device.enabled { "" } else { "（無効）" }
                        ),
                    )
                    .clicked()
                {
                    self.selected_device = Some(device.device_id.clone());
                    self.device_form = device.clone();
                }
            }

            columns[1].heading("子機情報");
            columns[1].label("子機ID");
            columns[1].add_enabled(
                self.selected_device.is_none(),
                TextEdit::singleline(&mut self.device_form.device_id),
            );
            columns[1].label("表示名");
            columns[1].add(TextEdit::singleline(&mut self.device_form.name));
            columns[1].label("種別");
            columns[1].add(TextEdit::singleline(&mut self.device_form.device_type));
            columns[1].checkbox(&mut self.device_form.enabled, "有効");
            columns[1].horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save = true;
                }
                if ui
                    .add_enabled(self.selected_device.is_some(), egui::Button::new("削除"))
                    .clicked()
                {
                    delete = true;
                }
            });
        });
        if save {
            self.save_device();
        }
        if delete {
            self.delete_selected_device();
        }
    }

    fn show_family_editor(&mut self, ui: &mut egui::Ui) {
        let families = self.families.clone();
        let mut save = false;
        let mut delete = false;
        ui.columns(2, |columns| {
            columns[0].heading("家族一覧");
            if columns[0].button("新規").clicked() {
                self.new_family();
            }
            for family in &families {
                let selected = self.selected_family == Some(family.family_id);
                if columns[0]
                    .selectable_label(
                        selected,
                        format!(
                            "{}（ID: {}）{}",
                            family.display_name,
                            family.family_id,
                            if family.enabled { "" } else { "（無効）" }
                        ),
                    )
                    .clicked()
                {
                    self.selected_family = Some(family.family_id);
                    self.family_form = family.clone();
                }
            }

            columns[1].heading("家族情報");
            columns[1].label("家族ID");
            columns[1].add_enabled(
                self.selected_family.is_none(),
                egui::DragValue::new(&mut self.family_form.family_id).range(1..=u32::MAX),
            );
            columns[1].label("表示名");
            columns[1].add(TextEdit::singleline(&mut self.family_form.display_name));
            columns[1].checkbox(&mut self.family_form.enabled, "有効");
            columns[1].horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save = true;
                }
                if ui
                    .add_enabled(self.selected_family.is_some(), egui::Button::new("削除"))
                    .clicked()
                {
                    delete = true;
                }
            });
        });
        if save {
            self.save_family();
        }
        if delete {
            self.delete_selected_family();
        }
    }

    fn show_action_editor(&mut self, ui: &mut egui::Ui) {
        let actions = self.actions.clone();
        let families = self.families.clone();
        let mut save = false;
        let mut delete = false;
        ui.columns(2, |columns| {
            columns[0].heading("Action一覧");
            if columns[0].button("新規").clicked() {
                self.new_action();
            }
            for action in &actions {
                let selected = self.selected_action == Some(action.action_id);
                if columns[0]
                    .selectable_label(
                        selected,
                        format!(
                            "{}（ID: {}）{}",
                            action.action_name,
                            action.action_id,
                            if action.enabled { "" } else { "（無効）" }
                        ),
                    )
                    .clicked()
                {
                    self.selected_action = Some(action.action_id);
                    self.action_form = action.clone();
                }
            }

            columns[1].heading("Action情報");
            columns[1].label("Action ID");
            columns[1].add_enabled(
                self.selected_action.is_none(),
                egui::DragValue::new(&mut self.action_form.action_id).range(1..=u32::MAX),
            );
            columns[1].label("Action名");
            columns[1].add(TextEdit::singleline(&mut self.action_form.action_name));
            columns[1].horizontal(|ui| {
                ui.label("対象");
                egui::ComboBox::from_id_salt("action_target_type")
                    .selected_text(&self.action_form.target_type)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.action_form.target_type,
                            "FAMILY".to_owned(),
                            "家族",
                        );
                        ui.selectable_value(
                            &mut self.action_form.target_type,
                            "COMMON".to_owned(),
                            "共通",
                        );
                    });
            });
            if self.action_form.target_type == "FAMILY" {
                let selected_name = self
                    .action_form
                    .target_family_id
                    .and_then(|id| families.iter().find(|family| family.family_id == id))
                    .map(|family| family.display_name.clone())
                    .unwrap_or_else(|| "未選択".to_owned());
                columns[1].horizontal(|ui| {
                    ui.label("対象家族");
                    egui::ComboBox::from_id_salt("action_target_family")
                        .selected_text(selected_name)
                        .show_ui(ui, |ui| {
                            for family in &families {
                                ui.selectable_value(
                                    &mut self.action_form.target_family_id,
                                    Some(family.family_id),
                                    format!("{}（ID: {}）", family.display_name, family.family_id),
                                );
                            }
                        });
                });
            } else {
                self.action_form.target_family_id = None;
            }
            columns[1].label("Web表示メッセージ（任意）");
            columns[1].add(TextEdit::singleline(&mut self.action_form.web_message));
            columns[1].checkbox(&mut self.action_form.enabled, "有効");

            columns[1].separator();
            columns[1].label("状態変更");
            let mut remove_change = None;
            for (index, change) in self.action_form.state_changes.iter().enumerate() {
                columns[1].horizontal(|ui| {
                    ui.label(format!("{} = {}", change.state_type, change.state_value));
                    if ui.small_button("削除").clicked() {
                        remove_change = Some(index);
                    }
                });
            }
            if let Some(index) = remove_change {
                self.action_form.state_changes.remove(index);
            }
            columns[1].horizontal(|ui| {
                egui::ComboBox::from_id_salt("state_type_input")
                    .selected_text(&self.state_type_input)
                    .show_ui(ui, |ui| {
                        for value in [
                            "ENTRY_PERMISSION",
                            "MEAL_NOTICE",
                            "SNACK_NOTICE",
                            "HELP_NOTICE",
                            "MAILBOX",
                        ] {
                            ui.selectable_value(
                                &mut self.state_type_input,
                                value.to_owned(),
                                value,
                            );
                        }
                    });
                egui::ComboBox::from_id_salt("state_value_input")
                    .selected_text(&self.state_value_input)
                    .show_ui(ui, |ui| {
                        for value in ["UNSET", "ON", "OFF", "OK", "NG", "MEETING"] {
                            ui.selectable_value(
                                &mut self.state_value_input,
                                value.to_owned(),
                                value,
                            );
                        }
                    });
                if ui.button("状態変更を追加").clicked()
                    && !self
                        .action_form
                        .state_changes
                        .iter()
                        .any(|change| change.state_type == self.state_type_input)
                {
                    self.action_form.state_changes.push(StateChangeRecord {
                        state_type: self.state_type_input.clone(),
                        state_value: self.state_value_input.clone(),
                    });
                }
            });

            columns[1].separator();
            columns[1].label("通知設定");
            columns[1].checkbox(
                &mut self.action_form.notification_enabled,
                "通知を有効にする",
            );
            columns[1].add(TextEdit::singleline(
                &mut self.action_form.notification_message,
            ));
            columns[1].label("通知先家族");
            for family in &families {
                let mut selected = self
                    .action_form
                    .notification_targets
                    .contains(&family.family_id);
                if columns[1]
                    .checkbox(
                        &mut selected,
                        format!("{}（ID: {}）", family.display_name, family.family_id),
                    )
                    .changed()
                {
                    if selected {
                        self.action_form.notification_targets.push(family.family_id);
                    } else {
                        self.action_form
                            .notification_targets
                            .retain(|id| *id != family.family_id);
                    }
                }
            }

            columns[1].horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save = true;
                }
                if ui
                    .add_enabled(self.selected_action.is_some(), egui::Button::new("削除"))
                    .clicked()
                {
                    delete = true;
                }
            });
        });
        if save {
            self.save_action();
        }
        if delete {
            self.delete_selected_action();
        }
    }

    fn show_destination_editor(&mut self, ui: &mut egui::Ui) {
        let destinations = self.notification_destinations.clone();
        let families = self.families.clone();
        let mut save = false;
        let mut delete = false;
        ui.columns(2, |columns| {
            columns[0].heading("通知先一覧");
            if columns[0].button("新規").clicked() {
                self.new_notification_destination();
            }
            for destination in &destinations {
                let Some(id) = destination.id else {
                    continue;
                };
                let family_name = families
                    .iter()
                    .find(|family| family.family_id == destination.family_id)
                    .map(|family| family.display_name.as_str())
                    .unwrap_or("不明な家族");
                if columns[0]
                    .selectable_label(
                        self.selected_notification_destination == Some(id),
                        format!("{}: {}", family_name, destination.notification_type),
                    )
                    .clicked()
                {
                    self.selected_notification_destination = Some(id);
                    self.notification_destination_form = destination.clone();
                }
            }

            columns[1].heading("通知先情報");
            let selected_family_name = families
                .iter()
                .find(|family| family.family_id == self.notification_destination_form.family_id)
                .map(|family| family.display_name.clone())
                .unwrap_or_else(|| "未選択".to_owned());
            columns[1].horizontal(|ui| {
                ui.label("家族");
                egui::ComboBox::from_id_salt("destination_family")
                    .selected_text(selected_family_name)
                    .show_ui(ui, |ui| {
                        for family in &families {
                            ui.selectable_value(
                                &mut self.notification_destination_form.family_id,
                                family.family_id,
                                format!("{}（ID: {}）", family.display_name, family.family_id),
                            );
                        }
                    });
            });
            columns[1].horizontal(|ui| {
                ui.label("通知種別");
                egui::ComboBox::from_id_salt("destination_type")
                    .selected_text(&self.notification_destination_form.notification_type)
                    .show_ui(ui, |ui| {
                        for value in ["LINE", "Slack", "メール", "その他"] {
                            ui.selectable_value(
                                &mut self.notification_destination_form.notification_type,
                                value.to_owned(),
                                value,
                            );
                        }
                    });
            });
            columns[1].label("送信先ID・アドレス");
            columns[1].add(TextEdit::singleline(
                &mut self.notification_destination_form.destination,
            ));
            columns[1].checkbox(&mut self.notification_destination_form.enabled, "有効");
            columns[1].horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save = true;
                }
                if ui
                    .add_enabled(
                        self.selected_notification_destination.is_some(),
                        egui::Button::new("削除"),
                    )
                    .clicked()
                {
                    delete = true;
                }
            });
        });
        if save {
            self.save_notification_destination();
        }
        if delete {
            self.delete_selected_notification_destination();
        }
    }

    fn show_event_viewer(&mut self, ui: &mut egui::Ui) {
        ui.heading("イベント履歴（最新500件）");
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("event_grid").striped(true).show(ui, |ui| {
                for heading in ["ID", "受信日時", "子機ID", "イベントID"] {
                    ui.label(RichText::new(heading).strong());
                }
                ui.end_row();
                for event in &self.events {
                    ui.label(event.id.to_string());
                    ui.label(&event.received_at);
                    ui.label(&event.device_id);
                    ui.label(&event.event_id);
                    ui.end_row();
                }
            });
        });
    }
}

fn pretty_json(value: &str) -> String {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| value.to_owned())
}

fn install_japanese_font(context: &egui::Context) -> Option<String> {
    let mut candidates = vec![
        PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
        PathBuf::from("/System/Library/Fonts/Supplemental/AppleGothic.ttf"),
        PathBuf::from("/Library/Fonts/Arial Unicode.ttf"),
        PathBuf::from("C:/Windows/Fonts/meiryo.ttc"),
        PathBuf::from("C:/Windows/Fonts/msgothic.ttc"),
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.insert(
            0,
            home.join("Library/Fonts/NotoSansJP-VariableFont_wght.ttf"),
        );
        candidates.push(home.join(".fonts/NotoSansJP-VariableFont_wght.ttf"));
    }

    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let mut definitions = egui::FontDefinitions::default();
        definitions.font_data.insert(
            "uchi-pulse-japanese".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(fonts) = definitions.families.get_mut(&family) {
                fonts.insert(0, "uchi-pulse-japanese".to_owned());
            }
        }
        context.set_fonts(definitions);
        return Some(path.display().to_string());
    }
    None
}

fn default_config_text() -> String {
    serde_json::to_string_pretty(&json!({
        "device_id": "node-01",
        "wifi": {
            "ssid": "change-me",
            "password": "change-me"
        },
        "network": {
            "mode": "DHCP",
            "static_ipv4": null
        },
        "gpio_inputs": [],
        "input_mappings": [],
        "double_click_interval_ms": 400,
        "long_press_threshold_ms": 1000,
        "ack_timeout_ms": 60000,
        "event_retry_count": 3,
        "heartbeat_interval_sec": 180
    }))
    .unwrap_or_default()
}

fn default_hub_config_text() -> String {
    serde_json::to_string_pretty(&json!({
        "families": [],
        "actions": [],
        "family_notification_destinations": []
    }))
    .unwrap_or_default()
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([860.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Uchi-Pulse Settings",
        options,
        Box::new(|context| Ok(Box::new(ControlApp::new(context)))),
    )
}
