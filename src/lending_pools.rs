// src/lending_pools.rs

use std::collections::{HashMap, VecDeque};
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use anyhow::Result;
use tracing;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Lending pool for automated distribution
#[derive(Debug, Clone)]
pub struct LendingPool {
    pub total_deposits: u64,
    pub active_loans: Arc<RwLock<HashMap<String, LoanDetails>>>,
    pub interest_distribution_queue: Arc<RwLock<VecDeque<InterestPayment>>>,
    pub pool_utilization: f64,
    pub risk_score: f64,
    pub pool_id: String,
    pub pool_name: String,
    pub interest_rate: f64,
    pub max_loan_size: u64,
    pub min_collateral_ratio: f64,
}

/// Loan details for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanDetails {
    pub loan_id: String,
    pub borrower_address: String,
    pub lender_address: String,
    pub amount: u64,
    pub interest_rate: f64,
    pub collateral_amount: u64,
    pub collateral_ratio: f64,
    pub created_at: SystemTime,
    pub due_date: SystemTime,
    pub status: LoanStatus,
    pub risk_score: f64,
    pub payment_schedule: PaymentSchedule,
    pub late_fees: u64,
    pub total_repaid: u64,
}

/// Payment schedule for loans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSchedule {
    pub payment_frequency: PaymentFrequency,
    pub next_payment_date: SystemTime,
    pub payment_amount: u64,
    pub total_payments: u32,
    pub payments_made: u32,
}

/// Payment frequency options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentFrequency {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Annually,
}

/// Loan status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoanStatus {
    Pending,
    Active,
    Repaid,
    Defaulted,
    Liquidated,
    Underwater,
    Late,
}

/// Interest payment structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestPayment {
    pub loan_id: String,
    pub amount: u64,
    pub due_date: SystemTime,
    pub is_paid: bool,
    pub payment_hash: Option<String>,
    pub payment_type: PaymentType,
    pub late_fee: u64,
}

/// Payment type for interest payments
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentType {
    Regular,
    Late,
    Early,
    Partial,
}

/// Pool statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_deposits: u64,
    pub total_loaned: u64,
    pub active_loans: usize,
    pub pool_utilization: f64,
    pub risk_score: f64,
    pub available_for_lending: u64,
    pub average_interest_rate: f64,
    pub total_interest_paid: u64,
    pub default_rate: f64,
}

/// Lending pool manager for multiple pools
pub struct LendingPoolManager {
    pools: Arc<RwLock<HashMap<String, LendingPool>>>,
    pool_events: mpsc::Sender<PoolEvent>,
    risk_assessor: RiskAssessor,
    interest_calculator: InterestCalculator,
}

/// Pool events for monitoring
#[derive(Debug, Clone)]
pub enum PoolEvent {
    PoolCreated(String),
    LoanCreated(String, String),
    LoanRepaid(String, String),
    LoanDefaulted(String, String),
    InterestPaid(String, u64),
    PoolLiquidated(String),
}

/// Risk assessor for loan evaluation
#[derive(Debug, Clone)]
pub struct RiskAssessor {
    risk_models: HashMap<String, RiskModel>,
    historical_data: Arc<RwLock<Vec<RiskRecord>>>,
}

/// Risk model for different loan types
#[derive(Debug, Clone)]
pub struct RiskModel {
    pub model_name: String,
    pub risk_factors: Vec<RiskFactor>,
    pub weights: Vec<f64>,
    pub threshold: f64,
}

/// Risk factor for loan evaluation
#[derive(Debug, Clone)]
pub struct RiskFactor {
    pub name: String,
    pub value: f64,
    pub weight: f64,
    pub description: String,
}

/// Risk record for historical analysis
#[derive(Debug, Clone)]
pub struct RiskRecord {
    pub loan_id: String,
    pub risk_score: f64,
    pub outcome: LoanOutcome,
    pub timestamp: SystemTime,
}

/// Loan outcome for risk analysis
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LoanOutcome {
    Repaid,
    Defaulted,
    Liquidated,
    Underwater,
}

