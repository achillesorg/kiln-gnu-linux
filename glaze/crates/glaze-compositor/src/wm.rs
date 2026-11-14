//! The compositor's window-management brain, portable and unit-tested.
//!
//! [`WindowManager`] is the single source of truth for *policy* state: which
//! windows exist, their stacking order, focus, workspace, minimised/maximised
//! flags, the remembered stacking geometry, and the stacking <-> tiling mode
//! ([`glaze_wm::LayoutPolicy`]). The Smithay side (`crate::state`) only mirrors
//! it onto real surfaces: after every mutation it asks [`WindowManager::targets`]
//! where each visible window goes and maps/unmaps/configures accordingly.
//!
//! All rectangles here are **outer frame rectangles** in logical output
//! coordinates (title bar and border included). The Smithay side converts to
//! and from client geometry with [`crate::frame::Insets`].

use glaze_common::model::{WindowInfo, WindowKind, WorkspaceInfo};
use glaze_wm::geometry::{snap_zone, SnapZone};
use glaze_wm::keybindings::{Key, Keymap, Mods, WmAction};
use glaze_wm::{
    arrange, AltTab, ArrangeInput, LayoutMode, LayoutPolicy, LayoutScope, ModeRequest, Rect,
    WorkspaceModel,
};

/// Number of workspaces the compositor starts with.
pub const DEFAULT_WORKSPACES: u32 = 4;
/// Distance between successive cascaded windows.
pub const CASCADE_STEP: i32 = 28;
/// Cascade wraps after this many steps.
pub const CASCADE_WRAP: i32 = 8;
/// Gap used when snapping to a zone.
pub const SNAP_GAP: i32 = 0;

/// One managed toplevel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedWindow {
    pub id: String,
    pub app_id: Option<String>,
    pub title: String,
    pub workspace: u32,
    /// Remembered stacking (floating) outer geometry.
    pub stacking: Rect,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    /// Keep floating on tiled workspaces (dialogs, or dragged out of a tile).
    pub pinned_floating: bool,
    /// Geometry before the window was snapped (halves, quarters, centre);
    /// dragging it off the snap or Super+Down brings it back.
    pub restore: Option<Rect>,
}

impl ManagedWindow {
    fn to_info(&self, focused: bool, floating: bool) -> WindowInfo {
        WindowInfo {
            id: self.id.clone(),
            app_id: self.app_id.clone(),
            title: self.title.clone(),
            workspace: Some(self.workspace),
            kind: WindowKind::Normal,
            focused,
            minimized: self.minimized,
            floating,
        }
    }
}

/// What an action did, so the backend knows what follow-up it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Policy state changed: re-sync surfaces with [`WindowManager::targets`].
    Relayout,
    /// Ask this window's client to close (the client decides; it may refuse).
    Close(String),
    /// Open the launcher (a shell action).
    Launcher,
    /// Nothing happened.
    Ignored(&'static str),
}

/// See the module docs.
#[derive(Debug, Clone)]
pub struct WindowManager {
    windows: Vec<ManagedWindow>,
    focused: Option<String>,
    workspaces: WorkspaceModel,
    pub layout: LayoutPolicy,
    alt_tab: AltTab,
    cycling: bool,
    /// Minimised windows, most recent last (Super+Shift+M restores the last).
    minimized_order: Vec<String>,
    output: Rect,
    work_area: Rect,
    cascade: i32,
    next_serial: u64,
    keymap: Keymap,
}

impl WindowManager {
    /// A manager for one output.
    pub fn new(output: Rect) -> Self {
        Self {
            windows: Vec::new(),
            focused: None,
            workspaces: WorkspaceModel::new(DEFAULT_WORKSPACES),
            layout: LayoutPolicy::default(),
            alt_tab: AltTab::new(),
            cycling: false,
            minimized_order: Vec::new(),
            output,
            work_area: output,
            cascade: 0,
            next_serial: 1,
            keymap: Keymap::default_glaze(),
        }
    }

    // --- geometry ---------------------------------------------------------

