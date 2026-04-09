use crate::data::models::*;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ============= User Operations =============

    pub async fn get_user_by_wallet(&self, wallet: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE wallet_address = $1"
        )
        .bind(wallet.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn get_user(&self, id: Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn create_user(&self, wallet: &str) -> Result<User> {
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (wallet_address, tier, trial_ends_at)
            VALUES ($1, 'free', NOW() + INTERVAL '14 days')
            RETURNING *
            "#
        )
        .bind(wallet.to_lowercase())
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn update_user_last_seen(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE users SET last_seen_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_user_tier(&self, id: Uuid, tier: &str, stream_id: Option<&str>) -> Result<()> {
        sqlx::query(
            "UPDATE users SET tier = $1, subscription_stream_id = $2 WHERE id = $3"
        )
        .bind(tier)
        .bind(stream_id)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_autopilot(&self, id: Uuid, enabled: bool, budget: Option<Decimal>) -> Result<()> {
        sqlx::query(
            "UPDATE users SET autopilot_enabled = $1, autopilot_budget_usd = $2 WHERE id = $3"
        )
        .bind(enabled)
        .bind(budget)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_fcm_token(&self, id: Uuid, token: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE users SET fcm_token = $1 WHERE id = $2")
            .bind(token)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_telegram_chat_id(&self, id: Uuid, chat_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE users SET telegram_chat_id = $1 WHERE id = $2")
            .bind(chat_id)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_email(&self, id: Uuid, email: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE users SET email = $1 WHERE id = $2")
            .bind(email)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_notifications_enabled(&self, id: Uuid, enabled: bool) -> Result<()> {
        sqlx::query("UPDATE users SET notifications_enabled = $1 WHERE id = $2")
            .bind(enabled)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ============= Guardian Signer Operations =============

    pub async fn create_guardian_signer(
        &self,
        user_id: Uuid,
        signer_address: &str,
        permissions: SignerPermissions,
    ) -> Result<GuardianSigner> {
        let signer = sqlx::query_as::<_, GuardianSigner>(
            r#"
            INSERT INTO guardian_signers (user_id, signer_address, permissions)
            VALUES ($1, $2, $3)
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(signer_address.to_lowercase())
        .bind(sqlx::types::Json(permissions))
        .fetch_one(&self.pool)
        .await?;

        Ok(signer)
    }

    pub async fn get_active_guardian_signer(&self, user_id: Uuid) -> Result<Option<GuardianSigner>> {
        let signer = sqlx::query_as::<_, GuardianSigner>(
            "SELECT * FROM guardian_signers WHERE user_id = $1 AND revoked_at IS NULL"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(signer)
    }

    pub async fn revoke_guardian_signer(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE guardian_signers SET revoked_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ============= Lending Position Operations =============

    pub async fn upsert_lending_position(
        &self,
        user_id: Uuid,
        protocol: &str,
        chain: &str,
        collateral_usd: Decimal,
        debt_usd: Decimal,
        health_factor: Decimal,
        liquidation_threshold: Decimal,
        block_number: i64,
    ) -> Result<LendingPosition> {
        let position = sqlx::query_as::<_, LendingPosition>(
            r#"
            INSERT INTO lending_positions (
                user_id, protocol, chain, collateral_usd, debt_usd,
                health_factor, liquidation_threshold, block_number, indexed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT (user_id, protocol, chain) DO UPDATE SET
                collateral_usd = $4,
                debt_usd = $5,
                health_factor = $6,
                liquidation_threshold = $7,
                block_number = $8,
                indexed_at = NOW(),
                is_active = true
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(protocol)
        .bind(chain)
        .bind(collateral_usd)
        .bind(debt_usd)
        .bind(health_factor)
        .bind(liquidation_threshold)
        .bind(block_number)
        .fetch_one(&self.pool)
        .await?;

        Ok(position)
    }

    pub async fn get_lending_positions(&self, user_id: Uuid) -> Result<Vec<LendingPosition>> {
        let positions = sqlx::query_as::<_, LendingPosition>(
            "SELECT * FROM lending_positions WHERE user_id = $1 AND is_active = true"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(positions)
    }

    pub async fn get_all_active_lending_positions(&self) -> Result<Vec<LendingPosition>> {
        let positions = sqlx::query_as::<_, LendingPosition>(
            "SELECT * FROM lending_positions WHERE is_active = true"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(positions)
    }

    pub async fn update_lending_position_health(
        &self,
        id: Uuid,
        health_factor: Decimal,
        collateral_usd: Decimal,
        debt_usd: Decimal,
        block_number: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE lending_positions
            SET health_factor = $1, collateral_usd = $2, debt_usd = $3,
                block_number = $4, indexed_at = NOW()
            WHERE id = $5
            "#
        )
        .bind(health_factor)
        .bind(collateral_usd)
        .bind(debt_usd)
        .bind(block_number)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_lending_alert_threshold(&self, id: Uuid, threshold: Decimal) -> Result<()> {
        sqlx::query("UPDATE lending_positions SET alert_threshold = $1 WHERE id = $2")
            .bind(threshold)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ============= LP Position Operations =============

    pub async fn upsert_lp_position(
        &self,
        user_id: Uuid,
        token_id: &str,
        chain: &str,
        token0: &str,
        token1: &str,
        fee_tier: i32,
        lower_tick: i32,
        upper_tick: i32,
    ) -> Result<LpPosition> {
        let position = sqlx::query_as::<_, LpPosition>(
            r#"
            INSERT INTO lp_positions (
                user_id, token_id, chain, token0, token1,
                fee_tier, lower_tick, upper_tick
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, token_id, chain) DO UPDATE SET
                token0 = $4,
                token1 = $5,
                fee_tier = $6,
                lower_tick = $7,
                upper_tick = $8,
                is_active = true
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(token_id)
        .bind(chain)
        .bind(token0)
        .bind(token1)
        .bind(fee_tier)
        .bind(lower_tick)
        .bind(upper_tick)
        .fetch_one(&self.pool)
        .await?;

        Ok(position)
    }

    pub async fn get_lp_positions(&self, user_id: Uuid) -> Result<Vec<LpPosition>> {
        let positions = sqlx::query_as::<_, LpPosition>(
            "SELECT * FROM lp_positions WHERE user_id = $1 AND is_active = true"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(positions)
    }

    pub async fn get_all_active_lp_positions(&self) -> Result<Vec<LpPosition>> {
        let positions = sqlx::query_as::<_, LpPosition>(
            "SELECT * FROM lp_positions WHERE is_active = true"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(positions)
    }

    pub async fn update_lp_position_status(
        &self,
        id: Uuid,
        in_range: bool,
        current_tick: i32,
        current_price_usd: Decimal,
        block_number: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE lp_positions
            SET in_range = $1, current_tick = $2, current_price_usd = $3,
                block_number = $4, indexed_at = NOW()
            WHERE id = $5
            "#
        )
        .bind(in_range)
        .bind(current_tick)
        .bind(current_price_usd)
        .bind(block_number)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============= Token Watchlist Operations =============

    pub async fn add_token_watch(
        &self,
        user_id: Uuid,
        token_address: &str,
        chain: &str,
        symbol: Option<&str>,
        reference_price: Decimal,
        alert_threshold_pct: Decimal,
    ) -> Result<TokenWatch> {
        let watch = sqlx::query_as::<_, TokenWatch>(
            r#"
            INSERT INTO token_watchlist (
                user_id, token_address, chain, symbol,
                reference_price_usd, reference_time, alert_threshold_pct
            )
            VALUES ($1, $2, $3, $4, $5, NOW(), $6)
            ON CONFLICT (user_id, token_address, chain) DO UPDATE SET
                symbol = $4,
                reference_price_usd = $5,
                reference_time = NOW(),
                alert_threshold_pct = $6
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(token_address.to_lowercase())
        .bind(chain)
        .bind(symbol)
        .bind(reference_price)
        .bind(alert_threshold_pct)
        .fetch_one(&self.pool)
        .await?;

        Ok(watch)
    }

    pub async fn get_token_watchlist(&self, user_id: Uuid) -> Result<Vec<TokenWatch>> {
        let watches = sqlx::query_as::<_, TokenWatch>(
            "SELECT * FROM token_watchlist WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(watches)
    }

    pub async fn remove_token_watch(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM token_watchlist WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_token_price(&self, id: Uuid, price: Decimal, change_pct: Decimal) -> Result<()> {
        sqlx::query(
            "UPDATE token_watchlist SET current_price_usd = $1, current_change_pct = $2 WHERE id = $3"
        )
        .bind(price)
        .bind(change_pct)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============= Health Factor History =============

    pub async fn insert_health_history(
        &self,
        position_id: Uuid,
        health_factor: Decimal,
        collateral_usd: Decimal,
        debt_usd: Decimal,
        block_number: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO health_factor_history (
                position_id, time, health_factor, collateral_usd, debt_usd, block_number
            )
            VALUES ($1, NOW(), $2, $3, $4, $5)
            "#
        )
        .bind(position_id)
        .bind(health_factor)
        .bind(collateral_usd)
        .bind(debt_usd)
        .bind(block_number)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_health_history(
        &self,
        position_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Vec<HealthDataPoint>> {
        let history = sqlx::query_as::<_, HealthDataPoint>(
            r#"
            SELECT * FROM health_factor_history
            WHERE position_id = $1 AND time >= $2
            ORDER BY time ASC
            "#
        )
        .bind(position_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;

        Ok(history)
    }

    // ============= Alert Operations =============

    pub async fn create_alert(
        &self,
        user_id: Uuid,
        position_id: Uuid,
        position_type: &str,
        alert_type: &str,
        current_value: Decimal,
        previous_value: Decimal,
        threshold: Decimal,
    ) -> Result<Alert> {
        let alert = sqlx::query_as::<_, Alert>(
            r#"
            INSERT INTO alerts (
                user_id, position_id, position_type, alert_type,
                current_value, previous_value, threshold
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(position_id)
        .bind(position_type)
        .bind(alert_type)
        .bind(current_value)
        .bind(previous_value)
        .bind(threshold)
        .fetch_one(&self.pool)
        .await?;

        Ok(alert)
    }

    pub async fn update_alert_simulation(
        &self,
        alert_id: Uuid,
        action: &str,
        amount_usd: Decimal,
        simulation: SimulationResult,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE alerts
            SET suggested_action = $1, suggested_amount_usd = $2,
                simulation_result = $3, simulation_expires_at = $4
            WHERE id = $5
            "#
        )
        .bind(action)
        .bind(amount_usd)
        .bind(sqlx::types::Json(simulation))
        .bind(expires_at)
        .bind(alert_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_pending_alerts(&self, user_id: Uuid, limit: i64) -> Result<Vec<Alert>> {
        let alerts = sqlx::query_as::<_, Alert>(
            r#"
            SELECT * FROM alerts
            WHERE user_id = $1 AND resolved_at IS NULL
            ORDER BY fired_at DESC
            LIMIT $2
            "#
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(alerts)
    }

    pub async fn get_alert(&self, id: Uuid) -> Result<Option<Alert>> {
        let alert = sqlx::query_as::<_, Alert>(
            "SELECT * FROM alerts WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(alert)
    }

    pub async fn snooze_alert(&self, id: Uuid, until: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "UPDATE alerts SET snoozed_until = $1, action_taken = 'snoozed' WHERE id = $2"
        )
        .bind(until)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn resolve_alert(&self, id: Uuid, action_taken: &str) -> Result<()> {
        sqlx::query(
            "UPDATE alerts SET resolved_at = NOW(), action_taken = $1 WHERE id = $2"
        )
        .bind(action_taken)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============= Transaction Operations =============

    pub async fn create_transaction(
        &self,
        user_id: Uuid,
        alert_id: Option<Uuid>,
        chain: &str,
        tx_type: &str,
        amount_usd: Decimal,
        gas_estimate: i64,
        is_autopilot: bool,
    ) -> Result<Transaction> {
        let tx = sqlx::query_as::<_, Transaction>(
            r#"
            INSERT INTO transactions (
                user_id, alert_id, chain, tx_type, amount_usd,
                gas_estimate, is_autopilot, status, simulated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', NOW())
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(alert_id)
        .bind(chain)
        .bind(tx_type)
        .bind(amount_usd)
        .bind(gas_estimate)
        .bind(is_autopilot)
        .fetch_one(&self.pool)
        .await?;

        Ok(tx)
    }

    pub async fn update_transaction_submitted(
        &self,
        id: Uuid,
        tx_hash: &str,
        used_private_mempool: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE transactions
            SET status = 'submitted', tx_hash = $1,
                used_private_mempool = $2, submitted_at = NOW()
            WHERE id = $3
            "#
        )
        .bind(tx_hash)
        .bind(used_private_mempool)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_transaction_confirmed(
        &self,
        id: Uuid,
        gas_used: i64,
        gas_cost_usd: Decimal,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE transactions
            SET status = 'confirmed', gas_used = $1,
                gas_cost_usd = $2, confirmed_at = NOW()
            WHERE id = $3
            "#
        )
        .bind(gas_used)
        .bind(gas_cost_usd)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_transaction_failed(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE transactions SET status = 'failed' WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_user_transactions(&self, user_id: Uuid, limit: i64) -> Result<Vec<Transaction>> {
        let txs = sqlx::query_as::<_, Transaction>(
            r#"
            SELECT * FROM transactions
            WHERE user_id = $1
            ORDER BY simulated_at DESC
            LIMIT $2
            "#
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(txs)
    }

    // ============= Billing Operations =============

    pub async fn create_billing_event(
        &self,
        user_id: Uuid,
        event_type: &str,
        amount_usd: Decimal,
        saved_amount_usd: Option<Decimal>,
        intervention_id: Option<Uuid>,
        tx_hash: Option<&str>,
    ) -> Result<BillingEvent> {
        let event = sqlx::query_as::<_, BillingEvent>(
            r#"
            INSERT INTO billing_events (
                user_id, event_type, amount_usd, saved_amount_usd,
                intervention_id, x402_tx_hash
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#
        )
        .bind(user_id)
        .bind(event_type)
        .bind(amount_usd)
        .bind(saved_amount_usd)
        .bind(intervention_id)
        .bind(tx_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(event)
    }

    // ============= Global Stats =============

    pub async fn get_global_stats(&self) -> Result<GlobalStats> {
        let stats = sqlx::query_as::<_, GlobalStats>(
            "SELECT * FROM global_stats WHERE id = 1"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
    }

    pub async fn update_global_stats(
        &self,
        total_saved_usd: Decimal,
        saved_this_week_usd: Decimal,
        total_positions: i32,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE global_stats
            SET total_saved_usd = $1, saved_this_week_usd = $2,
                total_positions = $3, updated_at = NOW()
            WHERE id = 1
            "#
        )
        .bind(total_saved_usd)
        .bind(saved_this_week_usd)
        .bind(total_positions)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============= Migrations =============

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await?;

        Ok(())
    }
}
