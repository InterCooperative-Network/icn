use serde::{Deserialize, Serialize};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::credentials::VerifiableCredential;

/// Types of Guardian-related VCs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GuardianCredentialType {
    /// Credential showing participation in a guardian circle
    GuardianParticipation,
    
    /// Credential showing training completion for guardians
    GuardianTraining,
    
    /// Credential for participation in restorative justice
    RestorativeParticipation,
    
    /// Credential for completing educational modules
    EducationalCompletion,
}

/// Guardian participation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianParticipationDetails {
    /// Circle ID the guardian participated in
    pub circle_id: String,
    
    /// Number of cases reviewed
    pub cases_reviewed: u32,
    
    /// Number of votes cast
    pub votes_cast: u32,
    
    /// Participation period start
    pub period_start: String,
    
    /// Participation period end
    pub period_end: String,
}

/// Guardian training details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianTrainingDetails {
    /// Training module name
    pub module_name: String,
    
    /// Training level achieved
    pub level: String,
    
    /// Date of completion
    pub completion_date: String,
    
    /// Training provider
    pub provider: String,
    
    /// Expiration date
    pub expiration_date: Option<String>,
}

/// Restorative participation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorativeParticipationDetails {
    /// Flow ID
    pub flow_id: String,
    
    /// Flow type
    pub flow_type: String,
    
    /// Role in the process
    pub role: String,
    
    /// Start date
    pub start_date: String,
    
    /// Completion date
    pub completion_date: Option<String>,
    
    /// Facilitator DID
    pub facilitator: String,
}

/// Educational completion details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EducationalCompletionDetails {
    /// Module ID
    pub module_id: String,
    
    /// Module name
    pub module_name: String,
    
    /// Completion date
    pub completion_date: String,
    
    /// Score, if applicable
    pub score: Option<u32>,
    
    /// Provider
    pub provider: String,
}

/// A guardian-related verifiable credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianCredential {
    /// Base VC data
    pub credential: VerifiableCredential,
    
    /// Type of guardian credential
    pub credential_type: GuardianCredentialType,
    
    /// Guardian participation details if applicable
    pub participation_details: Option<GuardianParticipationDetails>,
    
    /// Guardian training details if applicable
    pub training_details: Option<GuardianTrainingDetails>,
    
    /// Restorative participation details if applicable
    pub restorative_details: Option<RestorativeParticipationDetails>,
    
    /// Educational completion details if applicable
    pub educational_details: Option<EducationalCompletionDetails>,
}

impl GuardianCredential {
    /// Create a new Guardian Credential from a Verifiable Credential
    pub fn from_vc(vc: VerifiableCredential) -> Option<Self> {
        // Parse VC type and extract guardian-related data
        let credential_type = match vc.get_type() {
            Some(types) => {
                if types.contains(&"GuardianParticipationCredential".to_string()) {
                    GuardianCredentialType::GuardianParticipation
                } else if types.contains(&"GuardianTrainingCredential".to_string()) {
                    GuardianCredentialType::GuardianTraining
                } else if types.contains(&"RestorativeParticipationCredential".to_string()) {
                    GuardianCredentialType::RestorativeParticipation
                } else if types.contains(&"EducationalCompletionCredential".to_string()) {
                    GuardianCredentialType::EducationalCompletion
                } else {
                    return None;
                }
            },
            None => return None,
        };
        
        // Extract credential details from subject
        match credential_type {
            GuardianCredentialType::GuardianParticipation => {
                let participation_details = extract_participation_details(&vc);
                if participation_details.is_none() {
                    return None;
                }
                
                Some(Self {
                    credential: vc,
                    credential_type,
                    participation_details,
                    training_details: None,
                    restorative_details: None,
                    educational_details: None,
                })
            },
            GuardianCredentialType::GuardianTraining => {
                let training_details = extract_training_details(&vc);
                if training_details.is_none() {
                    return None;
                }
                
                Some(Self {
                    credential: vc,
                    credential_type,
                    participation_details: None,
                    training_details,
                    restorative_details: None,
                    educational_details: None,
                })
            },
            GuardianCredentialType::RestorativeParticipation => {
                let restorative_details = extract_restorative_details(&vc);
                if restorative_details.is_none() {
                    return None;
                }
                
                Some(Self {
                    credential: vc,
                    credential_type,
                    participation_details: None,
                    training_details: None,
                    restorative_details,
                    educational_details: None,
                })
            },
            GuardianCredentialType::EducationalCompletion => {
                let educational_details = extract_educational_details(&vc);
                if educational_details.is_none() {
                    return None;
                }
                
                Some(Self {
                    credential: vc,
                    credential_type,
                    participation_details: None,
                    training_details: None,
                    restorative_details: None,
                    educational_details,
                })
            },
        }
    }
    