    /// Output size or layer-shell exclusive zones changed.
    pub fn set_output(&mut self, output: Rect, work_area: Rect) {
        self.output = output;
        self.work_area = work_area;
        for w in &mut self.windows {
            w.stacking = clamp_visible(w.stacking, work_area);
        }
    }

    pub fn output(&self) -> Rect {
        self.output
    }

    pub fn work_area(&self) -> Rect {
        self.work_area
    }

    // --- lookup -----------------------------------------------------------

    pub fn get(&self, id: &str) -> Option<&ManagedWindow> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut ManagedWindow> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    /// All windows, bottom of the stack first.
    pub fn windows(&self) -> &[ManagedWindow] {
        &self.windows
    }

    pub fn focused(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    pub fn active_workspace(&self) -> u32 {
        self.workspaces.active()
    }

    pub fn workspace_count(&self) -> u32 {
        self.workspaces.count()
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// The effective layout mode of the active workspace.
    pub fn active_mode(&self) -> LayoutMode {
        self.layout.mode_for(self.active_workspace())
    }

    /// True when the window is laid out by the tiler right now.
    pub fn is_tiled(&self, id: &str) -> bool {
        self.get(id).is_some_and(|w| {
            !w.pinned_floating
                && !w.maximized
                && !w.fullscreen
                && self.layout.mode_for(w.workspace) == LayoutMode::Tiling
        })
    }

    // --- lifecycle --------------------------------------------------------

    /// Add a newly mapped window whose client picked `size` (outer, logical).
    /// It is placed (cascade, centred in the work area), raised and focused.
    pub fn add_window(
        &mut self,
        app_id: Option<String>,
        title: impl Into<String>,
        size: (i32, i32),
        pinned_floating: bool,
    ) -> String {
        let id = format!("glaze-toplevel-{}", self.next_serial);
        self.next_serial += 1;
        let stacking = self.next_placement(size);
        self.windows.push(ManagedWindow {
            id: id.clone(),
            app_id,
            title: title.into(),
            workspace: self.active_workspace(),
            stacking,
            minimized: false,
            maximized: false,
            fullscreen: false,
            pinned_floating,
            restore: None,
        });
        self.focus(&id);
        id
    }

    fn next_placement(&mut self, size: (i32, i32)) -> Rect {
        let area = self.work_area;
        let w = size.0.clamp(1, area.w.max(1));
        let h = size.1.clamp(1, area.h.max(1));
        let base_x = area.x + (area.w - w) / 2 - CASCADE_STEP * (CASCADE_WRAP / 2);
        let base_y = area.y + (area.h - h) / 2 - CASCADE_STEP * (CASCADE_WRAP / 2);
        let offset = self.cascade * CASCADE_STEP;
        self.cascade = (self.cascade + 1) % CASCADE_WRAP;
        clamp_visible(Rect::new(base_x + offset, base_y + offset, w, h), area)
    }

    /// Forget a window. Focus moves to the topmost visible window.
    pub fn remove_window(&mut self, id: &str) -> bool {
        let before = self.windows.len();
        self.windows.retain(|w| w.id != id);
        if self.windows.len() == before {
            return false;
        }
        self.alt_tab.remove(id);
        self.minimized_order.retain(|m| m != id);
        if self.focused.as_deref() == Some(id) {
            self.focused = None;
            self.focus_topmost();
        }
        true
    }

    pub fn set_title(&mut self, id: &str, title: impl Into<String>) {
        if let Some(w) = self.get_mut(id) {
            w.title = title.into();
        }
    }

    pub fn set_app_id(&mut self, id: &str, app_id: Option<String>) {
        if let Some(w) = self.get_mut(id) {
            w.app_id = app_id;
        }
    }

    // --- focus and stacking -----------------------------------------------

    /// Focus (activate) a window: restores it if minimised, switches to its
    /// workspace and raises it.
    pub fn focus(&mut self, id: &str) -> bool {
        let Some(ws) = self.get(id).map(|w| w.workspace) else {
            return false;
        };
        if let Some(w) = self.get_mut(id) {
            w.minimized = false;
        }
        self.minimized_order.retain(|m| m != id);
        if ws != self.active_workspace() {
            self.workspaces.switch_to(ws);
        }
        self.raise(id);
        self.focused = Some(id.to_owned());
        if !self.cycling {
            self.alt_tab.on_focus(id);
        }
        true
    }

    /// Move a window to the top of the stack without focusing it.
    pub fn raise(&mut self, id: &str) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let w = self.windows.remove(pos);
            self.windows.push(w);
        }
    }

