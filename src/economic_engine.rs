// src/economic_engine.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use std::sync::Arc;
use anyhow::Result;
use tracing;
use uuid::Uuid;
use crate::lending_pools;

/// Network statistics for economic calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_transactions: u64,
    pub active_users: u64,
    pub network_utilization: f64, // 0.0 to 1.0
    pub average_transaction_value: u64,
    pub mesh_congestion_level: f64, // 0.0 to 1.0
    pub total_lending_volume: u64,
    pub total_borrowing_volume: u64,
    pub average_collateral_ratio: f64,
}

/// Interest rate engine for dynamic rate calculation
#[derive(Debug)]
pub struct InterestRateEngine {
    pub base_rate: f64,
    pub supply_demand_multiplier: Arc<RwLock<f64>>,
    pub network_utilization_factor: Arc<RwLock<f64>>,
    pub historical_rates: Arc<RwLock<Vec<(SystemTime, f64)>>>,
    pub rate_adjustment_history: Arc<RwLock<Vec<RateAdjustment>>>,
    pub rate_network_correlation: Arc<RwLock<Vec<RateNetworkCorrelation>>>,
}

/// Rate adjustment record for transparency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateAdjustment {
    pub timestamp: SystemTime,
    pub old_rate: f64,
    pub new_rate: f64,
    pub reason: String,
    pub network_stats: NetworkStats,
}

/// Rate and network correlation data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateNetworkCorrelation {
    pub timestamp: SystemTime,
    pub rate: f64,
    pub network_utilization: f64,
    pub active_peers: u32,
    pub transaction_volume: u64,
}

/// Lending pool for automated distribution
#[derive(Debug, Clone)]
pub struct LendingPool {
    pub total_deposits: u64,
    pub active_loans: Arc<RwLock<HashMap<String, LoanDetails>>>,
    pub interest_distribution_queue: Arc<RwLock<Vec<InterestPayment>>>,
    pub pool_utilization: f64,
    pub risk_score: f64,
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
}

/// Loan status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoanStatus {
    Active,
    Repaid,
    Defaulted,
    Liquidated,
    Underwater,
}

/// Interest payment structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestPayment {
    pub loan_id: String,
    pub amount: u64,
    pub due_date: SystemTime,
    pub is_paid: bool,
    pub payment_hash: Option<String>,
}

/// Lending eligibility assessment for transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingEligibility {
    pub eligible: bool,
    pub loan_amount: u64,
    pub interest_rate: f64,
    pub collateral_ratio: f64,
}

/// Collateral requirements for loans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralRequirements {
    pub minimum_ratio: f64,
    pub liquidation_threshold: f64,
    pub maintenance_margin: f64,
    pub risk_adjustment: f64,
}

