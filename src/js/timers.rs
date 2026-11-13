use std::time::Instant;

use rquickjs::{Function, Persistent};

pub struct TimerEntry {
    pub id: u32,
    pub callback: Persistent<Function<'static>>,
    pub due_ms: f64,
    pub interval_ms: Option<f64>,
}

impl Clone for TimerEntry {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            callback: self.callback.clone(),
            due_ms: self.due_ms,
            interval_ms: self.interval_ms,
        }
    }
}

/// Host-side timer registry backing `setTimeout` / `setInterval`.
///
/// Timers are kept out of the JS heap so the event loop can inspect the next
/// deadline and schedule a real `TaskSource::Timer` task.
pub struct TimerRegistry {
    start: Instant,
    next_id: u32,
    timers: Vec<TimerEntry>,
}

impl TimerRegistry {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            next_id: 1,
            timers: Vec::new(),
        }
    }

    pub fn now_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    pub fn set(
        &mut self,
        callback: Persistent<Function<'static>>,
        delay_ms: f64,
        repeat: bool,
    ) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);

        // Match browsers: clamp to the i32 millisecond range and never allow
        // negative or non-finite delays.
        let delay_ms = if delay_ms.is_finite() {
            delay_ms.clamp(0.0, i32::MAX as f64)
        } else {
            0.0
        };
        let due_ms = self.now_ms() + delay_ms;

        self.timers.push(TimerEntry {
            id,
            callback,
            due_ms,
            interval_ms: repeat.then_some(delay_ms),
        });
        id
    }

    pub fn clear(&mut self, id: u32) -> bool {
        let before = self.timers.len();
        self.timers.retain(|timer| timer.id != id);
        self.timers.len() != before
    }

    pub fn next_deadline_ms(&self) -> Option<f64> {
        self.timers
            .iter()
            .map(|timer| timer.due_ms)
            .min_by(f64::total_cmp)
    }

    /// Remove and return one due timer. Repeating timers are reinserted before
    /// the callback runs, so `clearInterval(id)` from inside the callback works.
    pub fn take_due(&mut self, now_ms: f64) -> Option<TimerEntry> {
        let index = self
            .timers
            .iter()
            .enumerate()
            .filter(|(_, timer)| timer.due_ms <= now_ms)
            .min_by(|(_, left), (_, right)| left.due_ms.total_cmp(&right.due_ms))
            .map(|(index, _)| index)?;

        let entry = self.timers.remove(index);
        if let Some(interval_ms) = entry.interval_ms {
            let mut next = entry.clone();
            next.due_ms = (entry.due_ms + interval_ms.max(1.0)).max(now_ms + 1.0);
            self.timers.push(next);
        }
        Some(entry)
    }
}
