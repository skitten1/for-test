//! Diagnostic rules module
//!
//! Implements the rule engine for Query Profile diagnostics.
//! Rules are organized by operator type following the design document.

pub mod aggregate;
pub mod common;
pub mod exchange;
pub mod fragment;
pub mod join;
pub mod project;
pub mod query;
pub mod scan;
pub mod sink;
pub mod sort;

use crate::services::profile_analyzer::models::*;

// ============================================================================
// Rule Trait and Types
// ============================================================================

/// Severity level for diagnostic rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleSeverity {
    Info = 0,
    Warning = 1,
    Error = 2,
}

impl From<RuleSeverity> for HotSeverity {
    fn from(severity: RuleSeverity) -> Self {
        match severity {
            RuleSeverity::Info => HotSeverity::Mild,
            RuleSeverity::Warning => HotSeverity::Moderate,
            RuleSeverity::Error => HotSeverity::Severe,
        }
    }
}

/// Parameter suggestion for tuning
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParameterSuggestion {
    /// Parameter name (e.g., "enable_scan_datacache")
    pub name: String,
    /// Parameter type (Session or BE)
    pub param_type: ParameterType,
    /// Current value if set, None if using default
    pub current: Option<String>,
    /// Recommended value
    pub recommended: String,
    /// SQL command to set the parameter
    pub command: String,
    /// Human-readable description of what this parameter does
    pub description: String,
    /// Expected impact of changing this parameter
    pub impact: String,
}

/// Parameter type classification
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum ParameterType {
    Session,
    BE,
}

