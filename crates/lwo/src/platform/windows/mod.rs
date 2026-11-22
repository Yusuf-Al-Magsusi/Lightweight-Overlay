//! Windows platform: one click-through layered window per widget (a "panel"),
//! plus a hidden control window that owns the hotkeys, timer, tray icon and
//! the settings dialog.
//!
//! Lazy by design: while the overlay is hidden the timer is stopped and every
//! source is off, and while it is visible each widget is updated only when its
//! own refresh interval is due and redrawn only when it reports a change.
//! Between events the thread blocks in `GetMessageW`.

mod canvas;
mod gpu;
mod media;
mod monitors;
mod panel;
mod sensors;
mod settings;
mod sources;
mod tray;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use lwo_core::config::Config;
use lwo_core::hotkey::{Hotkey, Key};
use lwo_core::layout;
use lwo_core::widget::{Context, LocalTime, Source, SourceInfo, Stats, Style};
use lwo_core::APP_NAME;
use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use self::canvas::Gfx;
use self::media::WM_MEDIA_CHANGED;
use self::panel::Panel;
use self::sources::{probe_sources, Sources};
use super::App;

const TIMER_ID: usize = 1;
const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_EDIT: i32 = 2;
const WIDGET_GAP: f32 = 6.0;
/// A widget counts as due if its time comes within this much of the tick.
const DUE_TOLERANCE: Duration = Duration::from_millis(120);

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Read by the overlay window procedure: in edit mode, windows are draggable.
static EDITING: AtomicBool = AtomicBool::new(false);

/// Which panels `update_panels` should update.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Those whose refresh interval has elapsed.
    Due,
    /// Those that show the media session (it just changed).
    Media,
    /// All of them, and repaint regardless.
    Force,
}

struct State {
    control: HWND,
    hinst: HINSTANCE,
    overlay_class: HSTRING,
    gfx: Gfx,
    config: Config,
    config_path: Option<PathBuf>,
    /// What this machine's sources can provide; probed once at startup.
    info: SourceInfo,
    dpi_scale: f32,
    style: Style,
    opacity: u8,
    panels: Vec<Panel>,
    sources: Sources,
    /// Latest polled readings, refreshed on each timer tick.
    stats: Stats,
    visible: bool,
    editing: bool,
    /// Edit mode was started while the overlay was hidden: hide it again afterwards.
    hide_after_edit: bool,
    toggle_label: String,
    edit_label: String,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Run `f` on the state. Returns `None` if it is not set up yet or already
/// borrowed (a message arriving while we are inside a call, which we ignore).
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

fn local_time() -> (LocalTime, u16) {
    // SAFETY: GetLocalTime has no preconditions.
    let st = unsafe { GetLocalTime() };
    (
        LocalTime {
            hour: st.wHour as u8,
            minute: st.wMinute as u8,
            second: st.wSecond as u8,
        },
        st.wMilliseconds,
    )
}

impl State {
    fn context(&self, now: Instant) -> Context {
        let (time, _) = local_time();
        Context {
            time,
            now,
            media: self.sources.media_snapshot(),
            stats: self.stats.clone(),
            temperature_unit: self.config.units.temperature,
        }
    }

    /// Update the selected panels, repaint the ones that changed, and show or
    /// hide each window according to its widget (edit mode shows them all).
    fn update_panels(&mut self, pass: Pass) {
        let now = Instant::now();
        let ctx = self.context(now);
        let (style, opacity, editing) = (&self.style, self.opacity, self.editing);
        for p in &mut self.panels {
            let run = match pass {
                Pass::Force => true,
                Pass::Media => p.uses_media(),
                Pass::Due => now + DUE_TOLERANCE >= p.next_due,
            };
            if !run {
                continue;
            }
            p.next_due = now + p.interval();
            let changed = p.widget.update(&ctx);
            let want = editing || p.widget.visible();
            // Paint before showing so a window never appears blank.
            if want && (changed || pass == Pass::Force || !p.is_shown()) {
                let _ = p.paint(style, opacity, editing);
            }
            p.set_shown(want);
        }
    }

    /// Milliseconds until the next update is due, lined up with the second
    /// boundary when a widget updates every second so a clock never shows a
    /// stale second.
    fn next_delay_ms(&self) -> u32 {
        let now = Instant::now();
        let soonest = self
            .panels
            .iter()
            .map(|p| p.next_due.saturating_duration_since(now))
            .min()
            .unwrap_or(Duration::from_secs(1));
        let soonest_ms = soonest.as_millis().min(u128::from(u32::MAX)) as u32;
        let every_second = self
            .panels
            .iter()
            .any(|p| p.interval() <= Duration::from_secs(1));
        if every_second {
            let (_, ms) = local_time();
            let to_boundary = (1000 - u32::from(ms) + 5).clamp(20, 1005);
            to_boundary.min(soonest_ms.max(20))
        } else {
            soonest_ms.max(50)
        }
    }

