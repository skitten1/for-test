//! Sort/Merge operator diagnostic rules (T001-T005, W001)
//!
//! Rules for SORT and ANALYTIC (Window) operators.

use super::*;

/// T001: Sort rows too large
/// Condition: InputRowNum > 10M without LIMIT
pub struct T001SortRowsTooLarge;

impl DiagnosticRule for T001SortRowsTooLarge {
    fn id(&self) -> &str {
        "T001"
    }
    fn name(&self) -> &str {
        "Too Many Rows to Sort"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SORT")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let input_rows = context.node.metrics.push_row_num? as f64;

        if input_rows > 10_000_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Too many rows to sort ({:.0}); this may cause performance issues.",
                    input_rows
                ),
                reason: "The sort data volume is too large and consumes significant CPU and memory. Consider adding filters to reduce the sort volume or using Top-N optimization.".to_string(),
                suggestions: vec![
                    "Add a LIMIT clause to restrict the result set size.".to_string(),
                    "Check whether Top-N optimization can be used.".to_string(),
                    "Consider pre-sorting with a materialized view.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// T002: Sort spill detected
/// Condition: SpillBytes > 0
pub struct T002SortSpill;

impl DiagnosticRule for T002SortSpill {
    fn id(&self) -> &str {
        "T002"
    }
    fn name(&self) -> &str {
        "Sort Spilled to Disk"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SORT")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let spill_bytes = context
            .get_metric("SpillBytes")
            .or_else(|| context.get_metric("OperatorSpillBytes"))?;

        if spill_bytes > 0.0 {
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
                    "Sort spilled {} of data to disk.",
                    format_bytes(spill_bytes as u64)
                ),
                reason: "The sort data volume exceeded the memory limit and spilled to disk. Spilling can significantly reduce sort performance."
                    .to_string(),
                suggestions: vec![
                    "Increase the memory limit to avoid spilling.".to_string(),
                    "Add a LIMIT clause to reduce the amount of data to sort.".to_string(),
                    "Check whether the query can be optimized to reduce the amount of data being sorted.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("query_mem_limit") {
                        suggestions.push(s);
                    }
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

/// T003: Sort memory too high
/// Condition: OperatorPeakMemoryUsage > 1GB
pub struct T003SortMemoryHigh;

impl DiagnosticRule for T003SortMemoryHigh {
    fn id(&self) -> &str {
        "T003"
    }
    fn name(&self) -> &str {
        "Sort Memory Usage Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SORT")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let memory = context
            .get_metric("OperatorPeakMemoryUsage")
            .or_else(|| context.get_memory_usage().map(|v| v as f64))?;

        const ONE_GB: f64 = 1024.0 * 1024.0 * 1024.0;

        if memory > ONE_GB {
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
                message: format!("Sort memory usage: {}", format_bytes(memory as u64)),
                reason: "Sorting uses excessive memory, which may cause memory pressure or affect other operators.".to_string(),
                suggestions: vec![
                    "Add a LIMIT clause to reduce the amount of data to sort.".to_string(),
                    "Enable spilling to avoid OOM.".to_string(),
                    "Consider processing the data in batches.".to_string(),
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

/// W001: Window function memory too high
/// Condition: Memory > 500MB for ANALYTIC operator
pub struct W001WindowMemoryHigh;

impl DiagnosticRule for W001WindowMemoryHigh {
    fn id(&self) -> &str {
        "W001"
    }
    fn name(&self) -> &str {
        "Window Function Memory Usage Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        name.contains("ANALYTIC") || name.contains("WINDOW")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let memory = context.get_memory_usage()?;
        const THRESHOLD: u64 = 500 * 1024 * 1024; // 500MB

        if memory > THRESHOLD {
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
                message: format!("Window function memory usage: {}", format_bytes(memory)),
                reason: "Window functions use excessive memory, possibly because window partitions or window-function states are too large.".to_string(),
                suggestions: vec![
                    "Check whether PARTITION BY cardinality is too high.".to_string(),
                    "Consider reducing the window size.".to_string(),
                    "Check whether an aggregate function can be used instead.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// T004: Sort merging time too long
pub struct T004SortMergingTimeLong;

impl DiagnosticRule for T004SortMergingTimeLong {
    fn id(&self) -> &str {
        "T004"
    }
    fn name(&self) -> &str {
        "Sort Merge Takes Too Long"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SORT")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let merging_time = context.get_metric("MergingTime")?;
        let op_time = context.get_operator_time_ms()? * 1_000_000.0;
        if op_time == 0.0 {
            return None;
        }
        let ratio = merging_time / op_time;
        if ratio > 0.3 {
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
                message: format!("The Sort merge phase accounts for too much time ({:.1}%).", ratio * 100.0),
                reason: "Multi-way merge sorting takes too long. There may be too many merge runs or too much data in each run.".to_string(),
                suggestions: vec![
                    "Check whether the parallelism setting is appropriate.".to_string(),
                    "Consider reducing the number of partitions.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// T005: Merge waiting for upstream too long
pub struct T005MergeWaitingLong;

impl DiagnosticRule for T005MergeWaitingLong {
    fn id(&self) -> &str {
        "T005"
    }
    fn name(&self) -> &str {
        "Merge Waits Too Long for Upstream"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("MERGE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let pending_time = context.get_metric("6-PendingStageTime")?;
        let overall_time = context.get_metric("OverallStageTime").unwrap_or(1.0);
        if overall_time == 0.0 {
            return None;
        }
        let ratio = pending_time / overall_time;
        if ratio > 0.3 {
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
                message: format!("Merge spends {:.1}% of its time waiting for upstream data.", ratio * 100.0),
                reason: "The Merge operator spends too long waiting for upstream data; an upstream operator may be a performance bottleneck.".to_string(),
                suggestions: vec![
                    "Optimize the producer operator first.".to_string(),
                    "Increase the pipeline buffer size.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Get all sort rules
pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(T001SortRowsTooLarge),
        Box::new(T002SortSpill),
        Box::new(T003SortMemoryHigh),
        Box::new(T004SortMergingTimeLong),
        Box::new(T005MergeWaitingLong),
        Box::new(W001WindowMemoryHigh),
    ]
}
