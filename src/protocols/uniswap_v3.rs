use super::LpAdapter;
use alloy::primitives::Address;

// Contract addresses
pub const UNISWAP_V3_NFT_MANAGER_ETHEREUM: &str = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88";
pub const UNISWAP_V3_FACTORY_ETHEREUM: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";

/// Uniswap V3 LP adapter
pub struct UniswapV3Adapter {
    nft_manager: Address,
    factory: Address,
    chain: String,
}

impl UniswapV3Adapter {
    pub fn new(chain: &str) -> Self {
        // For now, only Ethereum mainnet addresses
        Self {
            nft_manager: UNISWAP_V3_NFT_MANAGER_ETHEREUM.parse().unwrap(),
            factory: UNISWAP_V3_FACTORY_ETHEREUM.parse().unwrap(),
            chain: chain.to_string(),
        }
    }

    pub fn nft_manager(&self) -> Address {
        self.nft_manager
    }

    pub fn factory(&self) -> Address {
        self.factory
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
    (price.ln() / 1.0001_f64.ln()) as i32
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
    }

    #[test]
    fn test_price_to_tick() {
        // price 1.0 = tick 0
        assert_eq!(price_to_tick(1.0), 0);

        // price 100 ≈ tick 46054
        let tick = price_to_tick(100.0);
        assert!((tick - 46054).abs() < 10);
    }

    #[test]
    fn test_in_range() {
        let lower_tick = 100;
        let upper_tick = 200;

        // Current tick in range
        let current = 150;
        assert!(current >= lower_tick && current <= upper_tick);

        // Current tick below range
        let current = 50;
        assert!(!(current >= lower_tick && current <= upper_tick));
    }
}
