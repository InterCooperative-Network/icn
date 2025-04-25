use std::collections::HashMap;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
    Frame,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use crate::tui::Component;
use crate::federation::FederationClient;
use crate::credentials::CredentialManager;

/// Status of a restorative flow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestorativeFlowStatus {
    /// Flow has been initiated
    Initiated,
    
    /// Flow is in progress
    InProgress,
    
    /// Flow has been completed successfully
    Completed,
    
    /// Flow was terminated early
    Terminated,
}

/// Type of restorative flow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestorativeFlowType {
    /// Restorative dialogue between parties
    Dialogue,
    
    /// Formal mediation with a mediator
    Mediation,
    
    /// Educational module that must be completed
    Educational,
}

impl RestorativeFlowType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RestorativeFlowType::Dialogue => "Restorative Dialogue",
            RestorativeFlowType::Mediation => "Mediation",
            RestorativeFlowType::Educational => "Educational Module",
        }
    }
}

/// A step in a restorative flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorativeStep {
    /// Step ID
    pub id: String,
    
    /// Step title
    pub title: String,
    
    /// Step description
    pub description: String,
    
    /// Whether the step is completed
    pub completed: bool,
    
    /// Step completion date, if completed
    pub completion_date: Option<String>,
    
    /// Required action description
    pub required_action: String,
}

/// A restorative flow process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorativeFlowProcess {
    /// Flow ID
    pub id: String,
    
    /// Flow title
    pub title: String,
    
    /// Flow description
    pub description: String,
    
    /// Flow type
    pub flow_type: RestorativeFlowType,
    
    /// Current status
    pub status: RestorativeFlowStatus,
    
    /// Related proposal ID
    pub proposal_id: String,
    
    /// Steps in the flow
    pub steps: Vec<RestorativeStep>,
    
    /// Current step index
    pub current_step: usize,
    
    /// Facilitator DID
    pub facilitator: String,
    
    /// Participants DIDs
    pub participants: Vec<String>,
    
    /// Creation date
    pub created_at: String,
    
    /// Last updated date
    pub updated_at: String,
}

/// Restorative Flow UI component
pub struct RestorativeFlow {
    /// List of all flows the user is involved in
    flows: Vec<RestorativeFlowProcess>,
    
    /// Currently selected flow index
    selected_flow: Option<usize>,
    
    /// List state for flow selection
    flow_list_state: ListState,
    
    /// Currently selected step index
    selected_step: Option<usize>,
    
    /// List state for step selection
    step_list_state: ListState,
    
    /// Federation client for interactions
    federation_client: FederationClient,
    
    /// Credential manager for issuing VCs
    credential_manager: CredentialManager,
    
    /// Whether we're showing flow details
    show_flow_details: bool,
    
    /// Whether we're showing step details
    show_step_details: bool,
}

impl RestorativeFlow {
    /// Create a new restorative flow component
    pub fn new(
        federation_client: FederationClient,
        credential_manager: CredentialManager,
    ) -> Self {
        let mut flow = Self {
            flows: Vec::new(),
            selected_flow: None,
            flow_list_state: ListState::default(),
            selected_step: None,
            step_list_state: ListState::default(),
            federation_client,
            credential_manager,
            show_flow_details: false,
            show_step_details: false,
        };
        
        // Load initial flow data
        flow.load_flows();
        
        flow
    }
    
    /// Load flows from the federation
    fn load_flows(&mut self) {
        // In a real implementation, this would fetch data from the federation
        // For now, add mock data
        self.add_mock_flows();
        
        // Select first flow if available
        if !self.flows.is_empty() && self.selected_flow.is_none() {
            self.selected_flow = Some(0);
            self.flow_list_state.select(Some(0));
            
            // Reset step selection
            self.selected_step = None;
            self.step_list_state.select(None);
        }
    }
    
