use anyhow::{Context as AnyhowContext, anyhow};
use std::sync::LazyLock;
use tera::{Context, Tera};

use crate::config::config;

static EMAIL_TEMPLATES: LazyLock<Result<Tera, tera::Error>> = LazyLock::new(|| {
    let pattern = format!("{}/../static/templates/**/*", env!("CARGO_MANIFEST_DIR"));
    loop_infra::mail::load_templates(&pattern)
});

#[tracing::instrument(
    name = "notify.email_code",
    skip_all,
    fields(email.subject = %subject, email.expire_minutes = expire_minutes)
)]
pub async fn email_code(
    content: &str,
    receiver: &str,
    subject: &str,
    from: &Option<String>,
    expire_minutes: u64,
) -> anyhow::Result<()> {
    let email_cfg = config()
        .email
        .clone()
        .ok_or_else(|| anyhow!("email config is missing"))?;

    let templates = EMAIL_TEMPLATES
        .as_ref()
        .map_err(|err| anyhow!("failed to load email templates: {err}"))?;

    let mut tera_ctx = Context::new();
    tera_ctx.insert("code", content);
    tera_ctx.insert("expire_minutes", &expire_minutes);
    let html_body = templates
        .render("verify_code_email.html", &tera_ctx)
        .context("failed to render verify code email template")?;

    loop_infra::mail::send_html_email(&email_cfg, receiver, subject, html_body, from.as_deref())
        .await
        .context("failed to send verify code email")
}
