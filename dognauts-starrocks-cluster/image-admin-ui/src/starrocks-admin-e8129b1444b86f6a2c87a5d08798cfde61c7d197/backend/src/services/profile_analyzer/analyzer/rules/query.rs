//! Query-level diagnostic rules (Q001-Q009)
//!
//! Rules that evaluate the entire query profile.

use super::{
    ParameterSuggestion, ParameterType, RuleSeverity, format_bytes, format_duration_ms,
    get_parameter_metadata, parse_duration_ms,
};
use crate::services::profile_analyzer::models::*;

/// Known default values for common StarRocks session parameters
fn get_parameter_default(name: &str) -> Option<&'static str> {
    match name {
        // DataCache related
        "enable_scan_datacache" => Some("true"),
        "enable_populate_datacache" => Some("true"),

        // Query optimization
        "enable_query_cache" => Some("false"),
        "enable_adaptive_sink_dop" => Some("false"),
        "enable_runtime_adaptive_dop" => Some("false"),
        "enable_spill" => Some("false"),

        // Parallelism
        "parallel_fragment_exec_instance_num" => Some("1"),
        "pipeline_dop" => Some("0"),
        "io_tasks_per_scan_operator" => Some("4"),

        _ => None,
    }
}

// Note: suggest_parameter_if_needed is replaced by QueryRuleContext::suggest_parameter

/// Query-level rule context containing all information needed for evaluation
pub struct QueryRuleContext<'a> {
    pub profile: &'a Profile,
    /// Live cluster variables (actual current values from cluster)
    pub cluster_variables: Option<&'a std::collections::HashMap<String, String>>,
}

impl<'a> QueryRuleContext<'a> {
    #[allow(dead_code)]
    pub fn new(profile: &'a Profile) -> Self {
        Self { profile, cluster_variables: None }
    }

    pub fn with_cluster_variables(
        profile: &'a Profile,
        cluster_variables: Option<&'a std::collections::HashMap<String, String>>,
    ) -> Self {
        Self { profile, cluster_variables }
    }

    /// Get current value of a parameter
    /// Priority: cluster_variables > non_default_variables > default
    pub fn get_variable_value(&self, name: &str) -> Option<String> {
        // First check live cluster variables (most accurate)
        if let Some(vars) = self.cluster_variables
            && let Some(value) = vars.get(name)
        {
            return Some(value.clone());
        }
        // Then check profile's non-default variables
        if let Some(info) = self.profile.summary.non_default_variables.get(name) {
            return Some(info.actual_value_str());
        }
        // Finally use known default
        get_parameter_default(name).map(|s| s.to_string())
    }

    /// Get current value as i64
    #[allow(dead_code)]
    pub fn get_variable_i64(&self, name: &str) -> Option<i64> {
        self.get_variable_value(name).and_then(|v| v.parse().ok())
    }

    /// Get current value as bool
    #[allow(dead_code)]
    pub fn get_variable_bool(&self, name: &str) -> Option<bool> {
        self.get_variable_value(name)
            .map(|v| v.eq_ignore_ascii_case("true"))
    }

