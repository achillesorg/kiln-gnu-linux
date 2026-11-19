//! Session backend: Glaze as the compositor of a tty (M4).
//!
//! - **Seat**: libseat (seatd, or logind through libseat), so the compositor
//!   runs as the logged-in user without root. Session pause/resume (VT
//!   switches) suspends libinput and the DRM device and restores both.
//! - **GPU**: udev picks the seat's primary GPU (`card0` on the dev VM,
//!   virtio-gpu without virgl). GBM buffers, EGL on the GBM device and the
//!   GLES renderer (Mesa falls back to `kms_swrast`, i.e. llvmpipe, there).
//! - **Outputs**: every connected connector gets a CRTC, its preferred mode
//!   (or `GLAZE_OUTPUT_MODE=WxH`) and a [`DrmCompositor`]; outputs are laid out
//!   left to right. Hotplug (udev `changed`) rescans connectors. The window
//!   manager works on the first (primary) output; the others show layers and
//!   whatever windows reach them.
//! - **Pacing**: a frame is rendered only when something is dirty or an
//!   animation runs, and never while the previous page flip is pending, so
//!   rendering follows vblank. When a render has no damage no flip happens;
//!   frame callbacks are then sent from a one-frame timer instead, so idle
//!   clients do not spin.
//! - **Input**: libinput through Smithay's `LibinputInputBackend`, fed to the
//!   same [`crate::input::process_input_event`] as the nested backend.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use calloop::timer::{TimeoutAction, Timer};
use calloop::RegistrationToken;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::texture::{TextureRenderBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, Offscreen};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::drm::control::{connector, crtc, Device as ControlDevice, ModeTypeFlags};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::Rectangle;
use smithay::utils::{DeviceFd, Transform};
use tracing::{error, info, warn};

use crate::backend::BackendData;
use crate::state::State;

pub type GbmDrmCompositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

/// Formats tried for the primary plane, in order.
const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Xrgb8888];
const STATS_EVERY: Duration = Duration::from_secs(5);

/// The offscreen composition target of one output.
pub struct OffscreenTarget {
    pub texture: GlesTexture,
    pub buffer: TextureRenderBuffer<GlesTexture>,
    pub tracker: OutputDamageTracker,
    pub size: (i32, i32),
    pub age: usize,
}

pub struct OutputSurface {
    pub offscreen: Option<OffscreenTarget>,
    pub output: Output,
    pub connector: connector::Handle,
    pub compositor: GbmDrmCompositor,
    /// A page flip is queued and its vblank has not arrived yet.
    pub pending: bool,
    /// Something on this output needs a new frame.
    pub dirty: bool,
    /// A frame-callback timer is armed (after an empty render).
    pub idle_timer: bool,
    pub refresh: Duration,
    pub last_vblank: Option<Instant>,
}

pub struct Gpu {
    pub node: DrmNode,
    pub drm: DrmDevice,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub renderer: GlesRenderer,
    pub surfaces: HashMap<crtc::Handle, OutputSurface>,
    pub token: RegistrationToken,
}

pub struct UdevData {
    pub session: LibSeatSession,
    pub primary: DrmNode,
    pub gpu: Option<Gpu>,
    pub libinput: Libinput,
    /// vblank-to-vblank intervals (ms) while frames are flowing.
    pub vblank_stats: crate::anim::FrameStats,
}

impl UdevData {
    pub fn renderer(&mut self) -> Option<&mut GlesRenderer> {
        self.gpu.as_mut().map(|g| &mut g.renderer)
    }
}

