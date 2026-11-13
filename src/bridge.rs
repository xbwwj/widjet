use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

use crate::canvas::WidgetId;

/// Platform-neutral description of a widget surface.
#[derive(Clone, Debug)]
pub struct WidgetSpec {
    pub width: u32,
    pub height: u32,
    pub anchor: u32,
    pub exclusive_zone: i32,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
}

impl Default for WidgetSpec {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            anchor: 0,
            exclusive_zone: 0,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum WidgetPatch {
    SetSize {
        width: u32,
        height: u32,
    },
    SetAnchor(u32),
    SetExclusiveZone(i32),
    SetMargin {
        top: i32,
        right: i32,
        bottom: i32,
        left: i32,
    },
}

/// Commands from the JS world to the app/platform world.
///
/// These are deferred and drained by the app tick, which keeps JS constructors
/// and property setters from driving Wayland synchronously.
#[derive(Clone, Debug)]
pub enum BridgeCommand {
    CreateWidget {
        widget_id: WidgetId,
        spec: WidgetSpec,
    },
    DestroyWidget {
        widget_id: WidgetId,
    },
    UpdateWidget {
        widget_id: WidgetId,
        patch: WidgetPatch,
    },
}

#[derive(Default)]
pub struct JsBridge {
    next_widget_id: Cell<WidgetId>,
    commands: RefCell<VecDeque<BridgeCommand>>,
}

impl JsBridge {
    pub fn alloc_widget_id(&self) -> WidgetId {
        let id = self.next_widget_id.get().max(1);
        self.next_widget_id.set(id.saturating_add(1));
        id
    }

    pub fn push(&self, command: BridgeCommand) {
        self.commands.borrow_mut().push_back(command);
    }

    pub fn take_commands(&self) -> Vec<BridgeCommand> {
        self.commands.borrow_mut().drain(..).collect()
    }
}
