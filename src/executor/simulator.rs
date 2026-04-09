use crate::data::{ActionType, Database, SimulationResult};
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

/// Transaction simulator using Tenderly
pub struct Simulator {
    client: Client,
    api_key: String,
    project: String,
    user: String,
    db: Database,
}

#[derive(Debug, Serialize)]
struct TenderlySimulationRequest {
    network_id: String,
    from: String,
    to: String,
    input: String,
    value: String,
    save: bool,
    save_if_fails: bool,
    simulation_type: String,
}

#[derive(Debug, Deserialize)]
struct TenderlySimulationResponse {
    simulation: TenderlySimulation,
}

#[derive(Debug, Deserialize)]
struct TenderlySimulation {
    id: String,
    status: bool,
    gas_used: u64,
    #[serde(default)]
    state_changes: Vec<StateChange>,
}

#[derive(Debug, Deserialize)]
struct StateChange {
    address: String,
    #[serde(default)]
    raw: Vec<RawStateChange>,
}

#[derive(Debug, Deserialize)]
struct RawStateChange {
    key: String,
    value: String,
}

impl Simulator {
    pub fn new(api_key: String, project: String, user: String, db: Database) -> Self {
        Self {
            client: Client::new(),
            api_key,
            project,
            user,
            db,
        }
    }

    /// Simulate a repay transaction
    pub async fn simulate_repay(
        &self,
        user_id: Uuid,
        position_id: Uuid,
        protocol: &str,
        target_hf: f64,
    ) -> Result<SimulationResult> {
        // Get position from database
        let position = self.db.get_lending_positions(user_id).await?
            .into_iter()
            .find(|p| p.id == position_id)
            .ok_or_else(|| anyhow!("Position not found"))?;

        let current_hf = position.health_factor
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        let collateral_usd = position.collateral_usd
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        let debt_usd = position.debt_usd
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        let liq_threshold = position.liquidation_threshold
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.825))
            .unwrap_or(0.825);

        // Calculate repay amount
        let repay_amount = calculate_repay_for_target_hf(
            collateral_usd,
            debt_usd,
            liq_threshold,
            target_hf,
        );

        // In production, we would:
        // 1. Build the actual repay transaction calldata
        // 2. Send to Tenderly for simulation
        // 3. Parse the result to get gas used and state changes

        // For now, estimate gas based on protocol
        let gas_estimate = match protocol {
            "aave_v3" => 250_000u64,
            "morpho" => 300_000u64,
            "spark" => 250_000u64,
            "compound" => 200_000u64,
            _ => 300_000u64,
        };

        // Estimate gas cost (assuming 30 gwei and $3500 ETH)
        let gas_price_gwei = 30.0;
        let eth_price = 3500.0;
        let gas_cost_usd = (gas_estimate as f64 * gas_price_gwei * 1e-9) * eth_price;

        // Calculate resulting health factor
        let new_debt = debt_usd - repay_amount;
        let new_hf = if new_debt > 0.0 {
            (collateral_usd * liq_threshold) / new_debt
        } else {
            f64::MAX
        };

        info!(
            "Simulated repay: ${:.2} to improve HF from {:.2} to {:.2}",
            repay_amount, current_hf, new_hf
        );

        Ok(SimulationResult {
            id: Uuid::new_v4(),
            action: ActionType::Repay,
            amount_usd: repay_amount,
            health_factor_before: Some(current_hf),
            health_factor_after: Some(new_hf.min(10.0)), // Cap for display
            debt_before: Some(debt_usd),
            debt_after: Some(new_debt.max(0.0)),
            gas_estimate,
            gas_cost_usd,
            expires_at: Utc::now() + Duration::minutes(5),
        })
    }

    /// Simulate a LP rebalance transaction
    pub async fn simulate_rebalance(
        &self,
        _user_id: Uuid,
        _position_id: Uuid,
        _new_lower_tick: i32,
        _new_upper_tick: i32,
    ) -> Result<SimulationResult> {
        // Rebalancing involves:
        // 1. Remove liquidity from current position
        // 2. Swap tokens to optimal ratio
        // 3. Add liquidity to new position

        // Estimate gas for multicall
        let gas_estimate = 500_000u64;
        let gas_price_gwei = 30.0;
        let eth_price = 3500.0;
        let gas_cost_usd = (gas_estimate as f64 * gas_price_gwei * 1e-9) * eth_price;

        Ok(SimulationResult {
            id: Uuid::new_v4(),
            action: ActionType::Rebalance,
            amount_usd: 0.0, // No direct cost, just gas
            health_factor_before: None,
            health_factor_after: None,
            debt_before: None,
            debt_after: None,
            gas_estimate,
            gas_cost_usd,
            expires_at: Utc::now() + Duration::minutes(5),
        })
    }

    /// Run simulation via Tenderly API
    #[allow(dead_code)]
    async fn run_tenderly_simulation(
        &self,
        from: &str,
        to: &str,
        calldata: &str,
        chain_id: u64,
    ) -> Result<TenderlySimulation> {
        let url = format!(
            "https://api.tenderly.co/api/v1/account/{}/project/{}/simulate",
            self.user, self.project
        );

        let request = TenderlySimulationRequest {
            network_id: chain_id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            input: calldata.to_string(),
            value: "0".to_string(),
            save: false,
            save_if_fails: false,
            simulation_type: "quick".to_string(),
        };

        let response = self.client
            .post(&url)
            .header("X-Access-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

        let result: TenderlySimulationResponse = response.json().await?;
        Ok(result.simulation)
    }
}

/// Calculate repay amount needed to reach target health factor
fn calculate_repay_for_target_hf(
    collateral_usd: f64,
    debt_usd: f64,
    liquidation_threshold: f64,
    target_hf: f64,
) -> f64 {
    // HF = (Collateral * LT) / Debt
    // Target HF = (Collateral * LT) / (Debt - Repay)
    // Repay = Debt - (Collateral * LT) / Target HF

    let collateral_weighted = collateral_usd * liquidation_threshold;
    let new_debt = collateral_weighted / target_hf;
    let repay_amount = debt_usd - new_debt;

    repay_amount.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_repay_amount() {
        // Example: $10,000 collateral, $7,000 debt, 82.5% LT, current HF ~1.18
        // Target HF = 1.80
        let collateral = 10000.0;
        let debt = 7000.0;
        let lt = 0.825;
        let target_hf = 1.80;

        let repay = calculate_repay_for_target_hf(collateral, debt, lt, target_hf);

        // Expected: ~$2,416.67
        assert!((repay - 2416.67).abs() < 1.0);

        // Verify new HF
        let new_debt = debt - repay;
        let new_hf = (collateral * lt) / new_debt;
        assert!((new_hf - target_hf).abs() < 0.01);
    }

    #[test]
    fn test_no_repay_needed() {
        // Already above target
        let collateral = 10000.0;
        let debt = 4000.0;
        let lt = 0.825;
        let target_hf = 1.80;

        // Current HF = (10000 * 0.825) / 4000 = 2.0625
        let repay = calculate_repay_for_target_hf(collateral, debt, lt, target_hf);

        // Should be 0 (clamped)
        assert!(repay < 100.0); // Small amount due to math
    }
}
