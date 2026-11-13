use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use super::{
    CanvasCommand, CanvasId, CanvasOwner, ImageSource, Size, Snapshot, TextMetrics, WidgetId,
    target::{CanvasTarget, VelloCpuTarget},
};

pub type SharedCanvasStore = Rc<RefCell<CanvasStore>>;

pub struct CanvasEntry {
    pub id: CanvasId,
    pub owner: CanvasOwner,
    pub size: Size,
    pub target: Box<dyn CanvasTarget>,
    pub revision: u64,
    pub dirty: bool,
    pub origin_clean: bool,
}

/// In-process equivalent of Servo's canvas paint thread map.
///
/// It is deliberately a narrow shared subsystem: JS can send commands and read
/// snapshots by `CanvasId`, but it never sees app state or platform objects.
pub struct CanvasStore {
    next_id: u64,
    canvases: HashMap<CanvasId, CanvasEntry>,
}

impl Default for CanvasStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            canvases: HashMap::new(),
        }
    }

    pub fn create(&mut self, size: Size, owner: CanvasOwner) -> CanvasId {
        let id = CanvasId(self.next_id);
        self.next_id += 1;
        self.canvases.insert(
            id,
            CanvasEntry {
                id,
                owner,
                size,
                target: Box::new(VelloCpuTarget::new(size)),
                revision: 0,
                dirty: false,
                origin_clean: true,
            },
        );
        id
    }

    pub fn destroy(&mut self, id: CanvasId) {
        self.canvases.remove(&id);
    }

    pub fn exists(&self, id: CanvasId) -> bool {
        self.canvases.contains_key(&id)
    }

    pub fn size(&self, id: CanvasId) -> Option<Size> {
        self.canvases.get(&id).map(|entry| entry.size)
    }

    pub fn owner(&self, id: CanvasId) -> Option<CanvasOwner> {
        self.canvases.get(&id).map(|entry| entry.owner)
    }

    pub fn set_owner(&mut self, id: CanvasId, owner: CanvasOwner) {
        if let Some(entry) = self.canvases.get_mut(&id) {
            entry.owner = owner;
        }
    }

    pub fn execute(&mut self, id: CanvasId, command: CanvasCommand) {
        let command = self.resolve_image_source(command);
        if let Some(entry) = self.canvases.get_mut(&id) {
            entry.target.execute(command);
            entry.revision = entry.revision.wrapping_add(1);
            entry.dirty = true;
        }
    }

    fn resolve_image_source(&mut self, command: CanvasCommand) -> CanvasCommand {
        match command {
            CanvasCommand::DrawImage {
                source: ImageSource::Canvas(source_id),
                source_rect,
                dest_rect,
                smoothing,
                transform,
            } => {
                let snapshot = self
                    .snapshot(source_id)
                    .unwrap_or_else(|| Arc::new(Snapshot::empty(Size::default())));
                CanvasCommand::DrawImage {
                    source: ImageSource::Bitmap(snapshot),
                    source_rect,
                    dest_rect,
                    smoothing,
                    transform,
                }
            }
            command => command,
        }
    }

    pub fn recreate(&mut self, id: CanvasId, size: Size) {
        if let Some(entry) = self.canvases.get_mut(&id) {
            entry.size = size;
            entry.target.execute(CanvasCommand::Recreate(size));
            entry.revision = entry.revision.wrapping_add(1);
            entry.dirty = true;
        }
    }

    /// Reset the bitmap without resetting the JS drawing state.
    pub fn clear(&mut self, id: CanvasId) {
        if let Some(entry) = self.canvases.get_mut(&id) {
            entry.target.clear();
            entry.revision = entry.revision.wrapping_add(1);
            entry.dirty = true;
        }
    }

    pub fn snapshot(&mut self, id: CanvasId) -> Option<Arc<Snapshot>> {
        self.canvases
            .get_mut(&id)
            .map(|entry| entry.target.snapshot())
    }

    pub fn get_image_data(
        &mut self,
        id: CanvasId,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Option<Snapshot> {
        let snapshot = self.snapshot(id)?;
        Some(snapshot.crop(x, y, width, height))
    }

    pub fn put_image_data(&mut self, id: CanvasId, x: i32, y: i32, snapshot: Arc<Snapshot>) {
        self.execute(id, CanvasCommand::PutImageData { x, y, snapshot });
    }

    pub fn find_for_widget(&self, widget: WidgetId) -> Option<CanvasId> {
        // Prefer the onscreen canvas. If the widget transferred control to an
        // offscreen canvas, the offscreen entry is the only one with a
        // placeholder pointing at it.
        self.canvases
            .values()
            .find(|entry| entry.owner == CanvasOwner::Onscreen { widget })
            .map(|entry| entry.id)
            .or_else(|| {
                self.canvases
                    .values()
                    .find(|entry| {
                        entry.owner
                            == CanvasOwner::Offscreen {
                                placeholder: Some(widget),
                            }
                    })
                    .map(|entry| entry.id)
            })
    }

    pub fn dirty_ids(&self) -> Vec<CanvasId> {
        self.canvases
            .values()
            .filter(|entry| entry.dirty)
            .map(|entry| entry.id)
            .collect()
    }

    pub fn mark_clean(&mut self, id: CanvasId) {
        if let Some(entry) = self.canvases.get_mut(&id) {
            entry.dirty = false;
        }
    }

    pub fn revision(&self, id: CanvasId) -> Option<u64> {
        self.canvases.get(&id).map(|entry| entry.revision)
    }

    pub fn measure_text(
        &mut self,
        id: CanvasId,
        text: &str,
        font_size: f32,
        font_family: &str,
        font_weight: f32,
        max_width: Option<f64>,
    ) -> TextMetrics {
        self.canvases
            .get_mut(&id)
            .map(|entry| {
                entry
                    .target
                    .measure_text(text, font_size, font_family, font_weight, max_width)
            })
            .unwrap_or_default()
    }

    pub fn origin_clean(&self, id: CanvasId) -> bool {
        self.canvases
            .get(&id)
            .map(|entry| entry.origin_clean)
            .unwrap_or(true)
    }
}
