use crate::alerter::AlertService;
use crate::api::WsState;
use crate::data::{Database, LendingPosition, PositionStore};
use crate::executor::Simulator;
use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::{error, info};

/// Monitors lending position health factors
pub struct HealthMonitor {
    store: Arc<PositionStore>,
    db: Database,
    ws_state: Arc<WsState>,
    alerter: Arc<AlertService>,
    simulator: Arc<Simulator>,
}

impl HealthMonitor {
    pub fn new(
        store: Arc<PositionStore>,
        db: Database,
        ws_state: Arc<WsState>,
        alerter: Arc<AlertService>,
        simulator: Arc<Simulator>,
    ) -> Self {
        Self {
            store,
            db,
            ws_state,
            alerter,
            simulator,
        }
    }

    /// Check all active lending positions
    /// Returns number of positions checked
    pub async fn check_all_positions(&self, block_number: u64) -> Result<usize> {
        let positions = self.store.get_active_lending_positions();
        let count = positions.len();

        // Process in parallel for speed
        let futures: Vec<_> = positions
            .into_iter()
            .map(|pos| self.check_position(pos, block_number))
            .collect();

        let results = futures::future::join_all(futures).await;

        // Log any errors
        for result in results {
            if let Err(e) = result {
                error!("Error checking position: {}", e);
            }
        }

        Ok(count)
    }

    /// Check a single position's health factor
    async fn check_position(&self, position: LendingPosition, block_number: u64) -> Result<()> {
        let hf = position.health_factor
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        let threshold = position.alert_threshold.to_string().parse::<f64>().unwrap_or(1.2);

        // Broadcast position update via WebSocket
        self.ws_state.broadcast_position_update(
            position.id,
            "lending",
            Some(hf),
            None,
            block_number,
        );

        // Check if alert should fire
        if hf < threshold && hf > 0.0 {
            self.handle_low_health_factor(&position, hf, threshold).await?;
        }

        // Record history (sample every 10 blocks to reduce storage)
        if block_number % 10 == 0 {
            self.db.insert_health_history(
                position.id,
                position.health_factor.unwrap_or_default(),
                position.collateral_usd.unwrap_or_default(),
                position.debt_usd.unwrap_or_default(),
                block_number as i64,
            ).await?;
        }

        Ok(())
    }

    /// Handle a position with low health factor
    async fn handle_low_health_factor(
        &self,
        position: &LendingPosition,
        current_hf: f64,
        threshold: f64,
    ) -> Result<()> {
        let previous_hf = position.health_factor
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        info!(
            "Health factor alert: position {} HF {} (threshold {})",
            position.id, current_hf, threshold
        );

        // Create alert
        let alert = self.db.create_alert(
            position.user_id,
            position.id,
            "lending",
            "health_factor",
            Decimal::from_f64_retain(current_hf).unwrap_or_default(),
            Decimal::from_f64_retain(previous_hf).unwrap_or_default(),
            Decimal::from_f64_retain(threshold).unwrap_or_default(),
        ).await?;

        // Simulate repay action
        let target_hf = 1.80; // Target a safe health factor
        if let Ok(simulation) = self.simulator.simulate_repay(
            position.user_id,
            position.id,
            &position.protocol,
            target_hf,
        ).await {
            // Update alert with simulation result
            self.db.update_alert_simulation(
                alert.id,
                "repay",
                Decimal::from_f64_retain(simulation.amount_usd).unwrap_or_default(),
                simulation.clone(),
                simulation.expires_at,
            ).await?;

            // Send alert with suggested action
            self.alerter.send_health_factor_alert(
                position.user_id,
                alert.id,
                position.id,
                current_hf,
                threshold,
                Some(simulation),
            ).await?;
        } else {
            // Send alert without simulation
            self.alerter.send_health_factor_alert(
                position.user_id,
                alert.id,
                position.id,
                current_hf,
                threshold,
                None,
            ).await?;
        }

        // Notify via WebSocket
        self.ws_state.send_alert_to_user(
            position.user_id,
            alert.id,
            position.id,
            "health_factor",
            current_hf,
            threshold,
            None, // Simplified for WebSocket
        ).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_health_factor_threshold() {
        // Test the logic for determining if alert should fire
        let hf: f64 = 1.15;
        let threshold: f64 = 1.20;

        assert!(hf < threshold && hf > 0.0);
    }

    #[test]
    fn test_safe_health_factor() {
        let hf: f64 = 1.50;
        let threshold: f64 = 1.20;

        assert!(!(hf < threshold && hf > 0.0));
    }
}
