use std::collections::HashMap;

use super::{Size, WidgetId, WidgetSpec};

/// All scheduler and platform-reconciliation state for one WidgetCanvas.
///
/// This is the single WidgetId-keyed registry shared with the JS bindings.
/// The platform backend has its own map of native handles, because those
/// handles belong to the backend and must not be stored in this model.
#[derive(Clone, Copy, Debug)]
pub struct WidgetState {
    pub spec: WidgetSpec,
    pub revision: u64,
    /// `None` means no native surface has been created yet; otherwise this is
    /// the desired-state revision currently applied to the native surface.
    pub applied_revision: Option<u64>,
    /// JS may allocate a WidgetCanvas before asking for a native surface.
    pub materialized: bool,
    /// Current JS-visible size, updated by setters and compositor configure.
    pub size: Size,
    pub last_animation_frame_ms: Option<f64>,
    pub frame_callback_since_ms: Option<f64>,
    pub watchdog_fallback: bool,
    pub animation_frame_task_queued: bool,
    /// Tombstone: JS asked to destroy the surface. ScriptThread performs the
    /// native destruction and removes this entry at the next reconciliation.
    pub removed: bool,
}

pub struct WidgetStore {
    next_id: WidgetId,
    entries: HashMap<WidgetId, WidgetState>,
}

impl Default for WidgetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entries: HashMap::new(),
        }
    }

    pub fn allocate_id(&mut self) -> WidgetId {
        let id = self.next_id.max(1);
        self.next_id = id.wrapping_add(1).max(1);
        id
    }

    pub fn register(&mut self, id: WidgetId, spec: WidgetSpec) {
        self.entries.insert(
            id,
            WidgetState {
                spec,
                revision: 1,
                applied_revision: None,
                materialized: false,
                size: Size::new(spec.width, spec.height),
                last_animation_frame_ms: None,
                frame_callback_since_ms: None,
                watchdog_fallback: false,
                animation_frame_task_queued: false,
                removed: false,
            },
        );
    }

    pub fn mark_materialized(&mut self, id: WidgetId) {
        if let Some(entry) = self.entries.get_mut(&id)
            && !entry.removed
        {
            entry.materialized = true;
        }
    }

    pub fn set_size(&mut self, id: WidgetId, size: Size) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if entry.removed {
                return;
            }
            let changed = entry.size != size
                || entry.spec.width != size.width
                || entry.spec.height != size.height;
            entry.size = size;
            entry.spec.width = size.width;
            entry.spec.height = size.height;
            if changed {
                entry.revision = entry.revision.wrapping_add(1);
            }
        }
    }

    pub fn update_spec(&mut self, id: WidgetId, update: impl FnOnce(&mut WidgetSpec)) {
        let Some(entry) = self.entries.get_mut(&id) else {
            return;
        };
        if entry.removed {
            return;
        }
        let previous = entry.spec;
        update(&mut entry.spec);
        if entry.spec != previous {
            entry.revision = entry.revision.wrapping_add(1);
        }
    }

    pub fn mark_removed(&mut self, id: WidgetId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.removed = true;
        }
    }

    pub fn remove(&mut self, id: WidgetId) -> Option<WidgetState> {
        self.entries.remove(&id)
    }

    pub fn get(&self, id: WidgetId) -> Option<WidgetState> {
        self.entries.get(&id).copied()
    }

    pub fn snapshots(&self) -> Vec<(WidgetId, WidgetState)> {
        self.entries
            .iter()
            .map(|(&id, &state)| (id, state))
            .collect()
    }

    pub fn is_active(&self, id: WidgetId) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|entry| entry.materialized && !entry.removed)
    }

    pub fn update_runtime(&mut self, id: WidgetId, update: impl FnOnce(&mut WidgetState)) {
        if let Some(entry) = self.entries.get_mut(&id) {
            update(entry);
        }
    }
}
