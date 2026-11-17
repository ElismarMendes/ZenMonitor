//! Teclado e mouse de verdade (SendInput). O jogo ignora mensagens de janela (testado em
//! 08/11/2026: `zen-diag tecla`) e aceita SendInput com a janela em foco (`zen-diag tecla-input`).
//! Sem driver de kernel.
//!
//! Regras, todas verificadas aqui:
//! - Só passos da lista fechada (`zen_core::actions::Step`).
//! - A janela do char tem de estar em foco ANTES DE CADA passo; se perder o foco, para.
//! - Ao final, restaura o foco; no jogo, estaciona o cursor sobre o personagem.

use std::time::Duration;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetLastInputInfo, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, LASTINPUTINFO, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetSystemMetrics, IsIconic, SetCursorPos, SetForegroundWindow,
    ShowWindow, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE,
};
use zen_core::actions::Step;

/// Segundos desde o último toque REAL de teclado/mouse no PC (inclui o nosso próprio input).
pub fn idle_secs() -> u64 {
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if !GetLastInputInfo(&mut li).as_bool() {
            return 0;
        }
        u64::from(GetTickCount().wrapping_sub(li.dwTime)) / 1000
    }
}

// F8 alterna o controle manual. Eventos injetados são ignorados.
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static MANUAL: AtomicBool = AtomicBool::new(false);
/// GetTickCount do último toque físico (teclado ou mouse).
static REAL_LAST: AtomicU32 = AtomicU32::new(0);
static KEY_DOWN: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicU32 = AtomicU32::new(0);
static GUARD_ON: AtomicBool = AtomicBool::new(false);
static POINTER_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static POINTER_DEPTH: AtomicU32 = AtomicU32::new(0);
static POINTER_BLOCKED: AtomicU32 = AtomicU32::new(0);
const BOT_MOUSE_MARKER:usize=0x5A454E4D;
struct CursorPin { hwnd:isize, previous:windows::Win32::Foundation::RECT, rect:windows::Win32::Foundation::RECT }
static CURSOR_PIN:std::sync::Mutex<Option<CursorPin>>=std::sync::Mutex::new(None);
static PIN_TICK:AtomicU32=AtomicU32::new(0);

fn same_rect(a:windows::Win32::Foundation::RECT,b:windows::Win32::Foundation::RECT)->bool {
    (a.left,a.top,a.right,a.bottom)==(b.left,b.top,b.right,b.bottom)
}
fn release_stale_pin(elapsed_ms:u32,paused:bool,focused:bool)->bool {
    elapsed_ms>=1000 || paused || !focused
}
fn release_pin() {
    use windows::Win32::UI::WindowsAndMessaging::{ClipCursor,GetClipCursor};
    if let Ok(mut pin)=CURSOR_PIN.lock() {
        if let Some(saved)=pin.as_ref() {
            let mut current=windows::Win32::Foundation::RECT::default();
            unsafe {
                // Não sobrescreve uma restrição que outra aplicação tenha instalado depois.
                if GetClipCursor(&mut current).is_err() {return;}
                if same_rect(current,saved.rect)&&ClipCursor(Some(&saved.previous)).is_err() {return;}
            }
            *pin=None;
        }
    }
}
fn pin_screen(hwnd:HWND,x:i32,y:i32)->Result<(),String> {
    use windows::Win32::{Foundation::RECT,UI::WindowsAndMessaging::{ClipCursor,GetClipCursor}};
    if manual_mode()||unsafe {GetForegroundWindow()}!=hwnd {return Err("pausa/foco mudou antes de proteger cursor".into());}
    let mut pin=CURSOR_PIN.lock().map_err(|_|"proteção do cursor indisponível")?;
    let mut current=RECT::default();
    unsafe {GetClipCursor(&mut current)}.map_err(|_|"não li o limite anterior do cursor")?;
    let previous=pin.as_ref().filter(|s|same_rect(current,s.rect)).map_or(current,|s|s.previous);
    let rect=RECT{left:x,top:y,right:x+1,bottom:y+1};
    unsafe {ClipCursor(Some(&rect))}.map_err(|_|"não consegui proteger o ponto do clique")?;
    *pin=Some(CursorPin{hwnd:hwnd.0 as isize,previous,rect});
    PIN_TICK.store(unsafe {GetTickCount()},Ordering::SeqCst);
    Ok(())
}
fn pin_relative(hwnd:HWND,x:i32,y:i32)->Result<(),String> {
    let (ox,oy)=crate::win::capture::frame_origin(hwnd.0 as isize).ok_or("janela indisponível para proteger cursor")?;
    pin_screen(hwnd,ox+x,oy+y)
}

fn suppress_pointer_move(external:bool,message:u32,critical:bool,paused:bool,focused:bool)->bool {
    external && message==0x200 && critical && !paused && focused // WM_MOUSEMOVE apenas
}

/// Reserva curta durante clique/botão segurado. Não bloqueia teclado, F8 ou outros apps.
struct PointerGuard { hwnd:isize, phase:&'static str }
impl PointerGuard {
    fn begin(hwnd:HWND,phase:&'static str)->Self {
        if POINTER_DEPTH.fetch_add(1,Ordering::SeqCst)==0 {
            POINTER_BLOCKED.store(0,Ordering::SeqCst);
            POINTER_HWND.store(hwnd.0 as isize,Ordering::SeqCst);
        }
        Self{hwnd:hwnd.0 as isize,phase}
    }
}
impl Drop for PointerGuard {
    fn drop(&mut self) {
        if POINTER_DEPTH.fetch_sub(1,Ordering::SeqCst)==1 {
            release_pin();
            POINTER_HWND.store(0,Ordering::SeqCst);
            let count=POINTER_BLOCKED.swap(0,Ordering::SeqCst);
            if count>0 {tracing::info!(hwnd=self.hwnd,fase=self.phase,movimentos=count,"CURSOR: movimentos externos ignorados durante ação; controle liberado");}
        }
    }
}
/// Faixa na tela que mostra o estado da trava (pedido do dono, 10/11 22:58).
static BADGE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);


