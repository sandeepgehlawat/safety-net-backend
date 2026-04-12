//! Executor Orchestrator
//!
//! Coordinates the full autopilot execution pipeline:
//! simulate -> sign -> submit -> confirm

use crate::api::WsState;
use crate::data::{Database, WsMessage};
use crate::executor::{CalldataBuilder, GuardianSigner, Simulator, TxSubmitter, TxState};

use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::Provider;
use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Action types that can be executed via autopilot
#[derive(Debug, Clone)]
pub enum AutopilotAction {
    /// Repay debt on a lending position
    Repay {
        user_id: Uuid,
        position_id: Uuid,
        wallet: Address,
        pool: Address,
        asset: Address,
        amount: U256,
        amount_usd: f64,
        rate_mode: u8,
    },
    /// Rebalance an LP position
    Rebalance {
        user_id: Uuid,
        position_id: Uuid,
        wallet: Address,
        token_id: U256,
        new_lower_tick: i32,
        new_upper_tick: i32,
    },
}

impl AutopilotAction {
    pub fn user_id(&self) -> Uuid {
        match self {
            AutopilotAction::Repay { user_id, .. } => *user_id,
            AutopilotAction::Rebalance { user_id, .. } => *user_id,
        }
    }

    pub fn position_id(&self) -> Uuid {
        match self {
            AutopilotAction::Repay { position_id, .. } => *position_id,
            AutopilotAction::Rebalance { position_id, .. } => *position_id,
        }
    }

    pub fn action_name(&self) -> &'static str {
        match self {
            AutopilotAction::Repay { .. } => "repay",
            AutopilotAction::Rebalance { .. } => "rebalance",
        }
    }

    pub fn protocol(&self) -> &'static str {
        match self {
            AutopilotAction::Repay { .. } => "aave_v3",
            AutopilotAction::Rebalance { .. } => "uniswap_v3",
        }
    }
}

/// Result of a successfully executed transaction
#[derive(Debug, Clone)]
pub struct TxResult {
    pub tx_id: Uuid,
    pub tx_hash: String,
    pub gas_used: u64,
    pub gas_cost_usd: f64,
    pub state: TxState,
}

/// Orchestrates the full autopilot execution pipeline
pub struct ExecutorOrchestrator {
    simulator: Arc<Simulator>,
    signer: Arc<GuardianSigner>,
    submitter: Arc<TxSubmitter>,
    db: Database,
    ws_state: Arc<WsState>,
    chain_id: u64,
}

impl ExecutorOrchestrator {
    pub fn new(
        simulator: Arc<Simulator>,
        signer: Arc<GuardianSigner>,
        submitter: Arc<TxSubmitter>,
        db: Database,
        ws_state: Arc<WsState>,
        chain_id: u64,
    ) -> Self {
        Self {
            simulator,
            signer,
            submitter,
            db,
            ws_state,
            chain_id,
        }
    }

    /// Execute a full autopilot action pipeline
    ///
    /// 1. Build calldata
    /// 2. Simulate via Tenderly
    /// 3. Verify simulation success
    /// 4. Check authorization
    /// 5. Sign transaction
    /// 6. Submit to mempool
    /// 7. Wait for confirmation
    pub async fn execute_autopilot_action<P>(
        &self,
        provider: P,
        action: AutopilotAction,
        alert_id: Option<Uuid>,
    ) -> Result<TxResult>
    where
        P: Provider + Clone + Send + Sync + 'static,
    {
        let user_id = action.user_id();
        let action_name = action.action_name();
        let protocol = action.protocol();

        info!(
            "Starting autopilot {} for user {} on {}",
            action_name, user_id, protocol
        );

        // 1. Build calldata
        let (calldata, target, amount_usd) = self.build_action_calldata(&action)?;

        // 2. Simulate via Tenderly
        let simulation = match &action {
            AutopilotAction::Repay { position_id, .. } => {
                self.simulator.simulate_repay(user_id, *position_id, protocol, 1.8).await?
            }
            AutopilotAction::Rebalance { position_id, new_lower_tick, new_upper_tick, .. } => {
                self.simulator.simulate_rebalance(user_id, *position_id, *new_lower_tick, *new_upper_tick).await?
            }
        };

        // Notify user of simulation result
        self.ws_state.send_to_user(
            user_id,
            WsMessage::TickerEvent {
                event_type: "simulation_complete".to_string(),
                message: format!("{} simulation: ${:.2} gas cost", action_name, simulation.gas_cost_usd),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            },
        ).await;

        // 3. Create transaction record
        let tx = self.db.create_transaction(
            user_id,
            alert_id,
            "ethereum", // TODO: get from chain_id
            action_name,
            Decimal::from_f64_retain(amount_usd).unwrap_or_default(),
            simulation.gas_estimate as i64,
            true, // is_autopilot
        ).await?;

        let tx_id = tx.id;

        // 4. Check authorization
        let authorized = self.signer.check_authorization(
            user_id,
            action_name,
            amount_usd,
            protocol,
        ).await?;

        if !authorized {
            let state = TxState::failed("Not authorized", "authorization");
            self.db.update_transaction_failed(tx_id).await?;
            self.notify_tx_status(user_id, tx_id, &state).await;
            return Err(anyhow!("Action not authorized for user"));
        }

        // 5. Sign transaction
        let signed_bytes = match self.sign_transaction(&action, &calldata, target).await {
            Ok(bytes) => bytes,
            Err(e) => {
                let state = TxState::failed(e.to_string(), "signing");
                self.db.update_transaction_failed(tx_id).await?;
                self.notify_tx_status(user_id, tx_id, &state).await;
                return Err(e);
            }
        };

        // 6. Submit transaction
        let tx_hash = match self.submitter.submit_public(&provider, tx_id, user_id, &signed_bytes).await {
            Ok(hash) => hash,
            Err(e) => {
                let state = TxState::failed(e.to_string(), "submission");
                self.db.update_transaction_failed(tx_id).await?;
                self.notify_tx_status(user_id, tx_id, &state).await;
                return Err(e);
            }
        };

        info!("Transaction {} submitted: {}", tx_id, tx_hash);

        // 7. Wait for confirmation
        match self.submitter.wait_for_confirmation(&provider, tx_id, user_id, &tx_hash).await {
            Ok(()) => {
                let state = TxState::confirmed(tx_hash.clone(), 0, simulation.gas_estimate);
                info!("Transaction {} confirmed", tx_id);

                Ok(TxResult {
                    tx_id,
                    tx_hash,
                    gas_used: simulation.gas_estimate,
                    gas_cost_usd: simulation.gas_cost_usd,
                    state,
                })
            }
            Err(e) => {
                let state = TxState::failed(e.to_string(), "confirmation");
                self.db.update_transaction_failed(tx_id).await?;
                self.notify_tx_status(user_id, tx_id, &state).await;
                Err(e)
            }
        }
    }

