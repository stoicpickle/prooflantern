use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSpec {
    pub schema_version: u32,
    pub project: ProjectIntent,
    pub capabilities: Vec<CapabilityIntent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectIntent {
    pub name: String,
    pub promise: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_keystone: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityIntent {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_label: Option<String>,
    pub role: CapabilityRole,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub proof_needed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_evidence: Vec<EvidenceFact>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapabilityRole {
    Core { order: u16 },
    Supporting { supports: String },
    Optional,
}

impl CapabilityRole {
    pub const fn is_core(&self) -> bool {
        matches!(self, Self::Core { .. })
    }

    pub const fn core_order(&self) -> Option<u16> {
        match self {
            Self::Core { order } => Some(*order),
            Self::Supporting { .. } | Self::Optional => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationSet {
    pub schema_version: u32,
    #[serde(default)]
    pub observations: Vec<MachineObservation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MachineObservation {
    pub capability_id: String,
    pub source: MachineSource,
    pub fact: EvidenceFact,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineSource {
    StaticScan,
    ImportedTestResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceFact {
    pub claim: Claim,
    pub freshness: Freshness,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<EvidenceLocation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Claim {
    ImplementationPresent,
    ImplementationAbsent,
    VerificationPassed,
    VerificationFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Current,
    Stale,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceLocation {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplementationState {
    Unknown,
    Present,
    Absent,
    Conflicting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationState {
    Unknown,
    Passed,
    Failed,
    Stale,
    Conflicting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayState {
    Proven,
    BuiltUnproven,
    Missing,
    ProofFailed,
    Unknown,
    Conflicting,
}

impl DisplayState {
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Proven => "✓",
            Self::BuiltUnproven => "◐",
            Self::Missing => "╳",
            Self::ProofFailed => "!",
            Self::Unknown => "?",
            Self::Conflicting => "⚠",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Proven => "PROVEN",
            Self::BuiltUnproven => "BUILT / UNPROVEN",
            Self::Missing => "MISSING",
            Self::ProofFailed => "PROOF FAILED",
            Self::Unknown => "UNKNOWN",
            Self::Conflicting => "CONFLICTING",
        }
    }

    pub const fn is_resolved(self) -> bool {
        matches!(self, Self::Proven)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceReason {
    pub source: EvidenceSource,
    pub fact: EvidenceFact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceSource {
    Human,
    StaticScan,
    ImportedTestResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityAssessment {
    pub intent: CapabilityIntent,
    pub implementation: ImplementationState,
    pub verification: VerificationState,
    pub display: DisplayState,
    pub reasons: Vec<EvidenceReason>,
}

impl CapabilityAssessment {
    pub fn map_label(&self) -> &str {
        self.intent
            .map_label
            .as_deref()
            .unwrap_or(&self.intent.label)
    }

    pub const fn why(&self) -> &'static str {
        match self.display {
            DisplayState::Proven => "Current recorded proof supports this capability.",
            DisplayState::BuiltUnproven => {
                "Implementation evidence exists, but no current passing proof is recorded."
            }
            DisplayState::Missing => {
                "Explicit current evidence says the required implementation is absent."
            }
            DisplayState::ProofFailed => "A current recorded proof failed.",
            DisplayState::Unknown => {
                "Accepted for the journey; no current technical evidence establishes its state."
            }
            DisplayState::Conflicting => {
                "Current evidence conflicts, so Proof Lantern will not guess a state."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusKind {
    JourneyBreak,
    FailedCheck,
    NeedsProof,
    NeedsEvidence,
    ResolveConflict,
}

impl FocusKind {
    pub const fn heading(self) -> &'static str {
        match self {
            Self::JourneyBreak => "JOURNEY BREAK",
            Self::FailedCheck => "FAILED CHECK",
            Self::NeedsProof => "NEEDS PROOF",
            Self::NeedsEvidence => "NEEDS EVIDENCE",
            Self::ResolveConflict => "RESOLVE CONFLICT",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FocusAction {
    pub heading: &'static str,
    pub instruction: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CurrentFocus<'a> {
    Complete {
        heading: &'static str,
        summary: &'static str,
    },
    Capability {
        capability: &'a CapabilityAssessment,
        pinned: bool,
        kind: FocusKind,
        summary: String,
        action: FocusAction,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FocusSelection {
    pub(crate) capability_id: String,
    pub(crate) pinned: bool,
    pub(crate) downstream_core_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelWarning {
    PinnedGapAlreadyProven(String),
    StaleEvidenceOnly(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluatedProject {
    pub project: ProjectIntent,
    pub capabilities: Vec<CapabilityAssessment>,
    pub(crate) focus_selection: Option<FocusSelection>,
    pub warnings: Vec<ModelWarning>,
}

impl EvaluatedProject {
    pub fn capability(&self, id: &str) -> Option<&CapabilityAssessment> {
        self.capabilities.iter().find(|item| item.intent.id == id)
    }

    pub fn core_capabilities(&self) -> impl Iterator<Item = &CapabilityAssessment> {
        self.capabilities
            .iter()
            .filter(|item| item.intent.role.is_core())
    }

    pub fn warning_messages(&self) -> impl Iterator<Item = String> + '_ {
        self.warnings.iter().map(|warning| match warning {
            ModelWarning::PinnedGapAlreadyProven(id) => format!(
                "Pinned focus \"{}\" is already proven; automatic focus selection was used.",
                self.capability_label(id)
            ),
            ModelWarning::StaleEvidenceOnly(id) => format!(
                "\"{}\" has only stale verification evidence; it does not count as current proof.",
                self.capability_label(id)
            ),
        })
    }

    fn capability_label<'a>(&'a self, id: &'a str) -> &'a str {
        self.capability(id)
            .map(|capability| capability.intent.label.as_str())
            .unwrap_or(id)
    }

    pub fn current_focus(&self) -> CurrentFocus<'_> {
        let Some(selection) = &self.focus_selection else {
            return CurrentFocus::Complete {
                heading: "CORE JOURNEY PROVEN",
                summary: "All accepted core capabilities have current recorded proof.",
            };
        };
        let capability = self
            .capability(&selection.capability_id)
            .expect("evaluated focus must reference a capability");
        let (kind, mut summary, action_heading, action_instruction) = match capability.display {
            DisplayState::Missing => (
                FocusKind::JourneyBreak,
                "Required implementation is recorded absent.".to_owned(),
                "PROOF NEEDED",
                capability.intent.proof_needed.clone(),
            ),
            DisplayState::ProofFailed => (
                FocusKind::FailedCheck,
                "A current recorded check failed; this does not establish that the implementation is absent."
                    .to_owned(),
                "NEXT CHECK",
                format!(
                    "Inspect the failure; replace or mark stale the failed record before rerunning: {}",
                    capability.intent.proof_needed
                ),
            ),
            DisplayState::BuiltUnproven => (
                FocusKind::NeedsProof,
                "Implementation evidence exists, but no current passing proof is recorded."
                    .to_owned(),
                "PROOF NEEDED",
                capability.intent.proof_needed.clone(),
            ),
            DisplayState::Unknown => (
                FocusKind::NeedsEvidence,
                "No current technical evidence establishes whether this capability exists or works."
                    .to_owned(),
                "NEXT CHECK",
                format!(
                    "Inspect the implementation or record evidence, then: {}",
                    capability.intent.proof_needed
                ),
            ),
            DisplayState::Conflicting => (
                FocusKind::ResolveConflict,
                "Current evidence conflicts, so Proof Lantern cannot establish this capability's state."
                    .to_owned(),
                "NEXT CHECK",
                format!(
                    "Reconcile or mark stale the conflicting records, then: {}",
                    capability.intent.proof_needed
                ),
            ),
            DisplayState::Proven => {
                unreachable!("focus selection excludes proven capabilities")
            }
        };
        if kind == FocusKind::JourneyBreak && !selection.downstream_core_ids.is_empty() {
            let labels = selection
                .downstream_core_ids
                .iter()
                .filter_map(|id| self.capability(id))
                .map(|item| item.intent.label.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            summary.push_str(&format!(" Downstream unresolved: {labels}."));
        }
        CurrentFocus::Capability {
            capability,
            pinned: selection.pinned,
            kind,
            summary,
            action: FocusAction {
                heading: action_heading,
                instruction: action_instruction,
            },
        }
    }
}
