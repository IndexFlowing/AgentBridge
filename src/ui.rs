//! Desktop tray console: start/stop the MCP gateway and edit local config.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, RichText, Stroke};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config::{self, Config, ExecutorConfig, SecurityConfig};
use crate::projects::{self, ProjectEntry, ProjectHub};
use crate::server::{self, ServeHandle, ServeOptions};

const ACCENT: Color32 = Color32::from_rgb(0x3d, 0xd6, 0xc6);
const BG: Color32 = Color32::from_rgb(0x0f, 0x14, 0x19);
const CARD: Color32 = Color32::from_rgb(0x1a, 0x22, 0x2c);
const TEXT: Color32 = Color32::from_rgb(0xe7, 0xec, 0xf1);
const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa6, 0xb2);
const DANGER: Color32 = Color32::from_rgb(0xff, 0x8d, 0x8d);

pub fn run() -> Result<()> {
    let mut native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AgentBridge")
            .with_inner_size([760.0, 700.0])
            .with_min_inner_size([580.0, 520.0]),
        ..Default::default()
    };
    native.persist_window = false;

    eframe::run_native(
        "AgentBridge",
        native,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            apply_fonts(&cc.egui_ctx);
            Ok(Box::new(TrayApp::load()))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

struct TrayApp {
    cfg_path: PathBuf,
    json_path: PathBuf,
    host: String,
    port: String,
    allow_any_host: bool,
    no_auth: bool,
    admin_password: String,
    auth_token: String,
    client_id: String,
    client_secret: String,
    executor_command: String,
    executor: ExecutorConfig,
    security: SecurityConfig,
    projects: Vec<ProjectRow>,
    default_project: String,
    auto_launch: bool,
    auto_start: bool,
    show_secrets: bool,
    status: String,
    error: Option<String>,
    info: Option<String>,
    running: Option<ServeHandle>,
    tray: Option<TrayIcon>,
    menu_show: MenuItem,
    menu_copy_url: MenuItem,
    menu_copy_pin: MenuItem,
    menu_start: MenuItem,
    menu_stop: MenuItem,
    menu_quit: MenuItem,
    quit: bool,
}

#[derive(Clone)]
struct ProjectRow {
    name: String,
    path: String,
    description: String,
    readonly: bool,
}

impl TrayApp {
    fn load() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let (cfg, cfg_path) = config::find_config(None).unwrap_or_else(|_| {
            let path = cwd.join(".agentbridge.toml");
            (Config::new(cwd.clone()), path)
        });
        let json_path = cfg_path
            .parent()
            .unwrap_or(cwd.as_path())
            .join("agentbridge.config.json");
        let (projects, default_project) = if json_path.is_file() {
            projects::load_workspaces_file(&json_path).unwrap_or_else(|_| {
                (
                    vec![default_entry(&cfg.workspace)],
                    Some("default".into()),
                )
            })
        } else {
            (
                vec![default_entry(&cfg.workspace)],
                Some("default".into()),
            )
        };
        let default_project = default_project.unwrap_or_else(|| {
            projects
                .first()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "default".into())
        });

        let auto_launch = auto_launcher()
            .ok()
            .and_then(|a| a.is_enabled().ok())
            .unwrap_or(false);

        let mut app = Self {
            cfg_path,
            json_path,
            host: cfg.host.clone(),
            port: cfg.port.to_string(),
            allow_any_host: cfg.allow_any_host,
            no_auth: cfg.no_auth,
            admin_password: cfg.admin_password.clone().unwrap_or_default(),
            auth_token: cfg.auth_token.clone().unwrap_or_default(),
            client_id: cfg.client_id.clone().unwrap_or_default(),
            client_secret: cfg.client_secret.clone().unwrap_or_default(),
            executor_command: cfg.executor.command.clone(),
            executor: cfg.executor.clone(),
            security: cfg.security.clone(),
            projects: projects.into_iter().map(ProjectRow::from).collect(),
            default_project,
            auto_launch,
            auto_start: true,
            show_secrets: false,
            status: "已停止".into(),
            error: None,
            info: None,
            running: None,
            tray: None,
            menu_show: MenuItem::new("打开设置", true, None),
            menu_copy_url: MenuItem::new("复制 MCP 地址", true, None),
            menu_copy_pin: MenuItem::new("复制 Admin PIN", true, None),
            menu_start: MenuItem::new("启动服务", true, None),
            menu_stop: MenuItem::new("停止服务", true, None),
            menu_quit: MenuItem::new("退出", true, None),
            quit: false,
        };
        app.install_tray();
        app
    }

    fn install_tray(&mut self) {
        let menu = Menu::new();
        let _ = menu.append(&self.menu_show);
        let _ = menu.append(&self.menu_copy_url);
        let _ = menu.append(&self.menu_copy_pin);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&self.menu_start);
        let _ = menu.append(&self.menu_stop);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&self.menu_quit);
        match TrayIconBuilder::new()
            .with_tooltip("AgentBridge")
            .with_icon(tray_icon_image())
            .with_menu(Box::new(menu))
            .build()
        {
            Ok(icon) => self.tray = Some(icon),
            Err(err) => self.error = Some(format!("托盘图标不可用：{err}")),
        }
    }

    fn to_config(&self) -> Result<Config> {
        let port: u16 = self
            .port
            .trim()
            .parse()
            .context("端口必须是 1–65535 的数字")?;
        let default_path = self
            .projects
            .iter()
            .find(|p| p.name == self.default_project)
            .or_else(|| self.projects.first())
            .map(|p| PathBuf::from(p.path.trim()))
            .filter(|p| !p.as_os_str().is_empty())
            .context("请至少添加一个项目")?;
        let mut cfg = Config::new(std::path::absolute(default_path)?);
        cfg.host = self.host.trim().to_string();
        cfg.port = port;
        cfg.allow_any_host = self.allow_any_host;
        cfg.no_auth = self.no_auth;
        cfg.auth_token = Some(self.auth_token.trim().to_string());
        cfg.admin_password = Some(self.admin_password.trim().to_string());
        cfg.client_id = Some(self.client_id.trim().to_string());
        cfg.client_secret = Some(self.client_secret.trim().to_string());
        cfg.executor = self.executor.clone();
        cfg.executor.command = if self.executor_command.trim().is_empty() {
            cfg.executor.kind.clone()
        } else {
            self.executor_command.trim().to_string()
        };
        cfg.security = self.security.clone();
        Ok(cfg)
    }

    fn project_entries(&self) -> Result<Vec<ProjectEntry>> {
        if self.projects.is_empty() {
            anyhow::bail!("请至少添加一个项目");
        }
        let mut out = Vec::new();
        for row in &self.projects {
            let name = crate::projects::validate_project_name(&row.name)?;
            let path = PathBuf::from(row.path.trim());
            if !path.is_dir() {
                anyhow::bail!("项目 `{name}` 的路径不是目录：{}", path.display());
            }
            out.push(ProjectEntry {
                id: String::new(),
                name,
                path: std::path::absolute(path)?,
                description: row.description.trim().to_string(),
                readonly: row.readonly,
                executor: crate::projects::default_project_executor(),
            });
        }
        Ok(out)
    }

    fn save_files(&mut self) -> Result<()> {
        let cfg = self.to_config()?;
        cfg.save_to_path(&self.cfg_path)?;
        let entries = self.project_entries()?;
        let default = if entries.iter().any(|e| e.name == self.default_project) {
            Some(self.default_project.clone())
        } else {
            entries.first().map(|e| e.name.clone())
        };
        projects::save_workspaces_file(&self.json_path, &entries, default)?;
        self.info = Some(format!(
            "已保存 {} 和 {}",
            self.cfg_path.display(),
            self.json_path.display()
        ));
        self.error = None;
        Ok(())
    }

    fn start_server(&mut self) {
        if self.running.is_some() {
            return;
        }
        self.error = None;
        self.info = None;
        let cfg = match self.to_config() {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let entries = match self.project_entries() {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if let Err(e) = self.save_files() {
            self.error = Some(e.to_string());
            return;
        }
        let default = Some(self.default_project.clone());
        let hub = match ProjectHub::open(entries, default, Arc::new(cfg.clone())) {
            Ok(h) => h,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let options = ServeOptions {
            allow_any_host: cfg.allow_any_host,
            no_auth: cfg.no_auth,
            client_id: cfg.client_id.clone(),
            client_secret: cfg.client_secret.clone(),
            admin_password: cfg.admin_password.clone(),
        };
        match server::spawn_server(cfg, hub, options) {
            Ok(handle) => {
                self.status = "运行中".into();
                self.info = Some(format!("MCP 已监听 {}", self.mcp_url()));
                self.running = Some(handle);
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.status = "已停止".into();
            }
        }
    }

    fn stop_server(&mut self) {
        if let Some(mut handle) = self.running.take() {
            handle.stop();
        }
        self.status = "已停止".into();
        self.info = Some("服务已停止".into());
    }

    fn mcp_url(&self) -> String {
        format!("http://{}:{}/mcp", self.host.trim(), self.port.trim())
    }

    fn authorize_url(&self) -> String {
        format!("http://{}:{}/oauth/authorize", self.host.trim(), self.port.trim())
    }

    fn live_pin(&self) -> String {
        self.running
            .as_ref()
            .map(|h| h.oauth.admin_password_value().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.admin_password.clone())
    }

    fn pin_is_generated(&self) -> bool {
        self.running
            .as_ref()
            .and_then(|h| h.oauth.generated_password())
            .is_some()
    }

    fn add_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("选择项目文件夹").pick_folder() {
            let name = dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into());
            let name = sanitize_name(&name);
            if self.projects.iter().any(|p| p.name == name) {
                self.error = Some(format!("项目名 `{name}` 已存在"));
                return;
            }
            if self.projects.is_empty() {
                self.default_project = name.clone();
            }
            self.projects.push(ProjectRow {
                name,
                path: dir.display().to_string(),
                description: String::new(),
                readonly: false,
            });
        }
    }

    fn apply_auto_launch(&mut self) {
        match auto_launcher() {
            Ok(auto) => {
                let result = if self.auto_launch {
                    auto.enable()
                } else {
                    auto.disable()
                };
                if let Err(err) = result {
                    self.error = Some(format!("开机启动设置失败：{err}"));
                    self.auto_launch = !self.auto_launch;
                }
            }
            Err(err) => {
                self.error = Some(format!("开机启动不可用：{err}"));
                self.auto_launch = false;
            }
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::DoubleClick { .. } | TrayIconEvent::Click { .. }) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.menu_show.id() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if event.id == self.menu_copy_url.id() {
                ctx.copy_text(self.mcp_url());
                self.info = Some("已复制 MCP 地址".into());
            } else if event.id == self.menu_copy_pin.id() {
                ctx.copy_text(self.live_pin());
                self.info = Some("已复制 Admin PIN".into());
            } else if event.id == self.menu_start.id() {
                self.start_server();
            } else if event.id == self.menu_stop.id() {
                self.stop_server();
            } else if event.id == self.menu_quit.id() {
                self.quit = true;
            }
        }
    }
}

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tray(ctx);

        if self.quit {
            self.stop_server();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.info = Some("已最小化到托盘，右键图标可退出".into());
        }

        let running = self.running.is_some();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("AgentBridge").color(ACCENT));
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .color(MUTED)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, color) = if running {
                        ("● 运行中", ACCENT)
                    } else {
                        ("○ 已停止", MUTED)
                    };
                    ui.label(RichText::new(label).color(color).strong());
                });
            });
            ui.label(RichText::new("本机 MCP 网关控制台 · 配置保存在 .agentbridge.toml").color(MUTED));
            ui.add_space(10.0);

            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("连接信息").strong().color(ACCENT));
                ui.add_space(4.0);
                copy_row(ui, "MCP", &self.mcp_url());
                copy_row(ui, "授权页", &self.authorize_url());
                let pin = self.live_pin();
                ui.horizontal(|ui| {
                    ui.label("PIN");
                    let shown = if pin.is_empty() {
                        "（启动后生成，或在下方填写以固定）".to_string()
                    } else if self.show_secrets {
                        pin.clone()
                    } else {
                        "••••••••".into()
                    };
                    ui.monospace(shown);
                    if ui.button("复制").clicked() && !pin.is_empty() {
                        ui.ctx().copy_text(pin);
                        self.info = Some("已复制 Admin PIN".into());
                    }
                    if ui
                        .button(if self.show_secrets { "隐藏" } else { "显示" })
                        .clicked()
                    {
                        self.show_secrets = !self.show_secrets;
                    }
                });
                if self.pin_is_generated() {
                    ui.label(
                        RichText::new("当前 PIN 是本次随机生成的。写入「Admin 密码」并保存后即可固定。")
                            .color(MUTED)
                            .small(),
                    );
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if running {
                    if ui
                        .add(egui::Button::new("停止服务").fill(Color32::from_rgb(0x5a, 0x2a, 0x2a)))
                        .clicked()
                    {
                        self.stop_server();
                    }
                } else if ui
                    .add(egui::Button::new(RichText::new("启动服务").color(Color32::from_rgb(0x06, 0x23, 0x1f))).fill(ACCENT))
                    .on_hover_text("启动 MCP / OAuth")
                    .clicked()
                {
                    self.start_server();
                }
                if ui.button("保存配置").clicked() {
                    if let Err(e) = self.save_files() {
                        self.error = Some(e.to_string());
                    }
                }
                if ui.button("打开授权页").clicked() {
                    let _ = open::that(self.authorize_url());
                }
            });

            if let Some(err) = &self.error {
                ui.colored_label(DANGER, err);
            }
            if let Some(info) = &self.info {
                ui.colored_label(ACCENT, info);
            }

            ui.add_space(8.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("服务").strong().color(ACCENT));
                    ui.add_enabled_ui(!running, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("主机");
                            ui.add(egui::TextEdit::singleline(&mut self.host).desired_width(140.0));
                            ui.label("端口");
                            ui.add(egui::TextEdit::singleline(&mut self.port).desired_width(70.0));
                        });
                        ui.checkbox(&mut self.allow_any_host, "允许任意 Host（Cloudflare Tunnel 需要）");
                        ui.checkbox(&mut self.no_auth, "关闭鉴权（仅本机调试）");
                    });
                    ui.checkbox(&mut self.auto_start, "打开控制台后自动启动服务");
                    if ui
                        .checkbox(&mut self.auto_launch, "开机自动启动 AgentBridge")
                        .changed()
                    {
                        self.apply_auto_launch();
                    }
                    ui.horizontal(|ui| {
                        ui.label("Admin 密码");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.admin_password)
                                .password(!self.show_secrets)
                                .desired_width(220.0)
                                .hint_text("留空则每次随机"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("静态 Bearer");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.auth_token)
                                .password(!self.show_secrets)
                                .desired_width(220.0)
                                .hint_text("可选"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Executor");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.executor_command)
                                .desired_width(220.0)
                                .hint_text("opencode"),
                        );
                    });
                });

                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("项目").strong().color(ACCENT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("＋ 添加文件夹").clicked() {
                                self.add_folder();
                            }
                        });
                    });
                    ui.label(RichText::new("同一进程挂载多个仓库；OAuth 只需授权一次。").color(MUTED).small());
                    ui.add_space(6.0);

                    let mut remove = None;
                    for i in 0..self.projects.len() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("名称");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.projects[i].name)
                                        .desired_width(140.0),
                                );
                                let is_default = self.default_project == self.projects[i].name;
                                if ui.selectable_label(is_default, "默认").clicked() {
                                    self.default_project = self.projects[i].name.clone();
                                }
                                ui.checkbox(&mut self.projects[i].readonly, "只读");
                                if ui.small_button("移除").clicked() {
                                    remove = Some(i);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("路径");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.projects[i].path)
                                        .desired_width(ui.available_width() - 80.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("说明");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.projects[i].description)
                                        .desired_width(ui.available_width() - 80.0),
                                );
                            });
                        });
                        ui.add_space(4.0);
                    }
                    if let Some(i) = remove {
                        let removed = self.projects.remove(i);
                        if self.default_project == removed.name {
                            self.default_project = self
                                .projects
                                .first()
                                .map(|p| p.name.clone())
                                .unwrap_or_default();
                        }
                    }
                });

                ui.add_space(12.0);
                ui.label(
                    RichText::new(format!(
                        "配置文件\n{}\n{}",
                        self.cfg_path.display(),
                        self.json_path.display()
                    ))
                    .small()
                    .color(MUTED),
                );
            });
        });

        if self.auto_start && self.running.is_none() && self.error.is_none() && !self.projects.is_empty()
        {
            // Auto-start once on first frame after load.
            static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !ONCE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                self.start_server();
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_server();
    }
}