unsafe extern "system" fn badge_proc(h: HWND, msg: u32, w: windows::Win32::Foundation::WPARAM, l: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT};
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, GetClientRect, WM_PAINT};
    if msg == WM_PAINT {
        // Só um ponto colorido, sem texto (dono, 11/11 00:26: "algo discreto que vendo não dá
        // para saber o que é"). COLORREF = 0x00BBGGRR: verde = automação, vermelho = manual.
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(h, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(h, &mut rc);
        let brush = CreateSolidBrush(COLORREF(if MANUAL.load(Ordering::SeqCst) { 0x0000_00D0 } else { 0x0000_A000 }));
        FillRect(dc, &rc, brush);
        let _ = DeleteObject(brush);
        let _ = EndPaint(h, &ps);
        return windows::Win32::Foundation::LRESULT(0);
    }
    DefWindowProcW(h, msg, w, l)
}

/// Cria a faixa (na thread do gancho, que tem o laço de mensagens). Não recebe clique
/// (WS_EX_TRANSPARENT) e não entra nos prints do bot (PrintWindow é por janela).
unsafe fn create_badge(hmod: windows::Win32::Foundation::HINSTANCE) {
    use windows::core::w;
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, SetLayeredWindowAttributes, LWA_ALPHA, SM_CXSCREEN, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };
    let class = w!("WndDot32");
    let wc = WNDCLASSW { lpfnWndProc: Some(badge_proc), hInstance: hmod, lpszClassName: class, ..Default::default() };
    RegisterClassW(&wc);
    let sw = GetSystemMetrics(SM_CXSCREEN);
    // 6x6 px no canto de cima à direita da tela.
    let (bw, bh) = (6, 6);
    let h = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
        class,
        w!(""),
        WS_POPUP,
        sw - bw - 2,
        2,
        bw,
        bh,
        None,
        None,
        hmod,
        None,
    );
    match h {
        Ok(h) => {
            let _ = SetLayeredWindowAttributes(h, COLORREF(0), 200, LWA_ALPHA);
            BADGE.store(h.0 as isize, Ordering::SeqCst);
            update_badge();
        }
        Err(e) => tracing::warn!(erro = %e, "trava manual: faixa na tela não criada"),
    }
}

/// Mostra a faixa no estado atual e pede para redesenhar.
fn update_badge() {
    use windows::Win32::Graphics::Gdi::InvalidateRect;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE;
    let raw = BADGE.load(Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    let h = HWND(raw as *mut _);
    unsafe {
        let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        let _ = InvalidateRect(h, None, true);
    }
}

/// Automação pausada porque você pediu (F8).
/// Automação parada: pelo dono (F8) ou pelo painel (automação do PC desligada).
pub fn manual_mode() -> bool {
    if let Some(epoch) = EXPLICIT_MOVE.with(|v| v.get()) {
        return epoch != CANCEL.load(Ordering::SeqCst);
    }
    MANUAL.load(Ordering::SeqCst) || PAUSED.load(Ordering::SeqCst)
}

/// Só a trava do mouse (indicador no painel).
pub fn manual_by_hotkey() -> bool {
    MANUAL.load(Ordering::SeqCst)
}

/// Automação do PC desligada/ligada pelo painel: desligada, qualquer sequência em andamento
/// para no próximo passo.
pub fn set_paused(on: bool) {
    PAUSED.store(on, Ordering::SeqCst);
    if on { CANCEL.fetch_add(1, Ordering::SeqCst); release_pin(); }
}

static PAUSED: AtomicBool = AtomicBool::new(false);

/// Segundos desde o último toque FÍSICO (sem o nosso input). Sem o gancho, cai no do Windows.
pub fn real_idle_secs() -> u64 {
    if !GUARD_ON.load(Ordering::SeqCst) {
        return idle_secs();
    }
    let now = unsafe { GetTickCount() };
    u64::from(now.wrapping_sub(REAL_LAST.load(Ordering::SeqCst))) / 1000
}

unsafe extern "system" fn mouse_hook(code: i32, w: windows::Win32::Foundation::WPARAM, l: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, MSLLHOOKSTRUCT};
    if code >= 0 {
        let info = &*(l.0 as *const MSLLHOOKSTRUCT);
        let protected=POINTER_HWND.load(Ordering::SeqCst);
        let paused=MANUAL.load(Ordering::SeqCst)||(PAUSED.load(Ordering::SeqCst)&&!EXPLICIT_ACTIVE.load(Ordering::SeqCst));
        if suppress_pointer_move(info.dwExtraInfo!=BOT_MOUSE_MARKER,w.0 as u32,protected!=0,paused,
            protected!=0 && GetForegroundWindow().0 as isize==protected) {
            if info.flags&3==0 {REAL_LAST.store(GetTickCount(),Ordering::SeqCst);}
            POINTER_BLOCKED.fetch_add(1,Ordering::SeqCst);
            return windows::Win32::Foundation::LRESULT(1);
        }
        if info.flags & 3 == 0 {
            REAL_LAST.store(GetTickCount(), Ordering::SeqCst);
            // Clique físico pode abrir/fechar painel. Movimento simples não invalida.
            if matches!(w.0 as u32,0x201|0x202|0x204|0x205|0x207|0x208) {crate::win::viewport::invalidate();}
        }
    }
    CallNextHookEx(None, code, w, l)
}

