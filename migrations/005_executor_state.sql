-- Migration: 005_executor_state
-- Adds executor state tracking columns for autopilot transactions

-- Transaction state tracking
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS state VARCHAR(20) DEFAULT 'pending';
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS state_history JSONB DEFAULT '[]';
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS signed_bytes BYTEA;
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS nonce BIGINT;
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS retry_count INT DEFAULT 0;
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS error_message TEXT;

-- Index for pending transaction lookups
CREATE INDEX IF NOT EXISTS idx_tx_pending
ON transactions(user_id, status) WHERE status IN ('pending', 'simulated', 'submitted');

-- Index for user transaction history
CREATE INDEX IF NOT EXISTS idx_tx_user_history
ON transactions(user_id, simulated_at DESC);

-- Daily spend reset tracking for autopilot
ALTER TABLE users ADD COLUMN IF NOT EXISTS autopilot_daily_reset_at TIMESTAMPTZ DEFAULT NOW();

-- Add chain_id to transactions
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS chain_id BIGINT DEFAULT 1;

-- Add simulation metadata
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS simulation_id UUID;
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS simulation_expires_at TIMESTAMPTZ;
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS gas_price_gwei NUMERIC(20, 8);

-- Update status enum constraint (if exists, drop and recreate)
-- Note: PostgreSQL doesn't have native enum, so status is VARCHAR

-- Create function to reset daily spend at midnight UTC
CREATE OR REPLACE FUNCTION reset_daily_autopilot_spend()
RETURNS void AS $$
BEGIN
    UPDATE users
    SET autopilot_daily_spent_usd = 0,
        autopilot_daily_reset_at = NOW()
    WHERE autopilot_daily_reset_at < DATE_TRUNC('day', NOW() AT TIME ZONE 'UTC');
END;
$$ LANGUAGE plpgsql;

-- Create index for autopilot-enabled users
CREATE INDEX IF NOT EXISTS idx_users_autopilot
ON users(id) WHERE autopilot_enabled = true;

-- Add block_confirmed to transactions
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS block_confirmed BIGINT;

-- Comments for documentation
COMMENT ON COLUMN transactions.state IS 'Current state: pending, simulated, approved, signed, submitted, confirmed, failed, cancelled';
COMMENT ON COLUMN transactions.state_history IS 'JSON array of state transitions with timestamps';
COMMENT ON COLUMN transactions.signed_bytes IS 'Raw signed transaction bytes (hex-encoded in application)';
COMMENT ON COLUMN transactions.nonce IS 'Ethereum transaction nonce';
COMMENT ON COLUMN transactions.retry_count IS 'Number of submission retry attempts';
COMMENT ON COLUMN users.autopilot_daily_reset_at IS 'Last time the daily autopilot spend was reset';
