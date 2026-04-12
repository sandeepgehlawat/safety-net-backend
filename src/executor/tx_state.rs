//! Transaction State Machine
//!
//! Manages transaction lifecycle states from pending to confirmed/failed.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Transaction states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TxState {
    /// Initial state - transaction created but not simulated
    Pending {
        created_at: DateTime<Utc>,
    },

    /// Transaction simulated via Tenderly
    Simulated {
        simulation_id: Uuid,
        gas_estimate: u64,
        expires_at: DateTime<Utc>,
    },

    /// User approved the transaction for execution
    Approved {
        approved_at: DateTime<Utc>,
    },

    /// Transaction signed by guardian wallet
    Signed {
        signed_at: DateTime<Utc>,
    },

    /// Transaction submitted to mempool
    Submitted {
        tx_hash: String,
        block_target: u64,
        submitted_at: DateTime<Utc>,
    },

    /// Transaction confirmed on-chain
    Confirmed {
        tx_hash: String,
        block_number: u64,
        gas_used: u64,
        confirmed_at: DateTime<Utc>,
    },

    /// Transaction failed at some stage
    Failed {
        reason: String,
        stage: String,
        failed_at: DateTime<Utc>,
    },

    /// Transaction cancelled by user
    Cancelled {
        cancelled_at: DateTime<Utc>,
    },
}

impl TxState {
    /// Create a new pending transaction
    pub fn new_pending() -> Self {
        TxState::Pending {
            created_at: Utc::now(),
        }
    }

    /// Get the state name as a string
    pub fn name(&self) -> &'static str {
        match self {
            TxState::Pending { .. } => "pending",
            TxState::Simulated { .. } => "simulated",
            TxState::Approved { .. } => "approved",
            TxState::Signed { .. } => "signed",
            TxState::Submitted { .. } => "submitted",
            TxState::Confirmed { .. } => "confirmed",
            TxState::Failed { .. } => "failed",
            TxState::Cancelled { .. } => "cancelled",
        }
    }

    /// Check if this transition is valid
    pub fn can_transition_to(&self, next: &TxState) -> bool {
        match (self, next) {
            // Pending can go to Simulated or Failed
            (TxState::Pending { .. }, TxState::Simulated { .. }) => true,
            (TxState::Pending { .. }, TxState::Failed { .. }) => true,
            (TxState::Pending { .. }, TxState::Cancelled { .. }) => true,

            // Simulated can go to Approved, Failed, or back to Pending (refresh)
            (TxState::Simulated { .. }, TxState::Approved { .. }) => true,
            (TxState::Simulated { .. }, TxState::Failed { .. }) => true,
            (TxState::Simulated { .. }, TxState::Cancelled { .. }) => true,
            (TxState::Simulated { .. }, TxState::Pending { .. }) => true, // Refresh simulation

            // Approved can go to Signed or Failed
            (TxState::Approved { .. }, TxState::Signed { .. }) => true,
            (TxState::Approved { .. }, TxState::Failed { .. }) => true,
            (TxState::Approved { .. }, TxState::Cancelled { .. }) => true,

            // Signed can go to Submitted or Failed
            (TxState::Signed { .. }, TxState::Submitted { .. }) => true,
            (TxState::Signed { .. }, TxState::Failed { .. }) => true,

            // Submitted can go to Confirmed or Failed
            (TxState::Submitted { .. }, TxState::Confirmed { .. }) => true,
            (TxState::Submitted { .. }, TxState::Failed { .. }) => true,

            // Terminal states cannot transition
            (TxState::Confirmed { .. }, _) => false,
            (TxState::Failed { .. }, _) => false,
            (TxState::Cancelled { .. }, _) => false,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TxState::Confirmed { .. } | TxState::Failed { .. } | TxState::Cancelled { .. }
        )
    }

    /// Check if simulation has expired
    pub fn is_expired(&self) -> bool {
        match self {
            TxState::Simulated { expires_at, .. } => Utc::now() > *expires_at,
            _ => false,
        }
    }

    /// Get simulation expiry time (if simulated)
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        match self {
            TxState::Simulated { expires_at, .. } => Some(*expires_at),
            _ => None,
        }
    }

    /// Get the transaction hash (if submitted or confirmed)
    pub fn tx_hash(&self) -> Option<&str> {
        match self {
            TxState::Submitted { tx_hash, .. } => Some(tx_hash),
            TxState::Confirmed { tx_hash, .. } => Some(tx_hash),
            _ => None,
        }
    }

    /// Create a simulated state with 5-minute expiry
    pub fn simulated(simulation_id: Uuid, gas_estimate: u64) -> Self {
        TxState::Simulated {
            simulation_id,
            gas_estimate,
            expires_at: Utc::now() + Duration::minutes(5),
        }
    }

    /// Create a failed state
    pub fn failed(reason: impl Into<String>, stage: impl Into<String>) -> Self {
        TxState::Failed {
            reason: reason.into(),
            stage: stage.into(),
            failed_at: Utc::now(),
        }
    }

    /// Create a confirmed state
    pub fn confirmed(tx_hash: String, block_number: u64, gas_used: u64) -> Self {
        TxState::Confirmed {
            tx_hash,
            block_number,
            gas_used,
            confirmed_at: Utc::now(),
        }
    }
}

/// State history entry for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    pub state: String,
    pub timestamp: DateTime<Utc>,
    pub details: Option<String>,
}

