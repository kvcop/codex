//! Ambient terminal rendering for the Codex companion.
//!
//! Ambient pets reuse the same extracted image frames as the full-screen viewer
//! but are rendered through a different ownership split: ratatui still owns the
//! transcript/composer layout, while the sprite itself is emitted through the
//! terminal image protocol after the frame draw completes.
//!
//! This module therefore owns two separate contracts:
//! choosing which animation frame should be visible for the current semantic
//! pet state, and translating that frame into a precise on-screen image request
//! that does not overlap reserved bottom-pane space. It does not persist pet
//! selection or decide when modal/popover UI should suppress the sprite.

#[cfg(test)]
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use unicode_width::UnicodeWidthStr as _;

use crate::tui::FrameRequester;

use super::DEFAULT_PET_ID;
use super::frames;
use super::image_protocol::ImageProtocol;
use super::image_protocol::PetImageSupport;
#[cfg(not(test))]
use super::image_protocol::ProtocolSelection;
use super::model::Animation;
#[cfg(test)]
use super::model::AnimationFrame;
use super::model::Pet;
use super::movement::MovementAnimationState;
use super::movement::PetMovement;
use super::movement::PetMovementTarget;

const PET_TARGET_HEIGHT_PX: u16 = 75;
const PET_COMPOSER_GAP_PX: u16 = 10;
const TERMINAL_ROW_HEIGHT_PX: u16 = 15;
const LANE_PATROL_MAX_ROWS: u16 = 4;
const UNSAFE_TUI_PET_MOVEMENT_ENV_VAR: &str = "CODEX_UNSAFE_TUI_PET_MOVEMENT";
const MOVEMENT_LOG_TARGET: &str = "codex_tui::pets::movement";

const RUNNING_LIFETIME: Duration = Duration::from_secs(3 * 60);
const FAILED_LIFETIME: Duration = Duration::from_secs(60 * 60);
const WAITING_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
const REVIEW_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetNotificationKind {
    Running,
    Waiting,
    Review,
    Failed,
}

impl PetNotificationKind {
    fn animation_name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Review => "review",
            Self::Failed => "failed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Waiting => "Needs input",
            Self::Review => "Ready",
            Self::Failed => "Blocked",
        }
    }

    fn fallback_body(self) -> &'static str {
        match self {
            Self::Running => "Thinking",
            Self::Waiting => "Needs input",
            Self::Review => "Ready",
            Self::Failed => "Blocked",
        }
    }

    fn lifetime(self) -> Duration {
        match self {
            Self::Running => RUNNING_LIFETIME,
            Self::Waiting => WAITING_LIFETIME,
            Self::Review => REVIEW_LIFETIME,
            Self::Failed => FAILED_LIFETIME,
        }
    }
}

#[derive(Debug, Clone)]
struct PetNotification {
    kind: PetNotificationKind,
    body: String,
    updated_at: Instant,
}

impl PetNotification {
    fn new(kind: PetNotificationKind, body: Option<String>) -> Self {
        Self {
            kind,
            body: body.unwrap_or_else(|| kind.fallback_body().to_string()),
            updated_at: Instant::now(),
        }
    }

    fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.updated_at) >= self.kind.lifetime()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AmbientPetDraw {
    pub(crate) frame: PathBuf,
    pub(crate) protocol: ImageProtocol,
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) clear_top_y: u16,
    pub(crate) columns: u16,
    pub(crate) rows: u16,
    pub(crate) height_px: u16,
    pub(crate) sixel_dir: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AmbientPetDrawContext<'a> {
    buffer: Option<&'a Buffer>,
    movement_bounds: Option<Rect>,
    movement_target: Option<PetMovementTarget>,
    movement_elapsed: Option<Duration>,
}

impl<'a> AmbientPetDrawContext<'a> {
    #[cfg(test)]
    pub(crate) const fn without_movement() -> Self {
        Self {
            buffer: None,
            movement_bounds: None,
            movement_target: None,
            movement_elapsed: None,
        }
    }

