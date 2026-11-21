//! The semi-transparent 6x6 grid window and its mouse/keyboard selection logic.
//!
//! State lives in a `Cell<State>` (a `Copy` struct): it is read, changed and written back
//! without calling Win32 in between, so window messages sent synchronously by Win32 calls
//! can never observe or clobber a half-updated state, and nothing can panic on re-entry.

use std::cell::Cell;
use std::ffi::c_void;

use windows::core::{w, Result};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetColorizationColor, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, FillRect, InvalidateRect, SelectObject, HDC, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, SetFocus, VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_LEFT,
    VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorPos,
    GetForegroundWindow,
    GetWindowThreadProcessId, IsWindow, LoadCursorW, RegisterClassW, SetForegroundWindow,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, HWND_TOPMOST, IDC_ARROW, LWA_ALPHA,
    SWP_SHOWWINDOW, SW_HIDE, WINDOW_EX_STYLE, WM_ACTIVATE, WM_CAPTURECHANGED, WM_CLOSE,
    WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_PAINT, WM_RBUTTONUP, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::placement::{self, cell_at, grid_line, COLS, ROWS};
use crate::util::{beep, rect_str};

// Look and feel. Sizes are in pixels at 100% scaling and are multiplied by the monitor scale.
const OPACITY: u8 = 200; // about 78%
const WIDTH_FRACTION: f64 = 0.40;
const MARGIN: f64 = 10.0;
const GAP: f64 = 2.0;
const OUTLINE: f64 = 2.0;
const BACKGROUND: COLORREF = rgb(0x18, 0x18, 0x18);
const TILE: COLORREF = rgb(0x3a, 0x3a, 0x3a);
const ACCENT_FALLBACK: COLORREF = rgb(0x00, 0x78, 0xd4);

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Pos {
    c: i32,
    r: i32,
}

#[derive(Clone, Copy)]
struct State {
    hwnd: HWND,
    visible: bool,
    target: HWND,
    work: RECT,
    scale: f64,
    accent: COLORREF,
    /// Highlighted cell; it is also corner B once corner A is set.
    cur: Pos,
    /// Corner A.
    anchor: Option<Pos>,
    /// Left button pressed while setting corner A (possible drag).
    dragging: bool,
    drag_start: Pos,
    /// Left button pressed to set corner B; applied on release.
    second_click: bool,
    /// Cursor position when the grid opened. Windows sends a synthetic mouse move when a
    /// window appears under a still pointer; it must not override the keyboard start cell.
    open_cursor: POINT,
    mouse_armed: bool,
}

const ORIGIN: Pos = Pos { c: 0, r: 0 };

thread_local! {
    static STATE: Cell<State> = const {
        Cell::new(State {
            hwnd: HWND(std::ptr::null_mut()),
            visible: false,
            target: HWND(std::ptr::null_mut()),
            work: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            scale: 1.0,
            accent: ACCENT_FALLBACK,
            cur: ORIGIN,
            anchor: None,
            dragging: false,
            drag_start: ORIGIN,
            second_click: false,
            open_cursor: POINT { x: 0, y: 0 },
            mouse_armed: false,
        })
    };
}

fn get() -> State {
    STATE.with(|s| s.get())
}

fn set(state: State) {
    STATE.with(|s| s.set(state));
}

/// Grid geometry inside the overlay's client area.
struct Layout {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    gap: i32,
}

impl Layout {
    fn new(client_w: i32, client_h: i32, scale: f64) -> Self {
        let m = (MARGIN * scale).round() as i32;
        Layout {
            x: m,
            y: m,
            w: (client_w - 2 * m).max(COLS),
            h: (client_h - 2 * m).max(ROWS),
            gap: ((GAP * scale).round() as i32).max(1),
        }
    }

