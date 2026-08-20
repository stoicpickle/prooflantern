use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    App,
    model::{
        CapabilityAssessment, CapabilityRole, Claim, CurrentFocus, DisplayState, EvidenceSource,
        FocusKind, Freshness,
    },
    text::{middle_truncate, truncate},
    theme::Palette,
};

const MINIMUM_WIDTH: u16 = 100;
const MINIMUM_HEIGHT: u16 = 30;
const WIDE_WIDTH: u16 = 128;
const WIDE_HEIGHT: u16 = 36;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let palette = Palette::default();
    frame.render_widget(
        Block::default().style(Style::default().bg(palette.background)),
        area,
    );
    if area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT {
        render_minimum_size(frame, area, palette);
        return;
    }

    let frame_rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);
    render_header(frame, frame_rows[0], app, palette);
    let wide = area.width >= WIDE_WIDTH && area.height >= WIDE_HEIGHT;
    if wide {
        let columns =
            Layout::horizontal([Constraint::Min(72), Constraint::Length(43)]).split(frame_rows[1]);
        render_journey(frame, columns[0], app, palette);
        render_inspector(frame, columns[1], app, palette);
    } else {
        render_journey(frame, frame_rows[1], app, palette);
        if app.inspector_open() {
            let drawer_height = if app.project_command_hints() { 14 } else { 13 };
            let height = frame_rows[1].height.min(drawer_height);
            let drawer = Rect::new(
                frame_rows[1].x,
                frame_rows[1].bottom().saturating_sub(height),
                frame_rows[1].width,
                height,
            );
            frame.render_widget(Clear, drawer);
            render_inspector(frame, drawer, app, palette);
        }
    }
    render_footer(frame, frame_rows[2], wide, app.inspector_open(), palette);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let focus = match app.project().current_focus() {
        CurrentFocus::Capability { capability, .. } => Some(capability),
        CurrentFocus::Complete { .. } => None,
    };
    let mut spans = vec![
        Span::styled("PROOF LANTERN", Style::default().fg(palette.hot).bold()),
        Span::styled(" // ", Style::default().fg(palette.grid)),
        Span::styled(
            app.project().project.name.to_uppercase(),
            Style::default().fg(palette.text).bold(),
        ),
    ];
    if let Some(focus) = focus {
        spans.extend([
            Span::styled("   FOCUS  ", Style::default().fg(palette.muted)),
            Span::styled(
                focus.intent.label.to_uppercase(),
                Style::default().fg(palette.warning).bold(),
            ),
        ]);
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(instrument_block(
            " PROJECT STATUS ",
            palette.primary,
            palette.background,
        )),
        area,
    );
}

fn render_journey(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let block = instrument_block(
        " ACCEPTED CORE JOURNEY ",
        palette.primary,
        palette.background,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let focus_height =
        5 + u16::from(!app.project().warnings.is_empty()) + u16::from(app.project_command_hints());
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(focus_height),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "PROMISE",
                Style::default().fg(palette.primary).bold(),
            )),
            Line::from(Span::styled(
                format!("“{}”", app.project().project.promise),
                Style::default().fg(palette.text),
            )),
        ])
        .wrap(Wrap { trim: true }),
        rows[0],
    );

    let core: Vec<_> = app.project().core_capabilities().collect();
    let node_areas = render_core_path(frame, rows[1], &core, app, palette);
    render_supporting(frame, rows[2], &core, &node_areas, app, palette);
    render_optional(frame, rows[3], app, palette);
    render_focus(frame, rows[4], app, palette);
}

fn render_core_path(
    frame: &mut Frame<'_>,
    area: Rect,
    core: &[&CapabilityAssessment],
    app: &App,
    palette: Palette,
) -> Vec<Rect> {
    if core.is_empty() {
        frame.render_widget(
            Paragraph::new("No accepted core capabilities.")
                .style(Style::default().fg(palette.muted)),
            area,
        );
        return Vec::new();
    }
    let mut constraints = Vec::with_capacity(core.len() * 2 - 1);
    for index in 0..core.len() {
        constraints.push(Constraint::Fill(1));
        if index + 1 < core.len() {
            constraints.push(Constraint::Length(6));
        }
    }
    let chunks = Layout::horizontal(constraints).split(area);
    let node_areas: Vec<_> = chunks.iter().step_by(2).copied().collect();
    for (index, capability) in core.iter().enumerate() {
        let node_area = node_areas[index];
        render_node(frame, node_area, capability, app, palette);
        if let Some(next) = core.get(index + 1) {
            let connector_area = chunks[index * 2 + 1];
            let connector = connector_for(capability.display, next.display);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    connector,
                    Style::default().fg(connector_color(capability.display, next.display, palette)),
                ))),
                Rect::new(
                    connector_area.x,
                    connector_area.y + 1,
                    connector_area.width,
                    1,
                ),
            );
        }
    }
    node_areas
}

