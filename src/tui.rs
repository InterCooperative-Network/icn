use crate::api::ApiClient;
use crate::federation::{FederationRuntime, MonitoringOptions};
use crate::identity::{Identity, IdentityManager};
use crate::storage::StorageManager;
use crate::sync::{Notification, SyncManager, SyncConfig};
use crate::websocket::{WebSocketServer, WebSocketConfig};
use crate::dashboard::ComputeDashboard;
use crate::ui::InputEvent;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};

/// Application state
struct App {
    identity_manager: Arc<Mutex<IdentityManager>>,
    federation_runtime: Arc<Mutex<FederationRuntime>>,
    storage_manager: StorageManager,
    sync_manager: Option<SyncManager>,
    websocket_server: Option<WebSocketServer>,
    
    // UI state
    tab_index: usize,
    tab_titles: Vec<&'static str>,
    identity_list_state: ListState,
    proposal_list_state: ListState,
    notification_list_state: ListState,
    
    // Data
    identities: Vec<String>,
    proposals: Vec<String>,
    notifications: Vec<Notification>,
    
    // Input state
    input: String,
    input_mode: InputMode,
    command_history: Vec<String>,
    command_index: usize,
    status_message: String,
    
    // Dashboards
    compute_dashboard: ComputeDashboard,
}

/// Input mode for the application
enum InputMode {
    Normal,
    Editing,
    Command,
}

impl App {
    /// Create a new application
    fn new(
        identity_manager: IdentityManager,
        federation_runtime: FederationRuntime,
        storage_manager: StorageManager,
    ) -> Self {
        // Create tab UI state
        let mut identity_list_state = ListState::default();
        identity_list_state.select(Some(0));
        
        let mut proposal_list_state = ListState::default();
        proposal_list_state.select(Some(0));
        
        let mut notification_list_state = ListState::default();
        notification_list_state.select(Some(0));
        
        // Create compute dashboard
        let compute_dashboard = ComputeDashboard::new(storage_manager.get_wallet_db());
        
        // Create application state
        Self {
            identity_manager: Arc::new(Mutex::new(identity_manager)),
            federation_runtime: Arc::new(Mutex::new(federation_runtime)),
            storage_manager,
            sync_manager: None,
            websocket_server: None,
            tab_index: 0,
            tab_titles: vec!["Identities", "Proposals", "Compute", "Notifications", "Console"],
            identity_list_state,
            proposal_list_state,
            notification_list_state,
            identities: Vec::new(),
            proposals: Vec::new(),
            notifications: Vec::new(),
            input: String::new(),
            input_mode: InputMode::Normal,
            command_history: Vec::new(),
            command_index: 0,
            status_message: "Welcome to ICN Wallet".to_string(),
            compute_dashboard,
        }
    }
    
    /// Start the sync manager
    fn start_sync_manager(&mut self) {
        // Create sync config
        let config = SyncConfig {
            inbox_sync_interval: 10, // 10 seconds for testing
            outbox_sync_interval: 10, // 10 seconds for testing
            dag_watch_interval: 5,    // 5 seconds for testing
            inbox_path: std::path::PathBuf::from("proposals/inbox"),
            outbox_path: std::path::PathBuf::from("proposals/outbox"),
        };
        
        // Create sync manager
        let federation_runtime = {
            let lock = self.federation_runtime.lock().unwrap();
            // Use .to_owned() instead of .clone() to avoid requiring the Clone trait
            FederationRuntime::new(
                lock.get_api_config().clone(),
                lock.get_identity().clone(),
                self.storage_manager.clone(),
            ).unwrap()
        };
        
        let sync_manager = SyncManager::new(
            federation_runtime,
            self.storage_manager.clone(),
            self.identity_manager.lock().unwrap().clone(),
            Some(config),
        );
        
        // Start background sync
        if let Err(e) = sync_manager.start() {
            self.status_message = format!("Failed to start sync manager: {}", e);
        } else {
            self.status_message = "Sync manager started".to_string();
        }
        
        self.sync_manager = Some(sync_manager);
        
        // Start WebSocket server
        self.start_websocket_server();
    }
    
