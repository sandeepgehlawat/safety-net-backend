/// Fee calculation utilities
pub struct FeeCalculator;

impl FeeCalculator {
    /// Calculate success fee (10% of saved amount)
    pub fn success_fee(saved_amount_usd: f64) -> f64 {
        saved_amount_usd * 0.10
    }

    /// Calculate per-check fee
    pub fn check_fee() -> f64 {
        0.0004
    }

    /// Calculate subscription rate per second
    pub fn subscription_rate_per_second() -> f64 {
        // $19/month
        19.0 / (30.0 * 24.0 * 60.0 * 60.0)
    }

    /// Calculate how much liquidation would have cost
    /// Liquidation penalty is typically 5-10% of debt
    pub fn liquidation_penalty(debt_usd: f64, penalty_pct: f64) -> f64 {
        debt_usd * penalty_pct
    }

    /// Calculate amount saved by preventing liquidation
    /// This is the liquidation penalty that would have been charged
    pub fn calculate_saved_amount(
        debt_usd: f64,
        penalty_pct: f64,
        gas_cost_usd: f64,
    ) -> f64 {
        let penalty = Self::liquidation_penalty(debt_usd, penalty_pct);
        // Saved = penalty avoided - gas cost of intervention
        (penalty - gas_cost_usd).max(0.0)
    }

    /// Calculate whether intervention is profitable for user
    /// Returns true if cost of intervention < cost of liquidation
    pub fn is_intervention_profitable(
        _repay_amount_usd: f64,
        gas_cost_usd: f64,
        debt_usd: f64,
        penalty_pct: f64,
    ) -> bool {
        let intervention_cost = gas_cost_usd; // Repay is just moving their own money
        let liquidation_cost = Self::liquidation_penalty(debt_usd, penalty_pct);

        intervention_cost < liquidation_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_fee() {
        assert!((FeeCalculator::success_fee(1000.0) - 100.0_f64).abs() < 0.01);
        assert!((FeeCalculator::success_fee(5000.0) - 500.0_f64).abs() < 0.01);
    }

    #[test]
    fn test_check_fee() {
        assert!((FeeCalculator::check_fee() - 0.0004_f64).abs() < 0.00001);
    }

    #[test]
    fn test_subscription_rate() {
        let rate = FeeCalculator::subscription_rate_per_second();
        // Should be about 0.000007
        assert!(rate > 0.000006_f64 && rate < 0.000008_f64);
    }

    #[test]
    fn test_liquidation_penalty() {
        // 5% penalty on $10,000 debt
        let penalty = FeeCalculator::liquidation_penalty(10000.0, 0.05);
        assert!((penalty - 500.0_f64).abs() < 0.01);
    }

    #[test]
    fn test_saved_amount() {
        // $10,000 debt, 5% penalty, $0.50 gas
        let saved = FeeCalculator::calculate_saved_amount(10000.0, 0.05, 0.50);
        // Should save $499.50
        assert!((saved - 499.50_f64).abs() < 0.01);
    }

    #[test]
    fn test_intervention_profitable() {
        // $10,000 debt, 5% liquidation penalty ($500)
        // $0.50 gas cost for intervention
        assert!(FeeCalculator::is_intervention_profitable(500.0, 0.50, 10000.0, 0.05));

        // Edge case: very high gas cost
        assert!(!FeeCalculator::is_intervention_profitable(500.0, 600.0, 10000.0, 0.05));
    }
}