fn render_node(
    frame: &mut Frame<'_>,
    area: Rect,
    capability: &CapabilityAssessment,
    app: &App,
    palette: Palette,
) {
    let selected = app
        .selected()
        .is_some_and(|item| item.intent.id == capability.intent.id);
    let state_style = state_style(capability.display, palette);
    let label_width = usize::from(area.width.saturating_sub(if selected { 4 } else { 1 }));
    let label = truncate(&capability.map_label().to_uppercase(), label_width.max(1));
    let lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", capability.display.glyph()),
            state_style.add_modifier(Modifier::BOLD),
        ),
        Span::styled(label, Style::default().fg(palette.text).bold()),
    ])];
    let mut lines = lines;
    if capability.display == DisplayState::BuiltUnproven {
        lines.extend([
            Line::from(Span::styled("BUILT /", state_style)),
            Line::from(Span::styled("UNPROVEN", state_style)),
        ]);
    } else {
        lines.push(Line::from(Span::styled(
            capability.display.label(),
            state_style,
        )));
    }
    if selected {
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(palette.hot))
                    .style(Style::default().bg(palette.panel)),
            ),
            area,
        );
    } else {
        let content = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        );
        frame.render_widget(Paragraph::new(lines), content);
    }
}

fn render_supporting(
    frame: &mut Frame<'_>,
    area: Rect,
    core: &[&CapabilityAssessment],
    node_areas: &[Rect],
    app: &App,
    palette: Palette,
) {
    let supports: Vec<_> = app
        .project()
        .capabilities
        .iter()
        .filter(|item| matches!(item.intent.role, CapabilityRole::Supporting { .. }))
        .collect();
    if supports.is_empty() || area.height == 0 {
        return;
    }
    let selected_index = app
        .selected()
        .and_then(|selected| {
            supports
                .iter()
                .position(|item| item.intent.id == selected.intent.id)
        })
        .unwrap_or(0);
    let capacity = usize::from(area.height);
    let start = support_window_start(supports.len(), selected_index, capacity);
    let lines = supports
        .iter()
        .skip(start)
        .take(capacity)
        .map(|support| {
            let CapabilityRole::Supporting { supports: parent } = &support.intent.role else {
                unreachable!("filtered supporting capability")
            };
            let parent_index = core
                .iter()
                .position(|item| item.intent.id == *parent)
                .unwrap_or(0);
            let indent = node_areas
                .get(parent_index)
                .map(|node| usize::from(node.x.saturating_sub(area.x).saturating_add(2)))
                .unwrap_or(0)
                .min(usize::from(area.width.saturating_sub(1)));
            let selected = app
                .selected()
                .is_some_and(|item| item.intent.id == support.intent.id);
            let style = if selected {
                Style::default().fg(palette.hot).bg(palette.panel).bold()
            } else {
                state_style(support.display, palette)
            };
            let label_width = usize::from(area.width)
                .saturating_sub(indent + 6 + "  SUPPORTING".len())
                .max(1);
            Line::from(vec![
                Span::raw(format!("{:indent$}└──── ", "")),
                Span::styled(
                    truncate(
                        &format!(
                            "{} {}",
                            support.display.glyph(),
                            support.map_label().to_uppercase()
                        ),
                        label_width,
                    ),
                    style,
                ),
                Span::styled("  SUPPORTING", Style::default().fg(palette.muted)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_optional(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let optional: Vec<_> = app
        .project()
        .capabilities
        .iter()
        .filter(|item| matches!(item.intent.role, CapabilityRole::Optional))
        .collect();
    if optional.is_empty() {
        return;
    }
    let mut spans = vec![Span::styled(
        "OPTIONAL  ",
        Style::default().fg(palette.muted).bold(),
    )];
    for item in optional {
        let selected = app
            .selected()
            .is_some_and(|selected| selected.intent.id == item.intent.id);
        spans.push(Span::styled(
            format!(
                "{} {}  ",
                item.display.glyph(),
                item.map_label().to_uppercase()
            ),
            if selected {
                Style::default().fg(palette.hot).bg(palette.panel)
            } else {
                Style::default().fg(palette.muted)
            },
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_focus(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    match app.project().current_focus() {
        CurrentFocus::Complete { heading, summary } => {
            let panel_title = format!(" {heading} ");
            let mut lines = vec![Line::from(summary)];
            if let Some(warning) = project_warning_line(app, area, palette) {
                lines.push(warning);
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(instrument_block(
                        &panel_title,
                        palette.success,
                        palette.background,
                    ))
                    .style(Style::default().fg(palette.success)),
                area,
            );
        }
        CurrentFocus::Capability {
            capability,
            kind,
            summary,
            action,
            ..
        } => {
            let width = usize::from(area.width.saturating_sub(2));
            let accent = focus_style(kind, palette);
            let action_width = width.saturating_sub(action.heading.len() + 2);
            let panel_title = format!(" {} ", kind.heading());
            let id_budget = (width / 3).max(8);
            let id = middle_truncate(
                &capability.intent.id,
                id_budget.saturating_sub("ID  ".len()),
            );
            let id_suffix = format!("   ID  {id}");
            let label_width = width.saturating_sub(2 + UnicodeWidthStr::width(id_suffix.as_str()));
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{} ", capability.display.glyph()),
                        state_style(capability.display, palette).bold(),
                    ),
                    Span::styled(
                        truncate(&capability.intent.label.to_uppercase(), label_width),
                        Style::default().fg(palette.text).bold(),
                    ),
                    Span::styled(id_suffix, Style::default().fg(palette.muted)),
                ]),
                Line::from(Span::styled(
                    truncate(&summary, width.max(1)),
                    Style::default().fg(accent),
                )),
                Line::from(vec![
                    Span::styled(
                        format!("{}  ", action.heading),
                        Style::default().fg(palette.primary).bold(),
                    ),
                    Span::styled(
                        truncate(&action.instruction, action_width),
                        Style::default().fg(palette.text),
                    ),
                ]),
            ];
            let command_width = width.saturating_sub(UnicodeWidthStr::width("RUN FROM MAP ROOT  "));
            if let Some(command) =
                project_explain_command(app, &capability.intent.id, command_width)
            {
                lines.push(Line::from(vec![
                    Span::styled(
                        "RUN FROM MAP ROOT  ",
                        Style::default().fg(palette.primary).bold(),
                    ),
                    Span::styled(command, Style::default().fg(palette.text)),
                ]));
            }
            if let Some(warning) = project_warning_line(app, area, palette) {
                lines.push(warning);
            }
            frame.render_widget(
                Paragraph::new(lines).block(instrument_block(
                    &panel_title,
                    accent,
                    palette.background,
                )),
                area,
            );
        }
    }
}

fn project_warning_line(app: &App, area: Rect, palette: Palette) -> Option<Line<'static>> {
    let mut warnings = app.project().warning_messages();
    let first = warnings.next()?;
    let remaining = warnings.count();
    let suffix = if remaining == 0 {
        String::new()
    } else {
        format!(" (+{remaining} more)")
    };
    let width = usize::from(area.width.saturating_sub(2)).max(1);
    Some(Line::from(Span::styled(
        truncate(&format!("⚠ {first}{suffix}"), width),
        Style::default().fg(palette.warning),
    )))
}

fn focus_style(kind: FocusKind, palette: Palette) -> ratatui::style::Color {
    match kind {
        FocusKind::JourneyBreak | FocusKind::FailedCheck | FocusKind::ResolveConflict => {
            palette.warning
        }
        FocusKind::NeedsProof => palette.primary,
        FocusKind::NeedsEvidence => palette.muted,
    }
}

fn render_inspector(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let Some(capability) = app.selected() else {
        frame.render_widget(
            Paragraph::new("No capability selected.").block(instrument_block(
                " INSPECTOR ",
                palette.primary,
                palette.panel,
            )),
            area,
        );
        return;
    };
    let block = instrument_block(" INSPECTOR ", palette.primary, palette.panel);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width).max(1);
    let command = project_explain_command(app, &capability.intent.id, width);
    let proof_height = match (command.is_some(), inner.width < 60) {
        (true, true) => 8,
        (true, false) => 4,
        (false, true) => 6,
        (false, false) => 3,
    }
    .min(inner.height);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(proof_height),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    format!("{} ", capability.display.glyph()),
                    state_style(capability.display, palette).bold(),
                ),
                Span::styled(
                    truncate(
                        &capability.intent.label.to_uppercase(),
                        width.saturating_sub(2),
                    ),
                    Style::default().fg(palette.text).bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    capability.display.label(),
                    state_style(capability.display, palette),
                ),
                Span::styled("   ◇ ACCEPTED", Style::default().fg(palette.muted)),
            ]),
            Line::from(Span::styled(
                format!(
                    "ID  {}",
                    middle_truncate(&capability.intent.id, width.saturating_sub(4))
                ),
                Style::default().fg(palette.muted),
            )),
        ]),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(vec![
            heading("WHY", palette),
            Line::from(Span::styled(
                capability.why(),
                Style::default().fg(palette.text),
            )),
        ])
        .wrap(Wrap { trim: true }),
        rows[1],
    );

    let mut evidence = vec![heading("EVIDENCE", palette)];
    let available = usize::from(rows[2].height.saturating_sub(1));
    if capability.reasons.is_empty() {
        evidence.push(Line::from(Span::styled(
            "No current evidence recorded.",
            Style::default().fg(palette.muted),
        )));
    } else if available > 0 {
        let detailed = rows[2].width < 60;
        if available == 1
            && capability.display == DisplayState::Conflicting
            && let Some(line) = compact_conflict_line(capability, width)
        {
            evidence.push(Line::from(Span::styled(
                line,
                Style::default().fg(palette.warning),
            )));
            frame.render_widget(Paragraph::new(evidence), rows[2]);
            if let Some(command) = command {
                let proof_rows =
                    Layout::vertical([Constraint::Min(2), Constraint::Length(2)]).split(rows[3]);
                render_inspector_proof(frame, proof_rows[0], capability, palette);
                render_inspector_command(frame, proof_rows[1], command, palette);
            } else {
                render_inspector_proof(frame, rows[3], capability, palette);
            }
            return;
        }
        let visible = visible_evidence(capability, available, detailed);
        for (index, (reason, show_location)) in visible.iter().enumerate() {
            let source = match reason.source {
                EvidenceSource::Human => "HUMAN",
                EvidenceSource::StaticScan => "STATIC SCAN",
                EvidenceSource::ImportedTestResult => "TEST RESULT",
            };
            let freshness = if reason.fact.freshness == crate::model::Freshness::Stale {
                " [STALE]"
            } else {
                ""
            };
            let location = if let Some(location) = &reason.fact.location {
                let suffix = match (location.line_start, location.line_end) {
                    (Some(start), Some(end)) => format!(":{start}-{end}"),
                    _ => String::new(),
                };
                format!("  {}{suffix}", location.path)
            } else {
                String::new()
            };
            let remaining = capability.reasons.len() - visible.len();
            let more = if index + 1 == visible.len() && remaining > 0 {
                format!("  (+{remaining} more)")
            } else {
                String::new()
            };
            if detailed {
                let summary = format!("{source}{freshness}  {}{more}", reason.fact.summary);
                let summary = if capability.reasons.len() == 1 && available >= 3 {
                    summary
                } else {
                    middle_truncate(&summary, width)
                };
                evidence.push(Line::from(Span::styled(
                    summary,
                    Style::default().fg(palette.text),
                )));
                if *show_location && !location.is_empty() {
                    evidence.push(Line::from(Span::styled(
                        middle_truncate(location.trim(), width),
                        Style::default().fg(palette.muted),
                    )));
                }
            } else {
                let line = format!(
                    "{source}{freshness}  {}{location}{more}",
                    reason.fact.summary
                );
                evidence.push(Line::from(Span::styled(
                    middle_truncate(&line, width),
                    Style::default().fg(palette.text),
                )));
            }
        }
    }
    frame.render_widget(Paragraph::new(evidence).wrap(Wrap { trim: true }), rows[2]);
    if let Some(command) = command {
        let proof_rows =
            Layout::vertical([Constraint::Min(2), Constraint::Length(2)]).split(rows[3]);
        render_inspector_proof(frame, proof_rows[0], capability, palette);
        render_inspector_command(frame, proof_rows[1], command, palette);
    } else {
        render_inspector_proof(frame, rows[3], capability, palette);
    }
}