    /// Create a smart parameter suggestion
    /// Returns None if current value already meets recommendation
    pub fn suggest_parameter(&self, name: &str) -> Option<ParameterSuggestion> {
        let cluster_info = self.profile.get_cluster_info();
        let current_str = self.get_variable_value(name);
        let current_i64 = current_str.as_ref().and_then(|v| v.parse::<i64>().ok());
        let current_bool = current_str.as_ref().map(|v| v.eq_ignore_ascii_case("true"));

        let (recommended, reason) = match name {
            "query_timeout" => {
                let current = current_i64.unwrap_or(300);
                if current >= 600 {
                    return None;
                }
                ("600".to_string(), "Increase the timeout to support complex queries.".to_string())
            },

            "query_mem_limit" => {
                let current = current_i64.unwrap_or(0);
                let total_bytes = cluster_info.total_scan_bytes;
                let recommended = if total_bytes > 0 {
                    (total_bytes * 2).clamp(4 * 1024 * 1024 * 1024, 32 * 1024 * 1024 * 1024) as i64
                } else {
                    8 * 1024 * 1024 * 1024
                };
                if current >= recommended {
                    return None;
                }
                let gb = recommended / (1024 * 1024 * 1024);
                (recommended.to_string(), format!("Recommended memory limit based on data volume: {} GB.", gb))
            },

            "enable_spill" => {
                if current_bool.unwrap_or(false) {
                    return None;
                }
                ("true".to_string(), "Enable this to prevent OOM on large queries.".to_string())
            },

            "pipeline_profile_level" => {
                let current = current_i64.unwrap_or(1);
                if current <= 1 {
                    return None;
                }
                ("1".to_string(), "Lower the Profile level to reduce collection overhead.".to_string())
            },

            "pipeline_dop" => {
                let current = current_i64.unwrap_or(0);
                if current == 0 {
                    return None;
                }
                ("0".to_string(), "Automatic mode is recommended.".to_string())
            },

            "enable_scan_datacache" => {
                if current_bool.unwrap_or(true) {
                    return None;
                }
                ("true".to_string(), "Enable DataCache to improve shared-data architecture performance.".to_string())
            },

            _ => return None,
        };

        let metadata = get_parameter_metadata(name);
        let command = format!("SET {} = {};", name, recommended);
        Some(ParameterSuggestion {
            name: name.to_string(),
            param_type: ParameterType::Session,
            current: current_str, // Always has value from get_variable_value
            recommended,
            command,
            description: metadata.description,
            impact: format!("{} ({})", metadata.impact, reason),
        })
    }
}

/// Query-level rule trait
pub trait QueryRule: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// Evaluate the rule with full context including cluster variables
    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic>;
}

/// Query-level diagnostic result
#[derive(Debug, Clone)]
pub struct QueryDiagnostic {
    pub rule_id: String,
    pub rule_name: String,
    pub severity: RuleSeverity,
    pub message: String,
    pub reason: String,
    pub suggestions: Vec<String>,
    pub parameter_suggestions: Vec<ParameterSuggestion>,
}

/// Q001: Query execution time too long
/// Condition: TotalTime > 60s
pub struct Q001LongRunning;

impl QueryRule for Q001LongRunning {
    fn id(&self) -> &str {
        "Q001"
    }
    fn name(&self) -> &str {
        "Query Execution Takes Too Long"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let total_time_ms = ctx
            .profile
            .summary
            .total_time_ms
            .or_else(|| parse_duration_ms(&ctx.profile.summary.total_time))?;

        if total_time_ms > 60_000.0 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                message: format!(
                    "Query execution time is {}, exceeding the 60-second threshold.",
                    format_duration_ms(total_time_ms)
                ),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Check for bottleneck operators.".to_string(),
                    "Consider optimizing the query plan.".to_string(),
                    "Check for data skew.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = ctx.suggest_parameter("query_timeout") {
                        suggestions.push(s);
                    }
                    if let Some(s) = ctx.suggest_parameter("query_mem_limit") {
                        suggestions.push(s);
                    }
                    suggestions
                },
            })
        } else {
            None
        }
    }
}

/// Q002: Query memory too high
/// Condition: QueryPeakMemory > 10GB
pub struct Q002HighMemory;

impl QueryRule for Q002HighMemory {
    fn id(&self) -> &str {
        "Q002"
    }
    fn name(&self) -> &str {
        "Query Memory Usage Is High"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let peak_memory = ctx.profile.summary.query_peak_memory?;
        const TEN_GB: u64 = 10 * 1024 * 1024 * 1024;

        if peak_memory > TEN_GB {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                message: format!("Query peak memory usage is {}, exceeding the 10 GB threshold.", format_bytes(peak_memory)),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Check for joins involving large tables.".to_string(),
                    "Consider enabling spilling.".to_string(),
                    "Optimize the query to reduce intermediate results.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = ctx.suggest_parameter("enable_spill") {
                        suggestions.push(s);
                    }
                    if let Some(s) = ctx.suggest_parameter("query_mem_limit") {
                        suggestions.push(s);
                    }
                    suggestions
                },
            })
        } else {
            None
        }
    }
}