/// Get parameter metadata (description and impact) for common StarRocks parameters
pub fn get_parameter_metadata(name: &str) -> ParameterMetadata {
    match name {
        // ========== DataCache 相关 ==========
        "enable_scan_datacache" => ParameterMetadata {
            description: "Enable DataCache read caching to read data from the local cache instead of remote storage.".to_string(),
            impact: "Improves query performance in shared-data architecture and reduces network I/O.".to_string(),
        },
        "enable_populate_datacache" => ParameterMetadata {
            description: "Enable DataCache write-through to cache remotely read data locally.".to_string(),
            impact: "Subsequent queries can use the local cache, but the first query incurs additional write overhead.".to_string(),
        },
        "datacache_evict_probability" => ParameterMetadata {
            description: "DataCache eviction probability (0–100), controlling the likelihood that cached data will be evicted.".to_string(),
            impact: "Reducing this value can reduce cache churn but may cause insufficient cache space.".to_string(),
        },

        // ========== 查询优化相关 ==========
        "enable_query_cache" => ParameterMetadata {
            description: "Enable query result caching so identical queries can return cached results directly.".to_string(),
            impact: "Significantly accelerates repeated queries but uses additional memory.".to_string(),
        },
        "enable_adaptive_sink_dop" => ParameterMetadata {
            description: "Enable adaptive Sink parallelism to adjust write parallelism dynamically based on data volume.".to_string(),
            impact: "Can improve data write performance and reduce the number of small files.".to_string(),
        },
        "enable_runtime_adaptive_dop" => ParameterMetadata {
            description: "Enable runtime adaptive parallelism to adjust execution parallelism dynamically based on the actual data volume.".to_string(),
            impact: "Can improve resource utilization and prevent small queries from consuming excessive resources.".to_string(),
        },
        "enable_spill" => ParameterMetadata {
            description: "Enable spilling intermediate results to disk when memory is insufficient.".to_string(),
            impact: "Can handle very large queries but may reduce query performance.".to_string(),
        },

        // ========== 扫描优化相关 ==========
        "enable_connector_adaptive_io_tasks" => ParameterMetadata {
            description: "Enable adaptive connector I/O task counts to adjust I/O parallelism dynamically based on data volume.".to_string(),
            impact: "Can improve external table scan performance and balance I/O and CPU resources.".to_string(),
        },
        "io_tasks_per_scan_operator" => ParameterMetadata {
            description: "Number of I/O tasks per scan operator, controlling local table scan parallelism.".to_string(),
            impact: "Increasing this value can improve scan throughput but also increases I/O pressure.".to_string(),
        },
        "connector_io_tasks_per_scan_operator" => ParameterMetadata {
            description: "Number of I/O tasks per connector scan operator, controlling external table scan parallelism.".to_string(),
            impact: "Increasing this value can improve external table scan throughput but also increases pressure on remote storage.".to_string(),
        },

        // ========== Join 优化相关 ==========
        "hash_join_push_down_right_table" => ParameterMetadata {
            description: "Enable Hash Join right-side pushdown to broadcast the small table to each node.".to_string(),
            impact: "Can reduce data Shuffle and improve Join performance.".to_string(),
        },
        "enable_local_shuffle_agg" => ParameterMetadata {
            description: "Enable local Shuffle aggregation to pre-aggregate data locally.".to_string(),
            impact: "Can reduce network transfer volume and improve aggregation performance.".to_string(),
        },

        // ========== Runtime Filter 相关 ==========
        "runtime_filter_on_exchange_node" => ParameterMetadata {
            description: "Enable Runtime Filters on Exchange nodes to pass filter conditions across nodes.".to_string(),
            impact: "Can filter data earlier and reduce Shuffle, but increases Filter construction overhead.".to_string(),
        },
        "global_runtime_filter_build_max_size" => ParameterMetadata {
            description: "Maximum build size (bytes) for a global Runtime Filter.".to_string(),
            impact: "Increasing this value supports larger Filters but consumes more memory.".to_string(),
        },

        // ========== 并行执行相关 ==========
        "parallel_fragment_exec_instance_num" => ParameterMetadata {
            description: "Number of parallel execution instances per Fragment.".to_string(),
            impact: "Increasing this value improves parallelism but consumes more resources.".to_string(),
        },
        "pipeline_dop" => ParameterMetadata {
            description: "Pipeline execution parallelism; 0 means automatic.".to_string(),
            impact: "Manual settings control resource usage; automatic mode adjusts based on the number of CPU cores.".to_string(),
        },

        // ========== 内存相关 ==========
        "query_mem_limit" => ParameterMetadata {
            description: "Memory limit (bytes) for a single query.".to_string(),
            impact: "Increasing this value allows larger data volumes but may affect other queries.".to_string(),
        },
        "query_timeout" => ParameterMetadata {
            description: "Query timeout (seconds).".to_string(),
            impact: "Increasing this value allows longer-running queries but may tie up resources for too long.".to_string(),
        },

        // ========== 聚合相关 ==========
        "streaming_preaggregation_mode" => ParameterMetadata {
            description: "Streaming pre-aggregation mode (auto/force_streaming/force_preaggregation).".to_string(),
            impact: "Auto mode selects the optimal strategy; force modes enforce the specified strategy.".to_string(),
        },
        "enable_sort_aggregate" => ParameterMetadata {
            description: "Enable sort aggregation, suitable for high-cardinality GROUP BY.".to_string(),
            impact: "Can reduce memory usage but requires additional sorting overhead.".to_string(),
        },

        // ========== Profile 相关 ==========
        "pipeline_profile_level" => ParameterMetadata {
            description: "Pipeline Profile detail level (0–2).".to_string(),
            impact: "Higher levels provide more detail but also increase collection overhead.".to_string(),
        },

        // ========== BE 参数 ==========
        "storage_page_cache_limit" => ParameterMetadata {
            description: "BE storage page cache size limit.".to_string(),
            impact: "Increasing this value can improve hot data read performance but uses more memory.".to_string(),
        },

        // ========== 默认 ==========
        _ => ParameterMetadata {
            description: format!("StarRocks parameter {}", name),
            impact: "Refer to the official StarRocks documentation for details.".to_string(),
        },
    }
}

/// Metadata for a parameter
#[derive(Debug, Clone)]
pub struct ParameterMetadata {
    pub description: String,
    pub impact: String,
}

impl ParameterSuggestion {
    /// Create a new parameter suggestion with automatic metadata lookup
    pub fn new(
        name: &str,
        param_type: ParameterType,
        current: Option<String>,
        recommended: &str,
        command: &str,
    ) -> Self {
        let metadata = get_parameter_metadata(name);
        Self {
            name: name.to_string(),
            param_type,
            current,
            recommended: recommended.to_string(),
            command: command.to_string(),
            description: metadata.description,
            impact: metadata.impact,
        }
    }
}