unsafe extern "system" fn key_hook(code: i32, w: windows::Win32::Foundation::WPARAM, l: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP};
    if code >= 0 {
        let info = &*(l.0 as *const KBDLLHOOKSTRUCT);
        // LLKHF_INJECTED = 0x10, LLKHF_LOWER_IL_INJECTED = 0x2
        if info.flags.0 & 0x12 == 0 {
            REAL_LAST.store(GetTickCount(), Ordering::SeqCst);
            if matches!(w.0 as u32,WM_KEYDOWN|WM_KEYUP|WM_SYSKEYDOWN|WM_SYSKEYUP) {crate::win::viewport::invalidate();}
            if info.vkCode == 0x77 {
                match w.0 as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        if !KEY_DOWN.swap(true, Ordering::SeqCst) {
                            let manual = EXPLICIT_ACTIVE.load(Ordering::SeqCst) || !MANUAL.load(Ordering::SeqCst);
                            MANUAL.store(manual, Ordering::SeqCst);
                            if manual {release_pin();}
                            CANCEL.fetch_add(1, Ordering::SeqCst);
                            update_badge();
                            std::thread::spawn(move || {
                                tracing::warn!(manual, "F8: controle manual alterado");
                                if manual { beep(&[(1200, 150), (700, 250)]); }
                                else { beep(&[(700, 150), (1200, 250)]); }
                            });
                        }
                    }
                    WM_KEYUP | WM_SYSKEYUP => { KEY_DOWN.store(false, Ordering::SeqCst); }
                    _ => {}
                }
                return windows::Win32::Foundation::LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, w, l)
}

fn beep(tones: &[(u32, u32)]) {
    for &(f, d) in tones {
        unsafe {
            let _ = windows::Win32::System::Diagnostics::Debug::Beep(f, d);
        }
    }
}

/// Liga F8; retoma após `release_s` sem uso.
pub fn start_manual_guard(release_s: u64) {
    if GUARD_ON.swap(true, Ordering::SeqCst) {
        return;
    }
    REAL_LAST.store(unsafe { GetTickCount() }, Ordering::SeqCst);
    std::thread::spawn(|| unsafe {
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, MSG, WH_KEYBOARD_LL, WH_MOUSE_LL};
        let hmod: windows::Win32::Foundation::HINSTANCE = GetModuleHandleW(None).map(Into::into).unwrap_or_default();
        let m = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), hmod, 0);
        let k = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), hmod, 0);
        if m.is_err() || k.is_err() {
            tracing::error!(mouse = ?m.err(), teclado = ?k.err(), "trava manual: gancho não instalado; usando o 'sem uso' do Windows");
            GUARD_ON.store(false, Ordering::SeqCst);
            return;
        }
        tracing::warn!("trava manual ligada: F8 para pausar ou retomar a automação");
        create_badge(hmod);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let stale_pin=CURSOR_PIN.lock().ok().and_then(|p|p.as_ref().map(|p|p.hwnd));
            if let Some(hwnd)=stale_pin {
                let paused=MANUAL.load(Ordering::SeqCst)||(PAUSED.load(Ordering::SeqCst)&&!EXPLICIT_ACTIVE.load(Ordering::SeqCst));
                if release_stale_pin(unsafe {GetTickCount()}.wrapping_sub(PIN_TICK.load(Ordering::SeqCst)),paused,
                    unsafe {GetForegroundWindow()}.0 as isize==hwnd) {release_pin();}
            }
            if !GUARD_ON.load(Ordering::SeqCst) {
                return;
            }
            if MANUAL.load(Ordering::SeqCst) && real_idle_secs() >= release_s {
                MANUAL.store(false, Ordering::SeqCst);
                tracing::warn!("controle manual encerrado: {release_s} s sem uso; automação volta");
                update_badge();
                beep(&[(700, 150), (1200, 250)]);
            }
        }
    });
}

fn kb(vk: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn mouse(dx: i32, dy: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: BOT_MOUSE_MARKER } },
    }
}

fn point_button(nx:i32,ny:i32,button:MOUSE_EVENT_FLAGS)->INPUT {
    // Sem MOVE, dx/dy do evento de botão não reposicionam o cursor. Movimento + botão
    // no mesmo INPUT eliminam a janela entre posicionamento e pressionamento/soltura.
    mouse(nx,ny,MOUSEEVENTF_MOVE|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK|button)
}

fn repair_cursor(hwnd:HWND,x:i32,y:i32)->Result<bool,String> {
    PIN_TICK.store(unsafe {GetTickCount()},Ordering::SeqCst);
    let (ox,oy)=crate::win::capture::frame_origin(hwnd.0 as isize).ok_or("janela do jogo indisponível")?;
    let mut p=POINT::default();
    unsafe {GetCursorPos(&mut p)}.map_err(|_|"não li a posição do cursor")?;
    if (i64::from(p.x)-i64::from(ox+x)).abs().max((i64::from(p.y)-i64::from(oy+y)).abs())<=1 {return Ok(false);}
    let (nx,ny)=absolute(hwnd,x,y)?;
    if !send(&[mouse(nx,ny,MOUSEEVENTF_MOVE|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK)]) {return Err("não consegui reposicionar o cursor".into());}
    Ok(true)
}

fn send(i: &[INPUT]) -> bool {
    unsafe { SendInput(i, std::mem::size_of::<INPUT>() as i32) as usize == i.len() }
}

/// Janela do jogo (hwnd) de uma sequência em andamento (`Session`): enquanto ela existir, o
/// foco NÃO volta para a janela anterior entre um passo e outro. Antes, cada passo devolvia o
/// foco para quem estava na frente (ex.: VMware) e o jogo perdia o foco no meio do fluxo.
static SESSION: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

// SendInput é global ao desktop. Uma captura de outro char pode continuar, mas nenhuma
// outra thread pode tomar seu foco durante uma sessão, tecla ou caminhada segurada.
#[derive(Default)]
struct InputOwner {
    target: Option<(std::thread::ThreadId, isize, u32)>,
    depth: usize,
}

impl InputOwner {
    fn acquire(&mut self, thread: std::thread::ThreadId, hwnd: isize, pid: u32) -> Result<(), String> {
        let requested = (thread, hwnd, pid);
        if self.target.is_some_and(|owner| owner != requested) {
            return Err("entrada reservada por outra rotina/janela; comando não enviado".into());
        }
        self.target = Some(requested);
        self.depth += 1;
        Ok(())
    }

