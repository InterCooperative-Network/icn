use crate::db::WalletDb;
use crate::resources::{ResourceToken, TokenBurn};
use crate::ui::{CentralPanel, Component, InputEvent, RenderFrame};
use crate::utils::format_timestamp;
use chrono::Utc;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs};
use std::collections::HashMap;

/// View for general resource tokens
pub struct ResourceView {
    db: WalletDb,
    tab_index: usize,
    resource_tokens: Vec<ResourceToken>,
    selected_token_index: usize,
    filter_type: Option<String>,
    available_types: Vec<String>,
}

impl ResourceView {
    pub fn new(db: WalletDb) -> Self {
        let mut view = Self {
            db,
            tab_index: 0,
            resource_tokens: Vec::new(),
            selected_token_index: 0,
            filter_type: None,
            available_types: Vec::new(),
        };
        view.reload_data();
        view
    }

    fn reload_data(&mut self) {
        // Load tokens from database
        let all_tokens = self.db.get_resource_tokens().unwrap_or_default();
        
        // Extract available types
        let mut type_set = std::collections::HashSet::new();
        for token in &all_tokens {
            if !token.token_type.is_empty() && !token.revoked {
                type_set.insert(token.token_type.clone());
            }
        }
        
        self.available_types = type_set.into_iter().collect();
        self.available_types.sort();
        
        // Filter tokens based on current filter
        self.resource_tokens = if let Some(filter) = &self.filter_type {
            all_tokens.into_iter()
                .filter(|t| t.token_type == *filter && !t.revoked)
                .collect()
        } else {
            all_tokens.into_iter()
                .filter(|t| !t.revoked)
                .collect()
        };
        
        // Sort tokens by type, then by amount
        self.resource_tokens.sort_by(|a, b| {
            let type_cmp = a.token_type.cmp(&b.token_type);
            if type_cmp == std::cmp::Ordering::Equal {
                b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                type_cmp
            }
        });
        
        // Adjust selected index if needed
        if !self.resource_tokens.is_empty() && self.selected_token_index >= self.resource_tokens.len() {
            self.selected_token_index = self.resource_tokens.len() - 1;
        }
    }

    fn render_tokens_tab(&self, frame: &mut RenderFrame, area: Rect) {
        // Create header with type filter
        let filter_text = match &self.filter_type {
            Some(filter) => format!("Filtering by type: {}", filter),
            None => "Showing all resource tokens".to_string(),
        };
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(10)].as_ref())
            .split(area);
            
        let filter_para = Paragraph::new(filter_text)
            .block(Block::default().title("Filter").borders(Borders::ALL));
            
        frame.render_widget(filter_para, chunks[0]);
        
        // Create token table
        if self.resource_tokens.is_empty() {
            let empty_msg = if self.filter_type.is_some() {
                "No tokens found with the current filter."
            } else {
                "No resource tokens found."
            };
            
            let empty_para = Paragraph::new(empty_msg)
                .block(Block::default().title("Resource Tokens").borders(Borders::ALL));
                
            frame.render_widget(empty_para, chunks[1]);
            return;
        }
        
        let rows = self.resource_tokens.iter().enumerate().map(|(idx, token)| {
            let style = if idx == self.selected_token_index {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            
            let expires = if let Some(expires_at) = token.expires_at {
                format_timestamp(expires_at)
            } else {
                "Never".to_string()
            };
            
            Row::new(vec![
                Cell::from(token.token_type.clone()),
                Cell::from(token.federation_scope.clone()),
                Cell::from(format!("{:.2}", token.amount)),
                Cell::from(expires),
            ]).style(style)
        }).collect::<Vec<_>>();
        
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Token Type").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Federation").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Amount").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Expires").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[
                Constraint::Percentage(30),
                Constraint::Percentage(30),
                Constraint::Percentage(15),
                Constraint::Percentage(25),
            ])
            .block(Block::default().title("Resource Tokens").borders(Borders::ALL));
            
        frame.render_widget(table, chunks[1]);
    }
    
    fn render_summary_tab(&self, frame: &mut RenderFrame, area: Rect) {
        // Group tokens by type
        let mut summary_by_type: HashMap<String, f64> = HashMap::new();
        
        for token in &self.resource_tokens {
            *summary_by_type.entry(token.token_type.clone()).or_default() += token.amount;
        }
        
        // Create rows for each type
        let mut rows = summary_by_type
            .iter()
            .map(|(token_type, amount)| {
                Row::new(vec![
                    Cell::from(token_type.clone()),
                    Cell::from(format!("{:.2}", amount)),
                ])
            })
            .collect::<Vec<_>>();
            
        // Sort by token type
        rows.sort_by(|a, b| {
            let a_type = a.cells[0].content.to_string();
            let b_type = b.cells[0].content.to_string();
            a_type.cmp(&b_type)
        });
        
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Token Type").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Total Amount").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[Constraint::Percentage(70), Constraint::Percentage(30)])
            .block(Block::default().title("Resource Summary").borders(Borders::ALL));
            
        frame.render_widget(table, area);
    }
}

impl Component for ResourceView {
    fn render(&mut self, frame: &mut RenderFrame, area: Rect) {
        // Create tabs layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(area);
            
        // Create tabs
        let tabs = Tabs::new(vec![
                Spans::from("Tokens"),
                Spans::from("Summary"),
            ])
            .select(self.tab_index)
            .block(Block::default().title("Resource Tokens").borders(Borders::ALL))
            .style(Style::default())
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
            
        frame.render_widget(tabs, chunks[0]);
        
        // Render selected tab
        match self.tab_index {
            0 => self.render_tokens_tab(frame, chunks[1]),
            1 => self.render_summary_tab(frame, chunks[1]),
            _ => {},
        }
    }

    fn handle_input(&mut self, event: InputEvent) -> bool {
        match event {
            InputEvent::KeyTab => {
                // Switch tab
                self.tab_index = (self.tab_index + 1) % 2;
                true
            }
            InputEvent::KeyBacktab => {
                // Switch tab backwards
                self.tab_index = (self.tab_index + 1) % 2;
                true
            }
            InputEvent::KeyChar('r') => {
                // Reload data
                self.reload_data();
                true
            }
            InputEvent::KeyChar('f') => {
                // Toggle filter
                if self.available_types.is_empty() {
                    self.filter_type = None;
                } else if self.filter_type.is_none() {
                    self.filter_type = Some(self.available_types[0].clone());
                } else {
                    let current = self.filter_type.as_ref().unwrap();
                    let position = self.available_types.iter().position(|t| t == current);
                    
                    if let Some(pos) = position {
                        if pos == self.available_types.len() - 1 {
                            self.filter_type = None;
                        } else {
                            self.filter_type = Some(self.available_types[pos + 1].clone());
                        }
                    } else {
                        self.filter_type = None;
                    }
                }
                
                self.reload_data();
                true
            }
            InputEvent::KeyUp => {
                if !self.resource_tokens.is_empty() {
                    self.selected_token_index = self.selected_token_index.saturating_sub(1);
                }
                true
            }
            InputEvent::KeyDown => {
                if !self.resource_tokens.is_empty() {
                    self.selected_token_index = (self.selected_token_index + 1).min(self.resource_tokens.len() - 1);
                }
                true
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "Resource Tokens"
    }

    fn help_text(&self) -> Vec<Spans> {
        vec![
            Spans::from(vec![
                Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Switch tabs | "),
                Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Reload data | "),
                Span::styled("f", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Filter by type | "),
                Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Navigate"),
            ]),
        ]
    }
} 