/// A diagnostic result from rule evaluation
///
/// Structure follows Aliyun EMR StarRocks diagnostic standard:
/// - message: 诊断结果概要说明 (Summary of the issue)
/// - reason: 详细诊断原因说明 (Detailed explanation of why this happens)
/// - suggestions: 建议措施 (Recommended actions)
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub rule_id: String,
    pub rule_name: String,
    pub severity: RuleSeverity,
    pub node_path: String,
    /// Plan node ID for associating diagnostic with execution tree node
    pub plan_node_id: Option<i32>,
    /// Summary of the diagnostic issue (诊断结果概要)
    pub message: String,
    /// Detailed explanation of why this issue occurs (详细诊断原因)
    pub reason: String,
    /// Recommended actions to fix the issue (建议措施)
    pub suggestions: Vec<String>,
    pub parameter_suggestions: Vec<ParameterSuggestion>,
}

impl Diagnostic {
    /// Convert to HotSpot for backward compatibility
    pub fn to_hotspot(&self) -> HotSpot {
        let mut all_suggestions = self.suggestions.clone();

        // Add parameter suggestions as formatted strings
        for param in &self.parameter_suggestions {
            all_suggestions.push(format!(
                "Adjust parameter: {} → {} (command: {}).",
                param.name, param.recommended, param.command
            ));
        }

        HotSpot {
            node_path: self.node_path.clone(),
            severity: self.severity.into(),
            issue_type: self.rule_id.clone(),
            description: self.message.clone(),
            suggestions: all_suggestions,
        }
    }
}

/// Context for rule evaluation
pub struct RuleContext<'a> {
    pub node: &'a ExecutionTreeNode,
    /// Non-default session variables from profile summary
    pub session_variables: &'a std::collections::HashMap<String, SessionVariableInfo>,
    /// Cluster information for smart recommendations
    pub cluster_info: Option<ClusterInfo>,
    /// Live cluster variables (actual current values from cluster)
    /// Takes precedence over session_variables for parameter recommendations
    pub cluster_variables: Option<&'a std::collections::HashMap<String, String>>,
    /// Default database from profile summary
    pub default_db: Option<&'a str>,
}

impl<'a> RuleContext<'a> {
    /// Get a metric value from unique_metrics as f64
    pub fn get_metric(&self, name: &str) -> Option<f64> {
        self.node
            .unique_metrics
            .get(name)
            .and_then(|v| parse_metric_value(v))
    }

    /// Get operator total time in ms
    pub fn get_operator_time_ms(&self) -> Option<f64> {
        self.node
            .metrics
            .operator_total_time
            .map(|ns| ns as f64 / 1_000_000.0)
    }

    /// Get time percentage
    pub fn get_time_percentage(&self) -> Option<f64> {
        self.node.time_percentage
    }

    /// Get memory usage in bytes
    pub fn get_memory_usage(&self) -> Option<u64> {
        self.node.metrics.memory_usage
    }

    /// Check if a session variable is already set to the expected value
    /// Returns true if the variable is set and matches the expected value
    pub fn is_variable_set_to(&self, name: &str, expected: &str) -> bool {
        self.session_variables
            .get(name)
            .map(|info| info.actual_value_is(expected))
            .unwrap_or(false)
    }

    /// Get current value of a session variable as string
    /// Priority: cluster_variables > session_variables (non-default) > None
    pub fn get_variable_value(&self, name: &str) -> Option<String> {
        // First check live cluster variables (most accurate)
        if let Some(cluster_vars) = self.cluster_variables
            && let Some(value) = cluster_vars.get(name)
        {
            return Some(value.clone());
        }
        // Fallback to profile's non-default variables
        self.session_variables
            .get(name)
            .map(|info| info.actual_value_str())
    }

    /// Create a parameter suggestion only if the parameter is not already set to the recommended value
    /// Returns None if the parameter is already set to the recommended value (no suggestion needed)
    ///
    /// Note: For parameters not in NonDefaultSessionVariables, we check against known defaults.
    /// If a parameter uses its default value and that default matches the recommendation, no suggestion is made.
    pub fn suggest_parameter(
        &self,
        name: &str,
        recommended: &str,
        command: &str,
    ) -> Option<ParameterSuggestion> {
        // Check if already set to recommended value in non-default variables
        if self.is_variable_set_to(name, recommended) {
            return None; // Already configured correctly, no suggestion needed
        }

        // If parameter is not in non_default_variables, check if default value matches recommendation
        if !self.session_variables.contains_key(name)
            && let Some(default) = get_parameter_default(name)
            && default.eq_ignore_ascii_case(recommended)
        {
            return None; // Using default value which matches recommendation
        }

        // Get current value if set
        let current = self.get_variable_value(name);

        // Get parameter metadata for description and impact
        let metadata = get_parameter_metadata(name);

        Some(ParameterSuggestion {
            name: name.to_string(),
            param_type: ParameterType::Session,
            current,
            recommended: recommended.to_string(),
            command: command.to_string(),
            description: metadata.description,
            impact: metadata.impact,
        })
    }

