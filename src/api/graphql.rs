use crate::data::{
    models::*, Database, PositionStore, SignerPermissions,
};
use async_graphql::{
    Context, EmptySubscription, Enum, InputObject, Object, Schema, SimpleObject, ID,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::sync::Arc;
use uuid::Uuid;

// ============= GraphQL Types =============

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlProtocol {
    AaveV3,
    Morpho,
    Spark,
    Compound,
    Euler,
    UniswapV3,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlChain {
    Ethereum,
    Arbitrum,
    Base,
    Optimism,
    Polygon,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlPositionStatus {
    Healthy,
    Warning,
    Critical,
}

impl From<PositionStatus> for GqlPositionStatus {
    fn from(status: PositionStatus) -> Self {
        match status {
            PositionStatus::Healthy => GqlPositionStatus::Healthy,
            PositionStatus::Warning => GqlPositionStatus::Warning,
            PositionStatus::Critical => GqlPositionStatus::Critical,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlAlertStatus {
    Pending,
    Snoozed,
    Resolved,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlAlertType {
    HealthFactor,
    OutOfRange,
    Drawdown,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlActionType {
    Repay,
    Rebalance,
    Withdraw,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum TimeRange {
    Hour1,
    Hour6,
    Hour24,
    Day7,
    Day30,
}

impl TimeRange {
    fn to_duration(&self) -> Duration {
        match self {
            TimeRange::Hour1 => Duration::hours(1),
            TimeRange::Hour6 => Duration::hours(6),
            TimeRange::Hour24 => Duration::hours(24),
            TimeRange::Day7 => Duration::days(7),
            TimeRange::Day30 => Duration::days(30),
        }
    }
}

// ============= GraphQL Object Types =============

#[derive(SimpleObject)]
pub struct GqlUser {
    pub id: ID,
    pub wallet_address: String,
    pub tier: String,
    pub autopilot_enabled: bool,
    pub autopilot_budget_usd: Option<f64>,
    pub trial_ends_at: Option<DateTime<Utc>>,
}

impl From<User> for GqlUser {
    fn from(user: User) -> Self {
        Self {
            id: ID(user.id.to_string()),
            wallet_address: user.wallet_address,
            tier: user.tier,
            autopilot_enabled: user.autopilot_enabled,
            autopilot_budget_usd: user.autopilot_budget_usd.map(|d| d.to_string().parse().unwrap_or(0.0)),
            trial_ends_at: user.trial_ends_at,
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlLendingPosition {
    pub id: ID,
    pub protocol: String,
    pub chain: String,
    pub health_factor: f64,
    pub collateral_usd: f64,
    pub debt_usd: f64,
    pub liquidation_threshold: f64,
    pub alert_threshold: f64,
    pub status: GqlPositionStatus,
    pub indexed_at: DateTime<Utc>,
}

impl From<LendingPosition> for GqlLendingPosition {
    fn from(pos: LendingPosition) -> Self {
        let hf = pos.health_factor.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0);
        let threshold = pos.alert_threshold.to_string().parse().unwrap_or(1.2);

        Self {
            id: ID(pos.id.to_string()),
            protocol: pos.protocol,
            chain: pos.chain,
            health_factor: hf,
            collateral_usd: pos.collateral_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            debt_usd: pos.debt_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            liquidation_threshold: pos.liquidation_threshold.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            alert_threshold: threshold,
            status: PositionStatus::from_health_factor(hf, threshold).into(),
            indexed_at: pos.indexed_at,
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlToken {
    pub address: String,
    pub symbol: Option<String>,
}

#[derive(SimpleObject)]
pub struct GqlLpPosition {
    pub id: ID,
    pub token_id: String,
    pub token0: GqlToken,
    pub token1: GqlToken,
    pub fee_tier: i32,
    pub lower_price_usd: f64,
    pub upper_price_usd: f64,
    pub current_price_usd: f64,
    pub in_range: bool,
    pub liquidity: String,
}

impl From<LpPosition> for GqlLpPosition {
    fn from(pos: LpPosition) -> Self {
        Self {
            id: ID(pos.id.to_string()),
            token_id: pos.token_id,
            token0: GqlToken {
                address: pos.token0,
                symbol: None,
            },
            token1: GqlToken {
                address: pos.token1,
                symbol: None,
            },
            fee_tier: pos.fee_tier,
            lower_price_usd: pos.lower_price_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            upper_price_usd: pos.upper_price_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            current_price_usd: pos.current_price_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            in_range: pos.in_range.unwrap_or(false),
            liquidity: pos.liquidity.map(|d| d.to_string()).unwrap_or_default(),
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlTokenWatch {
    pub id: ID,
    pub token_address: String,
    pub symbol: Option<String>,
    pub price_usd: f64,
    pub change_pct: f64,
    pub alert_threshold_pct: f64,
    pub status: String,
}

impl From<TokenWatch> for GqlTokenWatch {
    fn from(watch: TokenWatch) -> Self {
        let change_pct = watch.current_change_pct.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0);
        let status = if change_pct >= 0.0 {
            "ok"
        } else if change_pct > -10.0 {
            "ok"
        } else if change_pct > -20.0 {
            "warn"
        } else {
            "bad"
        };

        Self {
            id: ID(watch.id.to_string()),
            token_address: watch.token_address,
            symbol: watch.symbol,
            price_usd: watch.current_price_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            change_pct,
            alert_threshold_pct: watch.alert_threshold_pct.to_string().parse().unwrap_or(-20.0),
            status: status.to_string(),
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlSimulationResult {
    pub id: ID,
    pub action: String,
    pub amount_usd: f64,
    pub health_factor_before: Option<f64>,
    pub health_factor_after: Option<f64>,
    pub debt_before: Option<f64>,
    pub debt_after: Option<f64>,
    pub gas_estimate: i64,
    pub gas_cost_usd: f64,
    pub expires_at: DateTime<Utc>,
}

impl From<SimulationResult> for GqlSimulationResult {
    fn from(sim: SimulationResult) -> Self {
        Self {
            id: ID(sim.id.to_string()),
            action: sim.action.as_str().to_string(),
            amount_usd: sim.amount_usd,
            health_factor_before: sim.health_factor_before,
            health_factor_after: sim.health_factor_after,
            debt_before: sim.debt_before,
            debt_after: sim.debt_after,
            gas_estimate: sim.gas_estimate as i64,
            gas_cost_usd: sim.gas_cost_usd,
            expires_at: sim.expires_at,
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlAlert {
    pub id: ID,
    pub alert_type: String,
    pub position_id: Option<String>,
    pub position_type: Option<String>,
    pub current_value: f64,
    pub previous_value: f64,
    pub threshold: f64,
    pub suggested_action: Option<String>,
    pub simulation: Option<GqlSimulationResult>,
    pub fired_at: DateTime<Utc>,
    pub status: GqlAlertStatus,
}

impl From<Alert> for GqlAlert {
    fn from(alert: Alert) -> Self {
        let status = if alert.resolved_at.is_some() {
            GqlAlertStatus::Resolved
        } else if alert.snoozed_until.map(|t| t > Utc::now()).unwrap_or(false) {
            GqlAlertStatus::Snoozed
        } else {
            GqlAlertStatus::Pending
        };

        Self {
            id: ID(alert.id.to_string()),
            alert_type: alert.alert_type,
            position_id: alert.position_id.map(|id| id.to_string()),
            position_type: alert.position_type,
            current_value: alert.current_value.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            previous_value: alert.previous_value.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            threshold: alert.threshold.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            suggested_action: alert.suggested_action,
            simulation: alert.simulation_result.map(|s| s.0.into()),
            fired_at: alert.fired_at,
            status,
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlTransaction {
    pub id: ID,
    pub tx_type: String,
    pub chain: String,
    pub tx_hash: Option<String>,
    pub status: String,
    pub amount_usd: f64,
    pub gas_estimate: i64,
    pub gas_used: Option<i64>,
    pub gas_cost_usd: Option<f64>,
    pub is_autopilot: bool,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

impl From<Transaction> for GqlTransaction {
    fn from(tx: Transaction) -> Self {
        Self {
            id: ID(tx.id.to_string()),
            tx_type: tx.tx_type,
            chain: tx.chain,
            tx_hash: tx.tx_hash,
            status: tx.status.unwrap_or_else(|| "pending".to_string()),
            amount_usd: tx.amount_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            gas_estimate: tx.gas_estimate.unwrap_or(0),
            gas_used: tx.gas_used,
            gas_cost_usd: tx.gas_cost_usd.map(|d| d.to_string().parse().unwrap_or(0.0)),
            is_autopilot: tx.is_autopilot,
            submitted_at: tx.submitted_at,
            confirmed_at: tx.confirmed_at,
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlGlobalStats {
    pub total_saved_usd: f64,
    pub saved_this_week_usd: f64,
    pub total_positions: i32,
}

impl From<GlobalStats> for GqlGlobalStats {
    fn from(stats: GlobalStats) -> Self {
        Self {
            total_saved_usd: stats.total_saved_usd.to_string().parse().unwrap_or(0.0),
            saved_this_week_usd: stats.saved_this_week_usd.to_string().parse().unwrap_or(0.0),
            total_positions: stats.total_positions,
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlUserStats {
    pub positions_monitored: i32,
    pub alerts_prevented: i32,
    pub total_saved_usd: f64,
}

#[derive(SimpleObject)]
pub struct GqlHealthDataPoint {
    pub time: DateTime<Utc>,
    pub health_factor: f64,
    pub collateral_usd: f64,
    pub debt_usd: f64,
}

impl From<HealthDataPoint> for GqlHealthDataPoint {
    fn from(point: HealthDataPoint) -> Self {
        Self {
            time: point.time,
            health_factor: point.health_factor.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            collateral_usd: point.collateral_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
            debt_usd: point.debt_usd.map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
        }
    }
}

#[derive(SimpleObject)]
pub struct GqlGuardianSigner {
    pub id: ID,
    pub signer_address: String,
    pub permissions: GqlSignerPermissions,
    pub created_at: DateTime<Utc>,
}

#[derive(SimpleObject)]
pub struct GqlSignerPermissions {
    pub can_repay: bool,
    pub can_rebalance: bool,
    pub can_withdraw: bool,
    pub max_single_action_usd: f64,
    pub allowed_protocols: Vec<String>,
}

impl From<GuardianSigner> for GqlGuardianSigner {
    fn from(signer: GuardianSigner) -> Self {
        let perms = signer.permissions.0;
        Self {
            id: ID(signer.id.to_string()),
            signer_address: signer.signer_address,
            permissions: GqlSignerPermissions {
                can_repay: perms.can_repay,
                can_rebalance: perms.can_rebalance,
                can_withdraw: perms.can_withdraw,
                max_single_action_usd: perms.max_single_action_usd,
                allowed_protocols: perms.allowed_protocols,
            },
            created_at: signer.created_at,
        }
    }
}

#[derive(SimpleObject)]
pub struct SubscriptionResult {
    pub stream_id: String,
    pub rate_per_second: f64,
}

// ============= Input Types =============

#[derive(InputObject)]
pub struct SignerPermissionsInput {
    pub can_repay: Option<bool>,
    pub can_rebalance: Option<bool>,
    pub can_withdraw: Option<bool>,
    pub max_single_action_usd: Option<f64>,
    pub allowed_protocols: Option<Vec<String>>,
}

impl From<SignerPermissionsInput> for SignerPermissions {
    fn from(input: SignerPermissionsInput) -> Self {
        let default = SignerPermissions::default();
        Self {
            can_repay: input.can_repay.unwrap_or(default.can_repay),
            can_rebalance: input.can_rebalance.unwrap_or(default.can_rebalance),
            can_withdraw: input.can_withdraw.unwrap_or(default.can_withdraw),
            max_single_action_usd: input.max_single_action_usd.unwrap_or(default.max_single_action_usd),
            allowed_protocols: input.allowed_protocols.unwrap_or(default.allowed_protocols),
        }
    }
}

// ============= Query Root =============

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get current authenticated user
    async fn me(&self, ctx: &Context<'_>) -> async_graphql::Result<GqlUser> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let user = db.get_user(*user_id).await?
            .ok_or("User not found")?;

        Ok(user.into())
    }

    /// Get lending positions for current user
    async fn lending_positions(
        &self,
        ctx: &Context<'_>,
        protocol: Option<GqlProtocol>,
    ) -> async_graphql::Result<Vec<GqlLendingPosition>> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let positions = db.get_lending_positions(*user_id).await?;

        let filtered: Vec<GqlLendingPosition> = positions
            .into_iter()
            .filter(|p| {
                if let Some(proto) = protocol {
                    match proto {
                        GqlProtocol::AaveV3 => p.protocol == "aave_v3",
                        GqlProtocol::Morpho => p.protocol == "morpho",
                        GqlProtocol::Spark => p.protocol == "spark",
                        GqlProtocol::Compound => p.protocol == "compound",
                        GqlProtocol::Euler => p.protocol == "euler",
                        _ => false,
                    }
                } else {
                    true
                }
            })
            .map(Into::into)
            .collect();

        Ok(filtered)
    }

    /// Get LP positions for current user
    async fn lp_positions(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<GqlLpPosition>> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let positions = db.get_lp_positions(*user_id).await?;
        Ok(positions.into_iter().map(Into::into).collect())
    }

    /// Get token watchlist for current user
    async fn token_watchlist(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<GqlTokenWatch>> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let watches = db.get_token_watchlist(*user_id).await?;
        Ok(watches.into_iter().map(Into::into).collect())
    }

    /// Get alerts for current user
    async fn alerts(
        &self,
        ctx: &Context<'_>,
        status: Option<GqlAlertStatus>,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<GqlAlert>> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let limit = limit.unwrap_or(20) as i64;
        let alerts = db.get_pending_alerts(*user_id, limit).await?;

        let filtered: Vec<GqlAlert> = alerts
            .into_iter()
            .map(Into::into)
            .filter(|a: &GqlAlert| {
                if let Some(s) = status {
                    a.status == s
                } else {
                    true
                }
            })
            .collect();

        Ok(filtered)
    }

    /// Get a specific alert
    async fn alert(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<GqlAlert>> {
        let db = ctx.data::<Database>()?;
        let alert_id = Uuid::parse_str(&id)?;

        let alert = db.get_alert(alert_id).await?;
        Ok(alert.map(Into::into))
    }

    /// Get transaction history
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<GqlTransaction>> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let limit = limit.unwrap_or(50) as i64;
        let txs = db.get_user_transactions(*user_id, limit).await?;

        Ok(txs.into_iter().map(Into::into).collect())
    }

    /// Get health factor history for a position
    async fn health_history(
        &self,
        ctx: &Context<'_>,
        position_id: ID,
        range: TimeRange,
    ) -> async_graphql::Result<Vec<GqlHealthDataPoint>> {
        let db = ctx.data::<Database>()?;
        let pos_id = Uuid::parse_str(&position_id)?;

        let since = Utc::now() - range.to_duration();
        let history = db.get_health_history(pos_id, since).await?;

        Ok(history.into_iter().map(Into::into).collect())
    }

    /// Get global stats for landing page
    async fn global_stats(&self, ctx: &Context<'_>) -> async_graphql::Result<GqlGlobalStats> {
        let db = ctx.data::<Database>()?;
        let stats = db.get_global_stats().await?;
        Ok(stats.into())
    }

    /// Get user-specific stats
    async fn user_stats(&self, ctx: &Context<'_>) -> async_graphql::Result<GqlUserStats> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let positions = db.get_lending_positions(*user_id).await?;
        let lp_positions = db.get_lp_positions(*user_id).await?;
        let txs = db.get_user_transactions(*user_id, 1000).await?;

        let confirmed_txs: Vec<_> = txs.iter().filter(|t| t.status.as_deref() == Some("confirmed")).collect();
        let total_saved: f64 = confirmed_txs
            .iter()
            .filter_map(|t| t.amount_usd.as_ref())
            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0) * 0.1) // Estimated saved amount
            .sum();

        Ok(GqlUserStats {
            positions_monitored: (positions.len() + lp_positions.len()) as i32,
            alerts_prevented: confirmed_txs.len() as i32,
            total_saved_usd: total_saved,
        })
    }
}

// ============= Mutation Root =============

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Add a guardian signer for autopilot
    async fn add_guardian_signer(
        &self,
        ctx: &Context<'_>,
        permissions: SignerPermissionsInput,
    ) -> async_graphql::Result<GqlGuardianSigner> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        // Generate a new signer address (in production, this would be from KMS)
        let signer_address = format!("0x{}", hex::encode(&uuid::Uuid::new_v4().as_bytes()[..20]));

        let signer = db.create_guardian_signer(
            *user_id,
            &signer_address,
            permissions.into(),
        ).await?;

        Ok(signer.into())
    }

    /// Revoke a guardian signer
    async fn revoke_guardian_signer(
        &self,
        ctx: &Context<'_>,
        signer_id: ID,
    ) -> async_graphql::Result<bool> {
        let db = ctx.data::<Database>()?;
        let id = Uuid::parse_str(&signer_id)?;

        db.revoke_guardian_signer(id).await?;
        Ok(true)
    }

    /// Enable/disable autopilot
    async fn set_autopilot(
        &self,
        ctx: &Context<'_>,
        enabled: bool,
        budget_usd: Option<f64>,
    ) -> async_graphql::Result<GqlUser> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let budget = budget_usd.map(|b| Decimal::from_f64_retain(b).unwrap_or_default());
        db.update_autopilot(*user_id, enabled, budget).await?;

        let user = db.get_user(*user_id).await?.ok_or("User not found")?;
        Ok(user.into())
    }

    /// Snooze an alert
    async fn snooze_alert(
        &self,
        ctx: &Context<'_>,
        alert_id: ID,
        duration_minutes: i32,
    ) -> async_graphql::Result<GqlAlert> {
        let db = ctx.data::<Database>()?;
        let id = Uuid::parse_str(&alert_id)?;

        let until = Utc::now() + Duration::minutes(duration_minutes as i64);
        db.snooze_alert(id, until).await?;

        let alert = db.get_alert(id).await?.ok_or("Alert not found")?;
        Ok(alert.into())
    }

    /// Add a token to watchlist
    async fn add_to_watchlist(
        &self,
        ctx: &Context<'_>,
        token_address: String,
        alert_threshold_pct: Option<f64>,
    ) -> async_graphql::Result<GqlTokenWatch> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        let threshold = Decimal::from_f64_retain(alert_threshold_pct.unwrap_or(-20.0))
            .unwrap_or_else(|| Decimal::from(-20));

        // In production, fetch current price from Chainlink
        let reference_price = Decimal::from(0);

        let watch = db.add_token_watch(
            *user_id,
            &token_address,
            "ethereum",
            None,
            reference_price,
            threshold,
        ).await?;

        Ok(watch.into())
    }

    /// Remove token from watchlist
    async fn remove_from_watchlist(
        &self,
        ctx: &Context<'_>,
        watch_id: ID,
    ) -> async_graphql::Result<bool> {
        let db = ctx.data::<Database>()?;
        let id = Uuid::parse_str(&watch_id)?;

        db.remove_token_watch(id).await?;
        Ok(true)
    }

    /// Start subscription ($19/month streaming)
    async fn start_subscription(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<SubscriptionResult> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        // In production, this would call x402 contract
        let stream_id = format!("stream_{}", Uuid::new_v4());

        db.update_user_tier(*user_id, "autopilot", Some(&stream_id)).await?;

        Ok(SubscriptionResult {
            stream_id,
            rate_per_second: 0.000007, // $19/month
        })
    }

    /// Cancel subscription
    async fn cancel_subscription(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let user_id = ctx.data::<Uuid>()?;
        let db = ctx.data::<Database>()?;

        db.update_user_tier(*user_id, "free", None).await?;
        Ok(true)
    }
}

// ============= Schema Builder =============

pub type SafetyNetSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema(db: Database, store: Arc<PositionStore>) -> SafetyNetSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db)
        .data(store)
        .finish()
}
