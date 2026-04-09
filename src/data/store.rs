use crate::data::models::{LendingPosition, LpPosition, TokenWatch, PositionStatus};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use uuid::Uuid;

/// In-memory position store for fast access during block processing
/// Syncs with database periodically
pub struct PositionStore {
    // Lending positions indexed by position ID
    lending_positions: DashMap<Uuid, LendingPosition>,

    // LP positions indexed by position ID
    lp_positions: DashMap<Uuid, LpPosition>,

    // Token watchlist indexed by watch ID
    token_watchlist: DashMap<Uuid, TokenWatch>,

    // User -> position IDs mapping for fast user lookups
    user_lending_positions: DashMap<Uuid, Vec<Uuid>>,
    user_lp_positions: DashMap<Uuid, Vec<Uuid>>,
    user_token_watches: DashMap<Uuid, Vec<Uuid>>,

    // Active wallets we're monitoring (for block processing)
    active_wallets: RwLock<Vec<String>>,

    // Last processed block
    last_block: RwLock<u64>,
}

impl PositionStore {
    pub fn new() -> Self {
        Self {
            lending_positions: DashMap::new(),
            lp_positions: DashMap::new(),
            token_watchlist: DashMap::new(),
            user_lending_positions: DashMap::new(),
            user_lp_positions: DashMap::new(),
            user_token_watches: DashMap::new(),
            active_wallets: RwLock::new(Vec::new()),
            last_block: RwLock::new(0),
        }
    }

    // ============= Lending Positions =============

    pub fn insert_lending_position(&self, position: LendingPosition) {
        let user_id = position.user_id;
        let position_id = position.id;

        self.lending_positions.insert(position_id, position);

        self.user_lending_positions
            .entry(user_id)
            .or_default()
            .push(position_id);
    }

    pub fn get_lending_position(&self, id: Uuid) -> Option<LendingPosition> {
        self.lending_positions.get(&id).map(|r| r.clone())
    }

    pub fn get_active_lending_positions(&self) -> Vec<LendingPosition> {
        self.lending_positions
            .iter()
            .filter(|p| p.is_active)
            .map(|p| p.clone())
            .collect()
    }

    pub fn get_user_lending_positions(&self, user_id: Uuid) -> Vec<LendingPosition> {
        self.user_lending_positions
            .get(&user_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.lending_positions.get(id).map(|p| p.clone()))
                    .filter(|p| p.is_active)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn update_health_factor(&self, position_id: Uuid, health_factor: f64, block_number: u64) {
        if let Some(mut position) = self.lending_positions.get_mut(&position_id) {
            position.health_factor = Some(rust_decimal::Decimal::from_f64_retain(health_factor).unwrap_or_default());
            position.block_number = block_number as i64;
            position.indexed_at = chrono::Utc::now();
        }
    }

    pub fn get_critical_positions(&self) -> Vec<LendingPosition> {
        self.lending_positions
            .iter()
            .filter(|p| p.is_active && p.status() == PositionStatus::Critical)
            .map(|p| p.clone())
            .collect()
    }

    // ============= LP Positions =============

    pub fn insert_lp_position(&self, position: LpPosition) {
        let user_id = position.user_id;
        let position_id = position.id;

        self.lp_positions.insert(position_id, position);

        self.user_lp_positions
            .entry(user_id)
            .or_default()
            .push(position_id);
    }

    pub fn get_lp_position(&self, id: Uuid) -> Option<LpPosition> {
        self.lp_positions.get(&id).map(|r| r.clone())
    }

    pub fn get_active_lp_positions(&self) -> Vec<LpPosition> {
        self.lp_positions
            .iter()
            .filter(|p| p.is_active)
            .map(|p| p.clone())
            .collect()
    }

    pub fn get_user_lp_positions(&self, user_id: Uuid) -> Vec<LpPosition> {
        self.user_lp_positions
            .get(&user_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.lp_positions.get(id).map(|p| p.clone()))
                    .filter(|p| p.is_active)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn update_lp_range_status(&self, position_id: Uuid, in_range: bool, current_tick: i32, block_number: u64) {
        if let Some(mut position) = self.lp_positions.get_mut(&position_id) {
            position.in_range = Some(in_range);
            position.current_tick = Some(current_tick);
            position.block_number = Some(block_number as i64);
            position.indexed_at = Some(chrono::Utc::now());
        }
    }

    pub fn get_out_of_range_positions(&self) -> Vec<LpPosition> {
        self.lp_positions
            .iter()
            .filter(|p| p.is_active && p.in_range == Some(false))
            .map(|p| p.clone())
            .collect()
    }

    // ============= Token Watchlist =============

    pub fn insert_token_watch(&self, watch: TokenWatch) {
        let user_id = watch.user_id;
        let watch_id = watch.id;

        self.token_watchlist.insert(watch_id, watch);

        self.user_token_watches
            .entry(user_id)
            .or_default()
            .push(watch_id);
    }

    pub fn get_token_watch(&self, id: Uuid) -> Option<TokenWatch> {
        self.token_watchlist.get(&id).map(|r| r.clone())
    }

    pub fn get_all_token_watches(&self) -> Vec<TokenWatch> {
        self.token_watchlist
            .iter()
            .map(|w| w.clone())
            .collect()
    }

    pub fn get_user_token_watches(&self, user_id: Uuid) -> Vec<TokenWatch> {
        self.user_token_watches
            .get(&user_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.token_watchlist.get(id).map(|w| w.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn update_token_price(&self, watch_id: Uuid, price_usd: f64, change_pct: f64) {
        if let Some(mut watch) = self.token_watchlist.get_mut(&watch_id) {
            watch.current_price_usd = Some(rust_decimal::Decimal::from_f64_retain(price_usd).unwrap_or_default());
            watch.current_change_pct = Some(rust_decimal::Decimal::from_f64_retain(change_pct).unwrap_or_default());
        }
    }

    pub fn remove_token_watch(&self, watch_id: Uuid) -> Option<TokenWatch> {
        if let Some((_, watch)) = self.token_watchlist.remove(&watch_id) {
            // Remove from user mapping
            if let Some(mut user_watches) = self.user_token_watches.get_mut(&watch.user_id) {
                user_watches.retain(|id| *id != watch_id);
            }
            Some(watch)
        } else {
            None
        }
    }

    // ============= Active Wallets =============

    pub fn add_active_wallet(&self, wallet: String) {
        let mut wallets = self.active_wallets.write();
        if !wallets.contains(&wallet) {
            wallets.push(wallet);
        }
    }

    pub fn get_active_wallets(&self) -> Vec<String> {
        self.active_wallets.read().clone()
    }

    pub fn remove_active_wallet(&self, wallet: &str) {
        let mut wallets = self.active_wallets.write();
        wallets.retain(|w| w != wallet);
    }

    // ============= Block Tracking =============

    pub fn set_last_block(&self, block: u64) {
        *self.last_block.write() = block;
    }

    pub fn get_last_block(&self) -> u64 {
        *self.last_block.read()
    }

    // ============= Stats =============

    pub fn position_count(&self) -> (usize, usize, usize) {
        (
            self.lending_positions.len(),
            self.lp_positions.len(),
            self.token_watchlist.len(),
        )
    }
}

impl Default for PositionStore {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedPositionStore = Arc<PositionStore>;