/// Q003: Query spill detected
/// Condition: QuerySpillBytes > 0
pub struct Q003QuerySpill;

impl QueryRule for Q003QuerySpill {
    fn id(&self) -> &str {
        "Q003"
    }
    fn name(&self) -> &str {
        "Query Spilled to Disk"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let spill_bytes_str = ctx.profile.summary.query_spill_bytes.as_ref()?;

        // Parse spill bytes (e.g., "1.5 GB", "0.000 B")
        let spill_bytes = parse_spill_bytes(spill_bytes_str)?;

        if spill_bytes > 0 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                message: format!("The query spilled {} of data to disk.", format_bytes(spill_bytes)),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Increase the memory limit to reduce spilling.".to_string(),
                    "Optimize the query to reduce intermediate results.".to_string(),
                    "Check whether spilling affects performance.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = ctx.suggest_parameter("query_mem_limit") {
                        suggestions.push(s);
                    }
                    suggestions
                },
            })
        } else {
            None
        }
    }
}

/// Q005: Scan time dominates
/// Condition: ScanTime/TotalTime > 0.8
pub struct Q005ScanDominates;

impl QueryRule for Q005ScanDominates {
    fn id(&self) -> &str {
        "Q005"
    }
    fn name(&self) -> &str {
        "Scan Time Share Is High"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let scan_time_ms = ctx
            .profile
            .summary
            .query_cumulative_scan_time_ms
            .or_else(|| {
                ctx.profile
                    .summary
                    .query_cumulative_scan_time
                    .as_ref()
                    .and_then(|s| parse_duration_ms(s))
            })?;
        let total_time_ms = ctx
            .profile
            .summary
            .total_time_ms
            .or_else(|| parse_duration_ms(&ctx.profile.summary.total_time))?;

        if total_time_ms == 0.0 {
            return None;
        }

        let ratio = scan_time_ms / total_time_ms;

        if ratio > 0.8 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                message: format!("Scan time accounts for {:.1}%; data scanning is the query bottleneck.", ratio * 100.0),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Add filters to reduce the amount of data scanned.".to_string(),
                    "Check whether partition pruning is effective.".to_string(),
                    "Consider creating a materialized view.".to_string(),
                    "Check storage performance.".to_string(),
                ],
                // Only suggest if not already enabled
                parameter_suggestions: ctx
                    .suggest_parameter("enable_scan_datacache")
                    .into_iter()
                    .collect(),
            })
        } else {
            None
        }
    }
}

/// Q006: Network time dominates
/// Condition: NetworkTime/TotalTime > 0.5
pub struct Q006NetworkDominates;

impl QueryRule for Q006NetworkDominates {
    fn id(&self) -> &str {
        "Q006"
    }
    fn name(&self) -> &str {
        "Network Time Share Is High"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let network_time_ms = ctx
            .profile
            .summary
            .query_cumulative_network_time_ms
            .or_else(|| {
                ctx.profile
                    .summary
                    .query_cumulative_network_time
                    .as_ref()
                    .and_then(|s| parse_duration_ms(s))
            })?;
        let total_time_ms = ctx
            .profile
            .summary
            .total_time_ms
            .or_else(|| parse_duration_ms(&ctx.profile.summary.total_time))?;

        if total_time_ms == 0.0 {
            return None;
        }

        let ratio = network_time_ms / total_time_ms;

        if ratio > 0.5 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                message: format!("Network time accounts for {:.1}%; network transfer is the query bottleneck.", ratio * 100.0),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Consider using Colocate Join to reduce Shuffle.".to_string(),
                    "Check network bandwidth.".to_string(),
                    "Reduce cross-node data transfer.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Q004: CPU utilization low
pub struct Q004LowCPU;

impl QueryRule for Q004LowCPU {
    fn id(&self) -> &str {
        "Q004"
    }
    fn name(&self) -> &str {
        "Low CPU Utilization"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let cpu_time = ctx.profile.summary.query_cumulative_cpu_time_ms?;
        let wall_time = ctx.profile.summary.query_execution_wall_time_ms?;
        if wall_time == 0.0 {
            return None;
        }
        let ratio = cpu_time / wall_time;
        if ratio < 0.3 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: super::RuleSeverity::Warning,
                message: format!("CPU utilization is only {:.1}%; there may be a wait or I/O bottleneck.", ratio * 100.0),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec!["Check for wait states.".to_string(), "Increase parallelism.".to_string()],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = ctx.suggest_parameter("pipeline_dop") {
                        suggestions.push(s);
                    }
                    suggestions
                },
            })
        } else {
            None
        }
    }
}

