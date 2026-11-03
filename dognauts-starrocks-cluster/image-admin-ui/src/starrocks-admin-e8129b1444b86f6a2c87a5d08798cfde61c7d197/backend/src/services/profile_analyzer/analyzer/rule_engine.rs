//! Rule Engine for Profile Diagnostics
//!
//! Orchestrates rule evaluation, deduplication, suggestion generation,
//! conclusion and performance score calculation.

use super::rules::{
    Diagnostic, DiagnosticRule, RuleContext, RuleSeverity, get_all_rules, get_query_rules,
};
use crate::services::profile_analyzer::models::*;
use std::collections::HashSet;

/// Rule engine configuration
#[derive(Debug, Clone)]
pub struct RuleEngineConfig {
    /// Maximum number of suggestions to return
    pub max_suggestions: usize,
    /// Whether to include parameter suggestions
    pub include_parameters: bool,
    /// Minimum severity to report
    pub min_severity: RuleSeverity,
}

impl Default for RuleEngineConfig {
    fn default() -> Self {
        Self {
            max_suggestions: 100, // Increased from 5 to avoid truncating important diagnostics
            include_parameters: true,
            min_severity: RuleSeverity::Info,
        }
    }
}

/// Rule engine for profile diagnostics
pub struct RuleEngine {
    config: RuleEngineConfig,
    rules: Vec<Box<dyn DiagnosticRule>>,
}

impl RuleEngine {
    /// Create a new rule engine with default configuration
    pub fn new() -> Self {
        Self { config: RuleEngineConfig::default(), rules: get_all_rules() }
    }

    /// Create with custom configuration (used in tests)
    #[cfg(test)]
    pub fn with_config(config: RuleEngineConfig) -> Self {
        Self { config, rules: get_all_rules() }
    }

    /// Analyze a profile and return diagnostics (for backward compatibility and tests)
    #[allow(dead_code)]
    pub fn analyze(&self, profile: &Profile) -> Vec<Diagnostic> {
        self.analyze_with_cluster_variables(profile, None)
    }

    /// Analyze a profile with optional live cluster variables
    /// cluster_variables: actual current values from the cluster (takes precedence)
    pub fn analyze_with_cluster_variables(
        &self,
        profile: &Profile,
        cluster_variables: Option<&std::collections::HashMap<String, String>>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Evaluate query-level rules first
        let query_ctx = super::rules::query::QueryRuleContext::with_cluster_variables(
            profile,
            cluster_variables,
        );
        for rule in get_query_rules() {
            if let Some(diag) = rule.evaluate(&query_ctx)
                && diag.severity >= self.config.min_severity
            {
                diagnostics.push(Diagnostic {
                    rule_id: diag.rule_id,
                    rule_name: diag.rule_name,
                    severity: diag.severity,
                    node_path: "Query".to_string(),
                    plan_node_id: None,
                    message: diag.message,
                    reason: diag.reason,
                    suggestions: diag.suggestions,
                    parameter_suggestions: if self.config.include_parameters {
                        diag.parameter_suggestions
                    } else {
                        vec![]
                    },
                });
            }
        }

        // Evaluate node-level rules
        if let Some(execution_tree) = &profile.execution_tree {
            // Get session variables for context
            let session_variables = &profile.summary.non_default_variables;
            // Get cluster info for smart recommendations
            let cluster_info = Some(profile.get_cluster_info());
            // Get default database for table name resolution
            let default_db = profile.summary.default_db.as_deref();

            for node in &execution_tree.nodes {
                let context = RuleContext {
                    node,
                    session_variables,
                    cluster_info: cluster_info.clone(),
                    cluster_variables,
                    default_db,
                };

                for rule in &self.rules {
                    if rule.applicable_to(node)
                        && let Some(mut diag) = rule.evaluate(&context)
                        && diag.severity >= self.config.min_severity
                    {
                        if !self.config.include_parameters {
                            diag.parameter_suggestions.clear();
                        }
                        diagnostics.push(diag);
                    }
                }
            }
        }

        // Sort by severity (highest first)
        diagnostics.sort_by(|a, b| b.severity.cmp(&a.severity));

        // Deduplicate similar diagnostics
        diagnostics = self.deduplicate(diagnostics);

        // Limit results
        if diagnostics.len() > self.config.max_suggestions {
            diagnostics.truncate(self.config.max_suggestions);
        }

        diagnostics
    }

