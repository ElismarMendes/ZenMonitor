//! zen-agent.exe — sobe pelo Agendador de Tarefas no logon, "com privilégios mais altos".
//! Não é serviço do Windows: serviço roda na sessão 0 e não enxerga a janela do jogo.
// Sem janela de console: os logs vão para logs\agent.*.log ao lado do .exe.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("zen-agent só roda no Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    app::main()
}

#[cfg(windows)]
mod app {
    use base64::Engine;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use tokio::sync::mpsc;
    use zen_agent::config::AgentConfig;
    use zen_agent::monitor::Monitor;
    use zen_agent::win::{chat::ChatReader, health, input, WinAccess};
    use zen_core::actions::ActionKind;
    use zen_agent::{net, GameAccess};
    use zen_core::protocol::{AgentMsg, Command, Health};

    const CHAT_EVERY: Duration = Duration::from_secs(60);

    /// Hora local aproximada (fuso fixo do Brasil, UTC-3) para mensagens curtas do painel.
    fn chrono_hhmm(ts: u64) -> String {
        let t = ts.saturating_sub(3 * 3600) % 86400;
        format!("{:02}:{:02}", t / 3600, (t % 3600) / 60)
    }

    fn spot_has_issue(o: &zen_core::recovery::Observation, spot: (i32, i32), c: &zen_core::recovery::RecoveryConfig) -> bool {
        zen_core::recovery::RecoveryMachine::situation(o, Some(spot), c).is_some()
    }

    fn recovery_can_take_priority(cfg: &AgentConfig, rec: &Recovery, name: &str) -> bool {
        cfg.recovery_enabled && !cfg.no_automation.iter().any(|n| n == name)
            && !rec.hold.contains_key(name) && !rec.pending.contains(name)
            && !rec.machines.get(name).is_some_and(|m| m.gave_up)
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn log_dir() -> std::path::PathBuf {
        AgentConfig::path().with_file_name("logs")
    }

    /// Vigia: sem `--worker`, este processo só sobe o trabalhador e o reinicia se ele cair.
    /// Não confiamos no "reiniciar se falhar" do Agendador (ele não dispara em todo tipo de saída).
    /// A fase 4 usa a contagem de quedas para o rollback automático (3 quedas em 10 min).
    pub fn main() -> anyhow::Result<()> {
        AgentConfig::load_env_file();
        let test_mode = std::env::args().any(|a| a == "--teste");
        if std::env::args().any(|a| a == "--worker") {
            return worker(test_mode);
        }
        // Uma cópia só por sessão. Sem isso, cada duplo-clique deixava mais um agente
        // rodando escondido, cada um com a configuração antiga.
        if already_running() {
            message(
                "O ZenMonitor Agent já está rodando neste PC.\n\n\
                 Para parar (por exemplo, depois de editar o agent.toml), rode no PowerShell:\n\
                 taskkill /IM zen-agent.exe /F",
            );
            return Ok(());
        }
        // Confere a configuração ANTES de subir, e avisa na tela: este .exe não tem console.
        let mut cfg = AgentConfig::load_or_create()?;
        if test_mode {
            cfg.apply_test_mode();
        }
        if let Err(e) = cfg.validate() {
            message(&format!(
                "Configuração inválida em\n{}\n\n{e}\n\nCorrija o arquivo e abra o zen-agent.exe de novo.",
                AgentConfig::path().display()
            ));
            return Ok(());
        }
        let exe = std::env::current_exe()?;
        let mut crashes: Vec<Instant> = Vec::new();
        loop {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("--worker");
            if test_mode {
                cmd.arg("--teste");
            }
            let status = cmd.status();
            crashes.retain(|t| t.elapsed() < Duration::from_secs(600));
            crashes.push(Instant::now());
            let wait = if crashes.len() >= 3 { 60 } else { 5 };
            let _ = std::fs::write(
                AgentConfig::path().with_file_name("vigia.txt"),
                format!("trabalhador saiu: {status:?}; quedas em 10 min: {}\n", crashes.len()),
            );
            std::thread::sleep(Duration::from_secs(wait));
        }
    }

    fn already_running() -> bool {
        use windows::core::w;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        // O handle fica aberto até o processo sair: é ele que marca "estou rodando".
        unsafe {
            match CreateMutexW(None, true, w!("Local\\ZenMonitorAgent")) {
                Ok(_handle) => GetLastError() == ERROR_ALREADY_EXISTS,
                Err(_) => false,
            }
        }
    }

    fn message(text: &str) {
        use windows::core::{w, PCWSTR};
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
        let t: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let _ = MessageBoxW(None, PCWSTR(t.as_ptr()), w!("ZenMonitor Agent"), MB_OK | MB_ICONWARNING);
        }
    }

