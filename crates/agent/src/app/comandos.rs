use super::*;

pub(super) fn handle_command(
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
        Command::SetFarm { char_name, rules } => {
            let mut rules = rules;
            rules.retorno_min = rules.retorno_min.clamp(5, 24 * 60);
            rules.mapas.retain(|m| zen_core::spots::world_for_name(m).is_some_and(|w| w != 1));
            rules.mapas.dedup();
            cfg.farm.insert(char_name.clone(), rules.clone());
            let all = cfg.farm.clone();
            match save_field(move |c| c.farm = all) {
                Ok(()) => vec![ok(&format!("{char_name}: navegação e farm salvos (mapas alternativos: {}; retorno ao original a cada {} min)", if rules.alternativos && !rules.mapas.is_empty() { rules.mapas.join(" → ") } else { "nenhum".into() }, rules.retorno_min))],
                Err(e) => vec![err(&format!("falha ao salvar: {e}"))],
            }
        }
        Command::ReturnToOriginal { char_name } => {
            let Some(home) = zen_agent::spot_service::home(cfg, &char_name) else {
                return vec![err("este char não tem spot original (F1)")];
            };
            let Some(w) = access.list_windows().into_iter().find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(char_name.as_str())) else {
                return vec![err("char não encontrado neste PC")];
            };
            let Some(dest) = zen_agent::win::nav::Dest::from_world(home.world) else {
                return vec![err("mapa original sem teleporte calibrado")];
            };
            let _ = zen_agent::spot_service::clear_active(&char_name);
            rec.hold.remove(&char_name);
            match rec.start(&char_name, w.pid, w.hwnd, (home.x, home.y), dest, "voltar ao mapa original (painel)") {
                Ok(()) => vec![ok(&format!("{char_name}: voltando ao mapa original {} ({}, {})", dest.name(), home.x, home.y))],
                Err(e) => vec![err(&e)],
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
pub(super) fn save_field(f: impl FnOnce(&mut AgentConfig)) -> anyhow::Result<()> {
    AgentConfig::edit(f)
}

pub(super) fn tail_logs(max: usize) -> String {
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
