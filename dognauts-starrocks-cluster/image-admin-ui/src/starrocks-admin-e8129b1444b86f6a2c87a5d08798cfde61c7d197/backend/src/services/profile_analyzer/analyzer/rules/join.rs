//! Join operator diagnostic rules (J001-J010)
//!
//! Rules for HASH_JOIN, CROSS_JOIN, NEST_LOOP_JOIN operators.

use super::*;

/// J001: Join result explosion
/// Condition: PullRowNum > ProbeRows * 10
pub struct J001ResultExplosion;

impl DiagnosticRule for J001ResultExplosion {
    fn id(&self) -> &str {
        "J001"
    }
    fn name(&self) -> &str {
        "Join Result Expansion"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let output_rows = context.node.metrics.pull_row_num? as f64;
        let probe_rows = context.get_metric("ProbeRows")?;

        if probe_rows == 0.0 {
            return None;
        }

        let ratio = output_rows / probe_rows;

        if ratio > 10.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Error,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Join result expanded by {:.1}x (output {:.0} rows / probe {:.0} rows).",
                    ratio, output_rows, probe_rows
                ),
                reason: "The Join output is much larger than its input. This is often caused by a missing Join condition (resulting in a Cross Join) or an incorrect condition that produces 1:N matches.".to_string(),
                suggestions: vec![
                    "Check whether the JOIN condition is missing or incomplete.".to_string(),
                    "Check for many-to-many relationships.".to_string(),
                    "Consider adding more filter conditions.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J002: Build side larger than probe side
/// Condition: BuildRows > ProbeRows
pub struct J002BuildLargerThanProbe;

impl DiagnosticRule for J002BuildLargerThanProbe {
    fn id(&self) -> &str {
        "J002"
    }
    fn name(&self) -> &str {
        "Join Build Side Is Too Large"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("HASH")
            && node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let build_rows = context.get_metric("BuildRows")?;
        let probe_rows = context.get_metric("ProbeRows")?;

        if build_rows > probe_rows && build_rows > 100_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "The Build side row count ({:.0}) exceeds the Probe side ({:.0}); the Join order may be suboptimal.",
                    build_rows, probe_rows
                ),
                reason: "In a Hash Join, the Build side contains more data than the Probe side, resulting in an oversized Hash Table. Inaccurate statistics may have caused the optimizer to choose the wrong Build side.".to_string(),
                suggestions: vec![
                    "Run ANALYZE TABLE to update statistics.".to_string(),
                    "Check whether the optimizer selected the correct Join order.".to_string(),
                    "Consider using a Hint to specify the Join order.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J003: HashTable memory too large
/// Condition: HashTableMemoryUsage > 1GB
pub struct J003HashTableTooLarge;

impl DiagnosticRule for J003HashTableTooLarge {
    fn id(&self) -> &str {
        "J003"
    }
    fn name(&self) -> &str {
        "Hash Table Memory Usage Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("HASH")
            && node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let hash_memory = context
            .get_metric("HashTableMemoryUsage")
            .or_else(|| context.get_memory_usage().map(|v| v as f64))?;

        const ONE_GB: f64 = 1024.0 * 1024.0 * 1024.0;

        if hash_memory > ONE_GB {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!(
                    "{} (plan_node_id={})",
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)
                ),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Hash Table memory usage is {}; this may cause memory pressure.",
                    format_bytes(hash_memory as u64)
                ),
                reason: "The Join Hash Table uses excessive memory, which may cause memory pressure or trigger spilling."
                    .to_string(),
                suggestions: vec![
                    "Check whether the Build side contains too much data.".to_string(),
                    "Consider using a Runtime Filter to reduce the data volume.".to_string(),
                    "Enable spilling to avoid OOM.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("enable_spill") {
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

/// J004: Runtime Filter not generated
/// Condition: RuntimeFilterNum == 0 && BuildRows > 10000
pub struct J004NoRuntimeFilter;

impl DiagnosticRule for J004NoRuntimeFilter {
    fn id(&self) -> &str {
        "J004"
    }
    fn name(&self) -> &str {
        "Runtime Filter Was Not Generated"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let rf_num = context.get_metric("RuntimeFilterNum").unwrap_or(0.0);
        let build_rows = context.get_metric("BuildRows").unwrap_or(0.0);

        if rf_num == 0.0 && build_rows > 10_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "No Runtime Filter was generated for the Join; the Build side has {:.0} rows.",
                    build_rows
                ),
                reason: "The Runtime Filter is ineffective or has low selectivity, so it does not filter data effectively on the Scan side. The Filter may have failed to build or may have poor selectivity.".to_string(),
                suggestions: vec![
                    "Check whether enable_global_runtime_filter is enabled.".to_string(),
                    "Check whether the Join condition is suitable for generating a Runtime Filter.".to_string(),
                    "Check whether the Build side row count exceeds the threshold.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("enable_global_runtime_filter") {
                        suggestions.push(s);
                    }
                    if let Some(s) = context.suggest_parameter_smart("runtime_join_filter_push_down_limit") {
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

/// J009: Non-equi join fallback
/// Condition: JoinType contains CROSS or NESTLOOP
pub struct J009NonEquiJoin;

impl DiagnosticRule for J009NonEquiJoin {
    fn id(&self) -> &str {
        "J009"
    }
    fn name(&self) -> &str {
        "Non-Equi Join Fallback"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        name.contains("CROSS") || name.contains("NEST") || name.contains("LOOP")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let probe_rows = context.get_metric("ProbeRows").unwrap_or(0.0);
        let build_rows = context.get_metric("BuildRows").unwrap_or(0.0);

        // Only warn if significant data volume
        if probe_rows > 1000.0 || build_rows > 1000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Operator {} was used, possibly because of a non-equi Join.",
                    context.node.operator_name
                ),
                reason: "The Join condition contains a non-equality predicate, so Hash Join cannot be used. The query falls back to Nested Loop Join, which is less efficient.".to_string(),
                suggestions: vec![
                    "Check whether the JOIN condition contains an equality predicate.".to_string(),
                    "Try converting the non-equality predicate into an equality predicate.".to_string(),
                    "Consider restructuring the query logic.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J010: Probe cache unfriendly
/// Condition: ProbeRows >> BuildRows && HashTableMemory > L3 Cache
pub struct J010ProbeCacheUnfriendly;

impl DiagnosticRule for J010ProbeCacheUnfriendly {
    fn id(&self) -> &str {
        "J010"
    }
    fn name(&self) -> &str {
        "Probe Access Is Cache-Unfriendly"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("HASH")
            && node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let hash_memory = context
            .get_metric("HashTableMemoryUsage")
            .or_else(|| context.get_memory_usage().map(|v| v as f64))?;
        let probe_rows = context.get_metric("ProbeRows")?;
        let build_rows = context.get_metric("BuildRows")?;

        // L3 cache typically 30-50MB, use 50MB as threshold
        const L3_CACHE: f64 = 50.0 * 1024.0 * 1024.0;

        if hash_memory > L3_CACHE && probe_rows > build_rows * 100.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "The Hash Table ({}) exceeds the L3 cache, and the number of probe rows ({:.0}) is much larger than the number of build rows ({:.0}); access may be cache-unfriendly.",
                    format_bytes(hash_memory as u64), probe_rows, build_rows
                ),
                reason: "Hash table probing has a low cache hit rate. The Hash Table may exceed the CPU cache, or the probe data access pattern may be cache-unfriendly.".to_string(),
                suggestions: vec![
                    "Consider swapping the left and right tables in the Join.".to_string(),
                    "Use a Hint to specify the Join order.".to_string(),
                    "Check whether the statistics are accurate.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J005: Hash collision severe
pub struct J005HashCollision;

impl DiagnosticRule for J005HashCollision {
    fn id(&self) -> &str {
        "J005"
    }
    fn name(&self) -> &str {
        "Severe Hash Collisions"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let keys_per_bucket = context.get_metric("BuildKeysPerBucket%")?;
        if keys_per_bucket > 10.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("Severe hash-table collisions: an average of {:.1} keys per bucket.", keys_per_bucket),
                reason: "The hash table has many collisions, reducing probe efficiency. This may be caused by an uneven Join key distribution or an ineffective hash function.".to_string(),
                suggestions: vec!["Optimize the Join key selection.".to_string(), "Check for a large number of duplicate values.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J006: Join shuffle skew
pub struct J006ShuffleSkew;

impl DiagnosticRule for J006ShuffleSkew {
    fn id(&self) -> &str {
        "J006"
    }
    fn name(&self) -> &str {
        "Join Shuffle Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_probe = context.get_metric("__MAX_OF_ProbeRows")?;
        let min_probe = context.get_metric("__MIN_OF_ProbeRows").unwrap_or(0.0);
        if min_probe == 0.0 {
            return None;
        }
        let ratio = max_probe / ((max_probe + min_probe) / 2.0);
        if ratio > 3.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("Join data distribution is skewed; max/avg ratio: {:.2}", ratio),
                reason: "Shuffle Join data is distributed unevenly, causing some nodes to process more data. This is usually caused by hot values in the Join key.".to_string(),
                suggestions: vec!["Switch to a higher-cardinality Join key.".to_string(), "Add a salt value to the key.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J007: Partition join probe overhead high
pub struct J007PartitionProbeOverhead;

impl DiagnosticRule for J007PartitionProbeOverhead {
    fn id(&self) -> &str {
        "J007"
    }
    fn name(&self) -> &str {
        "Partitioned Join Probe Overhead Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let partition_nums = context.get_metric("PartitionNums")?;
        let probe_overhead = context.get_metric("PartitionProbeOverhead").unwrap_or(0.0);
        let search_time = context.get_metric("SearchHashTableTime").unwrap_or(1.0);
        if partition_nums > 1.0 && search_time > 0.0 && probe_overhead / search_time > 0.5 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!(
                    "{} (plan_node_id={})",
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)
                ),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Partition probe overhead is {:.1}%, with {:.0} partitions.",
                    probe_overhead / search_time * 100.0,
                    partition_nums
                ),
                reason: "Partition probe overhead is too high. The number of partitions may be excessive, or the partitioning strategy may be unsuitable.".to_string(),
                suggestions: vec![
                    "Check whether the number of partitions is appropriate.".to_string(),
                    "Consider increasing the memory limit to avoid excessive partitioning.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// J008: Runtime filter memory high
pub struct J008RFMemoryHigh;

impl DiagnosticRule for J008RFMemoryHigh {
    fn id(&self) -> &str {
        "J008"
    }
    fn name(&self) -> &str {
        "Runtime Filter Memory Usage Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let rf_bytes = context.get_metric("PartialRuntimeMembershipFilterBytes")?;
        const HUNDRED_MB: f64 = 100.0 * 1024.0 * 1024.0;
        if rf_bytes > HUNDRED_MB {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!(
                    "{} (plan_node_id={})",
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)
                ),
                plan_node_id: context.node.plan_node_id,
                message: format!("Runtime Filter memory usage: {}", format_bytes(rf_bytes as u64)),
                reason: "Runtime Filters consume too much memory. There may be too many Filters, or an individual Filter may be too large."
                    .to_string(),
                suggestions: vec![
                    "Reduce the runtime_filter_max_size setting.".to_string(),
                    "Check whether Join key cardinality is too high.".to_string(),
                ],
                parameter_suggestions: vec![ParameterSuggestion::new(
                    "runtime_filter_max_size",
                    ParameterType::Session,
                    None,
                    "67108864",
                    "SET runtime_filter_max_size = 67108864; -- 64MB",
                )],
            })
        } else {
            None
        }
    }
}

/// J011: Broadcast Join not recommended for large build table
/// Condition: DistributionMode = BROADCAST and BuildRows > 1M or HashTableMemoryUsage > 100MB
pub struct J011BroadcastNotRecommended;

impl DiagnosticRule for J011BroadcastNotRecommended {
    fn id(&self) -> &str {
        "J011"
    }
    fn name(&self) -> &str {
        "Broadcast Join Should Be Avoided"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("JOIN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        // Check if using Broadcast distribution
        let dist_mode = context.node.unique_metrics.get("DistributionMode")?;
        if !dist_mode.to_uppercase().contains("BROADCAST") {
            return None;
        }

        // Check build side size
        let build_rows = context.get_metric("BuildRows").unwrap_or(0.0);
        let hash_table_memory = context.get_metric("HashTableMemoryUsage").unwrap_or(0.0);

        // Thresholds: 1M rows or 100MB memory
        let rows_threshold = 1_000_000.0;
        let memory_threshold = 100.0 * 1024.0 * 1024.0; // 100MB

        if build_rows > rows_threshold || hash_table_memory > memory_threshold {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!(
                    "{} (plan_node_id={})",
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)
                ),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Broadcast Join Build side is too large ({:.0} rows, {:.2} MB).",
                    build_rows,
                    hash_table_memory / 1024.0 / 1024.0
                ),
                reason: "When a small table is joined with a large table, a Broadcast Join sends the small table to every node.\
                    If the Build side contains too much data, network and compute costs increase.\
                    This may happen when inaccurate statistics cause the optimizer to estimate table sizes incorrectly."
                    .to_string(),
                suggestions: vec![
                    "Add the [shuffle] Hint after the JOIN keyword to prevent Broadcast Join.".to_string(),
                    "Check whether statistics have been collected or have expired, and run ANALYZE TABLE.".to_string(),
                    "Check whether the Join order is appropriate.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Get all join rules
pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(J001ResultExplosion),
        Box::new(J002BuildLargerThanProbe),
        Box::new(J003HashTableTooLarge),
        Box::new(J004NoRuntimeFilter),
        Box::new(J005HashCollision),
        Box::new(J006ShuffleSkew),
        Box::new(J007PartitionProbeOverhead),
        Box::new(J008RFMemoryHigh),
        Box::new(J009NonEquiJoin),
        Box::new(J010ProbeCacheUnfriendly),
        Box::new(J011BroadcastNotRecommended),
    ]
}
