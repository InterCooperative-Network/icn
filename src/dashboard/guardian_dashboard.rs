use std::collections::HashMap;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Row, Table},
    Frame,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::identity::IdentityManager;
use crate::federation::FederationClient;
use crate::tui::Component;
use crate::storage::db::WalletDb;
use crate::credentials::CredentialManager;

/// Guardian Circle proposal states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GuardianCircleState {
    ProposalSubmitted,
    GuardiansSelected,
    Deliberation,
    VotingOpen,
    VotingClosed,
    VerdictReached,
    ActionTaken,
    RestorativeFlowInitiated,
    Complete,
}

/// Guardian proposal role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GuardianRole {
    /// User is the proposer
    Proposer,
    /// User is a selected guardian for the proposal
    Guardian,
    /// User is the subject of the proposal
    Subject,
    /// User is just viewing the proposal
    Viewer,
}

/// A proposal in the guardian dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianDashboardProposal {
    /// Proposal ID
    pub id: String,
    
    /// Proposal title
    pub title: String,
    
    /// Circle ID
    pub circle_id: String,
    
    /// Subject ID (content/user)
    pub subject_id: String,
    
    /// Current state
    pub state: GuardianCircleState,
    
    /// User's role in this proposal
    pub role: GuardianRole,
    
    /// Whether the user has voted (if they are a guardian)
    pub has_voted: bool,
    
    /// Creation timestamp
    pub created_at: u64,
    
    /// Last updated timestamp
    pub updated_at: u64,
}

/// Guardian Dashboard tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardianDashboardTab {
    /// "My Cases" - active cases the user is involved in
    MyCases,
    
    /// "History" - historical cases the user was involved in
    History,
    
    /// "Federation" - all active cases in the federation
    Federation,
}

impl GuardianDashboardTab {
    pub fn as_str(&self) -> &'static str {
        match self {
            GuardianDashboardTab::MyCases => "My Cases",
            GuardianDashboardTab::History => "History",
            GuardianDashboardTab::Federation => "Federation",
        }
    }
    
    pub fn all() -> [GuardianDashboardTab; 3] {
        [
            GuardianDashboardTab::MyCases,
            GuardianDashboardTab::History,
            GuardianDashboardTab::Federation,
        ]
    }
}

/// Filter criteria for guardian proposals
#[derive(Debug, Clone)]
pub struct GuardianDashboardFilter {
    /// Filter by circle ID
    pub circle_id: Option<String>,
    
    /// Filter by state
    pub state: Option<GuardianCircleState>,
    
    /// Filter by role
    pub role: Option<GuardianRole>,
}

/// Guardian Dashboard 
pub struct GuardianDashboard {
    /// Active tab
    active_tab: GuardianDashboardTab,
    
    /// Database connection
    db: WalletDb,
    
    /// Identity manager
    identity_manager: IdentityManager,
    
    /// Federation client
    federation_client: FederationClient,
    
    /// Credential manager
    credential_manager: CredentialManager,
    
    /// All proposals
    proposals: Vec<GuardianDashboardProposal>,
    
    /// Filtered proposals
    filtered_proposals: Vec<GuardianDashboardProposal>,
    
    /// Selected proposal index
    selected_proposal: Option<usize>,
    
    /// List state for proposal selection
    list_state: ListState,
    
    /// Current filter
    filter: GuardianDashboardFilter,
    
    /// Show details of selected proposal
    show_details: bool,
    
    /// Show help
    show_help: bool,
}

impl GuardianDashboard {
    /// Create a new guardian dashboard
    pub fn new(
        db: WalletDb,
        identity_manager: IdentityManager,
        federation_client: FederationClient,
        credential_manager: CredentialManager,
    ) -> Self {
        let mut dashboard = Self {
            active_tab: GuardianDashboardTab::MyCases,
            db,
            identity_manager,
            federation_client,
            credential_manager,
            proposals: Vec::new(),
            filtered_proposals: Vec::new(),
            selected_proposal: None,
            list_state: ListState::default(),
            filter: GuardianDashboardFilter {
                circle_id: None,
                state: None,
                role: None,
            },
            show_details: false,
            show_help: false,
        };
        
        // Load initial data
        dashboard.load_data();
        
        dashboard
    }
    