/// Set up libseat, udev, libinput and the primary GPU.
pub fn init(state: &mut State) -> io::Result<()> {
    let (session, notifier) = LibSeatSession::new().map_err(|e| {
        io::Error::other(format!(
            "libseat: no seat session ({e}); is seatd or logind running and is this a VT login?"
        ))
    })?;
    let seat_name = session.seat();
    info!(seat = %seat_name, active = session.is_active(), "libseat session");

    let primary = udev::primary_gpu(&seat_name)
        .ok()
        .flatten()
        .and_then(|p| DrmNode::from_path(p).ok())
        .or_else(|| {
            udev::all_gpus(&seat_name)
                .ok()?
                .into_iter()
                .find_map(|p| DrmNode::from_path(p).ok())
        })
        .and_then(|n| n.node_with_type(NodeType::Primary).and_then(Result::ok))
        .ok_or_else(|| io::Error::other("udev: no GPU (DRM card) on this seat"))?;
    info!(node = %primary, "primary GPU");

    let udev_backend =
        UdevBackend::new(&seat_name).map_err(|e| io::Error::other(format!("udev: {e}")))?;

    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat_name)
        .map_err(|()| io::Error::other("libinput: udev_assign_seat failed"))?;
    let input_backend = LibinputInputBackend::new(libinput.clone());
    state
        .loop_handle
        .insert_source(input_backend, |event, _, state| {
            crate::input::process_input_event(state, event);
        })
        .map_err(|e| io::Error::other(format!("libinput source: {e}")))?;

    state.backend = Some(BackendData::Udev(UdevData {
        session,
        primary,
        gpu: None,
        libinput,
        vblank_stats: Default::default(),
    }));
    state.software_cursor = true;

    state
        .loop_handle
        .insert_source(notifier, |event, _, state| match event {
            SessionEvent::PauseSession => pause(state),
            SessionEvent::ActivateSession => resume(state),
        })
        .map_err(|e| io::Error::other(format!("session source: {e}")))?;

    let devices: Vec<(DevT, std::path::PathBuf)> = udev_backend
        .device_list()
        .map(|(id, p)| (id, p.to_owned()))
        .collect();
    let mut last_error = String::new();
    for (id, path) in devices {
        if let Err(e) = device_added(state, id, &path) {
            warn!(path = %path.display(), %e, "GPU not usable");
            last_error = format!(" ({}: {e})", path.display());
        }
    }
    if state.output.is_none() {
        // No pixman fallback yet: fail cleanly so glaze-session starts Sway.
        return Err(io::Error::other(format!(
            "DRM: no connected output could be driven{last_error}"
        )));
    }

    state
        .loop_handle
        .insert_source(udev_backend, |event, _, state| match event {
            UdevEvent::Added { device_id, path } => {
                if let Err(e) = device_added(state, device_id, &path) {
                    warn!(path = %path.display(), %e, "hotplugged GPU not usable");
                }
            }
            UdevEvent::Changed { device_id } => {
                if udev_data(state)
                    .is_some_and(|d| d.gpu.as_ref().is_some_and(|g| g.node.dev_id() == device_id))
                {
                    info!("udev: GPU changed, rescanning connectors");
                    scan_connectors(state);
                }
            }
            UdevEvent::Removed { device_id } => device_removed(state, device_id),
        })
        .map_err(|e| io::Error::other(format!("udev source: {e}")))?;
    Ok(())
}

type DevT = libc::dev_t;

fn udev_data(state: &State) -> Option<&UdevData> {
    match state.backend.as_ref()? {
        BackendData::Udev(d) => Some(d),
        _ => None,
    }
}

fn udev_data_mut(state: &mut State) -> Option<&mut UdevData> {
    match state.backend.as_mut()? {
        BackendData::Udev(d) => Some(d),
        _ => None,
    }
}

fn device_added(state: &mut State, device_id: DevT, path: &Path) -> io::Result<()> {
    let node = DrmNode::from_dev_id(device_id).map_err(io::Error::other)?;
    let Some(data) = udev_data_mut(state) else {
        return Ok(());
    };
    if node != data.primary {
        info!(%node, "ignoring secondary GPU (single-GPU backend)");
        return Ok(());
    }
    if data.gpu.is_some() {
        return Ok(());
    }
    let fd = data
        .session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| io::Error::other(format!("open {}: {e}", path.display())))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier) =
        DrmDevice::new(fd.clone(), true).map_err(|e| io::Error::other(format!("drm: {e}")))?;
    let gbm = GbmDevice::new(fd).map_err(|e| io::Error::other(format!("gbm: {e}")))?;
    // SAFETY: the GBM device outlives the display (both live in `Gpu`).
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }
        .map_err(|e| io::Error::other(format!("EGL on GBM: {e}")))?;
    let ctx = EGLContext::new(&egl).map_err(|e| io::Error::other(format!("EGL context: {e}")))?;
    // SAFETY: the context is current only on this thread.
    let mut renderer =
        unsafe { GlesRenderer::new(ctx) }.map_err(|e| io::Error::other(format!("GLES: {e}")))?;
    info!(%node, "GPU ready: GBM + EGL + GLES");
    let gl_name = crate::render::gl_renderer_name(&mut renderer);
    state.set_renderer(&gl_name);

    let token = state
        .loop_handle
        .insert_source(notifier, move |event, meta, state| match event {
            DrmEvent::VBlank(crtc) => {
                let _ = meta;
                on_vblank(state, crtc);
            }
            DrmEvent::Error(e) => error!(%e, "DRM error"),
        })
        .map_err(|e| io::Error::other(format!("drm source: {e}")))?;

    if let Some(data) = udev_data_mut(state) {
        data.gpu = Some(Gpu {
            node,
            drm,
            gbm,
            renderer,
            surfaces: HashMap::new(),
            token,
        });
    }
    scan_connectors(state);
    Ok(())
}

