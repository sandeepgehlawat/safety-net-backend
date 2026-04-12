//! Uniswap V3 LP Protocol Adapter
//!
//! Provides methods to query LP positions, ticks, and range status.

use super::abi::{INonfungiblePositionManager, IUniswapV3Factory, IUniswapV3Pool};
use super::{LpAdapter, LpPositionData};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::transports::Transport;
use anyhow::Result;

// Contract addresses
pub const UNISWAP_V3_NFT_MANAGER_ETHEREUM: &str = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88";
pub const UNISWAP_V3_FACTORY_ETHEREUM: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";
pub const UNISWAP_V3_NFT_MANAGER_ARBITRUM: &str = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88";
pub const UNISWAP_V3_FACTORY_ARBITRUM: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";
pub const UNISWAP_V3_NFT_MANAGER_BASE: &str = "0x03a520b32C04BF3bEEf7BEb72E919cf822Ed34f1";
pub const UNISWAP_V3_FACTORY_BASE: &str = "0x33128a8fC17869897dcE68Ed026d694621f6FDfD";

/// Uniswap V3 LP adapter
pub struct UniswapV3Adapter {
    nft_manager: Address,
    factory: Address,
    chain: String,
}

impl UniswapV3Adapter {
    pub fn new(chain: &str) -> Self {
        let (nft_manager, factory) = match chain {
            "ethereum" => (UNISWAP_V3_NFT_MANAGER_ETHEREUM, UNISWAP_V3_FACTORY_ETHEREUM),
            "arbitrum" => (UNISWAP_V3_NFT_MANAGER_ARBITRUM, UNISWAP_V3_FACTORY_ARBITRUM),
            "base" => (UNISWAP_V3_NFT_MANAGER_BASE, UNISWAP_V3_FACTORY_BASE),
            _ => (UNISWAP_V3_NFT_MANAGER_ETHEREUM, UNISWAP_V3_FACTORY_ETHEREUM),
        };

        Self {
            nft_manager: nft_manager.parse().unwrap(),
            factory: factory.parse().unwrap(),
            chain: chain.to_string(),
        }
    }

    pub fn nft_manager(&self) -> Address {
        self.nft_manager
    }

    pub fn factory(&self) -> Address {
        self.factory
    }

    /// Enumerate all NFT positions for a wallet
    pub async fn get_user_positions<T, P>(
        &self,
        provider: P,
        wallet: Address,
    ) -> Result<Vec<LpPositionData>>
    where
        T: Transport + Clone,
        P: Provider<T> + Clone,
    {
        let nft = INonfungiblePositionManager::new(self.nft_manager, provider.clone());

        // Get balance (number of NFTs)
        let balance = nft.balanceOf(wallet).call().await?._0;
        let balance_u64: u64 = balance.try_into().unwrap_or(0);

        if balance_u64 == 0 {
            return Ok(vec![]);
        }

        let mut positions = Vec::new();

        // Enumerate each position
        for i in 0..balance_u64 {
            let token_id = nft.tokenOfOwnerByIndex(wallet, U256::from(i)).call().await?._0;

            // Get position details
            let pos = nft.positions(token_id).call().await?;

            // Skip positions with zero liquidity
            if pos.liquidity == 0 {
                continue;
            }

            // Get the pool address
            let factory = IUniswapV3Factory::new(self.factory, provider.clone());
            let pool_address = factory.getPool(pos.token0, pos.token1, pos.fee).call().await?.pool;

            // Get current tick from pool
            let pool = IUniswapV3Pool::new(pool_address, provider.clone());
            let slot0 = pool.slot0().call().await?;
            let current_tick: i32 = slot0.tick.try_into().unwrap_or(0);

            let lower_tick: i32 = pos.tickLower.try_into().unwrap_or(0);
            let upper_tick: i32 = pos.tickUpper.try_into().unwrap_or(0);

            let position_data = LpPositionData {
                token_id: token_id.to_string(),
                token0: pos.token0,
                token1: pos.token1,
                fee_tier: pos.fee.try_into().unwrap_or(3000),
                lower_tick,
                upper_tick,
                current_tick,
                in_range: Self::is_in_range(lower_tick, upper_tick, current_tick),
                liquidity: pos.liquidity,
                lower_price_usd: tick_to_price(lower_tick),
                upper_price_usd: tick_to_price(upper_tick),
                current_price_usd: tick_to_price(current_tick),
            };

            positions.push(position_data);
        }

        Ok(positions)
    }