    /// Smart parameter suggestion that considers cluster info and current values
    /// Returns None if:
    /// - Current value already meets or exceeds recommendation
    /// - Query is too small to benefit from the change
    /// - Parameter is already set to recommended value
    pub fn suggest_parameter_smart(&self, name: &str) -> Option<ParameterSuggestion> {
        let cluster_info = self.cluster_info.as_ref();

        // Get current value
        let current_str = self.get_variable_value(name);
        let default_str = get_parameter_default(name).map(|s| s.to_string());
        let effective_value = current_str.as_ref().or(default_str.as_ref());

        let current_i64 = effective_value.and_then(|v| v.parse::<i64>().ok());
        let current_bool = effective_value.map(|v| v.eq_ignore_ascii_case("true"));

        // Calculate smart recommendation based on parameter
        let (recommended, reason, param_type) = match name {
            // ========== 并行度相关 ==========
            "parallel_fragment_exec_instance_num" => {
                let be_count = cluster_info.map(|c| c.backend_num).unwrap_or(1).max(1);
                let recommended = be_count.min(16) as i64;
                let current = current_i64.unwrap_or(1);

                if current >= recommended {
                    return None;
                }

                // Don't recommend for small queries (< 100MB)
                if cluster_info
                    .map(|c| c.total_scan_bytes < 100_000_000)
                    .unwrap_or(true)
                {
                    return None;
                }

                (
                    recommended.to_string(),
                    format!("Recommended for a cluster with {} BE nodes.", be_count),
                    ParameterType::Session,
                )
            },

            "pipeline_dop" => {
                let current = current_i64.unwrap_or(0);
                if current == 0 {
                    return None; // Already auto
                }
                (
                    "0".to_string(),
                    "Automatic mode is recommended; the system adjusts based on the number of CPU cores.".to_string(),
                    ParameterType::Session,
                )
            },

            "io_tasks_per_scan_operator" => {
                let current = current_i64.unwrap_or(4);
                let total_bytes = cluster_info.map(|c| c.total_scan_bytes).unwrap_or(0);
                let is_large_scan = total_bytes > 1_000_000_000; // > 1GB
                let recommended = if is_large_scan { 8 } else { 4 };

                if current >= recommended || !is_large_scan {
                    return None;
                }

                (
                    recommended.to_string(),
                    "For large scans, consider increasing I/O parallelism.".to_string(),
                    ParameterType::Session,
                )
            },

            // ========== 内存相关 ==========
            "query_mem_limit" => {
                let current = current_i64.unwrap_or(0);
                let total_bytes = cluster_info.map(|c| c.total_scan_bytes).unwrap_or(0);

                // Recommend based on scan size: 2x scan size, min 4GB, max 32GB
                let recommended = if total_bytes > 0 {
                    (total_bytes * 2).clamp(4 * 1024 * 1024 * 1024, 32 * 1024 * 1024 * 1024) as i64
                } else {
                    8 * 1024 * 1024 * 1024 // Default 8GB
                };

                if current >= recommended {
                    return None;
                }

                let recommended_gb = recommended / (1024 * 1024 * 1024);
                (
                    recommended.to_string(),
                    format!("Recommended memory limit based on data volume: {} GB.", recommended_gb),
                    ParameterType::Session,
                )
            },

            "enable_spill" => {
                let current = current_bool.unwrap_or(false);
                if current {
                    return None; // Already enabled
                }
                ("true".to_string(), "Enable this to prevent OOM on large queries.".to_string(), ParameterType::Session)
            },

            // ========== 查询优化 ==========
            "query_timeout" => {
                let current = current_i64.unwrap_or(300);
                // Only recommend if current is default (300s) and query might be long
                if current >= 600 {
                    return None;
                }
                (
                    "600".to_string(),
                    "Increase the timeout to support complex queries.".to_string(),
                    ParameterType::Session,
                )
            },

            "enable_query_cache" => {
                let current = current_bool.unwrap_or(false);
                if current {
                    return None;
                }
                (
                    "true".to_string(),
                    "Enable query caching to accelerate repeated queries.".to_string(),
                    ParameterType::Session,
                )
            },

            // ========== Runtime Filter ==========
            "enable_global_runtime_filter" => {
                let current = current_bool.unwrap_or(true);
                if current {
                    return None;
                }
                (
                    "true".to_string(),
                    "Enable global Runtime Filters to improve Join performance.".to_string(),
                    ParameterType::Session,
                )
            },

            "runtime_join_filter_push_down_limit" => {
                let current = current_i64.unwrap_or(1024000);
                if current >= 10_000_000 {
                    return None;
                }
                (
                    "10000000".to_string(),
                    "Increase the RF pushdown threshold to support a larger Build side.".to_string(),
                    ParameterType::Session,
                )
            },

            // ========== DataCache ==========
            "enable_scan_datacache" => {
                let current = current_bool.unwrap_or(true);
                if current {
                    return None;
                }
                (
                    "true".to_string(),
                    "Enable DataCache to improve shared-data architecture performance.".to_string(),
                    ParameterType::Session,
                )
            },

            "enable_populate_datacache" => {
                let current = current_bool.unwrap_or(true);
                if current {
                    return None;
                }
                ("true".to_string(), "Enable cache population to warm up the cache.".to_string(), ParameterType::Session)
            },

            // ========== Profile ==========
            "pipeline_profile_level" => {
                let current = current_i64.unwrap_or(1);
                if current <= 1 {
                    return None;
                }
                (
                    "1".to_string(),
                    "Lower the Profile level to reduce collection overhead.".to_string(),
                    ParameterType::Session,
                )
            },

            // ========== BE 参数 ==========
            "storage_page_cache_limit" => {
                // BE parameter, always suggest if IO is bottleneck
                ("30%".to_string(), "Increase the page cache to improve hot data read performance.".to_string(), ParameterType::BE)
            },

            _ => return None, // No smart recommendation for this parameter
        };

        // Get current value: cluster_variables > session_variables > default
        let current = self
            .get_variable_value(name)
            .or_else(|| get_parameter_default(name).map(|s| s.to_string()));

        let metadata = get_parameter_metadata(name);
        let command = match param_type {
            ParameterType::Session => format!("SET {} = {};", name, recommended),
            ParameterType::BE => format!("-- BE config: {} = {}", name, recommended),
        };

        Some(ParameterSuggestion {
            name: name.to_string(),
            param_type,
            current,
            recommended,
            command,
            description: metadata.description,
            impact: format!("{} ({})", metadata.impact, reason),
        })
    }
}

