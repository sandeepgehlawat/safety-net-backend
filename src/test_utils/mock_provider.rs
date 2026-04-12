//! Mock blockchain provider for testing
//!
//! Provides a configurable mock that returns pre-set responses for RPC calls.

use alloy::primitives::{Address, U256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::{MockAaveUserData, MockUniswapPosition};

/// Mock response types for different contract calls
#[derive(Debug, Clone)]
pub enum MockResponse {
    /// Aave V3 getUserAccountData response
    AaveUserData(MockAaveUserData),
    /// Uniswap V3 NFT balanceOf response
    NftBalance(U256),
    /// Uniswap V3 NFT tokenOfOwnerByIndex response
    TokenId(U256),
    /// Uniswap V3 positions response
    UniswapPosition(MockUniswapPosition),
    /// Uniswap V3 pool slot0 response
    PoolSlot0 {
        sqrt_price_x96: U256,
        tick: i32,
        observation_index: u16,
        observation_cardinality: u16,
        observation_cardinality_next: u16,
        fee_protocol: u8,
        unlocked: bool,
    },
    /// Generic uint256 response
    Uint256(U256),
    /// Error response
    Error(String),
}

/// Key for looking up mock responses
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct MockKey {
    /// Target contract address
    pub to: Address,
    /// Function selector (first 4 bytes of calldata)
    pub selector: [u8; 4],
    /// Optional specific calldata hash for parameterized lookups
    pub calldata_hash: Option<u64>,
}

impl MockKey {
    pub fn new(to: Address, selector: [u8; 4]) -> Self {
        Self {
            to,
            selector,
            calldata_hash: None,
        }
    }

    pub fn with_calldata(to: Address, selector: [u8; 4], calldata: &[u8]) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        calldata.hash(&mut hasher);

        Self {
            to,
            selector,
            calldata_hash: Some(hasher.finish()),
        }
    }
}

/// Mock blockchain provider for testing
#[derive(Debug, Clone, Default)]
pub struct MockProvider {
    /// Pre-configured responses
    responses: Arc<RwLock<HashMap<MockKey, Vec<MockResponse>>>>,
    /// Call counter for sequence testing
    call_counts: Arc<RwLock<HashMap<MockKey, usize>>>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a mock response for a contract call
    pub fn mock_response(&self, key: MockKey, response: MockResponse) {
        let mut responses = self.responses.write().unwrap();
        responses.entry(key).or_default().push(response);
    }

    /// Register multiple responses for sequence testing (returned in order)
    pub fn mock_responses(&self, key: MockKey, responses_list: Vec<MockResponse>) {
        let mut responses = self.responses.write().unwrap();
        let entry = responses.entry(key).or_default();
        for r in responses_list {
            entry.push(r);
        }
    }

    /// Get mock response for a call
    pub fn get_response(&self, key: &MockKey) -> Option<MockResponse> {
        let responses = self.responses.read().unwrap();
        let mut counts = self.call_counts.write().unwrap();

        if let Some(response_list) = responses.get(key) {
            let count = counts.entry(key.clone()).or_insert(0);
            let response = if *count < response_list.len() {
                response_list[*count].clone()
            } else {
                // Return last response if we've exhausted the sequence
                response_list.last().cloned().unwrap_or(MockResponse::Error("No response".to_string()))
            };
            *count += 1;
            Some(response)
        } else {
            None
        }
    }

    /// Get number of times a call was made
    pub fn call_count(&self, key: &MockKey) -> usize {
        let counts = self.call_counts.read().unwrap();
        *counts.get(key).unwrap_or(&0)
    }

    /// Reset all call counts
    pub fn reset_counts(&self) {
        let mut counts = self.call_counts.write().unwrap();
        counts.clear();
    }

    /// Clear all mocks
    pub fn clear(&self) {
        self.responses.write().unwrap().clear();
        self.call_counts.write().unwrap().clear();
    }

    // ============= Convenience methods for common mocks =============

    /// Mock Aave V3 getUserAccountData
    pub fn mock_aave_user_data(&self, pool: Address, wallet: Address, data: MockAaveUserData) {
        // getUserAccountData selector: 0xbf92857c
        let selector = [0xbf, 0x92, 0x85, 0x7c];

        // Build calldata hash (selector + padded address)
        let mut calldata = vec![0xbf, 0x92, 0x85, 0x7c];
        calldata.extend_from_slice(&[0u8; 12]); // padding
        calldata.extend_from_slice(wallet.as_slice());

        let key = MockKey::with_calldata(pool, selector, &calldata);
        self.mock_response(key, MockResponse::AaveUserData(data.clone()));

        // Also register without calldata hash for simpler lookups
        let key_simple = MockKey::new(pool, selector);
        self.mock_response(key_simple, MockResponse::AaveUserData(data));
    }

    /// Mock Uniswap V3 NFT balanceOf
    pub fn mock_nft_balance(&self, nft_manager: Address, _owner: Address, balance: u64) {
        // balanceOf selector: 0x70a08231
        let selector = [0x70, 0xa0, 0x82, 0x31];
        let key = MockKey::new(nft_manager, selector);
        self.mock_response(key, MockResponse::NftBalance(U256::from(balance)));
    }

    /// Mock Uniswap V3 NFT tokenOfOwnerByIndex
    pub fn mock_token_of_owner(&self, nft_manager: Address, token_ids: Vec<U256>) {
        // tokenOfOwnerByIndex selector: 0x2f745c59
        let selector = [0x2f, 0x74, 0x5c, 0x59];
        let key = MockKey::new(nft_manager, selector);
        let responses: Vec<MockResponse> = token_ids.into_iter().map(MockResponse::TokenId).collect();
        self.mock_responses(key, responses);
    }

