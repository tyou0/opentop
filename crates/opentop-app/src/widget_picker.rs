use crate::widgets::{WidgetCatalog, WidgetId};

#[derive(Debug, Clone)]
pub struct WidgetPicker {
    draft: Vec<WidgetId>,
    selected: Option<WidgetId>,
}

impl WidgetPicker {
    pub fn new(enabled: &[WidgetId]) -> Self {
        Self {
            draft: enabled.to_vec(),
            selected: enabled.first().cloned(),
        }
    }

    pub fn draft(&self) -> &[WidgetId] {
        &self.draft
    }

    pub fn selected(&self) -> Option<&WidgetId> {
        self.selected.as_ref()
    }

    pub fn ordered_items(&self, catalog: &WidgetCatalog) -> Vec<WidgetId> {
        let mut items = self.draft.clone();
        let mut disabled: Vec<_> = catalog
            .entries()
            .iter()
            .filter(|entry| !self.draft.contains(&entry.id))
            .map(|entry| entry.id.clone())
            .collect();
        disabled.sort_by(|a, b| {
            let a_label = catalog
                .get(a)
                .map_or(a.as_str(), |entry| entry.label.as_str());
            let b_label = catalog
                .get(b)
                .map_or(b.as_str(), |entry| entry.label.as_str());
            a_label.cmp(b_label).then_with(|| a.cmp(b))
        });
        items.extend(disabled);
        items
    }

    pub fn move_selection(&mut self, delta: isize, catalog: &WidgetCatalog) {
        let items = self.ordered_items(catalog);
        if items.is_empty() {
            self.selected = None;
            return;
        }
        let index = self
            .selected
            .as_ref()
            .and_then(|id| items.iter().position(|item| item == id))
            .unwrap_or(0)
            .saturating_add_signed(delta)
            .min(items.len() - 1);
        self.selected = Some(items[index].clone());
    }

    pub fn toggle_selected(&mut self) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        if let Some(index) = self.draft.iter().position(|id| id == &selected) {
            self.draft.remove(index);
        } else {
            self.draft.push(selected);
        }
    }

    pub fn move_selected(&mut self, delta: isize) {
        let Some(index) = self
            .selected
            .as_ref()
            .and_then(|id| self.draft.iter().position(|item| item == id))
        else {
            return;
        };
        let target = index.saturating_add_signed(delta).min(self.draft.len() - 1);
        self.draft.swap(index, target);
    }

    pub fn restore_defaults(&mut self) {
        self.draft = WidgetCatalog::default_ids();
        if self.selected.is_none() {
            self.selected = self.draft.first().cloned();
        }
    }

    pub fn apply(self) -> Vec<WidgetId> {
        self.draft
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_draft_toggle_reorder_and_cancel_are_isolated() {
        let catalog = WidgetCatalog::builtins();
        let initial = vec![WidgetId::builtin("cpu_graph"), WidgetId::builtin("memory")];
        let mut picker = WidgetPicker::new(&initial);
        picker.selected = Some(WidgetId::builtin("memory"));
        picker.move_selected(-1);
        assert_eq!(picker.draft()[0].as_str(), "builtin:memory");
        picker.toggle_selected();
        assert_eq!(initial[0].as_str(), "builtin:cpu_graph");
        assert!(!picker.draft().contains(&WidgetId::builtin("memory")));
        assert!(!picker.ordered_items(&catalog).is_empty());
    }
}
