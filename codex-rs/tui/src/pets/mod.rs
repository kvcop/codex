//! Ambient terminal pets configured from the /pets slash command.
//!
//! The TUI treats built-in and custom pets differently on purpose:
//! built-in pets are versioned application assets fetched on demand into a
//! managed CODEX_HOME cache, while custom pets remain entirely user-owned data
//! under `$CODEX_HOME/pets/<pet-id>/pet.json` or legacy avatar directories.
//!
//! This module owns the TUI-facing contracts around that split:
//! resolving a selected pet id, preparing frames for terminal image protocols,
//! rendering the ambient sprite and picker preview, and preserving enough
//! metadata for `/pets` to behave like a first-class configuration surface.
//! It does not own config persistence or popup orchestration; callers must
//! ensure a built-in asset exists before loading it and must persist the final
//! selection only after the load succeeds.

use std::io::Write;
use std::path::PathBuf;

mod ambient;
mod asset_pack;
mod catalog;
mod frames;
mod image_protocol;
mod model;
mod movement;
mod picker;
mod preview;
mod sixel;

use anyhow::Context;
use anyhow::Result;

pub(crate) use ambient::AmbientPet;
pub(crate) use ambient::AmbientPetDraw;
pub(crate) use ambient::AmbientPetDrawContext;
pub(crate) use ambient::PetNotificationKind;
#[cfg(test)]
pub(crate) use ambient::test_ambient_pet;
pub(crate) use asset_pack::builtin_spritesheet_path;
#[cfg(test)]
pub(crate) use asset_pack::write_test_pack;
#[cfg(test)]
pub(crate) use image_protocol::ImageProtocol;
pub(crate) use image_protocol::PetImageSupport;
#[cfg(test)]
pub(crate) use image_protocol::PetImageUnsupportedReason;
#[cfg(not(test))]
pub(crate) use image_protocol::detect_pet_image_support;
pub(crate) use picker::PET_PICKER_VIEW_ID;
pub(crate) use picker::build_pet_picker_params;
pub(crate) use preview::PetPickerPreviewState;

pub(crate) const DEFAULT_PET_ID: &str = "codex";
pub(crate) const DISABLED_PET_ID: &str = "disabled";

/// Ensure that a selected built-in pet has a locally cached spritesheet.
///
/// Custom pets are intentionally a no-op here because their source of truth is
/// already local. Callers should invoke this before loading a built-in pet for
/// preview or selection; skipping it would make first-use preview and
/// persistence failures depend on deeper image-loading errors instead of the
/// asset-fetch boundary.
pub(crate) fn ensure_builtin_pack_for_pet(
    pet_id: &str,
    codex_home: &std::path::Path,
) -> Result<()> {
    if let Some(pet) = catalog::builtin_pet(pet_id) {
        asset_pack::ensure_builtin_pet(codex_home, pet)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum PetImageRenderError {
    Terminal(std::io::Error),
    Asset(anyhow::Error),
}

impl std::fmt::Display for PetImageRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal(err) => write!(f, "terminal image write failed: {err}"),
            Self::Asset(err) => write!(f, "pet image asset unavailable: {err}"),
        }
    }
}

impl std::error::Error for PetImageRenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Terminal(err) => Some(err),
            Self::Asset(err) => Some(err.as_ref()),
        }
    }
}

impl From<std::io::Error> for PetImageRenderError {
    fn from(err: std::io::Error) -> Self {
        Self::Terminal(err)
    }
}

pub(crate) fn render_ambient_pet_image(
    writer: &mut impl Write,
    state: &mut PetImageRenderState,
    request: Option<AmbientPetDraw>,
) -> std::result::Result<(), PetImageRenderError> {
    render_pet_image(writer, state, AMBIENT_PET_IMAGE_IDS, request)
}

pub(crate) fn render_pet_picker_preview_image(
    writer: &mut impl Write,
    state: &mut PetImageRenderState,
    request: Option<AmbientPetDraw>,
) -> std::result::Result<(), PetImageRenderError> {
    render_pet_image(writer, state, PET_PICKER_PREVIEW_IMAGE_IDS, request)
}

const AMBIENT_PET_IMAGE_IDS: PetImageIds = PetImageIds::new(0xC0DE);
const PET_PICKER_PREVIEW_IMAGE_IDS: PetImageIds = PetImageIds::new(0xC1DE);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PetImageIds {
    primary: u32,
    alternate: u32,
}

