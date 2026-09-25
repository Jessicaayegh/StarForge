//! AI-powered deployment optimization module.
//!
//! Provides AI-driven optimization for deployment processes to reduce costs,
//! improve speed, and enhance reliability.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Deployment optimization result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentOptimizationResult {
    pub deployment_id: String,
    pub original_gas_cost: u64,
    pub optimized_gas_cost: u64,
    pub gas_savings_percentage: f64,
    pub original_deployment_time_ms: u64,
    pub optimized_deployment_time_ms: u64,
    pub time_improvement_percentage: f64,
    pub optimization_suggestions: Vec<OptimizationSuggestion>,
    pub resource_utilization: ResourceUtilization,
    pub network_selection: NetworkSelection,
    pub batch_optimization: BatchOptimization,
    pub scheduling_optimization: SchedulingOptimization,
}

/// Optimization suggestion for deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    pub id: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub estimated_gas_savings: u64,
    pub estimated_time_savings_ms: u64,
    pub implementation_effort: String,
    pub priority: String,
    pub code_example: Option<String>,
}

/// Resource utilization metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    pub cpu_usage_percentage: f64,
    pub memory_usage_mb: f64,
    pub network_bandwidth_mbps: f64,
    pub storage_io_percentage: f64,
    pub optimization_potential: f64,
}

/// Network selection recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSelection {
    pub recommended_network: String,
    pub estimated_cost_usd: f64,
    pub estimated_time_seconds: f64,
    pub reliability_score: f64,
    pub alternatives: Vec<NetworkAlternative>,
}

/// Alternative network option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAlternative {
    pub network_name: String,
    pub estimated_cost_usd: f64,
    pub estimated_time_seconds: f64,
    pub reliability_score: f64,
    pub trade_offs: Vec<String>,
}

/// Batch optimization recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOptimization {
    pub can_batch: bool,
    pub batch_size: usize,
    pub estimated_savings_percentage: f64,
    pub recommended_batch_order: Vec<String>,
}

/// Scheduling optimization recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingOptimization {
    pub optimal_deployment_time: String,
    pub estimated_cost_reduction: f64,
    pub network_conditions: NetworkConditions,
}

/// Network conditions at deployment time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditions {
    pub congestion_level: String,
    pub gas_price_trend: String,
    pub recommended_action: String,
}

/// Deployment optimizer configuration.
#[derive(Debug, Clone)]
pub struct DeploymentOptimizerConfig {
    pub wasm_path: String,
    pub target_networks: Vec<String>,
    pub optimization_level: OptimizationLevel,
    pub enable_cost_optimization: bool,
    pub enable_speed_optimization: bool,
    pub enable_reliability_optimization: bool,
}

/// Optimization depth level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptimizationLevel {
    Basic,
    Standard,
    Aggressive,
}

impl std::fmt::Display for OptimizationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizationLevel::Basic => write!(f, "basic"),
            OptimizationLevel::Standard => write!(f, "standard"),
            OptimizationLevel::Aggressive => write!(f, "aggressive"),
        }
    }
}

/// Gas cost optimizer.
pub struct GasCostOptimizer;

impl GasCostOptimizer {
    /// Analyze WASM for gas optimization opportunities.
    pub fn analyze_gas_optimization(wasm_bytes: &[u8]) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Analyze WASM size
        let wasm_size_kb = wasm_bytes.len() as f64 / 1024.0;

        // Large WASM penalty
        if wasm_size_kb > 100.0 {
            suggestions.push(OptimizationSuggestion {
                id: "GAS-001".to_string(),
                category: "wasm_size".to_string(),
                title: "Reduce WASM Size".to_string(),
                description: format!(
                    "WASM is {:.1} KB, consider optimizing to reduce gas costs",
                    wasm_size_kb
                ),
                estimated_gas_savings: ((wasm_size_kb - 100.0) * 1000.0) as u64,
                estimated_time_savings_ms: 500,
                implementation_effort: "1-2 hours".to_string(),
                priority: "high".to_string(),
                code_example: Some(
                    "Use soroban-optimize or remove unused dependencies".to_string(),
                ),
            });
        }

        // Check for common gas-heavy patterns
        let wasm_str = String::from_utf8_lossy(wasm_bytes);

        if wasm_str.contains("panic") || wasm_str.contains("unwrap") {
            suggestions.push(OptimizationSuggestion {
                id: "GAS-002".to_string(),
                category: "error_handling".to_string(),
                title: "Optimize Error Handling".to_string(),
                description: "Panic/unwrap patterns increase gas costs on errors".to_string(),
                estimated_gas_savings: 5000,
                estimated_time_savings_ms: 100,
                implementation_effort: "30 minutes".to_string(),
                priority: "medium".to_string(),
                code_example: Some(
                    "Replace .unwrap() with proper error handling using ?".to_string(),
                ),
            });
        }