    /// Load proposal data from the database and federation
    pub fn load_data(&mut self) {
        self.proposals.clear();
        
        // In a real implementation, fetch from local database and federation
        self.load_mock_data();
        
        // Apply filters
        self.apply_filters();
        
        // Update selection
        if !self.filtered_proposals.is_empty() && self.selected_proposal.is_none() {
            self.selected_proposal = Some(0);
            self.list_state.select(Some(0));
        } else if self.filtered_proposals.is_empty() {
            self.selected_proposal = None;
            self.list_state.select(None);
        }
    }
    
    /// Load mock data for development
    fn load_mock_data(&mut self) {
        let active_did = match self.identity_manager.get_active_identity() {
            Some(id) => id.did(),
            None => return, // No active identity
        };
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        // Add mock proposals
        self.proposals.push(GuardianDashboardProposal {
            id: "proposal-1".to_string(),
            title: "Content moderation request".to_string(),
            circle_id: "circle-content-1".to_string(),
            subject_id: "content-123".to_string(),
            state: GuardianCircleState::VotingOpen,
            role: GuardianRole::Guardian,
            has_voted: false,
            created_at: now - 86400, // 1 day ago
            updated_at: now - 3600,  // 1 hour ago
        });
        
        self.proposals.push(GuardianDashboardProposal {
            id: "proposal-2".to_string(),
            title: "User conduct review".to_string(),
            circle_id: "circle-conduct-1".to_string(),
            subject_id: "did:icn:user:abc".to_string(),
            state: GuardianCircleState::Deliberation,
            role: GuardianRole::Guardian,
            has_voted: false,
            created_at: now - 172800, // 2 days ago
            updated_at: now - 7200,   // 2 hours ago
        });
        
        self.proposals.push(GuardianDashboardProposal {
            id: "proposal-3".to_string(),
            title: "Resource allocation appeal".to_string(),
            circle_id: "circle-resource-1".to_string(),
            subject_id: "allocation-567".to_string(),
            state: GuardianCircleState::Complete,
            role: GuardianRole::Proposer,
            has_voted: false,
            created_at: now - 604800, // 1 week ago
            updated_at: now - 432000, // 5 days ago
        });
        
        self.proposals.push(GuardianDashboardProposal {
            id: "proposal-4".to_string(),
            title: "Content removal request".to_string(),
            circle_id: "circle-content-1".to_string(),
            subject_id: "content-456".to_string(),
            state: GuardianCircleState::RestorativeFlowInitiated,
            role: GuardianRole::Subject,
            has_voted: false,
            created_at: now - 345600, // 4 days ago
            updated_at: now - 86400,  // 1 day ago
        });
        
        self.proposals.push(GuardianDashboardProposal {
            id: "proposal-5".to_string(),
            title: "Federation policy violation".to_string(),
            circle_id: "circle-policy-1".to_string(),
            subject_id: "app-789".to_string(),
            state: GuardianCircleState::VerdictReached,
            role: GuardianRole::Viewer,
            has_voted: false,
            created_at: now - 518400, // 6 days ago
            updated_at: now - 172800, // 2 days ago
        });
    }
    
    /// Apply the current filters to the proposals
    fn apply_filters(&mut self) {
        self.filtered_proposals = self.proposals.iter().cloned()
            .filter(|p| {
                // Filter by tab
                match self.active_tab {
                    GuardianDashboardTab::MyCases => {
                        // Show active cases where user is involved
                        (p.role == GuardianRole::Guardian || 
                         p.role == GuardianRole::Proposer || 
                         p.role == GuardianRole::Subject) &&
                        (p.state != GuardianCircleState::Complete)
                    },
                    GuardianDashboardTab::History => {
                        // Show historical cases where user was involved
                        (p.role == GuardianRole::Guardian || 
                         p.role == GuardianRole::Proposer || 
                         p.role == GuardianRole::Subject) &&
                        (p.state == GuardianCircleState::Complete)
                    },
                    GuardianDashboardTab::Federation => {
                        // Show all federation cases
                        true
                    },
                }
            })
            .filter(|p| {
                // Apply additional filters
                if let Some(circle_id) = &self.filter.circle_id {
                    if p.circle_id != *circle_id {
                        return false;
                    }
                }
                
                if let Some(state) = &self.filter.state {
                    if p.state != *state {
                        return false;
                    }
                }
                
                if let Some(role) = &self.filter.role {
                    if p.role != *role {
                        return false;
                    }
                }
                
                true
            })
            .collect();
        
        // Sort by updated_at (most recent first)
        self.filtered_proposals.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }
    
