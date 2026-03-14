pub mod engine;
pub mod repository;

use engine::AppManager;
use repository::{FrontierRepo, LeadRepo};
use std::sync::Arc;

pub struct AppState {
    pub manager: Arc<AppManager>,
    pub lead_repo: Arc<dyn LeadRepo>,
    pub frontier_repo: Arc<dyn FrontierRepo>,
}