    /// Cell rectangle shrunk by the gap, so the background shows through as thin lines.
    fn tile(&self, c: i32, r: i32) -> RECT {
        let a = self.gap / 2;
        let b = self.gap - a;
        RECT {
            left: grid_line(self.x, self.w, c, COLS) + a,
            top: grid_line(self.y, self.h, r, ROWS) + a,
            right: grid_line(self.x, self.w, c + 1, COLS) - b,
            bottom: grid_line(self.y, self.h, r + 1, ROWS) - b,
        }
    }

    fn hit(&self, px: i32, py: i32, clamp: bool) -> Option<Pos> {
        let inside = px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h;
        if !inside && !clamp {
            return None;
        }
        let c = ((px - self.x) as i64 * COLS as i64).div_euclid(self.w as i64) as i32;
        let r = ((py - self.y) as i64 * ROWS as i64).div_euclid(self.h as i64) as i32;
        Some(Pos {
            c: c.clamp(0, COLS - 1),
            r: r.clamp(0, ROWS - 1),
        })
    }
}

fn layout(hwnd: HWND, scale: f64) -> Layout {
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    Layout::new(rc.right - rc.left, rc.bottom - rc.top, scale)
}

fn accent_color() -> COLORREF {
    let mut argb = 0u32;
    let mut opaque = Default::default();
    match unsafe { DwmGetColorizationColor(&mut argb, &mut opaque) } {
        Ok(()) => {
            let (r, g, b) = ((argb >> 16) as u8, (argb >> 8) as u8, argb as u8);
            rgb(r, g, b)
        }
        Err(_) => ACCENT_FALLBACK,
    }
}

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub fn create(instance: HINSTANCE) -> Result<()> {
    unsafe {
        let class = w!("Tessera.Overlay");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_LAYERED.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0),
            class,
            w!("Tessera"),
            WS_POPUP,
            0,
            0,
            100,
            100,
            None,
            None,
            Some(instance),
            None,
        )?;
        SetLayeredWindowAttributes(hwnd, COLORREF(0), OPACITY, LWA_ALPHA)?;
        let corners: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corners as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        let mut s = get();
        s.hwnd = hwnd;
        set(s);
    }
    Ok(())
}

/// Hotkey handler: opens the grid for the foreground window, or closes it if already open.
pub fn toggle(main_hwnd: HWND) {
    if get().visible {
        close("hotkey pressed again", true);
    } else {
        open(main_hwnd);
    }
}

fn open(main_hwnd: HWND) {
    let me = get();
    let foreground = unsafe { GetForegroundWindow() };
    let target = match placement::inspect(foreground, &[me.hwnd, main_hwnd]) {
        Ok(t) => t,
        Err(reason) => {
            tlog!("grid not opened: {reason}");
            beep();
            return;
        }
    };

    let work = target.work;
    let ww = (work.right - work.left).max(1);
    let wh = (work.bottom - work.top).max(1);
    let scale = target.dpi as f64 / 96.0;
    let margin = (MARGIN * scale).round() as i32;
    let grid_w = (ww as f64 * WIDTH_FRACTION).round() as i32;
    let grid_h = (grid_w as f64 * wh as f64 / ww as f64).round() as i32;
    let (w, h) = (grid_w + 2 * margin, grid_h + 2 * margin);
    let x = work.left + (ww - w) / 2;
    let y = work.top + (wh - h) / 2;
    let (c, r) = cell_at(&work, target.frame.left, target.frame.top);

    let mut s = me;
    s.visible = true;
    s.target = target.hwnd;
    s.work = work;
    s.scale = scale;
    s.accent = accent_color();
    s.cur = Pos { c, r };
    s.anchor = None;
    s.dragging = false;
    s.second_click = false;
    s.mouse_armed = false;
    unsafe {
        let _ = GetCursorPos(&mut s.open_cursor);
    }
    set(s);

    let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    tlog!("grid opened at {} (start cell {c},{r})", rect_str(&rect));
    unsafe {
        let _ = SetWindowPos(me.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
    }
    invalidate(me.hwnd);
    take_focus(me.hwnd);
}

/// The hotkey gives Tessera the right to take the foreground; the thread-input
/// attachment is only a fallback for the rare case where that is not enough.
fn take_focus(hwnd: HWND) {
    unsafe {
        if !SetForegroundWindow(hwnd).as_bool() {
            let fg = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg, None);
            let me = GetCurrentThreadId();
            if fg_thread != 0 && fg_thread != me {
                let _ = AttachThreadInput(me, fg_thread, true);
                let _ = SetForegroundWindow(hwnd);
                let _ = BringWindowToTop(hwnd);
                let _ = AttachThreadInput(me, fg_thread, false);
            }
        }
        let _ = SetFocus(Some(hwnd));
        if GetForegroundWindow() != hwnd {
            tlog!("warning: the grid could not take the keyboard focus");
        }
    }
}

