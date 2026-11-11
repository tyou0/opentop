use ratatui::layout::{Position, Rect};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::widgets::{WidgetCatalog, WidgetId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRect {
    pub id: WidgetId,
    pub area: Rect,
    pub row: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PanelLayout {
    pub panels: Vec<PanelRect>,
    pub scroll_row: usize,
    pub columns: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutSettings {
    pub columns: Option<usize>,
    pub widths: BTreeMap<String, u16>,
    pub heights: BTreeMap<String, u16>,
}

impl PanelLayout {
    pub fn hit_test(&self, x: u16, y: u16) -> Option<&PanelRect> {
        self.panels
            .iter()
            .find(|panel| panel.area.contains(Position::new(x, y)))
    }
}

pub fn compute(
    ids: &[WidgetId],
    catalog: &WidgetCatalog,
    area: Rect,
    requested_scroll: usize,
    focused: Option<&WidgetId>,
    wrap: bool,
    settings: &LayoutSettings,
) -> PanelLayout {
    let body = Rect {
        x: area.x,
        y: area.y.saturating_add(1),
        width: area.width,
        height: area.height.saturating_sub(2),
    };
    if ids.is_empty() || body.is_empty() {
        return PanelLayout::default();
    }
    let auto_columns = || {
        if wrap {
            wrap_columns(ids, catalog, body)
                .unwrap_or_else(|| (area.width / 40).clamp(1, 3) as usize)
        } else {
            (area.width / 40).clamp(1, 3) as usize
        }
    };
    let columns = settings
        .columns
        .unwrap_or_else(auto_columns)
        .clamp(1, 8)
        .min(ids.len())
        .min(body.width as usize);
    let rows: Vec<&[WidgetId]> = ids.chunks(columns).collect();
    let row_heights: Vec<u16> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|id| catalog.get(id).map_or(5, |entry| entry.min_height))
                .max()
                .unwrap_or(1)
        })
        .collect();
    let focused_row = focused
        .and_then(|id| ids.iter().position(|candidate| candidate == id))
        .map(|index| index / columns);
    let mut scroll_row = requested_scroll.min(rows.len().saturating_sub(1));
    if let Some(focused_row) = focused_row {
        if focused_row < scroll_row
            || !row_visible(focused_row, scroll_row, &row_heights, body.height)
        {
            scroll_row = focused_row;
        }
    }

    let mut visible_rows = Vec::new();
    let mut used = 0u16;
    for (row_index, &row_height) in row_heights.iter().enumerate().skip(scroll_row) {
        let available = body.height.saturating_sub(used);
        if available == 0 {
            break;
        }
        if !visible_rows.is_empty() && row_height > available {
            break;
        }
        visible_rows.push(row_index);
        used = used.saturating_add(row_height.min(available));
    }
    let height_weights: Vec<u32> = visible_rows
        .iter()
        .map(|&row| {
            u32::from(row_heights[row]) * u32::from(weight(&settings.heights, &rows[row][0]))
        })
        .collect();
    let heights = weighted_lengths(body.height, &height_weights, 3);
    let mut panels = Vec::new();
    let mut y = body.y;
    for (visible_index, &row_index) in visible_rows.iter().enumerate() {
        let row_height = heights[visible_index];
        let width_weights: Vec<u32> = rows[row_index]
            .iter()
            .map(|id| u32::from(weight(&settings.widths, id)))
            .collect();
        let widths = weighted_lengths(body.width, &width_weights, 1);
        let mut x = body.x;
        for (column, id) in rows[row_index].iter().enumerate() {
            let width = widths[column];
            panels.push(PanelRect {
                id: id.clone(),
                area: Rect::new(x, y, width, row_height),
                row: row_index,
            });
            x = x.saturating_add(width);
        }
        y = y.saturating_add(row_height);
    }
    PanelLayout {
        panels,
        scroll_row,
        columns,
    }
}

/// Choose the fewest readable columns that fit the recommended row heights.
fn wrap_columns(ids: &[WidgetId], catalog: &WidgetCatalog, body: Rect) -> Option<usize> {
    let max_columns = ((body.width / 24).clamp(1, 8) as usize).min(ids.len());
    for columns in 1..=max_columns {
        let rows: Vec<&[WidgetId]> = ids.chunks(columns).collect();
        let total_height: u32 = rows
            .iter()
            .map(|row| {
                u32::from(
                    row.iter()
                        .map(|id| catalog.get(id).map_or(5, |entry| entry.min_height))
                        .max()
                        .unwrap_or(1),
                )
            })
            .sum();
        if total_height <= u32::from(body.height) {
            return Some(columns);
        }
    }
    None
}

fn weight(weights: &BTreeMap<String, u16>, id: &WidgetId) -> u16 {
    weights
        .get(id.as_str())
        .copied()
        .unwrap_or(100)
        .clamp(10, 1000)
}