    /// Start the WebSocket server
    fn start_websocket_server(&mut self) {
        if self.sync_manager.is_none() {
            self.status_message = "Cannot start WebSocket server without sync manager".to_string();
            return;
        }
        
        // Create websocket config
        let config = WebSocketConfig {
            host: "127.0.0.1".to_string(),
            port: 9876,
            ping_interval: 30,
        };
        
        // Get a clone of the sync manager to avoid ownership issues
        let sync_manager = self.sync_manager.as_ref().unwrap().clone();
        
        // Create WebSocket server
        let websocket_server = WebSocketServer::new(
            sync_manager,
            Some(config),
        );
        
        // Start WebSocket server
        if let Err(e) = websocket_server.start() {
            self.status_message = format!("Failed to start WebSocket server: {}", e);
        } else {
            self.status_message = "WebSocket server started on 127.0.0.1:9876".to_string();
        }
        
        self.websocket_server = Some(websocket_server);
    }
    
    fn update(&mut self) {
        // Process notifications from sync manager
        if let Some(sync_manager) = &self.sync_manager {
            if let Some(notification) = sync_manager.next_notification() {
                self.notifications.push(notification);
                // Keep a fixed number of notifications
                if self.notifications.len() > 100 {
                    self.notifications.remove(0);
                }
            }
        }
        
        // Update identity list
        self.identities = {
            let id_manager = self.identity_manager.lock().unwrap();
            id_manager.list_identities()
                .iter()
                .map(|id| format!("{} ({})", id.username(), id.did()))
                .collect()
        };
        
        // Update proposals list
        // In a real implementation, this would query the federation for active proposals
        self.proposals = Vec::new();
    }
    
    fn next_tab(&mut self) {
        self.tab_index = (self.tab_index + 1) % self.tab_titles.len();
    }
    
    fn previous_tab(&mut self) {
        self.tab_index = if self.tab_index > 0 {
            self.tab_index - 1
        } else {
            self.tab_titles.len() - 1
        };
    }
    
    fn next_item(&mut self) {
        match self.tab_index {
            0 => {
                // Identities tab
                if !self.identities.is_empty() {
                    let i = match self.identity_list_state.selected() {
                        Some(i) => {
                            if i >= self.identities.len() - 1 {
                                0
                            } else {
                                i + 1
                            }
                        }
                        None => 0,
                    };
                    self.identity_list_state.select(Some(i));
                }
            }
            1 => {
                // Proposals tab
                if !self.proposals.is_empty() {
                    let i = match self.proposal_list_state.selected() {
                        Some(i) => {
                            if i >= self.proposals.len() - 1 {
                                0
                            } else {
                                i + 1
                            }
                        }
                        None => 0,
                    };
                    self.proposal_list_state.select(Some(i));
                }
            }
            2 => {
                // Compute tab
                // This tab is handled by the compute_dashboard
            }
            3 => {
                // Notifications tab
                if !self.notifications.is_empty() {
                    let i = match self.notification_list_state.selected() {
                        Some(i) => {
                            if i >= self.notifications.len() - 1 {
                                0
                            } else {
                                i + 1
                            }
                        }
                        None => 0,
                    };
                    self.notification_list_state.select(Some(i));
                }
            }
            _ => {}
        }
    }
    
    fn previous_item(&mut self) {
        match self.tab_index {
            0 => {
                // Identities tab
                if !self.identities.is_empty() {
                    let i = match self.identity_list_state.selected() {
                        Some(i) => {
                            if i == 0 {
                                self.identities.len() - 1
                            } else {
                                i - 1
                            }
                        }
                        None => 0,
                    };
                    self.identity_list_state.select(Some(i));
                }
            }
            1 => {
                // Proposals tab
                if !self.proposals.is_empty() {
                    let i = match self.proposal_list_state.selected() {
                        Some(i) => {
                            if i == 0 {
                                self.proposals.len() - 1
                            } else {
                                i - 1
                            }
                        }
                        None => 0,
                    };
                    self.proposal_list_state.select(Some(i));
                }
            }
            2 => {
                // Compute tab
                // This tab is handled by the compute_dashboard
            }
            3 => {
                // Notifications tab
                if !self.notifications.is_empty() {
                    let i = match self.notification_list_state.selected() {
                        Some(i) => {
                            if i == 0 {
                                self.notifications.len() - 1
                            } else {
                                i - 1
                            }
                        }
                        None => 0,
                    };
                    self.notification_list_state.select(Some(i));
                }
            }
            _ => {}
        }
    }
    