/// Known default values for common StarRocks session parameters
/// This helps avoid suggesting parameters that are already at their recommended default values
fn get_parameter_default(name: &str) -> Option<&'static str> {
    match name {
        // DataCache related
        "enable_scan_datacache" => Some("true"),
        "enable_populate_datacache" => Some("true"),
        "datacache_evict_probability" => Some("100"),

        // Query optimization
        "enable_query_cache" => Some("false"),
        "enable_adaptive_sink_dop" => Some("false"),
        "enable_runtime_adaptive_dop" => Some("false"),
        "enable_spill" => Some("false"),

        // Scan optimization
        "enable_connector_adaptive_io_tasks" => Some("true"),
        "io_tasks_per_scan_operator" => Some("4"),
        "connector_io_tasks_per_scan_operator" => Some("16"),

        // Join optimization
        "hash_join_push_down_right_table" => Some("true"),
        "enable_local_shuffle_agg" => Some("true"),

        // Runtime filter
        "runtime_filter_on_exchange_node" => Some("false"),
        "global_runtime_filter_build_max_size" => Some("67108864"),

        // Parallel execution
        "parallel_fragment_exec_instance_num" => Some("1"),
        "pipeline_dop" => Some("0"),

        _ => None,
    }
}

/// Trait for diagnostic rules
pub trait DiagnosticRule: Send + Sync {
    /// Rule ID (e.g., "S001", "J001")
    fn id(&self) -> &str;