fn close(reason: &str, refocus_target: bool) {
    let mut s = get();
    if !s.visible {
        return;
    }
    s.visible = false;
    s.anchor = None;
    s.dragging = false;
    s.second_click = false;
    set(s);
    tlog!("grid closed: {reason}");
    unsafe {
        if GetCapture() == s.hwnd {
            let _ = ReleaseCapture();
        }
        let _ = ShowWindow(s.hwnd, SW_HIDE);
        if refocus_target && IsWindow(Some(s.target)).as_bool() {
            let _ = SetForegroundWindow(s.target);
        }
    }
}

fn apply(a: Pos, b: Pos) {
    let s = get();
    if !s.visible {
        return;
    }
    let (c0, c1) = (a.c.min(b.c), a.c.max(b.c));
    let (r0, r1) = (a.r.min(b.r), a.r.max(b.r));
    close("selection applied", false);
    placement::apply(s.target, s.work, c0, r0, c1, r1);
    unsafe {
        if IsWindow(Some(s.target)).as_bool() {
            let _ = SetForegroundWindow(s.target);
        }
    }
}

fn on_escape() {
    let mut s = get();
    if s.anchor.is_some() {
        s.anchor = None;
        s.dragging = false;
        s.second_click = false;
        set(s);
        unsafe {
            if GetCapture() == s.hwnd {
                let _ = ReleaseCapture();
            }
        }
        invalidate(s.hwnd);
    } else {
        close("cancelled", true);
    }
}

fn on_key(vk: VIRTUAL_KEY, repeat: bool) {
    let mut s = get();
    if !s.visible {
        return;
    }
    let step = |p: Pos, dc: i32, dr: i32| Pos {
        c: (p.c + dc).clamp(0, COLS - 1),
        r: (p.r + dr).clamp(0, ROWS - 1),
    };
    match vk {
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN => {
            s.cur = match vk {
                VK_LEFT => step(s.cur, -1, 0),
                VK_RIGHT => step(s.cur, 1, 0),
                VK_UP => step(s.cur, 0, -1),
                _ => step(s.cur, 0, 1),
            };
            set(s);
            invalidate(s.hwnd);
        }
        VK_SPACE if !repeat => match s.anchor {
            Some(a) => apply(a, s.cur),
            None => {
                s.anchor = Some(s.cur);
                set(s);
                invalidate(s.hwnd);
            }
        },
        VK_RETURN if !repeat => apply(s.anchor.unwrap_or(s.cur), s.cur),
        VK_ESCAPE => on_escape(),
        _ => {}
    }
}

fn on_mouse_move(px: i32, py: i32) {
    let mut s = get();
    if !s.visible {
        return;
    }
    if !s.mouse_armed {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        if pt == s.open_cursor {
            return;
        }
        s.mouse_armed = true;
        set(s);
    }
    let captured = s.dragging || s.second_click;
    if let Some(p) = layout(s.hwnd, s.scale).hit(px, py, captured) {
        if p != s.cur {
            s.cur = p;
            set(s);
            invalidate(s.hwnd);
        }
    }
}

