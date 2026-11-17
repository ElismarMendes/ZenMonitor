//! Diagnóstico autorizado de painéis no x9901. Sem Home, teleporte ou caminhada.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use zen_agent::win::{capture, input, nav, windows_list};
    use zen_core::actions::Step;
    let w=windows_list::find_by_class("The Classic MU Season 2").into_iter()
        .find(|w|zen_core::title::parse(&w.title).name.as_deref()==Some("x9901"))
        .ok_or_else(||anyhow::anyhow!("x9901 ausente"))?;
    let validate=std::env::args().any(|arg|arg=="--validate");
    let dir=std::path::Path::new(if validate {"target/layout-x9901-fixed"}else{"target/layout-x9901"});
    std::fs::create_dir_all(dir)?;
    let _session=input::Session::begin(w.hwnd).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(!nav::inventory_open(w.hwnd),"feche o inventário antes do diagnóstico");
    let helper=nav::helper_state(w.hwnd);
    anyhow::ensure!(helper==Some(true),"Helper não confirmado ativo antes do teste");
    if std::env::args().any(|arg|arg=="--hold-focus") {
        anyhow::ensure!(zen_agent::win::viewport::current(w.hwnd).map(|v|v.home())==Some((515,400)),"painéis já abertos antes do teste");
        // Mantém apenas o escopo de foco enquanto o script testa o agente instalado via API.
        // Não envia novas teclas ou cliques; Drop devolve a aplicação anterior ao terminar.
        std::thread::sleep(std::time::Duration::from_secs(15));
        return Ok(());
    }
    let shot=|tag:&str,expected:(i32,i32)|->anyhow::Result<()> {
        std::thread::sleep(std::time::Duration::from_millis(600));
        let img=capture::capture(w.hwnd,None).ok_or_else(||anyhow::anyhow!("sem captura {tag}"))?;
        std::fs::write(dir.join(format!("{tag}.png")),img.to_png().ok_or_else(||anyhow::anyhow!("sem PNG"))?)?;
        let layout=zen_agent::win::viewport::detect(&img);
        let mut p=windows::Win32::Foundation::POINT::default();
        unsafe {windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p)?;}
        let (ox,oy)=capture::frame_origin(w.hwnd).ok_or_else(||anyhow::anyhow!("sem origem"))?;
        let actual=(p.x-ox,p.y-oy);
        println!("{tag}: mapa={:?} helper={:?} layout={layout:?} cursor={actual:?} esperado={expected:?}",nav::observe_named(w.hwnd),nav::helper_state(w.hwnd));
        if validate {
            anyhow::ensure!(layout.map(|v|v.home())==Some(expected),"viewport incorreto em {tag}");
            anyhow::ensure!((actual.0-expected.0).abs()<=4&&(actual.1-expected.1).abs()<=4,"cursor incorreto em {tag}");
            anyhow::ensure!(nav::helper_state(w.hwnd)==helper,"Helper mudou em {tag}");
        }
        Ok(())
    };
    let key=|vk|input::run(w.hwnd,&[Step::Key(vk)]).map_err(anyhow::Error::msg);
    shot("0-closed",(515,400))?;
    key(0x56)?; shot("1-inventory",(363,400))?;
    input::run(w.hwnd,&[Step::Click{x:944,y:674}]).map_err(anyhow::Error::msg)?;
    shot("2-expanded",(211,400))?;
    input::run(w.hwnd,&[Step::Click{x:944,y:674}]).map_err(anyhow::Error::msg)?;
    key(0x56)?;
    key(0x43)?; shot("3-character",(363,400))?;
    key(0x56)?; shot("4-character-inventory",(211,400))?;
    key(0x56)?;
    key(0x43)?; shot("5-restored",(515,400))?;
    anyhow::ensure!(!nav::inventory_open(w.hwnd)&&nav::helper_state(w.hwnd)==helper,"estado final divergente");
    println!("LAYOUT SNAPSHOT OK");
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