fn device_removed(state: &mut State, device_id: DevT) {
    let Some(data) = udev_data_mut(state) else {
        return;
    };
    let matches = data
        .gpu
        .as_ref()
        .is_some_and(|g| g.node.dev_id() == device_id);
    if !matches {
        return;
    }
    let gpu = data.gpu.take().expect("checked");
    warn!(node = %gpu.node, "primary GPU removed");
    state.loop_handle.remove(gpu.token);
    for (_, s) in gpu.surfaces {
        remove_output(state, &s.output);
    }
}

fn remove_output(state: &mut State, output: &Output) {
    state.space.unmap_output(output);
    state.outputs.retain(|o| o != output);
    if state.output.as_ref() == Some(output) {
        state.output = state.outputs.first().cloned();
    }
    state.update_work_area();
}

/// Pick the mode: `GLAZE_OUTPUT_MODE=WxH[@Hz]` if offered, else the preferred
/// mode, else the first.
fn pick_mode(
    modes: &[smithay::reexports::drm::control::Mode],
) -> Option<smithay::reexports::drm::control::Mode> {
    if let Ok(want) = std::env::var("GLAZE_OUTPUT_MODE") {
        let (size, hz) = want.split_once('@').unwrap_or((want.as_str(), ""));
        if let Some((w, h)) = size.split_once('x') {
            let (w, h): (u16, u16) = (w.parse().unwrap_or(0), h.parse().unwrap_or(0));
            let hz: u32 = hz.parse().unwrap_or(0);
            if let Some(m) = modes
                .iter()
                .find(|m| m.size() == (w, h) && (hz == 0 || m.vrefresh() == hz))
            {
                return Some(*m);
            }
            warn!(
                want,
                "GLAZE_OUTPUT_MODE not offered by the connector; using the preferred mode"
            );
        }
    }
    modes
        .iter()
        .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
        .or_else(|| modes.first())
        .copied()
}

/// Output scale: `GLAZE_OUTPUT_SCALE`, else `[appearance] scale` from the
/// Glaze config (1, 1.25, 1.5, 2 ...), clamped to 1..=3.
pub fn output_scale() -> smithay::output::Scale {
    let v = std::env::var("GLAZE_OUTPUT_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            glaze_common::config::GlazeConfig::load_from(&glaze_common::paths::config_file())
                .ok()
                .map(|c| c.appearance.scale)
        })
        .filter(|v| v.is_finite())
        .unwrap_or(1.0)
        .clamp(1.0, 3.0);
    if (v - v.round()).abs() < 1e-6 {
        smithay::output::Scale::Integer(v.round() as i32)
    } else {
        smithay::output::Scale::Fractional(v)
    }
}

