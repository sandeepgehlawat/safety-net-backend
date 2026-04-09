-- Billing Events
CREATE TABLE billing_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),

    event_type VARCHAR(30) NOT NULL,
    amount_usd DECIMAL(12, 6) NOT NULL,

    -- For success fees
    saved_amount_usd DECIMAL(20, 6),
    intervention_id UUID REFERENCES transactions(id),

    -- Payment reference
    x402_tx_hash VARCHAR(66),
    x402_stream_id VARCHAR(66),

    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Global Stats (for landing page)
CREATE TABLE global_stats (
    id INT PRIMARY KEY DEFAULT 1,
    total_saved_usd DECIMAL(20, 2) DEFAULT 0,
    saved_this_week_usd DECIMAL(20, 2) DEFAULT 0,
    total_positions INT DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Initialize global stats
INSERT INTO global_stats (id) VALUES (1) ON CONFLICT DO NOTHING;

CREATE INDEX idx_billing_user_date ON billing_events(user_id, created_at);
