use crate::ai::{self, StreamEvent};
use crate::apps;
use crate::config;
use crate::hotkeys;
use crate::mcp;
use crate::router::{self, Mode};

use iced::futures::SinkExt;
use iced::widget::{button, column, container, scrollable, text, text_input};
use iced::{Element, Length, Subscription, Task, Theme};

const MAX_RESULTS: usize = 8;

pub struct Palette {
    query: String,
    apps: Vec<apps::AppEntry>,
    results: Vec<apps::AppEntry>,
    config: config::Config,
    window_id: Option<iced::window::Id>,
    mcp_slot: std::sync::Arc<tokio::sync::Mutex<Option<mcp::Registry>>>,
    mcp_tools: Vec<(String, mcp::ToolDef)>,
    ai_text: String,
    ai_streaming: bool,
    mcp_result: Option<String>,
    notice: String,
    hotkeys: Option<hotkeys::Hotkeys>,
}

#[derive(Clone)]
pub enum Message {
    QueryChanged(String),
    Submitted,
    EscapePressed,
    HotkeyPressed,
    WindowOpened(iced::window::Id),
    LaunchApp(String),
    AppsLoaded(Vec<apps::AppEntry>),
    McpReady {
        connected: usize,
        failed: usize,
        tools: Vec<(String, mcp::ToolDef)>,
    },
    McpCall(Result<String, String>),
    AiChunk(String),
    AiDone,
    AiFailed(String),
}

impl Palette {
    pub fn new() -> (Self, Task<Message>) {
        let config = config::load();
        let hotkeys = hotkeys::register()
            .map_err(|e| tracing::warn!("global hotkey registration failed: {e}"))
            .ok();

        let mcp_slot: std::sync::Arc<tokio::sync::Mutex<Option<mcp::Registry>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(None));

        let palette = Self {
            query: String::new(),
            apps: Vec::new(),
            results: Vec::new(),
            config: config.clone(),
            window_id: None,
            mcp_slot: mcp_slot.clone(),
            mcp_tools: Vec::new(),
            ai_text: String::new(),
            ai_streaming: false,
            mcp_result: None,
            notice: String::new(),
            hotkeys,
        };

        let apps_task = Task::perform(apps::enumerate(), Message::AppsLoaded);
        let mcp_task = {
            let mcp_cfg = config.mcp.clone();
            Task::perform(
                async move {
                    let registry = mcp::Registry::connect(&mcp_cfg).await;
                    let tools = registry
                        .tools()
                        .into_iter()
                        .map(|(s, t)| (s.to_string(), t.clone()))
                        .collect::<Vec<_>>();
                    let (connected, failed) = registry.stats();
                    *mcp_slot.lock().await = Some(registry);
                    Message::McpReady {
                        connected,
                        failed,
                        tools,
                    }
                },
                |m: Message| m,
            )
        };