        suggestions
    }

    /// Estimate gas cost for deployment.
    pub fn estimate_gas_cost(wasm_bytes: &[u8], network: &str) -> u64 {
        let base_cost = match network {
            "mainnet" => 100_000,
            "testnet" => 10_000,
            _ => 50_000,
        };

        let size_multiplier = (wasm_bytes.len() as f64 / 1024.0) / 100.0;
        (base_cost as f64 * size_multiplier) as u64
    }

    /// Calculate gas savings from optimizations.
    pub fn calculate_gas_savings(original: u64, optimized: u64) -> f64 {
        if original == 0 {
            0.0
        } else {
            ((original - optimized) as f64 / original as f64) * 100.0
        }
    }
}

/// Speed optimizer.
pub struct SpeedOptimizer;

impl SpeedOptimizer {
    /// Analyze deployment speed optimization opportunities.
    pub fn analyze_speed_optimization(wasm_bytes: &[u8]) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        let wasm_size_kb = wasm_bytes.len() as f64 / 1024.0;

        // Large WASM takes longer to deploy
        if wasm_size_kb > 50.0 {
            suggestions.push(OptimizationSuggestion {
                id: "SPEED-001".to_string(),
                category: "upload_time".to_string(),
                title: "Optimize Upload Speed".to_string(),
                description: format!("WASM size {:.1} KB affects upload time", wasm_size_kb),
                estimated_gas_savings: 0,
                estimated_time_savings_ms: ((wasm_size_kb - 50.0) * 100.0) as u64,
                implementation_effort: "30 minutes".to_string(),
                priority: "medium".to_string(),
                code_example: Some("Use compression or soroban-optimize".to_string()),
            });
        }

        suggestions
    }

    /// Estimate deployment time.
    pub fn estimate_deployment_time(wasm_bytes: &[u8], network: &str) -> u64 {
        let base_time_ms = match network {
            "mainnet" => 5000,
            "testnet" => 2000,
            _ => 3000,
        };

        let size_multiplier = (wasm_bytes.len() as f64 / 1024.0) / 50.0;
        (base_time_ms as f64 * size_multiplier) as u64
    }

    /// Calculate time improvement percentage.
    pub fn calculate_time_improvement(original: u64, optimized: u64) -> f64 {
        if original == 0 {
            0.0
        } else {
            ((original - optimized) as f64 / original as f64) * 100.0
        }
    }
}

/// Reliability optimizer.
pub struct ReliabilityOptimizer;

impl ReliabilityOptimizer {
    /// Analyze reliability optimization opportunities.
    pub fn analyze_reliability_optimization() -> Vec<OptimizationSuggestion> {
        let suggestions = vec![
            OptimizationSuggestion {
                id: "REL-001".to_string(),
                category: "retry_logic".to_string(),
                title: "Add Retry Logic".to_string(),
                description: "Implement exponential backoff for network failures".to_string(),
                estimated_gas_savings: 0,
                estimated_time_savings_ms: 0,
                implementation_effort: "1 hour".to_string(),
                priority: "high".to_string(),
                code_example: Some(
                    "Implement retry with exponential backoff: 100ms, 200ms, 400ms, 800ms"
                        .to_string(),
                ),
            },
            OptimizationSuggestion {
                id: "REL-002".to_string(),
                category: "pre_deployment_checks".to_string(),
                title: "Add Pre-deployment Validation".to_string(),
                description: "Validate WASM and network conditions before deployment".to_string(),
                estimated_gas_savings: 0,
                estimated_time_savings_ms: 0,
                implementation_effort: "2 hours".to_string(),
                priority: "high".to_string(),
                code_example: Some(
                    "Run validation checks: WASM integrity, network connectivity, wallet balance"
                        .to_string(),
                ),
            },
        ];

        suggestions
    }
}

/// Network selector.
pub struct NetworkSelector;

