use std::sync::Arc;
use std::time::SystemTime;

use super::hints::Hints;

#[derive(Debug, Clone)]
pub struct IngressedNotification {
    pub app_name: Option<Arc<str>>,
    pub actions: Vec<Arc<str>>,
    pub created_at: SystemTime,
    pub description: Arc<str>,
    pub icon: Option<Arc<str>>,
    pub id: u64,
    pub hints: Hints,
    pub replaces_id: Option<u64>,
    pub title: Arc<str>,
    pub ttl: Option<u64>,
}