    /// Drop keyboard focus (e.g. a layer-shell surface took it).
    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    fn focus_topmost(&mut self) {
        let ws = self.active_workspace();
        let next = self
            .windows
            .iter()
            .rev()
            .find(|w| w.workspace == ws && !w.minimized)
            .map(|w| w.id.clone());
        self.focused = None;
        if let Some(next) = next {
            self.focus(&next);
        }
    }

    // --- minimise / maximise ----------------------------------------------

    pub fn minimize(&mut self, id: &str) -> bool {
        let Some(w) = self.get_mut(id) else {
            return false;
        };
        if w.minimized {
            return false;
        }
        w.minimized = true;
        self.minimized_order.retain(|m| m != id);
        self.minimized_order.push(id.to_owned());
        if self.focused.as_deref() == Some(id) {
            self.focused = None;
            self.focus_topmost();
        }
        true
    }

    /// Restore a minimised window and focus it.
    pub fn restore(&mut self, id: &str) -> bool {
        self.focus(id)
    }

    pub fn set_maximized(&mut self, id: &str, maximized: bool) -> bool {
        match self.get_mut(id) {
            Some(w) if w.maximized != maximized => {
                w.maximized = maximized;
                true
            }
            _ => false,
        }
    }

    pub fn set_fullscreen(&mut self, id: &str, fullscreen: bool) -> bool {
        match self.get_mut(id) {
            Some(w) if w.fullscreen != fullscreen => {
                w.fullscreen = fullscreen;
                true
            }
            _ => false,
        }
    }

    /// The user moved/resized a window interactively. On a tiled workspace this
    /// pulls it out of the tiling (it becomes pinned floating).
    pub fn set_stacking_geometry(&mut self, id: &str, rect: Rect) {
        let tiled = self.is_tiled(id);
        if let Some(w) = self.get_mut(id) {
            w.stacking = rect;
            w.maximized = false;
            w.restore = None;
            if tiled {
                w.pinned_floating = true;
            }
        }
    }

    /// The most recently minimised window that still is.
    pub fn last_minimized(&self) -> Option<&str> {
        self.minimized_order
            .iter()
            .rev()
            .find(|id| self.get(id).is_some_and(|w| w.minimized))
            .map(String::as_str)
    }

    /// Bring a snapped window back to its pre-snap geometry.
    pub fn unsnap(&mut self, id: &str) -> bool {
        match self.get_mut(id) {
            Some(w) if w.restore.is_some() => {
                w.stacking = w.restore.take().expect("checked");
                true
            }
            _ => false,
        }
    }

    /// Super+Down: un-maximise, else un-snap, else minimise (glaze-snap's
    /// `restore-or-minimise`).
    pub fn restore_or_minimize(&mut self, id: &str) -> bool {
        if self.get(id).is_some_and(|w| w.maximized) {
            return self.set_maximized(id, false);
        }
        if self.unsnap(id) {
            return true;
        }
        self.minimize(id)
    }

    /// Centre the window in the work area, keeping its size.
    pub fn center(&mut self, id: &str) -> bool {
        let area = self.work_area;
        let Some(w) = self.get(id) else {
            return false;
        };
        let r = w.stacking;
        let restore = w.restore.or(Some(r));
        let rect = Rect::new(area.x + (area.w - r.w) / 2, area.y + (area.h - r.h) / 2, r.w, r.h);
        self.set_stacking_geometry(id, clamp_visible(rect, area));
        if let Some(w) = self.get_mut(id) {
            w.restore = restore;
        }
        true
    }

