pub mod models;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use models::{DomainMetrics, Lead, QueuedUrl};
use sqlx::{PgPool, Row};

#[async_trait]
pub trait FrontierRepo: Send + Sync {
    async fn get_pending_urls(&self, limit: i32) -> Result<Vec<QueuedUrl>>;
    async fn get_pending_urls_bfs(&self, limit: i32) -> Result<Vec<QueuedUrl>>;
    async fn mark_completed(&self, url_hash: &[u8]) -> Result<()>;
    async fn mark_failed(&self, url_hash: &[u8]) -> Result<()>;
    async fn add_to_frontier(&self, urls: Vec<QueuedUrl>) -> Result<()>;
    async fn get_all_url_hashes(&self) -> Result<Vec<Vec<u8>>>;
}

#[async_trait]
pub trait LeadRepo: Send + Sync {
    async fn upsert_lead(&self, lead: Lead) -> Result<()>;
    async fn get_leads(&self, limit: i32) -> Result<Vec<Lead>>;
    async fn get_lead(&self, fingerprint: &[u8]) -> Result<Option<Lead>>;
}

#[async_trait]
pub trait MetricsRepo: Send + Sync {
    async fn get_domain_metrics(&self, domain: &str) -> Result<Option<DomainMetrics>>;
    async fn upsert_domain_metrics(&self, metrics: DomainMetrics) -> Result<()>;
}

pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FrontierRepo for PostgresRepository {
    async fn get_pending_urls(&self, limit: i32) -> Result<Vec<QueuedUrl>> {
        let urls = sqlx::query_as::<_, QueuedUrl>(
            r#"
            UPDATE frontier
            SET status = 'processing'
            WHERE url_hash IN (
                SELECT url_hash
                FROM frontier
                WHERE status = 'pending' AND available_at <= $1
                ORDER BY priority DESC, available_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            RETURNING url_hash, url, domain, priority, status, available_at, depth
            "#,
        )
        .bind(Utc::now())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(urls)
    }

    async fn get_pending_urls_bfs(&self, limit: i32) -> Result<Vec<QueuedUrl>> {
        let urls = sqlx::query_as::<_, QueuedUrl>(
            r#"
            UPDATE frontier
            SET status = 'processing'
            WHERE url_hash IN (
                SELECT url_hash
                FROM frontier
                WHERE status = 'pending' AND available_at <= $1
                ORDER BY depth ASC, priority DESC, available_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            RETURNING url_hash, url, domain, priority, status, available_at, depth
            "#,
        )
        .bind(Utc::now())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(urls)
    }

    async fn mark_completed(&self, url_hash: &[u8]) -> Result<()> {
        sqlx::query("UPDATE frontier SET status = 'completed' WHERE url_hash = $1")
            .bind(url_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_failed(&self, url_hash: &[u8]) -> Result<()> {
        sqlx::query("UPDATE frontier SET status = 'failed' WHERE url_hash = $1")
            .bind(url_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn add_to_frontier(&self, urls: Vec<QueuedUrl>) -> Result<()> {
        for url in urls {
            sqlx::query(
                r#"
                INSERT INTO frontier (url_hash, url, domain, priority, status, available_at, depth)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (url_hash) DO NOTHING
                "#,
            )
            .bind(url.url_hash)
            .bind(url.url)
            .bind(url.domain)
            .bind(url.priority)
            .bind(url.status)
            .bind(url.available_at)
            .bind(url.depth)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn get_all_url_hashes(&self) -> Result<Vec<Vec<u8>>> {
        let rows = sqlx::query("SELECT url_hash FROM frontier")
            .fetch_all(&self.pool)
            .await?;

        let hashes = rows
            .into_iter()
            .map(|r| r.get::<Vec<u8>, _>("url_hash"))
            .collect();

        Ok(hashes)
    }
}

#[async_trait]
impl LeadRepo for PostgresRepository {
    async fn upsert_lead(&self, lead: Lead) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO leads (fingerprint, full_name, contact_info, score, signals, source_url, discovered_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (fingerprint) DO UPDATE SET
                score = EXCLUDED.score,
                signals = leads.signals || EXCLUDED.signals,
                contact_info = leads.contact_info || EXCLUDED.contact_info
            "#,
        )
        .bind(lead.fingerprint)
        .bind(lead.full_name)
        .bind(lead.contact_info)
        .bind(lead.score)
        .bind(lead.signals)
        .bind(lead.source_url)
        .bind(lead.discovered_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_leads(&self, limit: i32) -> Result<Vec<Lead>> {
        let leads = sqlx::query_as::<_, Lead>(
            "SELECT fingerprint, full_name, contact_info, score, signals, source_url, discovered_at FROM leads ORDER BY score DESC LIMIT $1"
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(leads)
    }

    async fn get_lead(&self, fingerprint: &[u8]) -> Result<Option<Lead>> {
        let lead = sqlx::query_as::<_, Lead>(
            "SELECT fingerprint, full_name, contact_info, score, signals, source_url, discovered_at FROM leads WHERE fingerprint = $1"
        )
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await?;
        Ok(lead)
    }
}

#[async_trait]
impl MetricsRepo for PostgresRepository {
    async fn get_domain_metrics(&self, domain: &str) -> Result<Option<DomainMetrics>> {
        let metrics = sqlx::query_as::<_, DomainMetrics>(
            "SELECT domain, last_fetch_at, crawl_delay_ms, error_count FROM domain_metrics WHERE domain = $1"
        )
        .bind(domain)
        .fetch_optional(&self.pool)
        .await?;
        Ok(metrics)
    }

    async fn upsert_domain_metrics(&self, metrics: DomainMetrics) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO domain_metrics (domain, last_fetch_at, crawl_delay_ms, error_count)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (domain) DO UPDATE SET
                last_fetch_at = EXCLUDED.last_fetch_at,
                crawl_delay_ms = EXCLUDED.crawl_delay_ms,
                error_count = EXCLUDED.error_count
            "#,
        )
        .bind(metrics.domain)
        .bind(metrics.last_fetch_at)
        .bind(metrics.crawl_delay_ms)
        .bind(metrics.error_count)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
