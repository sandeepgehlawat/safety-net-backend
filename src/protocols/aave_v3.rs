use super::{LendingAdapter, LendingPositionData};
use alloy::primitives::Address;

// Aave V3 Pool addresses by chain
pub const AAVE_V3_POOL_ETHEREUM: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";
pub const AAVE_V3_POOL_ARBITRUM: &str = "0x794a61358D6845594F94dc1DB02A252b5b4814aD";
pub const AAVE_V3_POOL_BASE: &str = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5";
pub const AAVE_V3_POOL_OPTIMISM: &str = "0x794a61358D6845594F94dc1DB02A252b5b4814aD";

/// Aave V3 protocol adapter
pub struct AaveV3Adapter {
    pool_address: Address,
    chain: String,
}

impl AaveV3Adapter {
    pub fn new(chain: &str) -> Self {
        let pool_address = match chain {
            "ethereum" => AAVE_V3_POOL_ETHEREUM,
            "arbitrum" => AAVE_V3_POOL_ARBITRUM,
            "base" => AAVE_V3_POOL_BASE,
            "optimism" => AAVE_V3_POOL_OPTIMISM,
            _ => AAVE_V3_POOL_ETHEREUM,
        };

        Self {
            pool_address: pool_address.parse().unwrap(),
            chain: chain.to_string(),
        }
    }

    pub fn pool_address(&self) -> Address {
        self.pool_address
    }
}

impl LendingAdapter for AaveV3Adapter {
    fn protocol_name(&self) -> &'static str {
        "aave_v3"
    }

    fn chain(&self) -> &str {
        &self.chain
    }

    fn calculate_repay_for_target_hf(
        &self,
        position: &LendingPositionData,
        target_hf: f64,
    ) -> f64 {
        // Health Factor = (Collateral * LiquidationThreshold) / Debt
        // Target HF = (Collateral * LT) / (Debt - Repay)
        // Repay = Debt - (Collateral * LT) / Target HF

        let collateral_weighted = position.collateral_usd * position.liquidation_threshold;
        let new_debt = collateral_weighted / target_hf;
        let repay_amount = position.debt_usd - new_debt;

        // Return positive amount (can't repay negative)
        repay_amount.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_repay_for_target_hf() {
        let adapter = AaveV3Adapter::new("ethereum");

        let position = LendingPositionData {
            wallet: Address::ZERO,
            health_factor: 1.18,
            collateral_usd: 10000.0,
            debt_usd: 7000.0,
            liquidation_threshold: 0.825,
        };

        let repay = adapter.calculate_repay_for_target_hf(&position, 1.80);
        // Expected: ~$2,416.67
        assert!((repay - 2416.67).abs() < 1.0);
    }

    #[test]
    fn test_protocol_name() {
        let adapter = AaveV3Adapter::new("ethereum");
        assert_eq!(adapter.protocol_name(), "aave_v3");
    }
}
