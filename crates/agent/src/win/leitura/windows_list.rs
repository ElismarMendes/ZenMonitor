use crate::GameWindow;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

struct Ctx {
    class: String,
    out: Vec<GameWindow>,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut Ctx);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut buf = [0u16; 256];
    let n = GetClassNameW(hwnd, &mut buf);
    if n <= 0 || String::from_utf16_lossy(&buf[..n as usize]) != ctx.class {
        return BOOL(1);
    }
    let mut tbuf = [0u16; 512];
    let tn = GetWindowTextW(hwnd, &mut tbuf);
    let title = String::from_utf16_lossy(&tbuf[..tn.max(0) as usize]);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != 0 {
        ctx.out.push(GameWindow {
            pid,
            hwnd: hwnd.0 as isize,
            title,
        });
    }
    BOOL(1)
}

/// Janelas visíveis com a classe do jogo. Não toca no processo: só lê o título.
pub fn find_by_class(class: &str) -> Vec<GameWindow> {
    let mut ctx = Ctx {
        class: class.to_string(),
        out: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.out.sort_by_key(|w| w.pid);
    ctx.out
}