    pub(crate) fn from_buffer_with_movement_bounds(
        buffer: &'a Buffer,
        movement_bounds: Rect,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            movement_bounds: Some(movement_bounds),
            movement_target: None,
            movement_elapsed: None,
        }
    }

    #[cfg(test)]
    fn from_buffer_with_movement_elapsed(buffer: &'a Buffer, movement_elapsed: Duration) -> Self {
        Self {
            buffer: Some(buffer),
            movement_bounds: Some(buffer.area),
            movement_target: None,
            movement_elapsed: Some(movement_elapsed),
        }
    }

    #[cfg(test)]
    fn with_movement_target(
        buffer: &'a Buffer,
        movement_target: PetMovementTarget,
        movement_elapsed: Duration,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            movement_bounds: Some(buffer.area),
            movement_target: Some(movement_target),
            movement_elapsed: Some(movement_elapsed),
        }
    }

    #[cfg(test)]
    fn with_movement_bounds(mut self, movement_bounds: Rect) -> Self {
        self.movement_bounds = Some(movement_bounds);
        self
    }
}

#[derive(Debug)]
pub(crate) struct AmbientPet {
    pet: Pet,
    support: PetImageSupport,
    frames: Vec<PathBuf>,
    sixel_dir: PathBuf,
    frame_requester: FrameRequester,
    notification: Option<PetNotification>,
    animation_started_at: Instant,
    animations_enabled: bool,
    movement: PetMovement,
    movement_started_at: Instant,
}

impl AmbientPet {
    /// Load the active ambient pet and prepare its frame cache.
    ///
    /// This resolves the selected pet id, extracts per-frame PNGs into the
    /// CODEX_HOME cache, and records the terminal protocol support snapshot used
    /// for later draw requests. A caller that repeatedly recreates `AmbientPet`
    /// instead of mutating one instance would lose animation timing continuity
    /// and pay the frame-cache preparation cost more often than necessary.
    pub(crate) fn load(
        selected_pet: Option<&str>,
        codex_home: &std::path::Path,
        frame_requester: FrameRequester,
        animations_enabled: bool,
    ) -> Result<Self> {
        let pet = Pet::load_with_codex_home(
            selected_pet.unwrap_or(DEFAULT_PET_ID),
            /*codex_home*/ Some(codex_home),
        )
        .with_context(|| "load ambient pet")?;
        let cache_dir = codex_home
            .join("cache")
            .join("tui-pets")
            .join("frame-cache")
            .join(&pet.id)
            .join(pet.frame_cache_key()?);
        let frame_dir = cache_dir.join("frames");
        let sixel_dir = cache_dir.join("sixel");
        let frames = frames::prepare_png_frames(&pet, &frame_dir)?;
        Ok(Self {
            pet,
            support: default_image_support(),
            frames,
            sixel_dir,
            frame_requester,
            notification: None,
            animation_started_at: Instant::now(),
            animations_enabled,
            movement: configured_movement(),
            movement_started_at: Instant::now(),
        })
    }

    pub(crate) fn set_notification(&mut self, kind: PetNotificationKind, body: Option<String>) {
        self.notification = Some(PetNotification::new(kind, body));
        self.animation_started_at = Instant::now();
    }

    pub(crate) fn image_enabled(&self) -> bool {
        self.support.protocol().is_some()
    }

    pub(crate) fn image_columns(&self) -> u16 {
        self.image_size().columns
    }

    #[cfg(test)]
    pub(crate) fn set_image_support_for_tests(&mut self, support: PetImageSupport) {
        self.support = support;
    }

    #[cfg(test)]
    fn enable_lane_patrol_for_tests(&mut self) {
        self.movement = PetMovement::lane_patrol();
        self.movement_started_at = Instant::now();
    }

    pub(crate) fn schedule_next_frame(&self) {
        if let Some(delay) = self.next_frame_delay() {
            self.frame_requester.schedule_frame_in(delay);
        }
    }