    /// Rule name
    fn name(&self) -> &str;

    /// Check if rule applies to this node
    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool;

    /// Evaluate the rule and return diagnostic if triggered
    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic>;
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Parse metric value from string (handles various formats)
pub fn parse_metric_value(value: &str) -> Option<f64> {
    let s = value.trim();

    // Handle percentage
    if s.ends_with('%') {
        return s.trim_end_matches('%').parse().ok();
    }

    // Handle bytes (e.g., "1.5 GB", "100 MB")
    if let Some(bytes) = parse_bytes(s) {
        return Some(bytes as f64);
    }

    // Handle time (e.g., "1s500ms", "100ms")
    if let Some(ms) = parse_duration_ms(s) {
        return Some(ms);
    }

    // Handle plain numbers with optional suffix
    let numeric_part: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();

    numeric_part.parse().ok()
}

/// Parse bytes string to u64
pub fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split_whitespace().collect();

    if parts.len() != 2 {
        return None;
    }

    let value: f64 = parts[0].parse().ok()?;
    let unit = parts[1].to_uppercase();

    let multiplier = match unit.as_str() {
        "B" => 1u64,
        "KB" | "K" => 1024,
        "MB" | "M" => 1024 * 1024,
        "GB" | "G" => 1024 * 1024 * 1024,
        "TB" | "T" => 1024 * 1024 * 1024 * 1024,
        _ => return None,
    };

    Some((value * multiplier as f64) as u64)
}

/// Parse duration string to milliseconds
pub fn parse_duration_ms(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let mut total_ms = 0.0;
    let mut num_buf = String::new();
    let mut found_unit = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' {
            num_buf.push(c);
            i += 1;
        } else {
            let value: f64 = num_buf.parse().unwrap_or(0.0);
            num_buf.clear();

            if c == 'h' {
                total_ms += value * 3600.0 * 1000.0;
                found_unit = true;
                i += 1;
            } else if c == 'm' {
                if i + 1 < chars.len() && chars[i + 1] == 's' {
                    total_ms += value;
                    i += 2;
                } else {
                    total_ms += value * 60.0 * 1000.0;
                    i += 1;
                }
                found_unit = true;
            } else if c == 's' {
                total_ms += value * 1000.0;
                found_unit = true;
                i += 1;
            } else if c == 'u' && i + 1 < chars.len() && chars[i + 1] == 's' {
                total_ms += value / 1000.0;
                found_unit = true;
                i += 2;
            } else if c == 'n' && i + 1 < chars.len() && chars[i + 1] == 's' {
                total_ms += value / 1_000_000.0;
                found_unit = true;
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    if found_unit { Some(total_ms) } else { None }
}

/// Format bytes to human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

/// Format duration in ms to human-readable string
pub fn format_duration_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.2}μs", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.2}ms", ms)
    } else if ms < 60000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else if ms < 3600000.0 {
        format!("{:.1}m", ms / 60000.0)
    } else {
        format!("{:.1}h", ms / 3600000.0)
    }
}

// ============================================================================
// Rule Registry
// ============================================================================

/// Get all registered rules
pub fn get_all_rules() -> Vec<Box<dyn DiagnosticRule>> {
    let mut rules: Vec<Box<dyn DiagnosticRule>> = Vec::new();

    // Common rules (G001, G002, G003)
    rules.extend(common::get_rules());

    // Scan rules (S001-S011)
    rules.extend(scan::get_rules());

    // Join rules (J001-J010)
    rules.extend(join::get_rules());

    // Aggregate rules (A001-A005)
    rules.extend(aggregate::get_rules());

    // Sort rules (T001-T005, W001)
    rules.extend(sort::get_rules());

    // Exchange rules (E001-E003)
    rules.extend(exchange::get_rules());

    // Fragment rules (F001-F003)
    rules.extend(fragment::get_rules());

    // Project/LocalExchange rules (P001, L001)
    rules.extend(project::get_rules());

    // OlapTableSink rules (I001-I003)
    rules.extend(sink::get_rules());

    // Query rules (Q001-Q009) - evaluated separately at query level

    rules
}

/// Get query-level rules
pub fn get_query_rules() -> Vec<Box<dyn query::QueryRule>> {
    query::get_rules()
}
