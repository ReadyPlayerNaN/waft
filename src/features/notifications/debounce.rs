use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use log::debug;
use tokio::sync::mpsc;
use tokio::time::{Sleep, sleep};

use super::store::NotificationOp;

/// Debounces ingress operations to prevent flooding the REDUCER.
///
/// Only ingress operations (new notifications) are debounced.
/// All other operations are forwarded immediately.
#[derive(Clone)]
pub struct NotificationDebouncer {
    /// Sender for ingress operations that need debouncing
    ingress_tx: mpsc::UnboundedSender<NotificationOp>,
    /// Sender for immediate operations (non-ingress)
    immediate_tx: mpsc::UnboundedSender<NotificationOp>,
}

impl NotificationDebouncer {
    /// Create a new debouncer with the given REDUCER sender.
    pub fn new(reducer_tx: mpsc::UnboundedSender<NotificationOp>) -> Self {
        let (ingress_tx, ingress_rx) = mpsc::unbounded_channel();
        let (immediate_tx, immediate_rx) = mpsc::unbounded_channel();

        // Spawn debouncer task
        relm4::tokio::spawn(debounce_task(ingress_rx, immediate_rx, reducer_tx.clone()));

        Self {
            ingress_tx,
            immediate_tx,
        }
    }

    /// Send a notification operation for debouncing/immediate processing.
    pub fn send(&self, op: NotificationOp) -> Result<(), mpsc::error::SendError<NotificationOp>> {
        match op {
            // Only debounce ingress operations
            NotificationOp::Ingress(_) => {
                self.ingress_tx.send(op)?;
            }
            // All other operations are immediate
            _ => {
                self.immediate_tx.send(op)?;
            }
        }
        Ok(())
    }
}

/// Debounce task that batches ingress operations and forwards all operations to REDUCER.
async fn debounce_task(
    mut ingress_rx: mpsc::UnboundedReceiver<NotificationOp>,
    mut immediate_rx: mpsc::UnboundedReceiver<NotificationOp>,
    reducer_tx: mpsc::UnboundedSender<NotificationOp>,
) {
    let debounce_timeout = Duration::from_millis(66);
    let mut pending_ingress: HashMap<u64, NotificationOp> = HashMap::new();
    let mut debounce_timer: Option<Pin<Box<Sleep>>> = None;

    loop {
        tokio::select! {
            // Handle ingress operations (debounced)
            Some(ingress_op) = ingress_rx.recv() => {
                if let NotificationOp::Ingress(notification) = ingress_op {
                    // Keep only the latest ingress operation per notification ID
                    pending_ingress.insert(notification.id, NotificationOp::Ingress(notification));

                    // Start new debounce timer if not already running
                    if debounce_timer.is_none() {
                        debounce_timer = Some(Box::pin(sleep(debounce_timeout)));
                    }
                }
            }

            // Handle immediate operations (forwarded right away)
            Some(immediate_op) = immediate_rx.recv() => {
                // If we have pending ingress operations, flush them first
                if !pending_ingress.is_empty() {
                    flush_ingress(&mut pending_ingress, &reducer_tx).await;
                    debounce_timer = None;
                }

                // Forward the immediate operation
                debug!("[debouncer] Immediate operation: {:?}", immediate_op);
                let _ = reducer_tx.send(immediate_op);
            }

            // Debounce timer expired - flush pending ingress operations
            _ = async {
                match debounce_timer.as_mut() {
                    Some(timer) => timer.await,
                    None => std::future::pending().await,
                }
            }, if debounce_timer.is_some() => {
                flush_ingress(&mut pending_ingress, &reducer_tx).await;
                debounce_timer = None;
            }
        }
    }
}

/// Flush pending ingress operations to the REDUCER.
async fn flush_ingress(
    pending_ingress: &mut HashMap<u64, NotificationOp>,
    reducer_tx: &mpsc::UnboundedSender<NotificationOp>,
) {
    if pending_ingress.is_empty() {
        return;
    }

    let batch: Vec<NotificationOp> = pending_ingress.values().map(|op| op.clone()).collect();

    debug!("[debouncer] Flushing {} ingress operations", batch.len());

    let _ = reducer_tx.send(NotificationOp::Batch(batch));
    pending_ingress.clear();
}
