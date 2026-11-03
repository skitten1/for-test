//! Aggregate operator diagnostic rules (A001-A005)
//!
//! Rules for AGGREGATE operators.

use super::*;

/// A001: Aggregation skew
/// Condition: max(AggComputeTime)/avg > 2
pub struct A001AggregationSkew;

impl DiagnosticRule for A001AggregationSkew {
    fn id(&self) -> &str {
        "A001"
    }
    fn name(&self) -> &str {
        "Aggregation Data Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("AGG")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_time = context.node.metrics.operator_total_time_max?;
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
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Aggregation data skew detected; max/avg ratio: {:.2}",
                    ratio
                ),
                reason: "Aggregation instances process significantly different data volumes, making some instances bottlenecks. This is usually caused by an uneven distribution of GROUP BY keys.".to_string(),
                suggestions: vec![
                    "Check the data distribution of the GROUP BY keys.".to_string(),
                    "Consider using two-phase aggregation.".to_string(),
                    "Check for hot keys.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// A002: HashTable memory too large
/// Condition: HashTableMemoryUsage > 1GB
pub struct A002HashTableTooLarge;

impl DiagnosticRule for A002HashTableTooLarge {
    fn id(&self) -> &str {
        "A002"
    }
    fn name(&self) -> &str {
        "Aggregation Hash Table Is Too Large"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("AGG")
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
                    "Aggregation Hash Table memory usage: {}",
                    format_bytes(memory)
                ),
                reason: "The Hash Table uses excessive memory, which may create memory pressure or trigger spilling. This is usually caused by high GROUP BY key cardinality or large aggregation-function states.".to_string(),
                suggestions: vec![
                    "Check whether GROUP BY cardinality is too high.".to_string(),
                    "Consider pre-aggregating with a materialized view.".to_string(),
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

/// A004: High cardinality GROUP BY
/// Condition: HashTableSize > 10M
pub struct A004HighCardinality;

impl DiagnosticRule for A004HighCardinality {
    fn id(&self) -> &str {
        "A004"
    }
    fn name(&self) -> &str {
        "High-Cardinality GROUP BY"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("AGG")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let hash_size = context
            .get_metric("HashTableSize")
            .or_else(|| context.node.metrics.pull_row_num.map(|v| v as f64))?;

        if hash_size > 10_000_000.0 {
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
                message: format!("GROUP BY cardinality is too high ({:.0} groups).", hash_size),
                reason:
                    "High GROUP BY key cardinality creates too many Hash Table entries, increases memory usage, and reduces aggregation efficiency."
                        .to_string(),
                suggestions: vec![
                    "Check whether the selected GROUP BY keys are appropriate.".to_string(),
                    "Consider using streaming aggregation.".to_string(),
                    "Consider creating a materialized view for pre-aggregation.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// A003: Aggregation data skew
pub struct A003DataSkew;

impl DiagnosticRule for A003DataSkew {
    fn id(&self) -> &str {
        "A003"
    }
    fn name(&self) -> &str {
        "Aggregation Data Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("AGGREGATE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_input = context.get_metric("__MAX_OF_InputRowCount")?;
        let min_input = context.get_metric("__MIN_OF_InputRowCount").unwrap_or(0.0);
        if min_input == 0.0 {
            return None;
        }
        let ratio = max_input / ((max_input + min_input) / 2.0);
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
                message: format!("Aggregation data skew detected; max/avg ratio: {:.2}", ratio),
                reason: "Input data is distributed unevenly across aggregation instances, causing some instances to process more data."
                    .to_string(),
                suggestions: vec!["Optimize the grouping-key selection.".to_string(), "Consider handling hot keys separately.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// A005: Expensive key expression
pub struct A005ExpensiveKeyExpr;

impl DiagnosticRule for A005ExpensiveKeyExpr {
    fn id(&self) -> &str {
        "A005"
    }
    fn name(&self) -> &str {
        "GROUP BY Key Expression Is Expensive"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("AGGREGATE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let expr_time = context.get_metric("ExprComputeTime")?;
        let agg_time = context.get_metric("AggFuncComputeTime").unwrap_or(1.0);
        if agg_time == 0.0 {
            return None;
        }
        let ratio = expr_time / agg_time;
        if ratio > 0.5 && expr_time > 100_000_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("GROUP BY key expression evaluation accounts for too much time ({:.1}%).", ratio * 100.0),
                reason: "GROUP BY keys contain complex expressions that must be evaluated for every row, increasing CPU overhead. Consider precomputing the expressions or using generated columns.".to_string(),
                suggestions: vec![
                    "Materialize complex expressions in a subquery.".to_string(),
                    "Convert the expression into a generated column.".to_string(),
                    "Avoid using complex functions in GROUP BY.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// A006: Low local aggregation efficiency
/// Condition: InputRowCount / OutputRowCount < 2.0 (aggregation reduces less than 50%)
pub struct A006LowLocalAggregation;

impl DiagnosticRule for A006LowLocalAggregation {
    fn id(&self) -> &str {
        "A006"
    }
    fn name(&self) -> &str {
        "Low Local Aggregation Efficiency"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        // Only apply to local/first-stage aggregation
        name.contains("AGGREGATE") || name.contains("AGG")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        // Get input and output row counts
        let input_rows = context
            .get_metric("InputRowCount")
            .or_else(|| context.node.metrics.pull_row_num.map(|v| v as f64))?;
        let output_rows = context
            .get_metric("OutputRowCount")
            .or_else(|| context.node.metrics.push_row_num.map(|v| v as f64))?;

        if output_rows == 0.0 || input_rows == 0.0 {
            return None;
        }

        // Calculate aggregation ratio
        let agg_ratio = input_rows / output_rows;

        // If aggregation ratio < 2.0 (less than 50% reduction), local aggregation is ineffective
        // Also check that we have significant data (> 10K rows)
        if agg_ratio < 2.0 && input_rows > 10_000.0 {
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
                    "Local aggregation is ineffective; aggregation ratio {:.2}:1 (input {:.0} rows → output {:.0} rows).",
                    agg_ratio, input_rows, output_rows
                ),
                reason:
                    "Compute nodes usually aggregate data locally first to reduce the result size before sending it to other nodes.\
                    When local aggregation does not reduce the data volume effectively, it consumes significant compute resources without reducing network traffic."
                        .to_string(),
                suggestions: vec![
                    "If aggregation takes too long overall, consider disabling two-phase aggregation.".to_string(),
                    "Check whether GROUP BY key cardinality is too high.".to_string(),
                    "Consider pre-aggregating at the data source.".to_string(),
                ],
                parameter_suggestions: vec![ParameterSuggestion::new(
                    "new_planner_agg_stage",
                    ParameterType::Session,
                    None,
                    "1",
                    "SET new_planner_agg_stage = 1; -- Disable two-phase aggregation",
                )],
            })
        } else {
            None
        }
    }
}

/// Get all aggregate rules
pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(A001AggregationSkew),
        Box::new(A002HashTableTooLarge),
        Box::new(A003DataSkew),
        Box::new(A004HighCardinality),
        Box::new(A005ExpensiveKeyExpr),
        Box::new(A006LowLocalAggregation),
    ]
}
