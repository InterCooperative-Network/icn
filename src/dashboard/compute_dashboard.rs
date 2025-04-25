use crate::db::WalletDb;
use crate::repository::{Repository, FederationStats};
use crate::types::TokenBurn;
use crate::ui::{Component, InputEvent, RenderFrame};
use chrono::{DateTime, Duration, Utc, TimeZone};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{
    Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, 
    Table, Row, Cell, Tabs, BarChart, Clear, List, ListItem
};

/// Loading state for async data operations
enum LoadingState {
    /// Not currently loading data
    Idle,
    /// Currently loading data with start time
    Loading(Instant),
    /// Failed to load data with error message
    Error(String),
}

/// Filter options for the time range
enum TimeFilter {
    Day,
    Week,
    Month,
    Year,
    All,
}

impl TimeFilter {
    fn as_duration(&self) -> Option<Duration> {
        match self {
            TimeFilter::Day => Some(Duration::days(1)),
            TimeFilter::Week => Some(Duration::weeks(1)),
            TimeFilter::Month => Some(Duration::days(30)),
            TimeFilter::Year => Some(Duration::days(365)),
            TimeFilter::All => None,
        }
    }
    
    fn as_label(&self) -> &'static str {
        match self {
            TimeFilter::Day => "24 Hours",
            TimeFilter::Week => "7 Days",
            TimeFilter::Month => "30 Days",
            TimeFilter::Year => "Year",
            TimeFilter::All => "All Time",
        }
    }
}

/// Dashboard for compute token usage
pub struct ComputeDashboard {
    db: WalletDb,
    burn_records: Vec<TokenBurn>,
    selected_tab: usize,
    time_filter: TimeFilter,
    active_federation: Option<String>,
    federations: Vec<String>,
    token_types: Vec<String>,
    selected_token_type: Option<String>,
    job_types: Vec<String>,
    active_job_type: Option<String>,
    active_proposal_id: Option<String>,
    loading_state: Arc<Mutex<LoadingState>>,
    show_help: bool,
    federation_reports: HashMap<String, FederationReport>,
}

/// Federation usage report data
#[derive(Debug, Clone)]
struct FederationReport {
    federation_id: String,
    total_tokens_burned: f64,
    avg_daily_usage: f64,
    peak_daily_usage: f64,
    peak_date: Option<DateTime<Utc>>,
    quota_remaining_percent: Option<f64>,
    projected_exhaustion_days: Option<i64>,
}

impl FederationReport {
    fn status_color(&self) -> Color {
        match self.projected_exhaustion_days {
            Some(days) if days < 7 => Color::Red,
            Some(days) if days < 30 => Color::Yellow,
            _ => Color::Green,
        }
    }
}

impl ComputeDashboard {
    /// Create a new compute dashboard
    pub fn new(db: WalletDb) -> Self {
        let loading_state = Arc::new(Mutex::new(LoadingState::Idle));
        
        let mut dashboard = Self {
            db,
            burn_records: Vec::new(),
            selected_tab: 0,
            time_filter: TimeFilter::Month,
            active_federation: None,
            federations: Vec::new(),
            token_types: Vec::new(),
            selected_token_type: None,
            job_types: Vec::new(),
            active_job_type: None,
            active_proposal_id: None,
            loading_state,
            show_help: false,
            federation_reports: HashMap::new(),
        };
        
        dashboard.load_data();
        dashboard
    }
    
