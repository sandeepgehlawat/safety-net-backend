pub mod abi;
pub mod aave_v3;
pub mod uniswap_v3;

use alloy::primitives::Address;

// ABI module provides contract interfaces (IAaveV3Pool, IUniswapV3Pool, etc.)
// Import specific interfaces as needed in adapter modules
pub use aave_v3::AaveV3Adapter;
pub use uniswap_v3::UniswapV3Adapter;

/// Get chain ID from chain name
pub fn chain_id(chain: &str) -> u64 {
    match chain {
        "ethereum" | "mainnet" => 1,
        "arbitrum" | "arbitrum_one" => 42161,
        "base" => 8453,
        "optimism" => 10,
        "polygon" => 137,
        _ => 1,
    }
}

/// Health factor and position data from a lending protocol
#[derive(Debug, Clone)]
pub struct LendingPositionData {
    pub wallet: Address,
    pub health_factor: f64,
    pub collateral_usd: f64,
    pub debt_usd: f64,
    pub liquidation_threshold: f64,
}

/// LP position data from Uniswap V3
#[derive(Debug, Clone)]
pub struct LpPositionData {
    pub token_id: String,
    pub token0: Address,
    pub token1: Address,
    pub fee_tier: u32,
    pub lower_tick: i32,
    pub upper_tick: i32,
    pub current_tick: i32,
    pub in_range: bool,
    pub liquidity: u128,
    pub lower_price_usd: f64,
    pub upper_price_usd: f64,
    pub current_price_usd: f64,
}

/// Trait for lending protocol adapters
pub trait LendingAdapter: Send + Sync {
    /// Get protocol name
    fn protocol_name(&self) -> &'static str;

    /// Get chain name
    fn chain(&self) -> &str;

    /// Calculate repay amount needed to reach target health factor
    fn calculate_repay_for_target_hf(
        &self,
        position: &LendingPositionData,
        target_hf: f64,
    ) -> f64;
}

/// Trait for LP protocol adapters
pub trait LpAdapter: Send + Sync {
    /// Get protocol name
    fn protocol_name(&self) -> &'static str;

    /// Get chain name
    fn chain(&self) -> &str;
}