    /// Deduplicate diagnostics by rule_id and node
    fn deduplicate(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for diag in diagnostics {
            let key = format!("{}:{}", diag.rule_id, diag.node_path);
            if seen.insert(key) {
                result.push(diag);
            }
        }

        result
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Conclusion and Score Generation (moved from SuggestionEngine)
// ============================================================================

impl RuleEngine {
    /// Generate a conclusion based on diagnostics and profile
    pub fn generate_conclusion(diagnostics: &[Diagnostic], profile: &Profile) -> String {
        if diagnostics.is_empty() {
            return "Query execution is healthy; no obvious performance issues were found.".to_string();
        }

        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == RuleSeverity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == RuleSeverity::Warning)
            .count();

        let total_time = Self::parse_total_time(&profile.summary.total_time).unwrap_or(0.0);

        if error_count > 0 {
            format!(
                "The query has {} critical performance issue(s) and a long execution time ({}). The main issue is {}. Address the critical issues first.",
                error_count,
                Self::format_duration(total_time),
                diagnostics
                    .first()
                    .map(|d| d.rule_name.as_str())
                    .unwrap_or("Unknown")
            )
        } else if warning_count > 2 {
            format!(
                "The query has {} moderate performance issue(s); overall performance could be improved. Execution time: {}.",
                warning_count,
                Self::format_duration(total_time)
            )
        } else if total_time > 300.0 {
            format!("Query execution is taking a long time ({}). Investigate performance hotspots.", Self::format_duration(total_time))
        } else {
            format!("The query has {} minor issue(s); overall performance is acceptable.", diagnostics.len())
        }
    }

    /// Generate aggregated suggestions from diagnostics
    pub fn generate_suggestions(diagnostics: &[Diagnostic]) -> Vec<String> {
        let mut suggestions = Vec::new();
        let mut unique_suggestions = HashSet::new();

        // Collect unique suggestions from diagnostics
        for diag in diagnostics {
            for suggestion in &diag.suggestions {
                if unique_suggestions.insert(suggestion.clone()) {
                    suggestions.push(suggestion.clone());
                }
            }
        }

        suggestions
    }

    /// Calculate performance score (0-100) based on diagnostics
    pub fn calculate_performance_score(diagnostics: &[Diagnostic], profile: &Profile) -> f64 {
        let mut score: f64 = 100.0;

        // Deduct points for diagnostics based on severity
        for diag in diagnostics {
            let penalty = match diag.severity {
                RuleSeverity::Error => 20.0,
                RuleSeverity::Warning => 10.0,
                RuleSeverity::Info => 3.0,
            };
            score -= penalty;
        }

        // Deduct points for long execution time
        if let Ok(total_seconds) = Self::parse_total_time(&profile.summary.total_time) {
            if total_seconds > 3600.0 {
                score -= 20.0;
            } else if total_seconds > 1800.0 {
                score -= 10.0;
            } else if total_seconds > 300.0 {
                score -= 5.0;
            }
        }

        score.max(0.0)
    }

    /// Parse total time string to seconds
    fn parse_total_time(time_str: &str) -> Result<f64, ()> {
        let s = time_str.trim();
        if s.is_empty() {
            return Err(());
        }

        let mut total_seconds = 0.0;
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
                    total_seconds += value * 3600.0;
                    found_unit = true;
                    i += 1;
                } else if c == 'm' {
                    if i + 1 < chars.len() && chars[i + 1] == 's' {
                        total_seconds += value / 1000.0;
                        i += 2;
                    } else {
                        total_seconds += value * 60.0;
                        i += 1;
                    }
                    found_unit = true;
                } else if c == 's' {
                    total_seconds += value;
                    found_unit = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }

        if found_unit { Ok(total_seconds) } else { Err(()) }
    }

    /// Format duration to human-readable string
    fn format_duration(seconds: f64) -> String {
        if seconds >= 3600.0 {
            format!("{:.1} hours", seconds / 3600.0)
        } else if seconds >= 60.0 {
            format!("{:.0} minutes", seconds / 60.0)
        } else {
            format!("{:.1} seconds", seconds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_engine_creation() {
        let engine = RuleEngine::new();
        assert!(!engine.rules.is_empty());
    }
}