    fn release(&mut self) {
        self.depth -= 1;
        if self.depth == 0 { self.target = None; }
    }
}

static INPUT_OWNER: std::sync::Mutex<InputOwner> = std::sync::Mutex::new(InputOwner { target: None, depth: 0 });

struct InputLease {
    hwnd: isize,
    pid: u32,
    name: Option<String>,
    // A reserva pertence à thread que a criou.
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl InputLease {
    fn acquire(hwnd: isize) -> Result<Self, String> {
        let pid = crate::win::capture::window_pid(hwnd).ok_or("janela de destino inválida")?;
        let name = zen_core::title::parse(&window_label(HWND(hwnd as *mut _))).name;
        INPUT_OWNER.lock().map_err(|_| "reserva de entrada indisponível")?
            .acquire(std::thread::current().id(), hwnd, pid)?;
        Ok(Self { hwnd, pid, name, _thread: std::marker::PhantomData })
    }

    fn validate(&self, foreground: bool) -> Result<(), String> {
        let hwnd = HWND(self.hwnd as *mut _);
        if crate::win::capture::window_pid(self.hwnd) != Some(self.pid)
            || zen_core::title::parse(&window_label(hwnd)).name != self.name
            || (foreground && unsafe { GetForegroundWindow() } != hwnd)
            || manual_mode()
        {
            return Err("janela, personagem ou foco mudou; comando não enviado".into());
        }
        Ok(())
    }
}

impl Drop for InputLease {
    fn drop(&mut self) {
        if let Ok(mut owner) = INPUT_OWNER.lock() { owner.release(); }
    }
}

pub(in crate::win) fn reserve(hwnd: isize) -> Result<impl Drop, String> {
    InputLease::acquire(hwnd)
}

#[cfg(test)]
mod input_owner_tests {
    use super::{cursor_maintenance_due, cursor_needs_home, InputOwner};

    #[test]
    fn cursor_pin_expires_on_stall_pause_or_focus_loss() {
        use super::release_stale_pin as release;
        assert!(!release(999,false,true));
        assert!(release(1000,false,true));
        assert!(release(0,true,true));
        assert!(release(0,false,false));
    }

    #[test]
    fn pointer_guard_releases_for_f8_other_apps_and_never_filters_buttons_or_bot_input() {
        use super::suppress_pointer_move as block;
        assert!(block(true,0x200,true,false,true));
        assert!(!block(true,0x200,true,true,true)); // F8/painel
        assert!(!block(true,0x200,true,false,false)); // outra aplicação
        assert!(!block(true,0x200,false,false,true)); // entre ações
        assert!(!block(false,0x200,true,false,true)); // SendInput do bot
        assert!(!block(true,0x201,true,false,true)); // botão físico não é movimento
    }

    #[test]
    fn button_packet_repositions_and_presses_at_same_point() {
        use super::*;
        let event=point_button(12345,23456,MOUSEEVENTF_LEFTDOWN);
        let mi=unsafe {event.Anonymous.mi};
        assert_eq!((mi.dx,mi.dy),(12345,23456));
        assert_eq!(mi.dwExtraInfo,BOT_MOUSE_MARKER);
        assert!(mi.dwFlags.contains(MOUSEEVENTF_MOVE|MOUSEEVENTF_LEFTDOWN|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK));
    }

    #[test]
    fn maintenance_waits_for_idle_and_never_interrupts_manual_or_pressed_button() {
        assert!(cursor_maintenance_due(10_000, 3, false, false));
        assert!(!cursor_maintenance_due(9_999, 3, false, false));
        assert!(!cursor_maintenance_due(10_000, 2, false, false));
        assert!(!cursor_maintenance_due(10_000, 3, true, false));
        assert!(!cursor_maintenance_due(10_000, 3, false, true));
    }

    #[test]
    fn cursor_at_character_is_left_alone_including_small_pixel_drift() {
        assert!(!cursor_needs_home((515, 400), (515, 400)));
        assert!(!cursor_needs_home((519, 396), (515, 400)));
        assert!(cursor_needs_home((520, 400), (515, 400)));
        assert!(cursor_needs_home((515, 395), (515, 400)));
    }

    #[test]
    fn home_uses_window_origin_even_on_negative_monitor_coordinates() {
        assert!(!cursor_needs_home((-1405, 426), (-1920 + 515, 26 + 400)));
        assert!(cursor_needs_home((515, 400), (-1920 + 515, 26 + 400)));
    }

    #[test]
    fn nested_steps_keep_session_reserved_until_last_release() {
        let mut owner = InputOwner::default();
        let thread = std::thread::current().id();
        owner.acquire(thread, 10, 100).unwrap();
        owner.acquire(thread, 10, 100).unwrap();
        owner.release();
        assert!(owner.acquire(thread, 20, 200).is_err());
        owner.release();
        owner.acquire(thread, 20, 200).unwrap();
    }

    #[test]
    fn rejects_other_thread_even_for_same_character() {
        let mut owner = InputOwner::default();
        owner.acquire(std::thread::current().id(), 10, 100).unwrap();
        let other = std::thread::spawn(|| std::thread::current().id()).join().unwrap();
        assert!(owner.acquire(other, 10, 100).is_err());
        assert_eq!(owner.depth, 1);
    }

