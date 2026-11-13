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
pub enum Task {
    Pointer {
        widget_id: WidgetId,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: u32,
        buttons: u32,
    },
    Resize {
        widget_id: WidgetId,
        width: u32,
        height: u32,
    },
    Close {
        widget_id: WidgetId,
    },
    AnimationFrame {
        widget_id: WidgetId,
    },
    Timer,
}

#[derive(Default)]
pub struct TaskQueue {
    user_interaction: VecDeque<Task>,
    rendering: VecDeque<Task>,
    timer: VecDeque<Task>,
    timer_queued: bool,
    next_source: usize,
}

impl TaskQueue {
    pub fn push(&mut self, source: TaskSource, task: Task) {
        if matches!(task, Task::Timer) {
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
    pub fn pop(&mut self) -> Option<Task> {
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
            if matches!(task, Task::Timer) {
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
        fn belongs_to(task: &Task, widget_id: WidgetId) -> bool {
            match task {
                Task::Pointer { widget_id: id, .. }
                | Task::Resize { widget_id: id, .. }
                | Task::Close { widget_id: id }
                | Task::AnimationFrame { widget_id: id } => *id == widget_id,
                Task::Timer => false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_sources_are_selected_round_robin() {
        let mut queue = TaskQueue::default();
        queue.push(TaskSource::Timer, Task::Timer);
        queue.push(TaskSource::Rendering, Task::AnimationFrame { widget_id: 1 });
        queue.push(
            TaskSource::UserInteraction,
            Task::Pointer {
                widget_id: 1,
                kind: PointerKind::Click,
                x: 0.0,
                y: 0.0,
                button: 0,
                buttons: 0,
            },
        );

        assert!(matches!(queue.pop(), Some(Task::Pointer { .. })));
        assert!(matches!(
            queue.pop(),
            Some(Task::AnimationFrame { widget_id: 1 })
        ));
        assert!(matches!(queue.pop(), Some(Task::Timer)));
        assert!(queue.is_empty());
    }

    #[test]
    fn destroying_widget_discards_its_queued_tasks_only() {
        let mut queue = TaskQueue::default();
        queue.push(
            TaskSource::UserInteraction,
            Task::Pointer {
                widget_id: 7,
                kind: PointerKind::Click,
                x: 0.0,
                y: 0.0,
                button: 0,
                buttons: 0,
            },
        );
        queue.push(TaskSource::Rendering, Task::AnimationFrame { widget_id: 7 });
        queue.push(TaskSource::Rendering, Task::AnimationFrame { widget_id: 8 });
        queue.push(TaskSource::Timer, Task::Timer);

        queue.remove_widget_tasks(7);
        assert!(matches!(
            queue.pop(),
            Some(Task::AnimationFrame { widget_id: 8 })
        ));
        assert!(matches!(queue.pop(), Some(Task::Timer)));
        assert!(queue.is_empty());
    }

    #[test]
    fn timer_tasks_are_coalesced() {
        let mut queue = TaskQueue::default();
        queue.push(TaskSource::Timer, Task::Timer);
        queue.push(TaskSource::Timer, Task::Timer);
        assert!(queue.has_timer());
        assert!(matches!(queue.pop(), Some(Task::Timer)));
        assert!(!queue.has_timer());
        assert!(queue.is_empty());
    }
}
