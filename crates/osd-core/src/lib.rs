//! Platform-independent OSD domain model and scrolling engine.

use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    BottomToTop,
    TopToBottom,
}

impl Default for Direction {
    fn default() -> Self { Self::BottomToTop }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
}

impl Message {
    pub fn new(text: impl Into<String>) -> Self { Self { text: text.into() } }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point { pub x: f64, pub y: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size { pub width: f64, pub height: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }

#[derive(Debug, Clone, PartialEq)]
pub struct TextElement {
    pub text: String,
    pub position: Point,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub elements: Vec<TextElement>,
}

#[derive(Debug, Clone, Copy)]
pub struct QueuePolicy {
    /// Maximum number of messages retained. New messages evict the oldest.
    pub capacity: usize,
}

impl QueuePolicy {
    pub fn new(capacity: usize) -> Self { Self { capacity: capacity.max(1) } }
}

#[derive(Debug)]
pub struct MessageBuffer {
    policy: QueuePolicy,
    messages: VecDeque<Message>,
}

impl MessageBuffer {
    pub fn new(policy: QueuePolicy) -> Self {
        Self { policy, messages: VecDeque::with_capacity(policy.capacity) }
    }

    pub fn push(&mut self, message: Message) {
        if self.messages.len() >= self.policy.capacity {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }

    pub fn len(&self) -> usize { self.messages.len() }
    pub fn is_empty(&self) -> bool { self.messages.is_empty() }
    pub fn capacity(&self) -> usize { self.policy.capacity }
    pub fn iter(&self) -> impl Iterator<Item = &Message> { self.messages.iter() }
    pub fn clear(&mut self) { self.messages.clear(); }
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollConfig {
    pub direction: Direction,
    pub speed_px_per_second: f64,
    pub line_height: f64,
    pub gap: f64,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            direction: Direction::default(),
            speed_px_per_second: 60.0,
            line_height: 24.0,
            gap: 6.0,
        }
    }
}

/// Platform-independent continuous-list scrolling model.
///
/// Messages remain in the buffer. The engine moves the whole train as one
/// unit and wraps the phase when the complete train has passed the viewport.
/// It never creates duplicate message elements.
#[derive(Debug)]
pub struct ScrollEngine {
    config: ScrollConfig,
    offset: f64,
}

impl ScrollEngine {
    pub fn new(config: ScrollConfig) -> Self { Self { config, offset: 0.0 } }

    pub fn offset(&self) -> f64 { self.offset }

    pub fn advance(&mut self, elapsed: Duration) {
        let delta = self.config.speed_px_per_second * elapsed.as_secs_f64();
        self.offset = match self.config.direction {
            Direction::BottomToTop => self.offset - delta,
            Direction::TopToBottom => self.offset + delta,
        };
    }

    pub fn reset(&mut self) { self.offset = 0.0; }

    /// New messages join the current train without restarting it.
    pub fn reset_if_needed(&mut self) {}

    pub fn layout(&self, buffer: &MessageBuffer, region: Rect) -> Scene {
        if buffer.is_empty() {
            return Scene::default();
        }

        let step = self.config.line_height + self.config.gap;
        if step <= 0.0 || region.height < 0.0 {
            return Scene::default();
        }

        let count = buffer.len();
        let content_height = self.config.line_height
            + (count.saturating_sub(1) as f64) * step;
        // One complete cycle is the distance from the first message entering
        // at one edge until the complete train has left at the other edge.
        let cycle = region.height + content_height;
        if cycle <= 0.0 {
            return Scene::default();
        }

        let phase = match self.config.direction {
            Direction::BottomToTop => (-self.offset).rem_euclid(cycle),
            Direction::TopToBottom => self.offset.rem_euclid(cycle),
        };
        let messages: Vec<&Message> = buffer.iter().collect();
        let mut elements = Vec::with_capacity(count);

        for (index, message) in messages.into_iter().enumerate() {
            let distance = index as f64 * step;
            let y = match self.config.direction {
                Direction::BottomToTop => region.y + region.height - phase - distance,
                Direction::TopToBottom => region.y + phase + distance,
            };

            // Keep one element per retained message. The renderer clips text
            // outside the viewport; the core should describe the complete
            // train rather than silently dropping messages based on viewport
            // visibility. This also makes the cyclic phase deterministic.
            elements.push(TextElement {
                text: message.text.clone(),
                position: Point { x: region.x + region.width, y },
                width: region.width,
                height: self.config.line_height,
            });
        }

        Scene { elements }
    }
}

pub trait Renderer {
    type Error;
    fn render(&mut self, scene: &Scene) -> Result<(), Self::Error>;
    fn display_size(&self) -> Size;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_discards_oldest() {
        let mut q = MessageBuffer::new(QueuePolicy::new(3));
        q.push(Message::new("A"));
        q.push(Message::new("B"));
        q.push(Message::new("C"));
        q.push(Message::new("D"));
        let got: Vec<_> = q.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(got, ["B", "C", "D"]);
    }

    #[test]
    fn scrolling_is_time_based() {
        let mut e = ScrollEngine::new(ScrollConfig { speed_px_per_second: 100.0, ..Default::default() });
        e.advance(Duration::from_millis(500));
        assert!((e.offset() + 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn direction_changes_offset_sign() {
        let mut e = ScrollEngine::new(ScrollConfig { direction: Direction::TopToBottom, speed_px_per_second: 10.0, ..Default::default() });
        e.advance(Duration::from_secs(1));
        assert_eq!(e.offset(), 10.0);
    }

    #[test]
    fn single_message_wraps_after_traversing_region() {
        let config = ScrollConfig { speed_px_per_second: 100.0, line_height: 20.0, gap: 10.0, ..Default::default() };
        let mut e = ScrollEngine::new(config);
        let mut q = MessageBuffer::new(QueuePolicy::new(10));
        q.push(Message::new("A"));
        let region = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };

        let initial = e.layout(&q, region);
        assert_eq!(initial.elements.len(), 1);
        assert_eq!(initial.elements[0].text, "A");
        assert_eq!(initial.elements[0].position.y, 100.0);

        // One complete cycle is viewport height + message height = 120px.
        e.advance(Duration::from_secs_f64(1.2));
        let wrapped = e.layout(&q, region);
        assert_eq!(wrapped.elements.len(), 1);
        assert_eq!(wrapped.elements[0].text, "A");
        assert!((wrapped.elements[0].position.y - initial.elements[0].position.y).abs() < 1e-9);
    }

    #[test]
    fn multiple_messages_are_one_spaced_train_without_duplicates() {
        let config = ScrollConfig { speed_px_per_second: 10.0, line_height: 20.0, gap: 10.0, ..Default::default() };
        let mut e = ScrollEngine::new(config);
        let mut q = MessageBuffer::new(QueuePolicy::new(10));
        q.push(Message::new("A"));
        q.push(Message::new("B"));
        let region = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };

        let scene = e.layout(&q, region);
        assert_eq!(scene.elements.len(), 2);
        assert_eq!(scene.elements[0].text, "A");
        assert_eq!(scene.elements[1].text, "B");
        assert!((scene.elements[0].position.y - scene.elements[1].position.y - 30.0).abs() < 1e-9);

        e.advance(Duration::from_secs_f64(0.5));
        let moved = e.layout(&q, region);
        assert_eq!(moved.elements.len(), 2);
        assert!((moved.elements[0].position.y - scene.elements[0].position.y + 5.0).abs() < 1e-9);
        assert!((moved.elements[1].position.y - scene.elements[1].position.y + 5.0).abs() < 1e-9);
    }

}
