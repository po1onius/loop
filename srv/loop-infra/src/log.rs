use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

pub fn init_log(log_dir: &str, file_name: &str) -> anyhow::Result<WorkerGuard> {
    let file_appender = tracing_appender::rolling::hourly(log_dir, file_name);
    let (file_layer, guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("debug"))
        .add_directive("h2=off".parse()?)
        .add_directive("nacos_sdk=off".parse()?)
        .add_directive("tower=off".parse()?)
        .add_directive("hyper_util=off".parse()?);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::Layer::new()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_writer(std::io::stdout)
                .with_file(true)
                .with_line_number(true),
        )
        .with(
            fmt::Layer::new()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_writer(file_layer)
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true),
        );

    tracing::subscriber::set_global_default(subscriber)
        .context("failed to set global tracing subscriber")?;
    tracing::debug!("tracing initialized");
    Ok(guard)
}
