use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::time::Duration;

use log::debug;
use tokio::time::{Sleep, sleep};

use super::store::NotificationOp;

/// Debounces ingress and dismiss operations to prevent flooding the REDUCER.
///
/// - Ingress operations are batched with a longer timeout (333ms)
/// - Dismiss operations are batched with a shorter timeout (50ms) since
///   optimistic UI provides instant feedback
/// - All other operations are forwarded immediately
#[derive(Clone)]
pub struct NotificationDebouncer {
    /// Sender for ingress operations that need debouncing
    ingress_tx: flume::Sender<NotificationOp>,
    /// Sender for dismiss operations that need debouncing
    dismiss_tx: flume::Sender<NotificationOp>,
    /// Sender for immediate operations
    immediate_tx: flume::Sender<NotificationOp>,
}

impl NotificationDebouncer {
    /// Create a new debouncer with the given REDUCER sender.
    pub fn new(reducer_tx: flume::Sender<NotificationOp>) -> Self {
        let (ingress_tx, ingress_rx) = flume::unbounded();
        let (dismiss_tx, dismiss_rx) = flume::unbounded();
        let (immediate_tx, immediate_rx) = flume::unbounded();

        // Spawn debouncer task
        tokio::spawn(async move {
            debounce_task(ingress_rx, dismiss_rx, immediate_rx, reducer_tx.clone()).await;
            log::warn!("[debouncer] task exited unexpectedly");
        });

        Self {
            ingress_tx,
            dismiss_tx,
            immediate_tx,
        }
    }

    /// Send a notification operation for debouncing/immediate processing.
    pub fn send(&self, op: NotificationOp) -> Result<(), flume::SendError<NotificationOp>> {
        match &op {
            // Debounce ingress operations (longer timeout)
            NotificationOp::Ingress(_) => {
                self.ingress_tx.send(op)?;
            }
            // Debounce dismiss operations (shorter timeout, optimistic UI)
            NotificationOp::NotificationDismiss(_) => {
                self.dismiss_tx.send(op)?;
            }
            // All other operations are immediate
            _ => {
                self.immediate_tx.send(op)?;
            }
        }
        Ok(())
    }
}

/// Debounce task that batches ingress and dismiss operations.
async fn debounce_task(
    ingress_rx: flume::Receiver<NotificationOp>,
    dismiss_rx: flume::Receiver<NotificationOp>,
    immediate_rx: flume::Receiver<NotificationOp>,
    reducer_tx: flume::Sender<NotificationOp>,
) {
    let ingress_timeout = Duration::from_millis(333);
    let dismiss_timeout = Duration::from_millis(50);

    let mut pending_ingress: HashMap<u64, NotificationOp> = HashMap::new();
    let mut pending_dismiss: HashSet<u64> = HashSet::new();

    let mut ingress_timer: Option<Pin<Box<Sleep>>> = None;
    let mut dismiss_timer: Option<Pin<Box<Sleep>>> = None;

    loop {
        tokio::select! {
            // Handle ingress operations (debounced with longer timeout)
            Ok(ingress_op) = ingress_rx.recv_async() => {
                if let NotificationOp::Ingress(ref notification) = ingress_op {
                    pending_ingress.insert(notification.id, ingress_op);

                    if ingress_timer.is_none() {
                        ingress_timer = Some(Box::pin(sleep(ingress_timeout)));
                    }
                }
            }

            // Handle dismiss operations (debounced with shorter timeout)
            Ok(dismiss_op) = dismiss_rx.recv_async() => {
                if let NotificationOp::NotificationDismiss(id) = dismiss_op {
                    log::debug!("[debouncer] dismiss received id={}", id);
                    pending_dismiss.insert(id);

                    if dismiss_timer.is_none() {
                        dismiss_timer = Some(Box::pin(sleep(dismiss_timeout)));
                    }
                }
            }

            // Handle immediate operations (forwarded right away)
            Ok(immediate_op) = immediate_rx.recv_async() => {
                // Flush pending operations first to maintain ordering
                if !pending_ingress.is_empty() {
                    flush_ingress(&mut pending_ingress, &reducer_tx).await;
                    ingress_timer = None;
                }
                if !pending_dismiss.is_empty() {
                    flush_dismiss(&mut pending_dismiss, &reducer_tx).await;
                    dismiss_timer = None;
                }

                debug!("[debouncer] Immediate operation: {:?}", immediate_op);
                let _ = reducer_tx.send(immediate_op);
            }

            // Ingress timer expired
            _ = async {
                match ingress_timer.as_mut() {
                    Some(timer) => timer.await,
                    None => std::future::pending().await,
                }
            }, if ingress_timer.is_some() => {
                flush_ingress(&mut pending_ingress, &reducer_tx).await;
                ingress_timer = None;
            }

            // Dismiss timer expired
            _ = async {
                match dismiss_timer.as_mut() {
                    Some(timer) => timer.await,
                    None => std::future::pending().await,
                }
            }, if dismiss_timer.is_some() => {
                flush_dismiss(&mut pending_dismiss, &reducer_tx).await;
                dismiss_timer = None;
            }
        }
    }
}

/// Flush pending ingress operations to the REDUCER.
async fn flush_ingress(
    pending_ingress: &mut HashMap<u64, NotificationOp>,
    reducer_tx: &flume::Sender<NotificationOp>,
) {
    if pending_ingress.is_empty() {
        return;
    }

    let batch: Vec<NotificationOp> = pending_ingress.values().cloned().collect();

    debug!("[debouncer] Flushing {} ingress operations", batch.len());

    let _ = reducer_tx.send(NotificationOp::Batch(batch));
    pending_ingress.clear();
}

/// Flush pending dismiss operations to the REDUCER.
async fn flush_dismiss(
    pending_dismiss: &mut HashSet<u64>,
    reducer_tx: &flume::Sender<NotificationOp>,
) {
    if pending_dismiss.is_empty() {
        return;
    }

    let batch: Vec<NotificationOp> = pending_dismiss
        .iter()
        .map(|id| NotificationOp::NotificationDismiss(*id))
        .collect();

    debug!("[debouncer] Flushing {} dismiss operations", batch.len());

    let result = reducer_tx.send(NotificationOp::Batch(batch));
    debug!("[debouncer] Flush dismiss result: {:?}", result.is_ok());
    pending_dismiss.clear();
}
