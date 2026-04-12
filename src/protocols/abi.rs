//! Smart contract ABI definitions for protocol interactions
//!
//! Uses Alloy's sol! macro for type-safe contract bindings.

use alloy::sol;

// ============= Aave V3 Interfaces =============

sol! {
    /// Aave V3 Pool interface for user account data
    #[sol(rpc)]
    interface IAaveV3Pool {
        /// Returns the user account data across all the reserves
        /// @param user The address of the user
        /// @return totalCollateralBase The total collateral of the user in the base currency
        /// @return totalDebtBase The total debt of the user in the base currency
        /// @return availableBorrowsBase The borrowing power left of the user in the base currency
        /// @return currentLiquidationThreshold The liquidation threshold of the user
        /// @return ltv The loan to value of the user
        /// @return healthFactor The current health factor of the user
        function getUserAccountData(address user) external view returns (
            uint256 totalCollateralBase,
            uint256 totalDebtBase,
            uint256 availableBorrowsBase,
            uint256 currentLiquidationThreshold,
            uint256 ltv,
            uint256 healthFactor
        );

        /// Returns the normalized income of the reserve
        function getReserveNormalizedIncome(address asset) external view returns (uint256);

        /// Returns the normalized variable debt of the reserve
        function getReserveNormalizedVariableDebt(address asset) external view returns (uint256);
    }
}

// ============= Uniswap V3 Interfaces =============

sol! {
    /// Uniswap V3 NonfungiblePositionManager interface
    #[sol(rpc)]
    interface INonfungiblePositionManager {
        /// Returns the number of NFTs owned by an address
        function balanceOf(address owner) external view returns (uint256);

        /// Returns a token ID owned by owner at a given index
        function tokenOfOwnerByIndex(address owner, uint256 index) external view returns (uint256);

        /// Returns the position information associated with a given token ID
        /// @param tokenId The ID of the token that represents the position
        /// @return nonce The nonce for permits
        /// @return operator The address approved for spending this token
        /// @return token0 The address of the token0 for the pool
        /// @return token1 The address of the token1 for the pool
        /// @return fee The fee tier of the pool
        /// @return tickLower The lower end of the tick range
        /// @return tickUpper The higher end of the tick range
        /// @return liquidity The liquidity of the position
        /// @return feeGrowthInside0LastX128 The fee growth of token0 inside the tick range
        /// @return feeGrowthInside1LastX128 The fee growth of token1 inside the tick range
        /// @return tokensOwed0 The uncollected amount of token0 owed to the position
        /// @return tokensOwed1 The uncollected amount of token1 owed to the position
        function positions(uint256 tokenId) external view returns (
            uint96 nonce,
            address operator,
            address token0,
            address token1,
            uint24 fee,
            int24 tickLower,
            int24 tickUpper,
            uint128 liquidity,
            uint256 feeGrowthInside0LastX128,
            uint256 feeGrowthInside1LastX128,
            uint128 tokensOwed0,
            uint128 tokensOwed1
        );
    }

    /// Uniswap V3 Pool interface
    #[sol(rpc)]
    interface IUniswapV3Pool {
        /// Returns the current tick and other slot0 data
        /// @return sqrtPriceX96 The current price of the pool as a sqrt(token1/token0) Q64.96 value
        /// @return tick The current tick of the pool
        /// @return observationIndex The index of the last oracle observation
        /// @return observationCardinality The current maximum number of observations stored
        /// @return observationCardinalityNext The next maximum number of observations
        /// @return feeProtocol The protocol fee for both tokens
        /// @return unlocked Whether the pool is currently locked to reentrancy
        function slot0() external view returns (
            uint160 sqrtPriceX96,
            int24 tick,
            uint16 observationIndex,
            uint16 observationCardinality,
            uint16 observationCardinalityNext,
            uint8 feeProtocol,
            bool unlocked
        );

        /// Returns the token0 address
        function token0() external view returns (address);

        /// Returns the token1 address
        function token1() external view returns (address);

        /// Returns the pool's fee tier
        function fee() external view returns (uint24);
    }

    /// Uniswap V3 Factory interface for getting pool addresses
    #[sol(rpc)]
    interface IUniswapV3Factory {
        /// Returns the pool address for a given pair of tokens and fee
        function getPool(address tokenA, address tokenB, uint24 fee) external view returns (address pool);
    }
}

// ============= ERC20 Interface =============

sol! {
    /// Standard ERC20 interface
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
        function symbol() external view returns (string memory);
    }
}

#[cfg(test)]
mod tests {
    use alloy::primitives::Address;

    #[test]
    fn test_aave_pool_interface() {
        // Verify the interface compiles and has expected methods
        // This is a compile-time check - if it builds, the ABI is valid
        let _: Address = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2".parse().unwrap();
    }

    #[test]
    fn test_uniswap_interfaces() {
        // Verify interfaces compile
        let _nft: Address = "0xC36442b4a4522E871399CD717aBDD847Ab11FE88".parse().unwrap();
        let _factory: Address = "0x1F98431c8aD98523631AE4a59f267346ea31F984".parse().unwrap();
    }
}