impl From<ProjectEntry> for ProjectRow {
    fn from(value: ProjectEntry) -> Self {
        Self {
            name: value.name,
            path: value.path.display().to_string(),
            description: value.description,
            readonly: value.readonly,
        }
    }
}

fn copy_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.monospace(value);
        if ui.button("复制").clicked() {
            ui.ctx().copy_text(value.to_string());
        }
    });
}

fn default_entry(path: &Path) -> ProjectEntry {
    ProjectEntry {
        id: String::new(),
        name: "default".into(),
        path: path.to_path_buf(),
        description: "Default workspace".into(),
        readonly: false,
        executor: crate::projects::default_project_executor(),
    }
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        let ok = if i == 0 {
            c.is_ascii_alphanumeric()
        } else {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
        };
        if ok {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "project".into()
    } else {
        out.chars().take(64).collect()
    }
}

fn auto_launcher() -> Result<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().context("cannot locate agentbridge executable")?;
    let path = exe.to_string_lossy().to_string();
    let args = ["tray"];
    #[cfg(target_os = "macos")]
    let auto = auto_launch::AutoLaunch::new("AgentBridge", &path, false, &args);
    #[cfg(not(target_os = "macos"))]
    let auto = auto_launch::AutoLaunch::new("AgentBridge", &path, &args);
    Ok(auto)
}

fn tray_icon_image() -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let i = ((y * SIZE + x) * 4) as usize;
            if dx * dx + dy * dy <= 14.8 * 14.8 {
                rgba[i] = 0x3d;
                rgba[i + 1] = 0xd6;
                rgba[i + 2] = 0xc6;
                rgba[i + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("icon")
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = Color32::from_rgb(0x10, 0x16, 0x1d);
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x24, 0x2e, 0x3a);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2c, 0x3a, 0x48);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x2c, 0x38, 0x46));
    ctx.set_visuals(visuals);
}

fn apply_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert("cjk".into(), FontData::from_owned(bytes).into());
        if let Some(fam) = fonts.families.get_mut(&FontFamily::Proportional) {
            fam.insert(0, "cjk".into());
        }
        if let Some(fam) = fonts.families.get_mut(&FontFamily::Monospace) {
            fam.push("cjk".into());
        }
        ctx.set_fonts(fonts);
        return;
    }
}
