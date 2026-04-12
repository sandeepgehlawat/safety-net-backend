//! Position Indexer
//!
//! Indexes user positions from Aave V3 and Uniswap V3 protocols.

use crate::data::{Database, PositionStore};
use crate::protocols::{AaveV3Adapter, UniswapV3Adapter};
use alloy::primitives::Address;
use alloy::providers::Provider;
use alloy::transports::Transport;
use anyhow::Result;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Indexes user positions from protocols
pub struct PositionIndexer {
    aave_adapter: AaveV3Adapter,
    uniswap_adapter: UniswapV3Adapter,
    store: Arc<PositionStore>,
    db: Database,
    chain: String,
}

impl PositionIndexer {
    pub fn new(
        chain: &str,
        store: Arc<PositionStore>,
        db: Database,
    ) -> Self {
        Self {
            aave_adapter: AaveV3Adapter::new(chain),
            uniswap_adapter: UniswapV3Adapter::new(chain),
            store,
            db,
            chain: chain.to_string(),
        }
    }

    /// Index all positions for a user using the provided RPC provider
    pub async fn index_user<T, P>(
        &self,
        provider: P,
        user_id: Uuid,
        wallet: &str,
        block_number: i64,
    ) -> Result<IndexResult>
    where
        T: Transport + Clone,
        P: Provider<T> + Clone,
    {
        let wallet_addr: Address = wallet.parse()?;
        let mut result = IndexResult::default();

        // 1. Index Aave V3 positions
        match self.aave_adapter.get_user_account_data(provider.clone(), wallet_addr).await {
            Ok(Some(aave_data)) => {
                // Convert to Decimal for database storage
                let collateral_usd = Decimal::from_str(&format!("{:.8}", aave_data.collateral_usd))?;
                let debt_usd = Decimal::from_str(&format!("{:.8}", aave_data.debt_usd))?;
                let health_factor = Decimal::from_str(&format!("{:.8}", aave_data.health_factor))?;
                let liq_threshold = Decimal::from_str(&format!("{:.4}", aave_data.liquidation_threshold))?;

                self.db.upsert_lending_position(
                    user_id,
                    "aave_v3",
                    &self.chain,
                    collateral_usd,
                    debt_usd,
                    health_factor,
                    liq_threshold,
                    block_number,
                ).await?;

                result.lending_positions += 1;
                info!(
                    "Indexed Aave V3 position for {}: HF={:.2}, collateral=${:.2}, debt=${:.2}",
                    wallet, aave_data.health_factor, aave_data.collateral_usd, aave_data.debt_usd
                );
            }
            Ok(None) => {
                info!("No Aave V3 position found for {}", wallet);
            }
            Err(e) => {
                warn!("Failed to index Aave V3 for {}: {}", wallet, e);
            }
        }

        // 2. Index Uniswap V3 positions
        match self.uniswap_adapter.get_user_positions(provider, wallet_addr).await {
            Ok(positions) => {
                for pos in positions {
                    self.db.upsert_lp_position(
                        user_id,
                        &pos.token_id,
                        &self.chain,
                        &pos.token0.to_string(),
                        &pos.token1.to_string(),
                        pos.fee_tier as i32,
                        pos.lower_tick,
                        pos.upper_tick,
                    ).await?;

                    result.lp_positions += 1;
                    info!(
                        "Indexed Uniswap V3 position #{} for {}: ticks [{}, {}], in_range={}",
                        pos.token_id, wallet, pos.lower_tick, pos.upper_tick, pos.in_range
                    );
                }
            }
            Err(e) => {
                warn!("Failed to index Uniswap V3 for {}: {}", wallet, e);
            }
        }

        // 3. Update in-memory store
        self.store.add_active_wallet(wallet.to_lowercase());

        info!(
            "Indexed {} lending, {} LP positions for {} on {}",
            result.lending_positions, result.lp_positions, wallet, self.chain
        );

        Ok(result)
    }

    /// Index all positions for a user (legacy - no provider, uses stub data)
    #[deprecated(note = "Use index_user with provider instead")]
    pub async fn index_user_stub(&self, _user_id: Uuid, wallet: &str) -> Result<IndexResult> {
        let result = IndexResult::default();

        // Add wallet to active monitoring
        self.store.add_active_wallet(wallet.to_lowercase());

        info!(
            "Indexed {} lending, {} LP positions for {} (stub)",
            result.lending_positions, result.lp_positions, wallet
        );

        Ok(result)
    }

    /// Get aave adapter
    pub fn aave_adapter(&self) -> &AaveV3Adapter {
        &self.aave_adapter
    }

    /// Get uniswap adapter
    pub fn uniswap_adapter(&self) -> &UniswapV3Adapter {
        &self.uniswap_adapter
    }

    /// Get chain name
    pub fn chain(&self) -> &str {
        &self.chain
    }
}

#[derive(Default, Debug)]
pub struct IndexResult {
    pub lending_positions: usize,
    pub lp_positions: usize,
}

impl IndexResult {
    pub fn total(&self) -> usize {
        self.lending_positions + self.lp_positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_result_default() {
        let result = IndexResult::default();
        assert_eq!(result.lending_positions, 0);
        assert_eq!(result.lp_positions, 0);
        assert_eq!(result.total(), 0);
    }

    #[test]
    fn test_index_result_total() {
        let result = IndexResult {
            lending_positions: 2,
            lp_positions: 3,
        };
        assert_eq!(result.total(), 5);
    }
}
