use anyhow::{Context, anyhow, bail};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Debug, Formatter},
    sync::OnceLock,
};
use tera::Tera;

static EMAIL_CONFIG: OnceLock<EmailConfig> = OnceLock::new();

/// SMTP provider settings. `token` is skipped during TOML deserialization and
/// must come from env vars or a secret file.
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SmtpConfig {
    pub sender: String,
    #[serde(skip)]
    pub token: String,
    pub domain: String,
}

impl Debug for SmtpConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("sender", &self.sender)
            .field("token", &"<redacted>")
            .field("domain", &self.domain)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct EmailConfig {
    pub from: String,
    pub smtp: SmtpConfig,
}

impl EmailConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure_non_empty("email.from", &self.from)?;
        ensure_non_empty("email.smtp.sender", &self.smtp.sender)?;
        ensure_non_empty("email.smtp.token", &self.smtp.token)?;
        ensure_non_empty("email.smtp.domain", &self.smtp.domain)?;
        Ok(())
    }
}

/// Initialize the process-wide email configuration used by mail helpers.
#[tracing::instrument(name = "infra.email.init", skip_all)]
pub fn init_email_config(config: EmailConfig) -> anyhow::Result<()> {
    config.validate()?;
    EMAIL_CONFIG
        .set(config)
        .map_err(|_| anyhow!("email config has already been initialized"))
}

pub fn email_config() -> anyhow::Result<&'static EmailConfig> {
    EMAIL_CONFIG
        .get()
        .context("email config is not initialized")
}

#[tracing::instrument(
    name = "mail.templates.load",
    skip_all,
    fields(template.pattern = %pattern)
)]
pub fn load_templates(pattern: &str) -> Result<Tera, tera::Error> {
    Tera::new(pattern)
}

#[tracing::instrument(
    name = "mail.smtp.send_html",
    skip_all,
    fields(email.subject = %subject, smtp.domain = %cfg.smtp.domain)
)]
pub async fn send_html_email(
    cfg: &EmailConfig,
    receiver: &str,
    subject: &str,
    html_body: String,
    from: Option<&str>,
) -> anyhow::Result<()> {
    let from = from.unwrap_or(&cfg.from);
    let email = Message::builder()
        .from(from.parse()?)
        .to(format!("<{}>", receiver).parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html_body)?;

    let creds = Credentials::new(cfg.smtp.sender.clone(), cfg.smtp.token.clone());
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.smtp.domain)?
        .credentials(creds)
        .build();

    mailer.send(email).await?;
    Ok(())
}

#[tracing::instrument(
    name = "mail.smtp.send_configured_html",
    skip_all,
    fields(email.subject = %subject)
)]
pub async fn send_configured_html_email(
    receiver: &str,
    subject: &str,
    html_body: String,
    from: Option<&str>,
) -> anyhow::Result<()> {
    send_html_email(email_config()?, receiver, subject, html_body, from).await
}

fn ensure_non_empty(name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("{name} is required");
    }
    Ok(())
}
