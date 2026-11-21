//! The options panel opened by a right click on the tray icon: choose the shortcut that opens
//! the grid (with every key marked free or taken), the grid size, and toggle "Start with
//! Windows".
//!
//! Like the rest of Tessera it is plain Win32 on the single UI thread. State lives in
//! `thread_local` cells that are read and written without calling Win32 in between.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, DrawTextW, FillRect, GetMonitorInfoW, GetSysColor,
    GetSysColorBrush, InvalidateRect, MonitorFromPoint, SetBkMode, SetTextColor, COLOR_HIGHLIGHT,
    COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, DRAWITEMSTRUCT, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
    MEASUREITEMSTRUCT, ODS_SELECTED,
};
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, SystemParametersInfoForDpi,
    MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, IsDialogMessageW, LoadCursorW,
    MoveWindow, RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowPos, SetWindowTextW,
    ShowWindow, BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON,
    BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CBN_SELCHANGE, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CBS_OWNERDRAWFIXED,
    CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, HMENU, IDCANCEL, IDC_ARROW,
    IDOK, MSG, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE,
    WM_COMMAND, WM_CREATE, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM,
    WM_MEASUREITEM, WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_GROUP, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::combo::{
    self, Availability, Combo, Tone, ALT, CONTROL, SHIFT, WIN,
};
use crate::grid;
use crate::hotkey;

const ID_WIN: i32 = 201;
const ID_CTRL: i32 = 202;
const ID_ALT: i32 = 203;
const ID_SHIFT: i32 = 204;
const ID_KEY: i32 = 205;
const ID_STATUS: i32 = 206;
const ID_AUTOSTART: i32 = 207;
const ID_RESET: i32 = 208;
/// The grid size radio buttons use consecutive ids, in the order of `grid::PRESETS`.
const ID_GRID_FIRST: i32 = 220;
const DM_GETDEFID: u32 = 0x0400; // WM_USER
const SS_LEFT: u32 = 0; // left-aligned static text

// Layout in pixels at 100% scaling (96 dpi); everything is multiplied by the window's scale.
const CLIENT_W: i32 = 360;
const CLIENT_H: i32 = 294;
const ITEM_H: i32 = 20;

const RED: COLORREF = rgb(0xb4, 0x1e, 0x1e);
const AMBER: COLORREF = rgb(0x9a, 0x67, 0x00);
const GREEN: COLORREF = rgb(0x0f, 0x7b, 0x0f);

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

/// Window handles and the current look of the open panel.
#[derive(Clone, Copy)]
struct Panel {
    hwnd: HWND,
    main: HWND,
    label: HWND,
    key_label: HWND,
    win: HWND,
    ctrl: HWND,
    alt: HWND,
    shift: HWND,
    key: HWND,
    status: HWND,
    grid_label: HWND,
    grids: [HWND; 4],
    autostart: HWND,
    reset: HWND,
    save: HWND,
    cancel: HWND,
    font: HFONT,
    tone: Tone,
}

/// One entry of the key drop-down.
struct Item {
    /// Text without the terminating NUL.
    text: Vec<u16>,
    availability: Availability,
    windows_shortcut: bool,
}

/// What the panel was opened with.
#[derive(Clone, Copy)]
struct Start {
    active: Option<Combo>,
    autostart: bool,
    grid: i32,
}

thread_local! {
    static PANEL: Cell<Option<Panel>> = const { Cell::new(None) };
    static START: Cell<Start> = const {
        Cell::new(Start { active: None, autostart: false, grid: grid::DEFAULT })
    };
    static KEYS: RefCell<Vec<(String, u32)>> = const { RefCell::new(Vec::new()) };
    static ITEMS: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
    /// Modifiers the drop-down was last probed for (`u32::MAX`: not built yet).
    static LIST_MODS: Cell<u32> = const { Cell::new(u32::MAX) };
    static CLASS_READY: Cell<bool> = const { Cell::new(false) };
}

/// SendMessageW with plain integers; returns the result as an integer.
fn send(h: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { SendMessageW(h, msg, Some(WPARAM(wp)), Some(LPARAM(lp))) }.0
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn px(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi as i64 / 96) as i32
}

fn loword(v: usize) -> i32 {
    (v & 0xffff) as i32
}

