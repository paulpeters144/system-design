use anyhow::Result;
use sqlx::{FromRow, PgPool};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct UrlRecord {
    pub id: i64,
    pub long_url: String,
    pub short_code: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait::async_trait]
pub trait UrlRepository: Send + Sync {
    async fn save(&self, long_url: &str, short_code: &str) -> Result<UrlRecord>;
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
        Ok(())
    }

    async fn save(&self, long_url: &str, short_code: &str) -> Result<UrlRecord> {
        let record = sqlx::query_as::<_, UrlRecord>(
            "INSERT INTO urls (long_url, short_code) 
             VALUES ($1, $2) 
             RETURNING id, long_url, short_code, created_at"
        )
        .bind(long_url)
        .bind(short_code)
        .fetch_one(&self.pool)
        .await?;

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