    /// Add mock flow data for development
    fn add_mock_flows(&mut self) {
        // Add a mock dialogue flow
        self.flows.push(RestorativeFlowProcess {
            id: "flow-123".to_string(),
            title: "Content Dispute Resolution".to_string(),
            description: "Restorative dialogue to resolve a dispute about content moderation".to_string(),
            flow_type: RestorativeFlowType::Dialogue,
            status: RestorativeFlowStatus::InProgress,
            proposal_id: "proposal-123".to_string(),
            steps: vec![
                RestorativeStep {
                    id: "step-1".to_string(),
                    title: "Initial Agreement".to_string(),
                    description: "Agree to participate in the dialogue process".to_string(),
                    completed: true,
                    completion_date: Some("2023-05-15".to_string()),
                    required_action: "Sign agreement to participate".to_string(),
                },
                RestorativeStep {
                    id: "step-2".to_string(),
                    title: "Share Perspectives".to_string(),
                    description: "Share your perspective on the issue".to_string(),
                    completed: true,
                    completion_date: Some("2023-05-18".to_string()),
                    required_action: "Submit your perspective document".to_string(),
                },
                RestorativeStep {
                    id: "step-3".to_string(),
                    title: "Facilitated Discussion".to_string(),
                    description: "Participate in a facilitated discussion with all parties".to_string(),
                    completed: false,
                    completion_date: None,
                    required_action: "Join the scheduled discussion".to_string(),
                },
                RestorativeStep {
                    id: "step-4".to_string(),
                    title: "Resolution Agreement".to_string(),
                    description: "Create and sign a resolution agreement".to_string(),
                    completed: false,
                    completion_date: None,
                    required_action: "Review and sign the resolution agreement".to_string(),
                },
            ],
            current_step: 2,
            facilitator: "did:icn:facilitator:123".to_string(),
            participants: vec![
                "did:icn:user:123".to_string(),
                "did:icn:user:456".to_string(),
            ],
            created_at: "2023-05-10".to_string(),
            updated_at: "2023-05-18".to_string(),
        });
        
        // Add a mock educational flow
        self.flows.push(RestorativeFlowProcess {
            id: "flow-456".to_string(),
            title: "Community Guidelines Education".to_string(),
            description: "Educational module on community guidelines and respectful engagement".to_string(),
            flow_type: RestorativeFlowType::Educational,
            status: RestorativeFlowStatus::Initiated,
            proposal_id: "proposal-456".to_string(),
            steps: vec![
                RestorativeStep {
                    id: "step-1".to_string(),
                    title: "Module Introduction".to_string(),
                    description: "Introduction to community guidelines".to_string(),
                    completed: false,
                    completion_date: None,
                    required_action: "Review introduction materials".to_string(),
                },
                RestorativeStep {
                    id: "step-2".to_string(),
                    title: "Learning Module 1".to_string(),
                    description: "Respectful communication principles".to_string(),
                    completed: false,
                    completion_date: None,
                    required_action: "Complete learning module and quiz".to_string(),
                },
                RestorativeStep {
                    id: "step-3".to_string(),
                    title: "Learning Module 2".to_string(),
                    description: "Conflict resolution strategies".to_string(),
                    completed: false,
                    completion_date: None,
                    required_action: "Complete learning module and quiz".to_string(),
                },
                RestorativeStep {
                    id: "step-4".to_string(),
                    title: "Final Assessment".to_string(),
                    description: "Final assessment to demonstrate understanding".to_string(),
                    completed: false,
                    completion_date: None,
                    required_action: "Complete assessment with 80% score or higher".to_string(),
                },
            ],
            current_step: 0,
            facilitator: "did:icn:facilitator:789".to_string(),
            participants: vec![
                "did:icn:user:123".to_string(),
            ],
            created_at: "2023-05-20".to_string(),
            updated_at: "2023-05-20".to_string(),
        });
    }
    
    /// Get the current active flow
    fn active_flow(&self) -> Option<&RestorativeFlowProcess> {
        match self.selected_flow {
            Some(index) if index < self.flows.len() => Some(&self.flows[index]),
            _ => None,
        }
    }
    
