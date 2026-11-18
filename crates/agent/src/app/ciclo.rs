use super::*;

pub(super) fn worker(test_mode: bool) -> anyhow::Result<()> {
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
    let mut notices = zen_agent::win::avisos::NoticeReader::default();
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
            if ok {
                rec.full_until.remove(&name);
            } else if matches!(&r, Err(e) if e.starts_with("nenhum spot livre confirmado")) {
                // Tudo lotado: o char já foi para a safe (spots.rs). Nova volta pelo catálogo só
                // daqui a ~1 h, com horário sorteado para não parecer robô.
                let at = now() + random_between(3000, 4200);
                rec.full_until.insert(name.clone(), at);
                let m = format!("{name}: todos os spots lotados; esperando na safe, nova busca às {}", chrono_hhmm(at));
                tracing::warn!(char = %name, nova_busca = %chrono_hhmm(at), "spots lotados: espera na safe");
                let _ = out_tx.try_send(AgentMsg::CommandResult { id: 0, ok: false, message: m.clone() });
                rec.set_status(&name, m);
                continue;
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
                if let Some(&at) = rec.full_until.get(&name) {
                    if now() < at {
                        rec.set_status(&name, format!("todos os spots lotados: esperando na safe, nova busca às {}", chrono_hhmm(at)));
                        continue;
                    }
                    rec.full_until.remove(&name);
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
            // Vigia da chegada (dono, 18/11 08:46): nos primeiros 3 min no spot, jogador de fora
            // por perto = o spot já era dele. Larga o spot e procura outro livre. Depois de 3 min
            // o spot é nosso e KS só vira aviso. Não vale para a volta depois de morte (sempre
            // volta) nem para char travado no spot.
            let recem=rec.farm_since.get(name).is_some_and(|t|now().saturating_sub(*t)<180);
            if recem && !foreign.is_empty() && cfg.automation_enabled && !rec.is_busy() && !input::manual_mode()
                && !rec.return_owned.contains(name) && !cfg.locked_spots.iter().any(|n|n==name)
                && !cfg.no_automation.iter().any(|n|n==name) {
                if let Some((hwnd,_))=seen.get(name).copied() {
                    let why=format!("spot já ocupado por {} ao chegar", foreign.join(", "));
                    let _=zen_agent::spot_service::clear_active(name);
                    rec.farm_since.remove(name);
                    if let Err(e)=rec.start(name,c.pid,hwnd,(spot.x,spot.y),spot_dest(&cfg,name),&why) {
                        tracing::warn!(char=%name,erro=%e,"vigia da chegada: busca não iniciou");
                    }
                    continue;
                }
            }
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
        // Avisos do servidor: o mesmo texto chega a todos os clientes; lê só um por PC.
        if cfg.server_notices && !monitor.ram_paused {
            if let Some(c) = chars.iter().filter(|c| c.title.name.is_some()).min_by_key(|c| c.pid) {
                for text in notices.poll(c.pid) {
                    tracing::info!(aviso = %text, "aviso do servidor");
                    let _ = out_tx.try_send(AgentMsg::ServerNotice {
                        char_name: c.title.name.clone().unwrap_or_default(),
                        at: now(),
                        text,
                    });
                }
            }
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
pub(super) fn confirm_death(access: &WinAccess, monitor: &mut Monitor, d: zen_agent::monitor::DeathEvent) -> Option<AgentMsg> {
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
