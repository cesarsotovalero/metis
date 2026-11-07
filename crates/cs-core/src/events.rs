use crate::{Measurement, Usage, UsageMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Observed,
    AgentReported,
    Inferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcome {
    Passed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    Metadata,
    Redacted,
    FullLocal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    Observe,
    Shadow,
    Conservative,
    Experimental,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Verified,
    DeclaredUnverified,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Resolved,
    Ambiguous,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    UsageRequestLevel,
    UsageSessionLevel,
    ToolEvents,
    ToolOutputOriginal,
    ToolOutputReplace,
    DeliveryConfirmation,
    ContextSnapshot,
    HistoryRewrite,
    CompactionEvents,
    RequirementsExplicit,
    ReasoningSummary,
    ChildSessionLinkage,
    RecoveryTool,
    RecoveryFile,
    ProviderRequestTransform,
}

/// Metadata observations only. These events never authorize an intervention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Payload {
    SessionStarted {},
    SessionResumed {
        previous_epoch: String,
    },
    SessionEnded {},
    TaskStarted {
        task_id: String,
    },
    TaskOutcomeRecorded {
        task_id: String,
        outcome: TaskOutcome,
        oracle_version: Option<String>,
        evidence_kind: EvidenceKind,
    },
    RequestStarted {
        request_id: String,
    },
    RequestStreaming {
        request_id: String,
    },
    RequestCompleted {
        request_id: String,
    },
    RequestFailed {
        request_id: String,
    },
    RequestCancelled {
        request_id: String,
    },
    UsageObserved {
        request_id: String,
        mode: UsageMode,
        usage: Usage,
    },
    ModelChanged {
        provider: String,
        model: String,
    },
    ToolStarted {
        tool_id: String,
    },
    ToolCompleted {
        tool_id: String,
        exit_code: i32,
    },
    ToolFailed {
        tool_id: String,
    },
    ToolCancelled {
        tool_id: String,
    },
    ArtifactStored {
        artifact_id: String,
        content_hash: String,
        byte_length: u64,
        capture_mode: CaptureMode,
    },
    FileVersionObserved {
        file_version_id: String,
        worktree_id: String,
        content_hash: String,
        predecessor_id: Option<String>,
    },
    ArtifactSuperseded {
        artifact_id: String,
        successor_id: String,
    },
    RequirementRecorded {
        requirement_id: String,
        text_ref: String,
        evidence_kind: EvidenceKind,
    },
    CompactionObserved {
        compaction_id: String,
    },
    ContextSnapshotObserved {
        snapshot_id: String,
        tokens: Measurement<u64>,
    },
    RepresentationProposed {
        representation_id: String,
        original_artifact_id: String,
    },
    RepresentationCommitted {
        representation_id: String,
        output_artifact_id: String,
    },
    RepresentationRejected {
        representation_id: String,
        reason_code: String,
    },
    RetrievalRequested {
        retrieval_id: String,
        artifact_id: String,
    },
    RetrievalCompleted {
        retrieval_id: String,
        byte_length: u64,
    },
    RetrievalUnavailable {
        retrieval_id: String,
        reason_code: String,
    },
    VisibilityGap {
        code: String,
    },
    AdapterCapabilityChanged {
        capability: Capability,
        status: CapabilityStatus,
        agent_version: String,
        evidence_ref: Option<String>,
    },
    QueueOverflow {
        dropped_events: u64,
    },
    ReconciliationResult {
        reconciliation_id: String,
        status: ReconciliationStatus,
        source_event_id: Option<String>,
    },
    PolicyActivated {
        policy_hash: String,
        mode: PolicyMode,
    },
    PolicyDisabled {
        policy_hash: String,
        reason_code: String,
    },
    ExperimentAssignment {
        experiment_id: String,
        arm: String,
    },
}

impl Payload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionStarted { .. } => "session_started",
            Self::SessionResumed { .. } => "session_resumed",
            Self::SessionEnded { .. } => "session_ended",
            Self::TaskStarted { .. } => "task_started",
            Self::TaskOutcomeRecorded { .. } => "task_outcome_recorded",
            Self::RequestStarted { .. } => "request_started",
            Self::RequestStreaming { .. } => "request_streaming",
            Self::RequestCompleted { .. } => "request_completed",
            Self::RequestFailed { .. } => "request_failed",
            Self::RequestCancelled { .. } => "request_cancelled",
            Self::UsageObserved { .. } => "usage_observed",
            Self::ModelChanged { .. } => "model_changed",
            Self::ToolStarted { .. } => "tool_started",
            Self::ToolCompleted { .. } => "tool_completed",
            Self::ToolFailed { .. } => "tool_failed",
            Self::ToolCancelled { .. } => "tool_cancelled",
            Self::ArtifactStored { .. } => "artifact_stored",
            Self::FileVersionObserved { .. } => "file_version_observed",
            Self::ArtifactSuperseded { .. } => "artifact_superseded",
            Self::RequirementRecorded { .. } => "requirement_recorded",
            Self::CompactionObserved { .. } => "compaction_observed",
            Self::ContextSnapshotObserved { .. } => "context_snapshot_observed",
            Self::RepresentationProposed { .. } => "representation_proposed",
            Self::RepresentationCommitted { .. } => "representation_committed",
            Self::RepresentationRejected { .. } => "representation_rejected",
            Self::RetrievalRequested { .. } => "retrieval_requested",
            Self::RetrievalCompleted { .. } => "retrieval_completed",
            Self::RetrievalUnavailable { .. } => "retrieval_unavailable",
            Self::VisibilityGap { .. } => "visibility_gap",
            Self::AdapterCapabilityChanged { .. } => "adapter_capability_changed",
            Self::QueueOverflow { .. } => "queue_overflow",
            Self::ReconciliationResult { .. } => "reconciliation_result",
            Self::PolicyActivated { .. } => "policy_activated",
            Self::PolicyDisabled { .. } => "policy_disabled",
            Self::ExperimentAssignment { .. } => "experiment_assignment",
        }
    }
}