    #[test]
    fn rejects_reused_handle_with_different_process() {
        let mut owner = InputOwner::default();
        let thread = std::thread::current().id();
        owner.acquire(thread, 10, 100).unwrap();
        assert!(owner.acquire(thread, 10, 200).is_err());
        assert_eq!(owner.depth, 1);
    }
}

fn in_session(hwnd: HWND) -> bool {
    SESSION.load(std::sync::atomic::Ordering::SeqCst) == hwnd.0 as isize
}

/// Texto curto que identifica uma janela (título ou classe) para o log.
fn window_label(h: HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowTextW};
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    if n > 0 {
        return String::from_utf16_lossy(&buf[..n as usize]);
    }
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Janela deste processo (a faixa da trava): nunca minimizar.
fn is_ours(h: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid == std::process::id()
}

fn is_shell(h: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    let c = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    matches!(c.as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW")
}

/// Janela de topo que está no ponto (coordenadas da janela do jogo) — nós ou outra na frente.
fn root_at(hwnd: HWND, x: i32, y: i32) -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    let (ox, oy) = crate::win::capture::frame_origin(hwnd.0 as isize)?;
    let h = unsafe { WindowFromPoint(POINT { x: ox + x, y: oy + y }) };
    if h.0.is_null() {
        return None;
    }
    Some(unsafe { GetAncestor(h, GA_ROOT) })
}

/// Garante que nada cobre o jogo nos pontos dados: minimiza a janela que estiver na frente
/// (pedido em 09/11: "trazer a tela do MU para frente e minimizar as existentes"). Só mexe em
/// quem está POR CIMA do jogo; barra de tarefas e área de trabalho ficam.
fn clear_view(hwnd: HWND, points: &[(i32, i32)]) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE;
    for _ in 0..4 {
        let blocker = points.iter().filter_map(|&(x, y)| root_at(hwnd, x, y)).find(|r| *r != hwnd && !is_shell(*r) && !is_ours(*r));
        let Some(b) = blocker else { return Ok(()) };
        tracing::warn!(janela = %window_label(b), "AUTOMAÇÃO: janela na frente do jogo; minimizando");
        unsafe {
            let _ = ShowWindow(b, SW_MINIMIZE);
        }
        std::thread::sleep(Duration::from_millis(400));
        focus(hwnd);
    }
    match points.iter().filter_map(|&(x, y)| root_at(hwnd, x, y)).find(|r| *r != hwnd && !is_shell(*r) && !is_ours(*r)) {
        Some(b) => Err(format!("a janela \"{}\" continua na frente do jogo", window_label(b))),
        None => Ok(()),
    }
}

/// Pontos de conferência: centro e cantos internos da área do jogo.
const VIEW_POINTS: [(i32, i32); 5] = [(515, 400), (100, 120), (930, 120), (100, 690), (930, 690)];

fn game_window(hwnd: HWND) -> bool {
    zen_core::title::parse(&window_label(hwnd)).is_logged_in()
}

fn mouse_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
    unsafe { [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].iter().any(|vk| GetAsyncKeyState(vk.0 as i32) < 0) }
}

fn cursor_needs_home(current: (i32, i32), home: (i32, i32)) -> bool {
    (i64::from(current.0) - i64::from(home.0)).abs().max(
        (i64::from(current.1) - i64::from(home.1)).abs()) > 4
}

/// Só atua na janela realmente em foco, com centro descoberto e nenhum botão pressionado.
fn park_game_cursor(hwnd: HWND) -> bool {
    if manual_mode() || mouse_button_down() || unsafe { GetForegroundWindow() } != hwnd || !game_window(hwnd) {
        return false;
    }
    let Some(layout)=crate::win::viewport::current(hwnd.0 as isize) else {return false;};
    let (x, y) = layout.home();
    if root_at(hwnd, x, y) != Some(hwnd) { return false; }
    let Some((ox, oy)) = crate::win::capture::frame_origin(hwnd.0 as isize) else { return false; };
    let mut current = POINT::default();
    if unsafe { GetCursorPos(&mut current) }.is_err()
        || !cursor_needs_home((current.x, current.y), (ox + x, oy + y)) { return false; }
    let Ok((nx, ny)) = absolute(hwnd, x, y) else { return false; };
    // Reconfere depois das consultas: o usuário pode ter pressionado um botão/trocado o foco.
    if manual_mode() || mouse_button_down() || unsafe { GetForegroundWindow() } != hwnd { return false; }
    send(&[mouse(nx, ny, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)])
}

fn restore_context(hwnd: HWND, previous: HWND, cursor: Option<POINT>) {
    // Se o dono tomou o foco ou pausou, preserve seu controle.
    if manual_mode() || mouse_button_down() || unsafe { GetForegroundWindow() } != hwnd { return; }
    park_game_cursor(hwnd);
    if !previous.0.is_null() && previous != hwnd {
        if game_window(previous) {
            // SetForegroundWindow pode retornar antes de a ativação se completar. Usar
            // a mesma confirmação da entrada evita estacionar na janela que ainda sai.
            if !focus(previous) {
                tracing::warn!(hwnd = previous.0 as isize, "CURSOR: foco anterior não confirmado ao devolver a janela");
            }
            return;
        }
        unsafe { let _ = SetForegroundWindow(previous); }
    }
    if game_window(previous) {
        park_game_cursor(previous);
    } else if let Some(c) = cursor {
        unsafe { let _ = SetCursorPos(c.x, c.y); }
    }
}

static LAST_CURSOR_CHECK: AtomicU32 = AtomicU32::new(0);

fn cursor_maintenance_due(elapsed_ms: u32, idle_s: u64, manual: bool, pressed: bool) -> bool {
    elapsed_ms >= 10_000 && idle_s >= 3 && !manual && !pressed
}

/// O trabalhador chama apenas para chars farmando. Não troca foco nem ativa janela de fundo.
pub fn maintain_cursor(hwnd_raw: isize) {
    let now = unsafe { GetTickCount() };
    let last = LAST_CURSOR_CHECK.load(Ordering::SeqCst);
    let hwnd = HWND(hwnd_raw as *mut _);
    if unsafe { GetForegroundWindow() } != hwnd
        || !cursor_maintenance_due(now.wrapping_sub(last), real_idle_secs(), manual_mode(), mouse_button_down()) {
        return;
    }
    let Ok(lease) = InputLease::acquire(hwnd_raw) else { return; };
    if lease.validate(true).is_err() { return; }
    LAST_CURSOR_CHECK.store(now, Ordering::SeqCst);
    if park_game_cursor(hwnd) {
        tracing::info!(hwnd = hwnd_raw, pid = lease.pid, layout=?crate::win::viewport::current(hwnd_raw), "CURSOR: reposicionado no centro do personagem durante farm");
    }
}