    fn enter_command_mode(&mut self) {
        self.input_mode = InputMode::Command;
        self.input = "".to_string();
    }
    
    fn execute_command(&mut self) {
        let command = self.input.clone();
        if !command.is_empty() {
            // Add command to history
            self.command_history.push(command.clone());
            self.command_index = self.command_history.len();
            
            // Parse and execute command
            let parts: Vec<&str> = command.split_whitespace().collect();
            if !parts.is_empty() {
                match parts[0] {
                    "quit" | "exit" => {
                        // Exit will be handled by the caller
                    },
                    "help" => {
                        self.status_message = "Available commands: help, quit, exit, id, proposal, dag".to_string();
                    },
                    "id" | "identity" => {
                        if parts.len() > 1 {
                            match parts[1] {
                                "list" => {
                                    // Already showing in the UI
                                    self.status_message = "Identities are listed in the Identities tab".to_string();
                                },
                                "use" => {
                                    if parts.len() > 2 {
                                        // Set active identity
                                        let mut id_manager = self.identity_manager.lock().unwrap();
                                        match id_manager.set_active_identity(parts[2]) {
                                            Ok(_) => {
                                                self.status_message = format!("Active identity set to {}", parts[2]);
                                            },
                                            Err(e) => {
                                                self.status_message = format!("Error setting active identity: {}", e);
                                            }
                                        }
                                    } else {
                                        self.status_message = "Usage: id use <did>".to_string();
                                    }
                                },
                                _ => {
                                    self.status_message = "Unknown identity command. Try 'id list' or 'id use <did>'".to_string();
                                }
                            }
                        } else {
                            self.status_message = "Usage: id [list|use]".to_string();
                        }
                    },
                    "proposal" => {
                        if parts.len() > 1 {
                            match parts[1] {
                                "list" => {
                                    // Already showing in the UI
                                    self.status_message = "Proposals are listed in the Proposals tab".to_string();
                                },
                                "show" => {
                                    if parts.len() > 2 {
                                        self.status_message = format!("Showing proposal {}", parts[2]);
                                        // In a real implementation, this would fetch and show the proposal
                                    } else {
                                        self.status_message = "Usage: proposal show <hash>".to_string();
                                    }
                                },
                                _ => {
                                    self.status_message = "Unknown proposal command. Try 'proposal list' or 'proposal show <hash>'".to_string();
                                }
                            }
                        } else {
                            self.status_message = "Usage: proposal [list|show]".to_string();
                        }
                    },
                    "dag" => {
                        if parts.len() > 1 {
                            match parts[1] {
                                "status" => {
                                    let federation = self.federation_runtime.lock().unwrap();
                                    match federation.get_dag_status(None) {
                                        Ok(status) => {
                                            self.status_message = format!(
                                                "DAG Status: Latest vertex: {}, Vertex count: {}, Synced: {}",
                                                status.latest_vertex, status.vertex_count, status.synced
                                            );
                                        },
                                        Err(e) => {
                                            self.status_message = format!("Error getting DAG status: {}", e);
                                        }
                                    }
                                },
                                _ => {
                                    self.status_message = "Unknown DAG command. Try 'dag status'".to_string();
                                }
                            }
                        } else {
                            self.status_message = "Usage: dag [status]".to_string();
                        }
                    },
                    _ => {
                        self.status_message = format!("Unknown command: {}", parts[0]);
                    }
                }
            }
        }
        
        // Reset input
        self.input = "".to_string();
        self.input_mode = InputMode::Normal;
    }
    
    fn previous_command(&mut self) {
        if !self.command_history.is_empty() {
            if self.command_index > 0 {
                self.command_index -= 1;
            }
            if self.command_index < self.command_history.len() {
                self.input = self.command_history[self.command_index].clone();
            }
        }
    }
    