/// Q007: Profile collection slow
pub struct Q007ProfileCollectSlow;

impl QueryRule for Q007ProfileCollectSlow {
    fn id(&self) -> &str {
        "Q007"
    }
    fn name(&self) -> &str {
        "Profile Collection Is Slow"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        // Check CollectProfileTime from variables or execution metrics
        let collect_time = ctx
            .profile
            .execution
            .metrics
            .get("CollectProfileTime")
            .and_then(|v| v.parse::<f64>().ok())?;
        if collect_time > 100_000_000.0 {
            // 100ms in ns
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: super::RuleSeverity::Info,
                message: format!("Profile collection time: {:.1} ms.", collect_time / 1_000_000.0),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec!["Reduce pipeline_profile_level.".to_string()],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = ctx.suggest_parameter("pipeline_profile_level") {
                        suggestions.push(s);
                    }
                    suggestions
                },
            })
        } else {
            None
        }
    }
}

/// Q008: Schedule time too long
pub struct Q008ScheduleTimeLong;

impl QueryRule for Q008ScheduleTimeLong {
    fn id(&self) -> &str {
        "Q008"
    }
    fn name(&self) -> &str {
        "Scheduling Time Is Too Long"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        let schedule_time = ctx.profile.summary.query_peak_schedule_time_ms?;
        let wall_time = ctx.profile.summary.query_execution_wall_time_ms?;
        if wall_time == 0.0 {
            return None;
        }
        let ratio = schedule_time / wall_time;
        if ratio > 0.3 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: super::RuleSeverity::Warning,
                message: format!("Scheduling time accounts for {:.1}%; Pipeline scheduling may be a bottleneck.", ratio * 100.0),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec!["Inspect the Pipeline scheduling bottleneck.".to_string(), "Increase parallelism.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Q009: Result delivery slow
pub struct Q009ResultDeliverySlow;

impl QueryRule for Q009ResultDeliverySlow {
    fn id(&self) -> &str {
        "Q009"
    }
    fn name(&self) -> &str {
        "Result Transfer Is Slow"
    }

    fn evaluate(&self, ctx: &QueryRuleContext) -> Option<QueryDiagnostic> {
        // Check ResultDeliverTime from execution metrics
        let deliver_time = ctx
            .profile
            .execution
            .metrics
            .get("ResultDeliverTime")
            .and_then(|v| v.parse::<f64>().ok())?;
        let wall_time = ctx.profile.summary.query_execution_wall_time_ms? * 1_000_000.0; // to ns
        if wall_time == 0.0 {
            return None;
        }
        let ratio = deliver_time / wall_time;
        if ratio > 0.2 {
            Some(QueryDiagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: super::RuleSeverity::Info,
                message: format!("Result transfer time accounts for {:.1}%.", ratio * 100.0),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec!["Check network bandwidth.".to_string(), "Reduce the result set size.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Parse spill bytes string (e.g., "1.5 GB", "0.000 B")
fn parse_spill_bytes(s: &str) -> Option<u64> {
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

/// Get all query-level rules
pub fn get_rules() -> Vec<Box<dyn QueryRule>> {
    vec![
        Box::new(Q001LongRunning),
        Box::new(Q002HighMemory),
        Box::new(Q003QuerySpill),
        Box::new(Q004LowCPU),
        Box::new(Q005ScanDominates),
        Box::new(Q006NetworkDominates),
        Box::new(Q007ProfileCollectSlow),
        Box::new(Q008ScheduleTimeLong),
        Box::new(Q009ResultDeliverySlow),
    ]
}
