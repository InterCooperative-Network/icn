//! Email Notification Client
//!
//! Implements email notification delivery via SMTP.
//! Supports HTML and plain text templates for various notification types.

use std::collections::HashMap;
use tracing::{debug, info};

use crate::notification_queue::NotificationType;

/// SMTP configuration
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    /// SMTP server host
    pub host: String,
    /// SMTP server port
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Sender email address
    pub from_address: String,
    /// Sender name
    pub from_name: String,
    /// Use TLS
    pub use_tls: bool,
    /// Use STARTTLS
    pub use_starttls: bool,
}

impl SmtpConfig {
    /// Create a new SMTP configuration
    pub fn new(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
        from_address: impl Into<String>,
        from_name: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            password: password.into(),
            from_address: from_address.into(),
            from_name: from_name.into(),
            use_tls: port == 465,
            use_starttls: port == 587,
        }
    }

    /// Create configuration for common providers
    pub fn sendgrid(
        api_key: impl Into<String>,
        from_address: impl Into<String>,
        from_name: impl Into<String>,
    ) -> Self {
        Self::new(
            "smtp.sendgrid.net",
            587,
            "apikey",
            api_key,
            from_address,
            from_name,
        )
    }

    pub fn mailgun(api_key: impl Into<String>, domain: &str, from_name: impl Into<String>) -> Self {
        Self::new(
            "smtp.mailgun.org",
            587,
            "api",
            api_key,
            format!("notifications@{domain}"),
            from_name,
        )
    }
}

/// Email message
#[derive(Debug, Clone)]
pub struct EmailMessage {
    /// Recipient email address
    pub to: String,
    /// Email subject
    pub subject: String,
    /// Plain text body
    pub text_body: String,
    /// HTML body (optional)
    pub html_body: Option<String>,
    /// Reply-to address
    pub reply_to: Option<String>,
    /// Custom headers
    pub headers: HashMap<String, String>,
}

impl EmailMessage {
    /// Create a new email message
    pub fn new(
        to: impl Into<String>,
        subject: impl Into<String>,
        text_body: impl Into<String>,
    ) -> Self {
        Self {
            to: to.into(),
            subject: subject.into(),
            text_body: text_body.into(),
            html_body: None,
            reply_to: None,
            headers: HashMap::new(),
        }
    }

    /// Add HTML body
    pub fn with_html(mut self, html_body: impl Into<String>) -> Self {
        self.html_body = Some(html_body.into());
        self
    }

    /// Set reply-to address
    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    /// Add custom header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

/// Email delivery result
#[derive(Debug)]
pub enum EmailResult {
    /// Successfully sent
    Success { message_id: String },
    /// Invalid recipient
    InvalidRecipient { error: String },
    /// Temporary failure (should retry)
    TemporaryFailure { error: String },
    /// Permanent failure (should not retry)
    PermanentFailure { error: String },
}

/// Email client for sending notifications
pub struct EmailClient {
    config: SmtpConfig,
}

impl EmailClient {
    /// Create a new email client
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    /// Send an email
    ///
    /// Note: This is a placeholder implementation. In production, integrate with:
    /// - lettre crate for SMTP
    /// - reqwest for API-based providers (SendGrid, Mailgun, SES)
    pub async fn send(&self, message: EmailMessage) -> EmailResult {
        // Log the email for now (actual SMTP implementation would go here)
        info!(
            to = %message.to,
            subject = %message.subject,
            from = %self.config.from_address,
            "Sending email notification"
        );

        // In a production system, we would:
        // 1. Connect to SMTP server
        // 2. Authenticate
        // 3. Send the message
        // 4. Handle errors

        // For now, return success to allow testing of the flow
        debug!(
            to = %message.to,
            "Email would be sent (SMTP implementation pending)"
        );

        EmailResult::Success {
            message_id: format!("mock-{}", uuid::Uuid::new_v4()),
        }
    }

    /// Send an email using HTTP API (for API-based providers)
    #[allow(dead_code)]
    async fn send_via_api(&self, _message: EmailMessage) -> EmailResult {
        // This would be implemented for providers like SendGrid, Mailgun, etc.
        EmailResult::PermanentFailure {
            error: "API-based email sending not implemented".to_string(),
        }
    }
}

// ============================================================================
// Email Templates
// ============================================================================

/// Email template configuration
pub struct EmailTemplates {
    /// Base URL for links in emails
    pub base_url: String,
    /// Cooperative name for branding
    pub coop_name: String,
}

impl EmailTemplates {
    /// Create new email templates configuration
    pub fn new(base_url: impl Into<String>, coop_name: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            coop_name: coop_name.into(),
        }
    }

