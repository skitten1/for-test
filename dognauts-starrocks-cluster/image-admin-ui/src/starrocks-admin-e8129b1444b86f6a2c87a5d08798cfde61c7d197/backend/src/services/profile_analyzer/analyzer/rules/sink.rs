//! OlapTableSink operator rules (I001-I003) for data import scenarios

use super::*;

/// I001: Import data skew
pub struct I001ImportDataSkew;

impl DiagnosticRule for I001ImportDataSkew {
    fn id(&self) -> &str {
        "I001"
    }
    fn name(&self) -> &str {
        "Load Data Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        name.contains("SINK") && (name.contains("OLAP") || name.contains("TABLE"))
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_chunks = context.get_metric("__MAX_OF_PushChunkNum")?;
        let min_chunks = context.get_metric("__MIN_OF_PushChunkNum").unwrap_or(0.0);
        if min_chunks == 0.0 {
            return None;
        }
        let ratio = max_chunks / min_chunks;
        if ratio > 3.0 {
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
                message: format!("Load data skew detected; PushChunkNum max/min ratio: {:.2}", ratio),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Check whether upstream operators have data skew.".to_string(),
                    "Optimize the bucketing key selection.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// I002: Import RPC latency high
pub struct I002ImportRPCLatency;

impl DiagnosticRule for I002ImportRPCLatency {
    fn id(&self) -> &str {
        "I002"
    }
    fn name(&self) -> &str {
        "Load RPC Latency Is High"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        name.contains("SINK") && (name.contains("OLAP") || name.contains("TABLE"))
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let client_time = context.get_metric("RpcClientSideTime")?;
        let server_time = context.get_metric("RpcServerSideTime").unwrap_or(1.0);
        if server_time == 0.0 {
            return None;
        }
        let ratio = client_time / server_time;
        if ratio > 2.0 && client_time > 1_000_000_000.0 {
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
                    "Client-side load RPC time is {:.1}x the server-side time; network transfer may be the bottleneck.",
                    ratio
                ),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Enable data compression to reduce network transfer volume.".to_string(),
                    "Check network bandwidth and latency.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// I003: Import filtered rows too many
pub struct I003ImportFilteredRows;

impl DiagnosticRule for I003ImportFilteredRows {
    fn id(&self) -> &str {
        "I003"
    }
    fn name(&self) -> &str {
        "Too Many Rows Filtered During Load"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        name.contains("SINK") && (name.contains("OLAP") || name.contains("TABLE"))
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let filtered = context.get_metric("RowsFiltered").unwrap_or(0.0);
        let read = context
            .get_metric("RowsRead")
            .or_else(|| context.node.metrics.push_row_num.map(|v| v as f64))?;
        if read == 0.0 {
            return None;
        }
        let ratio = filtered / read;
        if ratio > 0.1 && filtered > 1000.0 {
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
                    "The load filtered {:.0} rows ({:.1}%), which may indicate data quality issues.",
                    filtered,
                    ratio * 100.0
                ),
                reason: "Refer to the official StarRocks documentation for more information.".to_string(),
                suggestions: vec![
                    "Check whether the data format matches the table schema.".to_string(),
                    "Check for null values or type mismatches.".to_string(),
                    "Check the BE logs for detailed filtering reasons.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(I001ImportDataSkew),
        Box::new(I002ImportRPCLatency),
        Box::new(I003ImportFilteredRows),
    ]
}
