//! Aave V3 Protocol Adapter
//!
//! Provides methods to query user positions and health factors from Aave V3 pools.

use super::abi::IAaveV3Pool;
use super::{LendingAdapter, LendingPositionData};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::transports::Transport;
use anyhow::Result;

// Aave V3 Pool addresses by chain
pub const AAVE_V3_POOL_ETHEREUM: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";
pub const AAVE_V3_POOL_ARBITRUM: &str = "0x794a61358D6845594F94dc1DB02A252b5b4814aD";
pub const AAVE_V3_POOL_BASE: &str = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5";
pub const AAVE_V3_POOL_OPTIMISM: &str = "0x794a61358D6845594F94dc1DB02A252b5b4814aD";

// Aave uses 8 decimals for USD values
const AAVE_USD_DECIMALS: u8 = 8;
// Health factor has 18 decimals
const HEALTH_FACTOR_DECIMALS: u8 = 18;
// Liquidation threshold is in basis points (1 = 0.01%)
const LT_BASIS_POINTS: u64 = 10000;

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

    /// Query user position from Aave V3 Pool contract
    ///
    /// Returns `None` if the user has no position (all zeros).
    pub async fn get_user_account_data<T, P>(
        &self,
        provider: P,
        wallet: Address,
    ) -> Result<Option<LendingPositionData>>
    where
        T: Transport + Clone,
        P: Provider<T>,
    {
        // Create contract instance
        let pool = IAaveV3Pool::new(self.pool_address, provider);

        // Call getUserAccountData
        let result = pool.getUserAccountData(wallet).call().await?;

        // Parse the response
        let data = Self::parse_user_data(
            wallet,
            result.totalCollateralBase,
            result.totalDebtBase,
            result.currentLiquidationThreshold,
            result.healthFactor,
        );

        // Return None if no position (zero collateral and debt)
        if data.collateral_usd == 0.0 && data.debt_usd == 0.0 {
            return Ok(None);
        }

        Ok(Some(data))
    }

    /// Parse raw contract response to domain type
    fn parse_user_data(
        wallet: Address,
        total_collateral_base: U256,
        total_debt_base: U256,
        current_liquidation_threshold: U256,
        health_factor: U256,
    ) -> LendingPositionData {
        // Convert from 8 decimals to f64 USD
        let collateral_usd = Self::u256_to_usd(total_collateral_base, AAVE_USD_DECIMALS);
        let debt_usd = Self::u256_to_usd(total_debt_base, AAVE_USD_DECIMALS);

        // Convert liquidation threshold from basis points to decimal (8250 -> 0.825)
        let lt_bps: u64 = current_liquidation_threshold.try_into().unwrap_or(0);
        let liquidation_threshold = lt_bps as f64 / LT_BASIS_POINTS as f64;

        // Convert health factor from 18 decimals to f64
        let health_factor = Self::u256_to_f64(health_factor, HEALTH_FACTOR_DECIMALS);

        LendingPositionData {
            wallet,
            health_factor,
            collateral_usd,
            debt_usd,
            liquidation_threshold,
        }
    }

    /// Convert U256 with decimals to USD f64
    fn u256_to_usd(value: U256, decimals: u8) -> f64 {
        Self::u256_to_f64(value, decimals)
    }

    /// Convert U256 with decimals to f64
    fn u256_to_f64(value: U256, decimals: u8) -> f64 {
        // Handle potential overflow for very large values
        let divisor = 10u128.pow(decimals as u32);

        // For values that fit in u128
        if value <= U256::from(u128::MAX) {
            let val: u128 = value.try_into().unwrap_or(0);
            return val as f64 / divisor as f64;
        }

        // For larger values, use string conversion
        let value_str = value.to_string();
        value_str.parse::<f64>().unwrap_or(0.0) / divisor as f64
    }

    /// Check if health factor indicates overflow (user has no debt)
    pub fn is_health_factor_max(hf: U256) -> bool {
        // Aave returns type(uint256).max when user has no debt
        hf == U256::MAX
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

    #[test]
    fn test_parse_user_data_healthy_position() {
        let wallet = Address::ZERO;
        // 8 decimals: $10,000 = 10000 * 10^8 = 1_000_000_000_000
        let collateral = U256::from(1_000_000_000_000u64); // $10,000 (8 decimals)
        let debt = U256::from(500_000_000_000u64); // $5,000
        let lt = U256::from(8250u64); // 82.50%
        let hf = U256::from(1_650_000_000_000_000_000u128); // 1.65e18

        let data = AaveV3Adapter::parse_user_data(wallet, collateral, debt, lt, hf);

        assert!((data.collateral_usd - 10000.0).abs() < 0.01);
        assert!((data.debt_usd - 5000.0).abs() < 0.01);
        assert!((data.liquidation_threshold - 0.825).abs() < 0.001);
        assert!((data.health_factor - 1.65).abs() < 0.01);
    }

    #[test]
    fn test_parse_user_data_at_risk_position() {
        let wallet = Address::ZERO;
        // 8 decimals: $10,000 = 1_000_000_000_000, $7,500 = 750_000_000_000
        let collateral = U256::from(1_000_000_000_000u64);
        let debt = U256::from(750_000_000_000u64);
        let lt = U256::from(8250u64);
        let hf = U256::from(1_100_000_000_000_000_000u128); // 1.10e18

        let data = AaveV3Adapter::parse_user_data(wallet, collateral, debt, lt, hf);

        assert!((data.health_factor - 1.10).abs() < 0.01);
        // Position is at risk when HF < 1.2
        assert!(data.health_factor < 1.2);
    }

    #[test]
    fn test_parse_user_data_no_position() {
        let wallet = Address::ZERO;
        let data = AaveV3Adapter::parse_user_data(
            wallet,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
        );

        assert_eq!(data.collateral_usd, 0.0);
        assert_eq!(data.debt_usd, 0.0);
        assert_eq!(data.health_factor, 0.0);
    }

    #[test]
    fn test_parse_user_data_precision() {
        // Test with fractional USD values (8 decimals)
        // $12.34567890 = 12.34567890 * 10^8 = 1234567890
        let wallet = Address::ZERO;
        let collateral = U256::from(1_234_567_890u64); // $12.34567890
        let debt = U256::from(567_890_123u64); // $5.67890123
        let lt = U256::from(8000u64); // 80%
        let hf = U256::from(1_738_000_000_000_000_000u128); // 1.738

        let data = AaveV3Adapter::parse_user_data(wallet, collateral, debt, lt, hf);

        assert!((data.collateral_usd - 12.34567890).abs() < 0.0001);
        assert!((data.debt_usd - 5.67890123).abs() < 0.0001);
        assert!((data.liquidation_threshold - 0.80).abs() < 0.001);
    }

    #[test]
    fn test_health_factor_overflow_protection() {
        // When user has no debt, Aave returns uint256.max
        assert!(AaveV3Adapter::is_health_factor_max(U256::MAX));
        assert!(!AaveV3Adapter::is_health_factor_max(U256::from(1_000_000_000_000_000_000u128)));
    }

    #[test]
    fn test_u256_to_usd_various_values() {
        // Zero
        assert_eq!(AaveV3Adapter::u256_to_usd(U256::ZERO, 8), 0.0);

        // Small value
        let small = U256::from(100_000_000u64); // $1.00
        assert!((AaveV3Adapter::u256_to_usd(small, 8) - 1.0).abs() < 0.001);

        // Large value
        let large = U256::from(1_000_000_000_000u64); // $10,000
        assert!((AaveV3Adapter::u256_to_usd(large, 8) - 10000.0).abs() < 0.01);
    }

    #[test]
    fn test_pool_addresses() {
        let eth = AaveV3Adapter::new("ethereum");
        assert_eq!(
            eth.pool_address().to_string().to_lowercase(),
            "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"
        );

        let arb = AaveV3Adapter::new("arbitrum");
        assert_eq!(
            arb.pool_address().to_string().to_lowercase(),
            "0x794a61358d6845594f94dc1db02a252b5b4814ad"
        );

        let base = AaveV3Adapter::new("base");
        assert_eq!(
            base.pool_address().to_string().to_lowercase(),
            "0xa238dd80c259a72e81d7e4664a9801593f98d1c5"
        );
    }
}
