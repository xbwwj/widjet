//! The script thread's task queues.
//!
//! Platform callbacks are normalized to `PlatformEvent` and then converted to
//! tasks. JavaScript is only called while `ScriptThread` executes a task.

use std::collections::VecDeque;

use crate::canvas::WidgetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskSource {
    UserInteraction,
    Rendering,
    Timer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Enter,
    Leave,
    Move,
    Down,
    Up,
    Click,
    ContextMenu,
}

impl PointerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "pointerenter",
            Self::Leave => "pointerleave",
            Self::Move => "pointermove",
            Self::Down => "pointerdown",
            Self::Up => "pointerup",
            Self::Click => "click",
            Self::ContextMenu => "contextmenu",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Message {
    /// The platform pointer event
    Pointer {
        widget_id: WidgetId,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: u32,
        buttons: u32,
    },
    /// The platform configure / resize event
    Configure {
        widget_id: WidgetId,
        width: u32,
        height: u32,
    },
    /// The platform close event
    Close {
        widget_id: WidgetId,
    },
    AnimationFrame {
        widget_id: WidgetId,
    },
    Timer,
}

#[derive(Default)]
pub struct MessageQueue {
    user_interaction: VecDeque<Message>,
    rendering: VecDeque<Message>,
    timer: VecDeque<Message>,
    timer_queued: bool,
    next_source: usize,
}

impl MessageQueue {
    pub fn push(&mut self, source: TaskSource, task: Message) {
        if matches!(task, Message::Timer) {
            if self.timer_queued {
                return;
            }
            self.timer_queued = true;
        }

        match source {
            TaskSource::UserInteraction => self.user_interaction.push_back(task),
            TaskSource::Rendering => self.rendering.push_back(task),
            TaskSource::Timer => self.timer.push_back(task),
        }
    }

    /// Select one task using round-robin task-source selection. The HTML
    /// model does not define one global priority among task sources; round-robin
    /// avoids starving timers or rendering when input is continuously queued.
    pub fn pop(&mut self) -> Option<Message> {
        for offset in 0..3 {
            let source = (self.next_source + offset) % 3;
            let task = match source {
                0 => self.user_interaction.pop_front(),
                1 => self.rendering.pop_front(),
                _ => self.timer.pop_front(),
            };
            let Some(task) = task else {
                continue;
            };

            self.next_source = (source + 1) % 3;
            if matches!(task, Message::Timer) {
                self.timer_queued = false;
            }
            return Some(task);
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.user_interaction.is_empty() && self.rendering.is_empty() && self.timer.is_empty()
    }

    /// Discard work that belongs to a surface which has been destroyed.
    /// Timer tasks are global and are intentionally left untouched.
    pub fn remove_widget_tasks(&mut self, widget_id: WidgetId) {
        fn belongs_to(task: &Message, widget_id: WidgetId) -> bool {
            match task {
                Message::Pointer { widget_id: id, .. }
                | Message::Configure { widget_id: id, .. }
                | Message::Close { widget_id: id }
                | Message::AnimationFrame { widget_id: id } => *id == widget_id,
                Message::Timer => false,
            }
        }

        self.user_interaction
            .retain(|task| !belongs_to(task, widget_id));
        self.rendering.retain(|task| !belongs_to(task, widget_id));
    }

    pub fn has_timer(&self) -> bool {
        self.timer_queued
    }
}