    /// Load data from the database with loading indicator
    fn load_data(&mut self) {
        let db_clone = self.db.clone();
        let loading_state_clone = self.loading_state.clone();
        
        // Update the loading state
        {
            let mut lock = loading_state_clone.lock().unwrap();
            *lock = LoadingState::Loading(Instant::now());
        }
        
        // Start loading data in background thread
        thread::spawn(move || {
            // Simulate a small delay to demonstrate loading UI (can be removed in production)
            thread::sleep(std::time::Duration::from_millis(200));
            
            // Attempt to fetch the data
            let burn_records_result = db_clone.get_all_token_burns();
            
            // Update the loading state with the result
            let mut lock = loading_state_clone.lock().unwrap();
            match burn_records_result {
                Ok(records) => {
                    // Store the burn records and reset loading state
                    *lock = LoadingState::Idle;
                    
                    // Send the burn records back to the main thread through a channel
                    // In a real implementation, we would need to update the ComputeDashboard
                    // Here we're simulating completion for simplicity
                }
                Err(err) => {
                    // Store the error
                    *lock = LoadingState::Error(format!("Database error: {}", err));
                }
            }
        });
        
        // Try to fetch the data synchronously for immediate use
        match self.db.get_all_token_burns() {
            Ok(records) => {
                self.burn_records = records;
                
                // Extract unique federations, token types, and job types
                let mut federations = std::collections::HashSet::new();
                let mut token_types = std::collections::HashSet::new();
                let mut job_types = std::collections::HashSet::new();
                
                for burn in &self.burn_records {
                    federations.insert(burn.federation_scope.clone());
                    token_types.insert(burn.token_type.clone());
                    
                    // Collect job types
                    if let Some(job_type) = &burn.job_type {
                        job_types.insert(job_type.clone());
                    } else if let Some(job_id) = &burn.job_id {
                        // Extract prefix as fallback job type
                        if let Some(prefix) = job_id.split('.').next() {
                            job_types.insert(prefix.to_string());
                        }
                    }
                }
                
                self.federations = federations.into_iter().collect();
                self.federations.sort();
                
                self.token_types = token_types.into_iter().collect();
                self.token_types.sort();
                
                self.job_types = job_types.into_iter().collect();
                self.job_types.sort();
                
                // Set default selections if needed
                if self.active_federation.is_none() && !self.federations.is_empty() {
                    self.active_federation = Some(self.federations[0].clone());
                }
                
                if self.selected_token_type.is_none() && !self.token_types.is_empty() {
                    let compute_token = self.token_types.iter()
                        .find(|t| t.contains("compute"))
                        .cloned();
                        
                    self.selected_token_type = compute_token.or_else(|| Some(self.token_types[0].clone()));
                }
                
                // Update federation reports
                self.update_federation_reports();
            },
            Err(err) => {
                // Update loading state with error
                let mut lock = self.loading_state.lock().unwrap();
                *lock = LoadingState::Error(format!("Failed to load data: {}", err));
            }
        }
    }
    
    /// Update federation resource reports based on current data
    fn update_federation_reports(&mut self) {
        // Start loading animation
        *self.loading_state.lock().unwrap() = LoadingState::Loading(Instant::now());
        
        // Clone necessary data for the thread
        let time_filter = self.time_filter.clone();
        let loading_state = self.loading_state.clone();
        
        // Get repository from db
        let repository = Repository::new(self.db.clone());
        let repo_clone = repository.clone();
        
        thread::spawn(move || {
            // Convert time filter to days
            let period_days = match time_filter {
                TimeFilter::Day => Some(1),
                TimeFilter::Week => Some(7),
                TimeFilter::Month => Some(30),
                TimeFilter::Year => Some(365),
                TimeFilter::All => None,
            };
            
            // Fetch federation stats from repository
            let federation_stats = match repo_clone.get_federation_burn_stats(period_days) {
                Ok(stats) => stats,
                Err(e) => {
                    // Handle error
                    *loading_state.lock().unwrap() = LoadingState::Error(format!("Failed to load federation stats: {}", e));
                    return HashMap::new();
                }
            };
            
            // Convert to FederationReport format
            let mut reports = HashMap::new();
            for stats in federation_stats {
                // For now, hardcode quota as 1000 tokens per federation
                // In a real implementation, this would be fetched from federation contracts
                let quota = 1000.0;
                let remaining = quota - stats.total_tokens_burned;
                
                let quota_remaining_percent = if quota > 0.0 {
                    Some(100.0 * remaining / quota)
                } else {
                    None
                };
                
                let projected_exhaustion_days = if stats.avg_daily_burn > 0.0 && remaining > 0.0 {
                    Some((remaining / stats.avg_daily_burn) as i64)
                } else if remaining <= 0.0 {
                    Some(0)
                } else {
                    None
                };
                
                reports.insert(stats.federation_id.clone(), FederationReport {
                    federation_id: stats.federation_id,
                    total_tokens_burned: stats.total_tokens_burned,
                    avg_daily_usage: stats.avg_daily_burn,
                    peak_daily_usage: stats.peak_daily_burn,
                    peak_date: stats.peak_date,
                    quota_remaining_percent,
                    projected_exhaustion_days,
                });
            }
            
            // Update loading state
            *loading_state.lock().unwrap() = LoadingState::Idle;
            
            // Return the reports
            reports
        });
    }
    
    /// Filter burn records based on current settings
    fn filtered_burns(&self) -> Vec<&TokenBurn> {
        let now = Utc::now();
        let cutoff = self.time_filter.as_duration().map(|d| now - d);
        
        self.burn_records.iter()
            .filter(|burn| {
                // Time filter
                let timestamp = DateTime::<Utc>::from_timestamp(burn.timestamp, 0).unwrap_or_default();
                let time_match = match cutoff {
                    Some(cut) => timestamp >= cut,
                    None => true,
                };
                
                // Federation filter
                let federation_match = match &self.active_federation {
                    Some(fed) => burn.federation_scope == *fed,
                    None => true,
                };
                
                // Token type filter
                let type_match = match &self.selected_token_type {
                    Some(t) => burn.token_type == *t,
                    None => true,
                };
                
                // Job type filter
                let job_type_match = match &self.active_job_type {
                    Some(jt) => burn.job_type.as_ref().map_or(false, |t| t == jt),
                    None => true,
                };
                
                // Proposal ID filter
                let proposal_match = match &self.active_proposal_id {
                    Some(pid) => burn.proposal_id.as_ref().map_or(false, |p| p == pid),
                    None => true,
                };
                
                time_match && federation_match && type_match && job_type_match && proposal_match
            })
            .collect()
    }
    
