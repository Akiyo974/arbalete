//! Turns the system's scroll events into the horizontal swipes that page between Spaces.
//!
//! A trackpad says when the fingers touch, move and lift, then sends the momentum it adds on its
//! own: only the fingers move the Spaces, so momentum is ignored. A mouse wheel says none of this:
//! a horizontal notch moves one Space, then the wheel rests for a moment.

use serde::Serialize;

/// The rest after a mouse wheel moves one Space, in seconds: one burst of notches moves one Space.
const STEP_COOLDOWN: f64 = 0.35;

/// Where a scroll event is in its gesture, as the system reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// No gesture: a mouse wheel, or the momentum after one.
    None,
    Began,
    Changed,
    /// Ended or cancelled.
    Ended,
    /// Fingers resting or about to move, which say nothing about the swipe.
    Other,
}

/// One scroll event.
#[derive(Clone, Copy, Debug)]
pub struct Scroll {
    pub phase: Phase,
    pub momentum: bool,
    /// In points, as the system scrolls content: positive when it moves right or down, following
    /// the user's scrolling direction.
    pub dx: f64,
    pub dy: f64,
    /// Where the pointer is, in points from the main window's top.
    pub y: f64,
    /// In seconds.
    pub time: f64,
    /// Over the sidebar, where swipes page between Spaces.
    pub inside: bool,
}

/// What the sidebar receives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Swipe {
    /// The fingers follow a gesture: the UI moves the Spaces with them (app/macos/Bridge.swift).
    Gesture(Gesture),
    /// A mouse wheel moves to the next Space, `true`, or the previous one.
    Step(bool),
}

/// Sent to the sidebar as the detail of an `arbalete:swipe` event.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Gesture {
    pub phase: GesturePhase,
    /// In points, like `Scroll::dx`.
    pub dx: f64,
    /// Like `Scroll::y`.
    pub y: f64,
    /// In milliseconds.
    pub time: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GesturePhase {
    Begin,
    Move,
    End,
}

#[derive(Default)]
pub struct Tracker {
    state: State,
    last_step: Option<f64>,
}

#[derive(Default, PartialEq)]
enum State {
    #[default]
    Idle,
    /// The gesture began over the sidebar, without a movement yet to say its direction.
    Deciding,
    Tracking,
    /// A vertical gesture, which scrolls the tabs.
    Ignoring,
}

impl Tracker {
    pub fn track(&mut self, scroll: Scroll) -> Option<Swipe> {
        if scroll.momentum {
            return None;
        }
        let gesture = |phase| {
            Swipe::Gesture(Gesture {
                phase,
                dx: scroll.dx,
                y: scroll.y,
                time: scroll.time * 1000.0,
            })
        };
        match scroll.phase {
            Phase::None => self.step(scroll).map(Swipe::Step),
            Phase::Began => {
                self.state = if scroll.inside {
                    State::Deciding
                } else {
                    State::Ignoring
                };
                self.decide(scroll).then(|| gesture(GesturePhase::Begin))
            }
            Phase::Changed if self.state == State::Tracking => Some(gesture(GesturePhase::Move)),
            Phase::Changed => self.decide(scroll).then(|| gesture(GesturePhase::Begin)),
            Phase::Ended => {
                let tracking = std::mem::take(&mut self.state) == State::Tracking;
                tracking.then(|| gesture(GesturePhase::End))
            }
            Phase::Other => None,
        }
    }

    /// Decides on the first movement whether the gesture is a swipe: mostly horizontal.
    fn decide(&mut self, scroll: Scroll) -> bool {
        if self.state != State::Deciding || (scroll.dx == 0.0 && scroll.dy == 0.0) {
            return false;
        }
        self.state = if is_horizontal(scroll) {
            State::Tracking
        } else {
            State::Ignoring
        };
        self.state == State::Tracking
    }

    /// The next Space when the content would move left, as with pages side by side.
    fn step(&mut self, scroll: Scroll) -> Option<bool> {
        if !scroll.inside || !is_horizontal(scroll) {
            return None;
        }
        if self
            .last_step
            .is_some_and(|last| scroll.time - last < STEP_COOLDOWN)
        {
            return None;
        }
        self.last_step = Some(scroll.time);
        Some(scroll.dx < 0.0)
    }
}

