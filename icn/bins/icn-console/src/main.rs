//! ICN Console - Interactive TUI for cooperative management
//!
//! A terminal-based user interface for managing ICN cooperatives,
//! viewing members, ledger balances, governance proposals, and trust relationships.

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, TableState, Tabs},
    Frame, Terminal,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "icn-console")]
#[command(about = "Interactive TUI console for ICN cooperative management")]
struct Args {
    /// Gateway API endpoint
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    gateway: String,

    /// Cooperative ID to manage
    #[arg(short, long)]
    coop_id: Option<String>,

    /// JWT token for authentication
    #[arg(short, long)]
    token: Option<String>,
}

/// Available tabs in the console
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Members,
    Ledger,
    Governance,
    Trust,
}

impl Tab {
    fn all() -> Vec<Tab> {
        vec![
            Tab::Dashboard,
            Tab::Members,
            Tab::Ledger,
            Tab::Governance,
            Tab::Trust,
        ]
    }

    fn title(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Members => "Members",
            Tab::Ledger => "Ledger",
            Tab::Governance => "Governance",
            Tab::Trust => "Trust",
        }
    }
}

/// Member information
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Member {
    did: String,
    role: String,
    balance: f64,
    joined_at: Option<u64>,
}

/// Ledger transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Transaction {
    id: String,
    from: String,
    to: String,
    amount: f64,
    currency: String,
    memo: Option<String>,
    timestamp: u64,
}

/// Governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Proposal {
    id: String,
    title: String,
    state: String,
    votes_for: u32,
    votes_against: u32,
    votes_abstain: u32,
    closes_at: Option<u64>,
}

/// Trust edge
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustRelation {
    target: String,
    score: f64,
    labels: Vec<String>,
}

/// Application state
struct App {
    /// Current active tab
    current_tab: usize,
    /// Gateway API client
    client: Client,
    /// Gateway URL
    gateway_url: String,
    /// JWT token
    token: Option<String>,
    /// Cooperative ID
    coop_id: String,
    /// Status message
    status: String,
    /// Error message
    error: Option<String>,
    /// Whether app should quit
    should_quit: bool,

    // Data
    members: Vec<Member>,
    members_state: TableState,
    transactions: Vec<Transaction>,
    transactions_state: TableState,
    proposals: Vec<Proposal>,
    proposals_state: TableState,
    trust_relations: Vec<TrustRelation>,
    trust_state: TableState,

    // Dashboard stats
    total_members: usize,
    total_balance: f64,
    active_proposals: usize,
    network_peers: usize,

    // Input mode for forms
    input_mode: InputMode,
    input_buffer: String,
}

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Input,
}

impl App {
    fn new(gateway_url: String, coop_id: String, token: Option<String>) -> Self {
        Self {
            current_tab: 0,
            client: Client::new(),
            gateway_url,
            token,
            coop_id,
            status: "Press 'r' to refresh, 'q' to quit".to_string(),
            error: None,
            should_quit: false,

            members: Vec::new(),
            members_state: TableState::default(),
            transactions: Vec::new(),
            transactions_state: TableState::default(),
            proposals: Vec::new(),
            proposals_state: TableState::default(),
            trust_relations: Vec::new(),
            trust_state: TableState::default(),

            total_members: 0,
            total_balance: 0.0,
            active_proposals: 0,
            network_peers: 0,

            input_mode: InputMode::Normal,
            input_buffer: String::new(),
        }
    }

    fn next_tab(&mut self) {
        self.current_tab = (self.current_tab + 1) % Tab::all().len();
    }

    fn prev_tab(&mut self) {
        if self.current_tab > 0 {
            self.current_tab -= 1;
        } else {
            self.current_tab = Tab::all().len() - 1;
        }
    }

    fn current_tab(&self) -> Tab {
        Tab::all()[self.current_tab]
    }