    /// Super+Shift+Space: float a tiled window, or put it back in the tiling.
    pub fn toggle_floating(&mut self, id: &str) -> bool {
        let tiled_ws = self
            .get(id)
            .is_some_and(|w| self.layout.mode_for(w.workspace) == LayoutMode::Tiling);
        match self.get_mut(id) {
            Some(w) if tiled_ws => {
                w.pinned_floating = !w.pinned_floating;
                w.maximized = false;
                true
            }
            _ => false,
        }
    }

    /// Alt+Tab list in MRU order with the previewed entry, for the switcher.
    pub fn cycle_list(&self) -> (Vec<String>, Option<String>) {
        (
            self.alt_tab.order().to_vec(),
            self.alt_tab.candidate().map(str::to_owned),
        )
    }

    /// The client resized itself while stacking: remember its size.
    /// Sizes larger than the work area are clamped (the next configure asks
    /// the client to fit, e.g. LibreOffice opening wider than the screen).
    pub fn client_resized(&mut self, id: &str, size: (i32, i32)) {
        let tiled = self.is_tiled(id);
        let area = self.work_area;
        if let Some(w) = self.get_mut(id) {
            if !tiled && !w.maximized && !w.fullscreen {
                let oversized = size.0 > area.w || size.1 > area.h;
                w.stacking.w = size.0.clamp(1, area.w.max(1));
                w.stacking.h = size.1.clamp(1, area.h.max(1));
                if oversized {
                    // Pull it fully inside so the frame and buttons are reachable.
                    let r = &mut w.stacking;
                    r.x = r.x.min(area.right() - r.w).max(area.x);
                    r.y = r.y.min(area.bottom() - r.h).max(area.y);
                }
            }
        }
    }

    /// Snap a window to a zone of the work area (stacking geometry).
    pub fn snap(&mut self, id: &str, zone: SnapZone) -> bool {
        let rect = snap_zone(zone, self.work_area, SNAP_GAP);
        if zone == SnapZone::Maximize {
            let maximized = self.get(id).is_some_and(|w| w.maximized);
            return self.set_maximized(id, !maximized);
        }
        let Some(w) = self.get(id) else {
            return false;
        };
        if zone == SnapZone::Center {
            return self.center(id);
        }
        // Remember the free-floating geometry once (snapping between zones
        // keeps the original).
        let restore = w.restore.or(Some(w.stacking));
        self.set_stacking_geometry(id, rect);
        if let Some(w) = self.get_mut(id) {
            w.restore = restore;
        }
        true
    }

    // --- workspaces ---------------------------------------------------------

    pub fn switch_workspace(&mut self, index: u32) -> bool {
        if index == self.active_workspace() || !self.workspaces.switch_to(index) {
            return false;
        }
        self.focused = None;
        self.focus_topmost();
        true
    }

    pub fn move_to_workspace(&mut self, id: &str, index: u32) -> bool {
        if !self.workspaces.is_valid(index) {
            return false;
        }
        match self.get_mut(id) {
            Some(w) if w.workspace != index => {
                w.workspace = index;
            }
            _ => return false,
        }
        if self.focused.as_deref() == Some(id) {
            self.focused = None;
            self.focus_topmost();
        }
        true
    }

    // --- layout -------------------------------------------------------------

    /// Apply a stacking/tiling request. Returns the workspaces whose effective
    /// mode changed. Pinned-floating flags are reset on workspaces that switch
    /// to tiling, so "tile everything" really tiles everything.
    pub fn set_layout(&mut self, scope: LayoutScope, request: ModeRequest) -> Vec<u32> {
        let known: Vec<u32> = (0..self.workspace_count()).collect();
        let changed = self.layout.apply(scope, request, &known);
        for w in &mut self.windows {
            if changed.contains(&w.workspace)
                && self.layout.mode_for(w.workspace) == LayoutMode::Tiling
            {
                w.pinned_floating = false;
                w.maximized = false;
            }
        }
        changed
    }

