use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Component, Path},
};

use crate::model::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationError {
    pub messages: Vec<String>,
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.messages.join("; "))
    }
}

impl Error for EvaluationError {}

pub fn evaluate(
    spec: ProjectSpec,
    observations: ObservationSet,
) -> Result<EvaluatedProject, EvaluationError> {
    let errors = validate(&spec, &observations);
    if !errors.is_empty() {
        return Err(EvaluationError { messages: errors });
    }

    let mut warnings = Vec::new();
    let mut assessments = Vec::with_capacity(spec.capabilities.len());
    for capability in &spec.capabilities {
        let mut reasons: Vec<_> = capability
            .manual_evidence
            .iter()
            .cloned()
            .map(|fact| EvidenceReason {
                source: EvidenceSource::Human,
                fact,
            })
            .collect();
        reasons.extend(
            observations
                .observations
                .iter()
                .filter(|item| item.capability_id == capability.id)
                .map(|item| EvidenceReason {
                    source: match item.source {
                        MachineSource::StaticScan => EvidenceSource::StaticScan,
                        MachineSource::ImportedTestResult => EvidenceSource::ImportedTestResult,
                    },
                    fact: item.fact.clone(),
                }),
        );
        let (implementation, verification, display) = derive_states(&reasons);
        if verification == VerificationState::Stale {
            warnings.push(ModelWarning::StaleEvidenceOnly(capability.id.clone()));
        }
        assessments.push(CapabilityAssessment {
            intent: capability.clone(),
            implementation,
            verification,
            display,
            reasons,
        });
    }
    assessments.sort_by(|left, right| {
        role_order(&left.intent.role)
            .cmp(&role_order(&right.intent.role))
            .then_with(|| left.intent.id.cmp(&right.intent.id))
    });

    let focus_selection = choose_focus(&spec, &assessments, &mut warnings);
    Ok(EvaluatedProject {
        project: spec.project,
        capabilities: assessments,
        focus_selection,
        warnings,
    })
}

fn derive_states(
    reasons: &[EvidenceReason],
) -> (ImplementationState, VerificationState, DisplayState) {
    let current_claims: BTreeSet<_> = reasons
        .iter()
        .filter(|reason| reason.fact.freshness == Freshness::Current)
        .map(|reason| claim_rank(reason.fact.claim))
        .collect();
    let stale_verification = reasons.iter().any(|reason| {
        reason.fact.freshness == Freshness::Stale
            && matches!(
                reason.fact.claim,
                Claim::VerificationPassed | Claim::VerificationFailed
            )
    });
    let present = current_claims.contains(&claim_rank(Claim::ImplementationPresent))
        || current_claims.contains(&claim_rank(Claim::VerificationPassed));
    let absent = current_claims.contains(&claim_rank(Claim::ImplementationAbsent));
    let passed = current_claims.contains(&claim_rank(Claim::VerificationPassed));
    let failed = current_claims.contains(&claim_rank(Claim::VerificationFailed));

    let implementation = match (present, absent) {
        (true, true) => ImplementationState::Conflicting,
        (true, false) => ImplementationState::Present,
        (false, true) => ImplementationState::Absent,
        (false, false) => ImplementationState::Unknown,
    };
    let verification = match (passed, failed) {
        (true, true) => VerificationState::Conflicting,
        (true, false) => VerificationState::Passed,
        (false, true) => VerificationState::Failed,
        (false, false) if stale_verification => VerificationState::Stale,
        (false, false) => VerificationState::Unknown,
    };
    let display = if implementation == ImplementationState::Conflicting
        || verification == VerificationState::Conflicting
        || (implementation == ImplementationState::Absent
            && verification == VerificationState::Passed)
    {
        DisplayState::Conflicting
    } else {
        match (implementation, verification) {
            (ImplementationState::Absent, _) => DisplayState::Missing,
            (_, VerificationState::Passed) => DisplayState::Proven,
            (_, VerificationState::Failed) => DisplayState::ProofFailed,
            (ImplementationState::Present, _) => DisplayState::BuiltUnproven,
            _ => DisplayState::Unknown,
        }
    };
    (implementation, verification, display)
}