impl PetImageIds {
    const fn new(primary: u32) -> Self {
        Self {
            primary,
            alternate: primary + 1,
        }
    }

    fn all(self) -> [u32; 2] {
        [self.primary, self.alternate]
    }
}

#[derive(Debug, Default)]
pub(crate) struct PetImageRenderState {
    last_sixel_clear_area: Option<SixelClearArea>,
    last_protocol: Option<image_protocol::ImageProtocol>,
    last_draw_key: Option<PetImageDrawKey>,
    last_kitty_image_id: Option<u32>,
}

impl PetImageRenderState {
    pub(crate) fn force_next_draw(&mut self) {
        self.last_draw_key = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PetImageDrawKey {
    frame: PathBuf,
    protocol: image_protocol::ImageProtocol,
    x: u16,
    y: u16,
    clear_top_y: u16,
    columns: u16,
    rows: u16,
    height_px: u16,
    sixel_dir: PathBuf,
}

impl From<&AmbientPetDraw> for PetImageDrawKey {
    fn from(request: &AmbientPetDraw) -> Self {
        Self {
            frame: request.frame.clone(),
            protocol: request.protocol,
            x: request.x,
            y: request.y,
            clear_top_y: request.clear_top_y,
            columns: request.columns,
            rows: request.rows,
            height_px: request.height_px,
            sixel_dir: request.sixel_dir.clone(),
        }
    }
}

fn render_pet_image(
    writer: &mut impl Write,
    state: &mut PetImageRenderState,
    image_ids: PetImageIds,
    request: Option<AmbientPetDraw>,
) -> std::result::Result<(), PetImageRenderError> {
    use crossterm::cursor::MoveTo;
    use crossterm::cursor::RestorePosition;
    use crossterm::cursor::SavePosition;
    use crossterm::queue;
    use image_protocol::ImageProtocol;

    let Some(request) = request else {
        state.last_draw_key = None;
        let had_kitty_protocol = state.last_protocol.take().is_some_and(is_kitty_protocol);
        let had_kitty_image = state.last_kitty_image_id.take().is_some();
        if had_kitty_protocol || had_kitty_image {
            for image_id in image_ids.all() {
                write!(writer, "{}", image_protocol::kitty_delete_image(image_id))?;
            }
        }
        if let Some(area) = state.last_sixel_clear_area.take() {
            queue!(writer, SavePosition)?;
            clear_sixel_area(writer, area)?;
            queue!(writer, RestorePosition)?;
        }
        writer.flush()?;
        return Ok(());
    };

    let draw_key = PetImageDrawKey::from(&request);
    let request_is_kitty = is_kitty_protocol(request.protocol);
    if request_is_kitty
        && state.last_protocol == Some(request.protocol)
        && state.last_draw_key.as_ref() == Some(&draw_key)
    {
        return Ok(());
    }

    let next_kitty_image_id = if request_is_kitty {
        Some(match state.last_kitty_image_id {
            Some(previous) if previous == image_ids.primary => image_ids.alternate,
            Some(previous) if previous == image_ids.alternate => image_ids.primary,
            Some(_) | None => image_ids.primary,
        })
    } else {
        None
    };
    let payload = match request.protocol {
        ImageProtocol::Kitty => AmbientPetPayload::Text(
            image_protocol::kitty_transmit_png_with_id(
                &request.frame,
                request.columns,
                request.rows,
                next_kitty_image_id,
            )
            .map_err(PetImageRenderError::Asset)?,
        ),
        ImageProtocol::KittyLocalFile => AmbientPetPayload::Text(
            image_protocol::kitty_transmit_png_file_with_id(
                &request.frame,
                request.columns,
                request.rows,
                next_kitty_image_id,
            )
            .map_err(PetImageRenderError::Asset)?,
        ),
        ImageProtocol::Sixel => {
            let path =
                image_protocol::sixel_frame(&request.frame, &request.sixel_dir, request.height_px)
                    .map_err(PetImageRenderError::Asset)?;
            let sixel = std::fs::read(&path)
                .with_context(|| format!("read {}", path.display()))
                .map_err(PetImageRenderError::Asset)?;
            AmbientPetPayload::Bytes(sixel)
        }
    };

    let previous_protocol_was_kitty = state.last_protocol.is_some_and(is_kitty_protocol);
    let previous_kitty_image_id = state.last_kitty_image_id;
    let pre_draw_delete_kitty = previous_protocol_was_kitty && !request_is_kitty;
    let post_draw_delete_kitty_image_id = if request_is_kitty {
        previous_kitty_image_id.filter(|previous| Some(*previous) != next_kitty_image_id)
    } else {
        None
    };

    if pre_draw_delete_kitty {
        state.last_draw_key = None;
        for image_id in image_ids.all() {
            write!(writer, "{}", image_protocol::kitty_delete_image(image_id))?;
        }
    }

    queue!(writer, SavePosition)?;
    let current_sixel_clear_area = if matches!(request.protocol, ImageProtocol::Sixel) {
        Some(SixelClearArea::from(&request))
    } else {
        None
    };
    if let Some(previous_area) = state.last_sixel_clear_area.take()
        && Some(previous_area) != current_sixel_clear_area
    {
        clear_sixel_area(writer, previous_area)?;
    }
    if let Some(area) = current_sixel_clear_area {
        clear_sixel_area(writer, area)?;
        state.last_sixel_clear_area = Some(area);
    }
    queue!(writer, MoveTo(request.x, request.y))?;
    match payload {
        AmbientPetPayload::Text(payload) => write!(writer, "{payload}")?,
        AmbientPetPayload::Bytes(payload) => writer.write_all(&payload)?,
    }
    if let Some(image_id) = post_draw_delete_kitty_image_id {
        write!(
            writer,
            "{}",
            image_protocol::kitty_delete_image_preserving_data(image_id)
        )?;
    }
    queue!(writer, RestorePosition)?;
    writer.flush()?;
    state.last_protocol = Some(request.protocol);
    state.last_draw_key = Some(draw_key);
    state.last_kitty_image_id = next_kitty_image_id;
    Ok(())
}

enum AmbientPetPayload {
    Text(String),
    Bytes(Vec<u8>),
}

fn is_kitty_protocol(protocol: image_protocol::ImageProtocol) -> bool {
    matches!(
        protocol,
        image_protocol::ImageProtocol::Kitty | image_protocol::ImageProtocol::KittyLocalFile
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SixelClearArea {
    x: u16,
    clear_top_y: u16,
    clear_bottom_y: u16,
    columns: u16,
}

impl From<&AmbientPetDraw> for SixelClearArea {
    fn from(request: &AmbientPetDraw) -> Self {
        Self {
            x: request.x,
            clear_top_y: request.clear_top_y,
            clear_bottom_y: request.y.saturating_add(request.rows),
            columns: request.columns,
        }
    }
}

fn clear_sixel_area(writer: &mut impl Write, area: SixelClearArea) -> std::io::Result<()> {
    use crossterm::cursor::MoveTo;
    use crossterm::queue;

    let blank = " ".repeat(area.columns.into());
    for row in area.clear_top_y..area.clear_bottom_y {
        queue!(writer, MoveTo(area.x, row))?;
        write!(writer, "{blank}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::io;
    use std::path::PathBuf;

    use super::image_protocol::ImageProtocol;
    use super::*;

    fn kitty_request(frame: PathBuf) -> AmbientPetDraw {
        AmbientPetDraw {
            frame,
            protocol: ImageProtocol::Kitty,
            x: 2,
            y: 3,
            clear_top_y: 3,
            columns: 4,
            rows: 5,
            height_px: 75,
            sixel_dir: PathBuf::new(),
        }
    }

    #[test]
    fn ambient_pet_image_restores_cursor_after_drawing() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let request = AmbientPetDraw {
            frame,
            protocol: ImageProtocol::Kitty,
            x: 2,
            y: 3,
            clear_top_y: 3,
            columns: 4,
            rows: 5,
            height_px: 75,
            sixel_dir: PathBuf::new(),
        };
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();

        let output = String::from_utf8(output).unwrap();
        let save = output.find("\x1b7").expect("saves cursor position");
        let move_to = output.find("\x1b[4;3H").expect("moves to pet position");
        let image = output.find("cG5n").expect("writes image payload");
        let restore = output.find("\x1b8").expect("restores cursor position");
        assert!(save < move_to);
        assert!(move_to < image);
        assert!(image < restore);
    }

    #[test]
    fn kitty_pet_image_clear_deletes_without_moving_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let request = AmbientPetDraw {
            frame,
            protocol: ImageProtocol::Kitty,
            x: 2,
            y: 3,
            clear_top_y: 3,
            columns: 4,
            rows: 5,
            height_px: 75,
            sixel_dir: PathBuf::new(),
        };
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, /*request*/ None).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Ga=d,d=I,i=49374,q=2;"));
        assert!(!output.contains("\x1b7"));
        assert!(!output.contains("\x1b["));
        assert!(!output.contains("\x1b8"));
    }

    #[test]
    fn kitty_pet_image_skips_identical_redraw() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let request = kitty_request(frame);
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request.clone())).unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();

        assert!(
            output.is_empty(),
            "expected identical redraw to emit no terminal bytes, got {:?}",
            String::from_utf8_lossy(&output)
        );
    }