    fn next_command(&mut self) {
        if !self.command_history.is_empty() {
            if self.command_index < self.command_history.len() {
                self.command_index += 1;
            }
            if self.command_index < self.command_history.len() {
                self.input = self.command_history[self.command_index].clone();
            } else {
                self.input = "".to_string();
            }
        }
    }
}

/// Run the TUI application
pub fn run_tui(
    identity_manager: &IdentityManager,
    api_client: &ApiClient,
    storage_manager: &StorageManager,
) -> io::Result<()> {
    // Create FederationRuntime
    let api_config = api_client.get_config().clone();
    let federation_runtime = FederationRuntime::new(
        api_config,
        identity_manager.get_active_identity()
            .cloned()
            .unwrap_or_else(|| {
                // Create a dummy identity if none is active
                Identity::new("default", "user", crate::identity::KeyType::Ed25519)
                    .expect("Failed to create dummy identity")
            }),
        storage_manager.clone(),
    ).expect("Failed to create federation runtime");
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Create app
    let mut app = App::new(
        identity_manager.clone(),
        federation_runtime,
        storage_manager.clone(),
    );
    
    // Start sync manager
    app.start_sync_manager();
    
    // Main loop
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = std::time::Instant::now();
    
    loop {
        // Draw UI
        terminal.draw(|f| ui(f, &mut app))?;
        
        // Handle input
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
            
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Check if we're on the compute tab
                if app.tab_index == 2 && app.input_mode == InputMode::Normal {
                    // Convert crossterm key to our InputEvent
                    let input_event = match key.code {
                        KeyCode::Char('q') => {
                            // Special case: q should still exit the app
                            break;
                        },
                        KeyCode::Char('t') => {
                            app.next_tab();
                            continue;
                        },
                        KeyCode::Tab => {
                            app.next_tab();
                            continue;
                        },
                        KeyCode::BackTab => {
                            app.previous_tab();
                            continue;
                        },
                        KeyCode::Char('c') => {
                            app.enter_command_mode();
                            continue;
                        },
                        KeyCode::Char(c) => InputEvent::KeyChar(c),
                        KeyCode::Up => InputEvent::KeyUp,
                        KeyCode::Down => InputEvent::KeyDown,
                        KeyCode::Left => InputEvent::KeyLeft,
                        KeyCode::Right => InputEvent::KeyRight,
                        KeyCode::Enter => InputEvent::KeyEnter,
                        KeyCode::Esc => InputEvent::KeyEsc,
                        KeyCode::Tab => InputEvent::KeyTab,
                        KeyCode::BackTab => InputEvent::KeyBacktab,
                        _ => continue,
                    };
                    
                    // Pass the event to the dashboard
                    app.compute_dashboard.handle_input(input_event);
                    continue;
                }
                
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') => app.enter_command_mode(),
                        KeyCode::Char('t') => app.next_tab(),
                        KeyCode::Char('T') => app.previous_tab(),
                        KeyCode::Down => app.next_item(),
                        KeyCode::Up => app.previous_item(),
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.previous_tab(),
                        _ => {}
                    },
                    InputMode::Command => match key.code {
                        KeyCode::Enter => app.execute_command(),
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input.clear();
                        }
                        KeyCode::Up => app.previous_command(),
                        KeyCode::Down => app.next_command(),
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input.clear();
                        }
                        _ => {}
                    },
                }
            }
        }
        
        // Tick
        if last_tick.elapsed() >= tick_rate {
            app.update();
            last_tick = std::time::Instant::now();
        }
    }
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    // Stop sync manager and WebSocket server
    if let Some(sync_manager) = &app.sync_manager {
        sync_manager.stop();
    }
    
    if let Some(websocket_server) = &app.websocket_server {
        websocket_server.stop();
    }
    
    Ok(())
}

