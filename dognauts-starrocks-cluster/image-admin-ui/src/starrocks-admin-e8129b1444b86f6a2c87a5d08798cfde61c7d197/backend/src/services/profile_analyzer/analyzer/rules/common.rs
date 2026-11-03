//! Common diagnostic rules (G001-G003)
//!
//! These rules apply to all operator types.

use super::*;

/// G001: Time percentage too high (most consuming node)
/// Threshold: > 30% (aligned with StarRocks ExplainAnalyzer.java)
pub struct G001MostConsuming;

impl DiagnosticRule for G001MostConsuming {
    fn id(&self) -> &str {
        "G001"
    }
    fn name(&self) -> &str {
        "Operator Time Share Is High"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true // Applies to all nodes
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let percentage = context.get_time_percentage()?;

        if percentage > 30.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Error,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "🔴 Operator {} accounts for {:.1}% of execution time (the most time-consuming node).",
                    context.node.operator_name, percentage
                ),
                suggestions: get_operator_suggestions(&context.node.operator_name),
                reason: "This operator accounts for a large share of the total query time and is the primary bottleneck. Optimizing it is likely to provide the greatest benefit.".to_string(),
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// G001b: Time percentage high (second most consuming node)
/// Threshold: > 15% (aligned with StarRocks ExplainAnalyzer.java)
pub struct G001bSecondConsuming;

impl DiagnosticRule for G001bSecondConsuming {
    fn id(&self) -> &str {
        "G001b"
    }
    fn name(&self) -> &str {
        "Operator Time Share Is Elevated"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let percentage = context.get_time_percentage()?;

        // Only trigger if between 15% and 30% (G001 handles > 30%)
        if percentage > 15.0 && percentage <= 30.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "🟠 Operator {} accounts for {:.1}% of execution time (the second most time-consuming node).",
                    context.node.operator_name, percentage
                ),
                suggestions: get_operator_suggestions(&context.node.operator_name),
                reason: "This operator accounts for a large share of the total query time and is the primary bottleneck. Optimizing it is likely to provide the greatest benefit.".to_string(),
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// G002: Memory usage too high
/// Threshold: > 1GB
pub struct G002HighMemory;

impl DiagnosticRule for G002HighMemory {
    fn id(&self) -> &str {
        "G002"
    }
    fn name(&self) -> &str {
        "Operator Memory Usage Is High"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let memory = context.get_memory_usage()?;
        const ONE_GB: u64 = 1024 * 1024 * 1024;

        if memory > ONE_GB {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Operator {} memory usage is high: {}",
                    context.node.operator_name, format_bytes(memory)
                ),
                reason: "The operator uses excessive memory, which may cause the query to fail or trigger spilling. Check for data expansion or oversized intermediate results.".to_string(),
                suggestions: vec![
                    "Check for data expansion.".to_string(),
                    "Consider processing the data in batches.".to_string(),
                    "Check whether the Hash Table or intermediate results are too large.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("query_mem_limit") {
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

/// G003: Execution time skew across instances
/// Threshold: max/avg > 2
pub struct G003ExecutionSkew;

impl DiagnosticRule for G003ExecutionSkew {
    fn id(&self) -> &str {
        "G003"
    }
    fn name(&self) -> &str {
        "Operator Execution Time Skew"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        // Check if we have min/max time metrics
        let max_time = context.node.metrics.operator_total_time_max?;
        let _min_time = context.node.metrics.operator_total_time_min.unwrap_or(0);
        let avg_time = context.node.metrics.operator_total_time?;

        if avg_time == 0 {
            return None;
        }

        let ratio = max_time as f64 / avg_time as f64;

        if ratio > 2.0 {
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
                    "Operator {} has execution-time skew; max/avg ratio: {:.2}",
                    context.node.operator_name, ratio
                ),
                reason:
                    "Execution time varies significantly across operator instances, making some instances bottlenecks. This is usually caused by uneven data distribution."
                        .to_string(),
                suggestions: vec![
                    "Check whether data is distributed evenly.".to_string(),
                    "Check whether the bucketing keys are appropriate.".to_string(),
                    "Consider increasing parallelism.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("pipeline_dop") {
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

/// Get operator-specific suggestions based on operator name
fn get_operator_suggestions(operator_name: &str) -> Vec<String> {
    let name = operator_name.to_uppercase();

    if name.contains("SCAN") {
        vec![
            "Check whether filters can be added to reduce the amount of data scanned.".to_string(),
            "Consider adding an index or materialized view.".to_string(),
            "Check whether partition pruning is effective.".to_string(),
            "Run ANALYZE TABLE to update statistics.".to_string(),
        ]
    } else if name.contains("JOIN") {
        vec![
            "Check whether the JOIN order is optimal.".to_string(),
            "Consider using a Runtime Filter.".to_string(),
            "Check for data skew.".to_string(),
            "Run ANALYZE TABLE to update statistics.".to_string(),
        ]
    } else if name.contains("AGGREGATE") || name.contains("AGG") {
        vec![
            "Check whether the aggregation strategy is appropriate.".to_string(),
            "Consider using pre-aggregation or a materialized view.".to_string(),
            "Check the selection of GROUP BY keys.".to_string(),
        ]
    } else if name.contains("EXCHANGE") {
        vec![
            "Check whether data is distributed evenly.".to_string(),
            "Consider adjusting parallelism.".to_string(),
            "Check whether network bandwidth is sufficient.".to_string(),
        ]
    } else if name.contains("SORT") {
        vec![
            "Add a LIMIT clause to restrict the result set size.".to_string(),
            "Check whether Top-N optimization can be used.".to_string(),
            "Consider pre-sorting with a materialized view.".to_string(),
        ]
    } else {
        vec!["Check whether this operator is processing too much data.".to_string(), "Consider optimizing the query plan.".to_string()]
    }
}

/// Get all common rules
pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(G001MostConsuming),
        Box::new(G001bSecondConsuming),
        Box::new(G002HighMemory),
        Box::new(G003ExecutionSkew),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::profile_analyzer::models::{
        ExecutionTreeNode, HotSeverity, NodeType, OperatorMetrics,
    };
    use std::collections::HashMap;

    #[test]
    fn test_g001_threshold() {
        let rule = G001MostConsuming;
        assert_eq!(rule.id(), "G001");
    }

    #[test]
    fn test_g001_triggers_on_high_percentage() {
        let rule = G001MostConsuming;

        // Create a node with 99.84% time percentage
        let node = ExecutionTreeNode {
            id: "test_node".to_string(),
            operator_name: "OLAP_SCAN".to_string(),
            node_type: NodeType::OlapScan,
            plan_node_id: Some(0),
            parent_plan_node_id: None,
            metrics: OperatorMetrics::default(),
            children: vec![],
            depth: 0,
            is_hotspot: false,
            hotspot_severity: HotSeverity::Normal,
            fragment_id: None,
            pipeline_id: None,
            time_percentage: Some(99.84),
            rows: None,
            is_most_consuming: true,
            is_second_most_consuming: false,
            unique_metrics: HashMap::new(),
            has_diagnostic: false,
            diagnostic_ids: vec![],
        };

        let session_variables = std::collections::HashMap::new();
        let context = RuleContext {
            node: &node,
            session_variables: &session_variables,
            cluster_info: None,
            cluster_variables: None,
            default_db: None,
        };
        let result = rule.evaluate(&context);

        assert!(result.is_some(), "G001 should trigger for 99.84% time percentage");
        let diag = result.unwrap();
        assert_eq!(diag.rule_id, "G001");
        assert_eq!(diag.plan_node_id, Some(0));
    }
}