    /// Where every visible window of the active workspace goes, bottom of the
    /// stack first. Minimised windows and other workspaces are absent.
    pub fn targets(&self) -> Vec<(String, Rect)> {
        let ws = self.active_workspace();
        let visible: Vec<&ManagedWindow> = self
            .windows
            .iter()
            .filter(|w| w.workspace == ws && !w.minimized)
            .collect();
        // Tiling order is mapping order (stable), not stacking order, so that
        // raising a window does not reshuffle the tiles.
        let mut by_age: Vec<&ManagedWindow> = visible.clone();
        by_age.sort_by_key(|w| serial_of(&w.id));
        let inputs: Vec<ArrangeInput> = by_age
            .iter()
            .map(|w| ArrangeInput {
                id: w.id.clone(),
                stacking: w.stacking,
                pinned_floating: w.pinned_floating || w.maximized || w.fullscreen,
            })
            .collect();
        let arranged = arrange(
            self.layout.mode_for(ws),
            self.layout.algorithm,
            self.work_area,
            self.layout.gap,
            &inputs,
        );
        visible
            .iter()
            .map(|w| {
                let rect = if w.fullscreen {
                    self.output
                } else if w.maximized {
                    self.work_area
                } else {
                    arranged
                        .iter()
                        .find(|(id, _)| *id == w.id)
                        .map(|(_, r)| *r)
                        .unwrap_or(w.stacking)
                };
                (w.id.clone(), rect)
            })
            .collect()
    }

    // --- alt-tab ------------------------------------------------------------

    /// One Alt+Tab step: preview (raise + focus) the next MRU window.
    pub fn cycle(&mut self, forward: bool) -> Option<String> {
        if self.alt_tab.len() < 2 {
            return None;
        }
        self.cycling = true;
        let id = if forward {
            self.alt_tab.advance()
        } else {
            self.alt_tab.retreat()
        }?
        .to_owned();
        self.focus(&id);
        Some(id)
    }

    /// Alt released: commit the previewed window as most recent.
    pub fn end_cycle(&mut self) {
        if self.cycling {
            self.cycling = false;
            self.alt_tab.commit();
        }
    }

    pub fn is_cycling(&self) -> bool {
        self.cycling
    }

    // --- actions ------------------------------------------------------------

    /// Resolve a key press against the keymap.
    pub fn action_for(&self, mods: Mods, key: &Key) -> Option<WmAction> {
        self.keymap.action_for(mods, key).copied()
    }

    /// Apply a keybinding action.
    pub fn apply(&mut self, action: WmAction) -> Applied {
        let focused = self.focused.clone();
        let need_focus = |f: Option<String>| f.ok_or("no focused window");
        match action {
            WmAction::CloseWindow => match need_focus(focused) {
                Ok(id) => Applied::Close(id),
                Err(e) => Applied::Ignored(e),
            },
            WmAction::ToggleMaximize => match need_focus(focused) {
                Ok(id) => {
                    self.snap(&id, SnapZone::Maximize);
                    Applied::Relayout
                }
                Err(e) => Applied::Ignored(e),
            },
            WmAction::Snap(zone) => match need_focus(focused) {
                Ok(id) => {
                    self.snap(&id, zone);
                    Applied::Relayout
                }
                Err(e) => Applied::Ignored(e),
            },
            WmAction::Minimize => match need_focus(focused) {
                Ok(id) => {
                    self.minimize(&id);
                    Applied::Relayout
                }
                Err(e) => Applied::Ignored(e),
            },
            WmAction::ToggleTiling => {
                let ws = self.active_workspace();
                self.set_layout(LayoutScope::Workspace(ws), ModeRequest::Toggle);
                Applied::Relayout
            }
            WmAction::ToggleTilingGlobal => {
                self.set_layout(LayoutScope::Global, ModeRequest::Toggle);
                Applied::Relayout
            }
            WmAction::SwitchWorkspace(i) => {
                if self.switch_workspace(i) {
                    Applied::Relayout
                } else {
                    Applied::Ignored("already on that workspace")
                }
            }
            WmAction::NextWorkspace | WmAction::PrevWorkspace => {
                let n = self.workspace_count();
                let cur = self.active_workspace();
                let next = if action == WmAction::NextWorkspace {
                    (cur + 1) % n
                } else {
                    (cur + n - 1) % n
                };
                self.switch_workspace(next);
                Applied::Relayout
            }
            WmAction::MoveToWorkspace(i) => match need_focus(focused) {
                Ok(id) => {
                    self.move_to_workspace(&id, i);
                    Applied::Relayout
                }
                Err(e) => Applied::Ignored(e),
            },
            WmAction::MoveLeft | WmAction::MoveRight => match need_focus(focused) {
                Ok(id) => {
                    let n = self.workspace_count();
                    let cur = self.active_workspace();
                    let to = if action == WmAction::MoveRight {
                        (cur + 1) % n
                    } else {
                        (cur + n - 1) % n
                    };
                    self.move_to_workspace(&id, to);
                    Applied::Relayout
                }
                Err(e) => Applied::Ignored(e),
            },
            WmAction::CycleWindows | WmAction::CycleWindowsBack => {
                match self.cycle(action == WmAction::CycleWindows) {
                    Some(_) => Applied::Relayout,
                    None => Applied::Ignored("fewer than two windows"),
                }
            }
            WmAction::Launcher => Applied::Launcher,
        }
    }