fn on_left_down(px: i32, py: i32) {
    let mut s = get();
    if !s.visible {
        return;
    }
    let Some(p) = layout(s.hwnd, s.scale).hit(px, py, false) else {
        return; // click on the margin: ignored
    };
    s.cur = p;
    if s.anchor.is_none() {
        s.anchor = Some(p);
        s.dragging = true;
        s.drag_start = p;
    } else {
        s.second_click = true;
    }
    set(s);
    unsafe {
        SetCapture(s.hwnd);
    }
    invalidate(s.hwnd);
}

fn on_left_up(px: i32, py: i32) {
    let mut s = get();
    if !s.visible || !(s.dragging || s.second_click) {
        return;
    }
    let p = layout(s.hwnd, s.scale).hit(px, py, true).unwrap_or(s.cur);
    s.cur = p;
    let action = if s.dragging {
        s.dragging = false;
        // Released on another cell: it was a drag, apply now. Same cell: corner A is set.
        if p != s.drag_start {
            s.anchor.map(|a| (a, p))
        } else {
            None
        }
    } else {
        s.second_click = false;
        s.anchor.map(|a| (a, p))
    };
    set(s);
    unsafe {
        let _ = ReleaseCapture();
    }
    match action {
        Some((a, b)) => apply(a, b),
        None => invalidate(s.hwnd),
    }
}

fn fill(hdc: HDC, rect: &RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(hdc, rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

fn paint(hwnd: HWND) {
    let s = get();
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);

        // Double buffer: draw off-screen, then copy in one go.
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, bmp.into());

        fill(mem, &rc, BACKGROUND);
        let lay = Layout::new(w, h, s.scale);
        for r in 0..ROWS {
            for c in 0..COLS {
                fill(mem, &lay.tile(c, r), TILE);
            }
        }

        match s.anchor {
            Some(a) => {
                // Selected rectangle drawn as one merged block.
                let (c0, c1) = (a.c.min(s.cur.c), a.c.max(s.cur.c));
                let (r0, r1) = (a.r.min(s.cur.r), a.r.max(s.cur.r));
                let first = lay.tile(c0, r0);
                let last = lay.tile(c1, r1);
                let block = RECT {
                    left: first.left,
                    top: first.top,
                    right: last.right,
                    bottom: last.bottom,
                };
                fill(mem, &block, s.accent);
            }
            None => {
                // Highlighted cell: accent outline.
                let t = lay.tile(s.cur.c, s.cur.r);
                let o = ((OUTLINE * s.scale).round() as i32).max(1);
                fill(mem, &t, s.accent);
                let inner = RECT {
                    left: t.left + o,
                    top: t.top + o,
                    right: t.right - o,
                    bottom: t.bottom - o,
                };
                fill(mem, &inner, TILE);
            }
        }

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

fn mouse_xy(lp: LPARAM) -> (i32, i32) {
    let x = (lp.0 & 0xffff) as u16 as i16 as i32;
    let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32;
    (x, y)
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => paint(hwnd),
        WM_ERASEBKGND => return LRESULT(1),
        WM_KEYDOWN => on_key(VIRTUAL_KEY(wp.0 as u16), (lp.0 >> 30) & 1 == 1),
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lp);
            on_mouse_move(x, y);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lp);
            on_left_down(x, y);
        }
        WM_LBUTTONUP => {
            let (x, y) = mouse_xy(lp);
            on_left_up(x, y);
        }
        // Acting on release keeps the button-up from reaching the window underneath.
        WM_RBUTTONUP => on_escape(),
        WM_CAPTURECHANGED => {
            let mut s = get();
            if s.dragging || s.second_click {
                s.dragging = false;
                s.second_click = false;
                set(s);
            }
        }
        WM_ACTIVATE => {
            if wp.0 & 0xffff == 0 {
                close("lost focus", false);
            }
        }
        // Alt+F4 must hide the grid, never destroy the reusable window.
        WM_CLOSE => close("closed", true),
        // Size and position are always set explicitly; ignore DPI-change suggestions.
        WM_DPICHANGED => {}
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}