impl InterestRateEngine {
    /// Create a new interest rate engine
    pub fn new() -> Self {
        Self {
            base_rate: 0.05, // 5% base rate
            supply_demand_multiplier: Arc::new(RwLock::new(1.0)),
            network_utilization_factor: Arc::new(RwLock::new(1.0)),
            historical_rates: Arc::new(RwLock::new(Vec::new())),
            rate_adjustment_history: Arc::new(RwLock::new(Vec::new())),
            rate_network_correlation: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Calculate dynamic lending rate based on network activity
    pub async fn calculate_lending_rate(&self, network_stats: &NetworkStats) -> f64 {
        let utilization_bonus = network_stats.network_utilization * 0.02; // Up to 2% bonus
        let congestion_penalty = network_stats.mesh_congestion_level * 0.01; // Up to 1% penalty
        let volume_bonus = if network_stats.total_lending_volume > 1_000_000 {
            0.005 // 0.5% bonus for high volume
        } else {
            0.0
        };
        
        // Read network utilization factor and supply-demand multiplier
        let network_factor = *self.network_utilization_factor.read().await;
        let supply_demand_mult = *self.supply_demand_multiplier.read().await;
        
        // Apply network utilization factor to utilization bonus
        let adjusted_utilization_bonus = utilization_bonus * network_factor;
        
        // Apply supply-demand multiplier to base calculation
        let base_calculation = self.base_rate + adjusted_utilization_bonus - congestion_penalty + volume_bonus;
        let calculated_rate = base_calculation * supply_demand_mult;
        
        // Record rate change
        self.record_rate_change(calculated_rate, network_stats).await;
        
        calculated_rate.max(0.01) // Minimum 1% rate
    }

    /// Calculate borrowing rate based on collateral ratio
    pub async fn calculate_borrowing_rate(&self, collateral_ratio: f64, network_stats: &NetworkStats) -> f64 {
        let base_borrowing_rate = self.calculate_lending_rate(network_stats).await + 0.02; // 2% premium over lending
        
        // Lower rates for higher collateral
        let collateral_discount = if collateral_ratio >= 2.0 {
            0.01 // 1% discount for 200%+ collateral
        } else if collateral_ratio >= 1.5 {
            0.005 // 0.5% discount for 150%+ collateral
        } else {
            0.0
        };
        
        // Risk premium for low collateral
        let risk_premium = if collateral_ratio < 1.2 {
            0.03 // 3% risk premium for low collateral
        } else if collateral_ratio < 1.5 {
            0.015 // 1.5% risk premium for moderate collateral
        } else {
            0.0
        };
        
        let final_rate = base_borrowing_rate - collateral_discount + risk_premium;
        final_rate.max(0.02) // Minimum 2% rate
    }

    /// Adjust rates for mesh congestion
    pub async fn adjust_rates_for_mesh_congestion(&self, congestion_level: f64) -> f64 {
        let adjustment = congestion_level * 0.005; // 0.5% adjustment per congestion level
        let new_rate = self.base_rate + adjustment;
        
        // Record adjustment
        let adjustment_record = RateAdjustment {
            timestamp: SystemTime::now(),
            old_rate: self.base_rate,
            new_rate,
            reason: format!("Mesh congestion adjustment: {:.2}%", congestion_level * 100.0),
            network_stats: NetworkStats {
                total_transactions: 0,
                active_users: 0,
                network_utilization: 0.0,
                average_transaction_value: 0,
                mesh_congestion_level: congestion_level,
                total_lending_volume: 0,
                total_borrowing_volume: 0,
                average_collateral_ratio: 0.0,
            },
        };
        
        {
            let mut history = self.rate_adjustment_history.write().await;
            history.push(adjustment_record);
        }
        
        new_rate
    }

    /// Record rate change for transparency
    async fn record_rate_change(&self, new_rate: f64, network_stats: &NetworkStats) {
        let mut history = self.historical_rates.write().await;
        history.push((SystemTime::now(), new_rate));
        
        // Keep only last 1000 rate changes
        if history.len() > 1000 {
            history.remove(0);
        }
        
        // Record network stats correlation with rate change
        let mut correlation_history = self.rate_network_correlation.write().await;
        correlation_history.push(RateNetworkCorrelation {
            timestamp: SystemTime::now(),
            rate: new_rate,
            network_utilization: network_stats.network_utilization,
            active_peers: network_stats.active_users as u32,
            transaction_volume: network_stats.total_transactions,
        });
        
        // Keep only last 500 correlations
        if correlation_history.len() > 500 {
            correlation_history.remove(0);
        }
        
        tracing::info!("Interest rate adjusted to {:.3}% based on network stats (utilization: {:.2}%, peers: {})", 
            new_rate * 100.0, network_stats.network_utilization * 100.0, network_stats.active_users);
    }

    /// Get rate history for analysis
    pub async fn get_rate_history(&self) -> Vec<(SystemTime, f64)> {
        self.historical_rates.read().await.clone()
    }

    /// Get rate adjustment history
    pub async fn get_adjustment_history(&self) -> Vec<RateAdjustment> {
        self.rate_adjustment_history.read().await.clone()
    }
}

impl LendingPool {
    /// Create a new lending pool
    pub fn new() -> Self {
        Self {
            total_deposits: 0,
            active_loans: Arc::new(RwLock::new(HashMap::new())),
            interest_distribution_queue: Arc::new(RwLock::new(Vec::new())),
            pool_utilization: 0.0,
            risk_score: 0.0,
        }
    }

    /// Create a new loan
    pub async fn create_loan(
        &mut self,
        borrower: String,
        lender: String,
        amount: u64,
        collateral: u64,
        duration_days: u32,
        interest_rate: f64,
    ) -> Result<String> {
        let loan_id = format!("loan_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let collateral_ratio = collateral as f64 / amount as f64;
        
        let loan = LoanDetails {
            loan_id: loan_id.clone(),
            borrower_address: borrower,
            lender_address: lender,
            amount,
            interest_rate,
            collateral_amount: collateral,
            collateral_ratio,
            created_at: SystemTime::now(),
            due_date: SystemTime::now() + Duration::from_secs(duration_days as u64 * 86400),
            status: LoanStatus::Active,
            risk_score: self.calculate_loan_risk(collateral_ratio, amount),
        };

        // Add to active loans
        {
            let mut loans = self.active_loans.write().await;
            loans.insert(loan_id.clone(), loan);
        }

        // Update pool utilization
        self.update_pool_utilization().await;

        let amount_ron = crate::web3::utils::wei_to_ron(amount);
        tracing::info!("Created loan {}: {:.6} RON ({} wei) with {:.2}x collateral", loan_id, amount_ron, amount, collateral_ratio);
        Ok(loan_id)
    }

    /// Calculate loan risk score
    fn calculate_loan_risk(&self, collateral_ratio: f64, amount: u64) -> f64 {
        let base_risk = 1.0 - (collateral_ratio - 1.0).max(0.0) * 0.5;
        let amount_risk = (amount as f64 / 1_000_000.0).min(1.0) * 0.3; // Higher amounts = higher risk
        
        (base_risk + amount_risk).max(0.1).min(1.0)
    }

    /// Update pool utilization metrics
    async fn update_pool_utilization(&mut self) {
        let loans = self.active_loans.read().await;
        let total_loaned = loans.values().map(|loan| loan.amount).sum::<u64>();
        
        self.pool_utilization = if self.total_deposits > 0 {
            total_loaned as f64 / self.total_deposits as f64
        } else {
            0.0
        };

        // Calculate overall risk score
        let total_risk: f64 = loans.values().map(|loan| loan.risk_score).sum();
        self.risk_score = if loans.len() > 0 {
            total_risk / loans.len() as f64
        } else {
            0.0
        };
    }

    /// Add deposit to lending pool
    pub async fn add_deposit(&mut self, amount: u64) -> Result<()> {
        self.total_deposits += amount;
        self.update_pool_utilization().await;
        
        let amount_ron = crate::web3::utils::wei_to_ron(amount);
        let total_ron = crate::web3::utils::wei_to_ron(self.total_deposits);
        tracing::info!("Added {:.6} RON ({} wei) deposit to lending pool. Total: {:.6} RON ({} wei)", 
            amount_ron, amount, total_ron, self.total_deposits);
        Ok(())
    }

    /// Get pool statistics
    pub async fn get_pool_stats(&self) -> PoolStats {
        let loans = self.active_loans.read().await;
        let total_loaned = loans.values().map(|loan| loan.amount).sum::<u64>();
        let active_loan_count = loans.len();
        
        PoolStats {
            total_deposits: self.total_deposits,
            total_loaned,
            active_loans: active_loan_count,
            pool_utilization: self.pool_utilization,
            risk_score: self.risk_score,
            available_for_lending: self.total_deposits.saturating_sub(total_loaned),
        }
    }

    /// Process loan repayment
    pub async fn process_repayment(&mut self, loan_id: &str, repayment_amount: u64) -> Result<bool> {
        let mut loans = self.active_loans.write().await;
        
        if let Some(loan) = loans.get_mut(loan_id) {
            if loan.status == LoanStatus::Active {
                loan.status = LoanStatus::Repaid;
                
                // Calculate interest payment
                let interest_amount = (loan.amount as f64 * loan.interest_rate) as u64;
                let total_repayment = loan.amount + interest_amount;
                
                if repayment_amount >= total_repayment {
                    // Add interest payment to distribution queue
                    let interest_payment = InterestPayment {
                        loan_id: loan_id.to_string(),
                        amount: interest_amount,
                        due_date: SystemTime::now(),
                        is_paid: false,
                        payment_hash: None,
                    };
                    
                    {
                        let mut queue = self.interest_distribution_queue.write().await;
                        queue.push(interest_payment);
                    }
                    
                    // Drop the loans lock before calling update_pool_utilization
                    drop(loans);
                    self.update_pool_utilization().await;
                    
                    let interest_ron = crate::web3::utils::wei_to_ron(interest_amount);
                    tracing::info!("Loan {} repaid successfully with {:.6} RON ({} wei) interest", loan_id, interest_ron, interest_amount);
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
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
}

/// Economic engine for managing the entire banking system
pub struct EconomicEngine {
    pub interest_rate_engine: Arc<InterestRateEngine>,
    pub lending_pools: Arc<RwLock<HashMap<String, LendingPool>>>,
    pub lending_pools_manager: Arc<RwLock<Option<Arc<lending_pools::LendingPoolManager>>>>,
    pub network_stats: Arc<RwLock<NetworkStats>>,
    pub collateral_requirements: CollateralRequirements,
    pub token_distribution: Arc<RwLock<HashMap<String, u64>>>,
}

impl EconomicEngine {
    /// Create a new economic engine
    pub fn new() -> Self {
        Self {
            interest_rate_engine: Arc::new(InterestRateEngine::new()),
            lending_pools: Arc::new(RwLock::new(HashMap::new())),
            lending_pools_manager: Arc::new(RwLock::new(None)),
            network_stats: Arc::new(RwLock::new(NetworkStats {
                total_transactions: 0,
                active_users: 0,
                network_utilization: 0.0,
                average_transaction_value: 0,
                mesh_congestion_level: 0.0,
                total_lending_volume: 0,
                total_borrowing_volume: 0,
                average_collateral_ratio: 0.0,
            })),
            collateral_requirements: CollateralRequirements {
                minimum_ratio: 1.2,
                liquidation_threshold: 1.1,
                maintenance_margin: 1.15,
                risk_adjustment: 0.1,
            },
            token_distribution: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update network statistics
    pub async fn update_network_stats(&self, stats: NetworkStats) -> Result<()> {
        let mut current_stats = self.network_stats.write().await;
        *current_stats = stats.clone();
        
        // Adjust interest rates based on new stats
        let new_lending_rate = self.interest_rate_engine.calculate_lending_rate(&stats).await;
        tracing::info!("Updated network stats, new lending rate: {:.3}%", new_lending_rate * 100.0);
        
        Ok(())
    }

    /// Create a new lending pool
    pub async fn create_lending_pool(&self, pool_name: String) -> Result<()> {
        let mut pools = self.lending_pools.write().await;
        pools.insert(pool_name.clone(), LendingPool::new());
        
        tracing::info!("Created new lending pool: {}", pool_name);
        Ok(())
    }

    /// Get economic engine statistics
    pub async fn get_economic_stats(&self) -> EconomicStats {
        let network_stats = self.network_stats.read().await;
        let pools = self.lending_pools.read().await;
        
        // Calculate total active loans by iterating through pools
        let mut total_active_loans = 0;
        let mut total_pool_deposits: u64 = 0;
        
        // Read pool_utilization and risk_score fields to exercise them
        for pool in pools.values() {
            let pool_loans = pool.active_loans.read().await;
            total_active_loans += pool_loans.len();
            total_pool_deposits += pool.total_deposits;
            
            // Read pool_utilization and risk_score fields
            let _utilization = pool.pool_utilization;
            let _risk = pool.risk_score;
            
            // Read interest_distribution_queue field
            let queue = pool.interest_distribution_queue.read().await;
            let _queue_len = queue.len();
        }
        
        EconomicStats {
            network_stats: network_stats.clone(),
            total_pool_deposits,
            total_active_loans,
            current_lending_rate: self.interest_rate_engine.calculate_lending_rate(&network_stats).await,
            pool_count: pools.len(),
        }
    }

    /// Record successful transaction settlement
    pub async fn record_transaction_settled(&self, tx_id: Uuid) -> Result<()> {
        let mut stats = self.network_stats.write().await;
        stats.total_transactions += 1;
        tracing::debug!("Recorded successful settlement for transaction: {}", tx_id);
        Ok(())
    }

    /// Record transaction settlement with details for economic analysis
    pub async fn record_transaction_settled_with_details(
        &self, 
        amount: u64, 
        token_type: String, 
        from_address: String, 
        to_address: String
    ) -> Result<()> {
        let mut stats = self.network_stats.write().await;
        stats.total_transactions += 1;
        
        // Update average transaction value
        let current_total = stats.average_transaction_value * (stats.total_transactions - 1);
        stats.average_transaction_value = (current_total + amount) / stats.total_transactions;
        
                // Record token type distribution for economic analysis
        let mut token_stats = self.token_distribution.write().await;
        *token_stats.entry(token_type.clone()).or_insert(0) += 1;

        // Convert wei to RON for human-readable logging
        let amount_ron = if token_type == "RON" {
            crate::web3::utils::wei_to_ron(amount)
        } else {
            amount as f64 // For non-RON tokens, keep as-is
        };
        tracing::debug!("Recorded transaction settlement: {:.6} {} from {} to {} ({} wei)",
            amount_ron, token_type, from_address, to_address, amount);
        Ok(())
    }

    /// Record failed transaction settlement
    pub async fn record_transaction_failed(&self, tx_id: Uuid, error: &str) -> Result<()> {
        tracing::warn!("Recorded failed settlement for transaction {}: {}", tx_id, error);
        // Could implement retry logic or failure analysis here
        Ok(())
    }

    /// Record batch settlement completion
    pub async fn record_batch_settlement(&self, count: usize) -> Result<()> {
        let mut stats = self.network_stats.write().await;
        stats.total_transactions += count as u64;
        tracing::info!("Recorded batch settlement of {} transactions", count);
        Ok(())
    }

    /// Record incentive earned from store & forward
    pub async fn record_incentive_earned(&self, amount: u64) -> Result<()> {
        let amount_ron = crate::web3::utils::wei_to_ron(amount);
        tracing::info!("Recorded incentive earned: {:.6} RON ({} wei)", amount_ron, amount);
        // Could implement incentive distribution logic here
        Ok(())
    }

    /// Set lending pools manager for automated distribution
    pub async fn set_lending_pools(&self, lending_pools: Arc<lending_pools::LendingPoolManager>) -> Result<()> {
        // Store reference to lending pools for economic operations
        // Store the lending_pools reference for future use in economic calculations
        let mut engine = self.lending_pools_manager.write().await;
        *engine = Some(lending_pools.clone());
        tracing::info!("Lending pools connected to economic engine (manager: {:?})", 
            std::any::type_name_of_val(&*lending_pools));
        Ok(())
    }

    /// Get lending pools manager for economic calculations
    pub async fn get_lending_pools_manager(&self) -> Option<Arc<lending_pools::LendingPoolManager>> {
        let manager = self.lending_pools_manager.read().await;
        manager.clone()
    }

    /// Record lending pool events for economic analysis
    pub async fn record_pool_created(&self, pool_id: String) -> Result<()> {
        tracing::info!("Economic engine recorded pool creation: {}", pool_id);
        Ok(())
    }

    pub async fn record_loan_created(&self, loan_id: String, borrower: String) -> Result<()> {
        let mut stats = self.network_stats.write().await;
        stats.total_lending_volume += 1;
        tracing::info!("Economic engine recorded loan creation: {} for {}", loan_id, borrower);
        Ok(())
    }

    pub async fn record_loan_repaid(&self, loan_id: String, borrower: String) -> Result<()> {
        tracing::info!("Economic engine recorded loan repayment: {} by {}", loan_id, borrower);
        Ok(())
    }

    pub async fn record_loan_defaulted(&self, loan_id: String, borrower: String) -> Result<()> {
        tracing::warn!("Economic engine recorded loan default: {} by {}", loan_id, borrower);
        Ok(())
    }

    pub async fn record_interest_paid(&self, loan_id: String, amount: u64) -> Result<()> {
        tracing::info!("Economic engine recorded interest payment: {} RON for loan {}", amount, loan_id);
        Ok(())
    }

    pub async fn record_pool_liquidated(&self, pool_id: String) -> Result<()> {
        tracing::warn!("Economic engine recorded pool liquidation: {}", pool_id);
        Ok(())
    }

    /// Record distributed computing events for economic analysis
    pub async fn record_distributed_computing_task(&self, task_id: Uuid, node_count: usize) -> Result<()> {
        tracing::info!("Economic engine recorded distributed computing task: {} with {} nodes", task_id, node_count);
        Ok(())
    }

    pub async fn record_distributed_computing_completed(&self, task_id: Uuid, node_count: usize) -> Result<()> {
        tracing::info!("Economic engine recorded distributed computing completion: {} with {} nodes", task_id, node_count);
        Ok(())
    }

    pub async fn record_distributed_computing_failed(&self, task_id: Uuid, error: String) -> Result<()> {
        tracing::warn!("Economic engine recorded distributed computing failure: {} - {}", task_id, error);
        Ok(())
    }

    /// Record lending pool activity for economic analysis
    pub async fn record_lending_pool_activity(&self, transaction_id: String, transaction_amount: u64, loan_amount: u64, interest_rate: f64) -> Result<()> {
        let mut stats = self.network_stats.write().await;
        stats.total_lending_volume += loan_amount;
        stats.total_borrowing_volume += loan_amount;
        
        // Update average transaction value if this transaction qualifies for lending
        if loan_amount > 0 {
            let total_tx = stats.total_transactions;
            let current_avg = stats.average_transaction_value;
            stats.average_transaction_value = if total_tx > 0 {
                ((current_avg * total_tx) + transaction_amount) / (total_tx + 1)
            } else {
                transaction_amount
            };
        }
        
        let transaction_amount_ron = crate::web3::utils::wei_to_ron(transaction_amount);
        let loan_amount_ron = crate::web3::utils::wei_to_ron(loan_amount);
        tracing::info!("Economic engine recorded lending pool activity: transaction {} ({:.6} RON, {} wei) generated {:.6} RON ({} wei) loan at {:.2}% interest",
            transaction_id, transaction_amount_ron, transaction_amount, loan_amount_ron, loan_amount, interest_rate * 100.0);
        Ok(())
    }

    /// Add deposit to a lending pool (uses LendingPool::add_deposit)
    pub async fn add_deposit_to_pool(&self, pool_name: &str, amount: u64) -> Result<()> {
        let mut pools = self.lending_pools.write().await;
        if let Some(pool) = pools.get_mut(pool_name) {
            pool.add_deposit(amount).await?;
            tracing::info!("Added deposit of {:.6} RON ({} wei) to pool: {}", 
                crate::web3::utils::wei_to_ron(amount), amount, pool_name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Pool not found: {}", pool_name))
        }
    }

    /// Create a loan in a lending pool (uses LendingPool::create_loan)
    pub async fn create_loan_in_pool(
        &self,
        pool_name: &str,
        borrower: String,
        lender: String,
        amount: u64,
        collateral: u64,
        duration_days: u32,
    ) -> Result<String> {
        let network_stats = self.network_stats.read().await.clone();
        let interest_rate = self.interest_rate_engine.calculate_borrowing_rate(
            collateral as f64 / amount as f64,
            &network_stats
        ).await;

        // Clone borrower before moving it into create_loan
        let borrower_clone = borrower.clone();

        let mut pools = self.lending_pools.write().await;
        if let Some(pool) = pools.get_mut(pool_name) {
            let loan_id = pool.create_loan(borrower, lender, amount, collateral, duration_days, interest_rate).await?;
            
            // Record loan creation using cloned borrower
            drop(pools);
            self.record_loan_created(loan_id.clone(), borrower_clone).await?;
            
            Ok(loan_id)
        } else {
            Err(anyhow::anyhow!("Pool not found: {}", pool_name))
        }
    }

    /// Process loan repayment (uses LendingPool::process_repayment)
    pub async fn process_loan_repayment(&self, pool_name: &str, loan_id: &str, repayment_amount: u64) -> Result<bool> {
        let mut pools = self.lending_pools.write().await;
        if let Some(pool) = pools.get_mut(pool_name) {
            let success = pool.process_repayment(loan_id, repayment_amount).await?;
            if success {
                // Record successful repayment
                let loans = pool.active_loans.read().await;
                if let Some(loan) = loans.get(loan_id) {
                    let borrower = loan.borrower_address.clone();
                    drop(loans);
                    self.record_loan_repaid(loan_id.to_string(), borrower).await?;
                }
            }
            Ok(success)
        } else {
            Err(anyhow::anyhow!("Pool not found: {}", pool_name))
        }
    }

    /// Get pool statistics (uses LendingPool::get_pool_stats)
    pub async fn get_pool_statistics(&self, pool_name: &str) -> Result<PoolStats> {
        let pools = self.lending_pools.read().await;
        if let Some(pool) = pools.get(pool_name) {
            Ok(pool.get_pool_stats().await)
        } else {
            Err(anyhow::anyhow!("Pool not found: {}", pool_name))
        }
    }

    /// Update supply-demand multiplier for interest rate calculations
    pub async fn update_supply_demand_multiplier(&self, multiplier: f64) -> Result<()> {
        let mut mult = self.interest_rate_engine.supply_demand_multiplier.write().await;
        *mult = multiplier.max(0.1).min(5.0); // Clamp between 0.1 and 5.0
        tracing::info!("Updated supply-demand multiplier to: {:.3}", *mult);
        Ok(())
    }

    /// Update network utilization factor for interest rate calculations
    pub async fn update_network_utilization_factor(&self, factor: f64) -> Result<()> {
        let mut factor_lock = self.interest_rate_engine.network_utilization_factor.write().await;
        *factor_lock = factor.max(0.1).min(5.0); // Clamp between 0.1 and 5.0
        tracing::info!("Updated network utilization factor to: {:.3}", *factor_lock);
        Ok(())
    }

    /// Process interest distribution queue (uses interest_distribution_queue field)
    pub async fn process_interest_distributions(&self, pool_name: &str) -> Result<usize> {
        let pools = self.lending_pools.read().await;
        if let Some(pool) = pools.get(pool_name) {
            let queue = pool.interest_distribution_queue.read().await;
            let queue_len = queue.len();
            
            // Process pending interest payments
            let pending_count = queue.iter().filter(|payment| !payment.is_paid).count();
            
            tracing::info!("Pool {} has {} interest payments in queue ({} pending)", 
                pool_name, queue_len, pending_count);
            
            Ok(pending_count)
        } else {
            Err(anyhow::anyhow!("Pool not found: {}", pool_name))
        }
    }
}

/// Economic engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicStats {
    pub network_stats: NetworkStats,
    pub total_pool_deposits: u64,
    pub total_active_loans: usize,
    pub current_lending_rate: f64,
    pub pool_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_interest_rate_calculation() {
        let engine = InterestRateEngine::new();
        let stats = NetworkStats {
            total_transactions: 1000,
            active_users: 100,
            network_utilization: 0.8,
            average_transaction_value: 1000,
            mesh_congestion_level: 0.2,
            total_lending_volume: 500_000,
            total_borrowing_volume: 300_000,
            average_collateral_ratio: 1.5,
        };

        let lending_rate = engine.calculate_lending_rate(&stats).await;
        assert!(lending_rate > 0.05); // Should be higher than base rate
        assert!(lending_rate < 0.10); // Should be reasonable
    }

    #[tokio::test]
    async fn test_lending_pool_creation() {
        let mut pool = LendingPool::new();
        let loan_id = pool.create_loan(
            "borrower1".to_string(),
            "lender1".to_string(),
            1000,
            1500,
            30,
            0.06,
        ).await.unwrap();

        assert!(!loan_id.is_empty());
        
        let stats = pool.get_pool_stats().await;
        assert_eq!(stats.active_loans, 1);
    }
}
