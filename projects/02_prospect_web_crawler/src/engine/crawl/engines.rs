use crate::engine::CrawlEngine;
use crate::repository::FrontierRepo;
use crate::repository::models::QueuedUrl;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct LeadFocusedEngine {
    repository: Arc<dyn FrontierRepo>,
}

impl LeadFocusedEngine {
    pub fn new(repository: Arc<dyn FrontierRepo>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl CrawlEngine for LeadFocusedEngine {
    async fn select_batch(&self, limit: usize) -> Result<Vec<QueuedUrl>> {
        self.repository.get_pending_urls(limit as i32).await
    }
}

pub struct DiscoveryEngine {
    repository: Arc<dyn FrontierRepo>,
}

impl DiscoveryEngine {
    pub fn new(repository: Arc<dyn FrontierRepo>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl CrawlEngine for DiscoveryEngine {
    async fn select_batch(&self, limit: usize) -> Result<Vec<QueuedUrl>> {
        // Broad BFS: selects by depth ASC, priority DESC
        self.repository.get_pending_urls_bfs(limit as i32).await
    }
}