        (palette, Task::batch([apps_task, mcp_task]))
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn refresh_results(&mut self) {
        if let Mode::Apps { query } = router::parse(&self.query) {
            self.results = apps::fuzzy_search(&self.apps, &query)
                .into_iter()
                .take(MAX_RESULTS)
                .collect();
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueryChanged(q) => {
                self.query = q;
                self.notice.clear();
                self.mcp_result = None;
                self.refresh_results();
                Task::none()
            }
            Message::Submitted => match router::parse(&self.query) {
                Mode::Apps { .. } => match self.results.first() {
                    Some(app) => {
                        let exec = app.exec.clone();
                        self.launch(&exec)
                    }
                    None => {
                        self.set_notice("no matching app");
                        Task::none()
                    }
                },
                Mode::Ask { prompt } => {
                    if prompt.is_empty() {
                        self.set_notice("type a prompt after ?");
                        return Task::none();
                    }
                    if self.ai_streaming {
                        return Task::none();
                    }
                    self.ai_text.clear();
                    self.ai_streaming = true;
                    let cfg = self.config.ai.clone();
                    let events = ai::stream_answer(cfg, prompt);
                    use iced::futures::StreamExt;
                    Task::stream(events.map(|ev| match ev {
                        StreamEvent::Chunk(c) => Message::AiChunk(c),
                        StreamEvent::Done => Message::AiDone,
                        StreamEvent::Failed(e) => Message::AiFailed(e),
                    }))
                }
                Mode::Tool { name, raw_args } => {
                    let args: serde_json::Value = if raw_args.trim().is_empty() {
                        serde_json::json!({})
                    } else {
                        match serde_json::from_str(&raw_args) {
                            Ok(v) => v,
                            Err(e) => {
                                self.set_notice(format!("invalid tool args JSON: {e}"));
                                return Task::none();
                            }
                        }
                    };
                    let Some((server, tool)) = self.find_tool(&name) else {
                        self.set_notice(format!("no tool named '{name}'"));
                        return Task::none();
                    };
                    let server = server.clone();
                    let tool = tool.name.clone();
                    let slot = self.mcp_slot.clone();
                    Task::perform(
                        async move {
                            let mut guard = slot.lock().await;
                            match guard.as_mut() {
                                Some(reg) => reg.call(&server, &tool, &args).await.map(|v| {
                                    serde_json::to_string_pretty(&v)
                                        .unwrap_or_else(|_| v.to_string())
                                }),
                                None => Err("no MCP servers connected".to_string()),
                            }
                        },
                        Message::McpCall,
                    )
                }
            },
            Message::LaunchApp(exec) => self.launch(&exec),
            Message::EscapePressed => {
                if !self.query.is_empty() || !self.notice.is_empty() || self.mcp_result.is_some() {
                    self.query.clear();
                    self.notice.clear();
                    self.mcp_result = None;
                    self.results.clear();
                    Task::none()
                } else {
                    iced::exit()
                }
            }
            Message::HotkeyPressed => {
                self.query.clear();
                self.notice.clear();
                self.mcp_result = None;
                self.ai_text.clear();
                self.results = self.apps.iter().take(MAX_RESULTS).cloned().collect();
                match self.window_id {
                    Some(id) => iced::window::gain_focus(id),
                    None => Task::none(),
                }
            }
            Message::WindowOpened(id) => {
                self.window_id = Some(id);
                Task::none()
            }
            Message::AppsLoaded(apps) => {
                self.apps = apps;
                self.refresh_results();
                tracing::info!("loaded {} apps", self.apps.len());
                Task::none()
            }
            Message::McpReady {
                connected,
                failed,
                tools,
            } => {
                self.notice = format!("{connected} MCP server(s) connected, {failed} failed");
                tracing::info!("{}", self.notice);
                self.mcp_tools = tools;
                Task::none()
            }
            Message::McpCall(Ok(result)) => {
                self.mcp_result = Some(result);
                Task::none()
            }
            Message::McpCall(Err(e)) => {
                self.set_notice(format!("tool call failed: {e}"));
                Task::none()
            }
            Message::AiChunk(chunk) => {
                self.ai_text.push_str(&chunk);
                Task::none()
            }
            Message::AiDone => {
                self.ai_streaming = false;
                Task::none()
            }
            Message::AiFailed(e) => {
                self.ai_streaming = false;
                self.set_notice(e);
                Task::none()
            }
        }
    }

    fn launch(&mut self, exec: &str) -> Task<Message> {
        match apps::launch(exec) {
            Ok(_) => tracing::info!("launched: {exec}"),
            Err(e) => self.set_notice(format!("failed to launch '{exec}': {e}")),
        }
        Task::none()
    }

    fn set_notice(&mut self, n: impl Into<String>) {
        self.notice = n.into();
    }

    fn find_tool(&self, name: &str) -> Option<&(String, mcp::ToolDef)> {
        self.mcp_tools.iter().find(|(_, t)| t.name == name)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let input = text_input(
            "Search apps \u{2022} ? <ai prompt> \u{2022} # <mcp tool>",
            &self.query,
        )
        .on_input(Message::QueryChanged)
        .on_submit(Message::Submitted)
        .padding(12)
        .size(18);

        let body: Element<'_, Message> = match router::parse(&self.query) {
            Mode::Apps { .. } => self.apps_list(),
            Mode::Ask { .. } => self.ai_view(),
            Mode::Tool { name, .. } => self.tools_view(&name),
        };