    /// Get the current active proposal
    fn active_proposal(&self) -> Option<&GuardianDashboardProposal> {
        match self.selected_proposal {
            Some(index) if index < self.filtered_proposals.len() => {
                Some(&self.filtered_proposals[index])
            },
            _ => None,
        }
    }
    
    /// Handle user input
    pub fn handle_input(&mut self, event: Event) -> bool {
        if let Event::Key(key) = event {
            match key.code {
                // Tab switching
                KeyCode::Tab => {
                    let tabs = GuardianDashboardTab::all();
                    let current_idx = tabs.iter().position(|&t| t == self.active_tab).unwrap_or(0);
                    let next_idx = (current_idx + 1) % tabs.len();
                    self.active_tab = tabs[next_idx];
                    self.apply_filters();
                    return true;
                },
                KeyCode::BackTab => {
                    let tabs = GuardianDashboardTab::all();
                    let current_idx = tabs.iter().position(|&t| t == self.active_tab).unwrap_or(0);
                    let next_idx = if current_idx == 0 { tabs.len() - 1 } else { current_idx - 1 };
                    self.active_tab = tabs[next_idx];
                    self.apply_filters();
                    return true;
                },
                
                // Navigation
                KeyCode::Up => {
                    if let Some(selected) = self.selected_proposal {
                        if selected > 0 {
                            self.selected_proposal = Some(selected - 1);
                            self.list_state.select(self.selected_proposal);
                        }
                    } else if !self.filtered_proposals.is_empty() {
                        self.selected_proposal = Some(0);
                        self.list_state.select(self.selected_proposal);
                    }
                    return true;
                },
                KeyCode::Down => {
                    if let Some(selected) = self.selected_proposal {
                        if selected < self.filtered_proposals.len() - 1 {
                            self.selected_proposal = Some(selected + 1);
                            self.list_state.select(self.selected_proposal);
                        }
                    } else if !self.filtered_proposals.is_empty() {
                        self.selected_proposal = Some(0);
                        self.list_state.select(self.selected_proposal);
                    }
                    return true;
                },
                
                // Details view
                KeyCode::Enter => {
                    if self.active_proposal().is_some() {
                        self.show_details = !self.show_details;
                    }
                    return true;
                },
                
                // Refresh data
                KeyCode::Char('r') => {
                    self.load_data();
                    return true;
                },
                
                // Vote (if user is a guardian and proposal is in voting state)
                KeyCode::Char('v') => {
                    if let Some(proposal) = self.active_proposal() {
                        if proposal.role == GuardianRole::Guardian && 
                           proposal.state == GuardianCircleState::VotingOpen &&
                           !proposal.has_voted {
                            // In a real implementation, open a vote dialog
                            println!("Would open vote dialog for proposal {}", proposal.id);
                            
                            // For mock: mark as voted
                            let idx = self.selected_proposal.unwrap();
                            if idx < self.filtered_proposals.len() {
                                self.filtered_proposals[idx].has_voted = true;
                                
                                // Also update in main list
                                if let Some(main_idx) = self.proposals.iter().position(|p| p.id == proposal.id) {
                                    self.proposals[main_idx].has_voted = true;
                                }
                            }
                        }
                    }
                    return true;
                },
                
                // Toggle help
                KeyCode::Char('?') => {
                    self.show_help = !self.show_help;
                    return true;
                },
                
                // Exit details view or dashboard
                KeyCode::Esc => {
                    if self.show_details {
                        self.show_details = false;
                        return true;
                    }
                    if self.show_help {
                        self.show_help = false;
                        return true;
                    }
                },
                
                _ => {}
            }
        }
        
        false
    }
    
