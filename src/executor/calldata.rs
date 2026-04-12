//! Calldata Builder
//!
//! Builds properly ABI-encoded calldata for smart contract interactions.

use alloy::primitives::{Address, Bytes, U256};

/// Calldata builder for DeFi protocol interactions
pub struct CalldataBuilder;

impl CalldataBuilder {
    // ============= Aave V3 Calldata =============

    /// Build Aave V3 repay calldata
    ///
    /// Function: repay(address asset, uint256 amount, uint256 interestRateMode, address onBehalfOf)
    /// Selector: 0x573ade81
    ///
    /// # Arguments
    /// * `asset` - The address of the borrowed underlying asset
    /// * `amount` - The amount to repay (use U256::MAX for max repay)
    /// * `rate_mode` - The interest rate mode: 1 for Stable, 2 for Variable
    /// * `on_behalf_of` - The address of the user who will get their debt reduced
    pub fn build_aave_repay(
        asset: Address,
        amount: U256,
        rate_mode: u8,
        on_behalf_of: Address,
    ) -> Bytes {
        // Build the ABI-encoded calldata
        // repay(address,uint256,uint256,address)
        let mut data = Vec::with_capacity(4 + 32 * 4);

        // Function selector: keccak256("repay(address,uint256,uint256,address)")[:4]
        // = 0x573ade81
        data.extend_from_slice(&[0x57, 0x3a, 0xde, 0x81]);

        // Param 1: asset (address) - padded to 32 bytes
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(asset.as_slice());

        // Param 2: amount (uint256) - 32 bytes
        data.extend_from_slice(&amount.to_be_bytes::<32>());

        // Param 3: interestRateMode (uint256) - 32 bytes
        let rate_mode_u256 = U256::from(rate_mode);
        data.extend_from_slice(&rate_mode_u256.to_be_bytes::<32>());

        // Param 4: onBehalfOf (address) - padded to 32 bytes
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(on_behalf_of.as_slice());

        Bytes::from(data)
    }

    /// Build Aave V3 withdraw calldata
    ///
    /// Function: withdraw(address asset, uint256 amount, address to)
    /// Selector: 0x69328dec
    pub fn build_aave_withdraw(
        asset: Address,
        amount: U256,
        to: Address,
    ) -> Bytes {
        let mut data = Vec::with_capacity(4 + 32 * 3);

        // Function selector: withdraw(address,uint256,address)
        data.extend_from_slice(&[0x69, 0x32, 0x8d, 0xec]);

        // Param 1: asset
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(asset.as_slice());

        // Param 2: amount
        data.extend_from_slice(&amount.to_be_bytes::<32>());

        // Param 3: to
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(to.as_slice());

        Bytes::from(data)
    }

    // ============= Uniswap V3 Calldata =============

    /// Build Uniswap V3 decreaseLiquidity calldata
    ///
    /// Function: decreaseLiquidity((uint256 tokenId, uint128 liquidity, uint256 amount0Min, uint256 amount1Min, uint256 deadline))
    /// Selector: 0x0c49ccbe
    pub fn build_decrease_liquidity(
        token_id: U256,
        liquidity: u128,
        amount0_min: U256,
        amount1_min: U256,
        deadline: U256,
    ) -> Bytes {
        let mut data = Vec::with_capacity(4 + 32 * 5);

        // Function selector: decreaseLiquidity((uint256,uint128,uint256,uint256,uint256))
        data.extend_from_slice(&[0x0c, 0x49, 0xcc, 0xbe]);

        // Param 1: tokenId
        data.extend_from_slice(&token_id.to_be_bytes::<32>());

        // Param 2: liquidity (uint128 packed into uint256)
        let liquidity_u256 = U256::from(liquidity);
        data.extend_from_slice(&liquidity_u256.to_be_bytes::<32>());

        // Param 3: amount0Min
        data.extend_from_slice(&amount0_min.to_be_bytes::<32>());

        // Param 4: amount1Min
        data.extend_from_slice(&amount1_min.to_be_bytes::<32>());

        // Param 5: deadline
        data.extend_from_slice(&deadline.to_be_bytes::<32>());

        Bytes::from(data)
    }

    /// Build Uniswap V3 collect calldata
    ///
    /// Function: collect((uint256 tokenId, address recipient, uint128 amount0Max, uint128 amount1Max))
    /// Selector: 0xfc6f7865
    pub fn build_collect(
        token_id: U256,
        recipient: Address,
        amount0_max: u128,
        amount1_max: u128,
    ) -> Bytes {
        let mut data = Vec::with_capacity(4 + 32 * 4);

        // Function selector: collect((uint256,address,uint128,uint128))
        data.extend_from_slice(&[0xfc, 0x6f, 0x78, 0x65]);

        // Param 1: tokenId
        data.extend_from_slice(&token_id.to_be_bytes::<32>());

        // Param 2: recipient
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(recipient.as_slice());

        // Param 3: amount0Max (uint128 packed into uint256)
        let amount0_u256 = U256::from(amount0_max);
        data.extend_from_slice(&amount0_u256.to_be_bytes::<32>());

        // Param 4: amount1Max (uint128 packed into uint256)
        let amount1_u256 = U256::from(amount1_max);
        data.extend_from_slice(&amount1_u256.to_be_bytes::<32>());

        Bytes::from(data)
    }

