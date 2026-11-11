use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(String);

impl WidgetId {
    pub fn builtin(name: &str) -> Self {
        Self(format!("builtin:{name}"))
    }

    pub fn plugin(id: &str) -> Self {
        Self(format!("plugin:{id}"))
    }

    pub fn parse(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn builtin_name(&self) -> Option<&str> {
        self.0.strip_prefix("builtin:")
    }
}

impl From<&str> for WidgetId {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

impl From<String> for WidgetId {
    fn from(value: String) -> Self {
        Self::parse(value)
    }
}

#[derive(Debug, Clone)]
pub struct WidgetDescriptor {
    pub id: WidgetId,
    pub label: String,
    pub description: String,
    pub source: String,
    pub available: bool,
    pub status: Option<String>,
    pub min_height: u16,
}

#[derive(Debug, Clone)]
pub struct WidgetCatalog {
    entries: Vec<WidgetDescriptor>,
}

impl WidgetCatalog {
    pub fn builtins() -> Self {
        let definitions = [
            ("cpu_graph", "CPU", "Total CPU usage over time", 8),
            ("cpu_cores", "CPU Cores", "Per-core utilization", 6),
            ("memory", "Memory", "Used and available memory", 6),
            ("swap", "Swap", "Swap usage", 4),
            ("sensors", "Sensors", "Temperatures and fans", 6),
            ("power", "Power", "Power consumption", 6),
            ("cpu_tiers", "CPU Tiers", "Frequency clusters", 5),
            ("battery", "Battery", "Battery health and charge", 5),
            ("network_graph", "Network", "Interface traffic history", 6),
            ("disk_usage", "Disk Usage", "Mounted filesystem usage", 6),
            ("disk_health", "Disk Health", "SMART device health", 6),
            ("processes", "Processes", "Sortable process table", 10),
            ("connections", "Connections", "Network socket summary", 6),
            ("network_table", "Network Table", "Per-interface traffic", 6),
        ];
        Self {
            entries: definitions
                .into_iter()
                .map(|(name, label, description, min_height)| WidgetDescriptor {
                    id: WidgetId::builtin(name),
                    label: label.into(),
                    description: description.into(),
                    source: "built-in".into(),
                    available: true,
                    status: None,
                    min_height,
                })
                .collect(),
        }
    }

    pub fn entries(&self) -> &[WidgetDescriptor] {
        &self.entries
    }

    pub fn get(&self, id: &WidgetId) -> Option<&WidgetDescriptor> {
        self.entries.iter().find(|entry| &entry.id == id)
    }

    pub fn get_mut(&mut self, id: &WidgetId) -> Option<&mut WidgetDescriptor> {
        self.entries.iter_mut().find(|entry| &entry.id == id)
    }

    pub fn insert(&mut self, descriptor: WidgetDescriptor) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == descriptor.id) {
            *existing = descriptor;
        } else {
            self.entries.push(descriptor);
        }
    }

    pub fn add_missing(&mut self, ids: &[WidgetId]) {
        let mut seen: HashSet<WidgetId> =
            self.entries.iter().map(|entry| entry.id.clone()).collect();
        for id in ids {
            if seen.insert(id.clone()) {
                self.entries.push(WidgetDescriptor {
                    label: id.as_str().to_owned(),
                    description: "This configured widget is not available.".into(),
                    source: if id.as_str().starts_with("plugin:") {
                        "plugin".into()
                    } else {
                        "unknown".into()
                    },
                    id: id.clone(),
                    available: false,
                    status: Some("missing".into()),
                    min_height: 5,
                });
            }
        }
    }

    pub fn default_ids() -> Vec<WidgetId> {
        Self::builtins()
            .entries
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    }
}