    /// Render the credential to a TUI
    pub fn render<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let title = match self.credential_type {
            GuardianCredentialType::GuardianParticipation => "Guardian Participation Credential",
            GuardianCredentialType::GuardianTraining => "Guardian Training Credential",
            GuardianCredentialType::RestorativeParticipation => "Restorative Justice Participation",
            GuardianCredentialType::EducationalCompletion => "Educational Module Completion",
        };
        
        let content = match self.credential_type {
            GuardianCredentialType::GuardianParticipation => {
                if let Some(details) = &self.participation_details {
                    render_participation_details(details)
                } else {
                    vec![Spans::from("Error: Missing participation details")]
                }
            },
            GuardianCredentialType::GuardianTraining => {
                if let Some(details) = &self.training_details {
                    render_training_details(details)
                } else {
                    vec![Spans::from("Error: Missing training details")]
                }
            },
            GuardianCredentialType::RestorativeParticipation => {
                if let Some(details) = &self.restorative_details {
                    render_restorative_details(details)
                } else {
                    vec![Spans::from("Error: Missing restorative details")]
                }
            },
            GuardianCredentialType::EducationalCompletion => {
                if let Some(details) = &self.educational_details {
                    render_educational_details(details)
                } else {
                    vec![Spans::from("Error: Missing educational details")]
                }
            },
        };
        
        // Add issuer and issuance date
        let mut all_content = content;
        all_content.push(Spans::from(""));
        
        if let Some(issuer) = self.credential.get_issuer() {
            all_content.push(Spans::from(vec![
                Span::styled("Issuer: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(issuer),
            ]));
        }
        
        if let Some(issue_date) = self.credential.get_issuance_date() {
            all_content.push(Spans::from(vec![
                Span::styled("Issued: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(issue_date),
            ]));
        }
        
        let paragraph = Paragraph::new(all_content)
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(Color::White))
            .wrap(tui::widgets::Wrap { trim: false });
            
        f.render_widget(paragraph, area);
    }
    
    /// Get a summary of the credential
    pub fn get_summary(&self) -> String {
        match self.credential_type {
            GuardianCredentialType::GuardianParticipation => {
                if let Some(details) = &self.participation_details {
                    format!("Guardian Participation: {} (cases reviewed: {})", 
                        details.circle_id, 
                        details.cases_reviewed)
                } else {
                    "Guardian Participation Credential".to_string()
                }
            },
            GuardianCredentialType::GuardianTraining => {
                if let Some(details) = &self.training_details {
                    format!("Guardian Training: {} Level {}", 
                        details.module_name,
                        details.level)
                } else {
                    "Guardian Training Credential".to_string()
                }
            },
            GuardianCredentialType::RestorativeParticipation => {
                if let Some(details) = &self.restorative_details {
                    format!("Restorative Process: {} ({})", 
                        details.flow_type,
                        details.role)
                } else {
                    "Restorative Participation Credential".to_string()
                }
            },
            GuardianCredentialType::EducationalCompletion => {
                if let Some(details) = &self.educational_details {
                    format!("Educational Completion: {}", 
                        details.module_name)
                } else {
                    "Educational Completion Credential".to_string()
                }
            },
        }
    }
}

// Helper functions to extract credential details from VCs

fn extract_participation_details(vc: &VerifiableCredential) -> Option<GuardianParticipationDetails> {
    let subject = vc.get_subject()?;
    
    // This is a simplified mock implementation - in real code, we'd parse the VC structure properly
    Some(GuardianParticipationDetails {
        circle_id: subject.get("circleId").unwrap_or("unknown").to_string(),
        cases_reviewed: subject.get("casesReviewed").unwrap_or("0").parse().unwrap_or(0),
        votes_cast: subject.get("votesCast").unwrap_or("0").parse().unwrap_or(0),
        period_start: subject.get("periodStart").unwrap_or("unknown").to_string(),
        period_end: subject.get("periodEnd").unwrap_or("unknown").to_string(),
    })
}

fn extract_training_details(vc: &VerifiableCredential) -> Option<GuardianTrainingDetails> {
    let subject = vc.get_subject()?;
    
    // Parse expiration date
    let expiration = if subject.contains_key("expirationDate") {
        Some(subject.get("expirationDate").unwrap().to_string())
    } else {
        None
    };
    
    Some(GuardianTrainingDetails {
        module_name: subject.get("moduleName").unwrap_or("unknown").to_string(),
        level: subject.get("level").unwrap_or("unknown").to_string(),
        completion_date: subject.get("completionDate").unwrap_or("unknown").to_string(),
        provider: subject.get("provider").unwrap_or("unknown").to_string(),
        expiration_date: expiration,
    })
}