    /// Mock Uniswap V3 positions
    pub fn mock_position(&self, nft_manager: Address, position: MockUniswapPosition) {
        // positions selector: 0x99fbab88
        let selector = [0x99, 0xfb, 0xab, 0x88];
        let key = MockKey::new(nft_manager, selector);
        self.mock_response(key, MockResponse::UniswapPosition(position));
    }

    /// Mock Uniswap V3 pool slot0
    pub fn mock_pool_slot0(&self, pool: Address, tick: i32, sqrt_price_x96: U256) {
        // slot0 selector: 0x3850c7bd
        let selector = [0x38, 0x50, 0xc7, 0xbd];
        let key = MockKey::new(pool, selector);
        self.mock_response(
            key,
            MockResponse::PoolSlot0 {
                sqrt_price_x96,
                tick,
                observation_index: 0,
                observation_cardinality: 1,
                observation_cardinality_next: 1,
                fee_protocol: 0,
                unlocked: true,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider_basic() {
        let provider = MockProvider::new();
        let pool: Address = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2".parse().unwrap();
        let wallet: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();

        // Register mock
        provider.mock_aave_user_data(pool, wallet, MockAaveUserData::healthy());

        // Verify mock is retrievable
        let selector = [0xbf, 0x92, 0x85, 0x7c];
        let key = MockKey::new(pool, selector);
        let response = provider.get_response(&key);

        assert!(response.is_some());
        if let Some(MockResponse::AaveUserData(data)) = response {
            assert_eq!(data.health_factor, U256::from(1_650_000_000_000_000_000u128));
        } else {
            panic!("Expected AaveUserData response");
        }
    }

    #[test]
    fn test_mock_provider_sequence() {
        let provider = MockProvider::new();
        let nft_manager: Address = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88".parse().unwrap();

        // Register sequence of token IDs
        provider.mock_token_of_owner(
            nft_manager,
            vec![U256::from(100), U256::from(200), U256::from(300)],
        );

        let selector = [0x2f, 0x74, 0x5c, 0x59];
        let key = MockKey::new(nft_manager, selector);

        // Get responses in sequence
        let r1 = provider.get_response(&key);
        let r2 = provider.get_response(&key);
        let r3 = provider.get_response(&key);

        if let Some(MockResponse::TokenId(id)) = r1 {
            assert_eq!(id, U256::from(100));
        } else {
            panic!("Expected TokenId response");
        }

        if let Some(MockResponse::TokenId(id)) = r2 {
            assert_eq!(id, U256::from(200));
        } else {
            panic!("Expected TokenId response");
        }

        if let Some(MockResponse::TokenId(id)) = r3 {
            assert_eq!(id, U256::from(300));
        } else {
            panic!("Expected TokenId response");
        }
    }

    #[test]
    fn test_mock_provider_call_count() {
        let provider = MockProvider::new();
        let pool: Address = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2".parse().unwrap();

        let selector = [0xbf, 0x92, 0x85, 0x7c];
        let key = MockKey::new(pool, selector);

        provider.mock_response(key.clone(), MockResponse::AaveUserData(MockAaveUserData::healthy()));

        assert_eq!(provider.call_count(&key), 0);

        provider.get_response(&key);
        assert_eq!(provider.call_count(&key), 1);

        provider.get_response(&key);
        assert_eq!(provider.call_count(&key), 2);

        provider.reset_counts();
        assert_eq!(provider.call_count(&key), 0);
    }

    #[test]
    fn test_mock_nft_balance() {
        let provider = MockProvider::new();
        let nft_manager: Address = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88".parse().unwrap();
        let owner: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();

        provider.mock_nft_balance(nft_manager, owner, 5);

        let selector = [0x70, 0xa0, 0x82, 0x31];
        let key = MockKey::new(nft_manager, selector);
        let response = provider.get_response(&key);

        if let Some(MockResponse::NftBalance(balance)) = response {
            assert_eq!(balance, U256::from(5));
        } else {
            panic!("Expected NftBalance response");
        }
    }

    #[test]
    fn test_mock_pool_slot0() {
        let provider = MockProvider::new();
        let pool: Address = "0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8".parse().unwrap();

        provider.mock_pool_slot0(pool, 12345, U256::from(1_000_000_000_000_000_000u128));

        let selector = [0x38, 0x50, 0xc7, 0xbd];
        let key = MockKey::new(pool, selector);
        let response = provider.get_response(&key);

        if let Some(MockResponse::PoolSlot0 { tick, unlocked, .. }) = response {
            assert_eq!(tick, 12345);
            assert!(unlocked);
        } else {
            panic!("Expected PoolSlot0 response");
        }
    }

    #[test]
    fn test_mock_position() {
        let provider = MockProvider::new();
        let nft_manager: Address = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88".parse().unwrap();

        let position = MockUniswapPosition::in_range(1000);
        provider.mock_position(nft_manager, position.clone());

        let selector = [0x99, 0xfb, 0xab, 0x88];
        let key = MockKey::new(nft_manager, selector);
        let response = provider.get_response(&key);

        if let Some(MockResponse::UniswapPosition(pos)) = response {
            assert!(pos.tick_lower <= 1000 && pos.tick_upper >= 1000);
        } else {
            panic!("Expected UniswapPosition response");
        }
    }

    #[test]
    fn test_mock_no_response() {
        let provider = MockProvider::new();
        let unknown: Address = "0x0000000000000000000000000000000000000001".parse().unwrap();

        let key = MockKey::new(unknown, [0x00, 0x00, 0x00, 0x00]);
        let response = provider.get_response(&key);

        assert!(response.is_none());
    }
}
