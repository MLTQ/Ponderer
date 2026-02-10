use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use super::candidate_backends::{EpisodicMemoryBackendV3, FtsMemoryBackendV2};
use super::{KvMemoryBackend, MemoryBackend, MemoryDesignVersion, WorkingMemoryEntry};

const DEFAULT_QUERY_TOP_K: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvalTraceSet {
    pub name: String,
    pub traces: Vec<MemoryEvalTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvalTrace {
    pub id: String,
    pub steps: Vec<MemoryEvalStep>,
    pub checks: Vec<MemoryEvalCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryEvalStep {
    Write { key: String, content: String },
    Delete { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryEvalCheck {
    Get {
        key: String,
        expect_contains: Option<String>,
    },
    Query {
        query: String,
        expected_keys: Vec<String>,
        top_k: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalBackendKind {
    KvV1,
    FtsV2,
    EpisodicV3,
    NullV0,
}

impl EvalBackendKind {
    pub fn id(&self) -> &'static str {
        match self {
            EvalBackendKind::KvV1 => "kv_v1",
            EvalBackendKind::FtsV2 => "fts_v2",
            EvalBackendKind::EpisodicV3 => "episodic_v3",
            EvalBackendKind::NullV0 => "null_v0",
        }
    }

    pub fn build_backend(&self) -> Box<dyn MemoryBackend> {
        match self {
            EvalBackendKind::KvV1 => Box::new(KvMemoryBackend::new()),
            EvalBackendKind::FtsV2 => Box::new(FtsMemoryBackendV2::new()),
            EvalBackendKind::EpisodicV3 => Box::new(EpisodicMemoryBackendV3::new()),
            EvalBackendKind::NullV0 => Box::new(NullMemoryBackend),
        }
    }

    pub fn design_version(&self) -> MemoryDesignVersion {
        match self {
            EvalBackendKind::KvV1 => MemoryDesignVersion::kv_v1(),
            EvalBackendKind::FtsV2 => MemoryDesignVersion {
                design_id: "fts_v2".to_string(),
                schema_version: 2,
            },
            EvalBackendKind::EpisodicV3 => MemoryDesignVersion {
                design_id: "episodic_v3".to_string(),
                schema_version: 3,
            },
            EvalBackendKind::NullV0 => MemoryDesignVersion {
                design_id: "null_v0".to_string(),
                schema_version: 0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvalReport {
    pub trace_set_name: String,
    pub generated_at: DateTime<Utc>,
    pub candidates: Vec<MemoryEvalCandidateReport>,
    pub winner: Option<String>,
}

impl MemoryEvalReport {
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize memory eval report")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvalCandidateReport {
    pub backend_id: String,
    pub design_version: MemoryDesignVersion,
    pub metrics: MemoryEvalMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvalMetrics {
    pub traces: usize,
    pub steps_total: usize,
    pub checks_total: usize,
    pub get_checks: usize,
    pub get_passed: usize,
    pub query_checks: usize,
    pub recall_at_1: f64,
    pub recall_at_k: f64,
    pub mean_step_ms: f64,
    pub p95_step_ms: f64,
    pub mean_check_ms: f64,
    pub p95_check_ms: f64,
    pub final_entries: usize,
    pub final_bytes_estimate: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryShadowComparison {
    pub baseline_backend_id: String,
    pub candidate_backend_id: String,
    pub baseline_design_version: MemoryDesignVersion,
    pub candidate_design_version: MemoryDesignVersion,
    pub baseline_metrics: MemoryEvalMetrics,
    pub candidate_metrics: MemoryEvalMetrics,
    pub baseline_get_pass_rate: f64,
    pub candidate_get_pass_rate: f64,
    pub get_pass_rate_delta: f64,
    pub recall_at_k_delta: f64,
    pub recall_at_1_delta: f64,
    pub mean_check_latency_ratio: f64,
    pub baseline_safety_non_regression: bool,
    pub report: MemoryEvalReport,
}

pub fn default_replay_trace_set() -> MemoryEvalTraceSet {
    MemoryEvalTraceSet {
        name: "default_replay".to_string(),
        traces: vec![
            MemoryEvalTrace {
                id: "trace-1".to_string(),
                steps: vec![
                    MemoryEvalStep::Write {
                        key: "release".to_string(),
                        content: "ship memory backend and migration registry".to_string(),
                    },
                    MemoryEvalStep::Write {
                        key: "ops".to_string(),
                        content: "monitor heartbeat and collect metrics".to_string(),
                    },
                ],
                checks: vec![
                    MemoryEvalCheck::Get {
                        key: "release".to_string(),
                        expect_contains: Some("migration".to_string()),
                    },
                    MemoryEvalCheck::Query {
                        query: "memory migration".to_string(),
                        expected_keys: vec!["release".to_string()],
                        top_k: Some(3),
                    },
                ],
            },
            MemoryEvalTrace {
                id: "trace-2".to_string(),
                steps: vec![
                    MemoryEvalStep::Write {
                        key: "meeting".to_string(),
                        content: "discuss persona trajectory and memory scoring".to_string(),
                    },
                    MemoryEvalStep::Delete {
                        key: "ops".to_string(),
                    },
                ],
                checks: vec![MemoryEvalCheck::Query {
                    query: "trajectory scoring".to_string(),
                    expected_keys: vec!["meeting".to_string()],
                    top_k: Some(3),
                }],
            },
        ],
    }
}

pub fn load_trace_set(path: &Path) -> Result<MemoryEvalTraceSet> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read trace set from {}", path.display()))?;
    serde_json::from_str::<MemoryEvalTraceSet>(&raw)
        .with_context(|| format!("Failed to parse trace set JSON {}", path.display()))
}

pub fn write_report_json(report: &MemoryEvalReport, path: &Path) -> Result<()> {
    let json = report.to_json_pretty()?;
    std::fs::write(path, json)
        .with_context(|| format!("Failed to write memory eval report to {}", path.display()))
}

pub fn evaluate_trace_set(
    traces: &MemoryEvalTraceSet,
    candidates: &[EvalBackendKind],
) -> Result<MemoryEvalReport> {
    if candidates.is_empty() {
        anyhow::bail!("No memory eval backend candidates provided");
    }

    let mut reports = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        reports.push(evaluate_candidate(traces, candidate)?);
    }

    let winner = reports
        .iter()
        .max_by(|a, b| {
            let a_key = (
                ordered_f64(a.metrics.recall_at_k),
                ordered_f64(a.metrics.recall_at_1),
                ordered_f64(pass_rate(a.metrics.get_passed, a.metrics.get_checks)),
                std::cmp::Reverse(ordered_f64(a.metrics.mean_check_ms)),
            );
            let b_key = (
                ordered_f64(b.metrics.recall_at_k),
                ordered_f64(b.metrics.recall_at_1),
                ordered_f64(pass_rate(b.metrics.get_passed, b.metrics.get_checks)),
                std::cmp::Reverse(ordered_f64(b.metrics.mean_check_ms)),
            );
            a_key.cmp(&b_key)
        })
        .map(|r| r.backend_id.clone());

    Ok(MemoryEvalReport {
        trace_set_name: traces.name.clone(),
        generated_at: Utc::now(),
        candidates: reports,
        winner,
    })
}

pub fn evaluate_shadow_against_kv(
    traces: &MemoryEvalTraceSet,
    candidate: EvalBackendKind,
) -> Result<MemoryShadowComparison> {
    if candidate == EvalBackendKind::KvV1 {
        anyhow::bail!("Candidate backend for shadow comparison must differ from kv_v1");
    }

    let baseline_kind = EvalBackendKind::KvV1;
    let baseline_backend_id = baseline_kind.id().to_string();
    let candidate_backend_id = candidate.id().to_string();
    let report = evaluate_trace_set(traces, &[baseline_kind, candidate])?;

    let baseline = report
        .candidates
        .iter()
        .find(|c| c.backend_id == baseline_backend_id)
        .with_context(|| {
            format!(
                "Baseline backend '{}' missing from eval report",
                baseline_kind.id()
            )
        })?;
    let candidate_report = report
        .candidates
        .iter()
        .find(|c| c.backend_id == candidate_backend_id)
        .with_context(|| {
            format!(
                "Candidate backend '{}' missing from eval report",
                candidate.id()
            )
        })?;

    let baseline_get_pass_rate = pass_rate(
        baseline.metrics.get_passed,
        baseline.metrics.get_checks.max(1),
    );
    let candidate_get_pass_rate = pass_rate(
        candidate_report.metrics.get_passed,
        candidate_report.metrics.get_checks.max(1),
    );
    let mean_check_latency_ratio = if baseline.metrics.mean_check_ms <= 0.000_001 {
        if candidate_report.metrics.mean_check_ms <= 0.000_001 {
            1.0
        } else {
            f64::INFINITY
        }
    } else {
        candidate_report.metrics.mean_check_ms / baseline.metrics.mean_check_ms
    };

    Ok(MemoryShadowComparison {
        baseline_backend_id: baseline_backend_id.clone(),
        candidate_backend_id,
        baseline_design_version: baseline.design_version.clone(),
        candidate_design_version: candidate_report.design_version.clone(),
        baseline_metrics: baseline.metrics.clone(),
        candidate_metrics: candidate_report.metrics.clone(),
        baseline_get_pass_rate,
        candidate_get_pass_rate,
        get_pass_rate_delta: candidate_get_pass_rate - baseline_get_pass_rate,
        recall_at_k_delta: candidate_report.metrics.recall_at_k - baseline.metrics.recall_at_k,
        recall_at_1_delta: candidate_report.metrics.recall_at_1 - baseline.metrics.recall_at_1,
        mean_check_latency_ratio,
        baseline_safety_non_regression: candidate_get_pass_rate >= baseline_get_pass_rate,
        report,
    })
}

fn evaluate_candidate(
    traces: &MemoryEvalTraceSet,
    candidate: &EvalBackendKind,
) -> Result<MemoryEvalCandidateReport> {
    let backend = candidate.build_backend();
    let design_version = candidate.design_version();

    let mut steps_total = 0usize;
    let mut checks_total = 0usize;
    let mut get_checks = 0usize;
    let mut get_passed = 0usize;
    let mut query_checks = 0usize;
    let mut recall_at_1_sum = 0.0f64;
    let mut recall_at_k_sum = 0.0f64;
    let mut step_durations_ms: Vec<f64> = Vec::new();
    let mut check_durations_ms: Vec<f64> = Vec::new();
    let mut final_entries = 0usize;
    let mut final_bytes_estimate = 0usize;

    for trace in &traces.traces {
        let conn = Connection::open_in_memory()
            .with_context(|| format!("Failed to open in-memory DB for trace {}", trace.id))?;
        ensure_working_memory_table(&conn)?;

        for step in &trace.steps {
            let step_start = Instant::now();
            apply_step(backend.as_ref(), &conn, step)?;
            step_durations_ms.push(step_start.elapsed().as_secs_f64() * 1000.0);
            steps_total += 1;
        }

        for check in &trace.checks {
            let check_start = Instant::now();
            match check {
                MemoryEvalCheck::Get {
                    key,
                    expect_contains,
                } => {
                    get_checks += 1;
                    if evaluate_get_check(backend.as_ref(), &conn, key, expect_contains)? {
                        get_passed += 1;
                    }
                }
                MemoryEvalCheck::Query {
                    query,
                    expected_keys,
                    top_k,
                } => {
                    query_checks += 1;
                    let (r1, rk) = evaluate_query_check(
                        backend.as_ref(),
                        &conn,
                        query,
                        expected_keys,
                        top_k.unwrap_or(DEFAULT_QUERY_TOP_K),
                    )?;
                    recall_at_1_sum += r1;
                    recall_at_k_sum += rk;
                }
            }
            check_durations_ms.push(check_start.elapsed().as_secs_f64() * 1000.0);
            checks_total += 1;
        }

        let entries = backend.list_entries(&conn)?;
        final_entries += entries.len();
        final_bytes_estimate += entries
            .iter()
            .map(|e| e.key.len() + e.content.len())
            .sum::<usize>();
    }

    let query_count_f = query_checks.max(1) as f64;
    let metrics = MemoryEvalMetrics {
        traces: traces.traces.len(),
        steps_total,
        checks_total,
        get_checks,
        get_passed,
        query_checks,
        recall_at_1: recall_at_1_sum / query_count_f,
        recall_at_k: recall_at_k_sum / query_count_f,
        mean_step_ms: mean(&step_durations_ms),
        p95_step_ms: p95(&step_durations_ms),
        mean_check_ms: mean(&check_durations_ms),
        p95_check_ms: p95(&check_durations_ms),
        final_entries,
        final_bytes_estimate,
    };

    Ok(MemoryEvalCandidateReport {
        backend_id: candidate.id().to_string(),
        design_version,
        metrics,
    })
}

fn ensure_working_memory_table(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"CREATE TABLE IF NOT EXISTS working_memory (
            key TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"#,
        [],
    )
    .context("Failed to create working_memory table for evaluation")?;
    Ok(())
}

fn apply_step(backend: &dyn MemoryBackend, conn: &Connection, step: &MemoryEvalStep) -> Result<()> {
    match step {
        MemoryEvalStep::Write { key, content } => backend.set_entry(conn, key, content),
        MemoryEvalStep::Delete { key } => backend.delete_entry(conn, key),
    }
}

fn evaluate_get_check(
    backend: &dyn MemoryBackend,
    conn: &Connection,
    key: &str,
    expect_contains: &Option<String>,
) -> Result<bool> {
    let entry = backend.get_entry(conn, key)?;
    let passed = match (entry, expect_contains) {
        (Some(entry), Some(snippet)) => entry
            .content
            .to_lowercase()
            .contains(&snippet.to_lowercase()),
        (Some(_), None) => true,
        (None, _) => false,
    };
    Ok(passed)
}

fn evaluate_query_check(
    backend: &dyn MemoryBackend,
    conn: &Connection,
    query: &str,
    expected_keys: &[String],
    top_k: usize,
) -> Result<(f64, f64)> {
    if expected_keys.is_empty() {
        return Ok((1.0, 1.0));
    }

    let entries = backend.list_entries(conn)?;
    let ranked = rank_entries(query, &entries, top_k.max(1));
    let expected: HashSet<&str> = expected_keys.iter().map(|s| s.as_str()).collect();

    let top1_hit = ranked
        .first()
        .map(|entry| expected.contains(entry.key.as_str()))
        .unwrap_or(false);

    let hits = ranked
        .iter()
        .filter(|entry| expected.contains(entry.key.as_str()))
        .count();
    let recall_k = hits as f64 / expected.len() as f64;

    Ok((if top1_hit { 1.0 } else { 0.0 }, recall_k))
}

fn rank_entries<'a>(
    query: &str,
    entries: &'a [WorkingMemoryEntry],
    top_k: usize,
) -> Vec<&'a WorkingMemoryEntry> {
    let query_tokens = tokenize(query);

    let mut scored: Vec<(&WorkingMemoryEntry, usize)> = entries
        .iter()
        .map(|entry| {
            let mut score = 0usize;
            let key_tokens = tokenize(&entry.key);
            let content_tokens = tokenize(&entry.content);
            for token in &query_tokens {
                if key_tokens.contains(token) {
                    score += 3;
                }
                if content_tokens.contains(token) {
                    score += 1;
                }
            }
            (entry, score)
        })
        .collect();

    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.key.cmp(&b.0.key))
            .then_with(|| a.0.updated_at.cmp(&b.0.updated_at))
    });

    scored
        .into_iter()
        .take(top_k)
        .map(|(entry, _)| entry)
        .collect()
}

fn tokenize(input: &str) -> HashSet<String> {
    input
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn p95(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by_key(|v| ordered_f64(*v));
    let idx = (((sorted.len() as f64) * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[idx]
}

fn ordered_f64(v: f64) -> i64 {
    (v * 1_000_000.0) as i64
}

fn pass_rate(passed: usize, total: usize) -> f64 {
    passed as f64 / total.max(1) as f64
}

struct NullMemoryBackend;

impl MemoryBackend for NullMemoryBackend {
    fn design_version(&self) -> MemoryDesignVersion {
        MemoryDesignVersion {
            design_id: "null_v0".to_string(),
            schema_version: 0,
        }
    }

    fn set_entry(&self, _conn: &Connection, _key: &str, _content: &str) -> Result<()> {
        Ok(())
    }

    fn get_entry(&self, _conn: &Connection, _key: &str) -> Result<Option<WorkingMemoryEntry>> {
        Ok(None)
    }

    fn list_entries(&self, _conn: &Connection) -> Result<Vec<WorkingMemoryEntry>> {
        Ok(Vec::new())
    }

    fn delete_entry(&self, _conn: &Connection, _key: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn sample_trace_set() -> MemoryEvalTraceSet {
        default_replay_trace_set()
    }

    #[test]
    fn deterministic_results_for_same_input() {
        let traces = sample_trace_set();
        let candidates = [EvalBackendKind::KvV1, EvalBackendKind::NullV0];

        let first = evaluate_trace_set(&traces, &candidates).unwrap();
        let second = evaluate_trace_set(&traces, &candidates).unwrap();

        assert_eq!(first.winner, second.winner);
        assert_eq!(
            first
                .candidates
                .iter()
                .map(|c| (c.backend_id.clone(), c.metrics.recall_at_k))
                .collect::<Vec<_>>(),
            second
                .candidates
                .iter()
                .map(|c| (c.backend_id.clone(), c.metrics.recall_at_k))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn compares_two_backends_and_picks_winner() {
        let traces = sample_trace_set();
        let report =
            evaluate_trace_set(&traces, &[EvalBackendKind::KvV1, EvalBackendKind::NullV0]).unwrap();

        assert_eq!(report.candidates.len(), 2);
        assert_eq!(report.winner.as_deref(), Some("kv_v1"));

        let kv = report
            .candidates
            .iter()
            .find(|c| c.backend_id == "kv_v1")
            .unwrap();
        let null = report
            .candidates
            .iter()
            .find(|c| c.backend_id == "null_v0")
            .unwrap();

        assert!(kv.metrics.recall_at_k > null.metrics.recall_at_k);
        assert!(kv.metrics.get_passed > null.metrics.get_passed);
    }

    #[test]
    fn report_is_machine_readable_json_and_roundtrips() {
        let traces = sample_trace_set();
        let report =
            evaluate_trace_set(&traces, &[EvalBackendKind::KvV1, EvalBackendKind::NullV0]).unwrap();
        let json = report.to_json_pretty().unwrap();
        let reparsed: MemoryEvalReport = serde_json::from_str(&json).unwrap();
        assert_eq!(reparsed.trace_set_name, "default_replay");
        assert_eq!(reparsed.candidates.len(), 2);
    }

    #[test]
    fn can_load_traces_and_write_report_to_disk() {
        let traces = sample_trace_set();
        let tmp_traces = NamedTempFile::new().unwrap();
        let traces_json = serde_json::to_string_pretty(&traces).unwrap();
        std::fs::write(tmp_traces.path(), traces_json).unwrap();

        let loaded = load_trace_set(tmp_traces.path()).unwrap();
        assert_eq!(loaded.traces.len(), 2);

        let report =
            evaluate_trace_set(&loaded, &[EvalBackendKind::KvV1, EvalBackendKind::NullV0]).unwrap();
        let tmp_report = NamedTempFile::new().unwrap();
        write_report_json(&report, tmp_report.path()).unwrap();
        let written = std::fs::read_to_string(tmp_report.path()).unwrap();
        let reparsed: MemoryEvalReport = serde_json::from_str(&written).unwrap();
        assert_eq!(reparsed.candidates.len(), 2);
    }

    #[test]
    fn evaluates_candidate_backends_against_baseline() {
        let traces = sample_trace_set();
        let report = evaluate_trace_set(
            &traces,
            &[
                EvalBackendKind::KvV1,
                EvalBackendKind::FtsV2,
                EvalBackendKind::EpisodicV3,
            ],
        )
        .unwrap();

        assert_eq!(report.candidates.len(), 3);
        assert!(report.candidates.iter().any(|c| c.backend_id == "fts_v2"));
        assert!(report
            .candidates
            .iter()
            .any(|c| c.backend_id == "episodic_v3"));
    }

    #[test]
    fn shadow_eval_against_fts_has_no_get_safety_regression() {
        let traces = sample_trace_set();
        let shadow = evaluate_shadow_against_kv(&traces, EvalBackendKind::FtsV2).unwrap();

        assert_eq!(shadow.baseline_backend_id, "kv_v1");
        assert_eq!(shadow.candidate_backend_id, "fts_v2");
        assert!(shadow.baseline_safety_non_regression);
    }

    #[test]
    fn shadow_eval_against_episodic_has_no_get_safety_regression() {
        let traces = sample_trace_set();
        let shadow = evaluate_shadow_against_kv(&traces, EvalBackendKind::EpisodicV3).unwrap();

        assert_eq!(shadow.baseline_backend_id, "kv_v1");
        assert_eq!(shadow.candidate_backend_id, "episodic_v3");
        assert!(shadow.baseline_safety_non_regression);
    }
}
