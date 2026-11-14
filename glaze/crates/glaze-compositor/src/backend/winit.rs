//! Nested backend: a winit window inside a Wayland (or X11) parent, rendered
//! with Smithay's GLES renderer (llvmpipe in the dev VM).
//!
//! Pacing: winit is created with vsync off (Smithay's default), so frames are
//! paced by a calloop timer at the output refresh (60 Hz). A frame is only
//! rendered when something is dirty (client commit, input, layout change) or
//! an animation is running; the damage tracker limits each frame to what
//! changed. The DRM backend will pace on real vblank events instead.

use std::io;
use std::time::{Duration, Instant};

use calloop::timer::{TimeoutAction, Timer};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::Window as WinitWindow;
use smithay::utils::Transform;
use tracing::{info, warn};

use crate::backend::BackendData;
use crate::state::State;

pub const OUTPUT_NAME: &str = "Glaze-nested-1";
const REFRESH_MHZ: i32 = 60_000;
const FRAME: Duration = Duration::from_micros(16_667);
const STATS_EVERY: Duration = Duration::from_secs(5);

pub struct WinitData {
    pub backend: WinitGraphicsBackend<GlesRenderer>,
    pub damage_tracker: OutputDamageTracker,
}

/// Create the winit window, the output, and the event and frame sources.
pub fn init(state: &mut State, size: (i32, i32)) -> io::Result<()> {
    let attrs = WinitWindow::default_attributes()
        .with_inner_size(LogicalSize::new(size.0 as f64, size.1 as f64))
        .with_title("Glaze compositor (nested)")
        .with_visible(true);
    let (backend, winit_loop) = winit::init_from_attributes::<GlesRenderer>(attrs)
        .map_err(|e| io::Error::other(format!("winit/EGL init failed: {e}")))?;

    let size = backend.window_size();
    let mode = Mode {
        size,
        refresh: REFRESH_MHZ,
    };
    let output = Output::new(
        OUTPUT_NAME.to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Glaze".into(),
            model: "Nested".into(),
        },
    );
    let _global = output.create_global::<State>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    state.output = Some(output.clone());
    state.outputs = vec![output.clone()];
    let damage_tracker = OutputDamageTracker::from_output(&output);
    info!(
        width = size.w,
        height = size.h,
        renderer = "gles",
        "winit backend ready"
    );
    state.backend = Some(BackendData::Winit(WinitData {
        backend,
        damage_tracker,
    }));
    state.update_work_area();

    state
        .loop_handle
        .insert_source(winit_loop, move |event, _, state| match event {
            WinitEvent::Resized { size, .. } => {
                let Some(output) = state.output.clone() else {
                    return;
                };
                let mode = Mode {
                    size,
                    refresh: REFRESH_MHZ,
                };
                output.change_current_state(Some(mode), None, None, None);
                output.set_preferred(mode);
                info!(width = size.w, height = size.h, "nested window resized");
                state.update_work_area();
                state.needs_redraw = true;
            }
            WinitEvent::Input(event) => crate::input::process_input_event(state, event),
            WinitEvent::Redraw => state.needs_redraw = true,
            WinitEvent::CloseRequested => {
                info!("parent closed the nested window");
                state.request_exit();
            }
            WinitEvent::Focus(_) => {}
        })
        .map_err(|e| io::Error::other(format!("winit source: {e}")))?;

    state
        .loop_handle
        .insert_source(Timer::from_duration(FRAME), |_, _, state| {
            render_frame(state);
            TimeoutAction::ToDuration(FRAME)
        })
        .map_err(|e| io::Error::other(format!("frame timer: {e}")))?;
    Ok(())
}

/// Render and present one frame if anything is dirty or animating.
pub fn render_frame(state: &mut State) {
    state.retire_animations();
    let animating = state.animating();
    if !state.needs_redraw && !animating {
        return;
    }
    let Some(output) = state.output.clone() else {
        return;
    };
    let Some(BackendData::Winit(mut data)) = state.backend.take() else {
        return;
    };
    let clear = state.palette.desktop.premultiplied();
    let t0 = Instant::now();
    let age = data.backend.buffer_age().unwrap_or(0);
    let result = match data.backend.bind() {
        Ok((renderer, mut fb)) => {
            let elements = crate::render::output_elements(renderer, state, &output);
            match data
                .damage_tracker
                .render_output(renderer, &mut fb, age, &elements, clear)
            {
                Ok(r) => Ok(r.damage.cloned()),
                Err(e) => Err(format!("render: {e:?}")),
            }
        }
        Err(e) => Err(format!("bind: {e:?}")),
    };
    let rendered = match result {
        Ok(Some(damage)) => match data.backend.submit(Some(&damage)) {
            Ok(()) => true,
            Err(e) => {
                warn!(?e, "submit failed");
                false
            }
        },
        Ok(None) => false,
        Err(e) => {
            warn!(e, "frame failed");
            false
        }
    };
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    state.backend = Some(BackendData::Winit(data));
    state.needs_redraw = false;

    if rendered {
        state.frame_stats.push(ms);
        state.frames_total += 1;
    }
    crate::render::send_frames(state, &output, state.now());
    state.after_render(&output);

    if state.last_stats_log.elapsed() >= STATS_EVERY {
        let secs = state.last_stats_log.elapsed().as_secs_f64();
        let n = state.frame_stats.len();
        if let Some((min, avg, p95, max)) = state.frame_stats.take_summary() {
            info!(
                frames = n,
                fps = format!("{:.1}", n as f64 / secs),
                min_ms = format!("{min:.2}"),
                avg_ms = format!("{avg:.2}"),
                p95_ms = format!("{p95:.2}"),
                max_ms = format!("{max:.2}"),
                total = state.frames_total,
                "frame-time (render+submit, CPU wall clock)"
            );
        }
        state.last_stats_log = Instant::now();
    }
}