    fn next_frame_delay(&self) -> Option<Duration> {
        let protocol = self.support.protocol()?;

        let movement_animation_state = if self.animations_enabled {
            MovementAnimationState::Enabled
        } else {
            MovementAnimationState::Disabled
        };
        let movement_delay = if protocol_allows_movement(protocol) {
            self.movement.next_tick_delay(movement_animation_state)
        } else {
            None
        };
        if !self.animations_enabled {
            return movement_delay;
        }

        let animation_delay = self
            .current_animation()
            .and_then(|animation| {
                current_animation_frame(animation, self.animation_started_at.elapsed())
            })
            .and_then(|frame| frame.delay);
        match (animation_delay, movement_delay) {
            (Some(animation_delay), Some(movement_delay)) => {
                Some(animation_delay.min(movement_delay))
            }
            (Some(delay), None) | (None, Some(delay)) => Some(delay),
            (None, None) => None,
        }
    }

    /// Build an image draw request for the ambient pet anchored above the composer.
    ///
    /// Returning `None` means "do not render the sprite this frame", typically
    /// because the terminal protocol is unavailable or the current layout cannot
    /// fit the image without overlapping reserved UI. Callers should not try to
    /// partially clip the image themselves; that would desynchronize the image
    /// protocol output from the TUI's notion of cleared rows.
    pub(crate) fn draw_request(
        &self,
        area: Rect,
        composer_bottom_y: u16,
        context: AmbientPetDrawContext<'_>,
    ) -> Option<AmbientPetDraw> {
        let protocol = self.support.protocol()?;
        let size = self.image_size();
        let notification = self.visible_notification(Instant::now());
        let notification_height = notification.map_or(0, notification_height);
        let required_height = size.rows.saturating_add(notification_height);
        let sprite_bottom_y = composer_bottom_y.saturating_sub(composer_gap_rows());
        if sprite_bottom_y < area.y.saturating_add(required_height) || area.width < size.columns {
            return None;
        }

        let home = Rect::new(
            area.x + area.width.saturating_sub(size.columns),
            sprite_bottom_y.saturating_sub(size.rows),
            size.columns,
            size.rows,
        );
        let rect = self.movement_rect(protocol, home, context);
        Some(AmbientPetDraw {
            frame: self.current_frame_path()?,
            protocol,
            x: rect.x,
            y: rect.y,
            clear_top_y: area.y,
            columns: rect.width,
            rows: rect.height,
            height_px: size.height_px,
            sixel_dir: self.sixel_dir.clone(),
        })
    }

