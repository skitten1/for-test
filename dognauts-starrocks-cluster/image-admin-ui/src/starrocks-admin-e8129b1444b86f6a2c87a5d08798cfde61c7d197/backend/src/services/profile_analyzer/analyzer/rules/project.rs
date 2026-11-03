//! Project and LocalExchange operator rules (P001, L001)

use super::*;

/// P001: Project expression compute time high
pub struct P001ProjectExprHigh;

impl DiagnosticRule for P001ProjectExprHigh {
    fn id(&self) -> &str {
        "P001"
    }
    fn name(&self) -> &str {
        "Project Expression Evaluation Is Expensive"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("PROJECT")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let expr_time = context.get_metric("ExprComputeTime")?;
        let op_time = context.get_operator_time_ms()? * 1_000_000.0; // Convert to ns
        if op_time == 0.0 {
            return None;
        }
        let ratio = expr_time / op_time;
        if ratio > 0.5 && expr_time > 100_000_000.0 {
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
                message: format!("Project expression evaluation accounts for too much time ({:.1}%).", ratio * 100.0),
                reason: "The Project operator takes too long, possibly because expressions are complex or the data volume is large.".to_string(),
                suggestions: vec![
                    "Simplify complex expressions in SELECT.".to_string(),
                    "Move complex calculations into a materialized view for precomputation.".to_string(),
                    "Check for unnecessary type conversions.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// L001: LocalExchange memory too high
pub struct L001LocalExchangeMemory;

impl DiagnosticRule for L001LocalExchangeMemory {
    fn id(&self) -> &str {
        "L001"
    }
    fn name(&self) -> &str {
        "LocalExchange Memory Usage Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("LOCAL")
            && node.operator_name.to_uppercase().contains("EXCHANGE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let memory = context
            .get_metric("LocalExchangePeakMemoryUsage")
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
                message: format!("LocalExchange memory usage: {}", format_bytes(memory as u64)),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Check whether data flow between upstream and downstream operators is balanced.".to_string(),
                    "Adjust the pipeline_dop parameter.".to_string(),
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

pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![Box::new(P001ProjectExprHigh), Box::new(L001LocalExchangeMemory)]
}