    fn schedule(&self) {
        // SAFETY: valid control window; replaces any existing timer with the same id.
        unsafe {
            SetTimer(Some(self.control), TIMER_ID, self.next_delay_ms(), None);
        }
    }

    fn show(&mut self) {
        if self.panels.is_empty() {
            return;
        }
        self.sources.start(self.control);
        self.stats = self.sources.poll();
        self.visible = true;
        self.update_panels(Pass::Force);
        for p in self.panels.iter().filter(|p| p.is_shown()) {
            p.raise();
        }
        self.schedule();
    }

    fn hide(&mut self) {
        if self.editing {
            self.leave_edit_mode();
        }
        self.hide_after_edit = false;
        // SAFETY: valid control window; stopping the timer means a hidden overlay does no work.
        unsafe {
            let _ = KillTimer(Some(self.control), TIMER_ID);
        }
        for p in &mut self.panels {
            p.set_shown(false);
        }
        self.sources.stop();
        self.stats = Stats::default();
        self.visible = false;
    }

    fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    fn tick(&mut self) {
        if self.visible {
            self.stats = self.sources.poll();
            self.update_panels(Pass::Due);
            self.schedule();
        }
    }

    /// The media session changed: update the media widgets now and re-plan the
    /// timer (play and pause change how often they want updates).
    fn on_media_changed(&mut self) {
        if self.visible {
            self.update_panels(Pass::Media);
            self.schedule();
        }
    }

    // ---------------------------------------------------------- edit mode

    fn enter_edit_mode(&mut self) {
        if self.editing || self.panels.is_empty() {
            return;
        }
        if !self.visible {
            self.hide_after_edit = true;
            self.show();
        }
        self.editing = true;
        EDITING.store(true, Ordering::Relaxed);
        for p in &self.panels {
            p.set_click_through(false);
        }
        self.update_panels(Pass::Force);
    }

    /// Stop editing, make the windows click-through again and save where they are.
    fn leave_edit_mode(&mut self) -> Option<String> {
        if !self.editing {
            return None;
        }
        self.editing = false;
        EDITING.store(false, Ordering::Relaxed);
        for p in &self.panels {
            p.set_click_through(true);
        }
        let saved = self.save_layout();
        self.update_panels(Pass::Force);
        saved.err()
    }

    fn toggle_edit(&mut self) {
        if self.editing {
            let problem = self.leave_edit_mode();
            if self.hide_after_edit {
                self.hide();
            }
            if let Some(msg) = problem {
                tray::warn(self.control, &msg);
            }
        } else {
            self.enter_edit_mode();
        }
    }

    /// Record every panel's current position in the config and save it.
    fn save_layout(&mut self) -> std::result::Result<(), String> {
        for p in &self.panels {
            let (index, monitor) = monitors::of_window(p.hwnd);
            let placement = layout::placement_from_rect(index, monitor, p.rect(), self.dpi_scale);
            self.config.layout.insert(p.id.to_string(), placement);
        }
        match &self.config_path {
            Some(path) => self
                .config
                .save(path)
                .map_err(|e| format!("Could not save the layout to {}: {e}", path.display())),
            None => Ok(()),
        }
    }

    // ------------------------------------------------- config and rebuild

    /// Rebuild the widgets, windows, style and sources from `self.config`.
    /// The overlay must be hidden (and not editing) when this runs.
    fn rebuild(&mut self) -> Result<()> {
        self.panels.clear(); // destroys the old windows
        self.dpi_scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
        self.style = self.config.style(self.dpi_scale);
        self.opacity = (self.config.overlay.opacity * 255.0).round() as u8;

        // Drop widgets whose source can't run here (quietly: no error, no empty
        // panel) and tell the rest what exists.
        let mut widgets = crate::widgets::build(&self.config);
        let gpu_ok = self.info.gpu.is_some();
        widgets.retain(|w| w.sources().iter().all(|&s| s != Source::Gpu || gpu_ok));
        for w in &mut widgets {
            w.configure(&self.info);
        }
        self.sources = Sources::for_widgets(&widgets);

        let sizes: Vec<(i32, i32)> = widgets
            .iter()
            .map(|w| {
                let (width, height) = w.size(&self.gfx.measurer, &self.style);
                (width.ceil() as i32, height.ceil() as i32)
            })
            .collect();
        let gap = (WIDGET_GAP * self.style.scale).round() as i32;
        let (offsets, (group_w, group_h)) = layout::stack(&sizes, gap);

        let monitors = monitors::list();
        let primary = monitors[0];
        let o = &self.config.overlay;
        let origin = layout::anchored(
            o.anchor,
            (o.offset_x as f32 * self.dpi_scale).round() as i32,
            (o.offset_y as f32 * self.dpi_scale).round() as i32,
            group_w,
            group_h,
            primary,
        );

        let mut panels = Vec::with_capacity(widgets.len());
        for ((widget, size), offset) in widgets.into_iter().zip(sizes).zip(offsets) {
            let id = widget.id();
            let pos = match self.config.layout.get(id) {
                Some(placement) => {
                    let monitor = monitors
                        .get(placement.monitor as usize)
                        .copied()
                        .unwrap_or(primary);
                    layout::position_from_placement(
                        placement,
                        monitor,
                        size.0,
                        size.1,
                        self.dpi_scale,
                    )
                }
                None => (origin.0 + offset.0, origin.1 + offset.1),
            };
            panels.push(Panel::create(
                widget,
                size,
                pos,
                self.config.refresh_for(id),
                &self.gfx,
                self.hinst,
                &self.overlay_class,
            )?);
        }
        self.panels = panels;
        Ok(())
    }