    /// Get burn statistics for the current filters
    fn get_burn_stats(&self) -> (f64, usize, f64) {
        let burns = self.filtered_burns();
        
        let total_amount: f64 = burns.iter().map(|b| b.amount).sum();
        let burn_count = burns.len();
        
        let avg_amount = if burn_count > 0 {
            total_amount / (burn_count as f64)
        } else {
            0.0
        };
        
        (total_amount, burn_count, avg_amount)
    }
    
    /// Get burn data for charting
    fn get_usage_data(&self) -> Vec<(f64, f64)> {
        let burns = self.filtered_burns();
        if burns.is_empty() {
            return vec![];
        }
        
        let mut time_series_data: Vec<(DateTime<Utc>, f64)> = Vec::new();
        
        // Convert to time series
        for burn in burns {
            let dt = DateTime::<Utc>::from_timestamp(burn.timestamp, 0).unwrap_or_default();
            time_series_data.push((dt, burn.amount));
        }
        
        // Sort by timestamp
        time_series_data.sort_by_key(|(dt, _)| *dt);
        
        // Normalize time to chart coordinates (x: 0.0 to 1.0)
        let first_time = time_series_data.first().unwrap().0;
        let last_time = time_series_data.last().unwrap().0;
        
        let total_duration = last_time.signed_duration_since(first_time);
        let total_seconds = total_duration.num_seconds().max(1) as f64;
        
        time_series_data.iter()
            .map(|(dt, amount)| {
                let duration = dt.signed_duration_since(first_time);
                let x = duration.num_seconds() as f64 / total_seconds;
                (x, *amount)
            })
            .collect()
    }
    
    /// Get burn data grouped by job type
    fn get_job_type_distribution(&self) -> Vec<(String, f64)> {
        let burns = self.filtered_burns();
        if burns.is_empty() {
            return vec![];
        }
        
        let mut job_type_totals: HashMap<String, f64> = HashMap::new();
        
        for burn in burns {
            // Use the explicit job_type field if available, otherwise derive from job_id
            let job_type = match &burn.job_type {
                Some(job_type) => job_type.clone(),
                None => burn.job_id
                    .as_ref()
                    .and_then(|id| id.split('.').next())
                    .unwrap_or("unknown")
                    .to_string()
            };
                
            *job_type_totals.entry(job_type).or_insert(0.0) += burn.amount;
        }
        
        let mut result: Vec<(String, f64)> = job_type_totals
            .into_iter()
            .collect();
            
        // Sort by amount (descending)
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Limit to top 5 for display clarity
        if result.len() > 5 {
            let other_sum: f64 = result[5..].iter().map(|(_, v)| *v).sum();
            result.truncate(5);
            if other_sum > 0.0 {
                result.push(("Other".to_string(), other_sum));
            }
        }
        
        result
    }
    