fn render_inspector_proof(
    frame: &mut Frame<'_>,
    area: Rect,
    capability: &CapabilityAssessment,
    palette: Palette,
) {
    frame.render_widget(
        Paragraph::new(vec![
            heading("PROOF NEEDED", palette),
            Line::from(Span::styled(
                &capability.intent.proof_needed,
                Style::default().fg(palette.text),
            )),
        ])
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(palette.panel)),
        area,
    );
}

fn render_inspector_command(frame: &mut Frame<'_>, area: Rect, command: String, palette: Palette) {
    frame.render_widget(
        Paragraph::new(vec![
            heading("RUN FROM MAP ROOT", palette),
            Line::from(Span::styled(command, Style::default().fg(palette.text))),
        ])
        .style(Style::default().bg(palette.panel)),
        area,
    );
}

fn compact_conflict_line(capability: &CapabilityAssessment, width: usize) -> Option<String> {
    let pair: Vec<_> = capability
        .reasons
        .iter()
        .filter(|reason| reason.fact.freshness == Freshness::Current)
        .take(2)
        .collect();
    let [left, right] = pair.as_slice() else {
        return None;
    };
    let remaining = capability.reasons.len().saturating_sub(2);
    let suffix = if remaining > 0 {
        format!("  (+{remaining} more)")
    } else {
        String::new()
    };
    Some(middle_truncate(
        &format!(
            "CURRENT CONFLICT  {} ↔ {}{suffix}",
            claim_label(left.fact.claim),
            claim_label(right.fact.claim)
        ),
        width,
    ))
}

