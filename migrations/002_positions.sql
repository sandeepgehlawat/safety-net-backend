-- Lending Positions (Aave, Morpho, Spark, Compound, Euler)
CREATE TABLE lending_positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,

    protocol VARCHAR(20) NOT NULL,
    chain VARCHAR(20) NOT NULL DEFAULT 'ethereum',

    -- Position data
    collateral_usd DECIMAL(20, 6),
    debt_usd DECIMAL(20, 6),
    health_factor DECIMAL(10, 4),
    liquidation_threshold DECIMAL(5, 4),

    -- Tracking
    block_number BIGINT NOT NULL,
    indexed_at TIMESTAMPTZ NOT NULL,
    is_active BOOLEAN DEFAULT true,

    -- Alert config
    alert_threshold DECIMAL(10, 4) DEFAULT 1.20,

    UNIQUE(user_id, protocol, chain)
);

-- LP Positions (Uniswap v3)
CREATE TABLE lp_positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,

    protocol VARCHAR(20) NOT NULL DEFAULT 'uniswap_v3',
    chain VARCHAR(20) NOT NULL DEFAULT 'ethereum',

    -- NFT token ID
    token_id VARCHAR(78) NOT NULL,

    -- Pool info
    token0 VARCHAR(42) NOT NULL,
    token1 VARCHAR(42) NOT NULL,
    fee_tier INT NOT NULL,

    -- Range
    lower_tick INT NOT NULL,
    upper_tick INT NOT NULL,
    current_tick INT,

    -- Liquidity
    liquidity NUMERIC(78, 0),

    -- Status
    in_range BOOLEAN,
    lower_price_usd DECIMAL(20, 8),
    upper_price_usd DECIMAL(20, 8),
    current_price_usd DECIMAL(20, 8),

    -- Tracking
    block_number BIGINT,
    indexed_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT true,

    UNIQUE(user_id, token_id, chain)
);

-- Token Watchlist (for drawdown alerts)
CREATE TABLE token_watchlist (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,

    token_address VARCHAR(42) NOT NULL,
    chain VARCHAR(20) NOT NULL DEFAULT 'ethereum',
    symbol VARCHAR(20),

    -- Reference point for drawdown calculation
    reference_price_usd DECIMAL(20, 8),
    reference_time TIMESTAMPTZ,

    -- Alert threshold (e.g., -20 for 20% drop)
    alert_threshold_pct DECIMAL(5, 2) DEFAULT -20.0,

    -- Current state
    current_price_usd DECIMAL(20, 8),
    current_change_pct DECIMAL(5, 2),

    UNIQUE(user_id, token_address, chain)
);

-- Health Factor History (for charts - would be TimescaleDB in production)
CREATE TABLE health_factor_history (
    position_id UUID NOT NULL REFERENCES lending_positions(id) ON DELETE CASCADE,
    time TIMESTAMPTZ NOT NULL,
    health_factor DECIMAL(10, 4),
    collateral_usd DECIMAL(20, 6),
    debt_usd DECIMAL(20, 6),
    block_number BIGINT,
    PRIMARY KEY (position_id, time)
);

CREATE INDEX idx_lending_positions_user ON lending_positions(user_id) WHERE is_active = true;
CREATE INDEX idx_lp_positions_user ON lp_positions(user_id) WHERE is_active = true;
CREATE INDEX idx_token_watchlist_user ON token_watchlist(user_id);
CREATE INDEX idx_health_history_position_time ON health_factor_history(position_id, time DESC);