    /// Get burn data grouped by day for the last week/month
    fn get_daily_usage(&self) -> Vec<(String, f64)> {
        let burns = self.filtered_burns();
        if burns.is_empty() {
            return vec![];
        }
        
        let now = Utc::now();
        let mut daily_totals: HashMap<String, f64> = HashMap::new();
        
        // Number of days to include based on filter
        let days = match self.time_filter {
            TimeFilter::Day => 24, // For day filter, we'll show hourly data
            TimeFilter::Week => 7,
            TimeFilter::Month => 30,
            _ => 30, // Default to 30 days
        };
        
        // Initialize all days with zero (to show gaps properly)
        for i in 0..days {
            let date = match self.time_filter {
                TimeFilter::Day => {
                    // For day filter, use hours
                    let dt = now - Duration::hours(i);
                    dt.format("%H:00").to_string()
                },
                _ => {
                    // Otherwise use dates
                    let dt = now - Duration::days(i);
                    dt.format("%m/%d").to_string()
                }
            };
            daily_totals.insert(date, 0.0);
        }
        
        // Aggregate burn amounts by day
        for burn in burns {
            let dt = DateTime::<Utc>::from_timestamp(burn.timestamp, 0).unwrap_or_default();
            let key = match self.time_filter {
                TimeFilter::Day => dt.format("%H:00").to_string(),
                _ => dt.format("%m/%d").to_string(),
            };
            
            if let Some(total) = daily_totals.get_mut(&key) {
                *total += burn.amount;
            }
        }
        
        // Convert to vector and sort by date
        let mut result: Vec<(String, f64)> = daily_totals.into_iter().collect();
        
        // For day view (hourly data), sort by hour
        if self.time_filter == TimeFilter::Day {
            result.sort_by(|a, b| {
                let a_hour = a.0.split(':').next().unwrap_or("0").parse::<i32>().unwrap_or(0);
                let b_hour = b.0.split(':').next().unwrap_or("0").parse::<i32>().unwrap_or(0);
                a_hour.cmp(&b_hour)
            });
        } else {
            // For other views, sort by date
            result.sort_by(|a, b| {
                let a_parts: Vec<&str> = a.0.split('/').collect();
                let b_parts: Vec<&str> = b.0.split('/').collect();
                
                if a_parts.len() == 2 && b_parts.len() == 2 {
                    let a_month = a_parts[0].parse::<i32>().unwrap_or(0);
                    let b_month = b_parts[0].parse::<i32>().unwrap_or(0);
                    
                    let a_day = a_parts[1].parse::<i32>().unwrap_or(0);
                    let b_day = b_parts[1].parse::<i32>().unwrap_or(0);
                    
                    if a_month != b_month {
                        a_month.cmp(&b_month)
                    } else {
                        a_day.cmp(&b_day)
                    }
                } else {
                    a.0.cmp(&b.0)
                }
            });
        }
        
        result
    }
    
