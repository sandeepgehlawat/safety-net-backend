use crate::alerter::AlertService;
use crate::api::WsState;
use crate::data::{Database, LpPosition, PositionStore};
use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::info;

/// Monitors Uniswap V3 LP positions for range status
pub struct LpMonitor {
    store: Arc<PositionStore>,
    db: Database,
    ws_state: Arc<WsState>,
    alerter: Arc<AlertService>,
}

impl LpMonitor {
    pub fn new(
        store: Arc<PositionStore>,
        db: Database,
        ws_state: Arc<WsState>,
        alerter: Arc<AlertService>,
    ) -> Self {
        Self {
            store,
            db,
            ws_state,
            alerter,
        }
    }

    /// Check all active LP positions
    pub async fn check_all_positions(&self, block_number: u64) -> Result<usize> {
        let positions = self.store.get_active_lp_positions();
        let count = positions.len();

        for pos in positions {
            if let Err(e) = self.check_position(&pos, block_number).await {
                tracing::error!("Error checking LP position {}: {}", pos.id, e);
            }
        }

        Ok(count)
    }

    /// Check a single LP position
    async fn check_position(&self, position: &LpPosition, block_number: u64) -> Result<()> {
        let in_range = position.in_range.unwrap_or(true);
        let was_in_range = position.in_range; // Previous state

        // Broadcast position update
        self.ws_state.broadcast_position_update(
            position.id,
            "lp",
            None,
            Some(in_range),
            block_number,
        );

        // Alert if position went out of range
        if !in_range && was_in_range == Some(true) {
            self.handle_out_of_range(position).await?;
        }

        Ok(())
    }

    /// Handle a position that went out of range
    async fn handle_out_of_range(&self, position: &LpPosition) -> Result<()> {
        info!(
            "LP position {} went out of range (tick {} not in [{}, {}])",
            position.id,
            position.current_tick.unwrap_or(0),
            position.lower_tick,
            position.upper_tick
        );

        // Create alert
        let alert = self.db.create_alert(
            position.user_id,
            position.id,
            "lp",
            "out_of_range",
            Decimal::from(position.current_tick.unwrap_or(0)),
            Decimal::from(0), // No previous value
            Decimal::from(0), // Threshold N/A
        ).await?;

        // Send notification
        self.alerter.send_out_of_range_alert(
            position.user_id,
            alert.id,
            position,
        ).await?;

        // WebSocket notification
        self.ws_state.send_alert_to_user(
            position.user_id,
            alert.id,
            position.id,
            "out_of_range",
            position.current_tick.unwrap_or(0) as f64,
            0.0,
            None,
        ).await;

        Ok(())
    }

    /// Update LP position from protocol
    pub fn update_range_status(
        &self,
        position_id: uuid::Uuid,
        in_range: bool,
        current_tick: i32,
        block_number: u64,
    ) {
        self.store.update_lp_range_status(position_id, in_range, current_tick, block_number);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_in_range_detection() {
        let lower_tick: i32 = 100;
        let upper_tick: i32 = 200;
        let current_tick: i32 = 150;

        let in_range = current_tick >= lower_tick && current_tick <= upper_tick;
        assert!(in_range);
    }

    #[test]
    fn test_out_of_range_detection() {
        let lower_tick: i32 = 100;
        let upper_tick: i32 = 200;
        let current_tick: i32 = 250;

        let in_range = current_tick >= lower_tick && current_tick <= upper_tick;
        assert!(!in_range);
    }
}
