//! Testa o clique com deslocamentos externos no x9901, apenas no inventário expandido.
#[cfg(windows)]
fn main()->anyhow::Result<()> {
    use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
    use zen_agent::win::{capture,input,nav,viewport,windows_list};
    use zen_core::actions::Step;
    tracing_subscriber::fmt::init();
    let w=windows_list::find_by_class("The Classic MU Season 2").into_iter()
        .find(|w|zen_core::title::parse(&w.title).name.as_deref()==Some("x9901"))
        .ok_or_else(||anyhow::anyhow!("x9901 ausente"))?;
    input::start_manual_guard(180);
    std::thread::sleep(std::time::Duration::from_millis(600));
    let _session=input::Session::begin(w.hwnd).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(nav::helper_state(w.hwnd)==Some(true),"Helper não está ativo");
    let was_open=nav::inventory_open(w.hwnd);
    let layout=viewport::current(w.hwnd).ok_or_else(||anyhow::anyhow!("sem viewport"))?;
    anyhow::ensure!(layout.home()==if was_open {(363,400)}else{(515,400)},"outro painel aberto antes do teste");
    if !was_open {input::run(w.hwnd,&[Step::Key(0x56)]).map_err(anyhow::Error::msg)?;}
    let clip=||->anyhow::Result<_>{
        let mut r=windows::Win32::Foundation::RECT::default();
        unsafe {windows::Win32::UI::WindowsAndMessaging::GetClipCursor(&mut r)?;}
        Ok((r.left,r.top,r.right,r.bottom))
    };
    let original_clip=clip()?;
    let outcome=(||->anyhow::Result<()> {
        for expected in [(211,400),(363,400)] {
            let (ox,oy)=capture::frame_origin(w.hwnd).ok_or_else(||anyhow::anyhow!("sem origem"))?;
            let stop=Arc::new(AtomicBool::new(false));
            let stop_thread=stop.clone();
            let direct=std::env::args().any(|arg|arg=="--set-cursor-pos");
            println!("METODO: {}",if direct {"SetCursorPos"}else{"SendInput"});
            let external=std::thread::spawn(move|| {
                use windows::Win32::UI::Input::KeyboardAndMouse::*;
                use windows::Win32::UI::WindowsAndMessaging::*;
                let (vx,vy,vw,vh)=unsafe {(GetSystemMetrics(SM_XVIRTUALSCREEN),GetSystemMetrics(SM_YVIRTUALSCREEN),GetSystemMetrics(SM_CXVIRTUALSCREEN),GetSystemMetrics(SM_CYVIRTUALSCREEN))};
                let nx=((ox+100-vx) as i64*65535/(vw-1) as i64) as i32;
                let ny=((oy+400-vy) as i64*65535/(vh-1) as i64) as i32;
                let event=INPUT{r#type:INPUT_MOUSE,Anonymous:INPUT_0{mi:MOUSEINPUT{dx:nx,dy:ny,dwFlags:MOUSEEVENTF_MOVE|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK,..Default::default()}}};
                let mut count=0;
                while !stop_thread.load(Ordering::SeqCst) {
                    let sent=if direct {unsafe {SetCursorPos(ox+100,oy+400)}.is_ok()}
                        else {(unsafe {SendInput(&[event],std::mem::size_of::<INPUT>() as i32)})==1};
                    if sent {count+=1;}
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
                count
            });
            let action=input::run(w.hwnd,&[Step::Click{x:944,y:674}]);
            stop.store(true,Ordering::SeqCst);
            let count=external.join().unwrap();
            action.map_err(anyhow::Error::msg)?;
            anyhow::ensure!(clip()?==original_clip,"limite do cursor não foi restaurado após clique");
            std::thread::sleep(std::time::Duration::from_millis(300));
            let actual=viewport::current(w.hwnd).map(|v|v.home());
            println!("INTERFERENCIA: tentativas={count} viewport={actual:?} esperado={expected:?}");
            anyhow::ensure!(count>0&&actual==Some(expected),"clique com interferência não confirmou o painel");
        }
        let cancel=std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(150));
            input::set_paused(true);
        });
        let paused=input::run(w.hwnd,&[Step::RightHold{x:944,y:674,ms:1000}]);
        cancel.join().unwrap();
        let released=clip()?==original_clip;
        input::set_paused(false);
        anyhow::ensure!(paused.is_err()&&released,"pausa não liberou proteção/ação");
        println!("PAUSA OK: cursor liberado e ação interrompida");
        Ok(())
    })();
    // Retoma o estado original, sem repetir conversões/saques ou enviar Home.
    if viewport::current(w.hwnd).map(|v|v.home())==Some((211,400)) {
        input::run(w.hwnd,&[Step::Click{x:944,y:674}]).map_err(anyhow::Error::msg)?;
    }
    if !was_open&&nav::inventory_open(w.hwnd) {input::run(w.hwnd,&[Step::Key(0x56)]).map_err(anyhow::Error::msg)?;}
    outcome?;
    anyhow::ensure!(nav::helper_state(w.hwnd)==Some(true),"Helper mudou no teste");
    println!("POINTER INTERFERENCE OK; inventário original restaurado, Helper ativo");
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