fn extract_restorative_details(vc: &VerifiableCredential) -> Option<RestorativeParticipationDetails> {
    let subject = vc.get_subject()?;
    
    // Parse completion date
    let completion = if subject.contains_key("completionDate") {
        Some(subject.get("completionDate").unwrap().to_string())
    } else {
        None
    };
    
    Some(RestorativeParticipationDetails {
        flow_id: subject.get("flowId").unwrap_or("unknown").to_string(),
        flow_type: subject.get("flowType").unwrap_or("unknown").to_string(),
        role: subject.get("role").unwrap_or("unknown").to_string(),
        start_date: subject.get("startDate").unwrap_or("unknown").to_string(),
        completion_date: completion,
        facilitator: subject.get("facilitator").unwrap_or("unknown").to_string(),
    })
}

fn extract_educational_details(vc: &VerifiableCredential) -> Option<EducationalCompletionDetails> {
    let subject = vc.get_subject()?;
    
    // Parse score
    let score = if subject.contains_key("score") {
        Some(subject.get("score").unwrap().parse().unwrap_or(0))
    } else {
        None
    };
    
    Some(EducationalCompletionDetails {
        module_id: subject.get("moduleId").unwrap_or("unknown").to_string(),
        module_name: subject.get("moduleName").unwrap_or("unknown").to_string(),
        completion_date: subject.get("completionDate").unwrap_or("unknown").to_string(),
        score,
        provider: subject.get("provider").unwrap_or("unknown").to_string(),
    })
}

// Helper functions to render credential details to TUI spans

fn render_participation_details(details: &GuardianParticipationDetails) -> Vec<Spans> {
    vec![
        Spans::from(vec![
            Span::styled("Circle ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.circle_id),
        ]),
        Spans::from(vec![
            Span::styled("Cases Reviewed: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(details.cases_reviewed.to_string()),
        ]),
        Spans::from(vec![
            Span::styled("Votes Cast: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(details.votes_cast.to_string()),
        ]),
        Spans::from(vec![
            Span::styled("Period: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{} to {}", details.period_start, details.period_end)),
        ]),
    ]
}

fn render_training_details(details: &GuardianTrainingDetails) -> Vec<Spans> {
    let mut spans = vec![
        Spans::from(vec![
            Span::styled("Training Module: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.module_name),
        ]),
        Spans::from(vec![
            Span::styled("Level: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.level),
        ]),
        Spans::from(vec![
            Span::styled("Completed: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.completion_date),
        ]),
        Spans::from(vec![
            Span::styled("Provider: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.provider),
        ]),
    ];
    
    if let Some(expiration) = &details.expiration_date {
        spans.push(Spans::from(vec![
            Span::styled("Expires: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(expiration),
        ]));
    }
    
    spans
}

fn render_restorative_details(details: &RestorativeParticipationDetails) -> Vec<Spans> {
    let mut spans = vec![
        Spans::from(vec![
            Span::styled("Flow ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.flow_id),
        ]),
        Spans::from(vec![
            Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.flow_type),
        ]),
        Spans::from(vec![
            Span::styled("Role: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.role),
        ]),
        Spans::from(vec![
            Span::styled("Started: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.start_date),
        ]),
        Spans::from(vec![
            Span::styled("Facilitator: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.facilitator),
        ]),
    ];
    
    if let Some(completion) = &details.completion_date {
        spans.push(Spans::from(vec![
            Span::styled("Completed: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(completion),
        ]));
    } else {
        spans.push(Spans::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("In Progress", Style::default().fg(Color::Yellow)),
        ]));
    }
    
    spans
}

fn render_educational_details(details: &EducationalCompletionDetails) -> Vec<Spans> {
    let mut spans = vec![
        Spans::from(vec![
            Span::styled("Module: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.module_name),
        ]),
        Spans::from(vec![
            Span::styled("Module ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.module_id),
        ]),
        Spans::from(vec![
            Span::styled("Completed: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.completion_date),
        ]),
        Spans::from(vec![
            Span::styled("Provider: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&details.provider),
        ]),
    ];
    
    if let Some(score) = details.score {
        spans.push(Spans::from(vec![
            Span::styled("Score: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}%", score)),
        ]));
    }
    
    spans
} 