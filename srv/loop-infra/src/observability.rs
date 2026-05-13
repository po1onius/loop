use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode, header};
use opentelemetry::{
    KeyValue, global,
    propagation::Extractor,
    trace::{TraceContextExt, TracerProvider as _},
};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    propagation::TraceContextPropagator,
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
};
use std::{
    collections::BTreeMap,
    sync::{
        LazyLock, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

const DEFAULT_ENVIRONMENT: &str = "local";
const METRIC_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

static SERVICE_INFO: OnceLock<ServiceInfo> = OnceLock::new();
static HTTP_METRICS: LazyLock<HttpMetrics> = LazyLock::new(HttpMetrics::default);

tokio::task_local! {
    static REQUEST_ID: Option<String>;
}

#[derive(Clone, Debug)]
pub struct ObservabilityConfig {
    pub service_name: String,
    pub service_version: String,
    pub environment: String,
    pub log_dir: String,
    pub log_file: String,
    pub otlp_endpoint: Option<String>,
}

impl ObservabilityConfig {
    pub fn new(service_name: impl Into<String>, service_version: impl Into<String>) -> Self {
        let service_name = service_name.into();
        Self {
            log_file: first_non_empty_env(&["LOOP_LOG_FILE"])
                .unwrap_or_else(|| format!("{service_name}.log")),
            service_name,
            service_version: service_version.into(),
            environment: std::env::var("LOOP_ENV")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_string()),
            log_dir: first_non_empty_env(&["LOOP_LOG_DIR"]).unwrap_or_else(|| "log".to_string()),
            otlp_endpoint: first_non_empty_env(&[
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "OTEL_EXPORTER_OTLP_ENDPOINT",
            ]),
        }
    }
}

pub struct ObservabilityGuard {
    _log_guard: WorkerGuard,
    tracer_provider: Option<SdkTracerProvider>,
}

pub fn init(config: ObservabilityConfig) -> anyhow::Result<ObservabilityGuard> {
    let service_info = ServiceInfo {
        name: config.service_name.clone(),
        version: config.service_version.clone(),
        environment: config.environment.clone(),
    };
    let _ = SERVICE_INFO.set(service_info);

    global::set_text_map_propagator(TraceContextPropagator::new());
    let tracer_provider = init_tracer_provider(&config)?;
    let log_guard = init_tracing_subscriber(&config, tracer_provider.clone())?;
    tracing::info!(
        service.name = %config.service_name,
        service.version = %config.service_version,
        deployment.environment = %config.environment,
        otel.enabled = tracer_provider.is_some(),
        "observability initialized"
    );
    Ok(ObservabilityGuard {
        _log_guard: log_guard,
        tracer_provider,
    })
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if let Some(tracer_provider) = self.tracer_provider.take()
            && let Err(err) = tracer_provider.shutdown()
        {
            eprintln!("failed to shutdown opentelemetry tracer provider: {err:?}");
        }
    }
}

pub async fn extract_trace_context(req: Request, next: Next) -> Response {
    let parent_context = global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(req.headers()))
    });

    if parent_context.span().span_context().is_valid() {
        tracing::Span::current().set_parent(parent_context);
    }

    next.run(req).await
}

pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok().flatten()
}

pub fn current_trace_id() -> Option<String> {
    let context = tracing::Span::current().context();
    let span_context = context.span().span_context().clone();
    span_context
        .is_valid()
        .then(|| span_context.trace_id().to_string())
}

pub async fn record_http_metrics(req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_string();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let started_at = Instant::now();
    let _in_flight = InFlightGuard::new();
    // 保存请求级上下文，保证错误转换为响应时也能写入关联 ID。
    let response = REQUEST_ID.scope(request_id.clone(), next.run(req)).await;
    let status = response.status();
    let response_request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let request_id = request_id.or(response_request_id).unwrap_or_default();
    let trace_id = current_trace_id().unwrap_or_default();
    let elapsed = started_at.elapsed();
    let service_info = service_info();

    HTTP_METRICS.record(&method, &path, status, elapsed.as_secs_f64());
    tracing::info!(
        service.name = %service_info.name,
        deployment.environment = %service_info.environment,
        http.method = %method,
        http.route = %path,
        http.status_code = status.as_u16(),
        http.duration_ms = elapsed.as_secs_f64() * 1000.0,
        request.id = %request_id,
        trace.id = %trace_id,
        "http request completed"
    );

    response
}

