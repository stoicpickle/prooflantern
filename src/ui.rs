use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{
    App,
    model::{CapabilityAssessment, CapabilityRole, DisplayState, EvidenceSource},
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
            let height = frame_rows[1].height.min(13);
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
    let gap = app
        .project()
        .keystone
        .as_ref()
        .and_then(|item| app.project().capability(&item.capability_id));
    let mut spans = vec![
        Span::styled("PROOF LANTERN", Style::default().fg(palette.hot).bold()),
        Span::styled(" // ", Style::default().fg(palette.grid)),
        Span::styled(
            app.project().project.name.to_uppercase(),
            Style::default().fg(palette.text).bold(),
        ),
    ];
    if let Some(gap) = gap {
        spans.extend([
            Span::styled("   KEYSTONE  ", Style::default().fg(palette.muted)),
            Span::styled(
                gap.intent.label.to_uppercase(),
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
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(5),
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
    render_keystone(frame, rows[4], app, palette);
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
            Line::from(vec![
                Span::raw(format!("{:indent$}└──── ", "")),
                Span::styled(
                    format!(
                        "{} {}",
                        support.display.glyph(),
                        support.map_label().to_uppercase()
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

fn render_keystone(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let Some(gap) = &app.project().keystone else {
        frame.render_widget(
            Paragraph::new("No unresolved core capability.")
                .block(instrument_block(
                    " KEYSTONE GAP ",
                    palette.success,
                    palette.background,
                ))
                .style(Style::default().fg(palette.success)),
            area,
        );
        return;
    };
    let Some(capability) = app.project().capability(&gap.capability_id) else {
        return;
    };
    let impact = if gap.blocked_core_ids.is_empty() {
        "This unresolved core capability directly blocks the project promise.".to_owned()
    } else {
        let labels = gap
            .blocked_core_ids
            .iter()
            .filter_map(|id| app.project().capability(id))
            .map(|item| item.intent.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("The core journey stops here. Downstream: {labels}.")
    };
    let width = usize::from(area.width.saturating_sub(2));
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", capability.display.glyph()),
                state_style(capability.display, palette).bold(),
            ),
            Span::styled(
                capability.intent.label.to_uppercase(),
                Style::default().fg(palette.text).bold(),
            ),
        ]),
        Line::from(Span::styled(impact, Style::default().fg(palette.warning))),
        Line::from(vec![
            Span::styled(
                "PROOF NEEDED  ",
                Style::default().fg(palette.primary).bold(),
            ),
            Span::styled(
                truncate(&capability.intent.proof_needed, width.saturating_sub(14)),
                Style::default().fg(palette.text),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(instrument_block(
            " KEYSTONE GAP ",
            palette.warning,
            palette.background,
        )),
        area,
    );
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
    let width = usize::from(area.width.saturating_sub(4)).max(1);
    let mut lines = vec![
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
        Line::default(),
        heading("WHY", palette),
        Line::from(Span::styled(
            capability.why(),
            Style::default().fg(palette.text),
        )),
        Line::default(),
        heading("EVIDENCE", palette),
    ];
    if capability.reasons.is_empty() {
        lines.push(Line::from(Span::styled(
            "No current evidence recorded.",
            Style::default().fg(palette.muted),
        )));
    } else {
        for reason in capability.reasons.iter().take(3) {
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
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{source}{freshness}  "),
                    Style::default().fg(palette.primary),
                ),
                Span::styled(&reason.fact.summary, Style::default().fg(palette.text)),
            ]));
            if let Some(location) = &reason.fact.location {
                let suffix = match (location.line_start, location.line_end) {
                    (Some(start), Some(end)) => format!(":{start}-{end}"),
                    _ => String::new(),
                };
                lines.push(Line::from(Span::styled(
                    middle_truncate(&format!("{}{suffix}", location.path), width),
                    Style::default().fg(palette.muted),
                )));
            }
        }
    }
    lines.extend([
        Line::default(),
        heading("PROOF NEEDED", palette),
        Line::from(Span::styled(
            &capability.intent.proof_needed,
            Style::default().fg(palette.text),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(instrument_block(
                " INSPECTOR ",
                palette.primary,
                palette.panel,
            )),
        area,
    );
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
        key("G GAP", palette),
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

fn instrument_block(
    title: &'static str,
    border: ratatui::style::Color,
    panel: ratatui::style::Color,
) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(panel))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
