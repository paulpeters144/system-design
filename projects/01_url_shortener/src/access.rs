use anyhow::{Result, Context};
use sqlx::{FromRow, PgPool};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, Client};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct UrlRecord {
    pub id: i64,
    pub long_url: String,
    pub short_code: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait::async_trait]
pub trait UrlRepository: Send + Sync {
    async fn save(&self, long_url: &str, short_code: &str) -> Result<UrlRecord, RepositoryError>;
    async fn get_by_code(&self, short_code: &str) -> Result<Option<UrlRecord>>;
    async fn init_db(&self) -> Result<()>;
}

pub struct PostgresUrlRepository {
    pool: PgPool,
}

impl PostgresUrlRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl UrlRepository for PostgresUrlRepository {
    async fn init_db(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS urls (
                id BIGSERIAL PRIMARY KEY,
                long_url TEXT NOT NULL,
                short_code TEXT NOT NULL UNIQUE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS url_analytics (
                id BIGSERIAL PRIMARY KEY,
                url_id BIGINT REFERENCES urls(id),
                ip_address TEXT,
                user_agent TEXT,
                clicked_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn save(&self, long_url: &str, short_code: &str) -> Result<UrlRecord, RepositoryError> {
        let record = sqlx::query_as::<_, UrlRecord>(
            "INSERT INTO urls (long_url, short_code) 
             VALUES ($1, $2) 
             RETURNING id, long_url, short_code, created_at"
        )
        .bind(long_url)
        .bind(short_code)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let Some(pg_err) = e.as_database_error() {
                if pg_err.code().map_or(false, |c| c == "23505") {
                    return RepositoryError::Conflict(short_code.to_string());
                }
            }
            RepositoryError::Database(e)
        })?;

        Ok(record)
    }

    async fn get_by_code(&self, short_code: &str) -> Result<Option<UrlRecord>> {
        let record = sqlx::query_as::<_, UrlRecord>(
            "SELECT id, long_url, short_code, created_at 
             FROM urls 
             WHERE short_code = $1"
        )
        .bind(short_code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }
}

#[async_trait::async_trait]
pub trait CacheRepository: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<()>;
}

pub struct RedisCacheRepository {
    client: Client,
}

impl RedisCacheRepository {
    pub fn new(url: &str) -> Result<Self> {
        let client = Client::open(url).context("Failed to open Redis client")?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl CacheRepository for RedisCacheRepository {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let val: Option<String> = conn.get(key).await?;
        Ok(val)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.set_ex(key, value, ttl_secs).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait AnalyticsRepository: Send + Sync {
    async fn record_click(&self, url_id: i64, ip_address: Option<String>, user_agent: Option<String>) -> Result<()>;
}

#[async_trait::async_trait]
impl AnalyticsRepository for PostgresUrlRepository {
    async fn record_click(&self, url_id: i64, ip_address: Option<String>, user_agent: Option<String>) -> Result<()> {
        sqlx::query(
            "INSERT INTO url_analytics (url_id, ip_address, user_agent) VALUES ($1, $2, $3)"
        )
        .bind(url_id)
        .bind(ip_address)
        .bind(user_agent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
