//! Native Win32 read-only preview; it does not load WebView2.
use std::ptr::{null, null_mut};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

struct Preview {
    text: Vec<u16>,
    exported: bool,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: the preview owns its context until its window and message loop finish.
    unsafe {
        match message {
            WM_CREATE => {
                let create = &*(lparam as *const CREATESTRUCTW);
                let context = &mut *(create.lpCreateParams as *mut Preview);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
                let module = GetModuleHandleW(null());
                let text = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    wide("EDIT").as_ptr(),
                    context.text.as_ptr(),
                    WS_CHILD
                        | WS_VISIBLE
                        | WS_TABSTOP
                        | WS_VSCROLL
                        | WS_HSCROLL
                        | ES_MULTILINE as u32
                        | ES_READONLY as u32,
                    12,
                    12,
                    680,
                    380,
                    window,
                    101 as HMENU,
                    module,
                    null(),
                );
                let back = CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    wide("Back").as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                    460,
                    405,
                    100,
                    32,
                    window,
                    2 as HMENU,
                    module,
                    null(),
                );
                let export = CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    wide("Export report…").as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    575,
                    405,
                    120,
                    32,
                    window,
                    100 as HMENU,
                    module,
                    null(),
                );
                let font = GetStockObject(DEFAULT_GUI_FONT);
                for control in [text, back, export] {
                    SendMessageW(control, WM_SETFONT, font as usize, 1);
                }
                SetFocus(back);
                0
            }
            WM_SIZE => {
                let mut rect = RECT::default();
                GetClientRect(window, &mut rect);
                let width = rect.right.max(360);
                let height = rect.bottom.max(240);
                MoveWindow(GetDlgItem(window, 101), 12, 12, width - 24, height - 68, 1);
                MoveWindow(GetDlgItem(window, 2), width - 260, height - 44, 100, 32, 1);
                MoveWindow(
                    GetDlgItem(window, 100),
                    width - 148,
                    height - 44,
                    136,
                    32,
                    1,
                );
                0
            }
            WM_COMMAND if wparam & 0xffff == 100 || wparam & 0xffff == 2 => {
                let context = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Preview;
                if !context.is_null() {
                    (*context).exported = wparam & 0xffff == 100;
                }
                DestroyWindow(window);
                0
            }
            WM_CLOSE => {
                DestroyWindow(window);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}
pub(super) fn preview_report(text: &str) -> bool {
    let mut context = Preview {
        text: wide(&text.replace('\n', "\r\n")),
        exported: false,
    };
    // SAFETY: all pointers are owned strings/context kept alive for this synchronous window.
    unsafe {
        let module = GetModuleHandleW(null());
        let class = wide("WiCErrorReportPreview");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..Default::default()
        };
        RegisterClassW(&window_class);
        let window = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class.as_ptr(),
            wide("Error report — exact export preview — WiC Replay Viewer").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            760,
            560,
            null_mut(),
            null_mut(),
            module,
            &mut context as *mut _ as *const _,
        );
        if window.is_null() {
            return false;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(window, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    context.exported
}
