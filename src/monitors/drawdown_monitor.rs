use crate::alerter::AlertService;
use crate::api::WsState;
use crate::data::{Database, PositionStore, TokenWatch};
use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::info;

/// Monitors token prices for drawdown alerts
pub struct DrawdownMonitor {
    store: Arc<PositionStore>,
    db: Database,
    ws_state: Arc<WsState>,
    alerter: Arc<AlertService>,
}

impl DrawdownMonitor {
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

    /// Check all token watchlist entries
    pub async fn check_all_tokens(&self) -> Result<usize> {
        let watches = self.store.get_all_token_watches();
        let count = watches.len();

        for watch in watches {
            if let Err(e) = self.check_token(&watch).await {
                tracing::error!("Error checking token {}: {}", watch.token_address, e);
            }
        }

        Ok(count)
    }

    /// Check a single token
    async fn check_token(&self, watch: &TokenWatch) -> Result<()> {
        let reference_price = watch.reference_price_usd
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        let current_price = watch.current_price_usd
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        // Skip if no prices
        if reference_price == 0.0 || current_price == 0.0 {
            return Ok(());
        }

        // Calculate drawdown
        let change_pct = (current_price - reference_price) / reference_price * 100.0;
        let threshold = watch.alert_threshold_pct.to_string().parse::<f64>().unwrap_or(-20.0);

        // Update in store
        self.store.update_token_price(watch.id, current_price, change_pct);

        // Update in database
        self.db.update_token_price(
            watch.id,
            Decimal::from_f64_retain(current_price).unwrap_or_default(),
            Decimal::from_f64_retain(change_pct).unwrap_or_default(),
        ).await?;

        // Determine status
        let _status = if change_pct >= 0.0 {
            "ok"
        } else if change_pct > -10.0 {
            "ok"
        } else if change_pct > -20.0 {
            "warn"
        } else {
            "bad"
        };

        // Broadcast token update
        self.ws_state.broadcast_token_update(
            watch.symbol.as_deref().unwrap_or("???"),
            current_price,
            change_pct,
        );

        // Alert if threshold breached
        if change_pct <= threshold {
            self.handle_drawdown_alert(watch, current_price, change_pct, threshold).await?;
        }

        Ok(())
    }

    /// Handle a drawdown alert
    async fn handle_drawdown_alert(
        &self,
        watch: &TokenWatch,
        current_price: f64,
        change_pct: f64,
        threshold: f64,
    ) -> Result<()> {
        info!(
            "Drawdown alert: {} dropped {}% (threshold {}%)",
            watch.symbol.as_deref().unwrap_or(&watch.token_address),
            change_pct,
            threshold
        );

        // Create alert
        let alert = self.db.create_alert(
            watch.user_id,
            watch.id, // Using watch ID as position ID
            "token",
            "drawdown",
            Decimal::from_f64_retain(change_pct).unwrap_or_default(),
            Decimal::from_f64_retain(0.0).unwrap_or_default(),
            Decimal::from_f64_retain(threshold).unwrap_or_default(),
        ).await?;

        // Send notification
        self.alerter.send_drawdown_alert(
            watch.user_id,
            alert.id,
            watch.symbol.as_deref().unwrap_or(&watch.token_address),
            current_price,
            change_pct,
        ).await?;

        Ok(())
    }

    /// Update token price from external source (e.g., Chainlink)
    pub async fn update_price(&self, watch_id: uuid::Uuid, price_usd: f64) -> Result<()> {
        if let Some(watch) = self.store.get_token_watch(watch_id) {
            let reference_price = watch.reference_price_usd
                .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
                .unwrap_or(price_usd);

            let change_pct = (price_usd - reference_price) / reference_price * 100.0;

            self.store.update_token_price(watch_id, price_usd, change_pct);
            self.db.update_token_price(
                watch_id,
                Decimal::from_f64_retain(price_usd).unwrap_or_default(),
                Decimal::from_f64_retain(change_pct).unwrap_or_default(),
            ).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_drawdown_calculation() {
        let reference_price: f64 = 100.0;
        let current_price: f64 = 80.0;

        let change_pct = (current_price - reference_price) / reference_price * 100.0;
        assert!((change_pct - (-20.0_f64)).abs() < 0.0001);
    }

    #[test]
    fn test_status_determination() {
        // Positive change
        assert_eq!(determine_status(5.0), "ok");

        // Small negative
        assert_eq!(determine_status(-5.0), "ok");

        // Medium negative
        assert_eq!(determine_status(-15.0), "warn");

        // Large negative
        assert_eq!(determine_status(-25.0), "bad");
    }

    fn determine_status(change_pct: f64) -> &'static str {
        if change_pct >= 0.0 {
            "ok"
        } else if change_pct > -10.0 {
            "ok"
        } else if change_pct > -20.0 {
            "warn"
        } else {
            "bad"
        }
    }
}