    /// Render the overview tab with enhanced visualizations
    fn render_overview_tab(&self, frame: &mut RenderFrame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Filters
                Constraint::Length(8),  // Stats
                Constraint::Min(8),     // Usage Chart
                Constraint::Length(10), // Job Type Distribution
            ].as_ref())
            .split(area);
            
        // Render filters
        let filter_text = format!(
            "Federation: {:?} | Time: {} | Token: {:?} | Job Type: {:?} | Proposal: {:?}",
            self.active_federation.as_deref().unwrap_or("All"),
            self.time_filter.as_label(),
            self.selected_token_type.as_deref().unwrap_or("All"),
            self.active_job_type.as_deref().unwrap_or("All"),
            self.active_proposal_id.as_deref().map(|p| if p.len() > 8 { 
                format!("{}...", &p[0..8]) 
            } else { 
                p.to_string() 
            }).unwrap_or_else(|| "None".to_string())
        );
        
        let filter_para = Paragraph::new(filter_text)
            .block(Block::default().title("Filters").borders(Borders::ALL));
            
        frame.render_widget(filter_para, chunks[0]);
        
        // Render stats
        let (total_amount, burn_count, avg_amount) = self.get_burn_stats();
        
        let stats_text = vec![
            Spans::from(vec![
                Span::raw("Total consumption: "),
                Span::styled(
                    format!("{:.2}", total_amount),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                ),
            ]),
            Spans::from(vec![
                Span::raw("Burn operations: "),
                Span::styled(
                    burn_count.to_string(),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                ),
            ]),
            Spans::from(vec![
                Span::raw("Average per operation: "),
                Span::styled(
                    format!("{:.2}", avg_amount),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                ),
            ]),
        ];
        
        let stats_para = Paragraph::new(stats_text)
            .block(Block::default().title("Statistics").borders(Borders::ALL));
            
        frame.render_widget(stats_para, chunks[1]);
        
        // Render daily usage chart
        let daily_data = self.get_daily_usage();
        
        if daily_data.is_empty() {
            let empty_para = Paragraph::new("No data available for the selected filters.")
                .block(Block::default().title("Usage Over Time").borders(Borders::ALL));
            frame.render_widget(empty_para, chunks[2]);
        } else {
            let data_labels: Vec<String> = daily_data.iter().map(|(label, _)| label.clone()).collect();
            let data_values: Vec<u64> = daily_data.iter().map(|(_, value)| *value as u64).collect();
            
            let barchart = BarChart::default()
                .block(Block::default().title("Usage Over Time").borders(Borders::ALL))
                .data(&data_labels, &data_values)
                .bar_width(if daily_data.len() > 15 { 1 } else { 3 })
                .bar_gap(if daily_data.len() > 15 { 0 } else { 1 })
                .bar_style(Style::default().fg(Color::Cyan))
                .value_style(Style::default().fg(Color::Black).bg(Color::Cyan))
                .label_style(Style::default().fg(Color::Gray));
                
            frame.render_widget(barchart, chunks[2]);
        }
        
        // Render job type distribution
        let job_distribution = self.get_job_type_distribution();
        
        if job_distribution.is_empty() {
            let empty_para = Paragraph::new("No job type data available.")
                .block(Block::default().title("Resource Usage by Job Type").borders(Borders::ALL));
            frame.render_widget(empty_para, chunks[3]);
        } else {
            let job_labels: Vec<String> = job_distribution.iter().map(|(label, _)| label.clone()).collect();
            let job_values: Vec<u64> = job_distribution.iter().map(|(_, value)| *value as u64).collect();
            
            let job_chart = BarChart::default()
                .block(Block::default().title("Resource Usage by Job Type").borders(Borders::ALL))
                .data(&job_labels, &job_values)
                .bar_width(7)
                .bar_style(Style::default().fg(Color::Green))
                .value_style(Style::default().fg(Color::Black).bg(Color::Green))
                .label_style(Style::default().fg(Color::Gray));
                
            frame.render_widget(job_chart, chunks[3]);
        }
    }
    
    /// Render the burn history tab
    fn render_history_tab(&self, frame: &mut RenderFrame, area: Rect) {
        let burns = self.filtered_burns();
        
        if burns.is_empty() {
            let empty_para = Paragraph::new("No token burn records found with the current filters.")
                .block(Block::default().title("Burn History").borders(Borders::ALL));
                
            frame.render_widget(empty_para, area);
            return;
        }
        
        // Create rows for the table
        let rows = burns.iter().map(|burn| {
            let timestamp = DateTime::<Utc>::from_timestamp(burn.timestamp, 0)
                .unwrap_or_default()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
                
            let reason = burn.reason.as_deref().unwrap_or("-");
            let job_id = burn.job_id.as_deref().unwrap_or("-");
            let job_type = burn.job_type.as_deref().unwrap_or("-");
            let proposal_id = burn.proposal_id.as_deref().unwrap_or("-");
            
            Row::new(vec![
                Cell::from(timestamp),
                Cell::from(burn.token_type.clone()),
                Cell::from(format!("{:.2}", burn.amount)),
                Cell::from(burn.federation_scope.clone()),
                Cell::from(job_id.to_string()),
                Cell::from(job_type.to_string()),
                Cell::from(proposal_id.to_string()),
                Cell::from(reason.to_string()),
            ])
        }).collect::<Vec<_>>();
        
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Timestamp").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Token Type").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Amount").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Federation").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Job ID").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Job Type").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Proposal ID").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Reason").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[
                Constraint::Percentage(15),
                Constraint::Percentage(10),
                Constraint::Percentage(8),
                Constraint::Percentage(12),
                Constraint::Percentage(15),
                Constraint::Percentage(10),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
            ])
            .block(Block::default().title("Burn History").borders(Borders::ALL));
            
        frame.render_widget(table, area);
    }
    
    /// Render federation comparison tab
    fn render_federation_tab(&self, frame: &mut RenderFrame, area: Rect) {
        // Group data by federation
        let burns = self.filtered_burns();
        let mut by_federation: HashMap<String, f64> = HashMap::new();
        
        for burn in &burns {
            *by_federation.entry(burn.federation_scope.clone()).or_default() += burn.amount;
        }
        
        if by_federation.is_empty() {
            let empty_para = Paragraph::new("No data available for the selected filters.")
                .block(Block::default().title("Federation Comparison").borders(Borders::ALL));
                
            frame.render_widget(empty_para, area);
            return;
        }
        
        // Create data for bar chart
        let mut federation_names: Vec<String> = by_federation.keys().cloned().collect();
        federation_names.sort();
        
        let data: Vec<(&str, u64)> = federation_names.iter()
            .map(|name| {
                let amount = by_federation.get(name).unwrap_or(&0.0);
                // Convert to integer for the bar chart
                (name.as_str(), *amount as u64)
            })
            .collect();
            
        // Find max for scaling
        let max_value = data.iter().map(|(_, v)| *v).max().unwrap_or(1);
        
        let barchart = BarChart::default()
            .block(Block::default().title("Federation Comparison").borders(Borders::ALL))
            .data(&data)
            .bar_width(9)
            .bar_gap(3)
            .bar_style(Style::default().fg(Color::Yellow))
            .value_style(Style::default().fg(Color::Black).bg(Color::Yellow))
            .label_style(Style::default().fg(Color::White));
            
        frame.render_widget(barchart, area);
    }

    /// Render a help modal over the current view
    fn render_help_modal(&self, frame: &mut RenderFrame, area: Rect) {
        let modal_width = 60;
        let modal_height = 20;
        
        // Calculate centered position
        let modal_x = (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = (area.height.saturating_sub(modal_height)) / 2;
        
        let modal_area = Rect {
            x: area.x + modal_x,
            y: area.y + modal_y,
            width: modal_width,
            height: modal_height,
        };
        
        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);
        
        // Render the modal
        let help_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray))
            .title(Spans::from(vec![
                Span::styled("Help ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled("(Press Esc to close)", Style::default().fg(Color::Gray)),
            ]));
        
        frame.render_widget(help_block, modal_area);
        
        // Create help content
        let help_items = vec![
            Spans::from(Span::styled("Compute Dashboard Navigation", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch between dashboard tabs"),
            ]),
            Spans::from(vec![
                Span::styled("←/→", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate dashboard tabs"),
            ]),
            Spans::from(vec![
                Span::styled("t", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Cycle through time filters (Day, Week, Month, Year, All)"),
            ]),
            Spans::from(vec![
                Span::styled("f", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Cycle through federation filters"),
            ]),
            Spans::from(vec![
                Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Cycle through token types"),
            ]),
            Spans::from(vec![
                Span::styled("r", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Reload data from database"),
            ]),
            Spans::from(vec![
                Span::styled("d", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch to daily view"),
            ]),
            Spans::from(vec![
                Span::styled("w", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch to weekly view"),
            ]),
            Spans::from(vec![
                Span::styled("m", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch to monthly view"),
            ]),
            Spans::from(vec![
                Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch to yearly view"),
            ]),
            Spans::from(vec![
                Span::styled("a", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Switch to all-time view"),
            ]),
            Spans::from(vec![
                Span::styled("?", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Show/hide this help menu"),
            ]),
            Spans::from(vec![
                Span::styled("Esc", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Close help or return to normal mode"),
            ]),
            Spans::from(""),
            Spans::from(Span::styled("Federation Report Colors:", Style::default().fg(Color::Yellow))),
            Spans::from(vec![
                Span::styled("Green", Style::default().fg(Color::Green)),
                Span::raw(": >30 days remaining"),
            ]),
            Spans::from(vec![
                Span::styled("Yellow", Style::default().fg(Color::Yellow)),
                Span::raw(": 10-30 days remaining"),
            ]),
            Spans::from(vec![
                Span::styled("Red", Style::default().fg(Color::Red)),
                Span::raw(": <10 days remaining"),
            ]),
        ];
        
        let help_paragraph = Paragraph::new(help_items)
            .alignment(tui::layout::Alignment::Left)
            .wrap(tui::widgets::Wrap { trim: true });
        
        let inner_area = modal_area.inner(&Margin { 
            vertical: 1, 
            horizontal: 2 
        });
        
        frame.render_widget(help_paragraph, inner_area);
    }
    
    /// Render loading indicator or error message
    fn render_loading_state(&self, frame: &mut RenderFrame, area: Rect) {
        let loading_state = self.loading_state.lock().unwrap();
        
        match &*loading_state {
            LoadingState::Loading(start_time) => {
                // Only show loading indicator if it's been loading for at least 100ms
                if start_time.elapsed().as_millis() > 100 {
                    let elapsed_secs = start_time.elapsed().as_secs();
                    let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                    let spinner_char = spinner_chars[(elapsed_secs % 10) as usize];
                    
                    let loading_text = format!("{} Loading data...", spinner_char);
                    let loading_paragraph = Paragraph::new(loading_text)
                        .style(Style::default().fg(Color::Yellow))
                        .alignment(tui::layout::Alignment::Center)
                        .block(Block::default().borders(Borders::NONE));
                    
                    let loading_area = Rect {
                        x: area.x + 2,
                        y: area.y + 1,
                        width: area.width - 4,
                        height: 1,
                    };
                    
                    frame.render_widget(loading_paragraph, loading_area);
                }
            },
            LoadingState::Error(error) => {
                let error_text = format!("⚠ Error: {}", error);
                let error_paragraph = Paragraph::new(error_text)
                    .style(Style::default().fg(Color::Red))
                    .alignment(tui::layout::Alignment::Center)
                    .block(Block::default().borders(Borders::NONE));
                
                let error_area = Rect {
                    x: area.x + 2,
                    y: area.y + 1,
                    width: area.width - 4,
                    height: 1,
                };
                
                frame.render_widget(error_paragraph, error_area);
            },
            _ => {} // No loading indicator for Idle state
        }
    }
    
    /// Render the federation report tab
    fn render_federation_report_tab(&mut self, frame: &mut RenderFrame, area: Rect) {
        // Check if we need to fetch data
        if self.federation_reports.is_empty() {
            self.update_federation_reports();
        }
        
        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),    // Time filter
                Constraint::Length(5),    // Instructions
                Constraint::Percentage(80), // Federation list
            ].as_ref())
            .split(area);
        
        // Render time filter
        let time_text = format!("Time Range: {}", self.time_filter.as_label());
        let filter_para = Paragraph::new(time_text)
            .block(Block::default().title("Filter").borders(Borders::ALL));
        frame.render_widget(filter_para, chunks[0]);
        
        // Render instructions
        let instructions = vec![
            Spans::from(vec![
                Span::raw("Use "),
                Span::styled("t", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" to change time filter, "),
                Span::styled("r", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" to refresh data, "),
                Span::styled("?", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" for help"),
            ]),
            Spans::from(vec![
                Span::styled("Green", Style::default().fg(Color::Green)),
                Span::raw(": >30 days remaining, "),
                Span::styled("Yellow", Style::default().fg(Color::Yellow)),
                Span::raw(": 7-30 days remaining, "),
                Span::styled("Red", Style::default().fg(Color::Red)),
                Span::raw(": <7 days remaining"),
            ]),
        ];
        
        let instructions_para = Paragraph::new(instructions)
            .block(Block::default().title("Instructions").borders(Borders::ALL));
        frame.render_widget(instructions_para, chunks[1]);
        
        // Render federation list table
        let mut rows = Vec::new();
        
        for report in self.federation_reports.values() {
            let exhaustion_text = match report.projected_exhaustion_days {
                Some(days) if days > 0 => format!("{} days", days),
                Some(_) => "Exhausted!".to_string(),
                None => "N/A".to_string(),
            };
            
            let remaining_text = match report.quota_remaining_percent {
                Some(pct) if pct > 0.0 => format!("{:.1}%", pct),
                Some(_) => "0%".to_string(),
                None => "N/A".to_string(),
            };
            
            let peak_text = if let Some(date) = &report.peak_date {
                format!("{:.2} on {}", 
                    report.peak_daily_usage, 
                    date.format("%Y-%m-%d"))
            } else {
                format!("{:.2}", report.peak_daily_usage)
            };
            
            rows.push(Row::new(vec![
                Cell::from(report.federation_id.clone()),
                Cell::from(format!("{:.2}", report.total_tokens_burned)),
                Cell::from(format!("{:.2}", report.avg_daily_usage)),
                Cell::from(peak_text),
                Cell::from(remaining_text),
                Cell::from(Span::styled(
                    exhaustion_text,
                    Style::default().fg(report.status_color())
                )),
            ]));
        }
        
        let table = Table::new(rows)
            .header(
                Row::new(vec![
                    Cell::from("Federation ID").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Total Burn").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Avg Daily").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Peak Usage").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Quota Remaining").style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from("Projected Exhaustion").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[
                Constraint::Percentage(20),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
                Constraint::Percentage(20),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
            ])
            .block(Block::default().title("Federation Resource Usage Report").borders(Borders::ALL));
            
        frame.render_widget(table, chunks[2]);
    }
}

impl Component for ComputeDashboard {
    fn render(&mut self, frame: &mut RenderFrame, area: Rect) {
        // Re-load data before rendering to ensure latest data
        // Only reload if not currently loading
        if let LoadingState::Idle = *self.loading_state.lock().unwrap() {
            // Periodically reload data (e.g., every minute)
            // In a real app, you might want to check elapsed time since last reload
        }
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Tabs
                Constraint::Min(10),    // Content
            ].as_ref())
            .split(area);
            
        // Render tabs
        let tab_titles = vec!["Overview", "History", "Federation Breakdown", "Federation Report"];
        let tabs = Tabs::new(
            tab_titles.iter().map(|t| Spans::from(Span::raw(*t))).collect()
        )
        .block(Block::default().title("Compute Resource Token Dashboard").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .select(self.selected_tab);
        
        frame.render_widget(tabs, chunks[0]);
        
        // Render selected tab content
        match self.selected_tab {
            0 => self.render_overview_tab(frame, chunks[1]),
            1 => self.render_history_tab(frame, chunks[1]),
            2 => self.render_federation_tab(frame, chunks[1]),
            3 => self.render_federation_report_tab(frame, chunks[1]),
            _ => {}
        }
        
        // Render loading state
        self.render_loading_state(frame, area);
        
        // Render help modal if needed
        if self.show_help {
            self.render_help_modal(frame, area);
        }
    }

    fn handle_input(&mut self, event: InputEvent) -> bool {
        // If help is shown, only handle Escape key
        if self.show_help {
            match event {
                InputEvent::KeyEsc => {
                    self.show_help = false;
                    return true;
                },
                _ => return true, // Consume all inputs when help is shown
            }
        }
        
        match event {
            InputEvent::KeyTab => {
                // Switch tab
                self.selected_tab = (self.selected_tab + 1) % 4; // Updated for 4 tabs
                true
            }
            InputEvent::KeyBacktab => {
                // Switch tab backwards
                self.selected_tab = (self.selected_tab + 3) % 4; // Updated for 4 tabs
                true
            }
            InputEvent::KeyChar('r') => {
                // Reload data
                self.load_data();
                if self.selected_tab == 3 {
                    // Clear and reload federation reports
                    self.federation_reports.clear();
                    self.update_federation_reports();
                }
                true
            }
            InputEvent::KeyChar('t') => {
                // Cycle through time filters
                self.time_filter = match self.time_filter {
                    TimeFilter::Day => TimeFilter::Week,
                    TimeFilter::Week => TimeFilter::Month,
                    TimeFilter::Month => TimeFilter::Year,
                    TimeFilter::Year => TimeFilter::All,
                    TimeFilter::All => TimeFilter::Day,
                };
                
                // Update federation reports after changing filter
                self.update_federation_reports();
                true
            }
            InputEvent::KeyChar('f') => {
                // Cycle through federations
                if self.federations.is_empty() {
                    return true;
                }
                
                match &self.active_federation {
                    None => {
                        self.active_federation = Some(self.federations[0].clone());
                    }
                    Some(current) => {
                        let pos = self.federations.iter().position(|f| f == current);
                        match pos {
                            Some(i) if i < self.federations.len() - 1 => {
                                self.active_federation = Some(self.federations[i + 1].clone());
                            }
                            _ => {
                                self.active_federation = None;
                            }
                        }
                    }
                }
                true
            }
            InputEvent::KeyChar('y') => {
                // Cycle through token types
                if self.token_types.is_empty() {
                    return true;
                }
                
                match &self.selected_token_type {
                    None => {
                        self.selected_token_type = Some(self.token_types[0].clone());
                    }
                    Some(current) => {
                        let pos = self.token_types.iter().position(|t| t == current);
                        match pos {
                            Some(i) if i < self.token_types.len() - 1 => {
                                self.selected_token_type = Some(self.token_types[i + 1].clone());
                            }
                            _ => {
                                self.selected_token_type = None;
                            }
                        }
                    }
                }
                
                // Update federation reports after changing token type
                self.update_federation_reports();
                true
            }
            InputEvent::KeyChar('j') => {
                // Cycle through job types
                if self.job_types.is_empty() {
                    return true;
                }
                
                match &self.active_job_type {
                    None => {
                        self.active_job_type = Some(self.job_types[0].clone());
                    }
                    Some(current) => {
                        let pos = self.job_types.iter().position(|j| j == current);
                        match pos {
                            Some(i) if i < self.job_types.len() - 1 => {
                                self.active_job_type = Some(self.job_types[i + 1].clone());
                            }
                            _ => {
                                self.active_job_type = None;
                            }
                        }
                    }
                }
                true
            }
            InputEvent::KeyChar('p') => {
                // Clear proposal filter (toggle)
                self.active_proposal_id = match &self.active_proposal_id {
                    Some(_) => None,
                    None => {
                        // Find first proposal ID in records
                        let proposal = self.burn_records.iter()
                            .find_map(|burn| burn.proposal_id.clone());
                        proposal
                    }
                };
                true
            }
            InputEvent::KeyChar('?') => {
                // Toggle help menu
                self.show_help = !self.show_help;
                true
            }
            InputEvent::KeyChar('d') => {
                // Switch to daily view
                self.time_filter = TimeFilter::Day;
                self.update_federation_reports();
                true
            }
            InputEvent::KeyChar('w') => {
                // Switch to weekly view
                self.time_filter = TimeFilter::Week;
                self.update_federation_reports();
                true
            }
            InputEvent::KeyChar('m') => {
                // Switch to monthly view
                self.time_filter = TimeFilter::Month;
                self.update_federation_reports();
                true
            }
            InputEvent::KeyChar('a') => {
                // Switch to all time view
                self.time_filter = TimeFilter::All;
                self.update_federation_reports();
                true
            }
            InputEvent::KeyChar('c') => {
                // Clear all filters
                self.active_federation = None;
                self.selected_token_type = None;
                self.active_job_type = None;
                self.active_proposal_id = None;
                true
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "Compute Usage Dashboard"
    }

    fn help_text(&self) -> Vec<Spans> {
        vec![
            Spans::from(Span::raw("Tab: Switch view")),
            Spans::from(Span::raw("← →: Navigate tabs")),
            Spans::from(Span::raw("r: Reload data")),
            Spans::from(Span::raw("f: Change federation filter")),
            Spans::from(Span::raw("t: Change token type filter")),
            Spans::from(Span::raw("j: Change job type filter")),
            Spans::from(Span::raw("p: Toggle proposal filter")),
            Spans::from(Span::raw("c: Clear all filters")),
            Spans::from(Span::raw("d: Daily view")),
            Spans::from(Span::raw("w: Weekly view")),
            Spans::from(Span::raw("m: Monthly view")),
            Spans::from(Span::raw("y: Yearly view")),
            Spans::from(Span::raw("a: All time view")),
            Spans::from(Span::raw("?: Show help")),
        ]
    }
} 