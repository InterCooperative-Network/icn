use crate::db::WalletDb;
use crate::resources::{ResourceToken, TokenBurn};
use crate::ui::{CentralPanel, Component, InputEvent, RenderFrame};
use crate::utils::format_timestamp;
use chrono::{DateTime, Duration, Local, NaiveDateTime, Utc};
use itertools::Itertools;
use std::collections::HashMap;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs};

/// View for compute resource tokens including balance and burn history
pub struct ComputeTokenView {
    db: WalletDb,
    tab_index: usize,
    compute_tokens: Vec<ResourceToken>,
    burn_history: Vec<TokenBurn>,
    // Filters
    federation_filter: Option<String>,
    timeframe_filter: TimeFrame,
    sort_by: SortOption,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TimeFrame {
    All,
    LastDay,
    LastWeek,
    LastMonth,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SortOption {
    Newest,
    Oldest,
    AmountDesc,
    AmountAsc,
}

impl ComputeTokenView {
    pub fn new(db: WalletDb) -> Self {
        let mut view = Self {
            db,
            tab_index: 0,
            compute_tokens: Vec::new(),
            burn_history: Vec::new(),
            federation_filter: None,
            timeframe_filter: TimeFrame::All,
            sort_by: SortOption::Newest,
        };
        view.reload_data();
        view
    }

    fn reload_data(&mut self) {
        // Load compute tokens
        let tokens = self.db.get_resource_tokens().unwrap_or_default();
        self.compute_tokens = tokens
            .into_iter()
            .filter(|t| t.token_type == "icn:resource/compute" && !t.revoked)
            .collect();

        // Load burn history
        self.burn_history = self
            .db
            .get_token_burns()
            .unwrap_or_default()
            .into_iter()
            .filter(|b| {
                // Apply timeframe filter
                let timestamp = b.burn_timestamp;
                match self.timeframe_filter {
                    TimeFrame::All => true,
                    TimeFrame::LastDay => {
                        let now = Utc::now().timestamp() as u64;
                        timestamp > now - (24 * 60 * 60)
                    }
                    TimeFrame::LastWeek => {
                        let now = Utc::now().timestamp() as u64;
                        timestamp > now - (7 * 24 * 60 * 60)
                    }
                    TimeFrame::LastMonth => {
                        let now = Utc::now().timestamp() as u64;
                        timestamp > now - (30 * 24 * 60 * 60)
                    }
                }
            })
            .filter(|b| {
                // Apply federation filter
                if let Some(fed) = &self.federation_filter {
                    b.federation_scope == *fed
                } else {
                    true
                }
            })
            .collect();

        // Apply sorting
        match self.sort_by {
            SortOption::Newest => {
                self.burn_history.sort_by(|a, b| b.burn_timestamp.cmp(&a.burn_timestamp));
            }
            SortOption::Oldest => {
                self.burn_history.sort_by(|a, b| a.burn_timestamp.cmp(&b.burn_timestamp));
            }
            SortOption::AmountDesc => {
                self.burn_history.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
            }
            SortOption::AmountAsc => {
                self.burn_history.sort_by(|a, b| a.amount.partial_cmp(&b.amount).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
    }

    fn render_balance_tab(&self, frame: &mut RenderFrame, area: Rect) {
        // Calculate total tokens by federation
        let mut federation_totals: HashMap<String, f64> = HashMap::new();
        
        for token in &self.compute_tokens {
            *federation_totals.entry(token.federation_scope.clone()).or_default() += token.amount;
        }
        
        let total_compute = self.compute_tokens.iter().map(|t| t.amount).sum::<f64>();
        
        // Create rows for each federation
        let mut rows = federation_totals
            .iter()
            .map(|(fed, amount)| {
                Row::new(vec![
                    Cell::from(fed.clone()),
                    Cell::from(format!("{:.2}", amount)),
                ])
            })
            .collect::<Vec<_>>();
            
        // Add total row
        rows.push(
            Row::new(vec![
                Cell::from("TOTAL").style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from(format!("{:.2}", total_compute)).style(Style::default().add_modifier(Modifier::BOLD)),
            ])
        );
        
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Federation").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Available Compute Units").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[Constraint::Percentage(50), Constraint::Percentage(50)])
            .block(Block::default().title("Compute Token Balance").borders(Borders::ALL));
            
        frame.render_widget(table, area);
    }

    fn render_burn_history_tab(&self, frame: &mut RenderFrame, area: Rect) {
        // Split area for filters and table
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(10)].as_ref())
            .split(area);
            
        // Render filters
        let filters = vec![
            Span::raw("Timeframe: "),
            Span::styled(
                match self.timeframe_filter {
                    TimeFrame::All => "All",
                    TimeFrame::LastDay => "Last 24h",
                    TimeFrame::LastWeek => "Last Week",
                    TimeFrame::LastMonth => "Last Month",
                },
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" | Sort: "),
            Span::styled(
                match self.sort_by {
                    SortOption::Newest => "Newest First",
                    SortOption::Oldest => "Oldest First",
                    SortOption::AmountDesc => "Amount (High→Low)",
                    SortOption::AmountAsc => "Amount (Low→High)",
                },
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" | Federation: "),
            Span::styled(
                self.federation_filter.clone().unwrap_or_else(|| "All".to_string()),
                Style::default().fg(Color::Yellow),
            ),
        ];
        
        let filters_paragraph = Paragraph::new(Spans::from(filters))
            .block(Block::default().title("Filters").borders(Borders::ALL));
            
        frame.render_widget(filters_paragraph, chunks[0]);
        
        // Render burn history table
        let rows = self.burn_history
            .iter()
            .map(|burn| {
                Row::new(vec![
                    Cell::from(format_timestamp(burn.burn_timestamp)),
                    Cell::from(format!("{:.2}", burn.amount)),
                    Cell::from(burn.federation_scope.clone()),
                    Cell::from(burn.job_id.clone().unwrap_or_else(|| "-".to_string())),
                ])
            })
            .collect::<Vec<_>>();
            
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Timestamp").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Amount").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Federation").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Job ID").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[
                Constraint::Percentage(25),
                Constraint::Percentage(15),
                Constraint::Percentage(30),
                Constraint::Percentage(30),
            ])
            .block(Block::default().title("Compute Token Burn History").borders(Borders::ALL));
            
        frame.render_widget(table, chunks[1]);
    }
    
    fn render_analytics_tab(&self, frame: &mut RenderFrame, area: Rect) {
        // Group burns by federation
        let mut federation_burns: HashMap<String, f64> = HashMap::new();
        
        for burn in &self.burn_history {
            *federation_burns.entry(burn.federation_scope.clone()).or_default() += burn.amount;
        }
        
        // Calculate totals
        let total_burned = self.burn_history.iter().map(|b| b.amount).sum::<f64>();
        let total_jobs = self.burn_history.iter().filter(|b| b.job_id.is_some()).count();
        
        // Create rows
        let mut rows = federation_burns
            .iter()
            .map(|(fed, amount)| {
                let percentage = if total_burned > 0.0 {
                    (amount / total_burned) * 100.0
                } else {
                    0.0
                };
                
                Row::new(vec![
                    Cell::from(fed.clone()),
                    Cell::from(format!("{:.2}", amount)),
                    Cell::from(format!("{:.1}%", percentage)),
                ])
            })
            .collect::<Vec<_>>();
            
        // Add total row
        rows.push(
            Row::new(vec![
                Cell::from("TOTAL").style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from(format!("{:.2}", total_burned)).style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from("100.0%").style(Style::default().add_modifier(Modifier::BOLD)),
            ])
        );
        
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Federation").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Total Burned").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Percentage").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[
                Constraint::Percentage(40),
                Constraint::Percentage(30),
                Constraint::Percentage(30),
            ])
            .block(Block::default().title(format!("Usage Analytics (Total Jobs: {})", total_jobs)).borders(Borders::ALL));
            
        frame.render_widget(table, area);
    }
}