    fn next_item(&mut self) {
        match self.current_tab() {
            Tab::Members => {
                let i = match self.members_state.selected() {
                    Some(i) => {
                        if i >= self.members.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.members_state.select(Some(i));
            }
            Tab::Ledger => {
                let i = match self.transactions_state.selected() {
                    Some(i) => {
                        if i >= self.transactions.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.transactions_state.select(Some(i));
            }
            Tab::Governance => {
                let i = match self.proposals_state.selected() {
                    Some(i) => {
                        if i >= self.proposals.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.proposals_state.select(Some(i));
            }
            Tab::Trust => {
                let i = match self.trust_state.selected() {
                    Some(i) => {
                        if i >= self.trust_relations.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.trust_state.select(Some(i));
            }
            _ => {}
        }
    }

    fn prev_item(&mut self) {
        match self.current_tab() {
            Tab::Members => {
                let i = match self.members_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.members.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.members_state.select(Some(i));
            }
            Tab::Ledger => {
                let i = match self.transactions_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.transactions.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.transactions_state.select(Some(i));
            }
            Tab::Governance => {
                let i = match self.proposals_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.proposals.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.proposals_state.select(Some(i));
            }
            Tab::Trust => {
                let i = match self.trust_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.trust_relations.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.trust_state.select(Some(i));
            }
            _ => {}
        }
    }

    /// Refresh data from gateway API
    async fn refresh(&mut self) {
        self.status = "Refreshing...".to_string();
        self.error = None;

        // Try to fetch health/status first
        match self.fetch_health().await {
            Ok(health) => {
                self.network_peers = health.network_peers;
                self.status = format!("Connected to {} | {} peers", self.gateway_url, health.network_peers);
            }
            Err(e) => {
                self.error = Some(format!("Failed to connect: {e}"));
                self.status = "Disconnected".to_string();
                return;
            }
        }

        // Fetch members
        if let Ok(members) = self.fetch_members().await {
            self.total_members = members.len();
            self.total_balance = members.iter().map(|m| m.balance).sum();
            self.members = members;
        }

        // Fetch transactions
        if let Ok(txs) = self.fetch_transactions().await {
            self.transactions = txs;
        }

        // Fetch proposals
        if let Ok(proposals) = self.fetch_proposals().await {
            self.active_proposals = proposals.iter().filter(|p| p.state == "open").count();
            self.proposals = proposals;
        }

        // Fetch trust relations
        if let Ok(trust) = self.fetch_trust().await {
            self.trust_relations = trust;
        }
    }

    async fn fetch_health(&self) -> Result<HealthResponse> {
        let url = format!("{}/v1/health", self.gateway_url);
        let resp = self.client.get(&url).send().await?;
        let health: HealthResponse = resp.json().await?;
        Ok(health)
    }

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {t}"))
    }

    async fn fetch_members(&self) -> Result<Vec<Member>> {
        let url = format!("{}/v1/coops/{}/members", self.gateway_url, self.coop_id);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let api_members: Vec<ApiMember> = resp.json().await?;

        // Fetch balance for each member
        let mut members = Vec::new();
        for api_member in api_members {
            let balance = self.fetch_balance(&api_member.did).await.unwrap_or(0.0);
            members.push(Member {
                did: api_member.did,
                role: api_member.role,
                balance,
                joined_at: None,
            });
        }

        Ok(members)
    }

    async fn fetch_balance(&self, did: &str) -> Result<f64> {
        let url = format!(
            "{}/v1/ledger/{}/balance/{}",
            self.gateway_url,
            self.coop_id,
            urlencoding::encode(did)
        );
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(0.0);
        }

        let balance: ApiBalance = resp.json().await?;
        Ok(balance.balance)
    }

    async fn fetch_transactions(&self) -> Result<Vec<Transaction>> {
        let url = format!(
            "{}/v1/ledger/{}/history?limit=50",
            self.gateway_url, self.coop_id
        );
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let history: ApiHistory = resp.json().await?;
        let transactions = history
            .transactions
            .into_iter()
            .map(|tx| Transaction {
                id: tx.id,
                from: tx.from,
                to: tx.to,
                amount: tx.amount,
                currency: tx.currency,
                memo: tx.memo,
                timestamp: tx.timestamp,
            })
            .collect();

        Ok(transactions)
    }

    async fn fetch_proposals(&self) -> Result<Vec<Proposal>> {
        let url = format!("{}/v1/gov/proposals", self.gateway_url);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let api_proposals: Vec<ApiProposal> = resp.json().await?;

        // Fetch vote tally for each proposal
        let mut proposals = Vec::new();
        for api_prop in api_proposals {
            let tally = self.fetch_votes(&api_prop.id).await.unwrap_or_default();
            proposals.push(Proposal {
                id: api_prop.id,
                title: api_prop.title,
                state: api_prop.state,
                votes_for: tally.for_votes,
                votes_against: tally.against_votes,
                votes_abstain: tally.abstain_votes,
                closes_at: None,
            });
        }

        Ok(proposals)
    }

    async fn fetch_votes(&self, proposal_id: &str) -> Result<ApiVoteTally> {
        let url = format!("{}/v1/gov/proposals/{}/votes", self.gateway_url, proposal_id);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(ApiVoteTally::default());
        }

        let tally: ApiVoteTally = resp.json().await?;
        Ok(tally)
    }

    async fn fetch_trust(&self) -> Result<Vec<TrustRelation>> {
        // Trust API endpoint not yet implemented in gateway
        // Return empty for now
        Ok(Vec::new())
    }
}


#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HealthResponse {
    status: String,
    #[serde(default)]
    network_peers: usize,
    #[serde(default)]
    gossip_entries: usize,
    #[serde(default)]
    ledger_entries: usize,
}

#[derive(Debug, Deserialize)]
struct ApiMember {
    did: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct ApiBalance {
    balance: f64,
}

#[derive(Debug, Deserialize)]
struct ApiTransaction {
    id: String,
    from: String,
    to: String,
    amount: f64,
    currency: String,
    memo: Option<String>,
    timestamp: u64,
}

#[derive(Debug, Deserialize)]
struct ApiHistory {
    transactions: Vec<ApiTransaction>,
}

#[derive(Debug, Deserialize)]
struct ApiProposal {
    id: String,
    title: String,
    state: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[derive(Default)]
struct ApiVoteTally {
    #[serde(default)]
    for_votes: u32,
    #[serde(default)]
    against_votes: u32,
    #[serde(default)]
    abstain_votes: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let coop_id = args.coop_id.unwrap_or_else(|| "default".to_string());

    // Create app
    let mut app = App::new(args.gateway, coop_id, args.token);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;

    // Initial data fetch
    rt.block_on(app.refresh());

    // Main loop
    let res = run_app(&mut terminal, &mut app, &rt);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rt: &tokio::runtime::Runtime,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Char('r') => rt.block_on(app.refresh()),
                            KeyCode::Tab | KeyCode::Right => app.next_tab(),
                            KeyCode::BackTab | KeyCode::Left => app.prev_tab(),
                            KeyCode::Down | KeyCode::Char('j') => app.next_item(),
                            KeyCode::Up | KeyCode::Char('k') => app.prev_item(),
                            KeyCode::Char('1') => app.current_tab = 0,
                            KeyCode::Char('2') => app.current_tab = 1,
                            KeyCode::Char('3') => app.current_tab = 2,
                            KeyCode::Char('4') => app.current_tab = 3,
                            KeyCode::Char('5') => app.current_tab = 4,
                            KeyCode::Enter => {
                                // Action on selected item
                                if app.current_tab() == Tab::Governance {
                                    // Could open voting dialog
                                }
                            }
                            _ => {}
                        },
                        InputMode::Input => match key.code {
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                                app.input_buffer.clear();
                            }
                            KeyCode::Enter => {
                                // Process input
                                app.input_mode = InputMode::Normal;
                                app.input_buffer.clear();
                            }
                            KeyCode::Char(c) => {
                                app.input_buffer.push(c);
                            }
                            KeyCode::Backspace => {
                                app.input_buffer.pop();
                            }
                            _ => {}
                        },
                    }
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Status bar
        ])
        .split(size);

    // Render tabs
    let tab_titles: Vec<Line> = Tab::all()
        .iter()
        .map(|t| Line::from(Span::styled(t.title(), Style::default().fg(Color::White))))
        .collect();

    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).title(" ICN Console "))
        .select(app.current_tab)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // Render content based on current tab
    match app.current_tab() {
        Tab::Dashboard => render_dashboard(f, app, chunks[1]),
        Tab::Members => render_members(f, app, chunks[1]),
        Tab::Ledger => render_ledger(f, app, chunks[1]),
        Tab::Governance => render_governance(f, app, chunks[1]),
        Tab::Trust => render_trust(f, app, chunks[1]),
    }

    // Render status bar
    let status_style = if app.error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Green)
    };

    let status_text = if let Some(ref err) = app.error {
        err.clone()
    } else {
        app.status.clone()
    };

    let status = Paragraph::new(status_text)
        .style(status_style)
        .block(Block::default().borders(Borders::ALL).title(" Status "));
    f.render_widget(status, chunks[2]);
}

fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // Stats
            Constraint::Min(0),    // Recent activity
        ])
        .split(area);

    // Stats section
    let stats_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(chunks[0]);

    // Members stat
    let members_stat = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{}", app.total_members),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("members"),
    ])
    .alignment(ratatui::layout::Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Members "));
    f.render_widget(members_stat, stats_chunks[0]);

    // Balance stat
    let balance_color = if app.total_balance >= 0.0 {
        Color::Green
    } else {
        Color::Red
    };
    let balance_stat = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{:.1}", app.total_balance),
            Style::default()
                .fg(balance_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("total hours"),
    ])
    .alignment(ratatui::layout::Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Balance "));
    f.render_widget(balance_stat, stats_chunks[1]);

    // Proposals stat
    let proposals_stat = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{}", app.active_proposals),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("active votes"),
    ])
    .alignment(ratatui::layout::Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Proposals "));
    f.render_widget(proposals_stat, stats_chunks[2]);

    // Network stat
    let network_stat = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{}", app.network_peers),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("network peers"),
    ])
    .alignment(ratatui::layout::Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Network "));
    f.render_widget(network_stat, stats_chunks[3]);

    // Recent activity
    let recent_items: Vec<ListItem> = app
        .transactions
        .iter()
        .take(5)
        .map(|tx| {
            let content = format!(
                "{} -> {} : {} {}",
                truncate_did(&tx.from),
                truncate_did(&tx.to),
                tx.amount,
                tx.currency
            );
            ListItem::new(content)
        })
        .collect();

    let recent_list = List::new(recent_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Recent Activity "),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(recent_list, chunks[1]);
}

fn render_members(f: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = app
        .members
        .iter()
        .map(|m| {
            let balance_style = if m.balance >= 0.0 {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };

            Row::new(vec![
                Cell::from(truncate_did(&m.did)),
                Cell::from(m.role.clone()),
                Cell::from(format!("{:.1}", m.balance)).style(balance_style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
    )
    .header(
        Row::new(vec!["DID", "Role", "Balance"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Members ({}) ", app.members.len())),
    )
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(table, area, &mut app.members_state.clone());
}

fn render_ledger(f: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = app
        .transactions
        .iter()
        .map(|tx| {
            Row::new(vec![
                Cell::from(truncate_did(&tx.from)),
                Cell::from(truncate_did(&tx.to)),
                Cell::from(format!("{} {}", tx.amount, tx.currency)),
                Cell::from(tx.memo.clone().unwrap_or_default()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
        ],
    )
    .header(
        Row::new(vec!["From", "To", "Amount", "Memo"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Transactions ({}) ", app.transactions.len())),
    )
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(table, area, &mut app.transactions_state.clone());
}

fn render_governance(f: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = app
        .proposals
        .iter()
        .map(|p| {
            let state_style = match p.state.as_str() {
                "open" => Style::default().fg(Color::Yellow),
                "closed" => Style::default().fg(Color::Gray),
                _ => Style::default(),
            };

            let votes = format!(
                "{}/{}/{}",
                p.votes_for, p.votes_against, p.votes_abstain
            );

            Row::new(vec![
                Cell::from(p.id.clone()),
                Cell::from(p.title.clone()),
                Cell::from(p.state.clone()).style(state_style),
                Cell::from(votes),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(15),
            Constraint::Percentage(45),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
    )
    .header(
        Row::new(vec!["ID", "Title", "State", "Votes (F/A/Ab)"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Proposals ({}) - Press Enter to vote ", app.proposals.len())),
    )
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(table, area, &mut app.proposals_state.clone());
}

fn render_trust(f: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = app
        .trust_relations
        .iter()
        .map(|t| {
            let score_color = if t.score >= 0.7 {
                Color::Green
            } else if t.score >= 0.4 {
                Color::Yellow
            } else {
                Color::Red
            };

            Row::new(vec![
                Cell::from(truncate_did(&t.target)),
                Cell::from(format!("{:.2}", t.score)).style(Style::default().fg(score_color)),
                Cell::from(t.labels.join(", ")),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ],
    )
    .header(
        Row::new(vec!["Target", "Score", "Labels"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Trust Relations ({}) ", app.trust_relations.len())),
    )
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(table, area, &mut app.trust_state.clone());
}

/// Truncate a DID for display
fn truncate_did(did: &str) -> String {
    if did.len() > 20 {
        format!("{}...{}", &did[..10], &did[did.len() - 6..])
    } else {
        did.to_string()
    }
}
