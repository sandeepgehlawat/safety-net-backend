use crate::data::Database;
use alloy::{
    network::EthereumWallet,
    primitives::Address,
    signers::local::PrivateKeySigner,
};
use anyhow::{anyhow, Result};
use tracing::info;
use uuid::Uuid;

/// Guardian signer for autopilot mode
/// This signer is controlled by the system and has scoped permissions
pub struct GuardianSigner {
    wallet: EthereumWallet,
    address: Address,
    db: Database,
}

impl GuardianSigner {
    pub fn new(private_key: &str, db: Database) -> Result<Self> {
        let signer: PrivateKeySigner = private_key.parse()?;
        let address = signer.address();
        let wallet = EthereumWallet::from(signer);

        Ok(Self {
            wallet,
            address,
            db,
        })
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn wallet(&self) -> &EthereumWallet {
        &self.wallet
    }

    /// Check if action is authorized for user
    pub async fn check_authorization(
        &self,
        user_id: Uuid,
        action: &str,
        amount_usd: f64,
        protocol: &str,
    ) -> Result<bool> {
        // Get user's guardian signer permissions
        let signer = self.db.get_active_guardian_signer(user_id).await?;

        let signer = match signer {
            Some(s) => s,
            None => return Ok(false),
        };

        let permissions = &signer.permissions.0;

        // Check action type
        let action_allowed = match action {
            "repay" => permissions.can_repay,
            "rebalance" => permissions.can_rebalance,
            "withdraw" => permissions.can_withdraw,
            _ => false,
        };

        if !action_allowed {
            info!("Action {} not authorized for user {}", action, user_id);
            return Ok(false);
        }

        // Check amount limit
        if amount_usd > permissions.max_single_action_usd {
            info!(
                "Amount ${} exceeds limit ${} for user {}",
                amount_usd, permissions.max_single_action_usd, user_id
            );
            return Ok(false);
        }

        // Check protocol
        if !permissions.allowed_protocols.contains(&protocol.to_string()) {
            info!("Protocol {} not authorized for user {}", protocol, user_id);
            return Ok(false);
        }

        // Check daily spending limit
        let user = self.db.get_user(user_id).await?.ok_or_else(|| anyhow!("User not found"))?;
        let daily_spent: f64 = user.autopilot_daily_spent_usd.to_string().parse().unwrap_or(0.0);
        let budget: f64 = user.autopilot_budget_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0);

        if daily_spent + amount_usd > budget {
            info!(
                "Daily budget exceeded for user {}: spent ${} + ${} > ${}",
                user_id, daily_spent, amount_usd, budget
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Execute an autopilot action
    pub async fn execute_autopilot(
        &self,
        user_id: Uuid,
        action: &str,
        amount_usd: f64,
        protocol: &str,
        _calldata: &[u8],
    ) -> Result<()> {
        // Verify authorization
        if !self.check_authorization(user_id, action, amount_usd, protocol).await? {
            return Err(anyhow!("Action not authorized"));
        }

        info!(
            "Executing autopilot {} for user {}: ${} on {}",
            action, user_id, amount_usd, protocol
        );

        // In production:
        // 1. Build transaction with calldata
        // 2. Sign with guardian wallet
        // 3. Submit via private mempool (Flashbots)
        // 4. Monitor for confirmation

        // Update daily spending
        // (Would be done in submitter after confirmation)

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::data::SignerPermissions;

    #[test]
    fn test_permissions_check() {
        let permissions = SignerPermissions::default();

        // Default permissions
        assert!(permissions.can_repay);
        assert!(permissions.can_rebalance);
        assert!(!permissions.can_withdraw);
        assert_eq!(permissions.max_single_action_usd, 5000.0);
        assert!(permissions.allowed_protocols.contains(&"aave_v3".to_string()));
    }

    #[test]
    fn test_amount_limit() {
        let permissions = SignerPermissions {
            max_single_action_usd: 1000.0,
            ..Default::default()
        };

        assert!(500.0 <= permissions.max_single_action_usd);
        assert!(!(1500.0 <= permissions.max_single_action_usd));
    }
}
