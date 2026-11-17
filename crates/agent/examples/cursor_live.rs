//! Probe somente leitura para validar o cursor do agente real: não move, clica ou troca foco.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow};
    use zen_agent::win::{capture, windows_list};
    let wins = windows_list::find_by_class("The Classic MU Season 2");
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p)?; }
    let windows: Vec<_> = wins.iter().map(|w| serde_json::json!({
        "hwnd": w.hwnd, "pid": w.pid, "name": zen_core::title::parse(&w.title).name,
        "origin": capture::frame_origin(w.hwnd)
    })).collect();
    println!("{}", serde_json::json!({"cursor":[p.x,p.y],
        "foreground":unsafe { GetForegroundWindow() }.0 as isize,"windows":windows}));
    Ok(())
}
#[cfg(not(windows))]
fn main() {}