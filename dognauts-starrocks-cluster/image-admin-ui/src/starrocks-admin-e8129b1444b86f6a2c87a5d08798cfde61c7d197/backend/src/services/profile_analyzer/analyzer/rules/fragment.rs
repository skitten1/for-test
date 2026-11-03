//! Fragment level diagnostic rules (F001-F003)

use super::*;

/// F001: Execution time skew across instances
pub struct F001ExecutionTimeSkew;

impl DiagnosticRule for F001ExecutionTimeSkew {
    fn id(&self) -> &str {
        "F001"
    }
    fn name(&self) -> &str {
        "Instance Execution Time Skew"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true
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
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("Instance execution-time skew detected; max/avg ratio: {:.2}", ratio),
                reason: "The Fragment takes too long to execute and is the main query bottleneck. Analyze the operators inside the Fragment to locate the cause.".to_string(),
                suggestions: vec!["Check data distribution.".to_string(), "Optimize the bucketing strategy.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// F002: Memory allocation skew
pub struct F002MemorySkew;

impl DiagnosticRule for F002MemorySkew {
    fn id(&self) -> &str {
        "F002"
    }
    fn name(&self) -> &str {
        "Uneven Instance Memory Allocation"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_mem = context.get_metric("__MAX_OF_OperatorPeakMemoryUsage")?;
        let min_mem = context
            .get_metric("__MIN_OF_OperatorPeakMemoryUsage")
            .unwrap_or(0.0);
        if min_mem == 0.0 {
            return None;
        }
        let ratio = max_mem / ((max_mem + min_mem) / 2.0);
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
                message: format!("Instance memory allocation is uneven; max/avg ratio: {:.2}", ratio),
                reason: "Fragment memory usage is too high and may cause the query to fail.".to_string(),
                suggestions: vec!["Check for data skew.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// F003: Fragment prepare time too long
pub struct F003PrepareTimeLong;

impl DiagnosticRule for F003PrepareTimeLong {
    fn id(&self) -> &str {
        "F003"
    }
    fn name(&self) -> &str {
        "Fragment Preparation Takes Too Long"
    }

    fn applicable_to(&self, _node: &ExecutionTreeNode) -> bool {
        true
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let prepare_time = context.get_metric("FragmentInstancePrepareTime")?;
        if prepare_time > 1_000_000_000.0 {
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
                message: format!("Fragment preparation time: {:.1}s", prepare_time / 1_000_000_000.0),
                reason: "Execution times vary significantly across Fragment instances, indicating data skew or uneven resource allocation.".to_string(),
                suggestions: vec!["Check metadata loading.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![Box::new(F001ExecutionTimeSkew), Box::new(F002MemorySkew), Box::new(F003PrepareTimeLong)]
}
