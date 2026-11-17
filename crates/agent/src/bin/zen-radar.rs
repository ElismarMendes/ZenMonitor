//! Observador de validação local. Não registra hotkeys nem inicializa input/automações.
#[cfg(not(windows))]
fn main() {
    eprintln!("Observador disponível somente no Windows");
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use zen_agent::{
        config::AgentConfig,
        monitor::Monitor,
        net,
        win::{nav, objects, WinAccess},
        GameAccess,
    };
    use zen_core::protocol::{AgentMsg, Health};
    let cfg = AgentConfig::load_file()?;
    anyhow::ensure!(
        cfg.central_url == "ws://127.0.0.1:18787/ws/agent" && cfg.machine == "PC1" && !cfg.auto_token,
        "observador exige central isolado local :18787, PC1 e token manual"
    );
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let (out, rx) = tokio::sync::mpsc::channel(16);
    let (commands, mut received) = tokio::sync::mpsc::channel(16);
    rt.spawn(net::run(
        net::NetConfig {
            url: cfg.central_url,
            token: cfg.token,
            machine: cfg.machine,
            channel: "teste".into(),
            buffer: 0,
            auto_token: false,
            install_id: cfg.install_id,
            version: "radar-observer · somente leitura".into(),
        },
        rx,
        commands,
    ));
    let replies = out.clone();
    rt.spawn(async move {
        while let Some((id, _)) = received.recv().await {
            let _ = replies
                .send(AgentMsg::RouteResult {
                    id,
                    ok: false,
                    message: "observador de teste não executa comandos".into(),
                })
                .await;
        }
    });
    // Sem região OCR do Zen nem HP: não procura novas travas de atributos.
    let access = WinAccess::new(cfg.window_class, &[], false, vec!["x9901".into()]);
    let mut monitor = Monitor::new(false);
    let mut feed = zen_agent::navigation::Feed::default();
    let started = Instant::now();
    let mut seq = 0;
    while started.elapsed() < Duration::from_secs(900) {
        let mut chars = monitor.cycle(&access);
        for c in &mut chars {
            let Some(name) = c.title.name.as_deref() else { continue };
            let Some(w) = access.list_windows().into_iter().find(|w| w.pid == c.pid) else {
                continue;
            };
            let (map, pos) = nav::observe_named(w.hwnd);
            c.map = map.map(str::to_string);
            c.pos = pos;
            c.automation = Some(false);
            c.auto_status = "Observador experimental: somente leitura".into();
            if let Some(pos) = pos {
                c.nearby = objects::nearby_hwnd(w.hwnd, name, pos).map(|mut v| {
                    v.retain(|o| (o.x - pos.0).abs().max((o.y - pos.1).abs()) <= 25);
                    v.truncate(80);
                    v
                });
            }
            if let Some(snapshot) = feed.observe(c, access.process_start(c.pid).unwrap_or(0), seq, true) {
                let _ = out.blocking_send(AgentMsg::NavigationSnapshot { snapshot });
            }
        }
        let sent_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let _ = out.blocking_send(AgentMsg::Report {
            seq,
            sent_at,
            chars,
            events: vec![],
            health: Health {
                automation: false,
                ..Default::default()
            },
            catalog: vec![],
            catalog_other: vec![],
        });
        seq += 1;
        std::thread::sleep(Duration::from_secs(5));
    }
    Ok(())
}