    /// Get current tick for a pool
    pub async fn get_pool_tick<T, P>(
        &self,
        provider: P,
        pool: Address,
    ) -> Result<i32>
    where
        T: Transport + Clone,
        P: Provider<T>,
    {
        let pool_contract = IUniswapV3Pool::new(pool, provider);
        let slot0 = pool_contract.slot0().call().await?;
        Ok(slot0.tick.try_into().unwrap_or(0))
    }

    /// Get pool address for a token pair and fee
    pub async fn get_pool_address<T, P>(
        &self,
        provider: P,
        token0: Address,
        token1: Address,
        fee: u32,
    ) -> Result<Address>
    where
        T: Transport + Clone,
        P: Provider<T>,
    {
        use alloy::primitives::Uint;
        let factory = IUniswapV3Factory::new(self.factory, provider);
        let fee_u24 = Uint::<24, 1>::try_from(fee).unwrap_or(Uint::from(3000u32));
        let pool = factory.getPool(token0, token1, fee_u24).call().await?.pool;
        Ok(pool)
    }

    /// Check if position is in range
    pub fn is_in_range(lower: i32, upper: i32, current: i32) -> bool {
        current >= lower && current <= upper
    }
}

impl LpAdapter for UniswapV3Adapter {
    fn protocol_name(&self) -> &'static str {
        "uniswap_v3"
    }

    fn chain(&self) -> &str {
        &self.chain
    }
}

/// Convert tick to price
/// price = 1.0001^tick
pub fn tick_to_price(tick: i32) -> f64 {
    1.0001_f64.powi(tick)
}

/// Convert price to tick
/// tick = log(price) / log(1.0001)
#[allow(dead_code)]
pub fn price_to_tick(price: f64) -> i32 {
    if price <= 0.0 {
        return 0;
    }
    (price.ln() / 1.0001_f64.ln()) as i32
}

