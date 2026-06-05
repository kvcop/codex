use std::time::Duration;

use pretty_assertions::assert_eq;
use ratatui::layout::Rect;

use super::*;

#[test]
fn disabled_movement_returns_home_exactly() {
    let home = Rect::new(80, 20, 8, 5);
    let target = PetMovementTarget::new(80, 12);

    assert_eq!(
        PetMovement::disabled().current_rect(home, target, Duration::from_secs(/*secs*/ 42)),
        home
    );
}

#[test]
fn lane_patrol_moves_to_target_and_returns_home() {
    let home = Rect::new(80, 20, 8, 5);
    let target = PetMovementTarget::new(80, 12);

    assert_eq!(
        PetMovement::lane_patrol().current_rect(home, target, LANE_PATROL_HALF_TRIP / 2),
        Rect::new(80, 16, 8, 5)
    );
    assert_eq!(
        PetMovement::lane_patrol().current_rect(home, target, LANE_PATROL_HALF_TRIP),
        Rect::new(80, 12, 8, 5)
    );
    assert_eq!(
        PetMovement::lane_patrol().current_rect(home, target, LANE_PATROL_PERIOD),
        home
    );
}

#[test]
fn lane_patrol_interpolates_horizontal_axis() {
    let home = Rect::new(30, 20, 8, 5);
    let target = PetMovementTarget::new(42, 20);

    assert_eq!(
        PetMovement::lane_patrol().current_rect(home, target, LANE_PATROL_HALF_TRIP / 2),
        Rect::new(36, 20, 8, 5)
    );
}

#[test]
fn lane_patrol_interpolates_return_leg() {
    let home = Rect::new(80, 20, 8, 5);
    let target = PetMovementTarget::new(80, 12);

    assert_eq!(
        PetMovement::lane_patrol().current_rect(
            home,
            target,
            LANE_PATROL_HALF_TRIP + LANE_PATROL_HALF_TRIP / 2,
        ),
        Rect::new(80, 16, 8, 5)
    );
}

#[test]
fn lane_patrol_wraps_elapsed_time_by_period() {
    let home = Rect::new(80, 20, 8, 5);
    let target = PetMovementTarget::new(80, 12);

    assert_eq!(
        PetMovement::lane_patrol().current_rect(
            home,
            target,
            LANE_PATROL_PERIOD + LANE_PATROL_HALF_TRIP / 2,
        ),
        Rect::new(80, 16, 8, 5)
    );
}

#[test]
fn lane_patrol_preserves_home_size() {
    let home = Rect::new(30, 10, 11, 7);
    let target = PetMovementTarget::new(45, 2);
    let rect = PetMovement::lane_patrol().current_rect(home, target, LANE_PATROL_HALF_TRIP / 2);

    assert_eq!(rect.width, home.width);
    assert_eq!(rect.height, home.height);
}

#[test]
fn active_lane_patrol_reports_bounded_tick_delay() {
    assert_eq!(
        PetMovement::lane_patrol().next_tick_delay(MovementAnimationState::Enabled),
        Some(MOVEMENT_TICK_DELAY)
    );
}

#[test]
fn disabled_animation_reports_no_movement_tick() {
    assert_eq!(
        PetMovement::lane_patrol().next_tick_delay(MovementAnimationState::Disabled),
        None
    );
}

#[test]
fn disabled_movement_reports_no_movement_tick() {
    assert_eq!(
        PetMovement::disabled().next_tick_delay(MovementAnimationState::Enabled),
        None
    );
}