fn init_tracer_provider(config: &ObservabilityConfig) -> anyhow::Result<Option<SdkTracerProvider>> {
    let Some(endpoint) = config.otlp_endpoint.as_deref() else {
        return Ok(None);
    };

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_protocol(Protocol::Grpc)
        .build()?;
    let resource = Resource::builder_empty()
        .with_service_name(config.service_name.clone())
        .with_attributes([
            KeyValue::new("service.version", config.service_version.clone()),
            KeyValue::new("deployment.environment.name", config.environment.clone()),
        ])
        .build();
    let tracer_provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            1.0,
        ))))
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    global::set_tracer_provider(tracer_provider.clone());
    Ok(Some(tracer_provider))
}

fn init_tracing_subscriber(
    config: &ObservabilityConfig,
    tracer_provider: Option<SdkTracerProvider>,
) -> anyhow::Result<WorkerGuard> {
    let file_appender = tracing_appender::rolling::hourly(&config.log_dir, &config.log_file);
    let (file_layer, guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"))
        .add_directive("h2=off".parse()?)
        .add_directive("nacos_sdk=off".parse()?)
        .add_directive("tower=off".parse()?)
        .add_directive("hyper_util=off".parse()?)
        .add_directive("opentelemetry=info".parse()?)
        .add_directive("opentelemetry_otlp=info".parse()?)
        .add_directive("tonic=info".parse()?);

    let otel_layer = tracer_provider.map(|provider| {
        tracing_opentelemetry::layer().with_tracer(provider.tracer(config.service_name.clone()))
    });
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
        )
        .with(otel_layer);

    tracing::subscriber::set_global_default(subscriber)?;
    tracing::debug!("tracing initialized");
    Ok(guard)
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|key| key.as_str()).collect()
    }
}

