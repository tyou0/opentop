use opentop_core::snapshot::{Availability, Snapshot};
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};

pub(crate) const TABS: [&str; 5] = ["CPU", "Memory", "GPU", "Storage", "System"];

pub(crate) fn render(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    selected: usize,
    scroll: &mut u16,
    theme: &Theme,
) {
    let selected = selected.min(TABS.len() - 1);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(format!(" Hardware · {} ", TABS[selected]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Tabs::new(TABS.to_vec())
            .select(selected)
            .divider("  ")
            .style(Style::default().fg(theme.muted).bg(theme.bg))
            .highlight_style(
                Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        areas[0],
    );
    let lines = match selected {
        0 => cpu_lines(snapshot, theme),
        1 => memory_lines(snapshot, theme),
        2 => gpu_lines(snapshot, theme),
        3 => storage_lines(snapshot, theme),
        _ => system_lines(snapshot, theme),
    };
    let max_scroll = lines
        .len()
        .saturating_sub(areas[1].height as usize)
        .min(u16::MAX as usize) as u16;
    *scroll = (*scroll).min(max_scroll);
    frame.render_widget(Paragraph::new(lines).scroll((*scroll, 0)), areas[1]);
    let hint = if max_scroll > 0 {
        "←/→ tabs  ↑/↓ scroll  Esc close"
    } else {
        "←/→ tabs  Esc close"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(theme.muted)),
        areas[2],
    );
}

fn row(label: &str, value: impl Into<String>, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<15}"), Style::default().fg(theme.muted)),
        Span::styled(value.into(), Style::default().fg(theme.fg)),
    ])
}

fn known(value: &str) -> &str {
    if value.trim().is_empty() {
        "Unavailable"
    } else {
        value
    }
}

fn optional<T>(value: Option<T>, format: impl FnOnce(T) -> String) -> String {
    value.map(format).unwrap_or_else(|| "Unavailable".into())
}

fn cpu_lines(snapshot: &Snapshot, theme: &Theme) -> Vec<Line<'static>> {
    let cpu = &snapshot.cpu;
    let system = &snapshot.system;
    let logical = if cpu.cores.is_empty() {
        system.logical_cores
    } else {
        cpu.cores.len()
    };
    let temperature = snapshot
        .sensors
        .temps
        .iter()
        .find(|reading| reading.label.to_ascii_lowercase().contains("cpu"))
        .map(|reading| reading.celsius);
    let mut lines = vec![
        row("Model", known(&cpu.brand), theme),
        row("Vendor", known(&cpu.vendor), theme),
        row("Architecture", known(&system.cpu_arch), theme),
        row(
            "Physical cores",
            optional(system.physical_cores, |count| count.to_string()),
            theme,
        ),
        row(
            "Logical cores",
            optional((logical > 0).then_some(logical), |count| count.to_string()),
            theme,
        ),
        row(
            "Usage",
            optional(
                (!cpu.cores.is_empty()).then_some(cpu.usage_percent),
                |value| format!("{value:.1}%"),
            ),
            theme,
        ),
        row(
            "Frequency",
            optional(
                (cpu.frequency_mhz > 0).then_some(cpu.frequency_mhz),
                |value| format!("{value} MHz"),
            ),
            theme,
        ),
        row(
            "Temperature",
            optional(temperature, |value| format!("{value:.1} °C")),
            theme,
        ),
    ];
    for tier in &snapshot.sensors.cpu_tiers {
        lines.push(row(
            &tier.label,
            format!(
                "{} cores · {} MHz · {:.1}% active",
                tier.cores,
                tier.frequency_mhz,
                tier.active_ratio * 100.0
            ),
            theme,
        ));
    }
    lines
}

