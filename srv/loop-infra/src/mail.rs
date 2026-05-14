use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use tera::Tera;

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct SmtpConfig {
    pub sender: String,
    pub token: String,
    pub domain: String,
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct EmailConfig {
    pub from: String,
    pub smtp: SmtpConfig,
}

pub fn load_templates(pattern: &str) -> Result<Tera, tera::Error> {
    Tera::new(pattern)
}

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