pub async fn metrics_handler() -> impl IntoResponse {
    let body = HTTP_METRICS.render_prometheus();
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

#[derive(Clone, Debug)]
struct ServiceInfo {
    name: String,
    version: String,
    environment: String,
}

#[derive(Default)]
struct HttpMetrics {
    in_flight: AtomicU64,
    values: Mutex<BTreeMap<HttpMetricKey, HttpMetricValue>>,
}

impl HttpMetrics {
    fn record(&self, method: &str, path: &str, status: StatusCode, duration_seconds: f64) {
        let key = HttpMetricKey {
            method: method.to_string(),
            path: path.to_string(),
            status: status.as_u16(),
        };
        let mut values = self.values.lock().expect("http metrics mutex poisoned");
        values
            .entry(key)
            .or_default()
            .record(status, duration_seconds);
    }

    fn render_prometheus(&self) -> String {
        let service_info = service_info();
        let mut out = String::new();
        out.push_str("# HELP loop_service_info Service metadata.\n");
        out.push_str("# TYPE loop_service_info gauge\n");
        push_metric_line(
            &mut out,
            "loop_service_info",
            &[
                ("service", &service_info.name),
                ("version", &service_info.version),
                ("environment", &service_info.environment),
            ],
            "1",
        );

        out.push_str("# HELP loop_http_requests_in_flight Current in-flight HTTP requests.\n");
        out.push_str("# TYPE loop_http_requests_in_flight gauge\n");
        push_metric_line(
            &mut out,
            "loop_http_requests_in_flight",
            &[
                ("service", &service_info.name),
                ("environment", &service_info.environment),
            ],
            &self.in_flight.load(Ordering::Relaxed).to_string(),
        );

        out.push_str("# HELP loop_http_requests_total Total HTTP requests.\n");
        out.push_str("# TYPE loop_http_requests_total counter\n");
        out.push_str("# HELP loop_http_request_errors_total Total HTTP 5xx responses.\n");
        out.push_str("# TYPE loop_http_request_errors_total counter\n");
        out.push_str("# HELP loop_http_request_duration_seconds HTTP request duration.\n");
        out.push_str("# TYPE loop_http_request_duration_seconds histogram\n");

        let values = self.values.lock().expect("http metrics mutex poisoned");
        for (key, value) in values.iter() {
            let status = key.status.to_string();
            let base_labels = [
                ("service", service_info.name.as_str()),
                ("environment", service_info.environment.as_str()),
                ("method", key.method.as_str()),
                ("path", key.path.as_str()),
                ("status", status.as_str()),
            ];

            push_metric_line(
                &mut out,
                "loop_http_requests_total",
                &base_labels,
                &value.requests_total.to_string(),
            );
            if value.errors_total > 0 {
                push_metric_line(
                    &mut out,
                    "loop_http_request_errors_total",
                    &base_labels,
                    &value.errors_total.to_string(),
                );
            }

            let mut cumulative = 0;
            for (idx, upper_bound) in METRIC_BUCKETS.iter().enumerate() {
                cumulative += value.duration_buckets[idx];
                let le = format_bucket(*upper_bound);
                let labels = [
                    ("service", service_info.name.as_str()),
                    ("environment", service_info.environment.as_str()),
                    ("method", key.method.as_str()),
                    ("path", key.path.as_str()),
                    ("status", status.as_str()),
                    ("le", le.as_str()),
                ];
                push_metric_line(
                    &mut out,
                    "loop_http_request_duration_seconds_bucket",
                    &labels,
                    &cumulative.to_string(),
                );
            }

            let inf_labels = [
                ("service", service_info.name.as_str()),
                ("environment", service_info.environment.as_str()),
                ("method", key.method.as_str()),
                ("path", key.path.as_str()),
                ("status", status.as_str()),
                ("le", "+Inf"),
            ];
            push_metric_line(
                &mut out,
                "loop_http_request_duration_seconds_bucket",
                &inf_labels,
                &value.duration_count.to_string(),
            );
            push_metric_line(
                &mut out,
                "loop_http_request_duration_seconds_sum",
                &base_labels,
                &format!("{:.6}", value.duration_sum_seconds),
            );
            push_metric_line(
                &mut out,
                "loop_http_request_duration_seconds_count",
                &base_labels,
                &value.duration_count.to_string(),
            );
        }

        out
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct HttpMetricKey {
    method: String,
    path: String,
    status: u16,
}

#[derive(Clone, Debug, Default)]
struct HttpMetricValue {
    requests_total: u64,
    errors_total: u64,
    duration_sum_seconds: f64,
    duration_count: u64,
    duration_buckets: [u64; METRIC_BUCKETS.len()],
}

impl HttpMetricValue {
    fn record(&mut self, status: StatusCode, duration_seconds: f64) {
        self.requests_total += 1;
        if status.is_server_error() {
            self.errors_total += 1;
        }
        self.duration_sum_seconds += duration_seconds;
        self.duration_count += 1;
        for (idx, upper_bound) in METRIC_BUCKETS.iter().enumerate() {
            if duration_seconds <= *upper_bound {
                self.duration_buckets[idx] += 1;
                break;
            }
        }
    }
}

struct InFlightGuard;

impl InFlightGuard {
    fn new() -> Self {
        HTTP_METRICS.in_flight.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        HTTP_METRICS.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

fn service_info() -> ServiceInfo {
    SERVICE_INFO.get().cloned().unwrap_or_else(|| ServiceInfo {
        name: "unknown".to_string(),
        version: "unknown".to_string(),
        environment: DEFAULT_ENVIRONMENT.to_string(),
    })
}

fn push_metric_line(out: &mut String, name: &str, labels: &[(&str, &str)], value: &str) {
    out.push_str(name);
    out.push('{');
    for (idx, (key, value)) in labels.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(key);
        out.push_str("=\"");
        push_escaped_label_value(out, value);
        out.push('"');
    }
    out.push_str("} ");
    out.push_str(value);
    out.push('\n');
}

fn push_escaped_label_value(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
}

fn format_bucket(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}
