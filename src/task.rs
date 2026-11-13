//! A small HTML-style task queue.
//!
//! The runtime does not call into JS directly from platform callbacks. Every
//! callback is queued as a task with a [`TaskSource`], then the event loop runs
//! one task at a time and performs a microtask checkpoint after each one.

use std::collections::VecDeque;

use crate::canvas::WidgetId;

/// The task sources this runtime currently distinguishes.
///
/// This mirrors the HTML distinction between task sources without implementing
/// the full browser set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskSource {
    /// Pointer input and other user-interaction events.
    UserInteraction,
    /// Rendering work: resize steps and animation-frame callbacks.
    Rendering,
    /// `setTimeout` / `setInterval`.
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
            PointerKind::Enter => "pointerenter",
            PointerKind::Leave => "pointerleave",
            PointerKind::Move => "pointermove",
            PointerKind::Down => "pointerdown",
            PointerKind::Up => "pointerup",
            PointerKind::Click => "click",
            PointerKind::ContextMenu => "contextmenu",
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
    AnimationFrame,
    Timer,
}

#[derive(Default)]
pub struct TaskQueue {
    user_interaction: VecDeque<Task>,
    rendering: VecDeque<Task>,
    timer: VecDeque<Task>,
    animation_frame_queued: bool,
    timer_queued: bool,
}

impl TaskQueue {
    pub fn push(&mut self, source: TaskSource, task: Task) {
        match task {
            Task::AnimationFrame => {
                if self.animation_frame_queued {
                    return;
                }
                self.animation_frame_queued = true;
            }
            Task::Timer => {
                if self.timer_queued {
                    return;
                }
                self.timer_queued = true;
            }
            _ => {}
        }

        match source {
            TaskSource::UserInteraction => self.user_interaction.push_back(task),
            TaskSource::Rendering => self.rendering.push_back(task),
            TaskSource::Timer => self.timer.push_back(task),
        }
    }

    /// Pick the next task.
    ///
    /// User interaction is handled first, then rendering, then timers. This is
    /// an implementation-defined ordering, like a browser's task-source
    /// selection; the important part is that each queue is a real task source
    /// and not a direct JS callback from the platform layer.
    pub fn pop(&mut self) -> Option<Task> {
        let task = self
            .user_interaction
            .pop_front()
            .or_else(|| self.rendering.pop_front())
            .or_else(|| self.timer.pop_front());

        match task {
            Some(Task::AnimationFrame) => self.animation_frame_queued = false,
            Some(Task::Timer) => self.timer_queued = false,
            _ => {}
        }
        task
    }

    pub fn is_empty(&self) -> bool {
        self.user_interaction.is_empty() && self.rendering.is_empty() && self.timer.is_empty()
    }

    pub fn has_timer(&self) -> bool {
        self.timer_queued
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_sources_are_drained_in_priority_order() {
        let mut queue = TaskQueue::default();
        queue.push(TaskSource::Timer, Task::Timer);
        queue.push(TaskSource::Rendering, Task::AnimationFrame);
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
        assert!(matches!(queue.pop(), Some(Task::AnimationFrame)));
        assert!(matches!(queue.pop(), Some(Task::Timer)));
        assert!(queue.is_empty());
    }

    #[test]
    fn animation_frame_and_timer_tasks_are_coalesced() {
        let mut queue = TaskQueue::default();
        queue.push(TaskSource::Rendering, Task::AnimationFrame);
        queue.push(TaskSource::Rendering, Task::AnimationFrame);
        queue.push(TaskSource::Timer, Task::Timer);
        queue.push(TaskSource::Timer, Task::Timer);

        assert!(queue.has_timer());
        assert!(matches!(queue.pop(), Some(Task::AnimationFrame)));
        assert!(matches!(queue.pop(), Some(Task::Timer)));
        assert!(queue.is_empty());
    }
}