    /// Register both hotkeys and set the tray labels. Returns anything the user should hear about.
    fn register_hotkeys(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut register = |id: i32, text: &str, name: &str, label: &str| -> String {
            // SAFETY: valid window handle owned by this thread; a missing registration is fine.
            unsafe {
                let _ = UnregisterHotKey(Some(self.control), id);
            }
            match Hotkey::parse(text) {
                Ok(hk) => match register_hotkey(self.control, id, hk) {
                    Ok(()) => format!("{label} ({hk})"),
                    Err(e) => {
                        warnings.push(format!(
                            "The {name} hotkey {hk} is unavailable ({e}); another program may be using it. Use the tray icon instead."
                        ));
                        label.to_string()
                    }
                },
                Err(e) => {
                    warnings.push(format!(
                        "Invalid {name} hotkey: {e}. Use the tray icon instead."
                    ));
                    label.to_string()
                }
            }
        };
        self.toggle_label = register(
            HOTKEY_TOGGLE,
            &self.config.hotkeys.toggle.clone(),
            "show/hide",
            "Show / hide overlay",
        );
        self.edit_label = register(
            HOTKEY_EDIT,
            &self.config.hotkeys.edit.clone(),
            "edit-mode",
            "Edit layout",
        );
        warnings
    }

    /// Change the settings: validate, save, rebuild and carry on, all without a restart.
    // Temporary: the settings dialog (next commit) is the only caller.
    #[allow(dead_code)]
    /// `edit` receives a copy of the current config, so settings it doesn't
    /// touch (including saved positions) are kept. On failure nothing changes.
    fn update_config(
        &mut self,
        edit: impl FnOnce(&mut Config),
    ) -> std::result::Result<Vec<String>, String> {
        let mut config = self.config.clone();
        edit(&mut config);
        config.sanitize();
        let problems = config.problems();
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }

        let was_visible = self.visible;
        if self.editing {
            self.leave_edit_mode();
            config.layout = self.config.layout.clone(); // may have just been saved
        }
        if was_visible {
            self.hide();
        }
        self.config = config;

        let mut warnings = Vec::new();
        if let Some(path) = &self.config_path {
            if let Err(e) = self.config.save(path) {
                warnings.push(format!(
                    "Could not save settings to {}: {e}",
                    path.display()
                ));
            }
        }
        if let Err(e) = self.rebuild() {
            return Err(format!("Could not rebuild the overlay: {e}"));
        }
        warnings.extend(self.register_hotkeys());
        if was_visible {
            self.show();
        }
        Ok(warnings)
    }
}

fn vk_of(key: Key) -> u32 {
    match key {
        Key::Char(c) => c as u32, // VK codes for A-Z and 0-9 equal their ASCII values
        Key::F(n) => 0x70 + (n as u32 - 1),
        Key::Space => 0x20,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::End => 0x23,
        Key::Home => 0x24,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
    }
}

fn register_hotkey(hwnd: HWND, id: i32, hk: Hotkey) -> Result<()> {
    let mut mods = MOD_NOREPEAT;
    if hk.ctrl {
        mods |= MOD_CONTROL;
    }
    if hk.alt {
        mods |= MOD_ALT;
    }
    if hk.shift {
        mods |= MOD_SHIFT;
    }
    if hk.win {
        mods |= MOD_WIN;
    }
    // SAFETY: valid window handle owned by this thread.
    unsafe { RegisterHotKey(Some(hwnd), id, mods, vk_of(hk.key)) }
}