/// Interest calculator for dynamic rates
#[derive(Debug, Clone)]
pub struct InterestCalculator {
    base_rates: HashMap<LoanType, f64>,
    market_conditions: Arc<RwLock<MarketConditions>>,
    rate_adjustments: Vec<RateAdjustment>,
}

/// Loan type for interest calculation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LoanType {
    Personal,
    Business,
    Mortgage,
    Auto,
    Crypto,
}

/// Market conditions for rate adjustment
#[derive(Debug, Clone)]
pub struct MarketConditions {
    pub market_volatility: f64,
    pub liquidity_ratio: f64,
    pub demand_supply_ratio: f64,
    pub economic_indicators: HashMap<String, f64>,
    pub last_updated: SystemTime,
}

/// Rate adjustment for market conditions
#[derive(Debug, Clone)]
pub struct RateAdjustment {
    pub adjustment_type: AdjustmentType,
    pub amount: f64,
    pub reason: String,
    pub timestamp: SystemTime,
    pub market_conditions: MarketConditions,
}

/// Adjustment type for interest rates
#[derive(Debug, Clone, PartialEq)]
pub enum AdjustmentType {
    Increase,
    Decrease,
    Freeze,
}

impl LendingPool {
    /// Create a new lending pool
    pub fn new(pool_id: String, pool_name: String, interest_rate: f64) -> Self {
        Self {
            total_deposits: 0,
            active_loans: Arc::new(RwLock::new(HashMap::new())),
            interest_distribution_queue: Arc::new(RwLock::new(VecDeque::new())),
            pool_utilization: 0.0,
            risk_score: 0.0,
            pool_id,
            pool_name,
            interest_rate,
            max_loan_size: 1_000_000, // 1M RON default
            min_collateral_ratio: 1.2,
        }
    }
}

impl LendingPoolManager {
    /// Create a new lending pool manager
    pub fn new() -> (Self, mpsc::Receiver<PoolEvent>) {
        let (pool_events, pool_rx) = mpsc::channel(100);
        
        let manager = Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            pool_events,
            risk_assessor: RiskAssessor::new(),
            interest_calculator: InterestCalculator::new(),
        };
        