    /// Build calldata for an action
    fn build_action_calldata(&self, action: &AutopilotAction) -> Result<(Bytes, Address, f64)> {
        match action {
            AutopilotAction::Repay {
                wallet,
                pool,
                asset,
                amount,
                amount_usd,
                rate_mode,
                ..
            } => {
                let calldata = CalldataBuilder::build_aave_repay(
                    *asset,
                    *amount,
                    *rate_mode,
                    *wallet,
                );
                Ok((calldata, *pool, *amount_usd))
            }
            AutopilotAction::Rebalance { .. } => {
                // Rebalance involves multiple calls (decrease, swap, increase)
                // For now, return a placeholder
                Err(anyhow!("Rebalance not yet implemented"))
            }
        }
    }

    /// Sign a transaction with the guardian wallet
    async fn sign_transaction(
        &self,
        action: &AutopilotAction,
        _calldata: &Bytes,
        _target: Address,
    ) -> Result<Vec<u8>> {
        // In production, this would:
        // 1. Build TransactionRequest with calldata
        // 2. Estimate gas
        // 3. Set nonce
        // 4. Sign with guardian wallet

        // For now, delegate to signer
        self.signer.execute_autopilot(
            action.user_id(),
            action.action_name(),
            match action {
                AutopilotAction::Repay { amount_usd, .. } => *amount_usd,
                AutopilotAction::Rebalance { .. } => 0.0,
            },
            action.protocol(),
            &[],
        ).await?;

        // Return dummy signed bytes (in production, this would be actual signed tx)
        Ok(vec![0u8; 32])
    }

    /// Notify user of transaction status via WebSocket
    async fn notify_tx_status(&self, user_id: Uuid, tx_id: Uuid, state: &TxState) {
        self.ws_state.send_tx_status_to_user(
            user_id,
            tx_id,
            state.name(),
            state.tx_hash().map(|s| s.to_string()),
            None,
        ).await;
    }

    /// Check if autopilot is enabled for user
    pub async fn is_autopilot_enabled(&self, user_id: Uuid) -> Result<bool> {
        let user = self.db.get_user(user_id).await?
            .ok_or_else(|| anyhow!("User not found"))?;
        Ok(user.autopilot_enabled)
    }

    /// Get remaining daily budget for user
    pub async fn remaining_budget(&self, user_id: Uuid) -> Result<f64> {
        let user = self.db.get_user(user_id).await?
            .ok_or_else(|| anyhow!("User not found"))?;

        let budget: f64 = user.autopilot_budget_usd
            .map(|d| d.to_string().parse().unwrap_or(0.0))
            .unwrap_or(0.0);
        let spent: f64 = user.autopilot_daily_spent_usd.to_string().parse().unwrap_or(0.0);

        Ok((budget - spent).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autopilot_action_properties() {
        let user_id = Uuid::new_v4();
        let position_id = Uuid::new_v4();

        let repay = AutopilotAction::Repay {
            user_id,
            position_id,
            wallet: Address::ZERO,
            pool: Address::ZERO,
            asset: Address::ZERO,
            amount: U256::ZERO,
            amount_usd: 1000.0,
            rate_mode: 2,
        };

        assert_eq!(repay.user_id(), user_id);
        assert_eq!(repay.position_id(), position_id);
        assert_eq!(repay.action_name(), "repay");
        assert_eq!(repay.protocol(), "aave_v3");
    }

    #[test]
    fn test_rebalance_action_properties() {
        let user_id = Uuid::new_v4();
        let position_id = Uuid::new_v4();

        let rebalance = AutopilotAction::Rebalance {
            user_id,
            position_id,
            wallet: Address::ZERO,
            token_id: U256::ZERO,
            new_lower_tick: -100,
            new_upper_tick: 100,
        };

        assert_eq!(rebalance.user_id(), user_id);
        assert_eq!(rebalance.action_name(), "rebalance");
        assert_eq!(rebalance.protocol(), "uniswap_v3");
    }

    #[test]
    fn test_tx_result() {
        let result = TxResult {
            tx_id: Uuid::new_v4(),
            tx_hash: "0x123".to_string(),
            gas_used: 250_000,
            gas_cost_usd: 15.50,
            state: TxState::confirmed("0x123".to_string(), 12345, 250_000),
        };

        assert_eq!(result.tx_hash, "0x123");
        assert_eq!(result.gas_used, 250_000);
        assert!(result.state.is_terminal());
    }
}