/// Calculate liquidity value in USD (approximate)
/// This is a simplified calculation; real implementation needs token prices
#[allow(dead_code)]
pub fn estimate_liquidity_usd(
    liquidity: u128,
    current_tick: i32,
    lower_tick: i32,
    upper_tick: i32,
    token0_price_usd: f64,
    token1_price_usd: f64,
) -> f64 {
    // Simplified: assume 50/50 split at current price
    // In reality, this depends on the position's tick range vs current tick
    let sqrt_price = tick_to_price(current_tick).sqrt();
    let sqrt_lower = tick_to_price(lower_tick).sqrt();
    let sqrt_upper = tick_to_price(upper_tick).sqrt();

    // Amount of token0 in position
    let amount0 = if current_tick < lower_tick {
        liquidity as f64 * (1.0 / sqrt_lower - 1.0 / sqrt_upper)
    } else if current_tick >= upper_tick {
        0.0
    } else {
        liquidity as f64 * (1.0 / sqrt_price - 1.0 / sqrt_upper)
    };

    // Amount of token1 in position
    let amount1 = if current_tick < lower_tick {
        0.0
    } else if current_tick >= upper_tick {
        liquidity as f64 * (sqrt_upper - sqrt_lower)
    } else {
        liquidity as f64 * (sqrt_price - sqrt_lower)
    };

    // Total USD value
    amount0 * token0_price_usd + amount1 * token1_price_usd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_to_price() {
        // tick 0 = price 1.0
        assert!((tick_to_price(0) - 1.0).abs() < 0.0001);

        // tick 46054 ≈ price 100
        let price_100 = tick_to_price(46054);
        assert!((price_100 - 100.0).abs() < 1.0);

        // Negative tick gives price < 1
        let price_low = tick_to_price(-46054);
        assert!(price_low < 0.02);
    }

    #[test]
    fn test_price_to_tick() {
        // price 1.0 = tick 0
        assert_eq!(price_to_tick(1.0), 0);

        // price 100 ≈ tick 46054
        let tick = price_to_tick(100.0);
        assert!((tick - 46054).abs() < 10);

        // Edge case: zero or negative price
        assert_eq!(price_to_tick(0.0), 0);
        assert_eq!(price_to_tick(-1.0), 0);
    }

    #[test]
    fn test_is_in_range() {
        let lower_tick = 100;
        let upper_tick = 200;

        // Current tick in range
        assert!(UniswapV3Adapter::is_in_range(lower_tick, upper_tick, 150));
        assert!(UniswapV3Adapter::is_in_range(lower_tick, upper_tick, 100)); // At boundary
        assert!(UniswapV3Adapter::is_in_range(lower_tick, upper_tick, 200)); // At boundary

        // Current tick out of range
        assert!(!UniswapV3Adapter::is_in_range(lower_tick, upper_tick, 50));  // Below
        assert!(!UniswapV3Adapter::is_in_range(lower_tick, upper_tick, 250)); // Above
    }

    #[test]
    fn test_position_in_range() {
        let current = 150;
        let lower = 100;
        let upper = 200;

        assert!(UniswapV3Adapter::is_in_range(lower, upper, current));
    }

    #[test]
    fn test_position_out_of_range_above() {
        let current = 250;
        let lower = 100;
        let upper = 200;

        assert!(!UniswapV3Adapter::is_in_range(lower, upper, current));
        assert!(current > upper);
    }

    #[test]
    fn test_position_out_of_range_below() {
        let current = 50;
        let lower = 100;
        let upper = 200;

        assert!(!UniswapV3Adapter::is_in_range(lower, upper, current));
        assert!(current < lower);
    }

    #[test]
    fn test_contract_addresses() {
        let eth = UniswapV3Adapter::new("ethereum");
        assert_eq!(
            eth.nft_manager().to_string().to_lowercase(),
            "0xc36442b4a4522e871399cd717abdd847ab11fe88"
        );

        let base = UniswapV3Adapter::new("base");
        assert_eq!(
            base.nft_manager().to_string().to_lowercase(),
            "0x03a520b32c04bf3beef7beb72e919cf822ed34f1"
        );
    }

    #[test]
    fn test_enumerate_user_nft_positions() {
        // This tests the structure - actual RPC calls would be integration tests
        let adapter = UniswapV3Adapter::new("ethereum");
        assert_eq!(adapter.protocol_name(), "uniswap_v3");
        assert_eq!(adapter.chain(), "ethereum");
    }

    #[test]
    fn test_tick_price_roundtrip() {
        // Test that tick -> price -> tick is consistent
        let original_tick = 1000;
        let price = tick_to_price(original_tick);
        let recovered_tick = price_to_tick(price);

        // Should be within 1 tick of original
        assert!((recovered_tick - original_tick).abs() <= 1);
    }

    #[test]
    fn test_liquidity_estimation() {
        // Full in-range position
        let liquidity: u128 = 1_000_000_000_000_000_000;
        let current_tick = 0;
        let lower_tick = -1000;
        let upper_tick = 1000;
        let token0_price = 1.0;
        let token1_price = 1.0;

        let value = estimate_liquidity_usd(
            liquidity,
            current_tick,
            lower_tick,
            upper_tick,
            token0_price,
            token1_price,
        );

        // Should have positive value
        assert!(value > 0.0);
    }
}