impl StateHistoryEntry {
    pub fn new(state: &TxState, details: Option<String>) -> Self {
        Self {
            state: state.name().to_string(),
            timestamp: Utc::now(),
            details,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_state_transitions() {
        let pending = TxState::new_pending();
        let simulated = TxState::simulated(Uuid::new_v4(), 250_000);
        let approved = TxState::Approved {
            approved_at: Utc::now(),
        };
        let signed = TxState::Signed {
            signed_at: Utc::now(),
        };
        let submitted = TxState::Submitted {
            tx_hash: "0x123".to_string(),
            block_target: 12345,
            submitted_at: Utc::now(),
        };
        let confirmed = TxState::confirmed("0x123".to_string(), 12345, 200_000);

        // Happy path transitions
        assert!(pending.can_transition_to(&simulated));
        assert!(simulated.can_transition_to(&approved));
        assert!(approved.can_transition_to(&signed));
        assert!(signed.can_transition_to(&submitted));
        assert!(submitted.can_transition_to(&confirmed));
    }

    #[test]
    fn test_invalid_state_transitions() {
        let pending = TxState::new_pending();
        let simulated = TxState::simulated(Uuid::new_v4(), 250_000);
        let confirmed = TxState::confirmed("0x123".to_string(), 12345, 200_000);
        let approved = TxState::Approved {
            approved_at: Utc::now(),
        };

        // Can't skip states
        assert!(!pending.can_transition_to(&approved));
        assert!(!pending.can_transition_to(&confirmed));

        // Can't go backwards (except simulated -> pending for refresh)
        assert!(!approved.can_transition_to(&simulated));

        // Terminal states can't transition
        assert!(!confirmed.can_transition_to(&pending));
    }

    #[test]
    fn test_simulation_expiry() {
        // Non-expired simulation
        let simulated = TxState::simulated(Uuid::new_v4(), 250_000);
        assert!(!simulated.is_expired());

        // Expired simulation (manually created with past time)
        let expired = TxState::Simulated {
            simulation_id: Uuid::new_v4(),
            gas_estimate: 250_000,
            expires_at: Utc::now() - Duration::minutes(10),
        };
        assert!(expired.is_expired());
    }

    #[test]
    fn test_terminal_states() {
        let confirmed = TxState::confirmed("0x123".to_string(), 12345, 200_000);
        assert!(confirmed.is_terminal());

        let failed = TxState::failed("Simulation reverted", "simulation");
        assert!(failed.is_terminal());

        let cancelled = TxState::Cancelled {
            cancelled_at: Utc::now(),
        };
        assert!(cancelled.is_terminal());

        // Non-terminal states
        let pending = TxState::new_pending();
        assert!(!pending.is_terminal());

        let simulated = TxState::simulated(Uuid::new_v4(), 250_000);
        assert!(!simulated.is_terminal());
    }

    #[test]
    fn test_state_names() {
        assert_eq!(TxState::new_pending().name(), "pending");
        assert_eq!(
            TxState::simulated(Uuid::new_v4(), 250_000).name(),
            "simulated"
        );
        assert_eq!(
            TxState::Approved {
                approved_at: Utc::now()
            }
            .name(),
            "approved"
        );
        assert_eq!(TxState::Signed { signed_at: Utc::now() }.name(), "signed");
        assert_eq!(
            TxState::confirmed("0x123".to_string(), 12345, 200_000).name(),
            "confirmed"
        );
        assert_eq!(
            TxState::failed("test", "test").name(),
            "failed"
        );
    }

    #[test]
    fn test_tx_hash_extraction() {
        let pending = TxState::new_pending();
        assert!(pending.tx_hash().is_none());

        let submitted = TxState::Submitted {
            tx_hash: "0xabc".to_string(),
            block_target: 12345,
            submitted_at: Utc::now(),
        };
        assert_eq!(submitted.tx_hash(), Some("0xabc"));

        let confirmed = TxState::confirmed("0xdef".to_string(), 12346, 200_000);
        assert_eq!(confirmed.tx_hash(), Some("0xdef"));
    }

    #[test]
    fn test_state_history_entry() {
        let state = TxState::failed("Out of gas", "execution");
        let entry = StateHistoryEntry::new(&state, Some("User budget exceeded".to_string()));

        assert_eq!(entry.state, "failed");
        assert!(entry.details.is_some());
        assert_eq!(entry.details.unwrap(), "User budget exceeded");
    }

    #[test]
    fn test_fail_transitions_always_valid() {
        let failed = TxState::failed("Error", "test");

        let pending = TxState::new_pending();
        assert!(pending.can_transition_to(&failed));

        let simulated = TxState::simulated(Uuid::new_v4(), 250_000);
        assert!(simulated.can_transition_to(&failed));

        let approved = TxState::Approved {
            approved_at: Utc::now(),
        };
        assert!(approved.can_transition_to(&failed));

        let signed = TxState::Signed {
            signed_at: Utc::now(),
        };
        assert!(signed.can_transition_to(&failed));

        let submitted = TxState::Submitted {
            tx_hash: "0x123".to_string(),
            block_target: 12345,
            submitted_at: Utc::now(),
        };
        assert!(submitted.can_transition_to(&failed));
    }

    #[test]
    fn test_simulation_refresh_transition() {
        // Simulated can go back to pending for refresh
        let simulated = TxState::simulated(Uuid::new_v4(), 250_000);
        let pending = TxState::new_pending();

        assert!(simulated.can_transition_to(&pending));
    }
}