/// Bring every connected connector up, drop disconnected ones.
pub fn scan_connectors(state: &mut State) {
    let mut added: Vec<Output> = Vec::new();
    let mut removed: Vec<Output> = Vec::new();
    {
        let Some(data) = udev_data_mut(state) else {
            return;
        };
        let Some(gpu) = data.gpu.as_mut() else {
            return;
        };
        let Ok(res) = gpu.drm.resource_handles() else {
            warn!("DRM: cannot read resources");
            return;
        };
        // Drop surfaces whose connector went away.
        let mut gone = Vec::new();
        for (crtc, s) in &gpu.surfaces {
            let connected = gpu
                .drm
                .get_connector(s.connector, false)
                .map(|c| c.state() == connector::State::Connected)
                .unwrap_or(false);
            if !connected {
                gone.push(*crtc);
            }
        }
        for crtc in gone {
            if let Some(s) = gpu.surfaces.remove(&crtc) {
                info!(output = %s.output.name(), "connector disconnected");
                removed.push(s.output);
            }
        }
        for conn in res.connectors() {
            if gpu.surfaces.values().any(|s| s.connector == *conn) {
                continue;
            }
            let Ok(info) = gpu.drm.get_connector(*conn, true) else {
                continue;
            };
            if info.state() != connector::State::Connected {
                continue;
            }
            let Some(mode) = pick_mode(info.modes()) else {
                warn!(?conn, "connected connector without modes");
                continue;
            };
            // A free CRTC reachable through one of the connector's encoders.
            let used: Vec<crtc::Handle> = gpu.surfaces.keys().copied().collect();
            let crtc = info.encoders().iter().find_map(|e| {
                let enc = gpu.drm.get_encoder(*e).ok()?;
                res.filter_crtcs(enc.possible_crtcs())
                    .into_iter()
                    .find(|c| !used.contains(c))
            });
            let Some(crtc) = crtc else {
                warn!(?conn, "no free CRTC for connector");
                continue;
            };
            let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
            let surface = match gpu.drm.create_surface(crtc, mode, &[*conn]) {
                Ok(s) => s,
                Err(e) => {
                    warn!(output = %name, %e, "create_surface failed");
                    continue;
                }
            };
            let (mm_w, mm_h) = info.size().unwrap_or((0, 0));
            let output = Output::new(
                name.clone(),
                PhysicalProperties {
                    size: (mm_w as i32, mm_h as i32).into(),
                    subpixel: Subpixel::Unknown,
                    make: "Glaze".into(),
                    model: name.clone(),
                },
            );
            let out_mode = Mode::from(mode);
            output.set_preferred(out_mode);
            output.change_current_state(
                Some(out_mode),
                Some(Transform::Normal),
                Some(output_scale()),
                None,
            );
            let allocator = GbmAllocator::new(
                gpu.gbm.clone(),
                GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
            );
            let exporter = GbmFramebufferExporter::new(gpu.gbm.clone(), Some(gpu.node));
            let render_formats = gpu.renderer.egl_context().dmabuf_render_formats().clone();
            let compositor = match DrmCompositor::new(
                &output,
                surface,
                None,
                allocator,
                exporter,
                COLOR_FORMATS,
                render_formats,
                gpu.drm.cursor_size(),
                None, // no cursor plane: the pointer is composited (virtio-gpu cursor planes are flaky)
            ) {
                Ok(c) => c,
                Err(e) => {
                    warn!(output = %name, ?e, "DrmCompositor init failed");
                    continue;
                }
            };
            let hz = mode.vrefresh().max(1);
            info!(
                output = %name,
                width = mode.size().0,
                height = mode.size().1,
                refresh_hz = hz,
                "output connected"
            );
            gpu.surfaces.insert(
                crtc,
                OutputSurface {
                    offscreen: None,
                    output: output.clone(),
                    connector: *conn,
                    compositor,
                    pending: false,
                    dirty: true,
                    idle_timer: false,
                    refresh: Duration::from_secs_f64(1.0 / f64::from(hz)),
                    last_vblank: None,
                },
            );
            added.push(output);
        }
    }
    for output in removed {
        remove_output(state, &output);
    }
    for output in added {
        let existing: Vec<(i32, i32)> = state
            .outputs
            .iter()
            .filter_map(|o| state.space.output_geometry(o))
            .map(|g| (g.loc.x, g.size.w))
            .collect();
        let x = crate::wm::next_output_x(&existing);
        output.change_current_state(None, None, None, Some((x, 0).into()));
        let _global = output.create_global::<State>(&state.display_handle);
        state.space.map_output(&output, (x, 0));
        state.outputs.push(output.clone());
        if state.output.is_none() {
            state.output = Some(output);
        }
    }
    state.update_work_area();
    state.needs_redraw = true;
}

