//! Test utilities for safety-net-backend
//!
//! Provides mock blockchain providers, test database setup, and fixtures.

pub mod mock_provider;

use alloy::primitives::{Address, U256};

/// Create a test lending position data
pub fn create_test_lending_position(
    wallet: &str,
    health_factor: f64,
    collateral_usd: f64,
    debt_usd: f64,
) -> crate::protocols::LendingPositionData {
    crate::protocols::LendingPositionData {
        wallet: wallet.parse().unwrap_or(Address::ZERO),
        health_factor,
        collateral_usd,
        debt_usd,
        liquidation_threshold: 0.825,
    }
}

/// Create a test LP position data
pub fn create_test_lp_position(
    token_id: &str,
    lower_tick: i32,
    upper_tick: i32,
    current_tick: i32,
    liquidity: u128,
) -> crate::protocols::LpPositionData {
    crate::protocols::LpPositionData {
        token_id: token_id.to_string(),
        token0: Address::ZERO,
        token1: Address::ZERO,
        fee_tier: 3000,
        lower_tick,
        upper_tick,
        current_tick,
        in_range: current_tick >= lower_tick && current_tick <= upper_tick,
        liquidity,
        lower_price_usd: 0.0,
        upper_price_usd: 0.0,
        current_price_usd: 0.0,
    }
}

/// Mock user account data from Aave V3
#[derive(Debug, Clone, Default)]
pub struct MockAaveUserData {
    pub total_collateral_base: U256,
    pub total_debt_base: U256,
    pub available_borrows_base: U256,
    pub current_liquidation_threshold: U256,
    pub ltv: U256,
    pub health_factor: U256,
}

impl MockAaveUserData {
    /// Create mock data for a healthy position (HF > 1.5)
    pub fn healthy() -> Self {
        Self {
            total_collateral_base: U256::from(10_000_000_000u64), // $10,000 (8 decimals)
            total_debt_base: U256::from(5_000_000_000u64),        // $5,000
            available_borrows_base: U256::from(3_000_000_000u64),
            current_liquidation_threshold: U256::from(8250u64),   // 82.50%
            ltv: U256::from(8000u64),                              // 80%
            health_factor: U256::from(1_650_000_000_000_000_000u128), // 1.65e18
        }
    }

    /// Create mock data for an at-risk position (HF 1.0-1.2)
    pub fn at_risk() -> Self {
        Self {
            total_collateral_base: U256::from(10_000_000_000u64),
            total_debt_base: U256::from(7_500_000_000u64),
            available_borrows_base: U256::ZERO,
            current_liquidation_threshold: U256::from(8250u64),
            ltv: U256::from(8000u64),
            health_factor: U256::from(1_100_000_000_000_000_000u128), // 1.10e18
        }
    }

    /// Create mock data for a no-position user (all zeros)
    pub fn no_position() -> Self {
        Self::default()
    }

    /// Create with custom health factor (18 decimals)
    pub fn with_health_factor(hf_wei: u128) -> Self {
        let mut data = Self::healthy();
        data.health_factor = U256::from(hf_wei);
        data
    }
}

/// Mock Uniswap V3 position data
#[derive(Debug, Clone)]
pub struct MockUniswapPosition {
    pub nonce: u64,
    pub operator: Address,
    pub token0: Address,
    pub token1: Address,
    pub fee: u32,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,
    pub fee_growth_inside0_last_x128: U256,
    pub fee_growth_inside1_last_x128: U256,
    pub tokens_owed0: u128,
    pub tokens_owed1: u128,
}

impl Default for MockUniswapPosition {
    fn default() -> Self {
        Self {
            nonce: 0,
            operator: Address::ZERO,
            token0: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse().unwrap(), // WETH
            token1: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap(), // USDC
            fee: 3000,
            tick_lower: -887220,
            tick_upper: 887220,
            liquidity: 1_000_000_000_000_000_000u128,
            fee_growth_inside0_last_x128: U256::ZERO,
            fee_growth_inside1_last_x128: U256::ZERO,
            tokens_owed0: 0,
            tokens_owed1: 0,
        }
    }
}

impl MockUniswapPosition {
    /// Create an in-range position
    pub fn in_range(current_tick: i32) -> Self {
        Self {
            tick_lower: current_tick - 100,
            tick_upper: current_tick + 100,
            ..Default::default()
        }
    }

    /// Create an out-of-range position (above)
    pub fn out_of_range_above(current_tick: i32) -> Self {
        Self {
            tick_lower: current_tick - 200,
            tick_upper: current_tick - 100,
            ..Default::default()
        }
    }

    /// Create an out-of-range position (below)
    pub fn out_of_range_below(current_tick: i32) -> Self {
        Self {
            tick_lower: current_tick + 100,
            tick_upper: current_tick + 200,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_aave_healthy_position() {
        let data = MockAaveUserData::healthy();
        // Health factor should be 1.65e18
        assert_eq!(data.health_factor, U256::from(1_650_000_000_000_000_000u128));
        // Should have collateral
        assert!(data.total_collateral_base > U256::ZERO);
        // Should have debt
        assert!(data.total_debt_base > U256::ZERO);
    }

    #[test]
    fn test_mock_aave_at_risk_position() {
        let data = MockAaveUserData::at_risk();
        // Health factor should be 1.10e18
        assert_eq!(data.health_factor, U256::from(1_100_000_000_000_000_000u128));
        // No available borrows when at risk
        assert_eq!(data.available_borrows_base, U256::ZERO);
    }

    #[test]
    fn test_mock_aave_no_position() {
        let data = MockAaveUserData::no_position();
        assert_eq!(data.total_collateral_base, U256::ZERO);
        assert_eq!(data.total_debt_base, U256::ZERO);
        assert_eq!(data.health_factor, U256::ZERO);
    }

    #[test]
    fn test_mock_uniswap_in_range() {
        let current_tick = 1000;
        let pos = MockUniswapPosition::in_range(current_tick);
        assert!(pos.tick_lower <= current_tick);
        assert!(pos.tick_upper >= current_tick);
    }

    #[test]
    fn test_mock_uniswap_out_of_range_above() {
        let current_tick = 1000;
        let pos = MockUniswapPosition::out_of_range_above(current_tick);
        // Position is below current tick (price moved up past range)
        assert!(pos.tick_upper < current_tick);
    }

    #[test]
    fn test_mock_uniswap_out_of_range_below() {
        let current_tick = 1000;
        let pos = MockUniswapPosition::out_of_range_below(current_tick);
        // Position is above current tick (price moved down past range)
        assert!(pos.tick_lower > current_tick);
    }

    #[test]
    fn test_create_test_lending_position() {
        let pos = create_test_lending_position(
            "0x1234567890123456789012345678901234567890",
            1.5,
            10000.0,
            5000.0,
        );
        assert_eq!(pos.health_factor, 1.5);
        assert_eq!(pos.collateral_usd, 10000.0);
        assert_eq!(pos.debt_usd, 5000.0);
    }

    #[test]
    fn test_create_test_lp_position() {
        let pos = create_test_lp_position("12345", -100, 100, 50, 1000);
        assert!(pos.in_range);
        assert_eq!(pos.lower_tick, -100);
        assert_eq!(pos.upper_tick, 100);
        assert_eq!(pos.current_tick, 50);
    }

    #[test]
    fn test_create_test_lp_position_out_of_range() {
        let pos = create_test_lp_position("12345", -100, 100, 150, 1000);
        assert!(!pos.in_range);
    }
}
