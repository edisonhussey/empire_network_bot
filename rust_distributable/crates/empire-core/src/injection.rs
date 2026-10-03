use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::INJECTION_QUEUE_CAPACITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionRequest {
    pub id: Uuid,
    pub packet: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

impl InjectionRequest {
    pub fn new(packet: String, now_ms: i64, ttl_ms: i64) -> Self {
        Self {
            id: Uuid::new_v4(),
            packet,
            created_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(ttl_ms),
        }
    }

    pub fn expired(&self, now_ms: i64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

pub fn channel() -> (
    mpsc::Sender<InjectionRequest>,
    mpsc::Receiver<InjectionRequest>,
) {
    mpsc::channel(INJECTION_QUEUE_CAPACITY)
}