fn hiword(v: usize) -> i32 {
    ((v >> 16) & 0xffff) as i32
}

/// The panel window, if it is open (so the grid never targets it).
pub fn hwnd() -> Option<HWND> {
    PANEL.with(|p| p.get()).map(|p| p.hwnd)
}

/// Lets the panel handle Tab, Enter and Esc. Returns true if the message was consumed.
pub fn handle_message(msg: &MSG) -> bool {
    match PANEL.with(|p| p.get()) {
        Some(p) => unsafe { IsDialogMessageW(p.hwnd, msg) }.as_bool(),
        None => false,
    }
}

/// Opens the panel near the cursor, or brings it to the front if it is already open.
pub fn show(main: HWND, active: Option<Combo>, autostart: bool, grid: i32) {
    if let Some(p) = PANEL.with(|p| p.get()) {
        unsafe {
            let _ = SetForegroundWindow(p.hwnd);
        }
        return;
    }
    START.with(|s| s.set(Start { active, autostart, grid }));
    LIST_MODS.with(|m| m.set(u32::MAX));
    KEYS.with(|k| *k.borrow_mut() = combo::key_list());

    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let instance: HINSTANCE = module.into();
        if !CLASS_READY.with(|c| c.get()) {
            let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_STANDARD_CLASSES,
            });
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance,
                lpszClassName: w!("Tessera.Options"),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: GetSysColorBrush(COLOR_WINDOW),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                tlog!("options: cannot register the window class");
                return;
            }
            CLASS_READY.with(|c| c.set(true));
        }

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let work = info.rcWork;

        let style = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);
        let ex_style = WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0);
        let mut frame = RECT { left: 0, top: 0, right: px(CLIENT_W, dpi_x), bottom: px(CLIENT_H, dpi_x) };
        let _ = AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, dpi_x);
        let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
        // Above the cursor (the tray usually sits at the bottom), kept inside the work area.
        let x = (cursor.x - width / 2).clamp(work.left, (work.right - width).max(work.left));
        let y = (cursor.y - height - px(12, dpi_x)).clamp(work.top, (work.bottom - height).max(work.top));

        // The main window is passed as the creation parameter so WM_CREATE can reach it.
        let created = CreateWindowExW(
            ex_style,
            w!("Tessera.Options"),
            w!("Tessera options"),
            style,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance),
            Some(main.0 as *const c_void),
        );
        match created {
            Ok(hwnd) => {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                tlog!("options panel opened");
            }
            Err(e) => tlog!("options: cannot create the window: {e}"),
        }
    }
}

fn make(parent: HWND, instance: HINSTANCE, class: PCWSTR, text: PCWSTR, style: u32, id: i32) -> HWND {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            text,
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as usize as *mut c_void)),
            Some(instance),
            None,
        )
        .unwrap_or_default()
    }
}

fn make_font(dpi: u32) -> HFONT {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    unsafe {
        let read = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some(&mut metrics as *mut NONCLIENTMETRICSW as *mut c_void),
            0,
            dpi,
        );
        if read.is_ok() {
            CreateFontIndirectW(&metrics.lfMessageFont)
        } else {
            HFONT::default()
        }
    }
}

/// Applies the font and places every control for the given scale.
fn layout(panel: &mut Panel, dpi: u32) {
    unsafe {
        if !panel.font.is_invalid() {
            let _ = DeleteObject(panel.font.into());
        }
        panel.font = make_font(dpi);
        let font = panel.font.0 as usize;

        let place = |h: HWND, x: i32, y: i32, w: i32, h_: i32| {
            let _ = MoveWindow(h, px(x, dpi), px(y, dpi), px(w, dpi), px(h_, dpi), true);
            send(h, WM_SETFONT, font, 1);
        };
        place(panel.label, 16, 12, 328, 18);
        place(panel.win, 16, 38, 58, 22);
        place(panel.ctrl, 78, 38, 58, 22);
        place(panel.alt, 140, 38, 52, 22);
        place(panel.shift, 196, 38, 64, 22);
        place(panel.key_label, 16, 72, 34, 20);
        // The height of a drop-down is its height when open.
        place(panel.key, 54, 68, 150, 24 + 9 * ITEM_H + 4);
        place(panel.status, 16, 104, 328, 50);
        place(panel.grid_label, 16, 164, 328, 18);
        for (i, radio) in panel.grids.iter().enumerate() {
            place(*radio, 16 + 72 * i as i32, 186, 68, 22);
        }
        place(panel.autostart, 16, 222, 328, 22);
        place(panel.reset, 16, 258, 96, 26);
        place(panel.save, 196, 258, 72, 26);
        place(panel.cancel, 272, 258, 72, 26);
    }
}