fn pause(state: &mut State) {
    info!("session paused (VT switched away): input and DRM suspended");
    let Some(data) = udev_data_mut(state) else {
        return;
    };
    data.libinput.suspend();
    if let Some(gpu) = data.gpu.as_mut() {
        gpu.drm.pause();
        for s in gpu.surfaces.values_mut() {
            s.pending = false;
        }
    }
}

fn resume(state: &mut State) {
    info!("session resumed (VT switched back): reactivating input and DRM");
    let Some(data) = udev_data_mut(state) else {
        return;
    };
    if data.libinput.resume().is_err() {
        warn!("libinput resume failed");
    }
    if let Some(gpu) = data.gpu.as_mut() {
        if let Err(e) = gpu.drm.activate(false) {
            warn!(%e, "DRM activate failed");
        }
        for s in gpu.surfaces.values_mut() {
            if let Err(e) = s.compositor.reset_state() {
                warn!(?e, "DrmCompositor reset_state failed");
            }
            s.pending = false;
            s.dirty = true;
        }
    }
    // Connectors may have changed while we were away.
    scan_connectors(state);
    state.needs_redraw = true;
}

fn on_vblank(state: &mut State, crtc: crtc::Handle) {
    let now = Instant::now();
    let Some(data) = udev_data_mut(state) else {
        return;
    };
    let Some(gpu) = data.gpu.as_mut() else {
        return;
    };
    let Some(s) = gpu.surfaces.get_mut(&crtc) else {
        return;
    };
    if let Err(e) = s.compositor.frame_submitted() {
        warn!(?e, "frame_submitted");
    }
    s.pending = false;
    if let Some(last) = s.last_vblank {
        let gap = now - last;
        // Only count back-to-back frames (an idle gap is not a frame time).
        if gap < s.refresh * 3 {
            data.vblank_stats.push(gap.as_secs_f64() * 1000.0);
        }
    }
    s.last_vblank = Some(now);
    render_pending(state);
}

/// Called after every event-loop dispatch: render each output that needs a
/// frame and is not waiting for a flip.
pub fn render_pending(state: &mut State) {
    state.retire_animations();
    let animating = state.animating();
    let mark_all = std::mem::take(&mut state.needs_redraw) || animating;
    let active = udev_data(state).is_some_and(|d| d.session.is_active());
    let crtcs: Vec<crtc::Handle> = {
        let Some(data) = udev_data_mut(state) else {
            return;
        };
        let Some(gpu) = data.gpu.as_mut() else {
            return;
        };
        if mark_all {
            for s in gpu.surfaces.values_mut() {
                s.dirty = true;
            }
        }
        if !active {
            return;
        }
        gpu.surfaces
            .iter()
            .filter(|(_, s)| s.dirty && !s.pending)
            .map(|(c, _)| *c)
            .collect()
    };
    for crtc in crtcs {
        render_surface(state, crtc);
    }
    log_stats(state);
}

