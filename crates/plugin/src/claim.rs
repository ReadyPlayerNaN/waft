//! ClaimSender: lets plugins initiate EntityClaim checks via the daemon.

use uuid::Uuid;
use waft_protocol::urn::Urn;

/// Request a claim check for a specific entity URN.
pub struct ClaimRequest {
    pub urn: Urn,
    pub claim_id: Uuid,
}

/// Handle for plugins to send claim check requests to the runtime.
///
/// Clone freely; all clones share the same channel.
#[derive(Clone)]
pub struct ClaimSender {
    tx: tokio::sync::mpsc::Sender<ClaimRequest>,
}

impl ClaimSender {
    pub(crate) fn new(tx: tokio::sync::mpsc::Sender<ClaimRequest>) -> Self {
        Self { tx }
    }

    /// Request a claim check for an entity. Call from within `handle_action`.
    ///
    /// The runtime sends `PluginMessage::ClaimCheck` to the daemon and later
    /// calls `handle_claim_result` with the aggregated answer.
    pub async fn request(&self, urn: Urn) -> Uuid {
        let claim_id = Uuid::new_v4();
        if self.tx.send(ClaimRequest { urn, claim_id }).await.is_err() {
            log::warn!("[claim-sender] claim channel closed");
        }
        claim_id
    }
}