    /// Generate email for a notification
    pub fn generate_email(
        &self,
        to: &str,
        notification_type: NotificationType,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> EmailMessage {
        let (subject, text_body, html_body) = match notification_type {
            NotificationType::PaymentReceived => self.payment_received_template(title, body, data),
            NotificationType::PaymentSent => self.payment_sent_template(title, body, data),
            NotificationType::ProposalCreated => self.proposal_template(title, body, data),
            NotificationType::ProposalClosing => self.proposal_closing_template(title, body, data),
            NotificationType::VoteRecorded => self.vote_recorded_template(title, body, data),
            NotificationType::ProposalResult => self.proposal_result_template(title, body, data),
            NotificationType::MembershipChange => self.membership_template(title, body, data),
            NotificationType::AmendmentProposed => self.amendment_template(title, body, data),
            NotificationType::AmendmentResult => self.amendment_result_template(title, body, data),
            NotificationType::AppealFiled => self.appeal_filed_template(title, body, data),
            NotificationType::AppealDecision => self.appeal_decision_template(title, body, data),
            NotificationType::BalanceChange => self.balance_template(title, body, data),
            NotificationType::SecurityAlert => self.security_alert_template(title, body, data),
            NotificationType::System => self.system_template(title, body, data),
        };

        EmailMessage::new(to, subject, text_body).with_html(html_body)
    }

    fn payment_received_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let amount = data
            .and_then(|d| d.get("amount"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency = data
            .and_then(|d| d.get("currency"))
            .and_then(|v| v.as_str())
            .unwrap_or("credits");
        let from = data
            .and_then(|d| d.get("from"))
            .and_then(|v| v.as_str())
            .unwrap_or("someone");

        let subject = format!("[{}] {}", self.coop_name, title);
        let text = format!(
            "{}\n\nAmount: {} {}\nFrom: {}\n\nView your account at: {}/account",
            body, amount, currency, from, self.base_url
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #2d6a4f;">{}</h2>
    <p>{}</p>
    <div style="background: #f0f9f4; padding: 15px; border-radius: 8px; margin: 20px 0;">
        <p style="margin: 5px 0;"><strong>Amount:</strong> {} {}</p>
        <p style="margin: 5px 0;"><strong>From:</strong> {}</p>
    </div>
    <p><a href="{}/account" style="color: #2d6a4f;">View your account</a></p>
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            title,
            body,
            amount,
            currency,
            truncate_did(from),
            self.base_url,
            self.coop_name
        );

        (subject, text, html)
    }

    fn payment_sent_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let amount = data
            .and_then(|d| d.get("amount"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency = data
            .and_then(|d| d.get("currency"))
            .and_then(|v| v.as_str())
            .unwrap_or("credits");
        let to = data
            .and_then(|d| d.get("to"))
            .and_then(|v| v.as_str())
            .unwrap_or("someone");

        let subject = format!("[{}] {}", self.coop_name, title);
        let text = format!(
            "{}\n\nAmount: {} {}\nTo: {}\n\nView your account at: {}/account",
            body, amount, currency, to, self.base_url
        );
        let html = self.simple_notification_html(
            title,
            body,
            &format!("Amount: {} {} | To: {}", amount, currency, truncate_did(to)),
        );

        (subject, text, html)
    }

    fn proposal_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let proposal_id = data
            .and_then(|d| d.get("proposal_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let subject = format!("[{}] New Proposal: {}", self.coop_name, title);
        let text = format!(
            "{}\n\nVote now at: {}/governance/proposals/{}\n",
            body, self.base_url, proposal_id
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #1e40af;">New Proposal</h2>
    <p>{}</p>
    <a href="{}/governance/proposals/{}" style="display: inline-block; background: #1e40af; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; margin: 20px 0;">Vote Now</a>
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            body, self.base_url, proposal_id, self.coop_name
        );

        (subject, text, html)
    }

    fn proposal_closing_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] {}", self.coop_name, title);
        let text = format!("{body}\n\nDon't forget to cast your vote!");
        let html = self.simple_notification_html(
            title,
            body,
            "This proposal is closing soon. Cast your vote now!",
        );

        (subject, text, html)
    }

    fn vote_recorded_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] {}", self.coop_name, title);
        let text = format!("{body}\n\nThank you for participating!");
        let html = self.simple_notification_html(
            title,
            body,
            "Thank you for participating in cooperative governance.",
        );

        (subject, text, html)
    }

