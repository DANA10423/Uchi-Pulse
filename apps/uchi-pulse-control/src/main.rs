use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use eframe::egui::{self, Color32, RichText, TextEdit, TextStyle};
use serde_json::{Value, json};
use uchi_pulse_common::cdc::CdcStatus;

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
}

impl EndpointKind {
    fn label(self) -> &'static str {
        match self {
            Self::Node => "子機",
            Self::Hub => "親機",
        }
    }

    fn commands(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Node => COMMANDS,
            Self::Hub => HUB_COMMANDS,
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
        };
        self.status = format!("{}用の設定画面に切り替えました", endpoint.label());
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
                ui.label(RichText::new("Uchi-Pulse CDC Control").strong());
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
                    });
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

        egui::Panel::left("commands")
            .resizable(false)
            .default_size(150.0)
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(format!("{}設定", self.endpoint.label()));
            ui.label(format!(
                "設定取得で{}の現在値を読み込み、JSONを編集して設定保存できます。",
                self.endpoint.label()
            ));
            ui.add_space(6.0);
            ui.add(
                TextEdit::multiline(&mut self.config_text)
                    .font(TextStyle::Monospace)
                    .desired_rows(16)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(10.0);
            ui.separator();
            ui.heading("通信ログ");
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height(260.0)
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
        "Uchi-Pulse CDC Control",
        options,
        Box::new(|context| Ok(Box::new(ControlApp::new(context)))),
    )
}