fn is_checked(h: HWND) -> bool {
    send(h, BM_GETCHECK, 0, 0) == 1
}

fn set_checked(h: HWND, on: bool) {
    send(h, BM_SETCHECK, on as usize, 0);
}

fn chosen_modifiers(p: &Panel) -> u32 {
    [(p.win, WIN), (p.ctrl, CONTROL), (p.alt, ALT), (p.shift, SHIFT)]
        .iter()
        .filter(|(h, _)| is_checked(*h))
        .fold(0, |acc, (_, flag)| acc | flag)
}

fn chosen_index(p: &Panel) -> Option<usize> {
    usize::try_from(send(p.key, CB_GETCURSEL, 0, 0)).ok()
}

fn chosen_combo(p: &Panel) -> Combo {
    let key = chosen_index(p)
        .and_then(|i| KEYS.with(|k| k.borrow().get(i).map(|(_, vk)| *vk)))
        .unwrap_or(0);
    Combo { modifiers: chosen_modifiers(p), key }
}

fn set_grid(p: &Panel, size: i32) {
    for (radio, preset) in p.grids.iter().zip(grid::PRESETS) {
        set_checked(*radio, preset == size);
    }
}

fn chosen_grid(p: &Panel) -> i32 {
    p.grids
        .iter()
        .zip(grid::PRESETS)
        .find(|(radio, _)| is_checked(**radio))
        .map_or(grid::DEFAULT, |(_, preset)| preset)
}

/// One radio button per grid size; the first starts the group.
fn make_grid_radios(parent: HWND, instance: HINSTANCE) -> [HWND; 4] {
    let mut radios = [HWND::default(); 4];
    for (i, (slot, size)) in radios.iter_mut().zip(grid::PRESETS).enumerate() {
        let text = wide_z(&grid::label(size));
        let first = if i == 0 { WS_GROUP.0 | WS_TABSTOP.0 } else { 0 };
        *slot = make(
            parent,
            instance,
            w!("BUTTON"),
            PCWSTR(text.as_ptr()),
            BS_AUTORADIOBUTTON as u32 | first,
            ID_GRID_FIRST + i as i32,
        );
    }
    radios
}

/// Probes every key for the chosen modifiers and rebuilds the drop-down with the result.
fn rebuild_list(p: &Panel, modifiers: u32) {
    let active = START.with(|s| s.get()).active;
    let keep = chosen_index(p).unwrap_or(0);
    let keys = KEYS.with(|k| k.borrow().clone());

    let mut items = Vec::with_capacity(keys.len());
    for (name, vk) in &keys {
        let combo = Combo { modifiers, key: *vk };
        // With no modifier there is nothing to register; the status line asks for one.
        let availability = if modifiers == 0 {
            Availability::Free
        } else {
            hotkey::probe(p.main, active, combo)
        };
        let text = format!("{name}{}", combo::list_note(combo, availability));
        items.push(Item {
            text: wide(&text),
            availability,
            windows_shortcut: combo::windows_use(combo).is_some(),
        });
    }

    send(p.key, CB_RESETCONTENT, 0, 0);
    for item in &items {
        let z: Vec<u16> = item.text.iter().copied().chain(Some(0)).collect();
        send(p.key, CB_ADDSTRING, 0, z.as_ptr() as isize);
    }
    send(p.key, CB_SETCURSEL, keep.min(keys.len().saturating_sub(1)), 0);
    ITEMS.with(|i| *i.borrow_mut() = items);
    LIST_MODS.with(|m| m.set(modifiers));
}