    fn proposal_result_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] {}", self.coop_name, title);
        let text = body.to_string();
        let html = self.simple_notification_html(title, body, "");

        (subject, text, html)
    }

    fn membership_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] {}", self.coop_name, title);
        let text = body.to_string();
        let html = self.simple_notification_html(title, body, "");

        (subject, text, html)
    }

    fn amendment_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let amendment_id = data
            .and_then(|d| d.get("amendment_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let subject = format!("[{}] Constitutional Amendment: {}", self.coop_name, title);
        let text = format!(
            "{}\n\nReview and vote at: {}/governance/amendments/{}\n",
            body, self.base_url, amendment_id
        );
        let html = self.simple_notification_html(
            title,
            body,
            &format!(
                "View amendment: {}/governance/amendments/{}",
                self.base_url, amendment_id
            ),
        );

        (subject, text, html)
    }

    fn amendment_result_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] Amendment Result: {}", self.coop_name, title);
        let text = body.to_string();
        let html = self.simple_notification_html(title, body, "");

        (subject, text, html)
    }

    fn appeal_filed_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] {}", self.coop_name, title);
        let text = body.to_string();
        let html = self.simple_notification_html(
            title,
            body,
            "Your appeal has been filed and is under review.",
        );

        (subject, text, html)
    }

    fn appeal_decision_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] Appeal Decision: {}", self.coop_name, title);
        let text = body.to_string();
        let html = self.simple_notification_html(title, body, "");

        (subject, text, html)
    }

    fn balance_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let new_balance = data
            .and_then(|d| d.get("new_balance"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency = data
            .and_then(|d| d.get("currency"))
            .and_then(|v| v.as_str())
            .unwrap_or("credits");

        let subject = format!("[{}] {}", self.coop_name, title);
        let text = format!("{body}\n\nNew balance: {new_balance} {currency}");
        let html = self.simple_notification_html(
            title,
            body,
            &format!("Current balance: {new_balance} {currency}"),
        );

        (subject, text, html)
    }

    fn security_alert_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] SECURITY ALERT: {}", self.coop_name, title);
        let text = format!(
            "SECURITY ALERT\n\n{body}\n\nIf this was not you, please contact support immediately."
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background: #fef2f2; border: 2px solid #dc2626; border-radius: 8px; padding: 20px; margin-bottom: 20px;">
        <h2 style="color: #dc2626; margin-top: 0;">Security Alert</h2>
        <p style="color: #7f1d1d;">{}</p>
    </div>
    <p>If this was not you, please contact support immediately.</p>
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            body, self.coop_name
        );

        (subject, text, html)
    }

    fn system_template(
        &self,
        title: &str,
        body: &str,
        _data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let subject = format!("[{}] {}", self.coop_name, title);
        let text = body.to_string();
        let html = self.simple_notification_html(title, body, "");

        (subject, text, html)
    }

    /// Generate simple notification HTML
    fn simple_notification_html(&self, title: &str, body: &str, footer: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #374151;">{}</h2>
    <p>{}</p>
    {}
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            title,
            body,
            if footer.is_empty() {
                String::new()
            } else {
                format!("<p style=\"color: #6b7280; font-size: 14px;\">{footer}</p>")
            },
            self.coop_name
        )
    }
}

/// Truncate DID for display
fn truncate_did(did: &str) -> String {
    if did.len() > 20 {
        format!("{}...{}", &did[..12], &did[did.len() - 6..])
    } else {
        did.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smtp_config_sendgrid() {
        let config = SmtpConfig::sendgrid("api-key", "test@example.com", "Test Coop");
        assert_eq!(config.host, "smtp.sendgrid.net");
        assert_eq!(config.port, 587);
        assert_eq!(config.username, "apikey");
    }

    #[test]
    fn test_email_message_creation() {
        let msg = EmailMessage::new("user@example.com", "Test Subject", "Test body");
        assert_eq!(msg.to, "user@example.com");
        assert_eq!(msg.subject, "Test Subject");
        assert!(msg.html_body.is_none());

        let msg = msg.with_html("<p>HTML body</p>");
        assert!(msg.html_body.is_some());
    }

    #[test]
    fn test_email_templates() {
        let templates = EmailTemplates::new("https://mycoop.org", "MyCoop");

        let email = templates.generate_email(
            "user@example.com",
            NotificationType::PaymentReceived,
            "Payment Received",
            "You received a payment",
            Some(&serde_json::json!({
                "amount": 100,
                "currency": "hours",
                "from": "did:icn:alice"
            })),
        );

        assert!(email.subject.contains("Payment Received"));
        assert!(email.text_body.contains("100"));
        assert!(email.html_body.unwrap().contains("hours"));
    }

    #[test]
    fn test_security_alert_template() {
        let templates = EmailTemplates::new("https://mycoop.org", "MyCoop");

        let email = templates.generate_email(
            "user@example.com",
            NotificationType::SecurityAlert,
            "Account Frozen",
            "Your account has been frozen",
            None,
        );

        assert!(email.subject.contains("SECURITY ALERT"));
        assert!(email.html_body.unwrap().contains("dc2626")); // Red color
    }
}
