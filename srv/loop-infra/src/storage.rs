use anyhow::{Context, anyhow, bail};
use aws_sdk_s3::{
    Client,
    config::{BehaviorVersion, Credentials, Region},
    presigning::PresigningConfig,
};
use std::{
    fmt::{self, Debug, Formatter},
    sync::OnceLock,
    time::Duration,
};

static S3_STORAGE: OnceLock<S3Storage> = OnceLock::new();

/// Runtime S3-compatible object storage configuration.
///
/// 对象存储供应商和 bucket 参数依赖部署环境，凭证依赖 Secret。它们都只通过
/// env vars 或 secret files 注入；上传限制、预签名 TTL 等业务策略由业务服务
/// 自己维护。
#[derive(Clone)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint_url: Option<String>,
    pub presign_endpoint_url: Option<String>,
    pub public_base_url: Option<String>,
    pub key_prefix: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub force_path_style: bool,
}

impl Default for S3StorageConfig {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            region: "us-east-1".to_string(),
            endpoint_url: None,
            presign_endpoint_url: None,
            public_base_url: None,
            key_prefix: "media".to_string(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            session_token: None,
            force_path_style: false,
        }
    }
}

impl Debug for S3StorageConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3StorageConfig")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint_url", &self.endpoint_url)
            .field("presign_endpoint_url", &self.presign_endpoint_url)
            .field("public_base_url", &self.public_base_url)
            .field("key_prefix", &self.key_prefix)
            .field("access_key_id", &mask_secret(&self.access_key_id))
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .field("force_path_style", &self.force_path_style)
            .finish()
    }
}

impl S3StorageConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure_non_empty("storage.bucket", &self.bucket)?;
        ensure_non_empty("storage.region", &self.region)?;
        ensure_non_empty("storage.access_key_id", &self.access_key_id)?;
        ensure_non_empty("storage.secret_access_key", &self.secret_access_key)?;
        if let Some(endpoint_url) = &self.endpoint_url {
            ensure_non_empty("storage.endpoint_url", endpoint_url)?;
        }
        if let Some(presign_endpoint_url) = &self.presign_endpoint_url {
            ensure_non_empty("storage.presign_endpoint_url", presign_endpoint_url)?;
        }
        if let Some(public_base_url) = &self.public_base_url {
            ensure_non_empty("storage.public_base_url", public_base_url)?;
        }
        if let Some(session_token) = &self.session_token {
            ensure_non_empty("storage.session_token", session_token)?;
        }
        Ok(())
    }

    pub fn normalized_key_prefix(&self) -> String {
        self.key_prefix
            .trim()
            .trim_matches('/')
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Thin shared S3 client wrapper used by business services through module-level
/// helper functions. Handlers should not construct AWS clients themselves.
#[derive(Clone)]
pub struct S3Storage {
    client: Client,
    presign_client: Client,
    config: S3StorageConfig,
}

#[derive(Debug)]
pub struct PresignedStorageRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<PresignedStorageHeader>,
    pub expires_in: u64,
}

#[derive(Debug)]
pub struct PresignedStorageHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug)]
pub struct HeadObjectInfo {
    pub byte_size: i64,
    pub mime_type: Option<String>,
}