/// Draw the UI
fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    // Create layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());
    
    // Draw tabs
    let titles = app
        .tab_titles
        .iter()
        .map(|t| Spans::from(Span::styled(*t, Style::default().fg(Color::Green))))
        .collect();
    
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Tabs"))
        .highlight_style(Style::default().fg(Color::Yellow))
        .select(app.tab_index);
    
    f.render_widget(tabs, chunks[0]);
    
    // Draw content
    match app.tab_index {
        0 => draw_identities_tab(f, app, chunks[1]),
        1 => draw_proposals_tab(f, app, chunks[1]),
        2 => draw_compute_tab(f, app, chunks[1]),
        3 => draw_notifications_tab(f, app, chunks[1]),
        4 => draw_console_tab(f, app, chunks[1]),
        _ => {}
    }
    
    // Draw status bar
    let status = match app.input_mode {
        InputMode::Normal => format!("Status: {}", app.status_message),
        InputMode::Command => format!("Command: {}", app.input),
        InputMode::Editing => format!("Editing: {}", app.input),
    };
    
    let status_bar = Paragraph::new(status)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    
    f.render_widget(status_bar, chunks[2]);
}

/// Draw the identities tab
fn draw_identities_tab<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let identities: Vec<ListItem> = app
        .identities
        .iter()
        .map(|id| ListItem::new(vec![Spans::from(Span::raw(id))]))
        .collect();
    
    let identities_list = List::new(identities)
        .block(Block::default().borders(Borders::ALL).title("Identities"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    
    f.render_stateful_widget(identities_list, area, &mut app.identity_list_state);
}

/// Draw the proposals tab
fn draw_proposals_tab<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let proposals: Vec<ListItem> = app
        .proposals
        .iter()
        .map(|p| ListItem::new(vec![Spans::from(Span::raw(p))]))
        .collect();
    
    let proposals_list = if proposals.is_empty() {
        List::new(vec![ListItem::new(vec![Spans::from(Span::raw(
            "No proposals found",
        ))])])
        .block(Block::default().borders(Borders::ALL).title("Proposals"))
    } else {
        List::new(proposals)
            .block(Block::default().borders(Borders::ALL).title("Proposals"))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
    };
    
    f.render_stateful_widget(proposals_list, area, &mut app.proposal_list_state);
}

/// Draw the compute tab
fn draw_compute_tab<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    // Just delegate the rendering to the compute dashboard component
    app.compute_dashboard.render(f, area);
}

/// Draw the notifications tab
fn draw_notifications_tab<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let notifications: Vec<ListItem> = app
        .notifications
        .iter()
        .map(|n| {
            let content = format!("[{}] {}", n.timestamp, n.message);
            ListItem::new(vec![Spans::from(Span::raw(content))])
        })
        .collect();
    
    let notifications_list = if notifications.is_empty() {
        List::new(vec![ListItem::new(vec![Spans::from(Span::raw(
            "No notifications",
        ))])])
        .block(Block::default().borders(Borders::ALL).title("Notifications"))
    } else {
        List::new(notifications)
            .block(Block::default().borders(Borders::ALL).title("Notifications"))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
    };
    
    f.render_stateful_widget(notifications_list, area, &mut app.notification_list_state);
}

/// Draw the console tab
fn draw_console_tab<B: Backend>(f: &mut Frame<B>, app: &mut App, area: Rect) {
    let text = vec![
        Spans::from(Span::raw("ICN Wallet Console")),
        Spans::from(Span::raw("")),
        Spans::from(Span::raw("Press 'c' to enter a command")),
        Spans::from(Span::raw("Available commands:")),
        Spans::from(Span::raw("  help - Show help")),
        Spans::from(Span::raw("  quit, exit - Exit the application")),
        Spans::from(Span::raw("  id list - List identities")),
        Spans::from(Span::raw("  id use <did> - Set active identity")),
        Spans::from(Span::raw("  proposal list - List proposals")),
        Spans::from(Span::raw("  proposal show <hash> - Show proposal details")),
        Spans::from(Span::raw("  dag status - Show DAG status")),
    ];
    
    let console = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Console"))
        .style(Style::default().fg(Color::White))
        .wrap(ratatui::widgets::Wrap { trim: true });
    
    f.render_widget(console, area);
} 