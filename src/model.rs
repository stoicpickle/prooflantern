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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeystoneGap {
    pub capability_id: String,
    pub pinned: bool,
    pub state: DisplayState,
    pub blocked_core_ids: Vec<String>,
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
    pub keystone: Option<KeystoneGap>,
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

    pub fn gap_impact(&self, gap: &KeystoneGap) -> String {
        if gap.blocked_core_ids.is_empty() {
            return "This unresolved core capability directly blocks the project promise.".into();
        }
        let labels = gap
            .blocked_core_ids
            .iter()
            .filter_map(|id| self.capability(id))
            .map(|item| item.intent.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("The core journey stops here. Downstream: {labels}.")
    }
}