const fn claim_label(claim: Claim) -> &'static str {
    match claim {
        Claim::ImplementationPresent => "BUILT",
        Claim::ImplementationAbsent => "MISSING",
        Claim::VerificationPassed => "PASSED",
        Claim::VerificationFailed => "FAILED",
    }
}

fn visible_evidence(
    capability: &CapabilityAssessment,
    available: usize,
    detailed: bool,
) -> Vec<(&crate::model::EvidenceReason, bool)> {
    if !detailed {
        return capability
            .reasons
            .iter()
            .take(available)
            .map(|reason| (reason, false))
            .collect();
    }

    let mut remaining_rows = available;
    let mut visible = Vec::new();
    let conflict_pair = if capability.display == DisplayState::Conflicting {
        capability.reasons.len().min(2)
    } else {
        0
    };
    for (index, reason) in capability.reasons.iter().enumerate() {
        if remaining_rows == 0 {
            break;
        }
        let has_location = reason.fact.location.is_some();
        let pair_rows_to_reserve = conflict_pair.saturating_sub(index + 1);
        let show_location = has_location && remaining_rows >= 2 + pair_rows_to_reserve;
        remaining_rows -= 1 + usize::from(show_location);
        visible.push((reason, show_location));
    }
    visible
}

fn project_explain_command(app: &App, capability_id: &str, width: usize) -> Option<String> {
    if !app.project_command_hints() || !crate::reasoning::is_portable_capability_id(capability_id) {
        return None;
    }
    let command = format!("proof-lantern explain {capability_id}");
    (UnicodeWidthStr::width(command.as_str()) <= width).then_some(command)
}

fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    wide: bool,
    inspector_open: bool,
    palette: Palette,
) {
    let inspect = if inspector_open {
        "E CLOSE"
    } else {
        "E INSPECT"
    };
    let mut spans = vec![key("←→ NODE", palette), Span::raw("   ")];
    if !wide {
        spans.extend([key(inspect, palette), Span::raw("   ")]);
    }
    spans.extend([
        key("G FOCUS", palette),
        Span::raw("   "),
        key("Q EXIT", palette),
    ]);
    let line = Line::from(spans);
    frame.render_widget(
        Paragraph::new(line).block(instrument_block(
            " COMMAND ",
            palette.primary,
            palette.background,
        )),
        area,
    );
}

fn render_minimum_size(frame: &mut Frame<'_>, area: Rect, palette: Palette) {
    let text = vec![
        Line::from(Span::styled(
            "PROOF LANTERN // DISPLAY LIMIT",
            Style::default().fg(palette.hot).bold(),
        )),
        Line::default(),
        Line::from(format!("CURRENT  {:03} × {:02}", area.width, area.height)),
        Line::from(format!("REQUIRED {MINIMUM_WIDTH:03} × {MINIMUM_HEIGHT:02}")),
        Line::default(),
        Line::from("Resize the terminal to see the accepted core journey."),
        Line::from("Q EXIT"),
    ];
    let width = area.width.min(54);
    let height = area.height.min(11);
    let centered = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(palette.text))
            .block(instrument_block(
                " DISPLAY LIMIT ",
                palette.warning,
                palette.panel,
            )),
        centered,
    );
}