fn render_surface(state: &mut State, crtc: crtc::Handle) {
    let Some(BackendData::Udev(mut data)) = state.backend.take() else {
        return;
    };
    let mut flipped = false;
    let mut output = None;
    let mut refresh = Duration::from_millis(16);
    let t0 = Instant::now();
    if let Some(gpu) = data.gpu.as_mut() {
        if let Some(s) = gpu.surfaces.get_mut(&crtc) {
            s.dirty = false;
            output = Some(s.output.clone());
            refresh = s.refresh;
            let clear = if state.locked {
                state.palette.title_bg_inactive.premultiplied()
            } else {
                state.palette.desktop.premultiplied()
            };
            let elements = crate::render::output_elements(&mut gpu.renderer, state, &s.output);
            let t_elems = t0.elapsed();
            let n_elems = elements.len();
            // Optionally compose in an offscreen texture, then copy only the
            // damage into the scan-out buffer as one opaque texture.
            let composed = if direct_render() {
                None
            } else {
                compose_offscreen(&mut gpu.renderer, s, &elements, clear)
            };
            let to_scanout: &[crate::render::GlazeElement] = match &composed {
                Some(element) => std::slice::from_ref(element),
                None => &elements,
            };
            let r = s.compositor.render_frame(
                &mut gpu.renderer,
                to_scanout,
                clear,
                FrameFlags::empty(),
            );
            let t_render = t0.elapsed();
            if t_render > Duration::from_millis(50) {
                info!(
                    elements = n_elems,
                    build_ms = t_elems.as_secs_f64() * 1000.0,
                    render_ms = (t_render - t_elems).as_secs_f64() * 1000.0,
                    "slow frame"
                );
            }
            match r {
                Ok(result) => {
                    if debug_damage() && !result.is_empty {
                        log_damage(&result, n_elems);
                    }
                    if result.is_empty {
                        state.empty_renders += 1;
                    } else {
                        match s.compositor.queue_frame(()) {
                            Ok(()) => {
                                s.pending = true;
                                flipped = true;
                            }
                            Err(e) => warn!(?e, "queue_frame failed"),
                        }
                    }
                }
                Err(e) => warn!(?e, "render_frame failed"),
            }
        }
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    state.backend = Some(BackendData::Udev(data));
    let Some(output) = output else {
        return;
    };
    if flipped {
        state.frame_ema_ms = state.frame_ema_ms * 0.85 + ms * 0.15;
        state.frame_stats.push(ms);
        state.frames_total += 1;
        crate::render::send_frames(state, &output, state.now());
    } else {
        arm_idle_frame_timer(state, crtc, refresh);
    }
    state.after_render(&output);
}

/// `GLAZE_DEBUG_DAMAGE=1`: log every frame's primary-plane damage (perf
/// work: tells a full-frame redraw from a small one).
fn debug_damage() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("GLAZE_DEBUG_DAMAGE").is_ok_and(|v| v == "1"))
}

fn log_damage<B, F, E>(
    result: &smithay::backend::drm::compositor::RenderFrameResult<'_, B, F, E>,
    elements: usize,
) where
    B: smithay::backend::allocator::Buffer,
    F: smithay::backend::drm::Framebuffer,
{
    use smithay::backend::drm::compositor::PrimaryPlaneElement;
    let PrimaryPlaneElement::Swapchain(sc) = &result.primary_element else {
        info!(elements, "damage: primary plane scanned out directly");
        return;
    };
    let rects: Vec<_> = sc
        .damage
        .raw()
        .next()
        .map(|r| r.copied().collect())
        .unwrap_or_default();
    let area: i64 = rects
        .iter()
        .map(|r| i64::from(r.size.w) * i64::from(r.size.h))
        .sum();
    info!(elements, rects = rects.len(), area, first = ?rects.first(), "damage");
}

/// Draw straight into the scan-out buffer (default). `GLAZE_OFFSCREEN=1`
/// composes in an offscreen texture first and copies the damage: measured
/// on the VM (1280x800, llvmpipe, Thunar drag) direct was faster
/// (avg 5.1 ms / p95 9.3 ms against 7.7 / 13.6), so offscreen is opt-in for
/// hardware where scan-out memory is slow to read.
fn direct_render() -> bool {
    static DIRECT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DIRECT.get_or_init(|| !std::env::var("GLAZE_OFFSCREEN").is_ok_and(|v| v == "1"))
}