/// Sequência de passos num char (volta ao spot, baú): traz o jogo para a frente, tira o que
/// estiver por cima e mantém o foco até o fim; ao sair, devolve foco e mouse uma vez só.
pub struct Session {
    hwnd: HWND,
    previous: HWND,
    cursor: Option<POINT>,
    _lease: InputLease,
}

impl Session {
    pub fn begin(hwnd_raw: isize) -> Result<Self, String> {
        let lease = InputLease::acquire(hwnd_raw)?;
        let hwnd = HWND(hwnd_raw as *mut _);
        let previous = unsafe { GetForegroundWindow() };
        let mut c = POINT::default();
        let cursor = unsafe { GetCursorPos(&mut c).is_ok() }.then_some(c);
        if manual_mode() {
            return Err("automação parada (painel ou F8): automação pausada".into());
        }
        SESSION.compare_exchange(0, hwnd_raw, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "já existe uma sessão de automação em andamento")?;
        let s = Session { hwnd, previous, cursor, _lease: lease };
        if !focus(hwnd) {
            return Err("não consegui trazer a janela do char para a frente".into());
        }
        clear_view(hwnd, &VIEW_POINTS)?;
        s._lease.validate(true)?;
        park_game_cursor(hwnd);
        tracing::info!(hwnd = hwnd_raw, pid = s._lease.pid, janela = %window_label(hwnd), "INPUT: sessão iniciada");
        Ok(s)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        SESSION.store(0, std::sync::atomic::Ordering::SeqCst);
        restore_context(self.hwnd, self.previous, self.cursor);
    }
}

/// A janela do char saiu da frente no meio de uma sequência: até 3 tentativas (1,5 s entre
/// elas) de trazer de volta, a não ser que o dono tenha pedido o controle (F8).
fn regain(hwnd: HWND) -> bool {
    for n in 0..3 {
        if manual_mode() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1500));
        if focus(hwnd) {
            tracing::warn!(tentativa = n + 1, "AUTOMAÇÃO: a janela do char saiu da frente; trouxe de volta");
            return true;
        }
    }
    false
}

fn focus(hwnd: HWND) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{BringWindowToTop, GetWindowThreadProcessId, SW_MINIMIZE};
    let is_fg = || {
        let focused = unsafe { GetForegroundWindow() == hwnd };
        if focused { park_game_cursor(hwnd); }
        focused
    };
    let wait = |ms: u64| std::thread::sleep(Duration::from_millis(ms));
    if is_fg() {
        return true;
    }
    park_game_cursor(unsafe { GetForegroundWindow() });
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // 1) direto
        let _ = SetForegroundWindow(hwnd);
        wait(150);
        if is_fg() {
            return true;
        }
        // 2) o Windows só deixa trocar o foco depois de um input: ALT sozinho (não faz nada no jogo)
        send(&[kb(VK_MENU.0, KEYBD_EVENT_FLAGS(0)), kb(VK_MENU.0, KEYEVENTF_KEYUP)]);
        let _ = SetForegroundWindow(hwnd);
        wait(250);
        if is_fg() {
            return true;
        }
        // 3) anexa a entrada da thread da janela da frente à nossa (visto em 08/11: o console
        //    do central na frente recusou os passos 1 e 2)
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        if fg_thread != 0 && fg_thread != me {
            let _ = AttachThreadInput(me, fg_thread, true);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(me, fg_thread, false);
            wait(250);
            if is_fg() {
                return true;
            }
        }
        // 4) minimiza e restaura: a janela restaurada é ativada
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
        wait(300);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    wait(500);
    is_fg()
}

