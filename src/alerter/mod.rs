pub mod push;
pub mod telegram;
pub mod email;

use crate::api::WsState;
use crate::data::{Database, LpPosition, SimulationResult, User};
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

pub use push::PushNotifier;
pub use telegram::TelegramBot;
pub use email::EmailSender;

/// Unified alert service
pub struct AlertService {
    push: Option<PushNotifier>,
    telegram: Option<TelegramBot>,
    email: Option<EmailSender>,
    ws_state: Arc<WsState>,
    db: Database,
}

impl AlertService {
    pub fn new(
        push: Option<PushNotifier>,
        telegram: Option<TelegramBot>,
        email: Option<EmailSender>,
        ws_state: Arc<WsState>,
        db: Database,
    ) -> Self {
        Self {
            push,
            telegram,
            email,
            ws_state,
            db,
        }
    }

    /// Get user notification settings
    async fn get_user(&self, user_id: Uuid) -> Option<User> {
        match self.db.get_user(user_id).await {
            Ok(Some(user)) if user.notifications_enabled => Some(user),
            Ok(Some(_)) => {
                info!("Notifications disabled for user {}", user_id);
                None
            }
            Ok(None) => {
                warn!("User {} not found", user_id);
                None
            }
            Err(e) => {
                warn!("Error fetching user {}: {}", user_id, e);
                None
            }
        }
    }

    /// Send health factor alert
    pub async fn send_health_factor_alert(
        &self,
        user_id: Uuid,
        alert_id: Uuid,
        _position_id: Uuid,
        current_hf: f64,
        threshold: f64,
        simulation: Option<SimulationResult>,
    ) -> Result<()> {
        let title = format!("🚨 Critical: HF dropped to {:.2}", current_hf);

        let body = if let Some(ref sim) = simulation {
            format!(
                "Your position is approaching liquidation.\nRepay ${:.0} to bring HF to {:.2}\nGas: ~${:.2}",
                sim.amount_usd,
                sim.health_factor_after.unwrap_or(0.0),
                sim.gas_cost_usd
            )
        } else {
            format!(
                "Your position health factor dropped to {:.2} (threshold: {:.2})",
                current_hf, threshold
            )
        };

        let deep_link = format!("/alert/{}", alert_id);

        info!("Sending health factor alert to user {}: {}", user_id, title);

        // Look up user notification settings
        if let Some(user) = self.get_user(user_id).await {
            // Send push notification if FCM token available
            if let (Some(ref push), Some(ref token)) = (&self.push, &user.fcm_token) {
                push.send_to_token(token, &title, &body, &deep_link).await.ok();
            }

            // Send Telegram if chat ID available
            if let (Some(ref telegram), Some(ref chat_id)) = (&self.telegram, &user.telegram_chat_id) {
                telegram.send_alert_with_actions(chat_id, &format!("*{}*\n\n{}", title, body), alert_id).await.ok();
            }

            // Send email if configured
            if let (Some(ref email_sender), Some(ref user_email)) = (&self.email, &user.email) {
                email_sender.send(user_id, user_email, &title, &body).await.ok();
            }
        }

        // Broadcast via WebSocket (always)
        self.ws_state.broadcast_alert(user_id, &title);

        // Update delivery status in database
        self.db.resolve_alert(alert_id, "notified").await.ok();

        Ok(())
    }

    /// Send LP out-of-range alert
    pub async fn send_out_of_range_alert(
        &self,
        user_id: Uuid,
        alert_id: Uuid,
        position: &LpPosition,
    ) -> Result<()> {
        let title = "⚠️ LP Position Out of Range".to_string();
        let body = format!(
            "Your {} LP position is no longer earning fees.\nCurrent tick: {}\nRange: [{}, {}]",
            position.token_id,
            position.current_tick.unwrap_or(0),
            position.lower_tick,
            position.upper_tick
        );

        let deep_link = format!("/alert/{}", alert_id);

        info!("Sending out-of-range alert to user {}", user_id);

        if let Some(user) = self.get_user(user_id).await {
            if let (Some(ref push), Some(ref token)) = (&self.push, &user.fcm_token) {
                push.send_to_token(token, &title, &body, &deep_link).await.ok();
            }

            if let (Some(ref telegram), Some(ref chat_id)) = (&self.telegram, &user.telegram_chat_id) {
                telegram.send_to_chat(chat_id, &format!("*{}*\n\n{}", title, body)).await.ok();
            }
        }

        self.ws_state.broadcast_alert(user_id, &title);

        Ok(())
    }

    /// Send drawdown alert
    pub async fn send_drawdown_alert(
        &self,
        user_id: Uuid,
        alert_id: Uuid,
        symbol: &str,
        current_price: f64,
        change_pct: f64,
    ) -> Result<()> {
        let title = format!("📉 Drawdown Alert: {} {:.1}%", symbol, change_pct);
        let body = format!(
            "{} dropped {:.1}%\nCurrent price: ${:.2}",
            symbol, change_pct.abs(), current_price
        );

        let deep_link = format!("/alert/{}", alert_id);

        info!("Sending drawdown alert to user {}: {} {:.1}%", user_id, symbol, change_pct);

        if let Some(user) = self.get_user(user_id).await {
            if let (Some(ref push), Some(ref token)) = (&self.push, &user.fcm_token) {
                push.send_to_token(token, &title, &body, &deep_link).await.ok();
            }

            if let (Some(ref telegram), Some(ref chat_id)) = (&self.telegram, &user.telegram_chat_id) {
                telegram.send_to_chat(chat_id, &format!("*{}*\n\n{}", title, body)).await.ok();
            }
        }

        self.ws_state.broadcast_alert(user_id, &title);

        Ok(())
    }

    /// Broadcast ticker event (for landing page)
    pub fn broadcast_saved_event(&self, amount_usd: f64, protocol: &str) {
        self.ws_state.broadcast_ticker_event(
            "saved",
            &format!("${:.0} saved from {} liquidation", amount_usd, protocol),
        );
    }
}