fn is_horizontal(scroll: Scroll) -> bool {
    scroll.dx.abs() > scroll.dy.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scroll(phase: Phase, dx: f64, dy: f64, time: f64) -> Scroll {
        Scroll {
            phase,
            momentum: false,
            dx,
            dy,
            y: 100.0,
            time,
            inside: true,
        }
    }

    fn gesture(phase: GesturePhase, dx: f64, time: f64) -> Option<Swipe> {
        Some(Swipe::Gesture(Gesture {
            phase,
            dx,
            y: 100.0,
            time: time * 1000.0,
        }))
    }

    #[test]
    fn follows_a_horizontal_gesture_from_touch_to_lift() {
        let mut tracker = Tracker::default();
        assert_eq!(
            tracker.track(scroll(Phase::Began, -4.0, 1.0, 1.0)),
            gesture(GesturePhase::Begin, -4.0, 1.0)
        );
        assert_eq!(
            tracker.track(scroll(Phase::Changed, -10.0, 3.0, 1.01)),
            gesture(GesturePhase::Move, -10.0, 1.01)
        );
        // Once a swipe, a vertical movement still moves it.
        assert_eq!(
            tracker.track(scroll(Phase::Changed, 0.0, 8.0, 1.02)),
            gesture(GesturePhase::Move, 0.0, 1.02)
        );
        assert_eq!(
            tracker.track(scroll(Phase::Ended, 0.0, 0.0, 1.03)),
            gesture(GesturePhase::End, 0.0, 1.03)
        );
        assert_eq!(tracker.track(scroll(Phase::Changed, -4.0, 0.0, 1.04)), None);
    }

    #[test]
    fn decides_on_the_first_movement_when_the_touch_has_none() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.track(scroll(Phase::Began, 0.0, 0.0, 1.0)), None);
        assert_eq!(
            tracker.track(scroll(Phase::Changed, 6.0, 2.0, 1.01)),
            gesture(GesturePhase::Begin, 6.0, 1.01)
        );
    }

    #[test]
    fn leaves_vertical_gestures_to_scroll_the_tabs() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.track(scroll(Phase::Began, 2.0, 5.0, 1.0)), None);
        assert_eq!(tracker.track(scroll(Phase::Changed, 20.0, 0.0, 1.01)), None);
        assert_eq!(tracker.track(scroll(Phase::Ended, 0.0, 0.0, 1.02)), None);
    }

    #[test]
    fn ignores_gestures_beginning_outside_the_sidebar() {
        let mut tracker = Tracker::default();
        let began = Scroll {
            inside: false,
            ..scroll(Phase::Began, -10.0, 0.0, 1.0)
        };
        assert_eq!(tracker.track(began), None);
        assert_eq!(
            tracker.track(scroll(Phase::Changed, -10.0, 0.0, 1.01)),
            None
        );
        assert_eq!(tracker.track(scroll(Phase::Ended, 0.0, 0.0, 1.02)), None);
    }

    #[test]
    fn ignores_momentum() {
        let mut tracker = Tracker::default();
        tracker.track(scroll(Phase::Began, -10.0, 0.0, 1.0));
        tracker.track(scroll(Phase::Ended, 0.0, 0.0, 1.01));
        let momentum = Scroll {
            momentum: true,
            ..scroll(Phase::None, -30.0, 0.0, 1.02)
        };
        assert_eq!(tracker.track(momentum), None);
    }

    #[test]
    fn ignores_resting_fingers() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.track(scroll(Phase::Other, -10.0, 0.0, 1.0)), None);
        assert_eq!(tracker.track(scroll(Phase::Ended, 0.0, 0.0, 1.01)), None);
    }

    #[test]
    fn a_mouse_wheel_moves_one_space_per_burst() {
        let mut tracker = Tracker::default();
        let wheel = |dx, time| scroll(Phase::None, dx, 0.0, time);
        assert_eq!(tracker.track(wheel(-1.0, 1.0)), Some(Swipe::Step(true)));
        assert_eq!(tracker.track(wheel(-1.0, 1.1)), None);
        assert_eq!(tracker.track(wheel(1.0, 1.2)), None);
        assert_eq!(
            tracker.track(wheel(1.0, 1.0 + STEP_COOLDOWN)),
            Some(Swipe::Step(false))
        );
    }

    #[test]
    fn a_vertical_mouse_wheel_scrolls_the_tabs() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.track(scroll(Phase::None, 1.0, 3.0, 1.0)), None);
        let outside = Scroll {
            inside: false,
            ..scroll(Phase::None, -3.0, 0.0, 2.0)
        };
        assert_eq!(tracker.track(outside), None);
    }
}