fn weighted_lengths(total: u16, weights: &[u32], minimum: u16) -> Vec<u16> {
    if weights.is_empty() {
        return Vec::new();
    }
    let count = weights.len() as u32;
    let minimum = if count * u32::from(minimum) <= u32::from(total) {
        minimum
    } else {
        1
    };
    let remaining = u32::from(total).saturating_sub(count * u32::from(minimum));
    let sum: u64 = weights
        .iter()
        .map(|weight| u64::from((*weight).max(1)))
        .sum();
    let mut cumulative = 0u64;
    let mut previous = 0u64;
    weights
        .iter()
        .map(|weight| {
            cumulative += u64::from((*weight).max(1));
            let share = u64::from(remaining) * cumulative / sum;
            let length = u64::from(minimum) + share - previous;
            previous = share;
            length as u16
        })
        .collect()
}

fn row_visible(row: usize, scroll: usize, heights: &[u16], viewport_height: u16) -> bool {
    let used = heights[scroll..row]
        .iter()
        .fold(0u16, |sum, height| sum.saturating_add(*height));
    let remaining = viewport_height.saturating_sub(used);
    remaining > 0 && (row == scroll || heights[row] <= remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_rules_cover_small_and_wide_terminals() {
        let ids = WidgetCatalog::default_ids();
        let catalog = WidgetCatalog::builtins();
        for (width, height, expected) in [(40, 12, 1), (80, 24, 2), (120, 40, 3), (160, 48, 3)] {
            let layout = compute(
                &ids,
                &catalog,
                Rect::new(0, 0, width, height),
                0,
                ids.first(),
                false,
                &LayoutSettings::default(),
            );
            assert_eq!(
                layout.panels.iter().filter(|panel| panel.row == 0).count(),
                expected
            );
            assert_eq!(
                layout
                    .panels
                    .iter()
                    .filter(|panel| panel.row == 0)
                    .map(|panel| panel.area.width)
                    .sum::<u16>(),
                width
            );
        }
        assert!(
            compute(
                &ids,
                &catalog,
                Rect::new(0, 0, 1, 1),
                0,
                None,
                false,
                &LayoutSettings::default(),
            )
            .panels
            .is_empty()
        );
    }

    #[test]
    fn focus_scrolls_a_whole_row_into_the_viewport() {
        let ids = WidgetCatalog::default_ids();
        let catalog = WidgetCatalog::builtins();
        let target = ids[10].clone();
        let layout = compute(
            &ids,
            &catalog,
            Rect::new(0, 0, 80, 12),
            0,
            Some(&target),
            false,
            &LayoutSettings::default(),
        );
        assert_eq!(layout.scroll_row, 5);
        assert!(layout.panels.iter().any(|panel| panel.id == target));
        let hit = layout
            .hit_test(layout.panels[0].area.x, layout.panels[0].area.y)
            .unwrap();
        assert_eq!(hit.id, layout.panels[0].id);
    }

    #[test]
    fn wrap_fits_all_widgets_without_scroll() {
        let ids = WidgetCatalog::default_ids();
        let catalog = WidgetCatalog::builtins();
        let layout = compute(
            &ids,
            &catalog,
            Rect::new(0, 0, 160, 48),
            0,
            None,
            true,
            &LayoutSettings::default(),
        );
        assert_eq!(layout.scroll_row, 0);
        let visible_ids: Vec<WidgetId> = layout.panels.iter().map(|p| p.id.clone()).collect();
        assert_eq!(visible_ids, ids);
    }

    #[test]
    fn rows_fill_the_viewport_and_respect_saved_weights() {
        let ids = WidgetCatalog::default_ids()[..5].to_vec();
        let catalog = WidgetCatalog::builtins();
        let area = Rect::new(0, 0, 100, 36);
        let mut settings = LayoutSettings {
            columns: Some(2),
            ..Default::default()
        };
        let original = compute(&ids, &catalog, area, 0, None, true, &settings);
        assert_eq!(original.panels.len(), 5);
        for row in 0..3 {
            assert_eq!(
                original
                    .panels
                    .iter()
                    .filter(|panel| panel.row == row)
                    .map(|panel| panel.area.width)
                    .sum::<u16>(),
                area.width
            );
        }
        let bottom = original.panels.last().unwrap().area;
        assert_eq!(bottom.y + bottom.height, area.height - 1);

        settings.widths.insert(ids[0].as_str().into(), 160);
        settings.heights.insert(ids[0].as_str().into(), 160);
        let resized = compute(&ids, &catalog, area, 0, None, true, &settings);
        assert!(resized.panels[0].area.width > original.panels[0].area.width);
        assert!(resized.panels[0].area.height > original.panels[0].area.height);
        assert_eq!(resized.panels.last().unwrap().area.width, area.width);
    }
}
