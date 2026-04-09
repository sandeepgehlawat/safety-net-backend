-- Users & Authentication
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_address VARCHAR(42) UNIQUE NOT NULL,

    -- Tier: 'free', 'pay_as_save', 'autopilot'
    tier VARCHAR(20) DEFAULT 'free',
    trial_ends_at TIMESTAMPTZ,
    subscription_stream_id VARCHAR(66),

    -- Autopilot settings
    autopilot_enabled BOOLEAN DEFAULT false,
    autopilot_budget_usd DECIMAL(12, 2),
    autopilot_daily_spent_usd DECIMAL(12, 2) DEFAULT 0,

    -- Notification settings
    fcm_token VARCHAR(255),
    telegram_chat_id VARCHAR(50),
    email VARCHAR(255),
    notifications_enabled BOOLEAN DEFAULT true,

    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ
);

-- Guardian Signers (scoped permissions for autopilot)
CREATE TABLE guardian_signers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    signer_address VARCHAR(42) NOT NULL,

    -- Permissions JSON
    permissions JSONB NOT NULL DEFAULT '{
        "can_repay": true,
        "can_rebalance": true,
        "can_withdraw": false,
        "max_single_action_usd": 5000,
        "allowed_protocols": ["aave_v3", "morpho", "uniswap_v3"]
    }',

    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,

    UNIQUE(user_id, signer_address)
);

CREATE INDEX idx_users_wallet ON users(wallet_address);
CREATE INDEX idx_guardian_signers_user ON guardian_signers(user_id) WHERE revoked_at IS NULL;
