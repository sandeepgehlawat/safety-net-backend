-- Alerts
CREATE TABLE alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    position_id UUID,
    position_type VARCHAR(20),

    alert_type VARCHAR(30) NOT NULL,

    -- Alert data
    current_value DECIMAL(20, 6),
    previous_value DECIMAL(20, 6),
    threshold DECIMAL(20, 6),

    -- Pre-simulated remediation
    suggested_action VARCHAR(30),
    suggested_amount_usd DECIMAL(20, 6),
    simulation_result JSONB,
    simulation_expires_at TIMESTAMPTZ,

    -- Delivery
    fired_at TIMESTAMPTZ DEFAULT NOW(),
    delivery_status JSONB,

    -- Resolution
    action_taken VARCHAR(30),
    snoozed_until TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ
);

-- Transactions (interventions we executed)
CREATE TABLE transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    alert_id UUID REFERENCES alerts(id),

    chain VARCHAR(20) NOT NULL,
    tx_type VARCHAR(30) NOT NULL,

    -- Execution details
    tx_hash VARCHAR(66),
    status VARCHAR(20),

    -- Gas
    gas_estimate BIGINT,
    gas_used BIGINT,
    gas_cost_usd DECIMAL(10, 4),

    -- Amounts
    amount_usd DECIMAL(20, 6),

    -- Autopilot flag
    is_autopilot BOOLEAN DEFAULT false,
    used_private_mempool BOOLEAN DEFAULT false,

    -- Timestamps
    simulated_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ,
    confirmed_at TIMESTAMPTZ
);

CREATE INDEX idx_alerts_pending ON alerts(user_id, fired_at) WHERE resolved_at IS NULL;
CREATE INDEX idx_alerts_position ON alerts(position_id);
CREATE INDEX idx_transactions_status ON transactions(status, submitted_at);
CREATE INDEX idx_transactions_user ON transactions(user_id);