    fn movement_rect(
        &self,
        protocol: ImageProtocol,
        home: Rect,
        context: AmbientPetDrawContext<'_>,
    ) -> Rect {
        if !protocol_allows_movement(protocol) {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                "pet movement disabled for image protocol"
            );
            return home;
        }
        if !self.animations_enabled {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                "pet movement disabled because animations are disabled"
            );
            return home;
        }
        if !self.movement.is_active() {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                "pet movement inactive"
            );
            return home;
        }

        let Some(buffer) = context.buffer else {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                "pet movement has no rendered buffer context"
            );
            return home;
        };
        let movement_bounds = context.movement_bounds.unwrap_or(buffer.area);
        let Some(target) = context
            .movement_target
            .or_else(|| lane_patrol_target(home, movement_bounds))
        else {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                buffer_area = ?buffer.area,
                ?movement_bounds,
                "pet movement target unavailable"
            );
            return home;
        };

        let target_rect = Rect::new(target.x(), target.y(), home.width, home.height);
        let movement_elapsed = context
            .movement_elapsed
            .unwrap_or_else(|| self.movement_started_at.elapsed());
        let movement_rect = self.movement.current_rect(home, target, movement_elapsed);
        let target_rejection = rect_blank_rejection(buffer, movement_bounds, target_rect);
        let movement_rejection = rect_blank_rejection(buffer, movement_bounds, movement_rect);
        if target_rejection.is_none() && movement_rejection.is_none() {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                ?target_rect,
                ?movement_rect,
                ?movement_bounds,
                movement_elapsed_ms = movement_elapsed.as_millis(),
                "pet movement accepted"
            );
            movement_rect
        } else {
            tracing::trace!(
                target: MOVEMENT_LOG_TARGET,
                ?protocol,
                ?home,
                ?target_rect,
                ?movement_rect,
                ?movement_bounds,
                movement_elapsed_ms = movement_elapsed.as_millis(),
                ?target_rejection,
                ?movement_rejection,
                "pet movement rejected; drawing home"
            );
            home
        }
    }

    /// Build a centered preview draw request for the `/pets` picker side pane.
    ///
    /// The picker preview intentionally uses the first idle frame rather than
    /// the live animation state so selection browsing stays stable and does not
    /// require the full ambient animation lifecycle.
    pub(crate) fn preview_draw_request(&self, area: Rect) -> Option<AmbientPetDraw> {
        let protocol = self.support.protocol()?;
        let size = self.image_size();
        if area.width < size.columns || area.height < size.rows {
            return None;
        }

        let y = area.y + area.height.saturating_sub(size.rows) / 2;
        Some(AmbientPetDraw {
            frame: self.first_idle_frame_path()?,
            protocol,
            x: area.x + area.width.saturating_sub(size.columns) / 2,
            y,
            clear_top_y: y,
            columns: size.columns,
            rows: size.rows,
            height_px: size.height_px,
            sixel_dir: self.sixel_dir.clone(),
        })
    }

    fn visible_notification(&self, now: Instant) -> Option<&PetNotification> {
        self.notification
            .as_ref()
            .filter(|notification| !notification.is_expired(now))
    }

    fn current_animation(&self) -> Option<&Animation> {
        let animation_name = self
            .visible_notification(Instant::now())
            .map_or("idle", |notification| notification.kind.animation_name());
        let animation = self
            .pet
            .animations
            .get(animation_name)
            .or_else(|| self.pet.animations.get("idle"))?;
        if animation.loop_start.is_none() {
            let elapsed = self.animation_started_at.elapsed();
            if elapsed >= animation.total_duration()
                && let Some(fallback) = self.pet.animations.get(&animation.fallback)
            {
                return Some(fallback);
            }
        }
        Some(animation)
    }

    fn current_frame_path(&self) -> Option<PathBuf> {
        let sprite_index = self
            .current_animation()
            .and_then(|animation| {
                if self.animations_enabled {
                    current_animation_frame(animation, self.animation_started_at.elapsed())
                        .map(|frame| frame.sprite_index)
                } else {
                    animation.frames.first().map(|frame| frame.sprite_index)
                }
            })
            .unwrap_or(0);
        self.frame_path_for_sprite_index(sprite_index)
    }

    fn first_idle_frame_path(&self) -> Option<PathBuf> {
        let sprite_index = self
            .pet
            .animations
            .get("idle")
            .and_then(|animation| animation.frames.first())
            .map_or(0, |frame| frame.sprite_index);
        self.frame_path_for_sprite_index(sprite_index)
    }

    fn frame_path_for_sprite_index(&self, sprite_index: usize) -> Option<PathBuf> {
        self.frames
            .get(sprite_index.min(self.frames.len().saturating_sub(1)))
            .cloned()
    }

    fn image_size(&self) -> ImageSize {
        let rows = (f64::from(PET_TARGET_HEIGHT_PX) / f64::from(TERMINAL_ROW_HEIGHT_PX))
            .round()
            .max(/*other*/ 1.0) as u16;
        let aspect = f64::from(self.pet.frame_height) / f64::from(self.pet.frame_width) * 0.52;
        let columns = (f64::from(rows) / aspect).round() as u16;
        ImageSize {
            columns: columns.max(1),
            rows,
            height_px: PET_TARGET_HEIGHT_PX,
        }
    }
}

