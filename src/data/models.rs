use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ============= User Models =============

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub wallet_address: String,
    pub tier: String,
    pub trial_ends_at: Option<DateTime<Utc>>,
    pub subscription_stream_id: Option<String>,
    pub autopilot_enabled: bool,
    pub autopilot_budget_usd: Option<Decimal>,
    pub autopilot_daily_spent_usd: Decimal,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    // Notification preferences
    #[sqlx(default)]
    pub fcm_token: Option<String>,
    #[sqlx(default)]
    pub telegram_chat_id: Option<String>,
    #[sqlx(default)]
    pub email: Option<String>,
    #[sqlx(default)]
    pub notifications_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignerPermissions {
    pub can_repay: bool,
    pub can_rebalance: bool,
    pub can_withdraw: bool,
    pub max_single_action_usd: f64,
    pub allowed_protocols: Vec<String>,
}

impl Default for SignerPermissions {
    fn default() -> Self {
        Self {
            can_repay: true,
            can_rebalance: true,
            can_withdraw: false,
            max_single_action_usd: 5000.0,
            allowed_protocols: vec![
                "aave_v3".to_string(),
                "morpho".to_string(),
                "uniswap_v3".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GuardianSigner {
    pub id: Uuid,
    pub user_id: Uuid,
    pub signer_address: String,
    pub permissions: sqlx::types::Json<SignerPermissions>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

// ============= Position Models =============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    AaveV3,
    Morpho,
    Spark,
    Compound,
    Euler,
    UniswapV3,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::AaveV3 => "aave_v3",
            Protocol::Morpho => "morpho",
            Protocol::Spark => "spark",
            Protocol::Compound => "compound",
            Protocol::Euler => "euler",
            Protocol::UniswapV3 => "uniswap_v3",
        }
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Ethereum,
    Arbitrum,
    Base,
    Optimism,
    Polygon,
}

impl Chain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Chain::Ethereum => "ethereum",
            Chain::Arbitrum => "arbitrum",
            Chain::Base => "base",
            Chain::Optimism => "optimism",
            Chain::Polygon => "polygon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionStatus {
    Healthy,
    Warning,
    Critical,
}

impl PositionStatus {
    pub fn from_health_factor(hf: f64, alert_threshold: f64) -> Self {
        if hf >= alert_threshold + 0.3 {
            PositionStatus::Healthy
        } else if hf >= alert_threshold {
            PositionStatus::Warning
        } else {
            PositionStatus::Critical
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LendingPosition {
    pub id: Uuid,
    pub user_id: Uuid,
    pub protocol: String,
    pub chain: String,
    pub collateral_usd: Option<Decimal>,
    pub debt_usd: Option<Decimal>,
    pub health_factor: Option<Decimal>,
    pub liquidation_threshold: Option<Decimal>,
    pub block_number: i64,
    pub indexed_at: DateTime<Utc>,
    pub is_active: bool,
    pub alert_threshold: Decimal,
}

impl LendingPosition {
    pub fn status(&self) -> PositionStatus {
        let hf = self.health_factor.map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)).unwrap_or(0.0);
        let threshold = self.alert_threshold.to_string().parse::<f64>().unwrap_or(1.2);
        PositionStatus::from_health_factor(hf, threshold)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LpPosition {
    pub id: Uuid,
    pub user_id: Uuid,
    pub protocol: String,
    pub chain: String,
    pub token_id: String,
    pub token0: String,
    pub token1: String,
    pub fee_tier: i32,
    pub lower_tick: i32,
    pub upper_tick: i32,
    pub current_tick: Option<i32>,
    pub liquidity: Option<Decimal>,
    pub in_range: Option<bool>,
    pub lower_price_usd: Option<Decimal>,
    pub upper_price_usd: Option<Decimal>,
    pub current_price_usd: Option<Decimal>,
    pub block_number: Option<i64>,
    pub indexed_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TokenWatch {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_address: String,
    pub chain: String,
    pub symbol: Option<String>,
    pub reference_price_usd: Option<Decimal>,
    pub reference_time: Option<DateTime<Utc>>,
    pub alert_threshold_pct: Decimal,
    pub current_price_usd: Option<Decimal>,
    pub current_change_pct: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStatus {
    pub symbol: String,
    pub price_usd: f64,
    pub change_pct: f64,
    pub status: String, // "ok", "warn", "bad"
}

// ============= Alert Models =============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    HealthFactor,
    OutOfRange,
    Drawdown,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertType::HealthFactor => "health_factor",
            AlertType::OutOfRange => "out_of_range",
            AlertType::Drawdown => "drawdown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Repay,
    Rebalance,
    Withdraw,
}

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionType::Repay => "repay",
            ActionType::Rebalance => "rebalance",
            ActionType::Withdraw => "withdraw",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub id: Uuid,
    pub action: ActionType,
    pub amount_usd: f64,
    pub health_factor_before: Option<f64>,
    pub health_factor_after: Option<f64>,
    pub debt_before: Option<f64>,
    pub debt_after: Option<f64>,
    pub gas_estimate: u64,
    pub gas_cost_usd: f64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Alert {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub position_type: Option<String>,
    pub alert_type: String,
    pub current_value: Option<Decimal>,
    pub previous_value: Option<Decimal>,
    pub threshold: Option<Decimal>,
    pub suggested_action: Option<String>,
    pub suggested_amount_usd: Option<Decimal>,
    pub simulation_result: Option<sqlx::types::Json<SimulationResult>>,
    pub simulation_expires_at: Option<DateTime<Utc>>,
    pub fired_at: DateTime<Utc>,
    pub delivery_status: Option<sqlx::types::Json<serde_json::Value>>,
    pub action_taken: Option<String>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertStatus {
    Pending,
    Snoozed,
    Resolved,
}

// ============= Transaction Models =============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TxStatus {
    Pending,
    Submitted,
    Confirmed,
    Failed,
}

impl TxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxStatus::Pending => "pending",
            TxStatus::Submitted => "submitted",
            TxStatus::Confirmed => "confirmed",
            TxStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Transaction {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub alert_id: Option<Uuid>,
    pub chain: String,
    pub tx_type: String,
    pub tx_hash: Option<String>,
    pub status: Option<String>,
    pub gas_estimate: Option<i64>,
    pub gas_used: Option<i64>,
    pub gas_cost_usd: Option<Decimal>,
    pub amount_usd: Option<Decimal>,
    pub is_autopilot: bool,
    pub used_private_mempool: bool,
    pub simulated_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

// ============= Billing Models =============

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BillingEvent {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub event_type: String,
    pub amount_usd: Decimal,
    pub saved_amount_usd: Option<Decimal>,
    pub intervention_id: Option<Uuid>,
    pub x402_tx_hash: Option<String>,
    pub x402_stream_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GlobalStats {
    pub id: i32,
    pub total_saved_usd: Decimal,
    pub saved_this_week_usd: Decimal,
    pub total_positions: i32,
    pub updated_at: DateTime<Utc>,
}

// ============= WebSocket Messages =============

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    PositionUpdate {
        position_id: String,
        position_type: String,
        health_factor: Option<f64>,
        in_range: Option<bool>,
        block_number: u64,
    },
    AlertFired {
        alert_id: String,
        position_id: String,
        alert_type: String,
        current_value: f64,
        threshold: f64,
        suggested_action: Option<SuggestedAction>,
    },
    TxStatus {
        tx_id: String,
        status: String,
        tx_hash: Option<String>,
        confirmed_at: Option<u64>,
    },
    TokenUpdate {
        symbol: String,
        price_usd: f64,
        change_pct: f64,
        status: String,
    },
    BlockProcessed {
        block_number: u64,
        latency_ms: u64,
        positions_checked: u32,
    },
    TickerEvent {
        event_type: String,
        message: String,
        timestamp_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub action_type: String,
    pub amount_usd: f64,
    pub simulation: SimulationResult,
}

// ============= Health Factor History =============

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HealthDataPoint {
    pub position_id: Uuid,
    pub time: DateTime<Utc>,
    pub health_factor: Option<Decimal>,
    pub collateral_usd: Option<Decimal>,
    pub debt_usd: Option<Decimal>,
    pub block_number: Option<i64>,
}
