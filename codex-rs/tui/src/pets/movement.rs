//! Pure positioning model for ambient pet movement.
//!
//! The model deliberately knows nothing about terminal buffers, image
//! protocols, env vars, or config. Callers must decide whether a target is safe
//! before passing it in; this module only computes a bounded current rectangle
//! and a bounded redraw cadence for an already accepted route.

use std::time::Duration;

use ratatui::layout::Rect;

const LANE_PATROL_HALF_TRIP_MILLIS: u64 = 900;
const LANE_PATROL_HALF_TRIP: Duration =
    Duration::from_millis(/*millis*/ LANE_PATROL_HALF_TRIP_MILLIS);
const LANE_PATROL_PERIOD: Duration =
    Duration::from_millis(/*millis*/ LANE_PATROL_HALF_TRIP_MILLIS * 2);
const MOVEMENT_TICK_DELAY: Duration = Duration::from_millis(/*millis*/ 120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MovementAnimationState {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PetMovementMode {
    Disabled,
    LanePatrol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PetMovementTarget {
    x: u16,
    y: u16,
}

impl PetMovementTarget {
    pub(crate) const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub(crate) const fn x(self) -> u16 {
        self.x
    }

    pub(crate) const fn y(self) -> u16 {
        self.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetMovementDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PetMovement {
    mode: PetMovementMode,
}

impl PetMovement {
    pub(crate) const fn disabled() -> Self {
        Self {
            mode: PetMovementMode::Disabled,
        }
    }

    pub(crate) const fn lane_patrol() -> Self {
        Self {
            mode: PetMovementMode::LanePatrol,
        }
    }

    pub(crate) const fn is_active(self) -> bool {
        !matches!(self.mode, PetMovementMode::Disabled)
    }

    pub(crate) fn current_rect(
        self,
        home: Rect,
        target: PetMovementTarget,
        elapsed: Duration,
    ) -> Rect {
        match self.mode {
            PetMovementMode::Disabled => home,
            PetMovementMode::LanePatrol => lane_patrol_rect(home, target, elapsed),
        }
    }

    pub(crate) fn current_horizontal_direction(
        self,
        home: Rect,
        target: PetMovementTarget,
        elapsed: Duration,
    ) -> Option<PetMovementDirection> {
        match self.mode {
            PetMovementMode::Disabled => None,
            PetMovementMode::LanePatrol => lane_patrol_horizontal_direction(home, target, elapsed),
        }
    }

    pub(crate) fn next_tick_delay(
        self,
        animation_state: MovementAnimationState,
    ) -> Option<Duration> {
        match (self.mode, animation_state) {
            (PetMovementMode::Disabled, _) | (_, MovementAnimationState::Disabled) => None,
            (PetMovementMode::LanePatrol, MovementAnimationState::Enabled) => {
                Some(MOVEMENT_TICK_DELAY)
            }
        }
    }
}

fn lane_patrol_rect(home: Rect, target: PetMovementTarget, elapsed: Duration) -> Rect {
    let (from_x, from_y, to_x, to_y, progress_nanos) = lane_patrol_leg(home, target, elapsed);

    Rect {
        x: interpolate_axis(
            from_x,
            to_x,
            progress_nanos,
            LANE_PATROL_HALF_TRIP.as_nanos(),
        ),
        y: interpolate_axis(
            from_y,
            to_y,
            progress_nanos,
            LANE_PATROL_HALF_TRIP.as_nanos(),
        ),
        width: home.width,
        height: home.height,
    }
}

fn lane_patrol_horizontal_direction(
    home: Rect,
    target: PetMovementTarget,
    elapsed: Duration,
) -> Option<PetMovementDirection> {
    let (from_x, _, to_x, _, _) = lane_patrol_leg(home, target, elapsed);
    match to_x.cmp(&from_x) {
        std::cmp::Ordering::Less => Some(PetMovementDirection::Left),
        std::cmp::Ordering::Greater => Some(PetMovementDirection::Right),
        std::cmp::Ordering::Equal => None,
    }
}

fn lane_patrol_leg(
    home: Rect,
    target: PetMovementTarget,
    elapsed: Duration,
) -> (u16, u16, u16, u16, u128) {
    let elapsed_nanos = elapsed.as_nanos() % LANE_PATROL_PERIOD.as_nanos();
    let half_trip_nanos = LANE_PATROL_HALF_TRIP.as_nanos();
    if elapsed_nanos <= half_trip_nanos {
        (home.x, home.y, target.x, target.y, elapsed_nanos)
    } else {
        (
            target.x,
            target.y,
            home.x,
            home.y,
            elapsed_nanos.saturating_sub(half_trip_nanos),
        )
    }
}

fn interpolate_axis(from: u16, to: u16, progress_nanos: u128, total_nanos: u128) -> u16 {
    if total_nanos == 0 {
        return to;
    }

    let from = i128::from(from);
    let to = i128::from(to);
    let delta = to - from;
    let progress = progress_nanos.min(total_nanos) as i128;
    let total = total_nanos as i128;
    (from + delta * progress / total).clamp(0, i128::from(u16::MAX)) as u16
}

#[cfg(test)]
#[path = "movement_tests.rs"]
mod tests;
