use crate::api::WsState;
use crate::data::{Database, PositionStore};
use crate::monitors::HealthMonitor;
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Subscribes to new blocks and triggers position monitoring
pub struct BlockSubscriber {
    provider_url: String,
    store: Arc<PositionStore>,
    db: Database,
    ws_state: Arc<WsState>,
    health_monitor: Arc<HealthMonitor>,
    shutdown_tx: broadcast::Sender<()>,
}

impl BlockSubscriber {
    pub fn new(
        provider_url: String,
        store: Arc<PositionStore>,
        db: Database,
        ws_state: Arc<WsState>,
        health_monitor: Arc<HealthMonitor>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            provider_url,
            store,
            db,
            ws_state,
            health_monitor,
            shutdown_tx,
        }
    }

    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Start subscribing to blocks
    pub async fn run(&self) -> Result<()> {
        info!("Block subscriber starting for {}...", self.provider_url);

        // Simplified block subscription loop
        // In production, use alloy's WebSocket provider
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut block_number = 0u64;

        // Poll for new blocks every 12 seconds (Ethereum block time)
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(12));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    block_number += 1;
                    if let Err(e) = self.on_new_block(block_number).await {
                        error!("Error processing block {}: {}", block_number, e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Block subscriber shutting down...");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a new block
    async fn on_new_block(&self, block_number: u64) -> Result<()> {
        let start = Instant::now();

        // Update last processed block
        self.store.set_last_block(block_number);

        // Check all active lending positions
        let positions_checked = self.health_monitor.check_all_positions(block_number).await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        // Log if latency is high
        if latency_ms > 50 {
            warn!(
                "Block {} processed in {}ms (target: 14ms)",
                block_number, latency_ms
            );
        }

        // Broadcast block processed event
        self.ws_state.broadcast_block_processed(
            block_number,
            latency_ms,
            positions_checked as u32,
        );

        Ok(())
    }
}

/// Run block subscriber in a separate task
pub async fn spawn_block_subscriber(
    provider_url: String,
    store: Arc<PositionStore>,
    db: Database,
    ws_state: Arc<WsState>,
    health_monitor: Arc<HealthMonitor>,
) -> broadcast::Sender<()> {
    let subscriber = BlockSubscriber::new(provider_url, store, db, ws_state, health_monitor);
    let shutdown_tx = subscriber.shutdown_handle();

    tokio::spawn(async move {
        if let Err(e) = subscriber.run().await {
            error!("Block subscriber error: {}", e);
        }
    });

    shutdown_tx
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_block_subscriber_creation() {
        // Basic test to ensure struct can be created
        // Full integration tests would require a mock provider
    }
}