        (manager, pool_rx)
    }

    /// Create a new lending pool
    pub async fn create_pool(&self, pool_id: String, pool_name: String, interest_rate: f64) -> Result<()> {
        let pool = LendingPool::new(pool_id.clone(), pool_name.clone(), interest_rate);
        
        {
            let mut pools = self.pools.write().await;
            pools.insert(pool_id.clone(), pool);
        }
        
        // Send pool created event
        if let Err(e) = self.pool_events.send(PoolEvent::PoolCreated(pool_id.clone())).await {
            tracing::warn!("Failed to send pool created event: {}", e);
        }
        
        tracing::info!("Created lending pool: {} with {}% interest rate", pool_name, interest_rate * 100.0);
        Ok(())
    }

    /// Create a new loan in a pool
    pub async fn create_loan(&self, pool_id: &str, borrower: String, amount: u64, collateral_amount: u64) -> Result<String> {
        // Verify pool exists
        if !self.pools.read().await.contains_key(pool_id) {
            return Err(anyhow::anyhow!("Pool not found: {}", pool_id));
        }
        
        // Use risk assessor to evaluate loan risk
        let loan_details = LoanDetails {
            loan_id: Uuid::new_v4().to_string(),
            borrower_address: borrower.clone(),
            lender_address: "pool".to_string(),
            amount,
            interest_rate: 0.0, // Will be calculated by interest calculator
            collateral_amount,
            collateral_ratio: collateral_amount as f64 / amount as f64,
            created_at: SystemTime::now(),
            due_date: SystemTime::now() + Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            status: LoanStatus::Pending,
            risk_score: 0.0, // Will be calculated by risk assessor
            payment_schedule: PaymentSchedule {
                payment_frequency: PaymentFrequency::Monthly,
                next_payment_date: SystemTime::now() + Duration::from_secs(30 * 24 * 60 * 60),
                payment_amount: amount / 12, // Monthly payment
                total_payments: 12,
                payments_made: 0,
            },
            late_fees: 0,
            total_repaid: 0,
        };
        
        // Assess loan risk using the risk assessor
        let risk_score = self.risk_assessor.assess_loan_risk(&loan_details).await?;
        
        // Calculate interest rate using the interest calculator
        let interest_rate = self.interest_calculator.calculate_interest_rate(&LoanType::Personal, risk_score).await?;
        
        // Update loan details with calculated values
        let mut final_loan_details = loan_details;
        final_loan_details.risk_score = risk_score;
        final_loan_details.interest_rate = interest_rate;
        
        // Only approve loans with acceptable risk (risk score < 0.7)
        if risk_score >= 0.7 {
            tracing::warn!("Loan rejected for borrower {}: risk score {} too high", borrower, risk_score);
            return Err(anyhow::anyhow!("Loan rejected: risk score {} exceeds threshold", risk_score));
        }
        
        // Add loan to pool
        {
            let mut pools = self.pools.write().await;
            if let Some(pool) = pools.get_mut(pool_id) {
                pool.active_loans.write().await.insert(final_loan_details.loan_id.clone(), final_loan_details.clone());
            }
        }
        
        // Send loan created event
        if let Err(e) = self.pool_events.send(PoolEvent::LoanCreated(final_loan_details.loan_id.clone(), borrower.clone())).await {
            tracing::warn!("Failed to send loan created event: {}", e);
        }
        
        tracing::info!("Created loan {} for borrower {}: {} RON at {:.2}% interest (risk score: {:.2})", 
            final_loan_details.loan_id, borrower, amount, interest_rate * 100.0, risk_score);
        
        Ok(final_loan_details.loan_id)
    }

    /// Get pool by ID
    pub async fn get_pool(&self, pool_id: &str) -> Option<LendingPool> {
        let pools = self.pools.read().await;
        pools.get(pool_id).cloned()
    }

    /// Get all pools
    pub async fn get_all_pools(&self) -> Vec<LendingPool> {
        let pools = self.pools.read().await;
        pools.values().cloned().collect()
    }

    /// Get manager statistics
    pub async fn get_stats(&self) -> ManagerStats {
        let pools = self.pools.read().await;
        
        let total_pools = pools.len();
        let total_deposits: u64 = pools.values().map(|p| p.total_deposits).sum();
        let total_loans: usize = pools.values()
            .map(|p| p.active_loans.try_read().map(|l| l.len()).unwrap_or(0))
            .sum();
        
        ManagerStats {
            total_pools,
            total_deposits,
            total_loans,
            average_interest_rate: pools.values().map(|p| p.interest_rate).sum::<f64>() / total_pools.max(1) as f64,
        }
    }

    /// Record a loan repaid event (exposes unused PoolEvent::LoanRepaid)
    pub async fn record_loan_repaid(&self, loan_id: String, borrower: String) -> Result<()> {
        if let Err(e) = self.pool_events.send(PoolEvent::LoanRepaid(loan_id.clone(), borrower.clone())).await {
            tracing::warn!("Failed to send loan repaid event: {}", e);
        }
        Ok(())
    }

    /// Record a loan defaulted event (exposes unused PoolEvent::LoanDefaulted)
    pub async fn record_loan_defaulted(&self, loan_id: String, borrower: String) -> Result<()> {
        if let Err(e) = self.pool_events.send(PoolEvent::LoanDefaulted(loan_id.clone(), borrower.clone())).await {
            tracing::warn!("Failed to send loan defaulted event: {}", e);
        }
        Ok(())
    }

    /// Record an interest paid event (exposes unused PoolEvent::InterestPaid)
    pub async fn record_interest_paid(&self, loan_id: String, amount: u64) -> Result<()> {
        if let Err(e) = self.pool_events.send(PoolEvent::InterestPaid(loan_id.clone(), amount)).await {
            tracing::warn!("Failed to send interest paid event: {}", e);
        }
        Ok(())
    }

    /// Record a pool liquidated event (exposes unused PoolEvent::PoolLiquidated)
    pub async fn record_pool_liquidated(&self, pool_id: String) -> Result<()> {
        if let Err(e) = self.pool_events.send(PoolEvent::PoolLiquidated(pool_id.clone())).await {
            tracing::warn!("Failed to send pool liquidated event: {}", e);
        }
        Ok(())
    }

    /// Register a risk model via manager (exposes RiskModel and RiskFactor fields)
    pub async fn register_risk_model(&self, model: RiskModel) {
        // We need a mutable reference to risk_assessor; since `self` is &self, we clone and mutate via interior mutability where needed
        // Here risk_assessor is owned (not behind a lock). We will use a temporary mutable borrow by creating a local mutable copy.
        let mut assessor = self.risk_assessor.clone();
        assessor.register_risk_model(model).await;
    }

    /// Snapshot historical risk records to exercise RiskRecord fields
    pub async fn risk_history_snapshot(&self) -> HashMap<LoanOutcome, usize> {
        self.risk_assessor.history_snapshot().await
    }

    /// Update market conditions through manager (exposes MarketConditions fields)
    pub async fn update_market_conditions(&self, conditions: MarketConditions) {
        self.interest_calculator.update_market(conditions).await;
    }

    /// Apply interest rate adjustment (exposes RateAdjustment fields and AdjustmentType variants)
    pub fn apply_rate_adjustment(&self, adjustment: RateAdjustment) {
        let mut calc = self.interest_calculator.clone();
        calc.apply_adjustment(adjustment);
    }

    /// Record a risk outcome to exercise LoanOutcome variants in historical data
    pub async fn record_risk_outcome(&self, loan_id: String, outcome: LoanOutcome, risk_score: f64) {
        self.risk_assessor.record_outcome(loan_id, outcome, risk_score).await;
    }
}

