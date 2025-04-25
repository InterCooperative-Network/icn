use crate::db::WalletDb;
use crate::types::TokenBurn;
use crate::ui::{Component, InputEvent, RenderFrame};
use chrono::{DateTime, Duration, Utc, TimeZone};
use std::collections::HashMap;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{
    Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, 
    Table, Row, Cell, Tabs, BarChart
};

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
}

impl ComputeDashboard {
    /// Create a new compute dashboard
    pub fn new(db: WalletDb) -> Self {
        let mut dashboard = Self {
            db,
            burn_records: Vec::new(),
            selected_tab: 0,
            time_filter: TimeFilter::Month,
            active_federation: None,
            federations: Vec::new(),
            token_types: Vec::new(),
            selected_token_type: None,
        };
        
        dashboard.load_data();
        dashboard
    }
    
    /// Load data from the database
    fn load_data(&mut self) {
        // Get all token burns
        self.burn_records = self.db.get_all_token_burns().unwrap_or_default();
        
        // Extract unique federations and token types
        let mut federations = std::collections::HashSet::new();
        let mut token_types = std::collections::HashSet::new();
        
        for burn in &self.burn_records {
            federations.insert(burn.federation_scope.clone());
            token_types.insert(burn.token_type.clone());
        }
        
        self.federations = federations.into_iter().collect();
        self.federations.sort();
        
        self.token_types = token_types.into_iter().collect();
        self.token_types.sort();
        
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
                
                time_match && federation_match && type_match
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
            let job_type = burn.job_id
                .split('.')
                .next()
                .unwrap_or("unknown")
                .to_string();
                
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
            "Federation: {:?} | Time: {} | Token: {:?}",
            self.active_federation.as_deref().unwrap_or("All"),
            self.time_filter.as_label(),
            self.selected_token_type.as_deref().unwrap_or("All")
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
            
            Row::new(vec![
                Cell::from(timestamp),
                Cell::from(burn.token_type.clone()),
                Cell::from(format!("{:.2}", burn.amount)),
                Cell::from(burn.federation_scope.clone()),
                Cell::from(job_id.to_string()),
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
                    Cell::from("Reason").style(Style::default().add_modifier(Modifier::BOLD)),
                ])
            )
            .widths(&[
                Constraint::Percentage(20),
                Constraint::Percentage(15),
                Constraint::Percentage(10),
                Constraint::Percentage(15),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
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
}

impl Component for ComputeDashboard {
    fn render(&mut self, frame: &mut RenderFrame, area: Rect) {
        // Re-load data before rendering to ensure latest data
        self.load_data();
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Tabs
                Constraint::Min(10),    // Content
            ].as_ref())
            .split(area);
            
        // Render tabs
        let tab_titles = vec!["Overview", "History", "Federation Breakdown"];
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
            _ => {}
        }
    }

    fn handle_input(&mut self, event: InputEvent) -> bool {
        match event {
            InputEvent::KeyTab => {
                // Switch tab
                self.selected_tab = (self.selected_tab + 1) % 3;
                true
            }
            InputEvent::KeyBacktab => {
                // Switch tab backwards
                self.selected_tab = (self.selected_tab + 2) % 3;
                true
            }
            InputEvent::KeyChar('r') => {
                // Reload data
                self.load_data();
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
            Spans::from(Span::raw("t: Change token type filter")),
            Spans::from(Span::raw("f: Change federation filter")),
            Spans::from(Span::raw("d: Daily view")),
            Spans::from(Span::raw("w: Weekly view")),
            Spans::from(Span::raw("m: Monthly view")),
            Spans::from(Span::raw("y: Yearly view")),
            Spans::from(Span::raw("a: All time view")),
        ]
    }
} 