    #[test]
    fn kitty_pet_image_forced_redraw_bypasses_identical_dedupe() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let request = kitty_request(frame);
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request.clone())).unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, Some(request.clone())).unwrap();
        assert!(
            output.is_empty(),
            "expected identical redraw to be skipped before forcing"
        );

        state.force_next_draw();
        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("a=T,t=d,f=100,c=4,r=5,q=2,i=49375,m=0;"));
        assert!(output.contains("Ga=d,d=i,i=49374,q=2;"));
        assert!(output.contains("cG5n"));
    }

    #[test]
    fn kitty_pet_image_changed_frame_still_renders() {
        let dir = tempfile::tempdir().unwrap();
        let first_frame = dir.path().join("first.png");
        std::fs::write(&first_frame, b"one").unwrap();
        let second_frame = dir.path().join("second.png");
        std::fs::write(&second_frame, b"two").unwrap();
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(kitty_request(first_frame)))
            .unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, Some(kitty_request(second_frame)))
            .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("a=T,t=d,f=100,c=4,r=5,q=2,i=49375,m=0;"));
        assert!(output.contains("Ga=d,d=i,i=49374,q=2;"));
        assert!(output.contains("dHdv"));
    }

    #[test]
    fn kitty_pet_image_frame_transition_draws_new_before_deleting_old() {
        let dir = tempfile::tempdir().unwrap();
        let first_frame = dir.path().join("first.png");
        std::fs::write(&first_frame, b"one").unwrap();
        let second_frame = dir.path().join("second.png");
        std::fs::write(&second_frame, b"two").unwrap();
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(kitty_request(first_frame)))
            .unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, Some(kitty_request(second_frame)))
            .unwrap();

        let output = String::from_utf8(output).unwrap();
        let new_frame = output
            .find("a=T,t=d,f=100,c=4,r=5,q=2,i=49375,m=0;")
            .expect("draws the new frame using the alternate image id");
        let old_delete = output
            .find("Ga=d,d=i,i=49374,q=2;")
            .expect("deletes the old image id without freeing image data");
        assert!(new_frame < old_delete);
        assert!(!output.contains("Ga=d,d=I,i=49374,q=2;"));
        assert!(!output.contains("Ga=d,d=i,i=49375,q=2;"));
        assert!(!output.contains("Ga=d,d=I,i=49375,q=2;"));
    }

    #[test]
    fn kitty_pet_image_clear_after_skipped_redraw_deletes_last_image() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let request = kitty_request(frame);
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request.clone())).unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();
        assert!(output.is_empty());
        render_ambient_pet_image(&mut output, &mut state, /*request*/ None).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Ga=d,d=I,i=49374,q=2;"));
        assert!(!output.contains("\x1b7"));
        assert!(!output.contains("\x1b["));
        assert!(!output.contains("\x1b8"));
    }

    #[test]
    fn kitty_pet_image_clear_after_frame_transition_deletes_owned_images() {
        let dir = tempfile::tempdir().unwrap();
        let first_frame = dir.path().join("first.png");
        std::fs::write(&first_frame, b"one").unwrap();
        let second_frame = dir.path().join("second.png");
        std::fs::write(&second_frame, b"two").unwrap();
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(kitty_request(first_frame)))
            .unwrap();
        render_ambient_pet_image(&mut output, &mut state, Some(kitty_request(second_frame)))
            .unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, /*request*/ None).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Ga=d,d=I,i=49374,q=2;"));
        assert!(output.contains("Ga=d,d=I,i=49375,q=2;"));
        assert!(!output.contains("\x1b7"));
        assert!(!output.contains("\x1b["));
        assert!(!output.contains("\x1b8"));
    }

    #[test]
    fn ambient_and_picker_preview_use_separate_kitty_image_ranges() {
        assert_ne!(
            AMBIENT_PET_IMAGE_IDS.all(),
            PET_PICKER_PREVIEW_IMAGE_IDS.all()
        );
        assert!(
            AMBIENT_PET_IMAGE_IDS
                .all()
                .iter()
                .all(|id| !PET_PICKER_PREVIEW_IMAGE_IDS.all().contains(id))
        );
    }

    #[test]
    fn kitty_local_file_pet_image_uses_file_reference_without_inline_payload() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let request = AmbientPetDraw {
            frame,
            protocol: ImageProtocol::KittyLocalFile,
            x: 2,
            y: 3,
            clear_top_y: 3,
            columns: 4,
            rows: 2,
            height_px: 75,
            sixel_dir: PathBuf::new(),
        };
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("a=d,d=I,i=49374,q=2;"));
        assert!(!output.contains("a=d,d=i,i=49374,q=2;"));
        assert!(output.contains("\x1b[4;3H"));
        assert!(output.contains("a=T,t=f,f=100,c=4,r=2,q=2,i=49374;"));
        assert!(!output.contains("cG5n"));
        assert!(output.contains("\x1b8"));
    }

    #[test]
    fn sixel_pet_image_clears_cell_area_before_redrawing() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let sixel_dir = dir.path().join("sixel");
        std::fs::create_dir(&sixel_dir).unwrap();
        let sixel_frame = sixel_dir.join("frame_h75_v2.six");
        std::fs::write(&sixel_frame, b"fake-sixel").unwrap();
        let request = AmbientPetDraw {
            frame,
            protocol: ImageProtocol::Sixel,
            x: 2,
            y: 3,
            clear_top_y: 1,
            columns: 4,
            rows: 2,
            height_px: 75,
            sixel_dir,
        };
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\x1b[2;3H    \x1b[3;3H    \x1b[4;3H    \x1b[5;3H    \x1b[4;3H"));
        assert!(output.contains("fake-sixel"));
        assert!(output.contains("\x1b8"));
    }

    #[test]
    fn sixel_pet_image_clear_erases_last_drawn_area() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("frame.png");
        std::fs::write(&frame, b"png").unwrap();
        let sixel_dir = dir.path().join("sixel");
        std::fs::create_dir(&sixel_dir).unwrap();
        let sixel_frame = sixel_dir.join("frame_h75_v2.six");
        std::fs::write(&sixel_frame, b"fake-sixel").unwrap();
        let request = AmbientPetDraw {
            frame,
            protocol: ImageProtocol::Sixel,
            x: 2,
            y: 3,
            clear_top_y: 1,
            columns: 4,
            rows: 2,
            height_px: 75,
            sixel_dir,
        };
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap();
        output.clear();
        render_ambient_pet_image(&mut output, &mut state, /*request*/ None).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("Ga=d,d=I,i=49374,q=2;"));
        assert!(output.contains("\x1b7"));
        assert!(output.contains("\x1b[2;3H    \x1b[3;3H    \x1b[4;3H    \x1b[5;3H    "));
        assert!(output.contains("\x1b8"));
        assert!(!output.contains("fake-sixel"));
    }

    #[test]
    fn missing_frame_is_an_asset_error() {
        let dir = tempfile::tempdir().unwrap();
        let request = AmbientPetDraw {
            frame: dir.path().join("missing.png"),
            protocol: ImageProtocol::Kitty,
            x: 2,
            y: 3,
            clear_top_y: 3,
            columns: 4,
            rows: 5,
            height_px: 75,
            sixel_dir: PathBuf::new(),
        };
        let mut output = Vec::new();
        let mut state = PetImageRenderState::default();

        let err = render_ambient_pet_image(&mut output, &mut state, Some(request)).unwrap_err();

        assert!(matches!(err, PetImageRenderError::Asset(_)));
        assert!(err.source().is_some());
    }

    #[test]
    fn writer_failure_is_a_terminal_error() {
        struct FailingWriter;

        impl io::Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "test writer failed",
                ))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut writer = FailingWriter;
        let mut state = PetImageRenderState {
            last_protocol: Some(ImageProtocol::Kitty),
            ..Default::default()
        };

        let err = render_ambient_pet_image(&mut writer, &mut state, /*request*/ None).unwrap_err();

        assert!(matches!(err, PetImageRenderError::Terminal(_)));
        assert!(err.source().is_some());
    }
}