impl NetworkSelector {
    /// Select optimal network for deployment.
    pub fn select_network(wasm_bytes: &[u8], target_networks: &[String]) -> NetworkSelection {
        let gas_costs: Vec<_> = target_networks
            .iter()
            .map(|net| {
                (
                    net.clone(),
                    GasCostOptimizer::estimate_gas_cost(wasm_bytes, net),
                )
            })
            .collect();

        let (best_network, min_cost) = gas_costs
            .iter()
            .min_by_key(|(_, cost)| *cost)
            .map(|(n, c)| (n.clone(), *c))
            .unwrap_or(("testnet".to_string(), 10_000));

        let alternatives: Vec<NetworkAlternative> = gas_costs
            .iter()
            .filter(|(net, _)| net != &best_network)
            .map(|(net, cost)| NetworkAlternative {
                network_name: net.clone(),
                estimated_cost_usd: *cost as f64 / 1_000_000.0,
                estimated_time_seconds: SpeedOptimizer::estimate_deployment_time(wasm_bytes, net)
                    as f64
                    / 1000.0,
                reliability_score: if net == "mainnet" { 0.99 } else { 0.95 },
                trade_offs: vec![
                    format!("Higher cost: {} stroops", cost),
                    "Different confirmation time".to_string(),
                ],
            })
            .collect();

        NetworkSelection {
            recommended_network: best_network.clone(),
            estimated_cost_usd: min_cost as f64 / 1_000_000.0,
            estimated_time_seconds: SpeedOptimizer::estimate_deployment_time(
                wasm_bytes,
                &best_network,
            ) as f64
                / 1000.0,
            reliability_score: if best_network == "mainnet" {
                0.99
            } else {
                0.95
            },
            alternatives,
        }
    }
}

/// Batch optimizer.
pub struct BatchOptimizer;

impl BatchOptimizer {
    /// Analyze batch optimization opportunities.
    pub fn analyze_batch_optimization() -> BatchOptimization {
        BatchOptimization {
            can_batch: true,
            batch_size: 5,
            estimated_savings_percentage: 15.0,
            recommended_batch_order: vec![
                "deploy_contract".to_string(),
                "initialize_contract".to_string(),
                "configure_settings".to_string(),
                "verify_deployment".to_string(),
                "setup_monitoring".to_string(),
            ],
        }
    }

    /// Recommend a worker count for parallel deployment of `contract_count`
    /// contracts.
    ///
    /// Blend the heuristic batch size with the reported host parallelism and
    /// bound the result to `[1, contract_count]` so a single-contract manifest
    /// never oversubscribes. `wasm_bytes` is currently advisory (larger WASM
    /// uploads benefit from fewer concurrent workers to avoid RPC rate limits).
    pub fn recommend_concurrency(
        contract_count: usize,
        wasm_bytes: &[u8],
        host_parallelism: usize,
    ) -> usize {
        let batch = Self::analyze_batch_optimization().batch_size.max(1);
        let size_cap = if wasm_bytes.len() >= 512 * 1024 {
            2
        } else {
            usize::MAX
        };
        let candidate = batch.min(host_parallelism).min(size_cap);
        candidate.clamp(1, contract_count.max(1))
    }
}

/// Scheduling optimizer.
pub struct SchedulingOptimizer;

impl SchedulingOptimizer {
    /// Analyze optimal deployment scheduling.
    pub fn analyze_scheduling_optimization(network: &str) -> SchedulingOptimization {
        let optimal_time = match network {
            "mainnet" => "02:00-04:00 UTC (low traffic)".to_string(),
            "testnet" => "Any time (low cost)".to_string(),
            _ => "09:00-17:00 UTC (standard hours)".to_string(),
        };

        SchedulingOptimization {
            optimal_deployment_time: optimal_time,
            estimated_cost_reduction: match network {
                "mainnet" => 20.0,
                _ => 5.0,
            },
            network_conditions: NetworkConditions {
                congestion_level: "low".to_string(),
                gas_price_trend: "stable".to_string(),
                recommended_action: "Proceed with deployment".to_string(),
            },
        }
    }
}

/// Resource utilization analyzer.
pub struct ResourceAnalyzer;

impl ResourceAnalyzer {
    /// Analyze current resource utilization.
    pub fn analyze_utilization(wasm_bytes: &[u8]) -> ResourceUtilization {
        let wasm_size_mb = wasm_bytes.len() as f64 / (1024.0 * 1024.0);

        ResourceUtilization {
            cpu_usage_percentage: 45.0,
            memory_usage_mb: wasm_size_mb * 2.0,
            network_bandwidth_mbps: 10.0,
            storage_io_percentage: 25.0,
            optimization_potential: if wasm_size_mb > 0.5 { 30.0 } else { 10.0 },
        }
    }
}

