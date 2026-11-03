//! Scan operator diagnostic rules (S001-S014)
//!
//! Rules for OLAP_SCAN and CONNECTOR_SCAN operators.
//!
//! ## Rule List:
//! - S001: Data skew detection
//! - S002: IO skew detection
//! - S003: Poor filter effectiveness
//! - S004: Predicate not pushed down
//! - S005: IO thread pool saturation
//! - S006: Rowset fragmentation
//! - S007: Cold storage access (IO bound)
//! - S008: ZoneMap index not effective
//! - S009: Low cache hit rate (PageCache + DataCache for disaggregated storage)
//! - S010: Runtime Filter not effective
//! - S011: Accumulated soft deletes
//! - S012: Bitmap index not effective
//! - S013: Bloom filter index not effective
//! - S014: Colocate Join opportunity missed

use super::*;

/// S001: Data skew detection
/// Condition: max(RowsRead)/avg(RowsRead) > 2
pub struct S001DataSkew;

impl DiagnosticRule for S001DataSkew {
    fn id(&self) -> &str {
        "S001"
    }
    fn name(&self) -> &str {
        "Scan Data Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        // Check for max/min rows read metrics
        let max_rows = context
            .get_metric("__MAX_OF_RowsRead")
            .or_else(|| context.get_metric("RowsRead"))?;
        let min_rows = context.get_metric("__MIN_OF_RowsRead").unwrap_or(0.0);

        if min_rows == 0.0 {
            return None;
        }

        let avg_rows = (max_rows + min_rows) / 2.0;
        let ratio = max_rows / avg_rows;

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
                    "Scan data skew detected; max/avg ratio: {:.2}",
                    ratio
                ),
                reason: "StarRocks data is distributed unevenly across storage nodes, causing some nodes to scan more data and increasing query latency. This is usually caused by an unsuitable bucketing key.".to_string(),
                suggestions: vec![
                    "Check whether the bucketing keys are appropriate.".to_string(),
                    "Consider rebucketing to distribute data more evenly.".to_string(),
                    "Check for hot data.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S003: Poor filter effectiveness
/// Condition: RowsRead/RawRowsRead > 0.8 (less than 20% filtered)
pub struct S003PoorFilter;

impl DiagnosticRule for S003PoorFilter {
    fn id(&self) -> &str {
        "S003"
    }
    fn name(&self) -> &str {
        "Poor Filter Effectiveness"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let rows_read = context.get_metric("RowsRead")?;
        let raw_rows_read = context.get_metric("RawRowsRead")?;

        if raw_rows_read == 0.0 {
            return None;
        }

        let ratio = rows_read / raw_rows_read;

        // Only trigger if we're reading a significant amount of data
        if ratio > 0.8 && raw_rows_read > 100_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Poor filter effectiveness: only {:.1}% of data was filtered (read {:.0} rows / original {:.0} rows).",
                    (1.0 - ratio) * 100.0, rows_read, raw_rows_read
                ),
                reason: "Based on the scanned input and final output volumes, the Scan operator reads a large amount of data but does not significantly reduce the data passed downstream. StarRocks provides indexes, predicate pushdown, Runtime Filters, and other filtering methods.".to_string(),
                suggestions: vec![
                    "Add more selective filter conditions.".to_string(),
                    "Check whether predicates can be pushed down to the storage layer.".to_string(),
                    "Consider using partition pruning.".to_string(),
                    "Check whether the ZoneMap index is effective.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S007: Cold storage access (IO bound)
/// Condition: IOTime/ScanTime > 0.8 && BytesRead > 1GB
pub struct S007ColdStorage;

impl DiagnosticRule for S007ColdStorage {
    fn id(&self) -> &str {
        "S007"
    }
    fn name(&self) -> &str {
        "Cold Storage Access"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let io_time = context
            .get_metric("IOTime")
            .or_else(|| context.get_metric("IOTaskExecTime"))?;
        let scan_time = context
            .get_metric("ScanTime")
            .or_else(|| context.get_operator_time_ms())?;

        if scan_time == 0.0 {
            return None;
        }

        let bytes_read = context.get_metric("BytesRead").unwrap_or(0.0);
        let ratio = io_time / scan_time;

        const ONE_GB: f64 = 1024.0 * 1024.0 * 1024.0;

        if ratio > 0.8 && bytes_read > ONE_GB {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "I/O time accounts for {:.1}%, with {} of data read; a storage bottleneck may exist.",
                    ratio * 100.0, format_bytes(bytes_read as u64)
                ),
                reason: "Data is stored on cold storage (such as object storage), where I/O latency is higher. Cold storage typically has lower IOPS and throughput than local SSDs.".to_string(),
                suggestions: vec![
                    "Check storage performance and consider using SSDs.".to_string(),
                    "Increase the PageCache size.".to_string(),
                    "Check network bandwidth if remote storage is used.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("storage_page_cache_limit") {
                        suggestions.push(s);
                    }
                    if let Some(s) = context.suggest_parameter_smart("io_tasks_per_scan_operator") {
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

/// S009: Low cache hit rate (PageCache or DataCache)
/// Condition:
/// - PageCache: CachedPagesNum/ReadPagesNum < 0.3
/// - DataCache (disaggregated): CompressedBytesReadLocalDisk/(Local+Remote) < 0.7
pub struct S009LowCacheHit;

impl DiagnosticRule for S009LowCacheHit {
    fn id(&self) -> &str {
        "S009"
    }
    fn name(&self) -> &str {
        "Low Cache Hit Rate"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        // First, check DataCache metrics (for disaggregated storage-compute clusters)
        let bytes_local = context
            .get_metric("CompressedBytesReadLocalDisk")
            .unwrap_or(0.0);
        let bytes_remote = context
            .get_metric("CompressedBytesReadRemote")
            .unwrap_or(0.0);
        let total_bytes = bytes_local + bytes_remote;

        const MIN_BYTES: f64 = 10.0 * 1024.0 * 1024.0; // 10MB

        // DataCache scenario: remote bytes > 0 indicates disaggregated storage
        if bytes_remote > 0.0 && total_bytes > MIN_BYTES {
            let hit_rate = bytes_local / total_bytes;

            // Trigger if hit rate < 70%
            if hit_rate < 0.7 {
                let miss_rate = (1.0 - hit_rate) * 100.0;
                return Some(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    severity: if hit_rate < 0.3 { RuleSeverity::Error } else { RuleSeverity::Warning },
                    node_path: format!("{} (plan_node_id={})", 
                        context.node.operator_name,
                        context.node.plan_node_id.unwrap_or(-1)),
                    plan_node_id: context.node.plan_node_id,
                    message: format!(
                        "DataCache hit rate: {:.1}%; {:.1}% of data was read from remote storage (local: {}, remote: {}).",
                        hit_rate * 100.0,
                        miss_rate,
                        format_bytes(bytes_local as u64),
                        format_bytes(bytes_remote as u64)
                    ),
                    reason: "In shared-data architecture, DataCache is key to query performance. When large volumes of data must be read from remote storage (such as S3/OSS), network latency can significantly affect query performance.".to_string(),
                    suggestions: vec![
                        "Increase the DataCache capacity (datacache_disk_size).".to_string(),
                        "Check whether sufficient disk space is available for DataCache.".to_string(),
                        "Warm the cache for frequently accessed data using CACHE SELECT.".to_string(),
                        "Check whether other queries are competing for cache resources.".to_string(),
                    ],
                    // Only suggest parameters that are not already set to recommended values
                    parameter_suggestions: [
                        context.suggest_parameter(
                            "enable_scan_datacache",
                            "true",
                            "SET enable_scan_datacache = true;"
                        ),
                        context.suggest_parameter(
                            "enable_populate_datacache",
                            "true",
                            "SET enable_populate_datacache = true;"
                        ),
                    ].into_iter().flatten().collect(),
                });
            }
        }

        // Also check IO count metrics as alternative for DataCache
        let io_local = context.get_metric("IOCountLocalDisk").unwrap_or(0.0);
        let io_remote = context.get_metric("IOCountRemote").unwrap_or(0.0);
        let total_io = io_local + io_remote;

        if io_remote > 0.0 && total_io > 100.0 {
            let hit_rate = io_local / total_io;

            if hit_rate < 0.7 {
                return Some(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    severity: if hit_rate < 0.3 { RuleSeverity::Error } else { RuleSeverity::Warning },
                    node_path: format!("{} (plan_node_id={})", 
                        context.node.operator_name,
                        context.node.plan_node_id.unwrap_or(-1)),
                    plan_node_id: context.node.plan_node_id,
                    message: format!(
                        "DataCache I/O hit rate: {:.1}%; {:.1}% of I/O accesses use remote storage (local: {:.0}, remote: {:.0}).",
                        hit_rate * 100.0,
                        (1.0 - hit_rate) * 100.0,
                        io_local,
                        io_remote
                    ),
                    reason: "In shared-data architecture, DataCache is key to query performance. When many I/O requests access remote storage, network latency can significantly affect query performance.".to_string(),
                    suggestions: vec![
                        "Increase the DataCache capacity (datacache_disk_size).".to_string(),
                        "Warm the cache for frequently accessed data using CACHE SELECT.".to_string(),
                    ],
                    // Only suggest if not already enabled
                    parameter_suggestions: context.suggest_parameter(
                        "enable_scan_datacache",
                        "true",
                        "SET enable_scan_datacache = true;"
                    ).into_iter().collect(),
                });
            }
        }

        // Fallback: check PageCache metrics (for shared-nothing clusters)
        let cached_pages = context.get_metric("CachedPagesNum");
        let read_pages = context.get_metric("ReadPagesNum");

        if let (Some(cached), Some(total)) = (cached_pages, read_pages)
            && total > 1000.0
        {
            let hit_rate = cached / total;

            if hit_rate < 0.3 {
                return Some(Diagnostic {
                        rule_id: self.id().to_string(),
                        rule_name: self.name().to_string(),
                        severity: RuleSeverity::Info,
                        node_path: format!("{} (plan_node_id={})", 
                            context.node.operator_name,
                            context.node.plan_node_id.unwrap_or(-1)),
                        plan_node_id: context.node.plan_node_id,
                        message: format!(
                            "PageCache hit rate is only {:.1}% ({:.0}/{:.0} pages).",
                            hit_rate * 100.0, cached, total
                        ),
                        reason: "The PageCache hit rate is low, so a large amount of data must be read from disk. The cache may be too small, or the data access pattern may not be cache-friendly.".to_string(),
                        suggestions: vec![
                            "Increase PageCache capacity (storage_page_cache_limit).".to_string(),
                            "Check whether other queries are competing for the cache.".to_string(),
                        ],
                    parameter_suggestions: vec![],
                });
            }
        }

        None
    }
}

/// S010: Runtime Filter not effective on Scan
/// Condition: RuntimeFilterRows == 0 && RawRowsRead > 100k
pub struct S010RFNotEffective;

impl DiagnosticRule for S010RFNotEffective {
    fn id(&self) -> &str {
        "S010"
    }
    fn name(&self) -> &str {
        "Scan Runtime Filter Is Ineffective"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let rf_rows = context.get_metric("RuntimeFilterRows").unwrap_or(0.0);
        let raw_rows = context.get_metric("RawRowsRead").unwrap_or(0.0);

        if rf_rows == 0.0 && raw_rows > 100_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "The Runtime Filter did not filter any rows; {:.0} rows were scanned.",
                    raw_rows
                ),
                reason: "The Runtime Filter did not filter data effectively. It may have failed to build, timed out, or had poor selectivity.".to_string(),
                suggestions: vec![
                    "Check whether the Runtime Filter was generated.".to_string(),
                    "Check whether the Join condition is suitable for generating a Runtime Filter.".to_string(),
                    "Confirm that enable_global_runtime_filter is enabled.".to_string(),
                ],
                parameter_suggestions: {
                    let mut suggestions = Vec::new();
                    if let Some(s) = context.suggest_parameter_smart("enable_global_runtime_filter") {
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

/// S011: Accumulated soft deletes
/// Condition: DelVecFilterRows/RawRowsRead > 0.3
pub struct S011SoftDeletes;

impl DiagnosticRule for S011SoftDeletes {
    fn id(&self) -> &str {
        "S011"
    }
    fn name(&self) -> &str {
        "Too Many Accumulated Soft Deletes"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        // Only applicable to OLAP_SCAN (internal tables), not CONNECTOR_SCAN (external tables)
        node.operator_name.to_uppercase().contains("OLAP_SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let del_vec_rows = context.get_metric("DelVecFilterRows")?;
        let raw_rows = context.get_metric("RawRowsRead")?;

        if raw_rows == 0.0 {
            return None;
        }

        let ratio = del_vec_rows / raw_rows;

        if ratio > 0.3 {
            // Extract table name from unique_metrics
            let table_name = context
                .node
                .unique_metrics
                .get("Table")
                .map(|s| s.as_str())
                .unwrap_or("unknown_table");

            // Build full table name with database
            // If table already has db prefix (db.table), use it directly
            // Otherwise, use default_db from profile summary
            let full_table_name = if table_name.contains('.') {
                table_name.to_string()
            } else if let Some(db) = context.default_db {
                if db.is_empty() {
                    table_name.to_string()
                } else {
                    format!("{}.{}", db, table_name)
                }
            } else {
                table_name.to_string()
            };

            // Generate Compaction command (correct syntax: ALTER TABLE <table> COMPACT)
            let compaction_cmd = format!("ALTER TABLE {} COMPACT;", full_table_name);

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
                    "Table {} has {:.1}% soft-deleted rows; consider running Compaction.",
                    full_table_name,
                    ratio * 100.0
                ),
                reason: format!(
                    "Table {} contains many soft-deleted records ({:.0} rows). These rows must be filtered during scans, reducing query performance. Run Compaction to clean up the delete markers.",
                    full_table_name, del_vec_rows
                ),
                suggestions: vec![
                    format!("Run Compaction: {}", compaction_cmd),
                    "Check Compaction status: SHOW PROC '/compactions';".to_string(),
                    format!("View table Tablet status: SHOW TABLET FROM {};", full_table_name),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S002: IO skew detection
pub struct S002IOSkew;

impl DiagnosticRule for S002IOSkew {
    fn id(&self) -> &str {
        "S002"
    }
    fn name(&self) -> &str {
        "Scan I/O Skew"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let max_io = context.get_metric("__MAX_OF_IOTime")?;
        let min_io = context.get_metric("__MIN_OF_IOTime").unwrap_or(0.0);
        if min_io == 0.0 {
            return None;
        }
        let ratio = max_io / ((max_io + min_io) / 2.0);
        if ratio > 2.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("Scan I/O time is skewed; max/avg ratio: {:.2}", ratio),
                reason: "When multiple Scan instances read data, some take significantly longer than others. This may be caused by uneven node I/O utilization or uneven data distribution across nodes.".to_string(),
                suggestions: vec!["Check whether node I/O utilization is uneven.".to_string(), "Check storage devices for performance issues.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S004: Predicate not pushed down
pub struct S004PredicateNotPushed;

impl DiagnosticRule for S004PredicateNotPushed {
    fn id(&self) -> &str {
        "S004"
    }
    fn name(&self) -> &str {
        "Predicate Not Pushed Down"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let pushdown = context.get_metric("PushdownPredicates").unwrap_or(0.0);
        let pred_filter = context.get_metric("PredFilterRows").unwrap_or(0.0);
        let raw_rows = context.get_metric("RawRowsRead").unwrap_or(0.0);
        if pushdown == 0.0 && raw_rows > 10000.0 && pred_filter / raw_rows > 0.1 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("Predicates were not pushed down to the storage layer; {:.0} rows ({:.1}%) were filtered at the expression layer.", pred_filter, pred_filter / raw_rows * 100.0),
                reason: "The query condition could not be pushed down to the storage layer, so a large amount of data must be filtered in the compute layer. Possible causes include functions in the condition, type mismatches, or unsupported expressions.".to_string(),
                suggestions: vec!["Rewrite predicates as simple comparisons.".to_string(), "Add a ZoneMap/Bloom index.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S005: IO thread pool saturation
pub struct S005IOThreadPoolSaturation;

impl DiagnosticRule for S005IOThreadPoolSaturation {
    fn id(&self) -> &str {
        "S005"
    }
    fn name(&self) -> &str {
        "I/O Thread Pool Saturation"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let wait_time = context.get_metric("IOTaskWaitTime").unwrap_or(0.0);
        let peak_tasks = context.get_metric("PeakIOTasks").unwrap_or(100.0);
        if wait_time > 1_000_000_000.0 && peak_tasks < 10.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("The I/O thread pool may be saturated; wait time: {:.1}s.", wait_time / 1_000_000_000.0),
                reason: "High I/O thread pool utilization is causing long I/O task waits. Possible causes include too many concurrent queries or insufficient storage performance.".to_string(),
                suggestions: vec!["Increase the max_io_threads setting on the BE.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S006: Rowset fragmentation
pub struct S006RowsetFragmentation;

impl DiagnosticRule for S006RowsetFragmentation {
    fn id(&self) -> &str {
        "S006"
    }
    fn name(&self) -> &str {
        "Rowset Fragmentation"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let rowsets = context.get_metric("RowsetsReadCount").unwrap_or(0.0);
        let init_time = context.get_metric("SegmentInitTime").unwrap_or(0.0);
        if rowsets > 100.0 && init_time > 500_000_000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Warning,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!("Too many Rowsets ({:.0}); initialization took {:.1} ms.", rowsets, init_time / 1_000_000.0),
                reason: "Too many Rowsets are causing long Segment initialization times. This is usually caused by frequent small-batch loads or delayed Compaction.".to_string(),
                suggestions: vec!["Trigger manual Compaction.".to_string(), "Combine small load jobs into larger batches.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S008: ZoneMap index not effective
pub struct S008ZoneMapNotEffective;

impl DiagnosticRule for S008ZoneMapNotEffective {
    fn id(&self) -> &str {
        "S008"
    }
    fn name(&self) -> &str {
        "ZoneMap Index Is Ineffective"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let zonemap_rows = context.get_metric("ZoneMapIndexFilterRows").unwrap_or(0.0);
        let raw_rows = context.get_metric("RawRowsRead").unwrap_or(0.0);
        if zonemap_rows == 0.0 && raw_rows > 100000.0 {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!("{} (plan_node_id={})", context.node.operator_name, context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: "The ZoneMap index did not filter any data.".to_string(),
                reason: "The ZoneMap index did not filter data effectively. ZoneMap filtering uses min/max values of sort keys, so query conditions must include sort keys or prefix columns.".to_string(),
                suggestions: vec!["Make sure query conditions include sort keys or prefix columns.".to_string()],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// S012: Bitmap index not effective
/// Condition: BitmapIndexFilterRows = 0 with low cardinality column filter
///
/// Reason: Bitmap索引适用于基数较低且大量重复的字段（如性别、状态）。
/// 如果查询条件包含这类字段但未命中Bitmap索引，可能是：
/// 1. 未创建Bitmap索引
/// 2. 查询条件不支持Bitmap索引（如范围查询）
/// 3. 优化器选择了其他索引
pub struct S012BitmapIndexNotEffective;

impl DiagnosticRule for S012BitmapIndexNotEffective {
    fn id(&self) -> &str {
        "S012"
    }
    fn name(&self) -> &str {
        "Bitmap Index Is Ineffective"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let bitmap_rows = context.get_metric("BitmapIndexFilterRows").unwrap_or(0.0);
        let raw_rows = context.get_metric("RawRowsRead").unwrap_or(0.0);

        // Only trigger if scanning significant data and bitmap filter is 0
        if bitmap_rows == 0.0 && raw_rows > 100_000.0 {
            // Check if there's expression filter (suggesting there are filter conditions)
            let expr_filter = context.get_metric("ExprFilterRows").unwrap_or(0.0);
            if expr_filter > raw_rows * 0.1 {
                return Some(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    severity: RuleSeverity::Info,
                    node_path: format!("{} (plan_node_id={})", 
                        context.node.operator_name,
                        context.node.plan_node_id.unwrap_or(-1)),
                    plan_node_id: context.node.plan_node_id,
                    message: format!(
                        "The Bitmap index did not filter data; the expression filtered {:.0} rows.",
                        expr_filter
                    ),
                    reason: "Bitmap indexes are suitable for low-cardinality columns with many repeated values (such as gender or status). If a query filters such a column but does not use the index, the index may not exist or the query condition may not be supported.".to_string(),
                suggestions: vec![
                        "Create a Bitmap index on low-cardinality columns (such as status or type).".to_string(),
                        "Make sure the query predicate uses equality matching (=, IN).".to_string(),
                        "Check the BitmapIndexFilterRows metric in the Profile.".to_string(),
                    ],
                    parameter_suggestions: vec![],
                });
            }
        }
        None
    }
}

/// S013: Bloom filter index not effective
/// Condition: BloomFilterFilterRows = 0 with high cardinality column filter
///
/// Reason: Bloom Filter索引适用于高基数列（如ID列）的等值查询。
/// 如果查询条件包含这类字段但未命中Bloom Filter索引，可能是：
/// 1. 未创建Bloom Filter索引
/// 2. 查询条件不是等值匹配（Bloom Filter仅支持 = 和 IN）
/// 3. 列类型不支持（TINYINT, FLOAT, DOUBLE, DECIMAL不支持）
pub struct S013BloomFilterNotEffective;

impl DiagnosticRule for S013BloomFilterNotEffective {
    fn id(&self) -> &str {
        "S013"
    }
    fn name(&self) -> &str {
        "Bloom Filter Index Is Ineffective"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        node.operator_name.to_uppercase().contains("SCAN")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        let bloom_rows = context.get_metric("BloomFilterFilterRows").unwrap_or(0.0);
        let raw_rows = context.get_metric("RawRowsRead").unwrap_or(0.0);

        // Only trigger if scanning significant data and bloom filter is 0
        if bloom_rows == 0.0 && raw_rows > 100_000.0 {
            // Check if there's expression filter on potential ID columns
            let expr_filter = context.get_metric("ExprFilterRows").unwrap_or(0.0);
            if expr_filter > raw_rows * 0.5 {
                return Some(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    severity: RuleSeverity::Info,
                    node_path: format!("{} (plan_node_id={})", 
                        context.node.operator_name,
                        context.node.plan_node_id.unwrap_or(-1)),
                    plan_node_id: context.node.plan_node_id,
                    message: format!(
                        "The Bloom Filter index did not filter data; the expression filtered {:.0} rows.",
                        expr_filter
                    ),
                    reason: "Bloom Filter indexes are suitable for equality queries on high-cardinality columns (such as ID columns). They support only = and IN conditions, and do not support TINYINT/FLOAT/DOUBLE/DECIMAL types.".to_string(),
                suggestions: vec![
                        "Create a Bloom Filter index on high-cardinality columns (such as ID columns).".to_string(),
                        "Make sure the query predicate uses equality matching (=, IN).".to_string(),
                        "Note: Bloom Filters do not support TINYINT/FLOAT/DOUBLE/DECIMAL types.".to_string(),
                        "Check the BloomFilterFilterRows metric in the Profile.".to_string(),
                    ],
                    parameter_suggestions: vec![],
                });
            }
        }
        None
    }
}

/// S014: Colocate Join opportunity missed
/// Condition: Shuffle Join on tables that could be colocated
///
/// Reason: Colocate Join可以避免数据网络传输，显著提升Join性能。
/// 当两个表的分桶键相同且分桶数相同时，可以使用Colocate Join。
pub struct S014ColocateJoinOpportunity;

impl DiagnosticRule for S014ColocateJoinOpportunity {
    fn id(&self) -> &str {
        "S014"
    }
    fn name(&self) -> &str {
        "Can Be Optimized with Colocate Join"
    }

    fn applicable_to(&self, node: &ExecutionTreeNode) -> bool {
        let name = node.operator_name.to_uppercase();
        name.contains("HASH") && name.contains("JOIN") && name.contains("SHUFFLE")
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Diagnostic> {
        // Check if this is a shuffle join with significant network transfer
        let bytes_sent = context
            .get_metric("BytesSent")
            .or_else(|| context.get_metric("NetworkBytesSent"))
            .unwrap_or(0.0);

        const HUNDRED_MB: f64 = 100.0 * 1024.0 * 1024.0;

        if bytes_sent > HUNDRED_MB {
            Some(Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: RuleSeverity::Info,
                node_path: format!("{} (plan_node_id={})", 
                    context.node.operator_name,
                    context.node.plan_node_id.unwrap_or(-1)),
                plan_node_id: context.node.plan_node_id,
                message: format!(
                    "Shuffle Join transfers {} over the network; consider optimizing with Colocate Join.",
                    format_bytes(bytes_sent as u64)
                ),
                reason: "Colocate Join avoids network data transfer and can significantly improve Join performance. It can be used when both tables have the same bucketing key and bucket count.".to_string(),
                suggestions: vec![
                    "Place frequently joined tables in the same Colocation Group.".to_string(),
                    "Make sure both tables use the same bucketing key and bucket count.".to_string(),
                    "Use SHOW COLOCATION GROUP to view existing groups.".to_string(),
                    "Colocate Join avoids network data transfer and can significantly improve performance.".to_string(),
                ],
                parameter_suggestions: vec![],
            })
        } else {
            None
        }
    }
}

/// Get all scan rules
pub fn get_rules() -> Vec<Box<dyn DiagnosticRule>> {
    vec![
        Box::new(S001DataSkew),
        Box::new(S002IOSkew),
        Box::new(S003PoorFilter),
        Box::new(S004PredicateNotPushed),
        Box::new(S005IOThreadPoolSaturation),
        Box::new(S006RowsetFragmentation),
        Box::new(S007ColdStorage),
        Box::new(S008ZoneMapNotEffective),
        Box::new(S009LowCacheHit),
        Box::new(S010RFNotEffective),
        Box::new(S011SoftDeletes),
        Box::new(S012BitmapIndexNotEffective),
        Box::new(S013BloomFilterNotEffective),
        Box::new(S014ColocateJoinOpportunity),
    ]
}
