mod http;
mod infra;

use axum::{
    Router,
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use base64::Engine;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, sync::Arc};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct MeResp {
    user_id: String,
    email: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:auth.db")
        .await?;

    init_db(&db).await?;

    // 生产环境：从环境变量读，并且足够长（>=32 bytes），定期轮换
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-me-please-32bytes+".to_string());

    let state = AppState {
        db,
        jwt_enc: EncodingKey::from_secret(jwt_secret.as_bytes()),
        jwt_dec: DecodingKey::from_secret(jwt_secret.as_bytes()),
        jwt_issuer: "example-auth".to_string(),
        access_ttl: Duration::minutes(15),
        refresh_ttl: Duration::days(30),
    };

    // 示例：初始化一个用户（email: test@example.com, password: password）
    seed_user(&state).await?;

    let app = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/me", get(me))
        .with_state(Arc::new(state));

    let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
    info!("listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

async fn init_db(db: &SqlitePool) -> anyhow::Result<()> {
    // users
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    // refresh tokens (store hash only)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            expires_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            revoked_at INTEGER,
            used_at INTEGER,
            FOREIGN KEY(user_id) REFERENCES users(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn seed_user(state: &AppState) -> anyhow::Result<()> {
    // 这里为了演示，password_hash 就直接存明文（生产请用 argon2/bcrypt）
    let email = "test@example.com";
    let password_hash = "password";

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(email)
        .fetch_optional(&state.db)
        .await?;

    if existing.is_none() {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, email, password_hash) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(email)
            .bind(password_hash)
            .execute(&state.db)
            .await?;
        info!("seeded user: {email} / password");
    }
    Ok(())
}