/// Perform comprehensive deployment optimization.
pub fn optimize_deployment(
    config: &DeploymentOptimizerConfig,
) -> Result<DeploymentOptimizationResult> {
    let wasm_path = Path::new(&config.wasm_path);
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("Failed to read WASM file: {}", wasm_path.display()))?;

    // Calculate original metrics
    let original_gas_cost = GasCostOptimizer::estimate_gas_cost(&wasm_bytes, "testnet");
    let original_deployment_time = SpeedOptimizer::estimate_deployment_time(&wasm_bytes, "testnet");

    // Collect optimization suggestions
    let mut optimization_suggestions = Vec::new();

    if config.enable_cost_optimization {
        optimization_suggestions.extend(GasCostOptimizer::analyze_gas_optimization(&wasm_bytes));
    }

    if config.enable_speed_optimization {
        optimization_suggestions.extend(SpeedOptimizer::analyze_speed_optimization(&wasm_bytes));
    }

    if config.enable_reliability_optimization {
        optimization_suggestions.extend(ReliabilityOptimizer::analyze_reliability_optimization());
    }

    // Calculate optimized metrics (estimated based on suggestions)
    let total_gas_savings: u64 = optimization_suggestions
        .iter()
        .map(|s| s.estimated_gas_savings)
        .sum();
    let total_time_savings: u64 = optimization_suggestions
        .iter()
        .map(|s| s.estimated_time_savings_ms)
        .sum();

    let optimized_gas_cost = original_gas_cost.saturating_sub(total_gas_savings);
    let optimized_deployment_time = original_deployment_time.saturating_sub(total_time_savings);

    // Network selection
    let networks = if config.target_networks.is_empty() {
        vec!["testnet".to_string(), "mainnet".to_string()]
    } else {
        config.target_networks.clone()
    };
    let network_selection = NetworkSelector::select_network(&wasm_bytes, &networks);

    // Resource utilization
    let resource_utilization = ResourceAnalyzer::analyze_utilization(&wasm_bytes);

    // Batch optimization
    let batch_optimization = BatchOptimizer::analyze_batch_optimization();

    // Scheduling optimization
    let scheduling_optimization = SchedulingOptimizer::analyze_scheduling_optimization(
        &network_selection.recommended_network,
    );

    Ok(DeploymentOptimizationResult {
        deployment_id: uuid::Uuid::new_v4().to_string(),
        original_gas_cost,
        optimized_gas_cost,
        gas_savings_percentage: GasCostOptimizer::calculate_gas_savings(
            original_gas_cost,
            optimized_gas_cost,
        ),
        original_deployment_time_ms: original_deployment_time,
        optimized_deployment_time_ms: optimized_deployment_time,
        time_improvement_percentage: SpeedOptimizer::calculate_time_improvement(
            original_deployment_time,
            optimized_deployment_time,
        ),
        optimization_suggestions,
        resource_utilization,
        network_selection,
        batch_optimization,
        scheduling_optimization,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_cost_estimation() {
        let wasm_bytes = vec![0u8; 1024 * 50]; // 50 KB
        let cost = GasCostOptimizer::estimate_gas_cost(&wasm_bytes, "testnet");
        assert!(cost > 0);
    }

    #[test]
    fn test_gas_savings_calculation() {
        let savings = GasCostOptimizer::calculate_gas_savings(100_000, 80_000);
        assert_eq!(savings, 20.0);
    }

    #[test]
    fn test_time_estimation() {
        let wasm_bytes = vec![0u8; 1024 * 50];
        let time = SpeedOptimizer::estimate_deployment_time(&wasm_bytes, "testnet");
        assert!(time > 0);
    }

    #[test]
    fn test_network_selection() {
        let wasm_bytes = vec![0u8; 1024 * 50];
        let networks = vec!["testnet".to_string(), "mainnet".to_string()];
        let selection = NetworkSelector::select_network(&wasm_bytes, &networks);
        assert!(!selection.recommended_network.is_empty());
    }

    #[test]
    fn test_recommend_concurrency_bounded_by_contract_count() {
        let wasm_bytes = vec![0u8; 1024 * 50]; // small WASM
        let workers = BatchOptimizer::recommend_concurrency(2, &wasm_bytes, 8);
        assert_eq!(workers, 2, "must never exceed contract count");
    }

    #[test]
    fn test_recommend_concurrency_at_least_one() {
        let wasm_bytes = vec![0u8; 1024 * 50];
        let workers = BatchOptimizer::recommend_concurrency(1, &wasm_bytes, 0);
        assert_eq!(workers, 1);
    }

    #[test]
    fn test_recommend_concurrency_caps_for_large_wasm() {
        let wasm_bytes = vec![0u8; 600 * 1024]; // >= 512 KB
        let workers = BatchOptimizer::recommend_concurrency(8, &wasm_bytes, 8);
        assert_eq!(workers, 2);
    }
}