/// Executa os passos na janela. Devolve erro sem apertar nada se o foco não vier.
pub fn run(hwnd_raw: isize, steps: &[Step]) -> Result<(), String> {
    let lease = InputLease::acquire(hwnd_raw)?;
    lease.validate(false)?;
    let hwnd = HWND(hwnd_raw as *mut _);
    let previous = unsafe { GetForegroundWindow() };
    let mut cursor = POINT::default();
    let had_cursor = unsafe { GetCursorPos(&mut cursor).is_ok() };
    let result = (|| {
        if !focus(hwnd) {
            return Err("não consegui trazer a janela do char para a frente".to_string());
        }
        for (i, st) in steps.iter().enumerate() {
            if manual_mode() {
                return Err("automação parada (painel ou F8): parei".to_string());
            }
            if unsafe { GetForegroundWindow() } != hwnd && !regain(hwnd) {
                return Err(format!("a janela perdeu o foco no passo {} (alguém clicou em outra janela?)", i + 1));
            }
            let changes_layout=match *st {
                Step::Key(vk)=> !matches!(vk,0x24|0x30..=0x39),
                Step::Click{x,y}|Step::RightClick{x,y}=>crate::win::viewport::cached(hwnd_raw,lease.pid)
                    .map_or(y>=640,|layout|layout.click_changes_layout(x,y)),
                _=>false,
            };
            let mut pointer_guard=None;
            match *st {
                Step::Key(vk) => {
                    park_game_cursor(hwnd);
                    lease.validate(true)?;
                    if vk == 0x24 {
                        tracing::info!(hwnd = hwnd_raw, pid = lease.pid, janela = %window_label(hwnd), "INPUT: enviando Home ao Helper");
                    }
                    if changes_layout {crate::win::viewport::invalidate();}
                    send(&[kb(vk, KEYBD_EVENT_FLAGS(0))]);
                    // 200 ms: o jogo lê as teclas uma vez por quadro. Com o notebook a 7 FPS
                    // (143 ms por quadro) os 70 ms de antes caíam entre dois quadros e V/Home não
                    // faziam nada (YouSee, 12/11: "não li o Zen do inventário" a noite toda).
                    std::thread::sleep(Duration::from_millis(200));
                    send(&[kb(vk, KEYEVENTF_KEYUP)]);
                    if changes_layout {crate::win::viewport::invalidate();}
                }
                Step::Click { x, y } | Step::RightClick { x, y } | Step::RightHold { x, y, .. } | Step::Hover { x, y } | Step::Wheel { x, y, .. } => {
                    // 180 ms (era 70): com o jogo a 7 FPS o clique curto caía entre dois quadros e era
                    // perdido (YouSee, 12/11: 3 cliques para abrir o Trainer e o "Restore life" não pegou).
                    let hold_ms = if let Step::RightHold { ms, .. } = *st { u64::from(ms.min(15_000)) } else { 180 };
                    let (down, up) = if matches!(*st, Step::RightClick { .. } | Step::RightHold { .. }) {
                        (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
                    } else {
                        (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
                    };
                    // O clique vai para quem estiver NAQUELE ponto da tela: tem de ser o jogo.
                    clear_view(hwnd, &[(x, y)])?;
                    if unsafe { GetForegroundWindow() } != hwnd {
                        return Err("a janela perdeu o foco ao tirar outra da frente".into());
                    }
                    let (ox, oy) = crate::win::capture::frame_origin(hwnd.0 as isize).ok_or("não achei a janela do jogo")?;
                    let (sx, sy) = (ox + x, oy + y);
                    let (vx, vy, vw, vh) = unsafe {
                        (
                            GetSystemMetrics(SM_XVIRTUALSCREEN),
                            GetSystemMetrics(SM_YVIRTUALSCREEN),
                            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
                            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
                        )
                    };
                    let nx = ((sx - vx) as i64 * 65535 / (vw - 1) as i64) as i32;
                    let ny = ((sy - vy) as i64 * 65535 / (vh - 1) as i64) as i32;
                    let abs = MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
                    pointer_guard=Some(PointerGuard::begin(hwnd,"clique"));
                    pin_screen(hwnd,sx,sy)?;
                    send(&[mouse(nx, ny, MOUSEEVENTF_MOVE | abs)]);
                    std::thread::sleep(Duration::from_millis(60));
                    if matches!(*st, Step::Hover { .. }) {
                        continue;
                    }
                    if let Step::Wheel { delta, .. } = *st {
                        // Roda só com o cursor dentro do jogo e no ponto pedido (fora da janela
                        // da lista ela mexe no zoom da câmera).
                        lease.validate(true)?;
                        if root_at(hwnd, x, y) != Some(hwnd) {
                            return Err("ponto da rolagem coberto; comando não enviado".into());
                        }
                        let wheel = INPUT {
                            r#type: INPUT_MOUSE,
                            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: delta as _, dwFlags: MOUSEEVENTF_WHEEL, time: 0, dwExtraInfo: BOT_MOUSE_MARKER } },
                        };
                        if !send(&[wheel]) { return Err("a rolagem não foi enviada ao jogo".into()); }
                        std::thread::sleep(Duration::from_millis(120));
                        release_pin();
                        continue;
                    }
                    lease.validate(true)?;
                    if root_at(hwnd, x, y) != Some(hwnd) {
                        return Err("ponto de clique coberto; comando não enviado".into());
                    }
                    if changes_layout {crate::win::viewport::invalidate();}
                    if !send(&[point_button(nx,ny,down)]) {return Err("o clique não foi enviado ao jogo".into());}
                    // Segurando: confere o foco a cada 100 ms e solta se a janela perder a frente.
                    let t = std::time::Instant::now();
                    let mut repairs=0;
                    let mut repair_failed=false;
                    while t.elapsed() < Duration::from_millis(hold_ms) {
                        std::thread::sleep(Duration::from_millis(hold_ms.min(25)));
                        if unsafe { GetForegroundWindow() } != hwnd || manual_mode() {
                            break;
                        }
                        // Apenas posição do cursor: sem captura de tela/leitura da RAM do jogo.
                        match repair_cursor(hwnd,x,y) {
                            Ok(true)=>repairs+=1,
                            Ok(false)=>{},
                            Err(_)=>{repair_failed=true;break;},
                        }
                    }
                    let interrupted=unsafe {GetForegroundWindow()}!=hwnd || manual_mode() || repair_failed;
                    if !interrupted {
                        send(&[point_button(nx,ny,up)]);
                    } else {send(&[mouse(0,0,up)]);}
                    if repairs>0 {tracing::warn!(hwnd=hwnd_raw,correcoes=repairs,alvo=?(x,y),"CURSOR: deslocamento externo corrigido durante clique; ação não repetida");}
                    if changes_layout {crate::win::viewport::invalidate();}
                    if interrupted {return Err("clique interrompido por pausa/perda de foco; conferir o resultado antes de avançar".into());}
                }
                Step::WaitMs(ms) => std::thread::sleep(Duration::from_millis(u64::from(ms.min(30_000)))),
            }
            std::thread::sleep(Duration::from_millis(120));
            if pointer_guard.is_some() {release_pin();}
            if changes_layout {park_game_cursor(hwnd);}
            // O cliente pode consumir o mouse-up no quadro seguinte. Preserve o alvo
            // também durante a espera existente de confirmação, sem alongar o clique.
            drop(pointer_guard);
        }
        Ok(())
    })();
    if !in_session(hwnd) {
        restore_context(hwnd, previous, had_cursor.then_some(cursor));
    }
    result
}

/// Converte coordenadas da JANELA INTEIRA em coordenadas absolutas do SendInput.
fn absolute(hwnd: HWND, x: i32, y: i32) -> Result<(i32, i32), String> {
    let (ox, oy) = crate::win::capture::frame_origin(hwnd.0 as isize).ok_or("não achei a janela do jogo")?;
    let (sx, sy) = (ox + x, oy + y);
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
        )
    };
    Ok((((sx - vx) as i64 * 65535 / (vw - 1) as i64) as i32, ((sy - vy) as i64 * 65535 / (vh - 1) as i64) as i32))
}

