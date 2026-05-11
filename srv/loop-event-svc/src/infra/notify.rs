use anyhow::anyhow;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use tera::Context;

use crate::{config::CONFIG, infra::TERA};

pub async fn email_code(
    content: &str,
    receiver: &str,
    subject: &str,
    from: &Option<String>,
) -> anyhow::Result<()> {
    let Some(email_cfg) = &CONFIG.load().email else {
        return Err(anyhow!(""));
    };

    let mut tera_ctx = Context::new();
    tera_ctx.insert("code", content);
    tera_ctx.insert("expire_minutes", "2");
    let content = TERA.render("verify_code_email.html", &tera_ctx)?;

    let from = from.as_ref().unwrap_or(&email_cfg.from);

    let email = Message::builder()
        .from(from.parse()?)
        .to(format!("<{}>", receiver).parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(content.to_string())?;

    let creds = Credentials::new(email_cfg.smtp.sender.clone(), email_cfg.smtp.token.clone());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&email_cfg.smtp.domain)?
        .credentials(creds)
        .build();

    mailer.send(email).await?;

    Ok(())
}
