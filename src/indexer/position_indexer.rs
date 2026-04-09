use crate::data::{Database, PositionStore};
use crate::protocols::{AaveV3Adapter, UniswapV3Adapter};
use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Indexes user positions from protocols
pub struct PositionIndexer {
    aave_adapter: AaveV3Adapter,
    uniswap_adapter: UniswapV3Adapter,
    store: Arc<PositionStore>,
    db: Database,
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
        }
    }

    /// Index all positions for a user
    /// In production, this would query the blockchain
    /// For now, returns empty result
    pub async fn index_user(&self, _user_id: Uuid, wallet: &str) -> Result<IndexResult> {
        let result = IndexResult::default();

        // Add wallet to active monitoring
        self.store.add_active_wallet(wallet.to_lowercase());

        info!(
            "Indexed {} lending, {} LP positions for {}",
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
}

#[derive(Default)]
pub struct IndexResult {
    pub lending_positions: usize,
    pub lp_positions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_result_default() {
        let result = IndexResult::default();
        assert_eq!(result.lending_positions, 0);
        assert_eq!(result.lp_positions, 0);
    }
}
