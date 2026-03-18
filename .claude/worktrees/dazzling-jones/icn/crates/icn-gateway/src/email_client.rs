//! Email Notification Client
//!
//! Implements email notification delivery via SMTP using lettre.
//! Supports HTML and plain text templates for various notification types.

use std::collections::HashMap;
use tracing::{error, info, warn};

use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

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

    /// Build or get cached SMTP transport
    fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
        let creds = Credentials::new(self.config.username.clone(), self.config.password.clone());

        let tls_parameters = TlsParameters::builder(self.config.host.clone())
            .build()
            .map_err(|e| format!("Failed to build TLS parameters: {e}"))?;

        let builder = if self.config.use_tls {
            // Implicit TLS (port 465)
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                .map_err(|e| format!("Failed to create SMTP relay: {e}"))?
                .port(self.config.port)
                .tls(Tls::Wrapper(tls_parameters))
        } else if self.config.use_starttls {
            // STARTTLS (port 587)
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                .map_err(|e| format!("Failed to create SMTP relay: {e}"))?
                .port(self.config.port)
                .tls(Tls::Required(tls_parameters))
        } else {
            // No TLS (not recommended for production)
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                .port(self.config.port)
        };

        Ok(builder.credentials(creds).build())
    }

    /// Send an email via SMTP
    pub async fn send(&self, message: EmailMessage) -> EmailResult {
        info!(
            to = %message.to,
            subject = %message.subject,
            from = %self.config.from_address,
            "Sending email notification"
        );

        // Parse addresses
        let from_mailbox: Mailbox =
            match format!("{} <{}>", self.config.from_name, self.config.from_address).parse() {
                Ok(m) => m,
                Err(e) => {
                    error!(error = %e, "Invalid from address");
                    return EmailResult::PermanentFailure {
                        error: format!("Invalid from address: {e}"),
                    };
                }
            };

        let to_mailbox: Mailbox = match message.to.parse() {
            Ok(m) => m,
            Err(e) => {
                warn!(to = %message.to, error = %e, "Invalid recipient address");
                return EmailResult::InvalidRecipient {
                    error: format!("Invalid recipient: {e}"),
                };
            }
        };

        // Build the email message
        let mut email_builder = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(&message.subject);

        // Add reply-to if specified
        if let Some(reply_to) = &message.reply_to {
            if let Ok(reply_mailbox) = reply_to.parse::<Mailbox>() {
                email_builder = email_builder.reply_to(reply_mailbox);
            }
        }

        // Build body (multipart if HTML is provided)
        let email = if let Some(html_body) = &message.html_body {
            email_builder
                .multipart(
                    MultiPart::alternative()
                        .singlepart(
                            SinglePart::builder()
                                .header(ContentType::TEXT_PLAIN)
                                .body(message.text_body.clone()),
                        )
                        .singlepart(
                            SinglePart::builder()
                                .header(ContentType::TEXT_HTML)
                                .body(html_body.clone()),
                        ),
                )
                .map_err(|e| format!("Failed to build email: {e}"))
        } else {
            email_builder
                .body(message.text_body.clone())
                .map_err(|e| format!("Failed to build email: {e}"))
        };

        let email = match email {
            Ok(e) => e,
            Err(e) => {
                error!(error = %e, "Failed to build email message");
                return EmailResult::PermanentFailure { error: e };
            }
        };

        // Build transport and send
        let transport = match self.build_transport() {
            Ok(t) => t,
            Err(e) => {
                error!(error = %e, "Failed to create SMTP transport");
                return EmailResult::TemporaryFailure { error: e };
            }
        };

        match transport.send(email).await {
            Ok(response) => {
                let message_id = response
                    .message()
                    .next()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| format!("smtp-{}", uuid::Uuid::new_v4()));

                info!(
                    to = %message.to,
                    message_id = %message_id,
                    "Email sent successfully"
                );

                EmailResult::Success { message_id }
            }
            Err(e) => {
                let error_str = e.to_string();

                // Classify the error
                if error_str.contains("550")
                    || error_str.contains("551")
                    || error_str.contains("553")
                    || error_str.contains("invalid")
                {
                    warn!(to = %message.to, error = %error_str, "Invalid recipient");
                    EmailResult::InvalidRecipient { error: error_str }
                } else if error_str.contains("421")
                    || error_str.contains("450")
                    || error_str.contains("451")
                    || error_str.contains("connection")
                    || error_str.contains("timeout")
                {
                    warn!(to = %message.to, error = %error_str, "Temporary SMTP failure");
                    EmailResult::TemporaryFailure { error: error_str }
                } else {
                    error!(to = %message.to, error = %error_str, "Permanent SMTP failure");
                    EmailResult::PermanentFailure { error: error_str }
                }
            }
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
            NotificationType::DeliberationStarted => self.deliberation_template(title, body, data),
            NotificationType::DeliberationEnding => {
                self.deliberation_ending_template(title, body, data)
            }
            NotificationType::CommentAdded => self.comment_template(title, body, data),
            NotificationType::CommentReply => self.comment_template(title, body, data),
            NotificationType::CommentReaction => self.comment_template(title, body, data),
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

    fn deliberation_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let proposal_id = data
            .and_then(|d| d.get("proposal_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let subject = format!("[{}] Deliberation Started: {}", self.coop_name, title);
        let text = format!(
            "{}\n\nJoin the discussion at: {}/governance/proposals/{}\n",
            body, self.base_url, proposal_id
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #1e40af;">📣 Deliberation Started</h2>
    <p>{}</p>
    <a href="{}/governance/proposals/{}" style="display: inline-block; background: #1e40af; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; margin: 20px 0;">Join the Discussion</a>
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            body, self.base_url, proposal_id, self.coop_name
        );

        (subject, text, html)
    }

    fn deliberation_ending_template(
        &self,
        _title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let proposal_id = data
            .and_then(|d| d.get("proposal_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let hours_remaining = data
            .and_then(|d| d.get("hours_remaining"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let subject = format!("[{}] ⏰ Deliberation Ending Soon", self.coop_name);
        let text = format!(
            "{}\n\nOnly {} hours left! Join the discussion at: {}/governance/proposals/{}\n",
            body, hours_remaining, self.base_url, proposal_id
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background: #fef3c7; border: 2px solid #f59e0b; border-radius: 8px; padding: 20px; margin-bottom: 20px;">
        <h2 style="color: #d97706; margin-top: 0;">⏰ Deliberation Ending Soon</h2>
        <p style="color: #92400e;">{}</p>
        <p style="font-weight: bold;">Only {} hours remaining!</p>
    </div>
    <a href="{}/governance/proposals/{}" style="display: inline-block; background: #f59e0b; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; margin: 20px 0;">Join the Discussion</a>
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            body, hours_remaining, self.base_url, proposal_id, self.coop_name
        );

        (subject, text, html)
    }

    fn comment_template(
        &self,
        title: &str,
        body: &str,
        data: Option<&serde_json::Value>,
    ) -> (String, String, String) {
        let proposal_id = data
            .and_then(|d| d.get("proposal_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let subject = format!("[{}] {}", self.coop_name, title);
        let text = format!(
            "{}\n\nView the discussion at: {}/governance/proposals/{}\n",
            body, self.base_url, proposal_id
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #1e40af;">{}</h2>
    <p>{}</p>
    <a href="{}/governance/proposals/{}" style="display: inline-block; background: #1e40af; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; margin: 20px 0;">View Discussion</a>
    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">
    <p style="color: #666; font-size: 12px;">{}</p>
</body>
</html>"#,
            title, body, self.base_url, proposal_id, self.coop_name
        );

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