fn memory_lines(snapshot: &Snapshot, theme: &Theme) -> Vec<Line<'static>> {
    let memory = &snapshot.memory;
    if memory.total_bytes == 0 {
        return vec![row("Memory", "Unavailable", theme)];
    }
    vec![
        row("Total", units::bytes(memory.total_bytes), theme),
        row(
            "Used",
            format!(
                "{} ({:.1}%)",
                units::bytes(memory.used_bytes),
                memory.usage_percent
            ),
            theme,
        ),
        row("Available", units::bytes(memory.available_bytes), theme),
        row("Free", units::bytes(memory.free_bytes), theme),
        row("Swap total", units::bytes(memory.total_swap_bytes), theme),
        row(
            "Swap used",
            format!(
                "{} ({:.1}%)",
                units::bytes(memory.used_swap_bytes),
                memory.swap_percent
            ),
            theme,
        ),
        row("Swap free", units::bytes(memory.free_swap_bytes), theme),
    ]
}

fn gpu_lines(snapshot: &Snapshot, theme: &Theme) -> Vec<Line<'static>> {
    let Some(gpu) = &snapshot.sensors.gpu else {
        let status = if snapshot.sensors.source.is_empty() {
            "Waiting for sensor readings"
        } else {
            "Unavailable on this system"
        };
        return vec![row("GPU", status, theme)];
    };
    vec![
        row("Model", known(gpu.name.as_deref().unwrap_or("")), theme),
        row(
            "Cores",
            optional(gpu.cores, |value| value.to_string()),
            theme,
        ),
        row(
            "Usage",
            optional(gpu.usage_percent, |value| format!("{value:.1}%")),
            theme,
        ),
        row(
            "Frequency",
            optional(gpu.frequency_mhz, |value| format!("{value} MHz")),
            theme,
        ),
        row(
            "Temperature",
            optional(gpu.temperature_celsius, |value| format!("{value:.1} °C")),
            theme,
        ),
        row(
            "Power",
            optional(gpu.power_watts, |value| format!("{value:.1} W")),
            theme,
        ),
        row(
            "Memory used",
            optional(gpu.memory_used_bytes, units::bytes),
            theme,
        ),
        row(
            "Memory total",
            optional(gpu.memory_total_bytes, units::bytes),
            theme,
        ),
        row("Sensor source", known(&snapshot.sensors.source), theme),
    ]
}

fn storage_lines(snapshot: &Snapshot, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if snapshot.disks.is_empty() {
        lines.push(row("Volumes", "Unavailable", theme));
    }
    for disk in &snapshot.disks {
        lines.extend([
            row("Volume", known(&disk.name), theme),
            row("Mount", known(&disk.mount_point), theme),
            row("Type", known(&disk.disk_type), theme),
            row("Filesystem", known(&disk.file_system), theme),
            row(
                "Used",
                format!(
                    "{} / {} ({:.1}%)",
                    units::bytes(disk.used_bytes),
                    units::bytes(disk.total_bytes),
                    disk.usage_percent
                ),
                theme,
            ),
            row("Available", units::bytes(disk.available_bytes), theme),
            Line::default(),
        ]);
    }
    match &snapshot.disk_health {
        Availability::Available(disks) if !disks.is_empty() => {
            for disk in disks {
                lines.push(row(
                    "Health",
                    format!(
                        "{}: {}",
                        known(disk.model.as_deref().unwrap_or(&disk.device)),
                        known(disk.overall_health.as_deref().unwrap_or(""))
                    ),
                    theme,
                ));
            }
        }
        _ => lines.push(row("Health", "Unavailable", theme)),
    }
    lines
}

fn system_lines(snapshot: &Snapshot, theme: &Theme) -> Vec<Line<'static>> {
    let system = &snapshot.system;
    vec![
        row(
            "Host",
            known(system.hostname.as_deref().unwrap_or("")),
            theme,
        ),
        row(
            "Model",
            known(system.product.as_deref().unwrap_or("")),
            theme,
        ),
        row(
            "Motherboard",
            known(system.motherboard.as_deref().unwrap_or("")),
            theme,
        ),
        row(
            "OS",
            known(
                system
                    .long_os_version
                    .as_deref()
                    .or(system.os_name.as_deref())
                    .unwrap_or(""),
            ),
            theme,
        ),
        row("Kernel", known(&system.kernel_long_version), theme),
        row("Uptime", units::duration(system.uptime_secs), theme),
    ]
}