/// Brings the key list, the status line and the Save button in line with the controls.
fn refresh() {
    let Some(mut p) = PANEL.with(|p| p.get()) else { return };
    let modifiers = chosen_modifiers(&p);
    if LIST_MODS.with(|m| m.get()) != modifiers {
        rebuild_list(&p, modifiers);
    }
    let combo = chosen_combo(&p);
    let availability = chosen_index(&p)
        .and_then(|i| ITEMS.with(|items| items.borrow().get(i).map(|item| item.availability)))
        .unwrap_or(Availability::Free);

    let (tone, text) = combo::status(combo, availability);
    p.tone = tone;
    PANEL.with(|cell| cell.set(Some(p)));
    unsafe {
        let z = wide_z(&text);
        let _ = SetWindowTextW(p.status, PCWSTR(z.as_ptr()));
        let _ = InvalidateRect(Some(p.status), None, true);
        let _ = EnableWindow(p.save, combo::can_save(combo, availability));
    }
}

fn select_combo(p: &Panel, combo: Combo) {
    set_checked(p.win, combo.modifiers & WIN != 0);
    set_checked(p.ctrl, combo.modifiers & CONTROL != 0);
    set_checked(p.alt, combo.modifiers & ALT != 0);
    set_checked(p.shift, combo.modifiers & SHIFT != 0);
    // The list is probed for the new modifiers first, then the key is selected in it.
    rebuild_list(p, combo.modifiers);
    let index = KEYS.with(|k| k.borrow().iter().position(|(_, vk)| *vk == combo.key));
    if let Some(index) = index {
        send(p.key, CB_SETCURSEL, index, 0);
    }
}

fn on_create(hwnd: HWND, main: HWND) {
    let start = START.with(|s| s.get());
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }.map(Into::into).unwrap_or_default();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);

    let button = |text: PCWSTR, style: u32, id: i32| make(hwnd, instance, w!("BUTTON"), text, style | WS_TABSTOP.0, id);
    let check = BS_AUTOCHECKBOX as u32;
    let mut panel = Panel {
        hwnd,
        main,
        label: make(hwnd, instance, w!("STATIC"), w!("Shortcut that opens the grid"), SS_LEFT, 0),
        key_label: make(hwnd, instance, w!("STATIC"), w!("Key"), SS_LEFT, 0),
        win: button(w!("Win"), check, ID_WIN),
        ctrl: button(w!("Ctrl"), check, ID_CTRL),
        alt: button(w!("Alt"), check, ID_ALT),
        shift: button(w!("Shift"), check, ID_SHIFT),
        key: make(
            hwnd,
            instance,
            w!("COMBOBOX"),
            w!(""),
            CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32
                | WS_VSCROLL.0
                | WS_TABSTOP.0,
            ID_KEY,
        ),
        status: make(hwnd, instance, w!("STATIC"), w!(""), SS_LEFT, ID_STATUS),
        grid_label: make(hwnd, instance, w!("STATIC"), w!("Grid size"), SS_LEFT, 0),
        grids: make_grid_radios(hwnd, instance),
        // WS_GROUP ends the group of radio buttons before it.
        autostart: button(w!("Start with Windows"), check | WS_GROUP.0, ID_AUTOSTART),
        reset: button(w!("Use defaults"), BS_PUSHBUTTON as u32, ID_RESET),
        save: button(w!("Save"), BS_DEFPUSHBUTTON as u32, IDOK.0),
        cancel: button(w!("Cancel"), BS_PUSHBUTTON as u32, IDCANCEL.0),
        font: HFONT::default(),
        tone: Tone::Neutral,
    };
    layout(&mut panel, dpi);
    PANEL.with(|p| p.set(Some(panel)));

    set_checked(panel.autostart, start.autostart);
    set_grid(&panel, start.grid);
    select_combo(&panel, start.active.unwrap_or(Combo::DEFAULT));
    refresh();
}

fn on_save() {
    let Some(p) = PANEL.with(|p| p.get()) else { return };
    let combo = chosen_combo(&p);
    let availability = chosen_index(&p)
        .and_then(|i| ITEMS.with(|items| items.borrow().get(i).map(|item| item.availability)))
        .unwrap_or(Availability::Free);
    if !combo::can_save(combo, availability) {
        return;
    }
    let autostart = is_checked(p.autostart);
    let grid_size = chosen_grid(&p);
    // Close first, so the panel is gone when the confirmation balloon appears.
    unsafe {
        let _ = DestroyWindow(p.hwnd);
    }
    crate::apply_settings(p.main, combo, autostart, grid_size);
}