/// Render `elements` into the output's offscreen texture with its own damage
/// tracker (single buffer: only this frame's damage is redrawn) and return
/// one opaque element carrying that damage, or `None` when the offscreen
/// path is unavailable (then the caller renders directly).
fn compose_offscreen(
    renderer: &mut GlesRenderer,
    s: &mut OutputSurface,
    elements: &[crate::render::GlazeElement],
    clear: [f32; 4],
) -> Option<crate::render::GlazeElement> {
    let mode = s.output.current_mode()?;
    let size = (mode.size.w, mode.size.h);
    if s.offscreen.as_ref().map(|o| o.size) != Some(size) {
        let texture: GlesTexture = match Offscreen::<GlesTexture>::create_buffer(
            renderer,
            Fourcc::Abgr8888,
            (size.0, size.1).into(),
        ) {
            Ok(t) => t,
            Err(e) => {
                warn!(?e, "offscreen texture failed; rendering directly");
                return None;
            }
        };
        let full = Rectangle::from_size((size.0, size.1).into());
        let buffer = TextureRenderBuffer::from_texture(
            renderer,
            texture.clone(),
            1,
            Transform::Normal,
            Some(vec![full]),
        );
        let tracker = OutputDamageTracker::new((size.0, size.1), 1.0, Transform::Normal);
        info!(
            width = size.0,
            height = size.1,
            "offscreen composition target"
        );
        s.offscreen = Some(OffscreenTarget {
            texture,
            buffer,
            tracker,
            size,
            age: 0,
        });
    }
    let off = s.offscreen.as_mut()?;
    let damage: Vec<Rectangle<i32, smithay::utils::Physical>> = {
        let mut target = match renderer.bind(&mut off.texture) {
            Ok(t) => t,
            Err(e) => {
                warn!(?e, "binding the offscreen texture failed");
                return None;
            }
        };
        match off
            .tracker
            .render_output(renderer, &mut target, off.age, elements, clear)
        {
            Ok(r) => r.damage.cloned().unwrap_or_default(),
            Err(e) => {
                warn!(?e, "offscreen render failed");
                off.age = 0;
                return None;
            }
        }
    };
    off.age = 1;
    if !damage.is_empty() {
        let mut ctx = off.buffer.render();
        let _ = ctx.draw(|_| {
            Ok::<_, std::convert::Infallible>(
                damage
                    .iter()
                    .map(|r| Rectangle::new((r.loc.x, r.loc.y).into(), (r.size.w, r.size.h).into()))
                    .collect(),
            )
        });
    }
    Some(crate::render::GlazeElement::Texture(
        TextureRenderElement::from_texture_render_buffer(
            (0.0, 0.0),
            &off.buffer,
            None,
            None,
            None,
            smithay::backend::renderer::element::Kind::Unspecified,
        ),
    ))
}

/// No flip happened: answer frame callbacks one refresh later, not now, so a
/// client that commits nothing new on every callback does not spin.
fn arm_idle_frame_timer(state: &mut State, crtc: crtc::Handle, after: Duration) {
    {
        let Some(s) = udev_data_mut(state)
            .and_then(|d| d.gpu.as_mut())
            .and_then(|g| g.surfaces.get_mut(&crtc))
        else {
            return;
        };
        if s.idle_timer {
            return;
        }
        s.idle_timer = true;
    }
    let _ = state
        .loop_handle
        .insert_source(Timer::from_duration(after), move |_, _, state| {
            let output = udev_data_mut(state)
                .and_then(|d| d.gpu.as_mut())
                .and_then(|g| g.surfaces.get_mut(&crtc))
                .map(|s| {
                    s.idle_timer = false;
                    s.output.clone()
                });
            if let Some(output) = output {
                crate::render::send_frames(state, &output, state.now());
                let _ = state.display_handle.flush_clients();
            }
            TimeoutAction::Drop
        });
}

fn log_stats(state: &mut State) {
    if state.last_stats_log.elapsed() < STATS_EVERY {
        return;
    }
    let secs = state.last_stats_log.elapsed().as_secs_f64();
    let n = state.frame_stats.len();
    if let Some((min, avg, p95, max)) = state.frame_stats.take_summary() {
        let vb = udev_data_mut(state).and_then(|d| d.vblank_stats.take_summary());
        info!(
            frames = n,
            fps = format!("{:.1}", n as f64 / secs),
            min_ms = format!("{min:.2}"),
            avg_ms = format!("{avg:.2}"),
            p95_ms = format!("{p95:.2}"),
            max_ms = format!("{max:.2}"),
            vblank_avg_ms = vb.map(|v| format!("{:.2}", v.1)).unwrap_or_default(),
            vblank_p95_ms = vb.map(|v| format!("{:.2}", v.2)).unwrap_or_default(),
            total = state.frames_total,
            empty = state.empty_renders,
            wakeups = state.loop_wakeups,
            low_power = state.motion.low_power,
            "frame-time (render+queue, CPU wall clock; vblank = flip-to-flip)"
        );
    }
    state.empty_renders = 0;
    state.loop_wakeups = 0;
    state.last_stats_log = Instant::now();
}

/// Switch to another VT (Ctrl+Alt+Fn).
pub fn change_vt(data: &mut UdevData, vt: i32) {
    info!(vt, "switching VT");
    if let Err(e) = data.session.change_vt(vt) {
        warn!(vt, %e, "VT switch failed");
    }
}