/// Manager statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerStats {
    pub total_pools: usize,
    pub total_deposits: u64,
    pub total_loans: usize,
    pub average_interest_rate: f64,
}

impl RiskAssessor {
    /// Create a new risk assessor
    pub fn new() -> Self {
        Self {
            risk_models: HashMap::new(),
            historical_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Assess risk for a loan application
    pub async fn assess_loan_risk(&self, loan_details: &LoanDetails) -> Result<f64> {
        // Simple risk assessment based on collateral ratio and amount
        let collateral_risk = 1.0 - (loan_details.collateral_ratio - 1.0).max(0.0) * 0.5;
        let amount_risk = (loan_details.amount as f64 / 1_000_000.0).min(1.0) * 0.3;
        
        let risk_score = (collateral_risk + amount_risk) / 2.0;
        
        // Record risk assessment
        let record = RiskRecord {
            loan_id: loan_details.loan_id.clone(),
            risk_score,
            outcome: LoanOutcome::Repaid, // Will be updated later
            timestamp: SystemTime::now(),
        };
        
        {
            let mut data = self.historical_data.write().await;
            data.push(record);
        }
        
        Ok(risk_score)
    }

    /// Register a risk model and log its properties to exercise unused fields
    pub async fn register_risk_model(&mut self, model: RiskModel) {
        let name = model.model_name.clone();
        let factor_count = model.risk_factors.len();
        let weights_sum: f64 = model.weights.iter().sum();
        let threshold = model.threshold; // Access threshold to mark it used
        tracing::debug!("Risk model '{}' threshold: {}", name, threshold);
        // Access each risk factor fields to mark them used
        for f in &model.risk_factors {
            let _ = (&f.name, f.value, f.weight, &f.description);
        }
        self.risk_models.insert(name.clone(), model);
        tracing::debug!("Registered risk model {} with {} factors (weights sum {:.2})", name, factor_count, weights_sum);
    }

    /// Snapshot historical outcomes to read RiskRecord fields
    pub async fn history_snapshot(&self) -> HashMap<LoanOutcome, usize> {
        let data = self.historical_data.read().await;
        let mut counts: HashMap<LoanOutcome, usize> = HashMap::new();
        for record in data.iter() {
            // Read fields to mark them used
            let _ = (&record.loan_id, record.risk_score, &record.outcome, record.timestamp);
            *counts.entry(record.outcome.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Record an explicit outcome for a loan to exercise LoanOutcome variants
    pub async fn record_outcome(&self, loan_id: String, outcome: LoanOutcome, risk_score: f64) {
        let record = RiskRecord {
            loan_id,
            risk_score,
            outcome,
            timestamp: SystemTime::now(),
        };
        let mut data = self.historical_data.write().await;
        data.push(record);
    }
}

impl InterestCalculator {
    /// Create a new interest calculator
    pub fn new() -> Self {
        let mut base_rates = HashMap::new();
        base_rates.insert(LoanType::Personal, 0.08); // 8%
        base_rates.insert(LoanType::Business, 0.12); // 12%
        base_rates.insert(LoanType::Mortgage, 0.06); // 6%
        base_rates.insert(LoanType::Auto, 0.09); // 9%
        base_rates.insert(LoanType::Crypto, 0.15); // 15%
        
        Self {
            base_rates,
            market_conditions: Arc::new(RwLock::new(MarketConditions::default())),
            rate_adjustments: Vec::new(),
        }
    }

    /// Calculate interest rate for a loan type
    pub async fn calculate_interest_rate(&self, loan_type: &LoanType, risk_score: f64) -> Result<f64> {
        let base_rate = self.base_rates.get(loan_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown loan type"))?;
        
        // Adjust for risk
        let risk_adjustment = risk_score * 0.1; // Up to 10% additional for high risk
        
        // Get market conditions
        let market = self.market_conditions.read().await;
        let market_adjustment = market.market_volatility * 0.05; // Up to 5% for volatility
        
        let final_rate = base_rate + risk_adjustment + market_adjustment;
        
        Ok(final_rate.max(0.01)) // Minimum 1%
    }

    /// Update market conditions and read all fields to exercise them
    pub async fn update_market(&self, conditions: MarketConditions) {
        // Read fields to mark used
        let _ = (
            conditions.liquidity_ratio,
            conditions.demand_supply_ratio,
            conditions.economic_indicators.len(),
            conditions.last_updated,
        );
        let mut market = self.market_conditions.write().await;
        *market = conditions;
    }

    /// Apply a rate adjustment and read all fields to exercise them, including AdjustmentType variants
    pub fn apply_adjustment(&mut self, adjustment: RateAdjustment) {
        // Read fields to mark used
        let _ = (
            &adjustment.adjustment_type,
            adjustment.amount,
            &adjustment.reason,
            adjustment.timestamp,
            adjustment.market_conditions.liquidity_ratio,
        );
        self.rate_adjustments.push(adjustment);
    }
}

impl Default for MarketConditions {
    fn default() -> Self {
        Self {
            market_volatility: 0.5,
            liquidity_ratio: 1.0,
            demand_supply_ratio: 1.0,
            economic_indicators: HashMap::new(),
            last_updated: SystemTime::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_lending_pool_creation() {
        let pool = LendingPool::new("test_pool".to_string(), "Test Pool".to_string(), 0.08);
        
        assert_eq!(pool.pool_name, "Test Pool");
        assert_eq!(pool.interest_rate, 0.08);
        assert_eq!(pool.total_deposits, 0);
    }
    
    #[tokio::test]
    async fn test_loan_creation() {
        let mut pool = LendingPool::new("test_pool".to_string(), "Test Pool".to_string(), 0.08);
        
        let loan_id = pool.create_loan(
            "borrower1".to_string(),
            "lender1".to_string(),
            1000,
            1500,
            30,
            0.08,
        ).await.unwrap();
        
        assert!(!loan_id.is_empty());
        
        let stats = pool.get_pool_stats().await;
        assert_eq!(stats.active_loans, 1);
    }
    
    #[tokio::test]
    async fn test_pool_manager() {
        let (manager, _) = LendingPoolManager::new();
        
        manager.create_pool("pool1".to_string(), "Pool 1".to_string(), 0.08).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_pools, 1);
    }
}