        column![
            container(input).padding(8),
            container(body).padding(8).height(Length::Fill),
            container(text(self.notice.as_str()).size(12)).padding(8),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn apps_list(&self) -> Element<'_, Message> {
        if self.results.is_empty() {
            return container(text("no apps").size(14)).padding(12).into();
        }
        let items: Vec<Element<'_, Message>> = self
            .results
            .iter()
            .map(|app| {
                let label = match &app.description {
                    Some(d) => format!("{} \u{2014} {}", app.name, d),
                    None => app.name.clone(),
                };
                button(text(label).size(14))
                    .on_press(Message::LaunchApp(app.exec.clone()))
                    .padding(8)
                    .width(Length::Fill)
                    .into()
            })
            .collect();
        scrollable(column(items).spacing(2)).into()
    }

    fn ai_view(&self) -> Element<'_, Message> {
        if self.ai_text.is_empty() {
            let hint = if self.ai_streaming {
                "thinking...".to_string()
            } else {
                "press Enter to ask".to_string()
            };
            return container(text(hint).size(14)).padding(12).into();
        }
        scrollable(container(text(self.ai_text.as_str()).size(14)).padding(12)).into()
    }

    fn tools_view(&self, name: &str) -> Element<'_, Message> {
        if let Some(result) = &self.mcp_result {
            return scrollable(container(text(result.as_str()).size(13)).padding(12)).into();
        }
        let matching: Vec<&(String, mcp::ToolDef)> = self
            .mcp_tools
            .iter()
            .filter(|(_, t)| name.is_empty() || t.name.contains(name))
            .collect();
        if matching.is_empty() {
            if self.mcp_tools.is_empty() {
                return container(text("no MCP servers configured").size(14))
                    .padding(12)
                    .into();
            }
            return container(text("no matching tools").size(14))
                .padding(12)
                .into();
        }
        let items: Vec<Element<'_, Message>> = matching
            .iter()
            .map(|(server, tool)| {
                let label = format!(
                    "{} \u{00b7} {} \u{2014} {}",
                    server, tool.name, tool.description
                );
                button(text(label).size(14))
                    .on_press(Message::QueryChanged(format!("#{} ", tool.name)))
                    .padding(8)
                    .width(Length::Fill)
                    .into()
            })
            .collect();
        scrollable(column(items).spacing(2)).into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();
        if let Some(hk) = &self.hotkeys {
            subs.push(Subscription::run_with(hk.toggle_id, hotkey_stream));
        }
        subs.push(iced::keyboard::listen().filter_map(escape_key));
        subs.push(iced::window::open_events().map(Message::WindowOpened));
        Subscription::batch(subs)
    }
}

fn escape_key(event: iced::keyboard::Event) -> Option<Message> {
    match event {
        iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        } => Some(Message::EscapePressed),
        _ => None,
    }
}

/// Poll the global-hotkey receiver and forward presses into the iced loop.
/// Builder form for `Subscription::run_with`: takes the hotkey id by reference.
fn hotkey_stream(toggle_id: &u32) -> impl iced::futures::Stream<Item = Message> {
    let toggle_id = *toggle_id;
    iced::stream::channel(8, async move |mut sender| {
        use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
        let receiver = GlobalHotKeyEvent::receiver();
        loop {
            if let Ok(event) = receiver.try_recv() {
                if event.state == HotKeyState::Pressed && event.id == toggle_id {
                    let _ = sender.send(Message::HotkeyPressed).await;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
}

pub fn run() -> iced::Result {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    iced::application(Palette::new, Palette::update, Palette::view)
        .title("barkeep")
        .subscription(Palette::subscription)
        .theme(Palette::theme)
        .window(iced::window::Settings {
            size: iced::Size::new(720.0, 480.0),
            decorations: false,
            resizable: false,
            level: iced::window::Level::AlwaysOnTop,
            ..Default::default()
        })
        .run()
}
