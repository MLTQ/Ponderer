use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::eval::{MemoryEvalMetrics, MemoryEvalReport};
use super::MemoryDesignVersion;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDesignArchiveEntry {
    pub id: String,
    pub design_version: MemoryDesignVersion,
    pub description: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl MemoryDesignArchiveEntry {
    pub fn new(
        design_version: MemoryDesignVersion,
        description: Option<String>,
        metadata_json: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            design_version,
            description,
            metadata_json,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvalRunRecord {
    pub id: String,
    pub trace_set_name: String,
    pub report: MemoryEvalReport,
    pub created_at: DateTime<Utc>,
}

impl MemoryEvalRunRecord {
    pub fn from_report(report: MemoryEvalReport) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            trace_set_name: report.trace_set_name.clone(),
            report,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPromotionPolicy {
    /// Candidate recall@k must beat baseline by at least this delta.
    pub min_recall_at_k_gain: f64,
    /// Candidate recall@1 must beat baseline by at least this delta.
    pub min_recall_at_1_gain: f64,
    /// Candidate get-pass-rate must be at least this absolute value.
    pub min_candidate_get_pass_rate: f64,
    /// Candidate mean check latency may be at most this multiplier over baseline.
    pub max_mean_check_latency_multiplier: f64,
    /// Require candidate get-pass-rate to be no worse than baseline.
    pub require_non_decreasing_get_pass_rate: bool,
}

impl Default for MemoryPromotionPolicy {
    fn default() -> Self {
        Self {
            min_recall_at_k_gain: 0.05,
            min_recall_at_1_gain: 0.02,
            min_candidate_get_pass_rate: 0.9,
            max_mean_check_latency_multiplier: 1.25,
            require_non_decreasing_get_pass_rate: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionOutcome {
    Promote,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionMetricsSnapshot {
    pub baseline_backend_id: String,
    pub candidate_backend_id: String,
    pub baseline_metrics: MemoryEvalMetrics,
    pub candidate_metrics: MemoryEvalMetrics,
    pub recall_at_k_delta: f64,
    pub recall_at_1_delta: f64,
    pub baseline_get_pass_rate: f64,
    pub candidate_get_pass_rate: f64,
    pub get_pass_rate_delta: f64,
    pub mean_check_latency_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPromotionDecisionRecord {
    pub id: String,
    pub eval_run_id: String,
    pub candidate_design: MemoryDesignVersion,
    pub outcome: PromotionOutcome,
    pub rationale: String,
    pub policy: MemoryPromotionPolicy,
    pub metrics_snapshot: PromotionMetricsSnapshot,
    pub rollback_target: MemoryDesignVersion,
    pub created_at: DateTime<Utc>,
}

pub fn evaluate_promotion_policy(
    eval_run_id: &str,
    report: &MemoryEvalReport,
    baseline_backend_id: &str,
    candidate_backend_id: &str,
    current_design: &MemoryDesignVersion,
    policy: &MemoryPromotionPolicy,
) -> Result<MemoryPromotionDecisionRecord> {
    let baseline = report
        .candidates
        .iter()
        .find(|c| c.backend_id == baseline_backend_id)
        .with_context(|| {
            format!(
                "Baseline backend '{}' not found in report",
                baseline_backend_id
            )
        })?;

    let candidate = report
        .candidates
        .iter()
        .find(|c| c.backend_id == candidate_backend_id)
        .with_context(|| {
            format!(
                "Candidate backend '{}' not found in report",
                candidate_backend_id
            )
        })?;

    let baseline_get_pass_rate = ratio(baseline.metrics.get_passed, baseline.metrics.get_checks);
    let candidate_get_pass_rate = ratio(candidate.metrics.get_passed, candidate.metrics.get_checks);

    let recall_at_k_delta = candidate.metrics.recall_at_k - baseline.metrics.recall_at_k;
    let recall_at_1_delta = candidate.metrics.recall_at_1 - baseline.metrics.recall_at_1;
    let get_pass_rate_delta = candidate_get_pass_rate - baseline_get_pass_rate;
    let mean_check_latency_ratio = if baseline.metrics.mean_check_ms <= 0.000_001 {
        if candidate.metrics.mean_check_ms <= 0.000_001 {
            1.0
        } else {
            f64::INFINITY
        }
    } else {
        candidate.metrics.mean_check_ms / baseline.metrics.mean_check_ms
    };

    let snapshot = PromotionMetricsSnapshot {
        baseline_backend_id: baseline_backend_id.to_string(),
        candidate_backend_id: candidate_backend_id.to_string(),
        baseline_metrics: baseline.metrics.clone(),
        candidate_metrics: candidate.metrics.clone(),
        recall_at_k_delta,
        recall_at_1_delta,
        baseline_get_pass_rate,
        candidate_get_pass_rate,
        get_pass_rate_delta,
        mean_check_latency_ratio,
    };

    let mut gate_failures = Vec::new();

    if recall_at_k_delta < policy.min_recall_at_k_gain {
        gate_failures.push(format!(
            "recall@k gain {:.4} < required {:.4}",
            recall_at_k_delta, policy.min_recall_at_k_gain
        ));
    }

    if recall_at_1_delta < policy.min_recall_at_1_gain {
        gate_failures.push(format!(
            "recall@1 gain {:.4} < required {:.4}",
            recall_at_1_delta, policy.min_recall_at_1_gain
        ));
    }

    if candidate_get_pass_rate < policy.min_candidate_get_pass_rate {
        gate_failures.push(format!(
            "candidate get-pass-rate {:.4} < required {:.4}",
            candidate_get_pass_rate, policy.min_candidate_get_pass_rate
        ));
    }

    if policy.require_non_decreasing_get_pass_rate && get_pass_rate_delta < 0.0 {
        gate_failures.push(format!(
            "candidate get-pass-rate dropped by {:.4}",
            get_pass_rate_delta
        ));
    }

    if mean_check_latency_ratio > policy.max_mean_check_latency_multiplier {
        gate_failures.push(format!(
            "mean check latency ratio {:.4} > max {:.4}",
            mean_check_latency_ratio, policy.max_mean_check_latency_multiplier
        ));
    }

    let (outcome, rationale) = if gate_failures.is_empty() {
        (
            PromotionOutcome::Promote,
            format!(
                "Promote '{}' over '{}' (Δrecall@k={:.4}, Δrecall@1={:.4}, get-pass-rate={:.4}, latency-ratio={:.4})",
                candidate_backend_id,
                baseline_backend_id,
                recall_at_k_delta,
                recall_at_1_delta,
                candidate_get_pass_rate,
                mean_check_latency_ratio
            ),
        )
    } else {
        (
            PromotionOutcome::Hold,
            format!(
                "Hold '{}' due to policy gate failures: {}",
                candidate_backend_id,
                gate_failures.join("; ")
            ),
        )
    };

    Ok(MemoryPromotionDecisionRecord {
        id: Uuid::new_v4().to_string(),
        eval_run_id: eval_run_id.to_string(),
        candidate_design: candidate.design_version.clone(),
        outcome,
        rationale,
        policy: policy.clone(),
        metrics_snapshot: snapshot,
        rollback_target: current_design.clone(),
        created_at: Utc::now(),
    })
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::eval::{MemoryEvalCandidateReport, MemoryEvalMetrics, MemoryEvalReport};

    fn metrics(
        recall_k: f64,
        recall_1: f64,
        get_passed: usize,
        get_checks: usize,
        mean_check_ms: f64,
    ) -> MemoryEvalMetrics {
        MemoryEvalMetrics {
            traces: 1,
            steps_total: 2,
            checks_total: 2,
            get_checks,
            get_passed,
            query_checks: 1,
            recall_at_1: recall_1,
            recall_at_k: recall_k,
            mean_step_ms: 1.0,
            p95_step_ms: 1.0,
            mean_check_ms,
            p95_check_ms: mean_check_ms,
            final_entries: 2,
            final_bytes_estimate: 42,
        }
    }

    fn report() -> MemoryEvalReport {
        MemoryEvalReport {
            trace_set_name: "sample".to_string(),
            generated_at: Utc::now(),
            candidates: vec![
                MemoryEvalCandidateReport {
                    backend_id: "kv_v1".to_string(),
                    design_version: MemoryDesignVersion::kv_v1(),
                    metrics: metrics(0.60, 0.50, 9, 10, 2.0),
                },
                MemoryEvalCandidateReport {
                    backend_id: "fts_v2".to_string(),
                    design_version: MemoryDesignVersion {
                        design_id: "fts_v2".to_string(),
                        schema_version: 2,
                    },
                    metrics: metrics(0.72, 0.57, 10, 10, 2.2),
                },
            ],
            winner: Some("fts_v2".to_string()),
        }
    }

    #[test]
    fn policy_promotes_when_all_gates_pass() {
        let decision = evaluate_promotion_policy(
            "run-1",
            &report(),
            "kv_v1",
            "fts_v2",
            &MemoryDesignVersion::kv_v1(),
            &MemoryPromotionPolicy::default(),
        )
        .unwrap();

        assert_eq!(decision.outcome, PromotionOutcome::Promote);
        assert_eq!(decision.rollback_target, MemoryDesignVersion::kv_v1());
        assert_eq!(decision.candidate_design.design_id, "fts_v2");
    }

    #[test]
    fn policy_holds_when_recall_gain_is_too_small() {
        let mut strict = MemoryPromotionPolicy::default();
        strict.min_recall_at_k_gain = 0.20;

        let decision = evaluate_promotion_policy(
            "run-2",
            &report(),
            "kv_v1",
            "fts_v2",
            &MemoryDesignVersion::kv_v1(),
            &strict,
        )
        .unwrap();

        assert_eq!(decision.outcome, PromotionOutcome::Hold);
        assert!(decision.rationale.contains("recall@k gain"));
        assert_eq!(decision.rollback_target.design_id, "kv_v1");
    }

    #[test]
    fn evaluation_is_reproducible_for_same_inputs() {
        let policy = MemoryPromotionPolicy::default();
        let r = report();
        let current = MemoryDesignVersion::kv_v1();

        let a =
            evaluate_promotion_policy("run-x", &r, "kv_v1", "fts_v2", &current, &policy).unwrap();
        let b =
            evaluate_promotion_policy("run-x", &r, "kv_v1", "fts_v2", &current, &policy).unwrap();

        assert_eq!(a.outcome, b.outcome);
        assert_eq!(a.candidate_design, b.candidate_design);
        assert_eq!(a.rollback_target, b.rollback_target);
        assert_eq!(
            a.metrics_snapshot.recall_at_k_delta,
            b.metrics_snapshot.recall_at_k_delta
        );
        assert_eq!(
            a.metrics_snapshot.mean_check_latency_ratio,
            b.metrics_snapshot.mean_check_latency_ratio
        );
    }
}
