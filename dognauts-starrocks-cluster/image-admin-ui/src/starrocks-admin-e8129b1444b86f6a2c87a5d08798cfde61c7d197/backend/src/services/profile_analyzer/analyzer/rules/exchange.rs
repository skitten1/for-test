//! Exchange operator diagnostic rules (E001-E003)
//!
//! Rules for EXCHANGE_SINK and EXCHANGE_SOURCE operators.

use super::*;

/// E001: Network transfer too large
/// Condition: BytesSent > 1GB
pub struct E001NetworkTransferLarge;

impl DiagnosticRule for E001NetworkTransferLarge {
    fn id(&self) -> &str {
        "E001"
    }
    fn name(&self) -> &str {
        "Large Network Data Volume"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("EXCHANGE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let bytes_sent = context
            .get_metric("BytesSent")
            .or_else(|| context.get_metric("NetworkBytesSent"))?;

        const ONE_GB: f64 = 1024.0 * 1024.0 * 1024.0;

        if bytes_sent > ONE_GB {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Network transfer volume is {}; a network bottleneck may exist.",
                    format_bytes(bytes_sent as u64)
                ),
                reason: "The network transfer volume is excessive and consumes significant bandwidth and time. This may be caused by a large Shuffle volume or insufficient data pruning.".to_string(),
                suggestions: vec![
                    "Check whether the Shuffle data volume can be reduced.".to_string(),
                    "Consider using Colocate Join to avoid Shuffle.".to_string(),
                    "Check whether network bandwidth is sufficient.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    // Use smart recommendation for parallel_fragment_exec_instance_num
                    if let Some(s) = context.suggest_parameter_smart("parallel_fragment_exec_instance_num") {
                        suggestions.push(s);
                    }
                    // Use smart recommendation for pipeline_dop
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

/// E002: Network time too high
/// Condition: NetworkTime/OperatorTime > 0.5
pub struct E002NetworkTimeHigh;

impl DiagnosticRule for E002NetworkTimeHigh {
    fn id(&self) -> &str {
        "E002"
    }
    fn name(&self) -> &str {
        "Network Time Share Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("EXCHANGE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let network_time = context
            .get_metric("NetworkTime")
            .or_else(|| context.get_metric("OverallThroughputSendTime"))?;
        let operator_time = context.get_operator_time_ms()?;

        if operator_time == 0.0 {
            return None;
        }

        let ratio = network_time / operator_time;

        if ratio > 0.5 {
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
                message: format!("Network time accounts for {:.1}%; a network bottleneck may exist.", ratio * 100.0),
                reason: "Network transfer accounts for too much time, making the network the query bottleneck. Possible causes include insufficient bandwidth or cross-datacenter traffic."
                    .to_string(),
                suggestions: vec![
                    "Check network bandwidth and latency.".to_string(),
                    "Consider using Colocate Join.".to_string(),
                    "Reduce cross-node data transfer.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// E003: Data skew in shuffle
/// Condition: max(BytesSent)/avg > 2
pub struct E003ShuffleSkew;

impl DiagnosticRule for E003ShuffleSkew {
    fn id(&self) -> &str {
        "E003"
    }
    fn name(&self) -> &str {
        "Shuffle Data Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("EXCHANGE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_bytes = context.get_metric("__MAX_OF_BytesSent")?;
        let min_bytes = context.get_metric("__MIN_OF_BytesSent").unwrap_or(0.0);

        if min_bytes == 0.0 {
            return None;
        }

        let avg_bytes = (max_bytes + min_bytes) / 2.0;
        let ratio = max_bytes / avg_bytes;

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
                    "Shuffle data skew detected; max/avg ratio: {:.2}",
                    ratio
                ),
                reason: "Shuffle data is distributed unevenly across nodes, causing some nodes to receive more data. This is usually caused by hot values in the Shuffle key.".to_string(),
                suggestions: vec![
                    "Check whether the partition key is appropriate.".to_string(),
                    "Consider optimizing with Skew Join.".to_string(),
                    "Check for hot data.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Get all exchange rules
pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(E001NetworkTransferLarge),
        Box::new(E002NetworkTimeHigh),
        Box::new(E003ShuffleSkew),
    ]
}
