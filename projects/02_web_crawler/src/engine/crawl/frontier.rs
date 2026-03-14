use crate::repository::FrontierRepo;
use anyhow::Result;
use bloomfilter::Bloom;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Frontier {
    filter: RwLock<Bloom<Vec<u8>>>,
}

impl Frontier {
    pub async fn new(
        repository: Arc<dyn FrontierRepo>,
        capacity: usize,
        fp_rate: f64,
    ) -> Result<Self> {
        let mut bloom = Bloom::new_for_fp_rate(capacity, fp_rate);

        // Warm-up: stream hashes from Postgres
        let hashes = repository.get_all_url_hashes().await?;
        for hash in hashes {
            bloom.set(&hash);
        }

        Ok(Self {
            filter: RwLock::new(bloom),
        })
    }

    pub async fn contains(&self, url_hash: &[u8]) -> bool {
        self.filter.read().await.check(&url_hash.to_vec())
    }

    pub async fn add(&self, url_hash: &[u8]) -> bool {
        let mut filter = self.filter.write().await;
        if filter.check(&url_hash.to_vec()) {
            false
        } else {
            filter.set(&url_hash.to_vec());
            true
        }
    }
}