    /// Get the current active step
    fn active_step(&self) -> Option<&RestorativeStep> {
        let flow = self.active_flow()?;
        let step_index = match self.selected_step {
            Some(index) if index < flow.steps.len() => index,
            _ => return None,
        };
        
        Some(&flow.steps[step_index])
    }
    
    /// Complete the current step
    fn complete_current_step(&mut self) {
        if let Some(flow_index) = self.selected_flow {
            if flow_index < self.flows.len() {
                let flow = &mut self.flows[flow_index];
                
                // If we're on the current step and it's not completed yet
                if let Some(step_index) = self.selected_step {
                    if step_index < flow.steps.len() && !flow.steps[step_index].completed {
                        // Mark the step as completed
                        flow.steps[step_index].completed = true;
                        flow.steps[step_index].completion_date = Some("2023-05-25".to_string()); // Use current date in real impl
                        
                        // If this is the current step, advance to the next one
                        if step_index == flow.current_step && flow.current_step < flow.steps.len() - 1 {
                            flow.current_step += 1;
                        }
                        
                        // If all steps are complete, mark the flow as completed
                        if flow.steps.iter().all(|s| s.completed) {
                            flow.status = RestorativeFlowStatus::Completed;
                        }
                        
                        // In a real implementation, we'd also:
                        // 1. Send this update to the federation
                        // 2. Generate a credential for completing this step
                    }
                }
            }
        }
    }
    
    /// Handle user input
    pub fn handle_input(&mut self, event: Event) -> bool {
        if let Event::Key(key) = event {
            match key.code {
                // Navigation
                KeyCode::Up => {
                    if self.show_step_details {
                        // No navigation in step details
                        return true;
                    } else if self.show_flow_details {
                        // Navigate steps
                        if let Some(selected) = self.selected_step {
                            if selected > 0 {
                                self.selected_step = Some(selected - 1);
                                self.step_list_state.select(self.selected_step);
                            }
                        } else if let Some(flow) = self.active_flow() {
                            if !flow.steps.is_empty() {
                                self.selected_step = Some(0);
                                self.step_list_state.select(self.selected_step);
                            }
                        }
                    } else {
                        // Navigate flows
                        if let Some(selected) = self.selected_flow {
                            if selected > 0 {
                                self.selected_flow = Some(selected - 1);
                                self.flow_list_state.select(self.selected_flow);
                            }
                        } else if !self.flows.is_empty() {
                            self.selected_flow = Some(0);
                            self.flow_list_state.select(self.selected_flow);
                        }
                    }
                    return true;
                },
                KeyCode::Down => {
                    if self.show_step_details {
                        // No navigation in step details
                        return true;
                    } else if self.show_flow_details {
                        // Navigate steps
                        if let Some(flow) = self.active_flow() {
                            if let Some(selected) = self.selected_step {
                                if selected < flow.steps.len() - 1 {
                                    self.selected_step = Some(selected + 1);
                                    self.step_list_state.select(self.selected_step);
                                }
                            } else if !flow.steps.is_empty() {
                                self.selected_step = Some(0);
                                self.step_list_state.select(self.selected_step);
                            }
                        }
                    } else {
                        // Navigate flows
                        if let Some(selected) = self.selected_flow {
                            if selected < self.flows.len() - 1 {
                                self.selected_flow = Some(selected + 1);
                                self.flow_list_state.select(self.selected_flow);
                            }
                        } else if !self.flows.is_empty() {
                            self.selected_flow = Some(0);
                            self.flow_list_state.select(self.selected_flow);
                        }
                    }
                    return true;
                },
                
                // Enter - Go to details view
                KeyCode::Enter => {
                    if self.show_step_details {
                        // Already at the most detailed level
                        return true;
                    } else if self.show_flow_details {
                        // Show step details if a step is selected
                        if self.active_step().is_some() {
                            self.show_step_details = true;
                        }
                    } else {
                        // Show flow details if a flow is selected
                        if self.active_flow().is_some() {
                            self.show_flow_details = true;
                            // Reset step selection
                            self.selected_step = None;
                            self.step_list_state.select(None);
                        }
                    }
                    return true;
                },
                
                // Complete step
                KeyCode::Char('c') => {
                    if self.show_step_details {
                        self.complete_current_step();
                    }
                    return true;
                },
                
                // Refresh
                KeyCode::Char('r') => {
                    self.load_flows();
                    return true;
                },
                
                // Back / Exit
                KeyCode::Esc => {
                    if self.show_step_details {
                        self.show_step_details = false;
                        return true;
                    } else if self.show_flow_details {
                        self.show_flow_details = false;
                        self.selected_step = None;
                        self.step_list_state.select(None);
                        return true;
                    }
                },
                
                _ => {}
            }
        }
        
        false
    }
    
