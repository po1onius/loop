use anyhow::anyhow;
use lettre::{
    Message, SmtpTransport, Transport,
    message::header::ContentType,
    transport::smtp::authentication::{Credentials, Mechanism},
};

use crate::config::CONFIG;

pub fn email_code(
    content: &str,
    receiver: &str,
    subject: &str,
    from: &Option<String>,
) -> anyhow::Result<()> {
    let Some(email_cfg) = &CONFIG.load().email else {
        return Err(anyhow!(""));
    };

    let from = from.as_ref().unwrap_or(&email_cfg.from);

    let email = Message::builder()
        .from(from.parse()?)
        .to(format!("<{}>", receiver).parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(content.to_string())?;

    // Create the SMTPS transport
    let sender = SmtpTransport::relay(&email_cfg.smtp.domain)?
        // Add credentials for authentication
        .credentials(Credentials::new(
            email_cfg.smtp.sender.clone(),
            email_cfg.smtp.token.clone(),
        ))
        // Optionally configure expected authentication mechanism
        .authentication(vec![Mechanism::Plain])
        .build();

    // Send the email via remote relay
    sender.send(&email)?;
    Ok(())
}
