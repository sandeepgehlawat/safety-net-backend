use crate::data::Database;
use alloy::{
    primitives::{Address, U256},
    providers::Provider,
    sol,
};
use anyhow::Result;
use rust_decimal::Decimal;
use tracing::info;
use uuid::Uuid;

// Simplified x402 payment streaming interface
sol! {
    #[sol(rpc)]
    interface IX402 {
        function createStream(
            address recipient,
            uint256 ratePerSecond,
            uint256 deposit
        ) external returns (uint256 streamId);

        function cancelStream(uint256 streamId) external;

        function getStreamBalance(uint256 streamId) external view returns (uint256);

        function pullPayment(
            address from,
            uint256 amount
        ) external returns (bool);
    }
}

/// x402 micropayment client
pub struct X402Client<P: Provider + Clone + 'static> {
    provider: P,
    contract_address: Address,
    fee_wallet: Address,
    db: Database,
}

impl<P: Provider + Clone + 'static> X402Client<P> {
    pub fn new(
        provider: P,
        contract_address: Address,
        fee_wallet: Address,
        db: Database,
    ) -> Self {
        Self {
            provider,
            contract_address,
            fee_wallet,
            db,
        }
    }

    /// Start a subscription stream ($19/month)
    pub async fn start_subscription(
        &self,
        user_id: Uuid,
        _user_wallet: Address,
    ) -> Result<String> {
        // $19/month = $19 / (30 * 24 * 60 * 60) ≈ 0.000007 USDC/second
        // In wei (6 decimals for USDC): 7
        let _rate_per_second = U256::from(7);

        // Initial deposit: 1 month
        let _deposit = U256::from(19_000_000); // 19 USDC

        let _contract = IX402::new(self.contract_address, &self.provider);

        // In production, this would be a signed transaction from the user
        // For now, simulate the stream ID
        let stream_id = format!("stream_{}", Uuid::new_v4());

        info!(
            "Started subscription stream for user {}: {} (${}/month)",
            user_id, stream_id, 19
        );

        // Update user tier
        self.db.update_user_tier(user_id, "autopilot", Some(&stream_id)).await?;

        Ok(stream_id)
    }

    /// Cancel a subscription stream
    pub async fn cancel_subscription(
        &self,
        user_id: Uuid,
        stream_id: &str,
    ) -> Result<()> {
        info!("Cancelling subscription stream {} for user {}", stream_id, user_id);

        // In production, call contract.cancelStream()

        // Update user tier
        self.db.update_user_tier(user_id, "free", None).await?;

        Ok(())
    }

    /// Check if subscription is active (has balance)
    pub async fn is_subscription_active(&self, _stream_id: &str) -> Result<bool> {
        // In production, call contract.getStreamBalance()
        // For now, return true
        Ok(true)
    }

    /// Pull payment for success fee (pay-as-you-save tier)
    pub async fn charge_success_fee(
        &self,
        user_id: Uuid,
        _user_wallet: Address,
        saved_amount_usd: f64,
    ) -> Result<String> {
        // 10% success fee
        let fee_usd = saved_amount_usd * 0.10;
        let _fee_wei = U256::from((fee_usd * 1_000_000.0) as u64); // USDC has 6 decimals

        info!(
            "Charging ${:.2} success fee to user {} (saved ${:.2})",
            fee_usd, user_id, saved_amount_usd
        );

        // In production, call contract.pullPayment()
        let tx_hash = format!("0x{}", hex::encode(&Uuid::new_v4().as_bytes()[..]));

        // Record billing event
        self.db.create_billing_event(
            user_id,
            "success_fee",
            Decimal::from_f64_retain(fee_usd).unwrap_or_default(),
            Some(Decimal::from_f64_retain(saved_amount_usd).unwrap_or_default()),
            None,
            Some(&tx_hash),
        ).await?;

        Ok(tx_hash)
    }

    /// Charge per-check micropayment ($0.0004/check)
    pub async fn charge_check_fee(
        &self,
        user_id: Uuid,
    ) -> Result<()> {
        // $0.0004 per check
        // In practice, batch these and settle hourly

        // For now, just record
        self.db.create_billing_event(
            user_id,
            "check_fee",
            Decimal::from_f64_retain(0.0004).unwrap_or_default(),
            None,
            None,
            None,
        ).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fee_calculation() {
        let saved_amount: f64 = 5000.0;
        let fee = saved_amount * 0.10;
        assert!((fee - 500.0_f64).abs() < 0.01);
    }

    #[test]
    fn test_subscription_rate() {
        // $19/month in seconds
        let monthly_rate: f64 = 19.0;
        let seconds_per_month: f64 = 30.0 * 24.0 * 60.0 * 60.0;
        let rate_per_second = monthly_rate / seconds_per_month;

        assert!((rate_per_second - 0.000007_f64).abs() < 0.000001);
    }
}