    /// Build Uniswap V3 multicall for combined operations
    ///
    /// Wraps multiple calls into a single multicall transaction
    pub fn build_multicall(calls: Vec<Bytes>) -> Bytes {
        // multicall selector: 0xac9650d8
        let mut data = Vec::new();
        data.extend_from_slice(&[0xac, 0x96, 0x50, 0xd8]);

        // Dynamic array encoding
        // Offset to array data (32 bytes)
        data.extend_from_slice(&U256::from(32).to_be_bytes::<32>());

        // Array length
        data.extend_from_slice(&U256::from(calls.len()).to_be_bytes::<32>());

        // Calculate offsets for each bytes element
        let mut offset = 32 * calls.len(); // Start after all offset pointers
        let mut offsets = Vec::new();
        let mut encoded_calls = Vec::new();

        for call in &calls {
            offsets.push(offset);
            // Each bytes element: length (32 bytes) + data (padded to 32)
            let padded_len = ((call.len() + 31) / 32) * 32;
            offset += 32 + padded_len;

            // Encode the call bytes
            let mut encoded = Vec::new();
            encoded.extend_from_slice(&U256::from(call.len()).to_be_bytes::<32>());
            encoded.extend_from_slice(call);
            // Pad to 32 bytes
            let padding = padded_len - call.len();
            encoded.extend_from_slice(&vec![0u8; padding]);
            encoded_calls.push(encoded);
        }

        // Write offsets
        for off in offsets {
            data.extend_from_slice(&U256::from(off).to_be_bytes::<32>());
        }

        // Write encoded calls
        for encoded in encoded_calls {
            data.extend_from_slice(&encoded);
        }

        Bytes::from(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aave_repay_calldata_encoding() {
        let asset: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap(); // USDC
        let amount = U256::from(1_000_000_000u64); // 1000 USDC (6 decimals)
        let rate_mode = 2u8; // Variable
        let on_behalf_of: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();

        let calldata = CalldataBuilder::build_aave_repay(asset, amount, rate_mode, on_behalf_of);

        // Check selector
        assert_eq!(&calldata[0..4], &[0x57, 0x3a, 0xde, 0x81]);

        // Check total length: 4 (selector) + 4 * 32 (params) = 132 bytes
        assert_eq!(calldata.len(), 132);

        // Check asset address is at correct offset
        assert_eq!(&calldata[16..36], asset.as_slice());

        // Check on_behalf_of address
        assert_eq!(&calldata[112..132], on_behalf_of.as_slice());
    }

    #[test]
    fn test_aave_repay_max_amount() {
        let asset: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap();
        let amount = U256::MAX; // type(uint256).max for full repay
        let rate_mode = 2u8;
        let on_behalf_of: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();

        let calldata = CalldataBuilder::build_aave_repay(asset, amount, rate_mode, on_behalf_of);

        // Check that max amount is encoded correctly (all 1s)
        assert_eq!(&calldata[36..68], &[0xff; 32]);
    }

    #[test]
    fn test_decrease_liquidity_encoding() {
        let token_id = U256::from(12345u64);
        let liquidity = 1_000_000_000_000_000_000u128;
        let amount0_min = U256::ZERO;
        let amount1_min = U256::ZERO;
        let deadline = U256::from(9999999999u64);

        let calldata = CalldataBuilder::build_decrease_liquidity(
            token_id,
            liquidity,
            amount0_min,
            amount1_min,
            deadline,
        );

        // Check selector
        assert_eq!(&calldata[0..4], &[0x0c, 0x49, 0xcc, 0xbe]);

        // Check length: 4 + 5 * 32 = 164 bytes
        assert_eq!(calldata.len(), 164);

        // Check token_id is at correct position
        let encoded_token_id = U256::from_be_bytes::<32>(calldata[4..36].try_into().unwrap());
        assert_eq!(encoded_token_id, token_id);
    }

    #[test]
    fn test_collect_encoding() {
        let token_id = U256::from(12345u64);
        let recipient: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();
        let amount0_max = u128::MAX; // Collect all
        let amount1_max = u128::MAX;

        let calldata = CalldataBuilder::build_collect(token_id, recipient, amount0_max, amount1_max);

        // Check selector
        assert_eq!(&calldata[0..4], &[0xfc, 0x6f, 0x78, 0x65]);

        // Check length: 4 + 4 * 32 = 132 bytes
        assert_eq!(calldata.len(), 132);

        // Check recipient address
        assert_eq!(&calldata[48..68], recipient.as_slice());
    }

    #[test]
    fn test_aave_withdraw_encoding() {
        let asset: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse().unwrap(); // WETH
        let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let to: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();

        let calldata = CalldataBuilder::build_aave_withdraw(asset, amount, to);

        // Check selector: withdraw(address,uint256,address)
        assert_eq!(&calldata[0..4], &[0x69, 0x32, 0x8d, 0xec]);

        // Check length: 4 + 3 * 32 = 100 bytes
        assert_eq!(calldata.len(), 100);
    }

    #[test]
    fn test_multicall_encoding() {
        // Create two simple calls
        let call1 = CalldataBuilder::build_collect(
            U256::from(1u64),
            Address::ZERO,
            u128::MAX,
            u128::MAX,
        );
        let call2 = CalldataBuilder::build_collect(
            U256::from(2u64),
            Address::ZERO,
            u128::MAX,
            u128::MAX,
        );

        let multicall = CalldataBuilder::build_multicall(vec![call1.clone(), call2.clone()]);

        // Check selector
        assert_eq!(&multicall[0..4], &[0xac, 0x96, 0x50, 0xd8]);

        // Multicall should be longer than individual calls
        assert!(multicall.len() > call1.len() + call2.len());
    }
}