impl Component for ComputeTokenView {
    fn render(&mut self, frame: &mut RenderFrame, area: Rect) {
        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(area);
            
        // Create tabs
        let tabs = Tabs::new(vec![
                Spans::from("Balance"),
                Spans::from("Burn History"),
                Spans::from("Analytics"),
            ])
            .select(self.tab_index)
            .block(Block::default().title("Compute Tokens").borders(Borders::ALL))
            .style(Style::default())
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
            
        frame.render_widget(tabs, chunks[0]);
        
        // Render selected tab content
        match self.tab_index {
            0 => self.render_balance_tab(frame, chunks[1]),
            1 => self.render_burn_history_tab(frame, chunks[1]),
            2 => self.render_analytics_tab(frame, chunks[1]),
            _ => {},
        }
    }

    fn handle_input(&mut self, event: InputEvent) -> bool {
        match event {
            InputEvent::KeyTab => {
                // Switch tab
                self.tab_index = (self.tab_index + 1) % 3;
                true
            }
            InputEvent::KeyBacktab => {
                // Switch tab backwards
                self.tab_index = (self.tab_index + 2) % 3;
                true
            }
            InputEvent::KeyChar('r') => {
                // Reload data
                self.reload_data();
                true
            }
            InputEvent::KeyChar('t') => {
                // Cycle timeframe filter
                self.timeframe_filter = match self.timeframe_filter {
                    TimeFrame::All => TimeFrame::LastDay,
                    TimeFrame::LastDay => TimeFrame::LastWeek,
                    TimeFrame::LastWeek => TimeFrame::LastMonth,
                    TimeFrame::LastMonth => TimeFrame::All,
                };
                self.reload_data();
                true
            }
            InputEvent::KeyChar('s') => {
                // Cycle sort option
                self.sort_by = match self.sort_by {
                    SortOption::Newest => SortOption::Oldest,
                    SortOption::Oldest => SortOption::AmountDesc,
                    SortOption::AmountDesc => SortOption::AmountAsc,
                    SortOption::AmountAsc => SortOption::Newest,
                };
                self.reload_data();
                true
            }
            InputEvent::KeyChar('f') => {
                // Cycle federation filter
                let federations: Vec<String> = self.burn_history
                    .iter()
                    .map(|b| b.federation_scope.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                
                if federations.is_empty() {
                    self.federation_filter = None;
                } else if self.federation_filter.is_none() {
                    self.federation_filter = Some(federations[0].clone());
                } else {
                    let current = self.federation_filter.as_ref().unwrap();
                    let position = federations.iter().position(|f| f == current);
                    
                    if let Some(pos) = position {
                        if pos == federations.len() - 1 {
                            self.federation_filter = None;
                        } else {
                            self.federation_filter = Some(federations[pos + 1].clone());
                        }
                    } else {
                        self.federation_filter = None;
                    }
                }
                
                self.reload_data();
                true
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "Compute Tokens"
    }

    fn help_text(&self) -> Vec<Spans> {
        vec![
            Spans::from(vec![
                Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Switch tabs | "),
                Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Reload data | "),
                Span::styled("t", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Change timeframe | "),
                Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Change sort order | "),
                Span::styled("f", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Filter by federation"),
            ]),
        ]
    }
} 