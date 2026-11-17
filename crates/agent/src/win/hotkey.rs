//! Atalhos no jogo: F2 (cataloga o spot onde o char está) e F1 (fixa esse spot como o
//! de retorno do char). Gancho de teclado de baixo nível que SÓ OBSERVA:
//! - age apenas com a janela do jogo em primeiro plano (fora do jogo não aciona o agente);
//! - nunca bloqueia a tecla (o jogo também a recebe);
//! - ignora teclas injetadas (as do próprio agente, SendInput).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetClassNameW, GetForegroundWindow, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// F2: guarda o tile atual no catálogo.
    SaveSpot,
    /// F1: este char volta sempre para o tile atual (também entra no catálogo).
    AssignSpot,
}

static TX: OnceLock<Mutex<Sender<(Hotkey, isize, Option<super::capture::Image>)>>> = OnceLock::new();
static HELD: AtomicU32 = AtomicU32::new(0);
static CLASS: OnceLock<String> = OnceLock::new();

fn down(vk: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

fn decode_key(vk: u32, injected: bool, modified: bool) -> Option<Hotkey> {
    if injected || modified {
        return None;
    }
    match vk {
        0x70 => Some(Hotkey::AssignSpot),
        0x71 => Some(Hotkey::SaveSpot),
        _ => None,
    }
}

unsafe extern "system" fn proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && matches!(w.0 as u32, WM_KEYUP | WM_SYSKEYUP) {
        let k = &*(l.0 as *const KBDLLHOOKSTRUCT);
        if k.flags.0 & 0x12 == 0 {
            let bit = match k.vkCode {
                0x71 => 1,
                0x70 => 2,
                _ => 0,
            };
            HELD.fetch_and(!bit, Ordering::SeqCst);
        }
    }
    if code >= 0 && w.0 as u32 == WM_KEYDOWN {
        let k = &*(l.0 as *const KBDLLHOOKSTRUCT);
        let injected = k.flags.0 & (LLKHF_INJECTED.0 | 0x2) != 0;
        let which = decode_key(
            k.vkCode,
            injected,
            down(VK_CONTROL.0) || down(VK_MENU.0) || down(VK_SHIFT.0),
        );
        if let Some(h) = which {
            let bit = if h == Hotkey::SaveSpot { 1 } else { 2 };
            if HELD.fetch_or(bit, Ordering::SeqCst) & bit != 0 {
                return CallNextHookEx(None, code, w, l);
            }
            let fg = GetForegroundWindow();
            let mut buf = [0u16; 128];
            let n = GetClassNameW(fg, &mut buf);
            let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            if CLASS.get().is_some_and(|c| *c == class) {
                if let Some(tx) = TX.get().and_then(|m| m.lock().ok()) {
                    // Só a cópia de 300x24 pixels fica no gancho. OCR e disco ficam fora.
                    let frame = super::capture::visible_region(fg.0 as isize, super::nav::CORNER);
                    let _ = tx.send((h, fg.0 as isize, frame));
                }
            }
        }
    }
    // Nunca bloqueia: a tecla segue para o jogo.
    CallNextHookEx(None, code, w, l)
}

/// Sobe a thread do gancho. Os atalhos chegam no `Receiver` correspondente.
pub fn start(window_class: &str, tx: Sender<(Hotkey, isize, Option<super::capture::Image>)>) {
    let _ = CLASS.set(window_class.to_string());
    let _ = TX.set(Mutex::new(tx));
    std::thread::spawn(|| unsafe {
        let Ok(inst) = GetModuleHandleW(None) else {
            tracing::warn!("atalhos: GetModuleHandle falhou");
            return;
        };
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), HINSTANCE::from(inst), 0) {
            Ok(_) => {
                tracing::info!("atalhos ligados: F2 cataloga spot, F1 fixa o spot do char (só com o jogo na frente)")
            }
            Err(e) => {
                tracing::warn!(erro = %e, "atalhos: não consegui instalar o gancho de teclado");
                return;
            }
        }
        // O gancho de baixo nível precisa de uma fila de mensagens na própria thread.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn apenas_f1_f2_fisicos_sem_modificadores() {
        assert_eq!(decode_key(0x70, false, false), Some(Hotkey::AssignSpot));
        assert_eq!(decode_key(0x71, false, false), Some(Hotkey::SaveSpot));
        assert_eq!(decode_key(0x53, false, true), None);
        assert_eq!(decode_key(0x44, false, true), None);
        assert_eq!(decode_key(0x70, true, false), None);
        assert_eq!(decode_key(0x71, false, true), None);
    }
}