fn connector_for(current: DisplayState, next: DisplayState) -> &'static str {
    if next == DisplayState::Missing {
        "━━╸   "
    } else if current == DisplayState::Missing
        || matches!(
            current,
            DisplayState::ProofFailed | DisplayState::Conflicting
        )
    {
        " ┄┄┄  "
    } else {
        "━━━━━━"
    }
}

fn support_window_start(total: usize, selected: usize, capacity: usize) -> usize {
    if capacity == 0 || total <= capacity {
        return 0;
    }
    selected
        .saturating_sub(capacity / 2)
        .min(total.saturating_sub(capacity))
}

fn connector_color(
    current: DisplayState,
    next: DisplayState,
    palette: Palette,
) -> ratatui::style::Color {
    if next == DisplayState::Missing
        || matches!(
            current,
            DisplayState::Missing | DisplayState::ProofFailed | DisplayState::Conflicting
        )
    {
        palette.warning
    } else {
        palette.primary
    }
}

fn state_style(state: DisplayState, palette: Palette) -> Style {
    let color = match state {
        DisplayState::Proven => palette.success,
        DisplayState::BuiltUnproven => palette.primary,
        DisplayState::Missing | DisplayState::ProofFailed | DisplayState::Conflicting => {
            palette.warning
        }
        DisplayState::Unknown => palette.unknown,
    };
    Style::default().fg(color)
}

fn heading(label: &'static str, palette: Palette) -> Line<'static> {
    Line::from(Span::styled(
        label,
        Style::default().fg(palette.primary).bold(),
    ))
}

fn key(label: &str, palette: Palette) -> Span<'_> {
    Span::styled(label, Style::default().fg(palette.text).bold())
}

fn instrument_block<'a>(
    title: &'a str,
    border: ratatui::style::Color,
    panel: ratatui::style::Color,
) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(panel))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{evaluate, load_project};

    #[test]
    fn missing_capability_physically_breaks_the_core_path() {
        assert_eq!(
            connector_for(DisplayState::BuiltUnproven, DisplayState::Missing),
            "━━╸   "
        );
        assert_eq!(
            connector_for(DisplayState::Missing, DisplayState::Unknown),
            " ┄┄┄  "
        );
        assert_eq!(
            connector_for(DisplayState::Proven, DisplayState::BuiltUnproven),
            "━━━━━━"
        );
    }

    #[test]
    fn supporting_window_keeps_the_selected_capability_visible() {
        assert_eq!(support_window_start(6, 0, 3), 0);
        assert_eq!(support_window_start(6, 4, 3), 3);
        assert_eq!(support_window_start(2, 1, 3), 0);
    }

    #[test]
    fn command_hints_require_a_safe_id_and_enough_untruncated_width() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
        let (spec, observations) = load_project(root).unwrap();
        let app = App::new(evaluate(spec, observations).unwrap()).with_project_command_hints();

        assert_eq!(
            project_explain_command(&app, "reopen", 41).as_deref(),
            Some("proof-lantern explain reopen")
        );
        assert_eq!(project_explain_command(&app, "unsafe id", 80), None);
        assert_eq!(
            project_explain_command(&app, "1starts-with-digit", 80),
            None
        );
        assert_eq!(
            project_explain_command(&app, "-starts-like-an-option", 80),
            None
        );
        assert_eq!(
            project_explain_command(&app, "a-safe-but-too-long-capability-id", 41),
            None
        );
    }
}