    fn worker(test_mode: bool) -> anyhow::Result<()> {
        // Antes de tudo: versão nova em teste caindo demais = volta a anterior e sai.
        let (mut upd, upd_msg) = zen_agent::update::startup();
        let worker_started = Instant::now();
        let appender = tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("agent")
            .filename_suffix("log")
            .max_log_files(7)
            .build(log_dir())?;
        let (file, _guard) = tracing_appender::non_blocking(appender);
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with_writer(file)
            .with_ansi(false)
            .init();

        let mut cfg = AgentConfig::load_or_create()?;
        if test_mode {
            cfg.apply_test_mode();
            tracing::info!("MODO TESTE: só o char \"Teste\" do fake-client; o jogo real é ignorado");
        }
        let version = zen_agent::update::version_label(&upd);
        tracing::info!(versao = %version, maquina = %cfg.machine, "agente iniciando");
        // Trava manual (dono, 10/11 19:53): a automação não espera mais o "PC sem uso".
        if !test_mode {
            input::start_manual_guard(cfg.manual_release_s.max(30));
        }
        input::set_paused(!cfg.automation_enabled);
        if let Some(m) = &upd_msg {
            tracing::warn!("ATUALIZAÇÃO: {m}");
        }
        if let Ok(m) = std::fs::read_to_string(AgentConfig::dir().join("rollback.txt")) {
            tracing::error!("ATUALIZAÇÃO: {m}");
            let _ = std::fs::rename(AgentConfig::dir().join("rollback.txt"), AgentConfig::dir().join("rollback-visto.txt"));
        }
        let (upd_tx, upd_rx) = std::sync::mpsc::channel();
        match zen_agent::update::public_key() {
            Some(k) if !test_mode => zen_agent::update::spawn_checker(cfg.central_url.clone(), k, upd_tx),
            Some(_) => {}
            None => tracing::info!("atualização automática desligada: falta {} ao lado do agente", zen_agent::update::KEY_FILE),
        }
        let mut ready_update: Option<zen_agent::update::Ready> = None;
        tracing::info!(auto_token = cfg.auto_token, tem_token = !cfg.token.is_empty(), "token");

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let (out_tx, out_rx) = mpsc::channel::<AgentMsg>(64);
        zen_agent::spot_service::init(out_tx.clone())?;
        let (spot_changed_tx, spot_changed_rx) = std::sync::mpsc::channel::<()>();
        let (cmd_tx, mut cmd_raw_rx) = mpsc::channel::<(u64, Command)>(16);
        // "Salvar spot" pelo painel (12/11): a posição é lida NO INSTANTE em que o comando chega,
        // numa thread própria, e não quando o laço principal chega nele (até 5 s + o ciclo
        // depois: com o char andando, gravava outro tile).
        let (cmd_fwd_tx, cmd_rx) = std::sync::mpsc::channel::<(u64, Command, Option<Snap>)>();
        {
            let class = cfg.window_class.clone();
            let replies = out_tx.clone();
            let changed = spot_changed_tx.clone();
            std::thread::spawn(move || {
                while let Some((id, c)) = cmd_raw_rx.blocking_recv() {
                    if let Command::SaveSpot { char_name } = &c {
                        let started = Instant::now();
                        // Congela o quadro antes de esperar disco, monitoramento ou outros comandos.
                        let snap = zen_agent::win::windows_list::find_by_class(&class)
                            .into_iter()
                            .find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str()))
                            .map(|w| zen_agent::win::nav::observe_spot_now(w.hwnd))
                            .unwrap_or((None, None));
                        let capture_ms = started.elapsed().as_millis();
                        let _edit = SPOT_EDIT.lock().unwrap_or_else(|e| e.into_inner());
                        let reply = match AgentConfig::load_file() {
                            Ok(mut local) => {
                                let reply = save_spot_snapshot(id, char_name.clone(), Some(snap), &mut local);
                                let _ = changed.send(());
                                reply
                            }
                            Err(e) => vec![AgentMsg::CommandResult { id, ok: false, message: format!("falha ao carregar configuração: {e}") }],
                        };
                        tracing::info!(capture_ms, total_ms = started.elapsed().as_millis(), "salvar spot: painel");
                        drop(_edit);
                        for m in reply { let _ = replies.blocking_send(m); }
                        continue;
                    }
                    let snap = None;
                    if cmd_fwd_tx.send((id, c, snap)).is_err() {
                        return;
                    }
                }
            });
        }

        rt.spawn(net::run(
            net::NetConfig {
                url: cfg.central_url.clone(),
                token: cfg.token.clone(),
                machine: cfg.machine.clone(),
                channel: cfg.channel.clone(),
                buffer: cfg.buffer_reports,
                auto_token: cfg.auto_token,
                install_id: cfg.install_id.clone(),
                version: version.clone(),
            },
            out_rx,
            cmd_tx,
        ));

        // Ciclo de leitura numa thread normal: as chamadas Win32 são bloqueantes.
        let access = WinAccess::new(
            cfg.window_class.clone(),
            &cfg.zen_ocr_region,
            cfg.allow_loopback_tcp,
            cfg.allowed_chars.clone(),
        )
        .with_hp_region(&cfg.hp_region);
        if cfg.hp_region.is_empty() {
            tracing::info!("hp_region vazia: vida e detecção de morte DESLIGADAS");
        }
        if !cfg.allowed_chars.is_empty() {
            tracing::info!(chars = ?cfg.allowed_chars, "só estes chars serão lidos");
        }
        let mut monitor = Monitor::new(cfg.ram_paused);
        let locks_file = AgentConfig::dir().join("travas.json");
        if let Some(saved) = std::fs::read_to_string(&locks_file)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<zen_agent::monitor::SavedLock>>(&t).ok())
        {
            tracing::info!(quantas = saved.len(), "travas salvas encontradas (só voltam se for o mesmo processo do jogo)");
            monitor.restore_from(saved);
        }
        let mut last_locks = String::new();
        let mut chat = ChatReader::default();
        let mut last_chat = Instant::now() - CHAT_EVERY;
        let mut cpu = health::CpuMeter::new();
        let started = Instant::now();
        let mut seq = 0u64;
        let mut last_error: Option<String> = None;
        let mut untitled: std::collections::HashMap<u32, String> = Default::default();
        // Vida do Dark Raven (barra no topo da tela) por char: (valor, quando leu). Lida a cada
        // 30 s: a vida do pet cai devagar e cada print custa CPU.
        let mut pet: std::collections::HashMap<String, (Option<u8>, u64)> = Default::default();

        // Spots de chars gravados antes do catálogo existir entram no catálogo (uma vez).
        {
        let _edit = SPOT_EDIT.lock().unwrap_or_else(|e| e.into_inner());
        sync_saved_spots(&mut cfg, &spot_changed_rx);
        let missing: Vec<[i32; 2]> = cfg.spots.iter().filter(|(n, _)| home_dest(&cfg, n) == zen_agent::win::nav::Dest::Kanturu).map(|(_, s)| *s).filter(|s| !cfg.catalog.iter().any(|c| same_spot(*c, *s))).collect();
        if !missing.is_empty() {
            for s in missing {
                catalog_add(&mut cfg, s);
            }
            if let Err(e) = save_spots(&cfg) {
                tracing::warn!(erro = %e, "não gravei o catálogo inicial");
            }
        }
        }
        let mut rec = Recovery::new();
        let mut ks: std::collections::HashMap<String,(zen_core::spots::Spot,zen_core::ks::Analysis)> = Default::default();
        // Bloqueios "não volta sozinho" (PK / causa desconhecida) sobrevivem a reinício do agente.
        let holds_file = AgentConfig::dir().join("bloqueios.json");
        if let Some(h) = std::fs::read_to_string(&holds_file)
            .ok()
            .and_then(|t| serde_json::from_str::<std::collections::HashMap<String, String>>(&t).ok())
        {
            tracing::warn!(chars = ?h.keys().collect::<Vec<_>>(), "bloqueios de volta automática carregados");
            rec.hold = h;
        }
        let mut last_holds = serde_json::to_string(&rec.hold).unwrap_or_default();
        // O gancho congela os pixels no evento. Decodificação e gravação não esperam o monitor.
        let (hk_raw_tx, hk_raw_rx) = std::sync::mpsc::channel::<(zen_agent::win::hotkey::Hotkey, isize, Option<zen_agent::image::Image>)>();
        zen_agent::win::hotkey::start(&cfg.window_class, hk_raw_tx);
        {
            let class = cfg.window_class.clone();
            let replies = out_tx.clone();
            let changed = spot_changed_tx.clone();
            std::thread::spawn(move || {
                for (h, hwnd, captured) in hk_raw_rx {
                    let started = Instant::now();
                    let snap = zen_agent::win::nav::decode_spot_frame(captured);
                    let _edit = SPOT_EDIT.lock().unwrap_or_else(|e| e.into_inner());
                    let m = match AgentConfig::load_file() {
                        Ok(mut local) => {
                            let access = WinAccess::new(class.clone(), &local.zen_ocr_region, local.allow_loopback_tcp, local.allowed_chars.clone());
                            let m = handle_hotkey(h, hwnd, snap, &mut local, &access);
                            let _ = changed.send(());
                            m
                        }
                        Err(e) => format!("atalho falhou ao carregar configuração: {e}"),
                    };
                    tracing::info!(atalho = ?h, total_ms = started.elapsed().as_millis(), resultado = %m, "salvar spot: atalho");
                    drop(_edit);
                    let ok = spot_result_ok(&m);
                    let _ = replies.blocking_send(AgentMsg::CommandResult { id: 0, ok, message: m });
                }
            });
        }
        let mut rec_cfg: zen_core::recovery::RecoveryConfig;
        // Pick Window (Y): chars que saíram do mapa do spot e precisam ligar o Zen de novo.
        // Começa vazio: sem saber o estado (o botão alterna e não mostra), não clica.
        let mut pick_needed: std::collections::HashSet<String> = Default::default();
        let mut pick_retry: std::collections::HashMap<String, (u64, u32)> = Default::default();
        let mut next_cycle = Instant::now();
        loop {
            let tick = Instant::now();
            zen_agent::spot_service::heartbeat();
            {
                let _edit = SPOT_EDIT.lock().unwrap_or_else(|e| e.into_inner());
                sync_saved_spots(&mut cfg, &spot_changed_rx);
            }
            for name in zen_agent::spot_service::take_assignments() {
                // F1 é uma intenção nova: remove inclusive o hold criado por "Ir até aqui".
                rec.hold.remove(&name);rec.pending.remove(&name);rec.machines.remove(&name);rec.return_owned.remove(&name);
                rec.set_status(&name,"novo spot principal salvo; retorno automático habilitado".into());
            }
            // Comandos do painel a cada passo de 0,5 s (antes: só a cada ciclo de 5 s).
            while let Ok((id, c, snap)) = cmd_rx.try_recv() {
                // Pedido automático de logs (a cada 2 min) não polui o próprio log.
                if !matches!(c, Command::SendLogs) {
                    tracing::info!(id, comando = ?c, "comando recebido");
                }
                let reply = {
                    // Só operações que editam spots precisam serializar com o salvamento.
                    let edits_spots = matches!(&c, Command::GoTo { .. } | Command::AssignSpot { .. } | Command::DeleteSpot { .. } | Command::SetSpotLock { .. } | Command::SaveSpot { .. });
                    let edit = SPOT_EDIT.lock().unwrap_or_else(|e| e.into_inner());
                    sync_saved_spots(&mut cfg, &spot_changed_rx);
                    let _edit = if edits_spots { Some(edit) } else { drop(edit); None };
                    handle_command(id, c, snap, &mut cfg, &mut monitor, &access, &mut rec)
                };
                for m in reply {
                    let _ = out_tx.blocking_send(m);
                }
            }
            // Vida: leitura rápida entre um relatório e outro. Morte = print + aviso imediato.
            for d in monitor.fast_tick(&access) {
                let pid = d.pid;
                if let Some(m) = confirm_death(&access, &mut monitor, d) {
                    if let AgentMsg::Death { char_name, .. } = &m {
                        // Spot "seu": estava farmando nele havia 1 min ou mais. Aí volta o mais
                        // rápido possível, seja PK ou monstro. Chegou há pouco = pode ter entrado no
                        // spot de outro (ou voltou e morreu de novo): espera o chat dizer quem matou.
                        let farmed = rec.farm_since.get(char_name).map(|t| now().saturating_sub(*t));
                        let allowed = !cfg.no_return_on_death.iter().any(|n| n == char_name);
                        // Regra do dono (10/11 09:53): morreu, SEMPRE volta (com a espera sorteada).
                        // Não importa há quanto tempo estava no spot nem quem matou. O limite de
                        // voltas por janela do painel (Máquinas) continua valendo contra laço.
                        let owned = allowed;
                        if owned && cfg.spots.contains_key(char_name) {rec.return_owned.insert(char_name.clone());}
                        rec.death_farmed.insert(char_name.clone(), farmed);
                        if !allowed {
                            let h = format!("morreu às {}: volta automática desligada para este char (painel)", chrono_hhmm(now()));
                            rec.set_status(char_name, h.clone());
                            rec.hold.insert(char_name.clone(), h);
                        }
                        if owned {
                            let wait = random_between(cfg.return_delay_min_s, cfg.return_delay_max_s);
                            rec.machines.entry(char_name.clone()).or_default().rush_wait = Some(wait);
                            rec.hold.remove(char_name);
                            rec.set_status(char_name, format!("morreu: volta ao spot em {}", fmt_wait(wait)));
                            tracing::warn!(char = %char_name, farmou_min = farmed.unwrap_or(0) / 60, espera_s = wait, "morte: volta automática sorteada (sempre volta)");
                        }
                        if !monitor.ram_paused {
                            rec.scan_death_cause(char_name, pid, allowed && !owned);
                        } else if allowed && !owned {
                            rec.hold.insert(char_name.clone(), "morreu (leitura da RAM pausada): não volta sozinho".into());
                        }
                    }
                    let _ = out_tx.try_send(m);
                }
            }
            if Instant::now() < next_cycle {
                let spent = tick.elapsed();
                std::thread::sleep(Duration::from_millis(zen_agent::monitor::HP_TICK_MS).saturating_sub(spent));
                continue;
            }
            let t0 = Instant::now();
            next_cycle = t0 + Duration::from_millis(cfg.cycle_ms);
            rec_cfg = rec_cfg_of(&cfg);

            // ---- atualização automática
            if upd.pending.is_some() {
                if net::CONNECTED.load(std::sync::atomic::Ordering::SeqCst) {
                    if let Some(seq) = zen_agent::update::confirm(&mut upd) {
                        let m = format!("agente atualizado para a publicação {seq} ({version})");
                        tracing::warn!("ATUALIZAÇÃO: {m}");
                        let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok: true, message: m });
                    }
                } else if worker_started.elapsed().as_secs() > zen_core::update::TRIAL_S {
                    let m = zen_agent::update::do_rollback(&mut upd);
                    tracing::error!("ATUALIZAÇÃO: {m} (não conectou ao central em 10 min)");
                    let _ = std::fs::write(AgentConfig::dir().join("rollback.txt"), &m);
                    std::process::exit(3);
                }
            }
            if let Ok(r) = upd_rx.try_recv() {
                ready_update = Some(r);
            }
            if let Some(r) = ready_update.as_ref() {
                // Nunca no meio de uma recuperação/depósito.
                if !rec.is_busy() {
                    match zen_agent::update::apply(r) {
                        Ok(()) => {
                            tracing::warn!(publicacao = r.seq, "ATUALIZAÇÃO: exe trocado; reiniciando o trabalhador");
                            std::thread::sleep(Duration::from_millis(500));
                            std::process::exit(0);
                        }
                        Err(e) => {
                            tracing::error!("ATUALIZAÇÃO: troca falhou: {e}");
                            ready_update = None;
                        }
                    }
                }
            }

            let mut chars = monitor.cycle(&access);
            // Recuperações que terminaram.
            while let Ok((name, r)) = rec.done_rx.try_recv() {
                if let Some(delay) = raven_cooldown(&r) { rec.raven_block.insert(name.clone(), now() + delay); }
                let ok = r.is_ok();
                if ok {rec.return_owned.remove(&name);}
                // Prints do treino K -> J (só existem depois de "Fortuna: guardar K no J").
                for (tag, png) in zen_agent::win::nav::take_trace() {
                    let _ = out_tx.try_send(AgentMsg::Screenshot {
                        id: 0,
                        char_name: format!("{name}-passo-{tag}"),
                        png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                    });
                }
                match &r {
                    Ok(m) => tracing::warn!(char = %name, resultado = %m, "AUTOMAÇÃO: recuperação concluída"),
                    Err(e) => tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: recuperação falhou"),
                }
                let hhmm = chrono_hhmm(now());
                // v4: "ir até" do painel não conta como volta ao spot nem bloqueia a Fortuna.
                if matches!(&r, Ok(m) | Err(m) if m.starts_with("ir até") || m.starts_with("Elpis")) {
                    let m = match &r {
                        Ok(m) | Err(m) => format!("{name}: {m}"),
                    };
                    let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok, message: m });
                    rec.set_status(&name, match r {
                        Ok(m) => format!("{m} ({hhmm})"),
                        Err(e) => format!("{e} ({hhmm})"),
                    });
                    continue;
                }
                // Dark Raven: resultado vai para o painel (Resultados), com o print se falhou.
                if matches!(&r, Ok(m) | Err(m) if m.starts_with("Raven")) {
                    if !ok {
                        if let Some(png) = zen_agent::win::nav::take_error_shot() {
                            let _ = out_tx.try_send(AgentMsg::Screenshot {
                                id: 0,
                                char_name: format!("{name}-raven-erro"),
                                png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                            });
                        }
                    }
                    let m = match &r {
                        Ok(m) | Err(m) => format!("{name}: {m}"),
                    };
                    let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok, message: m });
                    rec.set_status(&name, match r {
                        Ok(m) => format!("{m} ({hhmm})"),
                        Err(e) => format!("{e} ({hhmm})"),
                    });
                    continue;
                }
                let is_deposit = match &r {
                    Ok(m) | Err(m) => m.starts_with("depósito"),
                };
                if is_deposit {
                    // Depósito: falhou = não tenta de novo sozinho por 15 min (você vê o motivo no painel e no celular).
                    if ok {
                        rec.deposit_block.remove(&name);
                        rec.deposit_wait_reason.remove(&name);
                    } else {
                        rec.deposit_block.insert(name.clone(), now() + 900);
                        // Print do erro vai para o central: /api/screenshot/<máquina>/<char>-bau-erro
                        if let Some(png) = zen_agent::win::nav::take_error_shot() {
                            let _ = out_tx.try_send(AgentMsg::Screenshot {
                                id: 0,
                                char_name: format!("{name}-bau-erro"),
                                png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                            });
                        }
                    }
                    let m = match &r {
                        Ok(m) | Err(m) => format!("{name}: {m}"),
                    };
                    // Deu certo mas a volta falhou: o print do erro também vai para o central.
                    if ok && m.contains("falhou") {
                        if let Some(png) = zen_agent::win::nav::take_error_shot() {
                            let _ = out_tx.try_send(AgentMsg::Screenshot {
                                id: 0,
                                char_name: format!("{name}-erro"),
                                png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                            });
                        }
                    }
                    let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok, message: m });
                    rec.set_status(&name, match r {
                        Ok(m) => format!("{m} ({hhmm})"),
                        Err(e) => format!("{e} ({hhmm}) — tenta de novo em 15 min"),
                    });
                    continue;
                }
                if let Some(m) = rec.machines.get_mut(&name) {
                    m.finished(now(), ok, &rec_cfg);
                }
                if !ok {
                    if let Some(png) = zen_agent::win::nav::take_error_shot() {
                        let _ = out_tx.try_send(AgentMsg::Screenshot {
                            id: 0,
                            char_name: format!("{name}-erro"),
                            png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                        });
                    }
                }
                rec.set_status(&name, match r {
                    Ok(_) => format!("voltou ao spot às {hhmm}"),
                    Err(e) => format!("falhou às {hhmm}: {e}"),
                });
            }
            // Quem matou (linhas [PK] do chat lidas logo depois da morte).
            while let Ok((name, lines, cause)) = rec.cause_rx.try_recv() {
                rec.pending.remove(&name);
                rec.known.entry(name.clone()).or_default().extend(lines);
                let hhmm = chrono_hhmm(now());
                use zen_core::chat::KillerKind;
                let recent = {
                    let d = rec.monster_deaths.entry(name.clone()).or_default();
                    if matches!(cause, Some((_, KillerKind::Monster))) {
                        d.push(now());
                    }
                    d.retain(|t| now().saturating_sub(*t) < 1800);
                    d.len()
                };
                let farmed = rec.death_farmed.remove(&name).flatten();
                let stay = match farmed {
                    Some(f) => format!("estava no spot havia {} min", f / 60),
                    None => "sem confirmação contínua de farm antes da morte".to_string(),
                };
                let allowed = !cfg.no_return_on_death.iter().any(|n| *n == name);
                let (msg, hold) = match &cause {
                    _ if !allowed => (
                        format!("{name}: morte às {hhmm} — volta automática desligada para este char: espera você"),
                        Some(format!("morreu às {hhmm}: volta automática desligada (painel)")),
                    ),
                    _ => (
                        format!(
                            "{name}: {} às {hhmm} — {stay}: volta ao spot{}",
                            match &cause {
                                Some((k, KillerKind::Player)) => format!("PK de {k}"),
                                Some((k, KillerKind::Monster)) => format!("morto pelo monstro {k}"),
                                None => "morte (quem matou não aparece no chat)".into(),
                            },
                            if recent >= 3 { format!(" ({recent} mortes para monstros em 30 min: spot pesado?)") } else { String::new() }
                        ),
                        None,
                    ),
                };
                tracing::warn!(char = %name, causa = ?cause, "{msg}");
                match hold {
                    Some(h) => {
                        rec.set_status(&name, h.clone());
                        rec.hold.insert(name.clone(), h);
                    }
                    None => {
                        rec.hold.remove(&name);
                    }
                }
                let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok: cause.as_ref().is_some_and(|c| c.1 == KillerKind::Monster), message: msg });
            }
            // O que cada char mostra na tela agora (mapa, tile, helper): mini-mapa e recuperação.
            let mut seen: std::collections::HashMap<String, (isize, zen_core::recovery::Observation)> = Default::default();
            let mut map_names: std::collections::HashMap<String, (Option<&'static str>, Option<(i32, i32)>)> = Default::default();
            for c in &chars {
                let Some(name) = c.title.name.clone() else { continue };
                let Some(hwnd) = monitor.hwnd_of(&access, &name) else { continue };
                // "in_kanturu" = está no mapa do SPOT (Kanturu, ou Arena para quem tem spot lá).
                // v4: lê o nome do mapa uma vez e serve aos dois (painel e recuperação).
                let (mname, pos) = zen_agent::win::nav::observe_named(hwnd);
                let want = spot_dest(&cfg, &name).name();
                let k = pos.map(|_| mname == Some(want));
                map_names.insert(name.clone(), (mname, pos));
                let obs = zen_core::recovery::Observation {
                    in_kanturu: k,
                    pos,
                    helper_on: zen_agent::win::nav::helper_state(hwnd),
                    disconnected: c.disconnected,
                };
                seen.insert(name, (hwnd, obs));
            }
            // Saiu do mapa do spot (cidade/teleporte/morte): o jogo desligou o pick do Zen.
            for (name, (mname, pos)) in &map_names {
                if zen_agent::win::nav::pick_left_spot_map(*mname, *pos, spot_dest(&cfg, name).name()) && pick_needed.insert(name.clone()) {
                    pick_retry.remove(name);
                    tracing::info!(char = %name, mapa = ?mname, "PICK: saiu do mapa do spot; o Zen será ligado de novo quando voltar a farmar");
                }
            }
            // Há quanto tempo cada char farma no spot (para saber se o spot já é dele).
            for (name, (_, o)) in &seen {
                let Some(spot) = effective_spot(&cfg,name) else {
                    rec.farm_since.remove(name);
                    continue;
                };
                let near = o.pos.is_some_and(|p| (p.0 - spot[0]).abs().max((p.1 - spot[1]).abs()) <= rec_cfg.far_tiles);
                if o.in_kanturu == Some(true) && o.helper_on == Some(true) && near {
                    rec.farm_since.entry(name.clone()).or_insert(now());
                } else if o.in_kanturu == Some(false) || o.helper_on == Some(false) || (o.pos.is_some() && !near) {
                    rec.farm_since.remove(name);
                }
            }
            // Depósito no baú: Zen chegou ao máximo. Antes da recuperação, uma coisa por vez.
            if cfg.automation_enabled && cfg.auto_deposit && !rec.is_busy() && !monitor.ram_paused && !input::manual_mode() {
                for c in &chars {
                    let Some(name) = c.title.name.clone() else { continue };
                    if cfg.no_automation.iter().any(|n| *n == name) {
                        continue;
                    }
                    let Some(spot) = effective_spot(&cfg,&name) else { continue };
                    if !c.zen.is_some_and(|z| z >= zen_core::vault::DEPOSIT_AT) || c.disconnected {
                        continue;
                    }
                    if cfg.no_auto_deposit.iter().any(|n| *n == name) {
                        continue;
                    }
                    if let Some(until) = rec.deposit_block.get(&name).copied().filter(|t| now() < *t) {
                        rec.deposit_wait(&name, format!("falha anterior: nova tentativa às {}", chrono_hhmm(until)));
                        continue;
                    }
                    if let Some(reason) = rec.hold.get(&name) {
                        rec.deposit_wait(&name, format!("bloqueio deste char: {reason}"));
                        continue;
                    }
                    let Some((hwnd, _)) = seen.get(&name).copied() else { continue };
                    // Outro char deste PC com problema (helper desligado, fora do spot): ele
                    // primeiro. Durante a Fortuna (2-3 min) ninguém mais é atendido (11/11 08:43:
                    // o ReDMooN morreu enquanto o x9901 fazia a Fortuna).
                    let priority = seen.iter().find(|(n, (_, o))| {
                        n.as_str() != name && recovery_can_take_priority(&cfg, &rec, n)
                            && effective_spot(&cfg,n).is_some_and(|s| spot_has_issue(o, (s[0], s[1]), &rec_cfg))
                    }).map(|(n,_)|n.clone());
                    if let Some(other) = priority {
                        rec.deposit_wait(&name, format!("{other}: recuperação tem prioridade"));
                        continue;
                    }
                    rec.deposit_wait_reason.remove(&name);
                    rec.deposit_block.remove(&name);
                    // Spot num mapa sem teleporte calibrado (Lost Tower, Dungeon): não sai de lá.
                    if !spot_dest(&cfg, &name).can_teleport() {
                        continue;
                    }
                    match rec.start_deposit(&name, c.pid, hwnd, (spot[0], spot[1]), spot_dest(&cfg, &name), false) {
                        Ok(()) => tracing::warn!(char = %name, zen = ?c.zen, "AUTOMAÇÃO: depósito no baú iniciado"),
                        Err(e) => tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: depósito não iniciou"),
                    }
                    break;
                }
            }
            // Dark Raven com pouca vida (11/11): Trainer de Lorencia e volta ao spot. Um por vez,
            // no máximo uma ida a cada 30 min por char (com ou sem sucesso), só no mapa do spot.
            if cfg.automation_enabled && cfg.raven_auto && !rec.is_busy() && !monitor.ram_paused && !input::manual_mode() {
                for c in &chars {
                    let Some(name) = c.title.name.clone() else { continue };
                    if cfg.no_automation.iter().any(|n| *n == name) || c.disconnected {
                        continue;
                    }
                    let Some(v) = pet.get(&name).and_then(|e| e.0) else { continue };
                    if v > cfg.raven_min_pct {
                        rec.raven_low.remove(&name);
                        continue;
                    }
                    // Duas leituras baixas seguidas (30 s entre elas) antes de sair do spot.
                    let read_at = pet.get(&name).map_or(0, |e| e.1);
                    let first = *rec.raven_low.entry(name.clone()).or_insert(read_at);
                    if read_at <= first || rec.raven_block.get(&name).is_some_and(|t| now() < *t) || rec.hold.contains_key(&name) {
                        continue;
                    }
                    let (Some(spot), Some((hwnd, obs))) = (effective_spot(&cfg,&name), seen.get(&name).copied()) else { continue };
                    if obs.in_kanturu != Some(true) {
                        continue; // fora do mapa do spot: a volta normal cuida primeiro
                    }
                    rec.raven_block.insert(name.clone(), now() + 60);
                    match rec.start_raven(&name, c.pid, hwnd, (spot[0], spot[1]), spot_dest(&cfg, &name), false) {
                        Ok(()) => rec.set_status(&name, format!("Dark Raven em {v}%: indo ao Trainer de Lorencia")),
                        Err(e) => rec.set_status(&name, format!("Dark Raven em {v}%, mas não saiu: {e}")),
                    }
                    break;
                }
            }
            // Automação do PC religada (botão do painel, 11/11): confere cada char e devolve ao spot.
            if cfg.automation_enabled && !rec.is_busy() && !monitor.ram_paused && !input::manual_mode() && !rec.relaunch.is_empty() {
                let name = rec.relaunch.remove(0);
                let c = chars.iter().find(|c| c.title.name.as_deref() == Some(name.as_str()));
                match (c, effective_spot(&cfg,&name), seen.get(&name).copied()) {
                    (Some(c), Some(spot), Some((hwnd, obs))) if !cfg.no_automation.iter().any(|n| *n == name) && !c.disconnected => {
                        let near = obs.pos.is_some_and(|p| (p.0 - spot[0]).abs().max((p.1 - spot[1]).abs()) <= zen_core::spots::ARRIVAL_RADIUS);
                        if obs.in_kanturu == Some(true) && near && obs.helper_on == Some(true) {
                            rec.set_status(&name, "automação religada: já está no spot com o helper ligado".into());
                        } else if let Err(e) = rec.start(&name, c.pid, hwnd, (spot[0], spot[1]), spot_dest(&cfg, &name), "automação religada pelo painel") {
                            rec.set_status(&name, format!("automação religada, mas a volta não iniciou: {e}"));
                        }
                    }
                    _ => {}
                }
            }
            // Um mapa secundário é temporário. Só a agenda, não KS, inicia a volta ao principal.
            if cfg.automation_enabled && cfg.recovery_enabled && cfg.spot_fallback && !rec.is_busy()
                && !monitor.ram_paused && !input::manual_mode() {
                for c in &chars {
                    let Some(name)=c.title.name.as_deref() else {continue};
                    if c.disconnected || c.hp==Some(0) || rec.pending.contains(name) || rec.return_owned.contains(name)
                        || rec.machines.get(name).is_some_and(|m|m.rush_wait.is_some()) {continue}
                    if cfg.no_automation.iter().any(|n|n==name) || rec.hold.contains_key(name)
                        || cfg.locked_spots.iter().any(|n|n==name) {continue}
                    let Some(mut active)=zen_agent::spot_service::active(name) else {continue};
                    let Some(home)=zen_agent::spot_service::home(&cfg,name) else {continue};
                    if !active.due(home,now()) {continue}
                    let Some((hwnd,_))=seen.get(name) else {continue};
                    let Some(dest)=zen_agent::win::nav::Dest::from_world(home.world) else {continue};
                    active.retry_at=now()+cfg.spot_primary_retry_s.clamp(60,86400);
                    if zen_agent::spot_service::set_active(name,active).is_err(){continue}
                    if let Err(e)=rec.start(name,c.pid,*hwnd,(home.x,home.y),dest,"reavaliando catálogo do mapa principal") {
                        rec.set_status(name,e);
                    }
                    break;
                }
            }
            // Recuperação automática: DC (fora do Kanturu) ou helper desligado. Só com a automação
            // ligada, spot salvo, PC sem uso e nenhuma outra recuperação rodando.
            if cfg.automation_enabled && cfg.recovery_enabled && !rec.is_busy() && !monitor.ram_paused {
                let manual = input::manual_mode();
                for c in &chars {
                    let Some(name) = c.title.name.clone() else { continue };
                    if cfg.no_automation.iter().any(|n| *n == name) {
                        continue;
                    }
                    let Some(spot) = effective_spot(&cfg,&name) else { continue };
                    let Some((hwnd, obs)) = seen.get(&name).copied() else { continue };
                    // Morte por PK (ou sem causa lida): espera você. Libera sozinho quando o char
                    // volta a farmar perto do spot (você o levou) ou pelo botão Voltar do painel.
                    if rec.pending.contains(&name) {
                        rec.set_status(&name, "morreu: lendo no chat quem matou".into());
                        continue;
                    }
                    if let Some(h) = rec.hold.get(&name).cloned() {
                        // "Ir até" pelo painel: só "Voltar ao spot" (ou morrer) solta. O helper
                        // pode arrastar o char para perto do spot, e isso não é "voltou a farmar
                        // no spot" (visto no teste de 10/11 19:08).
                        if h.starts_with("foi até") {
                            rec.set_status(&name, h);
                            continue;
                        }
                        if obs.in_kanturu == Some(true) && obs.helper_on == Some(true) && !spot_has_issue(&obs, (spot[0], spot[1]), &rec_cfg) {
                            rec.hold.remove(&name);
                            rec.monster_deaths.remove(&name);
                            rec.set_status(&name, "farmando de novo no spot".into());
                        } else {
                            rec.set_status(&name, h);
                        }
                        continue;
                    }
                    if manual {
                        // Você pausou com F8: não conta tentativa nem aperta nada.
                        if spot_has_issue(&obs, (spot[0], spot[1]), &rec_cfg) {
                            rec.set_status(&name, "fora do spot; CONTROLE MANUAL (F8 para retomar, ou 3 min sem mexer)".into());
                        }
                        continue;
                    }
                    let (decision, gave_up) = {
                        let m = rec.machines.entry(name.clone()).or_default();
                        let d = m.decide(now(), &obs, Some((spot[0], spot[1])), &rec_cfg);
                        (d, m.gave_up)
                    };
                    if let Some(trig) = decision {
                        let dest = spot_dest(&cfg, &name);
                        if !dest.can_teleport() && obs.in_kanturu == Some(false) {
                            // Fora do mapa do spot e sem botão do M calibrado: não tenta (só avisa).
                            rec.set_status(&name, format!("fora da {}: o teleporte para lá ainda não é automático; leve o char até o mapa e ele anda até o spot", dest.name()));
                            if let Some(m) = rec.machines.get_mut(&name) {
                                m.finished(now(), false, &rec_cfg);
                            }
                            continue;
                        }
                        if let Err(e) = rec.start(&name, c.pid, hwnd, (spot[0], spot[1]), dest, trig.label()) {
                            tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: recuperação não iniciou");
                            if let Some(m) = rec.machines.get_mut(&name) {
                                m.finished(now(), false, &rec_cfg);
                            }
                            rec.set_status(&name, format!("não iniciou: {e}"));
                        }
                        break; // uma por vez
                    } else if gave_up {
                        rec.set_status(&name, format!("desistiu: {} voltas nos últimos {} min (painel: Máquinas)", cfg.return_max, cfg.return_window_min));
                    }
                }
            }
            // Teclas do Ice Storm achadas pelo ícone durante a caminhada: grava no agent.toml.
            for (h, k) in zen_agent::win::nav::take_learned_keys() {
                if let Some(name) = seen.iter().find(|(_, (hw, _))| *hw == h).map(|(n, _)| n.clone()) {
                    if k > 0 && cfg.skill_keys.get(&name) != Some(&k) {
                        cfg.skill_keys.insert(name.clone(), k);
                        let map = cfg.skill_keys.clone();
                        tracing::warn!(char = %name, tecla = k, "tecla do Ice Storm aprendida e gravada");
                        if let Err(e) = save_field(move |c| c.skill_keys = map) {
                            tracing::warn!(erro = %e, "não gravei a tecla do Ice Storm");
                        }
                    }
                }
            }
            for c in chars.iter_mut() {
                let Some(name) = c.title.name.as_deref() else { continue };
                if let Some((_, o)) = seen.get(name) {
                    c.pos = if o.in_kanturu == Some(true) { o.pos } else { None };
                }
                // v4: mapa atual (qualquer mapa conhecido) e mapa do spot, para o painel.
                if let Some((mn, p)) = map_names.get(name) {
                    c.map = mn.map(String::from);
                    if mn.is_some() {
                        c.pos = *p;
                    }
                }
                c.home_spot = zen_agent::spot_service::home(&cfg,name);
                c.primary_retry_at = zen_agent::spot_service::active(name).filter(|a| a.spot.world != a.home.world && a.retry_at > 0).map(|a| a.retry_at);
                c.spot = effective_spot(&cfg,name).map(|s| (s[0], s[1]));
                c.return_on_death = Some(!cfg.no_return_on_death.iter().any(|n| n == name));
                c.auto_deposit = Some(cfg.auto_deposit && !cfg.no_auto_deposit.iter().any(|n| n == name));
                c.spot_locked = Some(cfg.locked_spots.iter().any(|n| n == name));
                let sk = cfg.skill_keys.get(name).copied().unwrap_or(0);
                c.skill_key = Some(sk);
                if let Some((h, _)) = seen.get(name) {
                    let e = pet.entry(name.to_string()).or_insert((None, 0));
                    if now().saturating_sub(e.1) >= 30 {
                        e.0 = zen_agent::win::nav::raven_now(*h);
                        e.1 = now();
                    }
                    c.pet_hp = e.0;
                    zen_agent::win::nav::set_clear_skill(*h, sk);
                    zen_agent::win::nav::set_char_name(*h, name);
                    // Mobs/NPCs/jogadores em volta, da memória do jogo (só leitura; respeita a
                    // pausa da RAM). Só os de até 25 tiles, no máximo 80.
                    if let (Some(pos), false) = (c.pos, monitor.ram_paused) {
                        c.nearby = zen_agent::win::objects::nearby_hwnd(*h, name, pos).map(|mut v| {
                            v.retain(|o| (o.x - pos.0).abs() <= 25 && (o.y - pos.1).abs() <= 25);
                            v.truncate(80);
                            v
                        });
                    }
                }
                c.spot_map = cfg.spots.get(name).map(|_| spot_dest(&cfg, name).name().to_string());
                let off = cfg.no_automation.iter().any(|n| n == name);
                c.automation = Some(!off);
                if off {
                    c.auto_status = if c.auto_status.is_empty() {
                        "automação desligada neste char".into()
                    } else {
                        format!("automação desligada neste char · {}", c.auto_status)
                    };
                }
                let spot = effective_spot(&cfg,name).map(|s| {
                    // Nome do dono no lugar do número (dono, 09/11 16:56: "coloque o nome de cada um no spot").
                    let n = match spot_dest(&cfg, name) {
                        zen_agent::win::nav::Dest::Kanturu => format!("spot do {name} "),
                        d => format!("spot do {name} na {} ", d.name()),
                    };
                    format!("{n}({}, {})", s[0], s[1])
                });
                c.auto_status = match (spot, rec.status_of(name)) {
                    (Some(s), Some(st)) => format!("{s} · {st}"),
                    (Some(s), None) => s,
                    (None, Some(st)) => st,
                    (None, None) => String::new(),
                };
            }
            // KS em spot assumido só gera análise/status: não chama nenhuma rotina de movimento.
            let group=zen_agent::spot_service::directory(false).unwrap_or_default();
            for c in &mut chars {
                let Some(name)=c.title.name.as_deref() else {continue};
                let Some(spot)=zen_agent::spot_service::effective(&cfg,name) else {continue};
                let farming=seen.get(name).is_some_and(|(_,o)|o.helper_on==Some(true)
                    && o.in_kanturu==Some(true) && o.pos.is_some_and(|p|(p.0-spot.x).abs().max((p.1-spot.y).abs())<=zen_core::spots::ARRIVAL_RADIUS));
                // Uma viagem de outro char n?o interrompe a amostra de quem continua farmando.
                if !farming {ks.remove(name);continue}
                let foreign=zen_core::spots::foreign_occupants(c.nearby.as_deref(),spot,name,|n|
                    group.friendly(n)||cfg.spots.keys().any(|f|f.eq_ignore_ascii_case(n))||cfg.friendly_chars.iter().any(|f|f.eq_ignore_ascii_case(n)));
                let Ok(foreign)=foreign else {continue};
                let foreign:Vec<String>=foreign.into_iter().map(str::to_owned).collect();
                if !rec.is_busy() && zen_agent::spot_service::active(name).is_none() {
                    if let Some(home)=zen_agent::spot_service::home(&cfg,name) {
                        let _=zen_agent::spot_service::set_active(name,zen_agent::spot_service::ActiveSpot{spot,home,retry_at:0});
                    }
                }
                let entry=ks.entry(name.to_string()).or_insert_with(||(spot,Default::default()));
                if entry.0!=spot {*entry=(spot,Default::default());}
                let zen=if c.zen_source==zen_core::protocol::ZenSource::Unknown {None}else{c.zen};
                if let Some(status)=entry.1.observe(now(),zen,&foreign) {c.auto_status.push_str(" · ");c.auto_status.push_str(&status);}
                if let Some(active)=zen_agent::spot_service::active(name).filter(|a|a.spot.world!=a.home.world) {
                    c.auto_status.push_str(&format!(" · temporário {}; principal {} será reavaliado às {}",active.spot.id(),active.home.id(),chrono_hhmm(active.retry_at)));
                }
            }
            // Manutenção leve do cursor somente no char em foco, farmando e sem outra rotina.
            if cfg.automation_enabled && !rec.is_busy() && !monitor.ram_paused && !input::manual_mode() {
                for (name, (hwnd, obs)) in &seen {
                    if cfg.no_automation.iter().any(|n| n == name) { continue; }
                    if obs.helper_on == Some(true) && !obs.disconnected && rec.farm_since.contains_key(name) {
                        input::maintain_cursor(*hwnd);
                    }
                }
            }
            // Pick Window: de volta ao spot e farmando há 30 s (helper ligado), Y -> Zen -> Y.
            // Um char por ciclo; falhou = tenta de novo em 5 min, até 3 vezes, depois avisa.
            if cfg.automation_enabled && cfg.zen_pick && !rec.is_busy() && !monitor.ram_paused && !input::manual_mode() {
                let due = seen.iter().find(|(n, (_, o))| {
                    !cfg.no_automation.iter().any(|x| x == *n) && !o.disconnected
                        && zen_agent::win::nav::pick_due(pick_needed.contains(*n), rec.farm_since.get(*n).copied(), o.helper_on, now(),
                            pick_retry.get(*n).map_or(0, |r| r.0), 30)
                }).map(|(n, (h, _))| (n.clone(), *h));
                if let Some((name, hwnd)) = due {
                    let world = spot_dest(&cfg, &name).world();
                    let who = name.clone();
                    match zen_agent::win::nav::enable_zen_pick(hwnd, world, &|s| tracing::info!(char = %who, "PICK: {s}")) {
                        Ok(msg) => {
                            tracing::warn!(char = %name, "PICK: {msg}");
                            pick_needed.remove(&name);
                            pick_retry.remove(&name);
                        }
                        Err(e) => {
                            let tries = pick_retry.get(&name).map_or(0, |r| r.1) + 1;
                            tracing::warn!(char = %name, tentativa = tries, erro = %e, "PICK: não ativado; farm segue");
                            if tries >= 3 {
                                pick_needed.remove(&name);
                                pick_retry.remove(&name);
                                let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok: false, message: format!("{name}: Pick do Zen (Y) não ativado em 3 tentativas: {e}") });
                            } else {
                                pick_retry.insert(name.clone(), (now() + 300, tries));
                            }
                        }
                    }
                }
            }
            // Abertura automática do inventário (V) para travar o Zen. Só com o PC sem uso.
            if cfg.automation_enabled && cfg.auto_inventory && !rec.is_busy() {
                let inv_cfg = zen_core::actions::AutoInvConfig {
                    idle_min_s: cfg.automation_idle_s,
                    ..Default::default()
                };
                // Sem trava manual conta como "PC sem uso" (o V não espera mais o Windows ficar parado).
                let inv_idle = if input::manual_mode() { 0 } else { u64::MAX / 4 };
                for r in monitor.auto_inventory(&access, now(), inv_idle, &inv_cfg) {
                    if cfg.no_automation.iter().any(|n| *n == r.char_name) {
                        continue;
                    }
                    // Confere de novo logo antes: você pode ter voltado ao PC nesse meio tempo.
                    if input::manual_mode() {
                        monitor.auto_inventory_failed(r.pid, now());
                        continue;
                    }
                    let steps = zen_core::actions::steps(ActionKind::ToggleInventory);
                    match input::run(r.hwnd, &steps) {
                        Ok(()) => tracing::info!(char = %r.char_name, abrir = r.open, "AUTOMAÇÃO: tecla V (trava do Zen)"),
                        Err(e) => {
                            tracing::warn!(char = %r.char_name, erro = %e, "AUTOMAÇÃO: tecla V não enviada");
                            monitor.auto_inventory_failed(r.pid, now());
                        }
                    }
                }
            }
            let holds = serde_json::to_string(&rec.hold).unwrap_or_default();
            if holds != last_holds {
                let tmp = holds_file.with_extension("json.tmp");
                if std::fs::write(&tmp, &holds).and_then(|_| std::fs::rename(&tmp, &holds_file)).is_ok() {
                    last_holds = holds;
                }
            }
            // Grava as travas quando mudam (arquivo temporário + rename: nunca fica pela metade).
            let locks = serde_json::to_string_pretty(&monitor.saved_locks()).unwrap_or_default();
            if locks != last_locks {
                let tmp = locks_file.with_extension("json.tmp");
                if std::fs::write(&tmp, &locks).and_then(|_| std::fs::rename(&tmp, &locks_file)).is_ok() {
                    last_locks = locks;
                }
            }
            let mut events = Vec::new();
            if cfg.chat_enabled && !monitor.ram_paused && last_chat.elapsed() >= CHAT_EVERY {
                let pids: Vec<u32> = chars.iter().map(|c| c.pid).collect();
                events = chat.scan(&pids, now());
                last_chat = Instant::now();
            }
            for c in &chars {
                if c.title.name.is_none() {
                    // Mostra o título de verdade: num PC novo o formato pode ser outro.
                    let t = access.list_windows().into_iter().find(|w| w.pid == c.pid).map(|w| w.title).unwrap_or_default();
                    let t = zen_core::sanitize(&t, 160);
                    if untitled.insert(c.pid, t.clone()).as_deref() != Some(t.as_str()) {
                        tracing::warn!(pid = c.pid, titulo = %t, "título sem nome de char reconhecível");
                    }
                    last_error = Some(format!("pid {}: título sem nome: \"{t}\"", c.pid));
                }
            }

            seq += 1;
            let msg = AgentMsg::Report {
                seq,
                sent_at: now(),
                chars,
                events,
                catalog: cfg.catalog.iter().map(|c| (c[0], c[1])).collect(),
                catalog_other: cfg
                    .catalog_maps
                    .iter()
                    .filter_map(|(m, v)| zen_agent::win::nav::Dest::from_name(m).map(|d| (d.world(), v)))
                    .flat_map(|(w, v)| v.iter().map(move |c| (w, c[0], c[1])))
                    .collect(),
                health: Health {
                    cpu_percent: cpu.sample(),
                    mem_mb: health::mem_mb(),
                    uptime_s: started.elapsed().as_secs(),
                    last_error: last_error.clone(),
                    ram_paused: monitor.ram_paused,
                    os: std::env::consts::OS.into(),
                    automation: cfg.automation_enabled,
                    manual: input::manual_by_hotkey(),
                    death_return: Some(zen_core::protocol::DeathReturn {
                        max_returns: cfg.return_max,
                        window_min: cfg.return_window_min,
                        delay_min_s: cfg.return_delay_min_s,
                        delay_max_s: cfg.return_delay_max_s,
                    }),
                },
            };
            if out_tx.try_send(msg).is_err() {
                tracing::warn!("fila de envio cheia; relatório descartado");
            }

            let spent = tick.elapsed();
            std::thread::sleep(Duration::from_millis(zen_agent::monitor::HP_TICK_MS).saturating_sub(spent));
        }
    }

    /// A memória disse "vida 0". Tira o print e confere o número na MESMA imagem.
    /// Tela com vida > 0 (duas vezes, 400 ms de intervalo) = endereço errado: refaz a trava
    /// e não avisa nada. Tela com 0 ou ilegível (tela de morte) = morte.
    fn confirm_death(access: &WinAccess, monitor: &mut Monitor, d: zen_agent::monitor::DeathEvent) -> Option<AgentMsg> {
        let mut shot = access.death_shot(d.hwnd);
        if matches!(shot, Some((_, Some(n))) if n > 0) {
            std::thread::sleep(Duration::from_millis(400));
            shot = access.death_shot(d.hwnd);
        }
        let screen_hp = shot.as_ref().and_then(|(_, h)| *h);
        if let Some(n) = screen_hp.filter(|n| *n > 0) {
            tracing::warn!(char = %d.char_name, tela = n, "memória disse vida 0 mas a tela mostra vida: refazendo a trava da vida");
            monitor.false_death(d.pid);
            return None;
        }
        tracing::warn!(char = %d.char_name, hp_antes = d.hp_before, tela = ?screen_hp, "MORTE detectada");
        Some(AgentMsg::Death {
            char_name: d.char_name,
            at: now(),
            hp_before: d.hp_before,
            screen_hp,
            png_base64: shot.map(|(png, _)| base64::engine::general_purpose::STANDARD.encode(png)),
        })
    }

    /// Recuperação até o spot: uma por vez, numa thread (os relatórios continuam saindo).
    struct Recovery {
        machines: std::collections::HashMap<String, zen_core::recovery::RecoveryMachine>,
        busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
        status: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
        done_tx: std::sync::mpsc::Sender<(String, Result<String, String>)>,
        done_rx: std::sync::mpsc::Receiver<(String, Result<String, String>)>,
        terrain: Option<std::sync::Arc<zen_core::terrain::Terrain>>,
        /// Terreno dos outros mapas de spot (Arena, Lost Tower, Dungeon), por World.
        other_terrain: std::collections::HashMap<u8, std::sync::Arc<zen_core::terrain::Terrain>>,
        /// Dark Raven: próxima ida ao Trainer só depois deste horário (evita laço NPC <-> spot).
        raven_block: std::collections::HashMap<String, u64>,
        /// Dark Raven: hora da primeira leitura baixa (a ida só depois da segunda).
        raven_low: std::collections::HashMap<String, u64>,
        /// Depósito falhou: não tenta de novo antes deste horário.
        deposit_block: std::collections::HashMap<String, u64>,
        deposit_wait_reason: std::collections::HashMap<String, String>,
        /// Chars cuja causa da morte ainda está sendo lida.
        pending: std::collections::HashSet<String>,
        /// Desde quando cada char está farmando perto do spot com o helper ligado.
        farm_since: std::collections::HashMap<String, u64>,
        /// Tempo de farm confirmado antes da morte (None = sem amostra contínua suficiente).
        death_farmed: std::collections::HashMap<String, Option<u64>>,
        /// Horários das mortes para monstros (3 em 30 min = para de voltar sozinho).
        monster_deaths: std::collections::HashMap<String, Vec<u64>>,
        /// Chars que não voltam sozinhos (PK ou causa desconhecida) -> motivo.
        hold: std::collections::HashMap<String, String>,
        return_owned: std::collections::HashSet<String>,
        /// Automação do PC religada pelo painel: estes chars são conferidos (mapa, spot,
        /// helper) e voltam ao spot se preciso, um por vez.
        relaunch: Vec<String>,
        /// Linhas "[PK] ... killed <char>" já vistas (mortes anteriores).
        known: std::collections::HashMap<String, std::collections::HashSet<(u64, String)>>,
        cause_tx: std::sync::mpsc::Sender<DeathCause>,
        cause_rx: std::sync::mpsc::Receiver<DeathCause>,
    }

    type DeathCause = (String, Vec<(u64, String)>, Option<(String, zen_core::chat::KillerKind)>);

    impl Recovery {
        fn new() -> Self {
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let (cause_tx, cause_rx) = std::sync::mpsc::channel();
            Self {
                pending: Default::default(),
                deposit_block: Default::default(),
                deposit_wait_reason: Default::default(),
                farm_since: Default::default(),
                death_farmed: Default::default(),
                monster_deaths: Default::default(),
                hold: Default::default(),
                return_owned: Default::default(),
                relaunch: Vec::new(),
                raven_block: Default::default(),
                raven_low: Default::default(),
                known: Default::default(),
                cause_tx,
                cause_rx,
                machines: Default::default(),
                busy: Default::default(),
                status: Default::default(),
                done_tx,
                done_rx,
                terrain: None,
                other_terrain: Default::default(),
            }
        }

        /// Lê no chat (memória do cliente da vítima) quem matou. Numa thread: ~10 s por varredura,
        /// até 2 varreduras (a linha pode chegar um pouco depois do HP zerar).
        fn scan_death_cause(&mut self, name: &str, pid: u32, block: bool) {
            if block {
                self.pending.insert(name.to_string());
            }
            let known = self.known.get(name).cloned().unwrap_or_default();
            let (tx, name) = (self.cause_tx.clone(), name.to_string());
            std::thread::spawn(move || {
                let cfg = zen_core::chat::ChatConfig::default();
                let suffix = format!(" killed {}", name.to_ascii_lowercase());
                let mut last = Vec::new();
                for wait in [1500u64, 3000] {
                    std::thread::sleep(Duration::from_millis(wait));
                    let t = Instant::now();
                    let lines: Vec<(u64, String)> = zen_agent::win::chat::pk_lines(pid)
                        .into_iter()
                        .filter(|(_, l)| l.to_ascii_lowercase().trim_end().ends_with(&suffix))
                        .collect();
                    let cause = zen_core::chat::death_cause(&lines, &name, &known, &cfg);
                    tracing::info!(char = %name, linhas = lines.len(), causa = ?cause, tempo = ?t.elapsed(), "chat lido depois da morte");
                    if cause.is_some() {
                        let _ = tx.send((name, lines, cause));
                        return;
                    }
                    last = lines;
                }
                let _ = tx.send((name, last, None));
            });
        }

        fn deposit_wait(&mut self, name: &str, reason: String) {
            if self.deposit_wait_reason.get(name) != Some(&reason) {
                tracing::warn!(char = name, motivo = %reason, "Fortuna automática: aguardando");
                self.deposit_wait_reason.insert(name.to_string(), reason);
            }
        }

        fn is_busy(&self) -> bool {
            self.busy.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn set_status(&self, name: &str, s: String) {
            if let Ok(mut m) = self.status.lock() {
                m.insert(name.to_string(), s);
            }
        }

        fn status_of(&self, name: &str) -> Option<String> {
            self.status.lock().ok().and_then(|m| m.get(name).cloned())
        }

        /// Dispara a recuperação de um char numa thread. Falha na hora se já houver uma rodando.
        /// Baú: M -> Kanturu, deposita tudo menos 40 mi (conferido na tela) e volta ao spot.
        fn start_deposit(&mut self, name: &str, pid: u32, hwnd: isize, spot: (i32, i32), dest: zen_agent::win::nav::Dest, all: bool) -> Result<(), String> {
            if self.is_busy() {
                return Err("já há uma automação em andamento".into());
            }
            if !dest.can_teleport() {
                return Err(format!("spot na {}: a Fortuna só sai de mapas com teleporte calibrado (Kanturu, Arena)", dest.name()));
            }
            let _terrain = self.spot_terrain(pid, hwnd, dest)?;
            self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
            let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
            if let Ok(mut m) = status.lock() {
                m.insert(name.clone(), "Fortuna: indo para Lorencia".into());
            }
            std::thread::spawn(move || {
                let st = status.clone();
                let n2 = name.clone();
                let cb = move |s: String| {
                    if let Ok(mut m) = st.lock() {
                        m.insert(n2.clone(), s);
                    }
                };
                let fmt = zen_core::fmt::int_br;
                // 09/11: o Zen virou Fortuna do Imperador em Lorencia (Lugard), não vai mais para o baú.
                let r = match zen_agent::win::nav::world_terrain_of(hwnd, zen_agent::win::nav::LORENCIA_WORLD)
                    .and_then(|lor| zen_agent::win::nav::fortune(hwnd, &lor, all, &cb))
                {
                    Ok(rep) => {
                        let back = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                        tracing::warn!(char = %name, fortunas = rep.converted, para_o_j = rep.to_bank, zen_antes = rep.zen_before, zen_depois = rep.zen_after, volta = ?back, "AUTOMAÇÃO: Fortuna do Imperador");
                        let msg = format!("depósito: {} Fortuna(s) do Imperador, {} guardada(s) no J, ficou {} no char", rep.converted, rep.to_bank, fmt(rep.zen_after));
                        match back {
                            Ok(_) => Ok(format!("{msg}, de volta ao spot")),
                            Err(e) => Ok(format!("{msg}; a volta ao spot falhou: {e}")),
                        }
                    }
                    Err(e) => {
                        tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: Fortuna falhou");
                        let _ = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                        Err(format!("depósito (Fortuna) falhou: {e}"))
                    }
                };
                busy.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send((name, r));
            });
            Ok(())
        }

        /// Dark Raven: Trainer de Lorencia (restaura com Zen) e volta ao spot.
        fn start_raven(&mut self, name: &str, pid: u32, hwnd: isize, spot: (i32, i32), dest: zen_agent::win::nav::Dest, manual: bool) -> Result<(), String> {
            if self.is_busy() {
                return Err("já há uma automação em andamento".into());
            }
            let _terrain = self.spot_terrain(pid, hwnd, dest)?;
            let lor = zen_agent::win::nav::world_terrain_of(hwnd, zen_agent::win::nav::LORENCIA_WORLD)?;
            self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
            let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
            tracing::warn!(char = %name, spot = ?spot, "AUTOMAÇÃO: Dark Raven com pouca vida: indo ao Trainer");
            std::thread::spawn(move || {
                let st = status.clone();
                let n2 = name.clone();
                let cb = move |s: String| {
                    if let Ok(mut m) = st.lock() {
                        m.insert(n2.clone(), s);
                    }
                };
                let r = match zen_agent::win::nav::restore_raven(hwnd, &lor, manual, &cb) {
                    Ok(rep) => {
                        let back = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                        tracing::warn!(char = %name, antes = ?rep.before, depois = rep.after, volta = ?back, "AUTOMAÇÃO: Dark Raven restaurado");
                        let msg = format!("Raven restaurado: {}% -> {}%", rep.before.map_or("?".into(), |v| v.to_string()), rep.after);
                        match back {
                            Ok(_) => Ok(format!("{msg}, de volta ao spot")),
                            Err(e) => Err(format!("{msg}, mas a volta ao spot falhou: {e}")),
                        }
                    }
                    Err(e) => {
                        tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: Dark Raven não restaurado");
                        let back = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                        Err(format!("Raven NÃO restaurado: {e}{}", if back.is_ok() { " (voltou ao spot)" } else { "" }))
                    }
                };
                busy.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send((name, r));
            });
            Ok(())
        }

        /// Elpis: Gemstone -> Harmony, todas as do inventário, o mais rápido possível (dono, 16/11
        /// 20:38). Ocupa a automação do PC até acabar.
        fn start_elpis(&mut self, name: &str, hwnd: isize) -> Result<(), String> {
            if self.is_busy() {
                return Err("já há uma automação em andamento".into());
            }
            self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
            let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
            std::thread::spawn(move || {
                let st = status.clone();
                let n2 = name.clone();
                let cb = move |s: String| {
                    if let Ok(mut m) = st.lock() {
                        m.insert(n2.clone(), s);
                    }
                };
                // Ação pedida no painel: roda mesmo com a automação do PC desligada (explicit_move);
                // desligar a automação no meio cancela no próximo passo.
                let r = match zen_agent::win::input::explicit_move(|| zen_agent::win::nav::elpis_refine(hwnd, &name, u32::MAX, &cb)) {
                    Ok(rep) => {
                        tracing::warn!(char = %name, refinos = rep.refined, harmonies = rep.ok, falhas = rep.failed, "ELPIS: acabaram as Gemstones");
                        Ok(format!("Elpis: acabaram as Gemstones; {} refino(s), {} Harmony(s), {} falha(s)", rep.refined, rep.ok, rep.failed))
                    }
                    Err(e) => {
                        tracing::warn!(char = %name, erro = %e, "ELPIS: parou");
                        Err(format!("Elpis parou: {e}"))
                    }
                };
                busy.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send((name, r));
            });
            Ok(())
        }

        /// Treino/limpeza: K -> J no lugar onde o char está (sem ir à cidade), com prints.
        fn start_bank(&mut self, name: &str, hwnd: isize) -> Result<(), String> {
            if self.is_busy() {
                return Err("já há uma automação em andamento".into());
            }
            self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
            let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
            std::thread::spawn(move || {
                let st = status.clone();
                let n2 = name.clone();
                let cb = move |s: String| {
                    if let Ok(mut m) = st.lock() {
                        m.insert(n2.clone(), s);
                    }
                };
                let r = zen_agent::win::nav::fortune_bank(hwnd, &cb);
                tracing::warn!(char = %name, resultado = ?r, "AUTOMAÇÃO: K -> J (treino)");
                let r = match r {
                    Ok(m) => Ok(format!("depósito (J): {m}")),
                    Err(e) => Err(format!("depósito (J) falhou: {e}")),
                };
                busy.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send((name, r));
            });
            Ok(())
        }

        /// v4: "ir até" do painel: (x, y) num mapa validado, sem mexer no spot.
        fn start_goto(&mut self, name: &str, hwnd: isize, world: u8, dest: zen_agent::win::nav::Dest, target: (i32, i32)) -> Result<(), String> {
            if self.is_busy() {
                return Err("já há uma automação em andamento".into());
            }
            let terrain = zen_agent::win::nav::world_terrain_of(hwnd, world)?;
            self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
            let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
            tracing::warn!(char = %name, world, destino = ?target, "AUTOMAÇÃO: ir até (painel)");
            std::thread::spawn(move || {
                let st = status.clone();
                let n2 = name.clone();
                let cb = move |s: String| {
                    if let Ok(mut m) = st.lock() {
                        m.insert(n2.clone(), s);
                    }
                };
                let r = input::explicit_move(|| zen_agent::win::nav::recover_in(hwnd, &terrain, target, dest, &cb));
                tracing::warn!(char = %name, resultado = ?r, "AUTOMAÇÃO: ir até concluído");
                let r = match r {
                    Ok(m) => Ok(format!("ir até ({}, {}): chegou — {m}. Fica lá até \"Voltar ao spot\"", target.0, target.1)),
                    Err(e) => Err(format!("ir até ({}, {}) falhou: {e}", target.0, target.1)),
                };
                busy.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send((name, r));
            });
            Ok(())
        }

        /// Terreno do mapa do spot (Kanturu ou Arena), carregado uma vez.
        fn spot_terrain(&mut self, pid: u32, hwnd: isize, dest: zen_agent::win::nav::Dest) -> Result<std::sync::Arc<zen_core::terrain::Terrain>, String> {
            if dest != zen_agent::win::nav::Dest::Kanturu {
                let w = dest.world();
                if !self.other_terrain.contains_key(&w) {
                    let t = zen_agent::win::nav::world_terrain_of(hwnd, w)?;
                    self.other_terrain.insert(w, std::sync::Arc::new(t));
                }
                return Ok(self.other_terrain[&w].clone());
            }
            if self.terrain.is_none() {
                self.terrain = Some(std::sync::Arc::new(zen_agent::win::nav::kanturu_terrain(pid)?));
            }
            Ok(self.terrain.clone().unwrap())
        }

        fn start(&mut self, name: &str, pid: u32, hwnd: isize, spot: (i32, i32), dest: zen_agent::win::nav::Dest, why: &str) -> Result<(), String> {
            if self.is_busy() {
                return Err("já há uma recuperação em andamento".into());
            }
            let _terrain = self.spot_terrain(pid, hwnd, dest)?;
            self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
            let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
            tracing::warn!(char = %name, motivo = why, spot = ?spot, "AUTOMAÇÃO: recuperação iniciada");
            if let Ok(mut m) = status.lock() {
                m.insert(name.clone(), format!("recuperando ({why})"));
            }
            let retain_owned=self.return_owned.contains(&name);
            std::thread::spawn(move || {
                let st = status.clone();
                let n2 = name.clone();
                let cb = move |s: String| {
                    if let Ok(mut m) = st.lock() {
                        m.insert(n2.clone(), s);
                    }
                };
                let report = |s: String| cb(if retain_owned { format!("retorno após morte/PK: {s}") } else { s });
                let r = zen_agent::win::spots::resume(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, retain_owned, &report);
                busy.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send((name, r));
            });
            Ok(())
        }
    }

    fn raven_cooldown(result: &Result<String,String>) -> Option<u64> {
        match result {
            Ok(m) | Err(m) if m.starts_with("Raven restaurado:") => Some(1800),
            Err(m) if m.starts_with("Raven N?O restaurado:") => Some(60),
            _ => None,
        }
    }

    fn rec_cfg_of(cfg: &AgentConfig) -> zen_core::recovery::RecoveryConfig {
        zen_core::recovery::RecoveryConfig {
            max_per_hour: cfg.return_max.clamp(1, 20) as usize,
            window_s: cfg.return_window_min.clamp(1, 240) * 60,
            far_tiles: cfg.spot_drift_tiles.clamp(1, 10),
            ..Default::default()
        }
    }

    /// Sorteio simples (sem crate de random): RandomState traz uma semente aleatória do SO.
    fn random_between(lo: u64, hi: u64) -> u64 {
        use std::hash::{BuildHasher, Hasher};
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(now());
        lo + h.finish() % (hi - lo + 1)
    }

    fn fmt_wait(s: u64) -> String {
        if s < 60 {
            format!("{s} s")
        } else {
            format!("{} min {:02} s", s / 60, s % 60)
        }
    }

    /// Um spot, um dono; char travado não troca de spot.
    fn can_assign(cfg: &AgentConfig, name: &str, spot: [i32; 2]) -> Result<(), String> {
        if cfg.locked_spots.iter().any(|n| n == name) && cfg.spots.get(name).is_some_and(|s| !same_spot(*s, spot)) {
            return Err(format!("{name} está travado no spot dele: destrave no painel antes"));
        }
        if let Some((other, _)) = cfg.spots.iter().find(|(n, s)| n.as_str() != name && home_dest(cfg, n) == zen_agent::win::nav::Dest::Kanturu && same_spot(**s, spot)) {
            return Err(format!("o spot ({}, {}) já é do {other}: troque ele de spot antes", spot[0], spot[1]));
        }
        Ok(())
    }

    /// Tiles a até 2 de distância contam como o mesmo spot.
    fn same_spot(a: [i32; 2], b: [i32; 2]) -> bool {
        (a[0] - b[0]).abs().max((a[1] - b[1]).abs()) <= 2
    }

    /// Põe o tile no fim do catálogo (se ainda não estiver). Devolve (posição 1..N, é novo).
    fn catalog_add(cfg: &mut AgentConfig, p: [i32; 2]) -> (usize, bool, [i32; 2]) {
        if let Some(i) = cfg.catalog.iter().position(|c| same_spot(*c, p)) {
            cfg.catalog[i] = p;
            return (i + 1, false, p);
        }
        cfg.catalog.push(p);
        (cfg.catalog.len(), true, p)
    }

    fn effective_spot(cfg: &AgentConfig,name:&str)->Option<[i32;2]> {
        zen_agent::spot_service::effective(cfg,name).map(|s|[s.x,s.y])
    }

    /// Mapa do spot do char (Kanturu se não houver outro marcado).
    fn home_dest(cfg: &AgentConfig, name: &str) -> zen_agent::win::nav::Dest {
        use zen_agent::win::nav::Dest;
        if cfg.arena.iter().any(|n| n == name) {
            return Dest::Arena;
        }
        cfg.spot_maps.get(name).and_then(|m| Dest::from_name(m)).unwrap_or(Dest::Kanturu)
    }

    fn spot_dest(cfg: &AgentConfig,name:&str)->zen_agent::win::nav::Dest {
        zen_agent::spot_service::effective(cfg,name).and_then(|s|zen_agent::win::nav::Dest::from_world(s.world))
            .unwrap_or_else(||home_dest(cfg,name))
    }

    static SPOT_EDIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn sync_saved_spots(cfg: &mut AgentConfig, rx: &std::sync::mpsc::Receiver<()>) {
        if rx.try_iter().count() == 0 { return; }
        match AgentConfig::load_file() {
            Ok(saved) => {
                cfg.catalog = saved.catalog;
                cfg.catalog_maps = saved.catalog_maps;
                cfg.spots = saved.spots;
                cfg.arena = saved.arena;
                cfg.spot_maps = saved.spot_maps;
            }
            Err(e) => tracing::error!(erro = %e, "não sincronizei os spots salvos"),
        }
    }

    fn spot_result_ok(message: &str) -> bool {
        !["ignorado", "falha", "falhou", "travado", "já é do", "sem spots", "cidade", "destrave"]
            .iter().any(|s| message.contains(s))
    }

    fn save_spots(cfg: &AgentConfig) -> anyhow::Result<()> {
        let (cat, spots, arena, sm) = (cfg.catalog.clone(), cfg.spots.clone(), cfg.arena.clone(), cfg.spot_maps.clone());
        save_field(move |c| {
            c.catalog = cat;
            c.spots = spots;
            c.arena = arena;
            c.spot_maps = sm;
        })
    }

    /// F2 / F1 apertados no jogo.
    /// Spot fora do Kanturu (Arena, Lost Tower, Dungeon, Devias, ...): um dono por spot no mesmo
    /// mapa; respeita a trava do char. Lorencia não (cidade).
    fn save_spot_other(cfg: &mut AgentConfig, name: &str, map: &str, p: [i32; 2]) -> Result<String, String> {
        if map == "Lorencia" {
            return Err("Lorencia é cidade: spot só nos mapas de farm".into());
        }
        if cfg.locked_spots.iter().any(|n| n == name) && cfg.spots.get(name).is_some_and(|s| !same_spot(*s, p)) {
            return Err(format!("{name} está travado no spot dele: destrave no painel antes"));
        }
        let same_map = |n: &String| if map == "Arena" { cfg.arena.contains(n) } else { cfg.spot_maps.get(n).map(String::as_str) == Some(map) };
        if let Some((other, _)) = cfg.spots.iter().find(|(n, s)| n.as_str() != name && same_map(n) && same_spot(**s, p)) {
            return Err(format!("esse spot da {map} já é do {other}"));
        }
        catalog_here(cfg,map,p)?;
        cfg.spots.insert(name.to_string(), p);
        cfg.arena.retain(|n| n != name);
        cfg.spot_maps.remove(name);
        if map == "Arena" {
            cfg.arena.push(name.to_string());
        } else {
            cfg.spot_maps.insert(name.to_string(), map.to_string());
        }
        let (spots, arena, sm, cm) = (cfg.spots.clone(), cfg.arena.clone(), cfg.spot_maps.clone(),cfg.catalog_maps.clone());
        save_field(move |c| {
            c.catalog_maps = cm;
            c.spots = spots;
            c.arena = arena;
            c.spot_maps = sm;
        })
        .map_err(|e| format!("falha ao salvar: {e}"))?;
        tracing::info!(char = %name, mapa = %map, spot = ?p, "spot salvo");
        Ok(format!("volta para o spot na {map} ({}, {})", p[0], p[1]))
    }

    /// Mapa e posição lidos na tela no instante do pedido (atalho ou painel).
    type Snap = (Option<&'static str>, Option<(i32, i32)>);

    fn catalog_here(cfg:&mut AgentConfig,map:&str,p:[i32;2])->Result<(usize,bool),String> {
        if map=="Lorencia" {return Err("Lorencia é cidade: sem spots de farm".into())}
        if map=="Kanturu" {
            let (id,new,_)=catalog_add(cfg,p);return Ok((id,new))
        }
        let list=cfg.catalog_maps.entry(map.into()).or_default();
        if let Some(i)=list.iter().position(|s|same_spot(*s,p)) {list[i]=p;Ok((i+1,false))}
        else {list.push(p);Ok((list.len(),true))}
    }

    fn handle_hotkey(h: zen_agent::win::hotkey::Hotkey, hwnd: isize, snap: Snap, cfg: &mut AgentConfig, access: &WinAccess) -> String {
        let Some(name)=access.list_windows().into_iter().find(|w|w.hwnd==hwnd)
            .and_then(|w|zen_core::title::parse(&w.title).name) else {return "atalho ignorado: janela sem char".into()};
        if h==zen_agent::win::hotkey::Hotkey::AssignSpot {
            return match save_spot_snapshot(0,name.clone(),Some(snap),cfg).into_iter().next() {
                Some(AgentMsg::CommandResult{ok,message,..})=>format!("F1 ({name}): {}{message}",if ok{""}else{"falhou: "}),
                _=>"F1 falhou: resultado ausente".into()
            };
        }
        let (Some(map),Some((x,y)))=snap else {return "F2 ignorado: mapa/coordenada ilegíveis no evento".into()};
        match catalog_here(cfg,map,[x,y]) {
            Ok((id,_))=>{
                let (catalog,catalog_maps)=(cfg.catalog.clone(),cfg.catalog_maps.clone());
                match save_field(move|c|{c.catalog=catalog;c.catalog_maps=catalog_maps;}) {
                    Ok(())=>format!("F2 ({name}): catálogo {map}, spot {id} em ({x}, {y}); retorno preservado"),
                    Err(e)=>format!("F2 falhou ao gravar: {e}")
                }
            }
            Err(e)=>format!("F2 falhou: {e}")
        }
    }

    fn save_spot_snapshot(id:u64,char_name:String,snap:Option<Snap>,cfg:&mut AgentConfig)->Vec<AgentMsg> {
        let result=(||->Result<String,String>{
            let (Some(map),Some((x,y)))=snap.unwrap_or((None,None)) else {return Err("mapa/coordenada ilegíveis no evento; não reli uma posição posterior".into())};
            let dest=zen_agent::win::nav::Dest::from_name(map).filter(|d|*d!=zen_agent::win::nav::Dest::Lorencia).ok_or("mapa sem spot de farm")?;
            let spot=zen_core::spots::Spot{world:dest.world(),x,y};
            if !spot.valid(){return Err("coordenadas inválidas".into())}
            if cfg.locked_spots.iter().any(|n|n==&char_name) &&
                zen_agent::spot_service::home(cfg,&char_name).is_some_and(|s|s!=spot) {
                return Err("char travado: destrave o spot antes de alterá-lo".into());
            }
            zen_agent::spot_service::request(&char_name,spot,false)?;
            let saved=(||->Result<String,String>{
                if map=="Kanturu" {
                    can_assign(cfg,&char_name,[x,y])?;
                    let (id,_,_)=catalog_add(cfg,[x,y]);
                    cfg.spots.insert(char_name.clone(),[x,y]);cfg.arena.retain(|n|n!=&char_name);cfg.spot_maps.remove(&char_name);
                    save_spots(cfg).map_err(|e|e.to_string())?;
                    Ok(format!("{char_name}: spot principal {id} em {map} ({x}, {y}) salvo e catalogado"))
                }else{
                    save_spot_other(cfg,&char_name,map,[x,y])
                }
            })();
            if saved.is_err(){let _=zen_agent::spot_service::request(&char_name,spot,true);}
            else {zen_agent::spot_service::clear_active(&char_name)?;}
            saved
        })();
        vec![match result {
            Ok(message)=>AgentMsg::CommandResult{id,ok:true,message},
            Err(message)=>AgentMsg::CommandResult{id,ok:false,message},
        }]
    }

    fn handle_command(
        id: u64,
        c: Command,
        snap: Option<Snap>,
        cfg: &mut AgentConfig,
        monitor: &mut Monitor,
        access: &WinAccess,
        rec: &mut Recovery,
    ) -> Vec<AgentMsg> {
        let ok = |m: &str| AgentMsg::CommandResult {
            id,
            ok: true,
            message: m.into(),
        };
        let err = |m: &str| AgentMsg::CommandResult {
            id,
            ok: false,
            message: m.into(),
        };
        match c {
            Command::RelockRam { char_name } => {
                if monitor.relock(access, &char_name) {
                    vec![ok("trava reiniciada; abra o inventário (V) nesse char")]
                } else {
                    vec![err("char não encontrado neste PC")]
                }
            }
            Command::Screenshot { char_name } => {
                match monitor
                    .hwnd_of(access, &char_name)
                    .and_then(|h| access.screenshot_png(h))
                {
                    Some(png) => vec![AgentMsg::Screenshot {
                        id,
                        char_name,
                        png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                    }],
                    None => vec![err("não consegui capturar a janela (minimizada?)")],
                }
            }
            Command::SendLogs => vec![AgentMsg::Logs {
                id,
                text: tail_logs(64 * 1024),
            }],
            Command::PauseRam | Command::ResumeRam => {
                let pause = matches!(c, Command::PauseRam);
                monitor.ram_paused = pause;
                cfg.ram_paused = pause;
                let _ = save_field(|c| c.ram_paused = pause);
                vec![ok(if pause {
                    "leitura de RAM pausada"
                } else {
                    "leitura de RAM retomada"
                })]
            }
            Command::SetConfig {
                chat_enabled,
                stopped_after_s: _,
                automation_enabled,
            } => {
                // stopped_after_s é regra do central; aqui só o que o agente usa.
                if let Some(v) = chat_enabled {
                    cfg.chat_enabled = v;
                }
                if let Some(v) = automation_enabled {
                    let was = cfg.automation_enabled;
                    cfg.automation_enabled = v;
                    // Desligar PARA o que estiver em andamento (o próximo passo de teclado/mouse
                    // recusa); religar confere onde cada char está e devolve ao spot.
                    input::set_paused(!v);
                    if v && !was {
                        rec.hold.retain(|_, h| !h.starts_with("foi até"));
                        rec.relaunch = cfg.spots.keys().cloned().collect();
                    }
                    if !v {
                        rec.relaunch.clear();
                    }
                    tracing::warn!(ligada = v, "AUTOMAÇÃO de teclado/mouse alterada pelo painel");
                }
                match save_field(|c| {
                    if let Some(v) = chat_enabled {
                        c.chat_enabled = v;
                    }
                    if let Some(v) = automation_enabled {
                        c.automation_enabled = v;
                    }
                }) {
                    Ok(()) => vec![ok("configuração salva")],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::GoTo { char_name, world, x, y, make_spot } => {
                use zen_agent::win::nav::{dest_of_world, Dest};
                if make_spot && !cfg.automation_enabled {
                    return vec![err("automação desligada neste PC (ligue no painel)")];
                }
                let Some(dest) = dest_of_world(world) else {
                    return vec![err(&format!("o mapa World{world} ainda não foi calibrado"))];
                };
                let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                    return vec![err("char não encontrado neste PC")];
                };
                if make_spot {
                    // Vira o spot do char (Kanturu: catálogo e um dono por spot; Arena: lista à parte).
                    match dest {
                        Dest::Kanturu => {
                            if let Err(e) = can_assign(cfg, &char_name, [x, y]) {
                                return vec![err(&e)];
                            }
                            let (_, _, spot) = catalog_add(cfg, [x, y]);
                            cfg.spots.insert(char_name.clone(), spot);
                            cfg.arena.retain(|n| n != &char_name);
                            cfg.spot_maps.remove(&char_name);
                        }
                        Dest::Arena => {
                            if cfg.locked_spots.iter().any(|n| n == &char_name) {
                                return vec![err(&format!("{char_name} está travado no spot dele: destrave no painel antes"))];
                            }
                            cfg.spots.insert(char_name.clone(), [x, y]);
                            if !cfg.arena.contains(&char_name) {
                                cfg.arena.push(char_name.clone());
                            }
                            cfg.spot_maps.remove(&char_name);
                        }
                        Dest::Lorencia => return vec![err("spot só no Kanturu ou na Arena (Lorencia é cidade)")],
                        other => {
                            // Outros mapas (11/11): spot livre, um dono por spot no mesmo mapa.
                            let map = other.name().to_string();
                            if cfg.locked_spots.iter().any(|n| n == &char_name) {
                                return vec![err(&format!("{char_name} está travado no spot dele: destrave no painel antes"))];
                            }
                            if let Some((o, _)) = cfg.spots.iter().find(|(n, s)| n.as_str() != char_name && cfg.spot_maps.get(*n) == Some(&map) && same_spot(**s, [x, y])) {
                                return vec![err(&format!("esse spot da {map} já é do {o}"))];
                            }
                            cfg.spots.insert(char_name.clone(), [x, y]);
                            cfg.arena.retain(|n| n != &char_name);
                            cfg.spot_maps.insert(char_name.clone(), map);
                        }
                    }
                    let (cat, spots, arena, sm) = (cfg.catalog.clone(), cfg.spots.clone(), cfg.arena.clone(), cfg.spot_maps.clone());
                    if let Err(e) = save_field(move |c| {
                        c.catalog = cat;
                        c.spots = spots;
                        c.arena = arena;
                        c.spot_maps = sm;
                    }) {
                        return vec![err(&format!("falha ao salvar: {e}"))];
                    }
                    zen_agent::spot_service::clear_active(&char_name).ok();
                    rec.hold.remove(&char_name);
                    return match rec.start(&char_name, w.pid, w.hwnd, (x, y), dest, "novo spot pelo painel") {
                        Ok(()) => vec![ok(&format!("{char_name}: novo spot ({x}, {y}) salvo; indo para lá"))],
                        Err(e) => vec![err(&e)],
                    };
                }
                // Só ir: o char fica lá; a volta automática não puxa de volta até "Voltar ao spot".
                rec.hold.insert(char_name.clone(), format!("foi até ({x}, {y}) pelo painel: não volta sozinho até \"Voltar ao spot\""));
                match rec.start_goto(&char_name, w.hwnd, world, dest, (x, y)) {
                    Ok(()) => vec![ok(&format!("{char_name}: indo até ({x}, {y})"))],
                    Err(e) => {
                        rec.hold.remove(&char_name);
                        vec![err(&e)]
                    }
                }
            }
            Command::SaveSpot { char_name } => save_spot_snapshot(id, char_name, snap, cfg),
            Command::SetDeathReturn { max_returns, window_min, delay_min_s, delay_max_s } => {
                let d = zen_core::protocol::DeathReturn { max_returns, window_min, delay_min_s, delay_max_s };
                if let Err(e) = d.validate() {
                    return vec![err(&e)];
                }
                cfg.return_max = max_returns;
                cfg.return_window_min = window_min;
                cfg.return_delay_min_s = delay_min_s;
                cfg.return_delay_max_s = delay_max_s;
                match save_field(move |c| {
                    c.return_max = max_returns;
                    c.return_window_min = window_min;
                    c.return_delay_min_s = delay_min_s;
                    c.return_delay_max_s = delay_max_s;
                }) {
                    Ok(()) => vec![ok(&format!(
                        "volta automática: até {max_returns} em {window_min} min; depois de morrer espera de {} a {}",
                        fmt_wait(delay_min_s),
                        fmt_wait(delay_max_s)
                    ))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::SetCharAutomation { char_name, enabled } => {
                cfg.no_automation.retain(|n| *n != char_name);
                if !enabled {
                    cfg.no_automation.push(char_name.clone());
                }
                let list = cfg.no_automation.clone();
                tracing::warn!(char = %char_name, ligada = enabled, "automação do char alterada pelo painel");
                match save_field(move |c| c.no_automation = list) {
                    Ok(()) => vec![ok(&format!("{char_name}: automação {}", if enabled { "LIGADA" } else { "DESLIGADA (só monitorando)" }))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::SetSkillKey { char_name, key } => {
                if key > 9 {
                    return vec![err("tecla inválida (use 1 a 9, 0 ou nenhuma)")];
                }
                // 0 no painel = nenhuma; a tecla "0" do teclado não é usada (evita confusão).
                if key == 0 {
                    cfg.skill_keys.remove(&char_name);
                } else {
                    cfg.skill_keys.insert(char_name.clone(), key);
                }
                let map = cfg.skill_keys.clone();
                tracing::warn!(char = %char_name, tecla = key, "skill de limpar mobs alterada pelo painel");
                match save_field(move |c| c.skill_keys = map) {
                    Ok(()) => vec![ok(&format!("{char_name}: {}", if key == 0 { "sem skill: trava no caminho usa o helper".to_string() } else { format!("skill da tecla {key} segurada no botão direito quando travar no caminho") }))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::SetSpotLock { char_name, locked } => {
                if locked && !cfg.spots.contains_key(&char_name) {
                    return vec![err("este char não tem spot para travar")];
                }
                cfg.locked_spots.retain(|n| *n != char_name);
                if locked {
                    cfg.locked_spots.push(char_name.clone());
                }
                let list = cfg.locked_spots.clone();
                match save_field(move |c| c.locked_spots = list) {
                    Ok(()) => vec![ok(&format!("{char_name}: {}", if locked { "travado no spot" } else { "destravado" }))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::SetAutoDeposit { char_name, enabled } => {
                cfg.no_auto_deposit.retain(|n| *n != char_name);
                if !enabled {
                    cfg.no_auto_deposit.push(char_name.clone());
                } else {
                    // Ligar pelo painel também tira o bloqueio de 1 h de uma falha anterior.
                    rec.deposit_block.remove(&char_name);
                }
                let list = cfg.no_auto_deposit.clone();
                let global = enabled && !cfg.auto_deposit;
                if global {
                    cfg.auto_deposit = true;
                }
                match save_field(move |c| {
                    c.no_auto_deposit = list;
                    if global {
                        c.auto_deposit = true;
                    }
                }) {
                    Ok(()) => vec![ok(&format!("{char_name}: depósito automático no baú {}", if enabled { "LIGADO" } else { "DESLIGADO" }))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::SetReturnOnDeath { char_name, enabled } => {
                cfg.no_return_on_death.retain(|n| *n != char_name);
                if !enabled {
                    cfg.no_return_on_death.push(char_name.clone());
                } else if rec.hold.get(&char_name).is_some_and(|h| h.contains("desligada")) {
                    rec.hold.remove(&char_name);
                }
                let list = cfg.no_return_on_death.clone();
                match save_field(move |c| c.no_return_on_death = list) {
                    Ok(()) => vec![ok(&format!(
                        "{char_name}: volta automática depois de morrer {}",
                        if enabled { "LIGADA" } else { "DESLIGADA" }
                    ))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::AssignSpot { char_name, x, y } => {
                // Confere antes de mexer no catálogo (catalog_add junta tiles a até 2 de distância).
                let probe = cfg.catalog.iter().copied().find(|c| same_spot(*c, [x, y])).unwrap_or([x, y]);
                if let Err(e) = can_assign(cfg, &char_name, probe) {
                    return vec![err(&e)];
                }
                let (n, _, spot) = catalog_add(cfg, [x, y]);
                cfg.spots.insert(char_name.clone(), spot);
                cfg.arena.retain(|n| n != &char_name);
                cfg.spot_maps.remove(&char_name);
                match save_spots(cfg) {
                    Ok(()) => {let _=zen_agent::spot_service::clear_active(&char_name); vec![ok(&format!("{char_name}: volta para o spot {n} ({}, {})", spot[0], spot[1]))]},
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::DeleteSpot { x, y, world } if world.is_some_and(|w| w != zen_agent::win::nav::KANTURU_WORLD) => {
                // Spot do catálogo de outro mapa: sai do catálogo e dos chars que voltavam para ele.
                let Some(map) = world.and_then(zen_agent::win::nav::Dest::from_world).map(|d| d.name().to_string()) else {
                    return vec![err("mapa desconhecido")];
                };
                let on_map = |cfg: &AgentConfig, n: &String| home_dest(cfg, n).name() == map;
                if let Some((n, _)) = cfg.spots.iter().find(|(n, s)| **s == [x, y] && on_map(cfg, n) && cfg.locked_spots.contains(n)) {
                    return vec![err(&format!("o spot ({x}, {y}) é do {n}, que está travado: destrave antes"))];
                }
                let before = cfg.catalog_maps.get(&map).map_or(0, Vec::len);
                if let Some(v) = cfg.catalog_maps.get_mut(&map) {
                    v.retain(|c| *c != [x, y]);
                }
                let freed: Vec<String> = cfg.spots.iter().filter(|(n, s)| **s == [x, y] && on_map(cfg, n)).map(|(n, _)| n.clone()).collect();
                for n in &freed {
                    cfg.spots.remove(n);
                    zen_agent::spot_service::clear_active(n).ok();
                    cfg.spot_maps.remove(n);
                    cfg.arena.retain(|a| a != n);
                }
                if before == cfg.catalog_maps.get(&map).map_or(0, Vec::len) && freed.is_empty() {
                    return vec![err("esse spot não está no catálogo")];
                }
                let (spots, arena, sm, cm) = (cfg.spots.clone(), cfg.arena.clone(), cfg.spot_maps.clone(), cfg.catalog_maps.clone());
                match save_field(move |c| {
                    c.spots = spots;
                    c.arena = arena;
                    c.spot_maps = sm;
                    c.catalog_maps = cm;
                }) {
                    Ok(()) => vec![ok(&format!("spot ({x}, {y}) da {map} excluído{}", if freed.is_empty() { String::new() } else { format!("; sem spot agora: {}", freed.join(", ")) }))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::DeleteSpot { x, y, .. } => {
                if let Some((n, _)) = cfg.spots.iter().find(|(n, s)| **s == [x, y] && spot_dest(cfg, n) == zen_agent::win::nav::Dest::Kanturu && cfg.locked_spots.contains(n)) {
                    return vec![err(&format!("o spot ({x}, {y}) é do {n}, que está travado: destrave antes"))];
                }
                let before = cfg.catalog.len();
                cfg.catalog.retain(|c| *c != [x, y]);
                let freed: Vec<String> = cfg.spots.iter().filter(|(n, s)| **s == [x, y] && spot_dest(cfg, n) == zen_agent::win::nav::Dest::Kanturu).map(|(n, _)| n.clone()).collect();
                for n in &freed {
                    cfg.spots.remove(n);
                    zen_agent::spot_service::clear_active(n).ok();
                }
                if before == cfg.catalog.len() && freed.is_empty() {
                    return vec![err("esse spot não está no catálogo")];
                }
                match save_spots(cfg) {
                    Ok(()) => vec![ok(&format!(
                        "spot ({x}, {y}) excluído{}",
                        if freed.is_empty() { String::new() } else { format!("; sem spot agora: {}", freed.join(", ")) }
                    ))],
                    Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
                }
            }
            Command::RunAction { char_name, action } => {
                if action == ActionKind::RefineGemstones {
                    let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                        return vec![err("char não encontrado neste PC")];
                    };
                    tracing::warn!(char = %char_name, "AUTOMAÇÃO: Elpis (Gemstone -> Harmony) pedida pelo painel");
                    return match rec.start_elpis(&char_name, w.hwnd) {
                        Ok(()) => vec![ok(&format!("{char_name}: Elpis iniciada: todas as Gemstones do inventário (acompanhe no card)"))],
                        Err(e) => vec![err(&e)],
                    };
                }
                if !cfg.automation_enabled {
                    return vec![err("automação desligada neste PC (ligue no painel ou no agent.toml)")];
                }
                if action == ActionKind::FortuneBank {
                    let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                        return vec![err("char não encontrado neste PC")];
                    };
                    tracing::warn!(char = %char_name, "AUTOMAÇÃO: K -> J pedido pelo painel");
                    return match rec.start_bank(&char_name, w.hwnd) {
                        Ok(()) => vec![ok(&format!("{char_name}: K -> J iniciado (prints em {char_name}-passo-*)"))],
                        Err(e) => vec![err(&e)],
                    };
                }
                if action == ActionKind::Fortune || action == ActionKind::FortuneAll {
                    let Some(spot) = effective_spot(cfg,&char_name) else {
                        return vec![err("este char não tem spot salvo (use \"Salvar spot\")")];
                    };
                    let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                        return vec![err("char não encontrado neste PC")];
                    };
                    let all = action == ActionKind::FortuneAll;
                    tracing::warn!(char = %char_name, bau_inteiro = all, "AUTOMAÇÃO: Fortuna pedida pelo painel");
                    return match rec.start_deposit(&char_name, w.pid, w.hwnd, (spot[0], spot[1]), spot_dest(cfg, &char_name), all) {
                        Ok(()) => vec![ok(&format!("{char_name}: viagem da Fortuna iniciada (acompanhe no card)"))],
                        Err(e) => vec![err(&e)],
                    };
                }
                if action == ActionKind::RestoreRaven {
                    let Some(spot) = effective_spot(cfg,&char_name) else {
                        return vec![err("este char não tem spot salvo (use \"Salvar spot\")")];
                    };
                    let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                        return vec![err("char não encontrado neste PC")];
                    };
                    if zen_agent::win::nav::raven_confirmed(w.hwnd).is_none() {
                        return vec![err("não vejo a barra do Dark Raven na tela deste char")];
                    }
                    return match rec.start_raven(&char_name, w.pid, w.hwnd, (spot[0], spot[1]), spot_dest(cfg, &char_name), true) {
                        Ok(()) => vec![ok(&format!("{char_name}: indo ao Trainer restaurar o Raven (acompanhe no card)"))],
                        Err(e) => vec![err(&e)],
                    };
                }
                if action == ActionKind::ReturnToSpot {
                    let Some(spot) = effective_spot(cfg,&char_name) else {
                        return vec![err("este char não tem spot salvo (use \"Salvar spot\")")];
                    };
                    let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                        return vec![err("char não encontrado neste PC")];
                    };
                    return match rec.start(&char_name, w.pid, w.hwnd, (spot[0], spot[1]), spot_dest(cfg, &char_name), "pedido do painel") {
                        Ok(()) => {
                            rec.hold.remove(&char_name);
                            rec.monster_deaths.remove(&char_name);
                            rec.pending.remove(&char_name);
                            vec![ok(&format!("{char_name}: recuperação iniciada (acompanhe no card)"))]
                        }
                        Err(e) => vec![err(&e)],
                    };
                }
                if rec.is_busy() {
                    return vec![err("há uma recuperação em andamento; tente em 1 minuto")];
                }
                let Some(h) = monitor.hwnd_of(access, &char_name) else {
                    return vec![err("char não encontrado neste PC")];
                };
                match input::run(h, &zen_core::actions::steps(action)) {
                    Ok(()) => {
                        tracing::info!(char = %char_name, acao = action.label(), "AUTOMAÇÃO: ação do painel executada");
                        vec![ok(&format!("{}: {} feito", char_name, action.label()))]
                    }
                    Err(e) => {
                        tracing::warn!(char = %char_name, acao = action.label(), erro = %e, "AUTOMAÇÃO: ação do painel falhou");
                        vec![err(&e)]
                    }
                }
            }
            Command::RestartAgent => {
                // Código 3: o vigia (tarefa agendada com "reiniciar se falhar") sobe de novo.
                tracing::info!("reiniciando por comando");
                std::process::exit(3);
            }
            Command::CheckUpdate => vec![err("atualização automática entra na fase 4")],
        }
    }

    /// Grava uma mudança no agent.toml partindo do ARQUIVO (não da memória), para que
    /// modo teste e variáveis de ambiente não sejam gravados por engano.
    fn save_field(f: impl FnOnce(&mut AgentConfig)) -> anyhow::Result<()> {
        AgentConfig::edit(f)
    }

    fn tail_logs(max: usize) -> String {
        let dir = log_dir();
        let newest = std::fs::read_dir(&dir).ok().and_then(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("agent"))
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        });
        let mut head = String::new();
        for extra in ["vigia.txt", "rollback-visto.txt", zen_agent::update::STATE_FILE] {
            if let Ok(t) = std::fs::read_to_string(AgentConfig::dir().join(extra)) {
                head.push_str(&format!("=== {extra} ===\n{}\n", t.chars().take(2000).collect::<String>()));
            }
        }
        // Os dois arquivos mais novos (o log vira à meia-noite UTC = 21h no Brasil).
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|it| it.filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("agent")).collect())
            .unwrap_or_default();
        files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        let _ = newest;
        if files.is_empty() {
            return head + "sem logs";
        }
        let mut body = Vec::new();
        for f in files.iter().rev().take(2).rev() {
            body.extend_from_slice(format!("=== {} ===\n", f.file_name().to_string_lossy()).as_bytes());
            body.extend_from_slice(&std::fs::read(f.path()).unwrap_or_default());
        }
        let start = body.len().saturating_sub(max);
        format!("{head}{}", String::from_utf8_lossy(&body[start..]))
    }

    #[cfg(test)]
    mod spot_tests {
        #[test]
        fn raven_bloqueio_longo_so_depois_da_restauracao_confirmada() {
            assert_eq!(super::raven_cooldown(&Ok("Raven restaurado: 0% -> 100%".into())),Some(1800));
            assert_eq!(super::raven_cooldown(&Err("Raven N?O restaurado: sem rota".into())),Some(60));
            assert_eq!(super::raven_cooldown(&Err("n?o iniciou".into())),None);
        }
        use super::*;

        #[test]
        fn f2_cataloga_outro_mapa_sem_trocar_principal_ou_retorno() {
            let mut cfg=AgentConfig::default();
            cfg.spots.insert("Teste".into(),[91,66]);
            let before=(cfg.spots.clone(),cfg.arena.clone(),cfg.spot_maps.clone());
            assert_eq!(catalog_here(&mut cfg,"Dungeon",[20,30]).unwrap(),(1,true));
            assert_eq!(catalog_here(&mut cfg,"Dungeon",[20,30]).unwrap(),(1,false));
            assert_eq!(before,(cfg.spots.clone(),cfg.arena.clone(),cfg.spot_maps.clone()));
            assert_eq!(cfg.catalog_maps["Dungeon"],vec![[20,30]]);
        }

        #[test]
        fn personagem_em_espera_manual_nao_bloqueia_fortuna_do_outro() {
            let mut cfg = AgentConfig::default();
            cfg.recovery_enabled = true;
            let mut rec = Recovery::new();
            assert!(recovery_can_take_priority(&cfg,&rec,"x9901"));
            rec.hold.insert("x9901".into(),"foi até pelo painel".into());
            assert!(!recovery_can_take_priority(&cfg,&rec,"x9901"));
            rec.hold.clear();
            cfg.no_automation.push("x9901".into());
            assert!(!recovery_can_take_priority(&cfg,&rec,"x9901"));
            cfg.no_automation.clear();
            cfg.recovery_enabled = false;
            assert!(!recovery_can_take_priority(&cfg,&rec,"x9901"));
        }

        #[test]
        fn salvar_preserva_tile_exato_perto_de_spot_existente() {
            let mut cfg = AgentConfig::default();
            cfg.catalog = vec![[120, 20]];
            let (id, new, saved) = catalog_add(&mut cfg, [121, 22]);
            assert_eq!((id, new, saved), (1, false, [121, 22]));
            assert_eq!(cfg.catalog, vec![[121, 22]]);
        }

        #[test]
        fn captura_ausente_ou_invalida_falha_sem_reler_ou_alterar_spots() {
            let mut cfg = AgentConfig::default();
            cfg.spots.insert("Teste".into(), [120, 20]);
            for snap in [None, Some((None, None)), Some((None, Some((121, 22))))] {
                let replies = save_spot_snapshot(17, "Teste".into(), snap, &mut cfg);
                assert!(matches!(&replies[0], AgentMsg::CommandResult { id: 17, ok: false, .. }));
                assert_eq!(cfg.spots["Teste"], [120, 20]);
            }
        }

        #[test]
        fn recusas_e_erros_nao_aparecem_como_sucesso() {
            for message in ["atalho ignorado", "falhou ao gravar", "falha ao salvar", "char travado", "esse spot já é do Outro", "Lorencia é cidade: sem spots"] {
                assert!(!spot_result_ok(message), "{message}");
            }
            assert!(spot_result_ok("F2 (Teste): spot 1 salvo em (121, 22)"));
        }
    }

    #[allow(dead_code)]
    fn _assert_access_is_object_safe(_: &dyn GameAccess) {}
}