/// Botão esquerdo SEGURADO na janela do char (o char anda na direção do cursor).
/// Solta o botão e devolve foco e mouse no `release` ou quando sai de escopo (Drop),
/// inclusive em erro. Se a janela perder o foco, `steer` solta e devolve erro.
pub struct Hold {
    hwnd: HWND,
    previous: HWND,
    cursor: Option<POINT>,
    down: bool,
    lease: InputLease,
    pointer: Option<PointerGuard>,
}

impl Hold {
    pub fn start(hwnd_raw: isize, x: i32, y: i32) -> Result<Self, String> {
        let lease = InputLease::acquire(hwnd_raw)?;
        lease.validate(false)?;
        let hwnd = HWND(hwnd_raw as *mut _);
        let previous = unsafe { GetForegroundWindow() };
        let mut c = POINT::default();
        let cursor = unsafe { GetCursorPos(&mut c).is_ok() }.then_some(c);
        let mut h = Hold { hwnd, previous, cursor, down: false, lease, pointer:None };
        if manual_mode() {
            return Err("automação parada (painel ou F8)".into());
        }
        if !focus(hwnd) {
            return Err("não consegui trazer a janela do char para a frente".into());
        }
        clear_view(hwnd, &[(x, y), (515, 400)])?;
        let (nx, ny) = absolute(hwnd, x, y)?;
        let abs = MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        h.pointer=Some(PointerGuard::begin(hwnd,"caminhada segurada"));
        pin_relative(hwnd,x,y)?;
        send(&[mouse(nx, ny, MOUSEEVENTF_MOVE | abs)]);
        std::thread::sleep(Duration::from_millis(50));
        if unsafe { GetForegroundWindow() } != hwnd {
            return Err("a janela perdeu o foco antes de segurar".into());
        }
        h.lease.validate(true)?;
        if root_at(hwnd, x, y) != Some(hwnd) { return Err("ponto de caminhada coberto".into()); }
        if !send(&[point_button(nx,ny,MOUSEEVENTF_LEFTDOWN)]) {return Err("não consegui segurar o botão no jogo".into());}
        h.down = true;
        Ok(h)
    }

    /// Move o cursor com o botão segurado.
    pub fn steer(&mut self, x: i32, y: i32) -> Result<(), String> {
        if let Err(e) = self.lease.validate(false) {
            self.release();
            return Err(e);
        }
        if manual_mode() {
            self.release();
            return Err("automação parada (painel ou F8): botão solto".into());
        }
        if unsafe { GetForegroundWindow() } != self.hwnd {
            self.release();
            // Perder a frente no meio do caminho largava o char andando sem helper (dono, 11/11):
            // tenta trazer a janela de volta e segura de novo antes de desistir.
            if !regain(self.hwnd) {
                return Err("a janela perdeu o foco: botão solto".into());
            }
            let (nx, ny) = absolute(self.hwnd, x, y)?;
            let abs = MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
            self.pointer=Some(PointerGuard::begin(self.hwnd,"retomada da caminhada"));
            pin_relative(self.hwnd,x,y)?;
            send(&[mouse(nx, ny, MOUSEEVENTF_MOVE | abs)]);
            std::thread::sleep(Duration::from_millis(50));
            if unsafe { GetForegroundWindow() } != self.hwnd {
                return Err("a janela perdeu o foco: botão solto".into());
            }
            self.lease.validate(true)?;
            if !send(&[point_button(nx,ny,MOUSEEVENTF_LEFTDOWN)]) {self.release();return Err("não consegui retomar o botão no jogo".into());}
            self.down = true;
            return Ok(());
        }
        let (nx, ny) = absolute(self.hwnd, x, y)?;
        pin_relative(self.hwnd,x,y)?;
        send(&[mouse(nx, ny, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)]);
        Ok(())
    }

    pub fn release(&mut self) {
        if self.down {
            send(&[mouse(0, 0, MOUSEEVENTF_LEFTUP)]);
            self.down = false;
        }
        self.pointer.take();
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        self.release();
        if in_session(self.hwnd) {
            return;
        }
        restore_context(self.hwnd, self.previous, self.cursor);
    }
}

static EXPLICIT_ACTIVE: AtomicBool = AtomicBool::new(false);
thread_local! {
    static EXPLICIT_MOVE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Autoriza somente este trajeto, preservando as pausas globais. F8 cancela.
pub fn explicit_move<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(Option<u32>);
    impl Drop for Restore {
        fn drop(&mut self) {
            EXPLICIT_MOVE.with(|v| v.set(self.0));
            EXPLICIT_ACTIVE.store(self.0.is_some(), Ordering::SeqCst);
        }
    }
    EXPLICIT_ACTIVE.store(true, Ordering::SeqCst);
    let _restore = Restore(EXPLICIT_MOVE.with(|v| v.replace(Some(CANCEL.load(Ordering::SeqCst)))));
    f()
}

#[cfg(test)]
mod explicit_move_tests {
    use super::*;

    #[test]
    fn explicit_move_preserves_pause_and_accepts_new_stop() {
        set_paused(true);
        MANUAL.store(true, Ordering::SeqCst);
        assert!(manual_mode());
        explicit_move(|| {
            assert!(!manual_mode());
            explicit_move(|| assert!(!manual_mode()));
            assert!(!manual_mode());
            CANCEL.fetch_add(1, Ordering::SeqCst); // F8 durante o trajeto.
            assert!(manual_mode());
        });
        assert!(manual_mode());
        assert!(MANUAL.load(Ordering::SeqCst));
        assert!(PAUSED.load(Ordering::SeqCst));
        assert!(!EXPLICIT_ACTIVE.load(Ordering::SeqCst));
        MANUAL.store(false, Ordering::SeqCst);
        set_paused(false);
    }
}
