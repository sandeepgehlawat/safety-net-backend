use crate::api::WsState;
use crate::data::Database;
use alloy::{
    primitives::{Address, Bytes, U256},
    providers::Provider,
    rpc::types::TransactionRequest,
};
use anyhow::Result;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

/// Transaction submitter supporting both public and private mempools
pub struct TxSubmitter {
    db: Database,
    ws_state: Arc<WsState>,
    flashbots_rpc: String,
    http_client: Client,
}

#[derive(Debug, Serialize)]
struct FlashbotsBundle {
    txs: Vec<String>,
    block_number: String,
    min_timestamp: Option<u64>,
    max_timestamp: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FlashbotsResponse {
    #[serde(rename = "bundleHash")]
    bundle_hash: Option<String>,
    error: Option<String>,
}

impl TxSubmitter {
    pub fn new(
        db: Database,
        ws_state: Arc<WsState>,
        flashbots_rpc: String,
    ) -> Self {
        Self {
            db,
            ws_state,
            flashbots_rpc,
            http_client: Client::new(),
        }
    }

    /// Submit transaction to public mempool
    pub async fn submit_public<P: Provider + Clone + Send + Sync + 'static>(
        &self,
        provider: &P,
        tx_id: Uuid,
        user_id: Uuid,
        signed_tx: &[u8],
    ) -> Result<String> {
        // Update status to submitted
        self.db.update_transaction_submitted(tx_id, "", false).await?;

        // Broadcast pending status
        self.ws_state.send_tx_status_to_user(
            user_id,
            tx_id,
            "submitted",
            None,
            None,
        ).await;

        // Send transaction
        let pending = provider.send_raw_transaction(signed_tx).await?;
        let tx_hash = format!("{:?}", pending.tx_hash());

        // Update with hash
        self.db.update_transaction_submitted(tx_id, &tx_hash, false).await?;

        info!("Transaction {} submitted: {}", tx_id, tx_hash);

        // Notify via WebSocket
        self.ws_state.send_tx_status_to_user(
            user_id,
            tx_id,
            "submitted",
            Some(tx_hash.clone()),
            None,
        ).await;

        Ok(tx_hash)
    }

    /// Submit transaction via Flashbots private mempool
    pub async fn submit_private(
        &self,
        tx_id: Uuid,
        user_id: Uuid,
        signed_tx: &str,
        target_block: u64,
    ) -> Result<String> {
        // Update status
        self.db.update_transaction_submitted(tx_id, "", true).await?;

        // Create bundle
        let bundle = FlashbotsBundle {
            txs: vec![signed_tx.to_string()],
            block_number: format!("0x{:x}", target_block),
            min_timestamp: None,
            max_timestamp: None,
        };

        // Send to Flashbots
        let response = self.http_client
            .post(&self.flashbots_rpc)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_sendBundle",
                "params": [bundle]
            }))
            .send()
            .await?;

        let result: FlashbotsResponse = response.json().await?;

        if let Some(error) = result.error {
            error!("Flashbots error: {}", error);
            self.db.update_transaction_failed(tx_id).await?;
            return Err(anyhow::anyhow!("Flashbots error: {}", error));
        }

        let bundle_hash = result.bundle_hash.unwrap_or_default();
        info!("Transaction {} submitted via Flashbots: {}", tx_id, bundle_hash);

        // Update with bundle hash
        self.db.update_transaction_submitted(tx_id, &bundle_hash, true).await?;

        // Notify via WebSocket
        self.ws_state.send_tx_status_to_user(
            user_id,
            tx_id,
            "submitted",
            Some(bundle_hash.clone()),
            None,
        ).await;

        Ok(bundle_hash)
    }

    /// Wait for transaction confirmation
    pub async fn wait_for_confirmation<P: Provider + Clone + Send + Sync + 'static>(
        &self,
        provider: &P,
        tx_id: Uuid,
        user_id: Uuid,
        tx_hash: &str,
    ) -> Result<()> {
        // Parse transaction hash
        let hash: alloy::primitives::TxHash = tx_hash.parse()?;

        // Wait for receipt (with timeout)
        let receipt = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            provider.get_transaction_receipt(hash),
        ).await??;

        if let Some(receipt) = receipt {
            let gas_used = receipt.gas_used as i64;
            let gas_price = receipt.effective_gas_price;
            let eth_price = 3500.0; // Would fetch from oracle
            let gas_cost_usd = (gas_used as f64 * gas_price as f64 * 1e-18) * eth_price;

            // Update database
            self.db.update_transaction_confirmed(
                tx_id,
                gas_used,
                Decimal::from_f64_retain(gas_cost_usd).unwrap_or_default(),
            ).await?;

            info!(
                "Transaction {} confirmed: gas used {}, cost ${:.2}",
                tx_id, gas_used, gas_cost_usd
            );

            // Notify via WebSocket
            self.ws_state.send_tx_status_to_user(
                user_id,
                tx_id,
                "confirmed",
                Some(tx_hash.to_string()),
                Some(chrono::Utc::now().timestamp() as u64),
            ).await;
        } else {
            error!("Transaction {} not found", tx_hash);
            self.db.update_transaction_failed(tx_id).await?;

            self.ws_state.send_tx_status_to_user(
                user_id,
                tx_id,
                "failed",
                Some(tx_hash.to_string()),
                None,
            ).await;
        }

        Ok(())
    }

    /// Build a repay transaction for Aave V3
    pub fn build_aave_repay_tx(
        &self,
        pool_address: Address,
        asset: Address,
        amount: U256,
        rate_mode: u8,
        on_behalf_of: Address,
    ) -> TransactionRequest {
        // Aave V3 repay function selector: 0x573ade81
        // repay(address asset, uint256 amount, uint256 rateMode, address onBehalfOf)
        let mut calldata = vec![0x57, 0x3a, 0xde, 0x81];

        // Encode parameters (simplified - in production use proper ABI encoding)
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(asset.as_slice());
        calldata.extend_from_slice(&amount.to_be_bytes::<32>());
        calldata.extend_from_slice(&[0u8; 31]);
        calldata.push(rate_mode);
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(on_behalf_of.as_slice());

        TransactionRequest::default()
            .to(pool_address)
            .input(Bytes::from(calldata).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flashbots_bundle_serialization() {
        let bundle = FlashbotsBundle {
            txs: vec!["0x1234".to_string()],
            block_number: "0x100".to_string(),
            min_timestamp: None,
            max_timestamp: None,
        };

        let json = serde_json::to_string(&bundle).unwrap();
        assert!(json.contains("0x1234"));
        assert!(json.contains("0x100"));
    }
}