    /// Render the restorative flow UI
    fn render_restorative_flow<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        if self.show_step_details {
            if let Some(step) = self.active_step() {
                self.render_step_details(f, area, step);
            }
        } else if self.show_flow_details {
            if let Some(flow) = self.active_flow() {
                self.render_flow_details(f, area, flow);
            }
        } else {
            self.render_flow_list(f, area);
        }
    }
    
    /// Render the list of flows
    fn render_flow_list<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let items: Vec<ListItem> = self.flows
            .iter()
            .map(|flow| {
                let status_text = match flow.status {
                    RestorativeFlowStatus::Initiated => Span::styled("Initiated", Style::default().fg(Color::Blue)),
                    RestorativeFlowStatus::InProgress => Span::styled("In Progress", Style::default().fg(Color::Yellow)),
                    RestorativeFlowStatus::Completed => Span::styled("Completed", Style::default().fg(Color::Green)),
                    RestorativeFlowStatus::Terminated => Span::styled("Terminated", Style::default().fg(Color::Red)),
                };
                
                let title = Spans::from(vec![
                    Span::raw(format!("{}: ", flow.flow_type.as_str())),
                    Span::styled(&flow.title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]);
                
                let status = Spans::from(vec![
                    Span::raw("  Status: "),
                    status_text,
                ]);
                
                let description = Spans::from(Span::raw(&flow.description));
                
                let progress = Spans::from(vec![
                    Span::raw("  Progress: "),
                    Span::raw(format!(
                        "Step {} of {} ({}%)", 
                        flow.current_step + 1, 
                        flow.steps.len(),
                        (flow.steps.iter().filter(|s| s.completed).count() * 100) / flow.steps.len()
                    )),
                ]);
                
                ListItem::new(vec![title, status, description, progress])
            })
            .collect();
            
        let list_block = if self.flows.is_empty() {
            Block::default()
                .title("Restorative Processes - No active processes")
                .borders(Borders::ALL)
        } else {
            Block::default()
                .title(format!("Restorative Processes - {} process(es)", self.flows.len()))
                .borders(Borders::ALL)
        };
        
        let list = List::new(items)
            .block(list_block)
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");
            
        f.render_stateful_widget(list, area, &mut self.flow_list_state);
        
        // Render footer with keys
        let footer_text = vec![
            Spans::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": View Details  "),
                Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate  "),
                Span::styled("r", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Refresh  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Back/Exit"),
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
    
    /// Render details of a flow
    fn render_flow_details<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, flow: &RestorativeFlowProcess) {
        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6), // Header
                Constraint::Min(0),    // Steps
            ])
            .split(area);
            
        // Render header
        let header_text = vec![
            Spans::from(vec![
                Span::styled(&flow.title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Spans::from(vec![
                Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(flow.flow_type.as_str()),
            ]),
            Spans::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                match flow.status {
                    RestorativeFlowStatus::Initiated => Span::styled("Initiated", Style::default().fg(Color::Blue)),
                    RestorativeFlowStatus::InProgress => Span::styled("In Progress", Style::default().fg(Color::Yellow)),
                    RestorativeFlowStatus::Completed => Span::styled("Completed", Style::default().fg(Color::Green)),
                    RestorativeFlowStatus::Terminated => Span::styled("Terminated", Style::default().fg(Color::Red)),
                },
            ]),
            Spans::from(vec![
                Span::styled("Description: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&flow.description),
            ]),
        ];
        
        let header = Paragraph::new(header_text)
            .block(Block::default().title("Process Details").borders(Borders::ALL));
            
        f.render_widget(header, chunks[0]);
        
        // Render steps
        let step_items: Vec<ListItem> = flow.steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let title = if i == flow.current_step {
                    Spans::from(vec![
                        Span::styled("➤ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("Step {}: {}", i + 1, step.title), 
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    ])
                } else {
                    Spans::from(vec![
                        Span::raw(format!("  Step {}: ", i + 1)),
                        Span::styled(&step.title, Style::default().fg(Color::White)),
                    ])
                };
                
                let status = Spans::from(vec![
                    Span::raw("    Status: "),
                    if step.completed {
                        Span::styled("Completed ✓", Style::default().fg(Color::Green))
                    } else if i < flow.current_step {
                        Span::styled("Skipped", Style::default().fg(Color::Yellow))
                    } else if i == flow.current_step {
                        Span::styled("Current", Style::default().fg(Color::Cyan))
                    } else {
                        Span::styled("Pending", Style::default().fg(Color::Gray))
                    },
                ]);
                
                ListItem::new(vec![title, status])
            })
            .collect();
            
        let steps_list = List::new(step_items)
            .block(Block::default().title("Steps").borders(Borders::ALL))
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");
            
        f.render_stateful_widget(steps_list, chunks[1], &mut self.step_list_state);
        
        // Render footer with keys
        let footer_text = vec![
            Spans::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": View Step Details  "),
                Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate Steps  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Back to List"),
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
    
    /// Render details of a step
    fn render_step_details<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, step: &RestorativeStep) {
        let flow = self.active_flow().unwrap();
        
        // Create content
        let content = vec![
            Spans::from(vec![
                Span::styled(&step.title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Description: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&step.description),
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                if step.completed {
                    Span::styled("Completed ✓", Style::default().fg(Color::Green))
                } else {
                    Span::styled("Pending", Style::default().fg(Color::Yellow))
                },
            ]),
            Spans::from(""),
            if let Some(date) = &step.completion_date {
                Spans::from(vec![
                    Span::styled("Completed on: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(date),
                ])
            } else {
                Spans::from(vec![
                    Span::styled("Required Action: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&step.required_action),
                ])
            },
            Spans::from(""),
            Spans::from(""),
            if !step.completed && self.selected_step == Some(flow.current_step) {
                Spans::from(vec![
                    Span::styled("Press 'c' to complete this step", 
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ])
            } else {
                Spans::from("")
            },
        ];
        
        let block = Block::default()
            .title(format!("Step {} of {}", 
                self.selected_step.map(|i| i + 1).unwrap_or(0), 
                flow.steps.len()))
            .borders(Borders::ALL);
            
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(tui::widgets::Wrap { trim: false });
            
        f.render_widget(paragraph, area);
        
        // Render footer with keys
        let footer_text = vec![
            Spans::from(vec![
                if !step.completed && self.selected_step == Some(flow.current_step) {
                    Span::styled("c", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled(" ", Style::default())
                },
                if !step.completed && self.selected_step == Some(flow.current_step) {
                    Span::raw(": Complete Step  ")
                } else {
                    Span::raw("                 ")
                },
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Back to Process"),
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
}

impl Component for RestorativeFlow {
    fn render<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        self.render_restorative_flow(f, area);
    }
    
    fn handle_input(&mut self, event: Event) -> bool {
        self.handle_input(event)
    }
} 