fn on_draw_item(item: &DRAWITEMSTRUCT) {
    if item.itemID == u32::MAX {
        return; // empty list
    }
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let (text, availability, windows_shortcut) = ITEMS.with(|items| {
        items
            .borrow()
            .get(item.itemID as usize)
            .map(|i| (i.text.clone(), i.availability, i.windows_shortcut))
            .unwrap_or_else(|| (Vec::new(), Availability::Free, false))
    });
    unsafe {
        let background = if selected { COLOR_HIGHLIGHT } else { COLOR_WINDOW };
        FillRect(item.hDC, &item.rcItem, GetSysColorBrush(background));
        let color = if selected {
            COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT))
        } else {
            match (availability, windows_shortcut) {
                (Availability::Taken, _) => RED,
                (Availability::Free, true) => AMBER,
                _ => COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
            }
        };
        SetBkMode(item.hDC, TRANSPARENT);
        SetTextColor(item.hDC, color);
        let mut text = text;
        let mut rect = item.rcItem;
        rect.left += 6;
        DrawTextW(item.hDC, &mut text, &mut rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    }
}

fn tone_color(tone: Tone) -> COLORREF {
    match tone {
        Tone::Good => GREEN,
        Tone::Warning => AMBER,
        Tone::Bad => RED,
        Tone::Neutral => COLORREF(unsafe { GetSysColor(COLOR_WINDOWTEXT) }),
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // lp points at the CREATESTRUCTW; its first field is the creation parameter.
            let create = lp.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            let main = HWND(unsafe { (*create).lpCreateParams });
            on_create(hwnd, main);
            LRESULT(0)
        }
        WM_COMMAND => {
            let (id, code) = (loword(wp.0), hiword(wp.0) as u32);
            match id {
                ID_WIN | ID_CTRL | ID_ALT | ID_SHIFT if code == BN_CLICKED => refresh(),
                ID_KEY if code == CBN_SELCHANGE => refresh(),
                ID_RESET if code == BN_CLICKED => {
                    if let Some(p) = PANEL.with(|p| p.get()) {
                        select_combo(&p, Combo::DEFAULT);
                        set_grid(&p, grid::DEFAULT);
                        refresh();
                    }
                }
                x if x == IDOK.0 => on_save(),
                x if x == IDCANCEL.0 => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            }
            LRESULT(0)
        }
        WM_MEASUREITEM => {
            let item = unsafe { &mut *(lp.0 as *mut MEASUREITEMSTRUCT) };
            if item.CtlID == ID_KEY as u32 {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
                item.itemHeight = px(ITEM_H, dpi) as u32;
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_DRAWITEM => {
            let item = unsafe { &*(lp.0 as *const DRAWITEMSTRUCT) };
            if item.CtlID == ID_KEY as u32 {
                on_draw_item(item);
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_CTLCOLORSTATIC => {
            let control = HWND(lp.0 as *mut c_void);
            let hdc = windows::Win32::Graphics::Gdi::HDC(wp.0 as *mut c_void);
            let color = match PANEL.with(|p| p.get()) {
                Some(p) if p.status == control => tone_color(p.tone),
                _ => COLORREF(unsafe { GetSysColor(COLOR_WINDOWTEXT) }),
            };
            unsafe {
                SetTextColor(hdc, color);
                SetBkMode(hdc, TRANSPARENT);
                LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
            }
        }
        WM_DPICHANGED => {
            let dpi = hiword(wp.0) as u32;
            let suggested = unsafe { &*(lp.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            if let Some(mut p) = PANEL.with(|p| p.get()) {
                layout(&mut p, dpi);
                PANEL.with(|cell| cell.set(Some(p)));
            }
            LRESULT(0)
        }
        // DM_GETDEFID: tells the dialog manager that Enter means IDOK (DC_HASDEFID = 0x534b).
        DM_GETDEFID => LRESULT((0x534b << 16) | IDOK.0 as isize),
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(p) = PANEL.with(|p| p.replace(None)) {
                if !p.font.is_invalid() {
                    unsafe {
                        let _ = DeleteObject(p.font.into());
                    }
                }
            }
            ITEMS.with(|i| i.borrow_mut().clear());
            tlog!("options panel closed");
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
