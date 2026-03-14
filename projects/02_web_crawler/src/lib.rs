pub mod engine;
pub mod repository;

use std::sync::Arc;
use repository::{FrontierRepo, LeadRepo};
use engine::AppManager;

pub struct AppState {
    pub manager: Arc<AppManager>,
    pub lead_repo: Arc<dyn LeadRepo>,
    pub frontier_repo: Arc<dyn FrontierRepo>,
}