fn validate(spec: &ProjectSpec, observations: &ObservationSet) -> Vec<String> {
    let mut errors = Vec::new();
    if spec.schema_version != 1 {
        errors.push(format!(
            "unsupported project schema version {}",
            spec.schema_version
        ));
    }
    if observations.schema_version != 1 {
        errors.push(format!(
            "unsupported observations schema version {}",
            observations.schema_version
        ));
    }
    if spec.project.name.trim().is_empty() {
        errors.push("project name must not be blank".into());
    }
    if spec.project.promise.trim().is_empty() {
        errors.push("project promise must not be blank".into());
    }

    let mut ids = BTreeSet::new();
    let mut core_orders = BTreeSet::new();
    for capability in &spec.capabilities {
        if capability.id.trim().is_empty() {
            errors.push("capability id must not be blank".into());
        } else if !ids.insert(capability.id.as_str()) {
            errors.push(format!("duplicate capability id {}", capability.id));
        }
        if capability.label.trim().is_empty() {
            errors.push(format!(
                "capability {} label must not be blank",
                capability.id
            ));
        }
        if capability
            .map_label
            .as_ref()
            .is_some_and(|label| label.trim().is_empty())
        {
            errors.push(format!(
                "capability {} map_label must not be blank",
                capability.id
            ));
        }
        if capability.proof_needed.trim().is_empty() {
            errors.push(format!(
                "capability {} proof_needed must not be blank",
                capability.id
            ));
        }
        if let CapabilityRole::Core { order } = capability.role
            && !core_orders.insert(order)
        {
            errors.push(format!("duplicate core order {order}"));
        }
        for fact in &capability.manual_evidence {
            validate_fact(fact, &capability.id, &mut errors);
        }
    }
    if !spec.capabilities.iter().any(|item| item.role.is_core()) {
        errors.push("project must define at least one core capability".into());
    }

    for capability in &spec.capabilities {
        for dependency in &capability.depends_on {
            if dependency == &capability.id {
                errors.push(format!("capability {} depends on itself", capability.id));
            } else if !ids.contains(dependency.as_str()) {
                errors.push(format!(
                    "capability {} depends on unknown capability {dependency}",
                    capability.id
                ));
            }
        }
        if let CapabilityRole::Supporting { supports } = &capability.role {
            if supports == &capability.id {
                errors.push(format!("capability {} supports itself", capability.id));
            } else if !ids.contains(supports.as_str()) {
                errors.push(format!(
                    "capability {} supports unknown capability {supports}",
                    capability.id
                ));
            } else if !spec
                .capabilities
                .iter()
                .any(|item| item.id == *supports && item.role.is_core())
            {
                errors.push(format!(
                    "capability {} must support a core capability",
                    capability.id
                ));
            }
        }
    }
    if has_dependency_cycle(&spec.capabilities) {
        errors.push("capability dependencies must not contain a cycle".into());
    }

    if let Some(pin) = &spec.project.pinned_keystone {
        match spec.capabilities.iter().find(|item| item.id == *pin) {
            None => errors.push(format!("pinned keystone {pin} does not exist")),
            Some(item) if !item.role.is_core() => {
                errors.push(format!("pinned keystone {pin} is not a core capability"))
            }
            Some(_) => {}
        }
    }

    for observation in &observations.observations {
        if !ids.contains(observation.capability_id.as_str()) {
            errors.push(format!(
                "observation references unknown capability {}",
                observation.capability_id
            ));
        }
        if observation.fact.location.is_none() {
            errors.push(format!(
                "machine evidence for {} must have an inspectable location",
                observation.capability_id
            ));
        }
        validate_fact(&observation.fact, &observation.capability_id, &mut errors);
        match (observation.source, observation.fact.claim) {
            (MachineSource::StaticScan, Claim::ImplementationPresent)
            | (
                MachineSource::ImportedTestResult,
                Claim::VerificationPassed | Claim::VerificationFailed,
            ) => {}
            _ => errors.push(format!(
                "{:?} cannot assert {:?} for {}",
                observation.source, observation.fact.claim, observation.capability_id
            )),
        }
    }
    errors
}

fn validate_fact(fact: &EvidenceFact, capability_id: &str, errors: &mut Vec<String>) {
    if fact.summary.trim().is_empty() {
        errors.push(format!(
            "evidence for {capability_id} must have a nonblank summary"
        ));
    }
    if let Some(location) = &fact.location {
        let path = Path::new(&location.path);
        if location.path.trim().is_empty()
            || path.is_absolute()
            || path.components().any(|part| part == Component::ParentDir)
        {
            errors.push(format!(
                "evidence path for {capability_id} must be a repo-relative path without .."
            ));
        }
        match (location.line_start, location.line_end) {
            (None, None) => {}
            (Some(start), Some(end)) if start > 0 && start <= end => {}
            _ => errors.push(format!(
                "evidence lines for {capability_id} must be paired, 1-based, and ordered"
            )),
        }
    }
}