impl S3Storage {
    #[tracing::instrument(name = "storage.s3.init", skip_all, fields(storage.bucket = %config.bucket))]
    pub fn new(config: S3StorageConfig) -> anyhow::Result<Self> {
        config.validate()?;
        let credentials = Credentials::new(
            config.access_key_id.clone(),
            config.secret_access_key.clone(),
            config.session_token.clone(),
            None,
            "loop-config",
        );
        let base_config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()))
            .credentials_provider(credentials);
        let mut s3_config = base_config.clone();
        if let Some(endpoint_url) = config
            .endpoint_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            s3_config = s3_config.endpoint_url(endpoint_url.to_string());
        }

        let mut presign_config = base_config;
        let presign_endpoint_url = config
            .presign_endpoint_url
            .as_ref()
            .or(config.endpoint_url.as_ref())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        if let Some(endpoint_url) = presign_endpoint_url {
            presign_config = presign_config.endpoint_url(endpoint_url.to_string());
        }
        if config.force_path_style {
            s3_config = s3_config.force_path_style(true);
            presign_config = presign_config.force_path_style(true);
        }

        tracing::info!(
            event = "storage.s3.initialized",
            storage_bucket = %config.bucket,
            storage_region = %config.region,
            endpoint_configured = config.endpoint_url.is_some(),
            presign_endpoint_configured = config.presign_endpoint_url.is_some(),
            public_base_url_configured = config.public_base_url.is_some(),
            force_path_style = config.force_path_style,
            "S3 storage client initialized"
        );

        Ok(Self {
            client: Client::from_conf(s3_config.build()),
            presign_client: Client::from_conf(presign_config.build()),
            config,
        })
    }

    pub fn config(&self) -> &S3StorageConfig {
        &self.config
    }

    #[tracing::instrument(
        name = "storage.s3.presign_put",
        skip_all,
        fields(storage.bucket = %self.config.bucket, storage.key = %key, storage.mime_type = %mime_type)
    )]
    pub async fn presign_put(
        &self,
        key: &str,
        mime_type: &str,
        expires_in: u64,
    ) -> anyhow::Result<PresignedStorageRequest> {
        let presign_config = PresigningConfig::expires_in(Duration::from_secs(expires_in))
            .context("failed to build S3 put presigning config")?;
        let request = self
            .presign_client
            .put_object()
            .bucket(&self.config.bucket)
            .key(key)
            .content_type(mime_type)
            .presigned(presign_config)
            .await
            .context("failed to presign S3 put object request")?;

        tracing::info!(
            event = "storage.s3.presigned_put",
            storage_bucket = %self.config.bucket,
            storage_key = %key,
            storage_mime_type = %mime_type,
            expires_in = expires_in,
            "S3 PUT presigned URL generated"
        );

        to_presigned_request(request, expires_in)
    }

    #[tracing::instrument(
        name = "storage.s3.presign_get",
        skip_all,
        fields(storage.bucket = %self.config.bucket, storage.key = %key)
    )]
    pub async fn presign_get(
        &self,
        key: &str,
        expires_in: u64,
    ) -> anyhow::Result<PresignedStorageRequest> {
        let presign_config = PresigningConfig::expires_in(Duration::from_secs(expires_in))
            .context("failed to build S3 get presigning config")?;
        let request = self
            .presign_client
            .get_object()
            .bucket(&self.config.bucket)
            .key(key)
            .presigned(presign_config)
            .await
            .context("failed to presign S3 get object request")?;

        tracing::info!(
            event = "storage.s3.presigned_get",
            storage_bucket = %self.config.bucket,
            storage_key = %key,
            expires_in = expires_in,
            "S3 GET presigned URL generated"
        );

        to_presigned_request(request, expires_in)
    }

    #[tracing::instrument(
        name = "storage.s3.head",
        skip_all,
        fields(storage.bucket = %self.config.bucket, storage.key = %key)
    )]
    pub async fn head_object(&self, key: &str) -> anyhow::Result<HeadObjectInfo> {
        let output = self
            .client
            .head_object()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await
            .context("failed to head S3 object")?;
        let byte_size = output
            .content_length()
            .context("S3 head object response missing content length")?;
        Ok(HeadObjectInfo {
            byte_size,
            mime_type: output.content_type().map(ToString::to_string),
        })
    }

    pub fn public_url(&self, key: &str) -> Option<String> {
        self.config.public_base_url.as_ref().map(|base| {
            format!(
                "{}/{}",
                base.trim_end_matches('/'),
                key.trim_start_matches('/')
            )
        })
    }
}

/// Initialize the process-wide S3 storage client.
#[tracing::instrument(name = "infra.storage.s3.init", skip_all)]
pub fn init_s3_storage(config: S3StorageConfig) -> anyhow::Result<()> {
    let storage = S3Storage::new(config)?;
    S3_STORAGE
        .set(storage)
        .map_err(|_| anyhow!("S3 storage client has already been initialized"))
}

pub fn s3_storage() -> anyhow::Result<&'static S3Storage> {
    S3_STORAGE
        .get()
        .context("S3 storage client is not initialized")
}

pub fn s3_storage_config() -> anyhow::Result<&'static S3StorageConfig> {
    Ok(s3_storage()?.config())
}

pub async fn presign_put(
    key: &str,
    mime_type: &str,
    expires_in: u64,
) -> anyhow::Result<PresignedStorageRequest> {
    s3_storage()?.presign_put(key, mime_type, expires_in).await
}

pub async fn presign_get(key: &str, expires_in: u64) -> anyhow::Result<PresignedStorageRequest> {
    s3_storage()?.presign_get(key, expires_in).await
}

pub async fn head_object(key: &str) -> anyhow::Result<HeadObjectInfo> {
    s3_storage()?.head_object(key).await
}

pub fn public_url(key: &str) -> anyhow::Result<Option<String>> {
    Ok(s3_storage()?.public_url(key))
}

fn to_presigned_request(
    request: aws_sdk_s3::presigning::PresignedRequest,
    expires_in: u64,
) -> anyhow::Result<PresignedStorageRequest> {
    let headers = request
        .headers()
        .map(|(name, value)| {
            Ok(PresignedStorageHeader {
                name: name.to_string(),
                value: value.to_string(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PresignedStorageRequest {
        method: request.method().to_string(),
        url: request.uri().to_string(),
        headers,
        expires_in,
    })
}

fn ensure_non_empty(name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("{name} is required");
    }
    Ok(())
}

fn mask_secret(value: &str) -> String {
    let visible = value.chars().take(4).collect::<String>();
    if visible.is_empty() {
        "<empty>".to_string()
    } else {
        format!("{visible}***")
    }
}