    /// Render the Guardian Dashboard
    fn render_dashboard<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        // Create main layout (tabs on top, content below)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tabs
                Constraint::Min(0),    // Content
            ])
            .split(area);
            
        // Create the tab bar
        let tabs = Tabs::new(
            GuardianDashboardTab::all()
                .iter()
                .map(|t| {
                    Spans::from(Span::styled(
                        t.as_str(),
                        if *t == self.active_tab {
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        },
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .block(Block::default().title("Guardian Circle Dashboard").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .select(
            GuardianDashboardTab::all()
                .iter()
                .position(|t| *t == self.active_tab)
                .unwrap_or(0),
        );
        
        f.render_widget(tabs, chunks[0]);
        
        // If we're showing details, render proposal details
        if self.show_details {
            if let Some(proposal) = self.active_proposal() {
                self.render_proposal_details(f, chunks[1], proposal);
            }
        } else if self.show_help {
            self.render_help(f, chunks[1]);
        } else {
            // Otherwise render the proposal list
            self.render_proposal_list(f, chunks[1]);
        }
    }
    
    /// Render the proposal list
    fn render_proposal_list<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let items: Vec<ListItem> = self.filtered_proposals
            .iter()
            .map(|p| {
                let title = match p.role {
                    GuardianRole::Guardian => {
                        if p.state == GuardianCircleState::VotingOpen && !p.has_voted {
                            // Highlight proposals that need votes
                            Spans::from(vec![
                                Span::styled("[NEEDS VOTE] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                                Span::styled(&p.title, Style::default().fg(Color::White)),
                            ])
                        } else {
                            Spans::from(vec![
                                Span::styled("[GUARDIAN] ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                                Span::styled(&p.title, Style::default().fg(Color::White)),
                            ])
                        }
                    },
                    GuardianRole::Proposer => {
                        Spans::from(vec![
                            Span::styled("[PROPOSER] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                            Span::styled(&p.title, Style::default().fg(Color::White)),
                        ])
                    },
                    GuardianRole::Subject => {
                        Spans::from(vec![
                            Span::styled("[SUBJECT] ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                            Span::styled(&p.title, Style::default().fg(Color::White)),
                        ])
                    },
                    GuardianRole::Viewer => {
                        Spans::from(Span::styled(&p.title, Style::default().fg(Color::White)))
                    },
                };
                
                let status = Spans::from(vec![
                    Span::raw("  Status: "),
                    match p.state {
                        GuardianCircleState::ProposalSubmitted => 
                            Span::styled("Submitted", Style::default().fg(Color::Gray)),
                        GuardianCircleState::GuardiansSelected => 
                            Span::styled("Guardians Selected", Style::default().fg(Color::Blue)),
                        GuardianCircleState::Deliberation => 
                            Span::styled("Deliberation", Style::default().fg(Color::Cyan)),
                        GuardianCircleState::VotingOpen => 
                            Span::styled("Voting Open", Style::default().fg(Color::Yellow)),
                        GuardianCircleState::VotingClosed => 
                            Span::styled("Voting Closed", Style::default().fg(Color::DarkGray)),
                        GuardianCircleState::VerdictReached => 
                            Span::styled("Verdict Reached", Style::default().fg(Color::Magenta)),
                        GuardianCircleState::ActionTaken => 
                            Span::styled("Action Taken", Style::default().fg(Color::Green)),
                        GuardianCircleState::RestorativeFlowInitiated => 
                            Span::styled("Restorative Flow", Style::default().fg(Color::LightCyan)),
                        GuardianCircleState::Complete => 
                            Span::styled("Complete", Style::default().fg(Color::DarkGray)),
                    },
                ]);
                
                ListItem::new(vec![title, status])
            })
            .collect();
            
        let list_block = if self.filtered_proposals.is_empty() {
            Block::default()
                .title(format!("{} - No proposals found", self.active_tab.as_str()))
                .borders(Borders::ALL)
        } else {
            Block::default()
                .title(format!("{} - {} proposal(s)", self.active_tab.as_str(), self.filtered_proposals.len()))
                .borders(Borders::ALL)
        };
        
        let list = List::new(items)
            .block(list_block)
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");
            
        f.render_stateful_widget(list, area, &mut self.list_state);
        
        // Render footer with keys
        let footer_text = vec![
            Spans::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": View Details  "),
                Span::styled("Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch Tabs  "),
                Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate  "),
                Span::styled("v", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Vote  "),
                Span::styled("r", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Refresh  "),
                Span::styled("?", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Help"),
            ]),
        ];
        
        let footer = Paragraph::new(footer_text)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::TOP));
            
        // Create one more vertical split for the footer
        let footer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);
            
        f.render_widget(footer, footer_layout[1]);
    }
    
    /// Render details of a proposal
    fn render_proposal_details<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, proposal: &GuardianDashboardProposal) {
        // Create layout for the details view
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Length(10), // Info table
                Constraint::Min(0),     // Details content
            ])
            .split(area);
            
        // Render title
        let title = Paragraph::new(vec![
            Spans::from(vec![
                Span::styled(&proposal.title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Spans::from(vec![
                Span::styled(
                    format!("ID: {}", proposal.id),
                    Style::default().fg(Color::Gray),
                ),
            ]),
        ])
        .block(Block::default().title("Proposal Details").borders(Borders::ALL));
        
        f.render_widget(title, chunks[0]);
        
        // Render info table
        let info_rows = vec![
            Row::new(vec![
                "Circle ID".to_string(),
                proposal.circle_id.clone(),
            ]),
            Row::new(vec![
                "Subject".to_string(),
                proposal.subject_id.clone(),
            ]),
            Row::new(vec![
                "State".to_string(),
                format!("{:?}", proposal.state),
            ]),
            Row::new(vec![
                "Your Role".to_string(),
                format!("{:?}", proposal.role),
            ]),
            Row::new(vec![
                "Created".to_string(),
                format_timestamp(proposal.created_at),
            ]),
            Row::new(vec![
                "Updated".to_string(),
                format_timestamp(proposal.updated_at),
            ]),
        ];
        
        let info_table = Table::new(info_rows)
            .block(Block::default().title("Information").borders(Borders::ALL))
            .widths(&[
                Constraint::Percentage(20),
                Constraint::Percentage(80),
            ])
            .column_spacing(1)
            .header(Row::new(vec!["Field", "Value"])
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                
        f.render_widget(info_table, chunks[1]);
        
        // Render actions panel
        let actions = match proposal.role {
            GuardianRole::Guardian => {
                match proposal.state {
                    GuardianCircleState::VotingOpen if !proposal.has_voted => {
                        vec![
                            Spans::from(vec![
                                Span::styled("Actions: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                                Span::styled("Press 'v' to Vote", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                            ]),
                        ]
                    },
                    _ => {
                        vec![
                            Spans::from(vec![
                                Span::styled("Actions: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                                Span::raw("No actions available in current state"),
                            ]),
                        ]
                    }
                }
            },
            GuardianRole::Subject => {
                if proposal.state == GuardianCircleState::RestorativeFlowInitiated {
                    vec![
                        Spans::from(vec![
                            Span::styled("Actions: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                            Span::styled("Review restorative measures required", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        ]),
                    ]
                } else {
                    vec![
                        Spans::from(vec![
                            Span::styled("Actions: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                            Span::raw("No actions available in current state"),
                        ]),
                    ]
                }
            },
            _ => {
                vec![
                    Spans::from(vec![
                        Span::styled("Actions: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::raw("No actions available for your role"),
                    ]),
                ]
            },
        };
        
        // Create details panel with additional info and actions
        let details = Paragraph::new(actions)
            .block(Block::default().title("Actions").borders(Borders::ALL));
            
        f.render_widget(details, chunks[2]);
    }
    
    /// Render help screen
    fn render_help<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let help_text = vec![
            Spans::from(Span::styled("Guardian Dashboard Help", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("/"),
                Span::styled("Shift+Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch between dashboard views"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Up/Down", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate proposals"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": View proposal details"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("v", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Vote on proposal (if eligible)"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("r", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Refresh data"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Exit details or help view"),
            ]),
            Spans::from(""),
            Spans::from(""),
            Spans::from(Span::styled("Guardian Roles:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Guardian", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                Span::raw(": You are selected to review and vote on the proposal"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Proposer", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": You submitted this proposal"),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Subject", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::raw(": You are the subject of this proposal"),
            ]),
        ];
        
        let help = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL));
            
        f.render_widget(help, area);
    }
}

impl Component for GuardianDashboard {
    fn render<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        self.render_dashboard(f, area);
    }
    
    fn handle_input(&mut self, event: Event) -> bool {
        self.handle_input(event)
    }
}

/// Format a timestamp as a readable date
fn format_timestamp(timestamp: u64) -> String {
    // For simplicity, format relative to now
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
        
    let diff = if now > timestamp {
        now - timestamp
    } else {
        0
    };
    
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{} minutes ago", diff / 60)
    } else if diff < 86400 {
        format!("{} hours ago", diff / 3600)
    } else {
        format!("{} days ago", diff / 86400)
    }
} 