fn composer_gap_rows() -> u16 {
    ((f64::from(PET_COMPOSER_GAP_PX) / f64::from(TERMINAL_ROW_HEIGHT_PX)).round() as u16)
        .max(/*other*/ 1)
}

#[cfg(not(test))]
fn default_image_support() -> PetImageSupport {
    ProtocolSelection::Auto.resolve()
}

#[cfg(test)]
fn default_image_support() -> PetImageSupport {
    PetImageSupport::Unsupported(super::image_protocol::PetImageUnsupportedReason::Terminal)
}

#[derive(Debug, Clone, Copy)]
struct ImageSize {
    columns: u16,
    rows: u16,
    height_px: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnimationFrameTick {
    sprite_index: usize,
    delay: Option<Duration>,
}

fn current_animation_frame(animation: &Animation, elapsed: Duration) -> Option<AnimationFrameTick> {
    if animation.frames.len() <= 1 {
        return Some(AnimationFrameTick {
            sprite_index: animation.frames.first()?.sprite_index,
            delay: None,
        });
    }

    let elapsed_nanos = elapsed.as_nanos();
    if let Some(loop_start) = animation
        .loop_start
        .filter(|idx| *idx < animation.frames.len())
    {
        let total_nanos = animation.total_duration().as_nanos();
        let prefix_nanos = animation.frames[..loop_start]
            .iter()
            .map(|frame| frame.duration.as_nanos())
            .sum::<u128>();
        let loop_nanos = animation.frames[loop_start..]
            .iter()
            .map(|frame| frame.duration.as_nanos())
            .sum::<u128>();
        let effective_elapsed = if elapsed_nanos >= total_nanos && loop_nanos > 0 {
            prefix_nanos + elapsed_nanos.saturating_sub(prefix_nanos) % loop_nanos
        } else {
            elapsed_nanos
        };
        frame_at_elapsed(animation, effective_elapsed)
    } else if elapsed_nanos >= animation.total_duration().as_nanos() {
        Some(AnimationFrameTick {
            sprite_index: animation.frames.last()?.sprite_index,
            delay: None,
        })
    } else {
        frame_at_elapsed(animation, elapsed_nanos)
    }
}

fn frame_at_elapsed(animation: &Animation, elapsed_nanos: u128) -> Option<AnimationFrameTick> {
    let mut remaining_elapsed = elapsed_nanos;
    for frame in &animation.frames {
        let frame_nanos = frame.duration.as_nanos().max(/*other*/ 1);
        if remaining_elapsed < frame_nanos {
            return Some(AnimationFrameTick {
                sprite_index: frame.sprite_index,
                delay: Some(nanos_to_duration(frame_nanos - remaining_elapsed)),
            });
        }
        remaining_elapsed = remaining_elapsed.saturating_sub(frame_nanos);
    }

    Some(AnimationFrameTick {
        sprite_index: animation.frames.last()?.sprite_index,
        delay: None,
    })
}

fn nanos_to_duration(nanos: u128) -> Duration {
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn notification_height(notification: &PetNotification) -> u16 {
    if notification.body == notification.kind.label() {
        1
    } else {
        2
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BlankRectRejection {
    InvalidRect {
        rect: Rect,
        buffer_area: Rect,
    },
    MissingCell {
        x: u16,
        y: u16,
    },
    Cell {
        x: u16,
        y: u16,
        reason: BlankCellRejection,
        symbol: String,
        background: Option<Color>,
        modifiers: Modifier,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlankCellRejection {
    Skipped,
    EmptySymbol,
    NonWhitespace,
    VisualModifier,
    WideSymbolContinuation,
}

fn rect_blank_rejection(
    buffer: &Buffer,
    movement_bounds: Rect,
    rect: Rect,
) -> Option<BlankRectRejection> {
    if rect.width == 0 || rect.height == 0 || !rect_is_inside(rect, movement_bounds) {
        return Some(BlankRectRejection::InvalidRect {
            rect,
            buffer_area: movement_bounds,
        });
    }

    let checked_area = rect_intersection(rect, buffer.area);
    for y in checked_area.y..checked_area.bottom() {
        for x in checked_area.x..checked_area.right() {
            let Some(cell) = buffer.cell((x, y)) else {
                return Some(BlankRectRejection::MissingCell { x, y });
            };
            if let Some(reason) = cell_blank_rejection(cell) {
                return Some(blank_cell_rejection(cell, x, y, reason));
            }
            if cell_is_covered_by_wide_symbol(buffer, x, y) {
                return Some(blank_cell_rejection(
                    cell,
                    x,
                    y,
                    BlankCellRejection::WideSymbolContinuation,
                ));
            }
        }
    }
    None
}

fn blank_cell_rejection(
    cell: &ratatui::buffer::Cell,
    x: u16,
    y: u16,
    reason: BlankCellRejection,
) -> BlankRectRejection {
    BlankRectRejection::Cell {
        x,
        y,
        reason,
        symbol: cell.symbol().to_string(),
        background: cell.style().bg,
        modifiers: cell.style().add_modifier,
    }
}

fn cell_blank_rejection(cell: &ratatui::buffer::Cell) -> Option<BlankCellRejection> {
    if cell.skip {
        return Some(BlankCellRejection::Skipped);
    }

    let symbol = cell.symbol();
    if symbol.is_empty() {
        return Some(BlankCellRejection::EmptySymbol);
    }
    if !symbol.chars().all(char::is_whitespace) {
        return Some(BlankCellRejection::NonWhitespace);
    }

    let visually_occupied_modifiers =
        Modifier::REVERSED | Modifier::UNDERLINED | Modifier::CROSSED_OUT;
    if cell
        .style()
        .add_modifier
        .intersects(visually_occupied_modifiers)
    {
        return Some(BlankCellRejection::VisualModifier);
    }
    None
}

fn cell_is_covered_by_wide_symbol(buffer: &Buffer, x: u16, y: u16) -> bool {
    let mut prev_x = x;
    while prev_x > buffer.area.x {
        prev_x = prev_x.saturating_sub(1);
        let Some(cell) = buffer.cell((prev_x, y)) else {
            return false;
        };
        let width = cell.symbol().width() as u16;
        if width > 1 && prev_x.saturating_add(width) > x {
            return true;
        }
    }
    false
}

fn rect_is_inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn rect_intersection(lhs: Rect, rhs: Rect) -> Rect {
    let x = lhs.x.max(rhs.x);
    let y = lhs.y.max(rhs.y);
    let right = lhs.right().min(rhs.right());
    let bottom = lhs.bottom().min(rhs.bottom());
    Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}

#[cfg(test)]
pub(crate) fn test_ambient_pet(
    frame_requester: FrameRequester,
    animations_enabled: bool,
) -> AmbientPet {
    AmbientPet {
        pet: Pet {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            description: String::new(),
            spritesheet_path: PathBuf::from("spritesheet.webp"),
            frame_width: 192,
            frame_height: 208,
            columns: 8,
            rows: 9,
            frame_count: 72,
            animations: HashMap::from([("idle".to_string(), test_animation())]),
        },
        support: PetImageSupport::Supported(ImageProtocol::Kitty),
        frames: vec![PathBuf::from("frame-0.png"), PathBuf::from("frame-1.png")],
        sixel_dir: PathBuf::new(),
        frame_requester,
        notification: None,
        animation_started_at: Instant::now()
            .checked_sub(Duration::from_millis(/*millis*/ 15))
            .unwrap(),
        animations_enabled,
        movement: PetMovement::disabled(),
        movement_started_at: Instant::now(),
    }
}

fn configured_movement() -> PetMovement {
    let raw_value = env::var(UNSAFE_TUI_PET_MOVEMENT_ENV_VAR).ok();
    let movement = movement_from_env_value(raw_value.as_deref());
    tracing::trace!(
        target: MOVEMENT_LOG_TARGET,
        env_var = UNSAFE_TUI_PET_MOVEMENT_ENV_VAR,
        value = ?raw_value,
        active = movement.is_active(),
        "resolved ambient pet movement setting"
    );
    movement
}

fn movement_from_env_value(value: Option<&str>) -> PetMovement {
    match value {
        Some("lane-patrol") => PetMovement::lane_patrol(),
        Some(_) | None => PetMovement::disabled(),
    }
}

fn lane_patrol_target(home: Rect, movement_bounds: Rect) -> Option<PetMovementTarget> {
    if home.width == 0
        || home.height == 0
        || home.x < movement_bounds.x
        || home.right() > movement_bounds.right()
    {
        return None;
    }

    let lowest_home_y_inside_bounds = movement_bounds.bottom().checked_sub(home.height)?;
    if lowest_home_y_inside_bounds < movement_bounds.y {
        return None;
    }

    let bounded_home_y = home.y.min(lowest_home_y_inside_bounds);
    let rows_up = bounded_home_y
        .saturating_sub(movement_bounds.y)
        .min(LANE_PATROL_MAX_ROWS);
    if rows_up == 0 {
        return None;
    }

    let target = PetMovementTarget::new(home.x, bounded_home_y - rows_up);
    let target_rect = Rect::new(target.x(), target.y(), home.width, home.height);
    rect_is_inside(target_rect, movement_bounds).then_some(target)
}

fn protocol_allows_movement(protocol: ImageProtocol) -> bool {
    match protocol {
        ImageProtocol::Kitty | ImageProtocol::KittyLocalFile => true,
        ImageProtocol::Sixel => false,
    }
}

#[cfg(test)]
fn test_animation() -> Animation {
    Animation {
        frames: vec![
            AnimationFrame {
                sprite_index: 0,
                duration: Duration::from_millis(/*millis*/ 10),
            },
            AnimationFrame {
                sprite_index: 1,
                duration: Duration::from_millis(/*millis*/ 10),
            },
        ],
        loop_start: Some(/*loop_start*/ 0),
        fallback: "idle".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::style::Style;

    #[test]
    fn notification_labels_match_codex_app_vocabulary() {
        assert_eq!(PetNotificationKind::Running.label(), "Running");
        assert_eq!(PetNotificationKind::Waiting.label(), "Needs input");
        assert_eq!(PetNotificationKind::Review.label(), "Ready");
        assert_eq!(PetNotificationKind::Failed.label(), "Blocked");
    }

    #[test]
    fn animation_frame_uses_per_frame_duration() {
        let animation = test_animation();

        assert_eq!(
            current_animation_frame(&animation, Duration::from_millis(/*millis*/ 15)),
            Some(AnimationFrameTick {
                sprite_index: 1,
                delay: Some(Duration::from_millis(/*millis*/ 5)),
            })
        );
    }

    #[test]
    fn reduced_motion_uses_stable_first_frame_and_schedules_no_follow_up() {
        let pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );

        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-0.png")));
        assert_eq!(pet.next_frame_delay(), None);
    }

    #[test]
    fn disabled_movement_draws_current_home_placement() {
        let pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        let area = Rect::new(0, 0, 80, 24);

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::without_movement(),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
        assert_eq!(draw.columns, 9);
        assert_eq!(draw.rows, 5);
    }

    #[test]
    fn sixel_protocol_stays_home_when_movement_is_configured() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        pet.set_image_support_for_tests(PetImageSupport::Supported(ImageProtocol::Sixel));
        let area = Rect::new(0, 0, 80, 24);
        let buffer = Buffer::empty(area);

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }

    #[test]
    fn lane_patrol_from_rendered_buffer_moves_inside_right_lane() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let buffer = Buffer::empty(area);

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::from_buffer_with_movement_elapsed(
                    &buffer,
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 14);
        assert_eq!(draw.columns, 9);
        assert_eq!(draw.rows, 5);
    }

    #[test]
    fn lane_patrol_from_short_rendered_buffer_moves_when_sprite_fits() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 41);
        let rendered_buffer = Buffer::empty(Rect::new(0, 34, 80, 6));

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::from_buffer_with_movement_elapsed(
                    &rendered_buffer,
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 34);
        assert_eq!(draw.columns, 9);
        assert_eq!(draw.rows, 5);
    }

    #[test]
    fn lane_patrol_uses_full_bounds_when_rendered_buffer_is_short() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let rendered_buffer = Buffer::empty(Rect::new(0, 18, 80, 6));

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::from_buffer_with_movement_elapsed(
                    &rendered_buffer,
                    Duration::from_millis(/*millis*/ 900),
                )
                .with_movement_bounds(area),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 14);
        assert_eq!(draw.columns, 9);
        assert_eq!(draw.rows, 5);
    }