fn choose_focus(
    spec: &ProjectSpec,
    assessments: &[CapabilityAssessment],
    warnings: &mut Vec<ModelWarning>,
) -> Option<FocusSelection> {
    let by_id: BTreeMap<_, _> = assessments
        .iter()
        .map(|item| (item.intent.id.as_str(), item))
        .collect();
    if let Some(pin) = &spec.project.pinned_keystone {
        let assessment = by_id.get(pin.as_str()).expect("validated pin must exist");
        if assessment.display.is_resolved() {
            warnings.push(ModelWarning::PinnedGapAlreadyProven(pin.clone()));
        } else {
            return Some(focus_for(assessment, true, assessments));
        }
    }

    assessments
        .iter()
        .filter(|item| item.intent.role.is_core() && !item.display.is_resolved())
        .max_by(|left, right| {
            severity(left.display)
                .cmp(&severity(right.display))
                .then_with(|| {
                    blocked_core_ids(&left.intent.id, assessments)
                        .len()
                        .cmp(&blocked_core_ids(&right.intent.id, assessments).len())
                })
                .then_with(|| {
                    right
                        .intent
                        .role
                        .core_order()
                        .cmp(&left.intent.role.core_order())
                })
                .then_with(|| right.intent.id.cmp(&left.intent.id))
        })
        .map(|item| focus_for(item, false, assessments))
}

fn focus_for(
    assessment: &CapabilityAssessment,
    pinned: bool,
    assessments: &[CapabilityAssessment],
) -> FocusSelection {
    FocusSelection {
        capability_id: assessment.intent.id.clone(),
        pinned,
        downstream_core_ids: blocked_core_ids(&assessment.intent.id, assessments),
    }
}

fn blocked_core_ids(id: &str, assessments: &[CapabilityAssessment]) -> Vec<String> {
    let by_id: BTreeMap<_, _> = assessments
        .iter()
        .map(|item| (item.intent.id.as_str(), item))
        .collect();
    let mut blocked = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut frontier = vec![id];
    while let Some(current) = frontier.pop() {
        for item in assessments.iter().filter(|item| {
            item.intent.depends_on.iter().any(|value| value == current)
                && item.intent.role.is_core()
        }) {
            if visited.insert(item.intent.id.clone()) {
                if !item.display.is_resolved() {
                    blocked.insert(item.intent.id.clone());
                }
                frontier.push(item.intent.id.as_str());
            }
        }
    }
    blocked
        .into_iter()
        .filter(|candidate| by_id.contains_key(candidate.as_str()))
        .collect()
}

fn has_dependency_cycle(capabilities: &[CapabilityIntent]) -> bool {
    fn visit<'a>(
        id: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        active: &mut BTreeSet<&'a str>,
        complete: &mut BTreeSet<&'a str>,
    ) -> bool {
        if complete.contains(id) {
            return false;
        }
        if !active.insert(id) {
            return true;
        }
        if graph
            .get(id)
            .into_iter()
            .flatten()
            .any(|next| visit(next, graph, active, complete))
        {
            return true;
        }
        active.remove(id);
        complete.insert(id);
        false
    }

    let graph: BTreeMap<_, _> = capabilities
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.depends_on.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    graph
        .keys()
        .copied()
        .any(|id| visit(id, &graph, &mut active, &mut complete))
}

const fn severity(state: DisplayState) -> u8 {
    match state {
        DisplayState::Missing | DisplayState::ProofFailed => 3,
        DisplayState::Unknown | DisplayState::Conflicting => 2,
        DisplayState::BuiltUnproven => 1,
        DisplayState::Proven => 0,
    }
}

const fn claim_rank(claim: Claim) -> u8 {
    match claim {
        Claim::ImplementationPresent => 0,
        Claim::ImplementationAbsent => 1,
        Claim::VerificationPassed => 2,
        Claim::VerificationFailed => 3,
    }
}

fn role_order(role: &CapabilityRole) -> (u8, u16) {
    match role {
        CapabilityRole::Core { order } => (0, *order),
        CapabilityRole::Supporting { .. } => (1, u16::MAX),
        CapabilityRole::Optional => (2, u16::MAX),
    }
}