    // --- shell models -------------------------------------------------------

    /// The shell's view of the windows (stacking order, bottom first).
    pub fn window_infos(&self) -> Vec<WindowInfo> {
        self.windows
            .iter()
            .map(|w| {
                let floating = !self.is_tiled(&w.id);
                w.to_info(self.focused.as_deref() == Some(w.id.as_str()), floating)
            })
            .collect()
    }

    /// The shell's view of the workspaces.
    pub fn workspace_infos(&self) -> Vec<WorkspaceInfo> {
        self.workspaces.to_infos()
    }
}

/// Where a newly connected output goes: right of every existing output
/// (`(x, width)` of each, logical). Outputs are laid out in a row, primary
/// (the first) at 0.
pub fn next_output_x(existing: &[(i32, i32)]) -> i32 {
    existing.iter().map(|(x, w)| x + w).max().unwrap_or(0)
}

fn serial_of(id: &str) -> u64 {
    id.rsplit('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX)
}

/// Keep at least a grabbable strip of the window inside `area`.
fn clamp_visible(r: Rect, area: Rect) -> Rect {
    const KEEP: i32 = 48;
    let w = r.w.min(area.w.max(1));
    let h = r.h.min(area.h.max(1));
    let x = r.x.clamp(area.x - w + KEEP, area.right() - KEEP);
    let y = r.y.clamp(area.y, area.bottom() - KEEP);
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wm() -> WindowManager {
        WindowManager::new(Rect::new(0, 0, 1280, 800))
    }

    fn rect_of(wm: &WindowManager, id: &str) -> Rect {
        wm.targets()
            .into_iter()
            .find(|(i, _)| i == id)
            .map(|(_, r)| r)
            .expect("visible")
    }

    #[test]
    fn new_windows_are_focused_raised_and_inside() {
        let mut m = wm();
        let a = m.add_window(Some("foot".into()), "a", (640, 400), false);
        let b = m.add_window(Some("foot".into()), "b", (640, 400), false);
        assert_eq!(m.focused(), Some(b.as_str()));
        assert_eq!(m.windows().last().unwrap().id, b);
        let ra = rect_of(&m, &a);
        let rb = rect_of(&m, &b);
        assert_ne!((ra.x, ra.y), (rb.x, rb.y), "cascade offsets new windows");
        assert!(m.work_area().contains(&ra));
    }

    #[test]
    fn focus_raises() {
        let mut m = wm();
        let a = m.add_window(None, "a", (300, 200), false);
        let _b = m.add_window(None, "b", (300, 200), false);
        m.focus(&a);
        assert_eq!(m.windows().last().unwrap().id, a);
    }

    #[test]
    fn minimize_hides_and_restore_shows() {
        let mut m = wm();
        let a = m.add_window(None, "a", (300, 200), false);
        let b = m.add_window(None, "b", (300, 200), false);
        assert!(m.minimize(&b));
        assert!(m.targets().iter().all(|(id, _)| id != &b));
        assert_eq!(m.focused(), Some(a.as_str()), "focus falls back");
        assert!(m.window_infos().iter().any(|w| w.id == b && w.minimized));
        assert!(m.restore(&b));
        assert!(m.targets().iter().any(|(id, _)| id == &b));
        assert_eq!(m.focused(), Some(b.as_str()));
    }

    #[test]
    fn tiling_toggle_changes_geometry_and_back_is_lossless() {
        let mut m = wm();
        let a = m.add_window(None, "a", (500, 300), false);
        let b = m.add_window(None, "b", (500, 300), false);
        let before = (rect_of(&m, &a), rect_of(&m, &b));
        assert_eq!(m.apply(WmAction::ToggleTiling), Applied::Relayout);
        assert_eq!(m.active_mode(), LayoutMode::Tiling);
        let (ta, tb) = (rect_of(&m, &a), rect_of(&m, &b));
        assert_ne!(ta, before.0);
        assert_eq!(ta.y, 0);
        assert_eq!(ta.h, 800);
        assert!(ta.right() <= tb.x, "side by side, oldest left");
        m.apply(WmAction::ToggleTiling);
        assert_eq!((rect_of(&m, &a), rect_of(&m, &b)), before);
    }

    #[test]
    fn workspace_toggle_is_per_workspace_and_global_overrides() {
        let mut m = wm();
        m.apply(WmAction::ToggleTiling);
        assert_eq!(m.layout.mode_for(0), LayoutMode::Tiling);
        assert_eq!(m.layout.mode_for(1), LayoutMode::Stacking);
        m.apply(WmAction::ToggleTilingGlobal);
        // Global was stacking -> tiling, overrides cleared.
        assert_eq!(m.layout.mode_for(0), LayoutMode::Tiling);
        assert_eq!(m.layout.mode_for(1), LayoutMode::Tiling);
    }

    #[test]
    fn dragging_a_tiled_window_floats_it() {
        let mut m = wm();
        let a = m.add_window(None, "a", (500, 300), false);
        let _b = m.add_window(None, "b", (500, 300), false);
        m.apply(WmAction::ToggleTiling);
        m.set_stacking_geometry(&a, Rect::new(100, 100, 400, 300));
        assert!(!m.is_tiled(&a));
        assert_eq!(rect_of(&m, &a), Rect::new(100, 100, 400, 300));
        // Re-toggling tiling on reclaims it.
        m.apply(WmAction::ToggleTiling);
        m.apply(WmAction::ToggleTiling);
        assert!(m.is_tiled(&a));
    }

    #[test]
    fn maximize_uses_work_area() {
        let mut m = wm();
        m.set_output(Rect::new(0, 0, 1280, 800), Rect::new(0, 0, 1280, 760));
        let a = m.add_window(None, "a", (500, 300), false);
        m.apply(WmAction::ToggleMaximize);
        assert_eq!(rect_of(&m, &a), Rect::new(0, 0, 1280, 760));
        m.apply(WmAction::ToggleMaximize);
        assert_eq!(rect_of(&m, &a).w, 500);
    }

    #[test]
    fn workspaces_hide_other_windows() {
        let mut m = wm();
        let a = m.add_window(None, "a", (300, 200), false);
        m.apply(WmAction::SwitchWorkspace(1));
        assert!(m.targets().is_empty());
        assert_eq!(m.focused(), None);
        let b = m.add_window(None, "b", (300, 200), false);
        assert_eq!(m.get(&b).unwrap().workspace, 1);
        // Activating a window on another workspace switches there.
        m.focus(&a);
        assert_eq!(m.active_workspace(), 0);
        m.apply(WmAction::MoveToWorkspace(2));
        assert!(m.targets().is_empty());
    }

    #[test]
    fn alt_tab_cycles_mru_and_commits() {
        let mut m = wm();
        let a = m.add_window(None, "a", (300, 200), false);
        let b = m.add_window(None, "b", (300, 200), false);
        let c = m.add_window(None, "c", (300, 200), false);
        assert_eq!(m.cycle(true), Some(b.clone()));
        assert_eq!(m.cycle(true), Some(a.clone()));
        m.end_cycle();
        assert_eq!(m.focused(), Some(a.as_str()));
        // MRU is now a, c, b: one Alt+Tab goes back to c.
        assert_eq!(m.cycle(true), Some(c.clone()));
        m.end_cycle();
    }

    #[test]
    fn close_only_requests() {
        let mut m = wm();
        let a = m.add_window(None, "a", (300, 200), false);
        assert_eq!(m.apply(WmAction::CloseWindow), Applied::Close(a.clone()));
        assert!(m.get(&a).is_some(), "the client decides");
        m.remove_window(&a);
        assert_eq!(
            m.apply(WmAction::CloseWindow),
            Applied::Ignored("no focused window")
        );
    }

    #[test]
    fn client_resize_updates_stacking_size_only_when_stacking() {
        let mut m = wm();
        let a = m.add_window(None, "a", (300, 200), false);
        m.client_resized(&a, (350, 250));
        assert_eq!(rect_of(&m, &a).w, 350);
        m.apply(WmAction::ToggleTiling);
        m.client_resized(&a, (10, 10));
        m.apply(WmAction::ToggleTiling);
        assert_eq!(rect_of(&m, &a).w, 350);
    }

    #[test]
    fn snap_remembers_and_restores_the_free_geometry() {
        let mut m = wm();
        let a = m.add_window(None, "a", (500, 300), false);
        let free = rect_of(&m, &a);
        m.apply(WmAction::Snap(SnapZone::LeftHalf));
        assert_eq!(rect_of(&m, &a), Rect::new(0, 0, 640, 800));
        m.apply(WmAction::Snap(SnapZone::TopRight));
        assert_eq!(rect_of(&m, &a), Rect::new(640, 0, 640, 400));
        assert!(m.restore_or_minimize(&a));
        assert_eq!(rect_of(&m, &a), free, "back to the pre-snap geometry");
        assert!(m.restore_or_minimize(&a));
        assert!(m.get(&a).unwrap().minimized, "then Super+Down minimises");
        assert_eq!(m.last_minimized(), Some(a.as_str()));
        m.focus(&a);
        assert_eq!(m.last_minimized(), None);
    }

    #[test]
    fn center_keeps_size_and_floating_toggle_only_on_tiled() {
        let mut m = wm();
        let a = m.add_window(None, "a", (400, 200), false);
        assert!(m.center(&a));
        assert_eq!(rect_of(&m, &a), Rect::new(440, 300, 400, 200));
        assert!(!m.toggle_floating(&a), "stacking: nothing to toggle");
        m.apply(WmAction::ToggleTiling);
        assert!(m.is_tiled(&a));
        assert!(m.toggle_floating(&a));
        assert!(!m.is_tiled(&a));
        let (order, _) = m.cycle_list();
        assert_eq!(order, vec![a.clone()]);
    }

    #[test]
    fn oversized_clients_are_fitted_to_the_work_area() {
        let mut m = wm();
        let a = m.add_window(None, "writer", (900, 600), false);
        m.client_resized(&a, (1500, 1000));
        let r = rect_of(&m, &a);
        assert!(r.w <= 1280 && r.h <= 800, "{r:?}");
        assert!(m.work_area().contains(&r));
    }

    #[test]
    fn outputs_are_laid_out_left_to_right() {
        assert_eq!(next_output_x(&[]), 0);
        assert_eq!(next_output_x(&[(0, 1280)]), 1280);
        // A hot-unplugged middle output leaves a gap; new outputs go to the end.
        assert_eq!(next_output_x(&[(0, 1280), (2560, 1920)]), 4480);
    }

    #[test]
    fn infos_report_tiling_as_not_floating() {
        let mut m = wm();
        let a = m.add_window(Some("foot".into()), "t", (300, 200), false);
        assert!(m.window_infos()[0].floating);
        m.apply(WmAction::ToggleTiling);
        let info = &m.window_infos()[0];
        assert_eq!(info.id, a);
        assert!(!info.floating);
        assert!(info.focused);
        assert_eq!(m.workspace_infos().len(), DEFAULT_WORKSPACES as usize);
    }
}