    #[test]
    fn lane_patrol_full_bounds_still_rejects_occupied_rendered_cells() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let mut rendered_buffer = Buffer::empty(Rect::new(0, 18, 80, 6));
        rendered_buffer[(71, 18)]
            .set_symbol("X")
            .set_style(Style::default());

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::from_buffer_with_movement_elapsed(
                    &rendered_buffer,
                    Duration::from_millis(/*millis*/ 900),
                )
                .with_movement_bounds(area),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }

    #[test]
    fn lane_patrol_without_room_in_rendered_buffer_stays_home() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 41);
        let rendered_buffer = Buffer::empty(Rect::new(0, 36, 80, 5));

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::from_buffer_with_movement_elapsed(
                    &rendered_buffer,
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 35);
    }

    #[test]
    fn lane_patrol_stays_home_when_animations_are_disabled() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let buffer = Buffer::empty(area);

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::from_buffer_with_movement_elapsed(
                    &buffer,
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
        assert_eq!(pet.next_frame_delay(), None);
    }

    #[test]
    fn movement_from_env_value_enables_lane_patrol_without_mutating_env() {
        assert_eq!(
            movement_from_env_value(Some("lane-patrol")),
            PetMovement::lane_patrol()
        );
        assert_eq!(
            movement_from_env_value(Some("bogus")),
            PetMovement::disabled()
        );
        assert_eq!(movement_from_env_value(None), PetMovement::disabled());
    }

    #[test]
    fn occupied_movement_target_draws_home() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        buffer[(71, 12)].set_symbol("X").set_style(Style::default());

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }

    #[test]
    fn background_blank_movement_target_is_safe() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        buffer[(71, 12)]
            .set_symbol(" ")
            .set_style(Style::default().bg(Color::Blue));

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 12);
    }

    #[test]
    fn struck_blank_movement_target_draws_home() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        buffer[(71, 12)]
            .set_symbol(" ")
            .set_style(Style::default().add_modifier(Modifier::CROSSED_OUT));

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }

    #[test]
    fn wide_glyph_continuation_in_target_draws_home() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        buffer.set_string(70, 12, "\u{754c}", Style::default());

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }

    #[test]
    fn occupied_intermediate_movement_rect_draws_home() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        buffer[(71, 15)].set_symbol("X").set_style(Style::default());

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 450),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }

    #[test]
    fn safe_movement_target_draws_inside_right_lane() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let buffer = Buffer::empty(area);

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 12);
        assert_eq!(draw.columns, 9);
        assert_eq!(draw.rows, 5);
    }

    #[test]
    fn movement_target_outside_rendered_buffer_draws_home() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        pet.enable_lane_patrol_for_tests();
        let area = Rect::new(0, 0, 80, 24);
        let rendered_buffer = Buffer::empty(Rect::new(0, 20, 80, 4));

        let draw = pet
            .draw_request(
                area,
                area.bottom(),
                AmbientPetDrawContext::with_movement_target(
                    &rendered_buffer,
                    PetMovementTarget::new(71, 12),
                    Duration::from_millis(/*millis*/ 900),
                ),
            )
            .expect("draw request");

        assert_eq!(draw.x, 71);
        assert_eq!(draw.y, 18);
    }
}