unsafe extern "system" fn control_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let taskbar_created = TASKBAR_CREATED.load(Ordering::Relaxed);
    match msg {
        WM_TIMER if wp.0 == TIMER_ID => {
            with_state(|s| s.tick());
            LRESULT(0)
        }
        WM_HOTKEY => {
            match wp.0 as i32 {
                HOTKEY_TOGGLE => {
                    with_state(|s| s.toggle());
                }
                HOTKEY_EDIT => {
                    with_state(|s| s.toggle_edit());
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_MEDIA_CHANGED => {
            with_state(|s| s.on_media_changed());
            LRESULT(0)
        }
        tray::WM_TRAY => {
            match (lp.0 & 0xFFFF) as u32 {
                WM_LBUTTONUP => {
                    with_state(|s| s.toggle());
                }
                WM_RBUTTONUP => {
                    let text = with_state(|s| tray::MenuText {
                        toggle: s.toggle_label.clone(),
                        edit: s.edit_label.clone(),
                        editing: s.editing,
                    });
                    if let Some(text) = text {
                        tray::show_menu(hwnd, &text);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                tray::ID_TOGGLE => {
                    with_state(|s| s.toggle());
                }
                tray::ID_EDIT => {
                    with_state(|s| s.toggle_edit());
                }
                tray::ID_SETTINGS => settings::open(hwnd),
                tray::ID_QUIT => {
                    // Save any layout being edited before the windows go away.
                    with_state(|s| s.leave_edit_mode());
                    // SAFETY: destroying our own window.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            tray::remove(hwnd);
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        m if m == taskbar_created && m != 0 => {
            // Explorer restarted; its tray is empty again.
            let _ = tray::add(hwnd);
            LRESULT(0)
        }
        // SAFETY: default handling for every other message.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Window procedure for the widget windows. They are click-through, except in
/// edit mode, where every pixel answers "title bar" so Windows moves the window
/// when it is dragged.
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST && EDITING.load(Ordering::Relaxed) {
        return LRESULT(HTCAPTION as isize);
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub fn run(app: App) -> std::result::Result<(), String> {
    run_inner(app).map_err(|e| e.to_string())
}

fn run_inner(app: App) -> Result<()> {
    let App {
        mut config,
        config_path,
        mut warnings,
    } = app;
    config.sanitize();

    // SAFETY: process-wide setup, done once before any window exists.
    unsafe {
        // Without this Windows bitmap-scales the overlay on high-DPI screens.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // One instance at a time.
        let name = HSTRING::from(format!("Local\\{APP_NAME}-single-instance"));
        // The handle is intentionally never closed: it must live until the process exits.
        let _mutex = CreateMutexW(None, false, &name)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
    }

    // SAFETY: window class registration and creation on this thread.
    let (control, hinst, overlay_class) = unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let overlay_class = HSTRING::from(format!("{APP_NAME}-overlay"));
        let control_class = HSTRING::from(format!("{APP_NAME}-control"));
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: hinst,
            lpszClassName: windows::core::PCWSTR(overlay_class.as_ptr()),
            ..Default::default()
        });
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(control_proc),
            hInstance: hinst,
            lpszClassName: windows::core::PCWSTR(control_class.as_ptr()),
            ..Default::default()
        });
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
        // Never shown; exists to receive the hotkey, timer, tray and settings messages.
        let control = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            &control_class,
            &HSTRING::from(APP_NAME),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;
        (control, hinst, overlay_class)
    };

    let start_visible = config.overlay.start_visible;
    let mut state = State {
        control,
        hinst,
        overlay_class,
        gfx: Gfx::new()?,
        config,
        config_path,
        // Probed whether or not the GPU widget is on now, so it can be switched
        // on later in the settings without a restart.
        info: probe_sources(),
        dpi_scale: 1.0,
        style: Style {
            font: String::new(),
            scale: 1.0,
            text: lwo_core::widget::Color::new(1.0, 1.0, 1.0, 1.0),
            background: lwo_core::widget::Color::new(0.0, 0.0, 0.0, 0.7),
            accent: lwo_core::widget::Color::new(0.3, 0.8, 0.5, 1.0),
            temperature_unit: Default::default(),
        },
        opacity: 255,
        panels: Vec::new(),
        sources: Sources::for_widgets(&[]),
        stats: Stats::default(),
        visible: false,
        editing: false,
        hide_after_edit: false,
        toggle_label: String::new(),
        edit_label: String::new(),
    };
    state.rebuild()?;
    warnings.extend(state.register_hotkeys());
    STATE.with(|cell| *cell.borrow_mut() = Some(state));

    tray::add(control)?;
    if !warnings.is_empty() {
        tray::warn(control, &warnings.join("\n"));
    }
    if start_visible {
        with_state(|s| s.show());
    }

    // Blocks here; no CPU use until the timer, a hotkey, the tray or a source posts a message.
    let mut msg = MSG::default();
    // SAFETY: standard message loop on the thread that created the windows.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if settings::is_dialog_message(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
