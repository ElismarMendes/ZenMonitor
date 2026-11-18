use crate::win::nav::*;

/// NPCs vivos perto do char, lidos da memória no começo de cada trecho segurado.
pub(in crate::win::nav) static LIVE_NPCS: std::sync::Mutex<Vec<(i32, i32)>> = std::sync::Mutex::new(Vec::new());

pub(in crate::win::nav) fn refresh_live_npcs(hwnd: isize, pos: (i32, i32)) {
    let v: Vec<(i32, i32)> = objects_near(hwnd, pos)
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.kind == zen_core::objects::Kind::Npc && cheb((o.x, o.y), pos) <= 12)
        .map(|o| (o.x, o.y))
        .collect();
    if let Ok(mut g) = LIVE_NPCS.lock() {
        *g = v;
    }
}

// ---------------------------------------------------------------- NPCs aprendidos (10/11 22:33)
// Os NPCs vêm do servidor: não estão em nenhum arquivo do cliente. Quando uma janela de NPC (o
// baú) abre no meio de uma caminhada, o tile onde o cursor estava é gravado por mapa em
// npcs_aprendidos.json (ao lado do agente) e o cursor passa a desviar dele em qualquer caminhada.

/// Mapa (World) da caminhada em andamento: uma por vez (a automação é sequencial).
pub(in crate::win::nav) static CUR_WORLD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub(in crate::win::nav) static LEARNED: std::sync::Mutex<Option<std::collections::BTreeMap<u8, Vec<(i32, i32)>>>> = std::sync::Mutex::new(None);

pub(in crate::win::nav) fn learned_path() -> std::path::PathBuf {
    crate::config::AgentConfig::dir().join("npcs_aprendidos.json")
}

pub(in crate::win::nav) fn with_learned<R>(f: impl FnOnce(&mut std::collections::BTreeMap<u8, Vec<(i32, i32)>>) -> R) -> Option<R> {
    let mut g = LEARNED.lock().ok()?;
    let m = g.get_or_insert_with(|| {
        std::fs::read_to_string(learned_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    });
    Some(f(m))
}

pub fn learned_npcs(world: u8) -> Vec<(i32, i32)> {
    with_learned(|m| m.get(&world).cloned().unwrap_or_default()).unwrap_or_default()
}

/// Tile aproximado embaixo do ponto `q` da tela, com o char em `pos` (modelo pixel->tile).
pub(in crate::win::nav) fn tile_at_px(pos: (i32, i32), q: (i32, i32)) -> (i32, i32) {
    let (px, py) = ((q.0 - CENTER.0) as f64, (q.1 - CENTER.1) as f64);
    let m = MODEL;
    (pos.0 + (m[0][0] * px + m[0][1] * py).round() as i32, pos.1 + (m[1][0] * px + m[1][1] * py).round() as i32)
}

pub(in crate::win::nav) fn learn_npc(world: u8, tile: (i32, i32)) {
    let added = with_learned(|m| {
        let v = m.entry(world).or_default();
        if v.iter().any(|t| cheb(*t, tile) <= 1) {
            return false;
        }
        v.push(tile);
        true
    })
    .unwrap_or(false);
    if added {
        tracing::warn!(world, tile = ?tile, "NPC aprendido no caminho: o cursor vai desviar dele");
        if let Some(Ok(t)) = with_learned(|m| serde_json::to_string_pretty(m)) {
            let tmp = learned_path().with_extension("json.tmp");
            if std::fs::write(&tmp, t).is_ok() {
                let _ = std::fs::rename(&tmp, learned_path());
            }
        }
    }
}

/// Ponto da direção pedida ou, se cair no NPC, girado até sair dele.
pub(in crate::win::nav) fn avoid_npc(pos: (i32, i32), at: impl Fn(f64) -> (i32, i32)) -> (i32, i32) {
    let base = at(0.0);
    if !on_npc(pos, base) {
        return base;
    }
    for extra in [0.6, -0.6, 1.0, -1.0, 1.5, -1.5, 2.0, -2.0] {
        let q = at(extra);
        if !on_npc(pos, q) {
            return q;
        }
    }
    base
}

/// Anda até `target`: segura o botão guiando pela rota até 2 tiles; termina com os
/// cliques existentes e só conclui após confirmar o mapa e o tile exato.
pub fn walk(hwnd: isize, t: &Terrain, target: (i32, i32), status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    walk_with_arrival(hwnd, t, target, WalkArrival::Exact, status)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::win::nav) enum WalkArrival { Exact, Spot, Trainer, Near }

/// Parâmetros da navegação (agent.toml `[navegacao]`; padrão em zen_core::navegacao).
static NAV_TUNING: std::sync::Mutex<Option<zen_core::navegacao::NavTuning>> = std::sync::Mutex::new(None);

pub fn set_nav_tuning(t: zen_core::navegacao::NavTuning) {
    if let Ok(mut g) = NAV_TUNING.lock() {
        *g = Some(t);
    }
}

pub(in crate::win::nav) fn nav_tuning() -> zen_core::navegacao::NavTuning {
    NAV_TUNING.lock().ok().and_then(|g| *g).unwrap_or_default()
}

/// Anda até perto (até 2 tiles) de `target`: patrulha e aproximação de monstro, onde o tile
/// exato não importa e insistir nele com mobs em volta só gastava cliques.
pub fn walk_close(hwnd: isize, t: &Terrain, target: (i32, i32), status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    walk_with_arrival(hwnd, t, target, WalkArrival::Near, status)
}
pub(in crate::win::nav) use zen_core::spots::ARRIVAL_RADIUS as SPOT_ARRIVAL_TOLERANCE;

pub(in crate::win::nav) fn walk_with_arrival(hwnd: isize, t: &Terrain, target: (i32, i32), arrival: WalkArrival, status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    let dest = Dest::from_world(t.world).ok_or("mapa do terreno não reconhecido")?;
    ensure_action_map(hwnd, dest)?;
    walk_legacy(hwnd, t, target, arrival, status)?;
    confirm_destination(hwnd, dest, Some((target, match arrival { WalkArrival::Spot => SPOT_ARRIVAL_TOLERANCE, WalkArrival::Trainer => 1, WalkArrival::Exact => 0, WalkArrival::Near => 2 })))
}

pub(in crate::win::nav) fn final_walk_arrived(pos: (i32, i32), target: (i32, i32), mobs: bool, elapsed_s: u64, arrival: WalkArrival) -> bool {
    pos == target || (arrival == WalkArrival::Near && cheb(pos, target) <= 2) || (arrival == WalkArrival::Trainer && cheb(pos,target) <= 1) || (arrival == WalkArrival::Spot &&
        ((cheb(pos, target) <= 2 && mobs) || (elapsed_s >= 10 && cheb(pos, target) <= 1)))
}

pub(in crate::win::nav) fn walking_route(t: &Terrain, from: (i32,i32), to: (i32,i32), arrival: WalkArrival) -> Option<Vec<(i32,i32)>> {
    if arrival == WalkArrival::Trainer { npc_route(t, from, to) } else { t.path(from, to).or_else(|| t.path_escape(from, to)) }
}

pub(in crate::win::nav) fn walk_legacy(hwnd: isize, t: &Terrain, target: (i32, i32), arrival: WalkArrival, status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    // Obstáculos temporários (mobs parados na rota, lidos na memória): só para o A* deste
    // trecho, numa cópia do terreno. O ATT nunca muda. A cópia só é refeita quando a lista muda.
    let tun = nav_tuning();
    let blocks: std::cell::RefCell<(Vec<(i32, i32)>, (i32, i32), Option<Terrain>)> = std::cell::RefCell::new((Vec::new(), (0, 0), None));
    let route = |from: (i32, i32), to: (i32, i32)| -> Option<Vec<(i32, i32)>> {
        let b = blocks.borrow();
        if let Some(dynt) = b.2.as_ref() {
            if let Some(r) = walking_route(dynt, from, to, arrival) {
                return Some(r);
            }
        }
        walking_route(t, from, to, arrival)
    };
    let set_blocks = |v: Vec<(i32, i32)>, at: (i32, i32)| {
        let mut b = blocks.borrow_mut();
        b.2 = if v.is_empty() { None } else { Some(t.with_blocked(&v)) };
        b.0 = v;
        b.1 = at;
    };
    let t0 = std::time::Instant::now();
    let ms = || t0.elapsed().as_millis() as u64;
    CUR_WORLD.store(t.world, std::sync::atomic::Ordering::SeqCst);
    let mut last = settle(hwnd).ok_or("não li as coordenadas")?;
    let next_wp = |pos: (i32, i32)| -> Option<(f64, f64)> {
        let path = route(pos, target)?;
        let b = blocks.borrow();
        let wp = b.2.as_ref().unwrap_or(t).waypoints(pos, &path, 6)[0];
        Some(((wp.0 - pos.0) as f64, (wp.1 - pos.1) as f64))
    };
    // Vigia de progresso pela coordenada real (zen_core::navegacao::Progress).
    let mut prog = zen_core::navegacao::Progress::new(ms(), last, target, route(last, target).map(|p| p.len()).unwrap_or(usize::MAX), tun);
    let mut local_detours = 0u32;
    let mut mob_seen: std::collections::HashSet<(i32, i32)> = Default::default();
    // ---- segurar e guiar
    let mut detours = 0u32;
    let mut angle = 0.0f64;
    let (mut clears, mut since_clear) = (0u32, 2u32);
    let mut click_rounds = 0u32;
    let mut vault_opens = 0u32;
    // Mob parado na rota (dono, 11/11): ataca ELE e segue. Cada tile de bloqueio no máximo 3
    // vezes (mob que não morre / renasce: aí desvia como antes, sem ficar empurrando).
    let mut hits: std::collections::HashMap<(i32, i32), u32> = Default::default();
    'hold: while cheb(last, target) > 2 {
        crate::win::spots::check_approach_at(hwnd, t.world, last, false)?;
        if t0.elapsed().as_secs() > 240 {
            return Err("tempo esgotado andando (4 min)".into());
        }
        let d = next_wp(last).ok_or_else(|| format!("sem rota de {last:?} até {target:?}"))?;
        refresh_live_npcs(hwnd, last);
        let (hx, hy) = hold_px(last, d, angle);
        let mut h = input::Hold::start(hwnd, hx, hy)?;
        let mut cur_px = (hx, hy);
        let mut still = 0;
        let mut best = route(last, target).map(|p| p.len()).unwrap_or(usize::MAX);
        loop {
            sleep(250);
            if t0.elapsed().as_secs() > 240 {
                return Err("tempo esgotado andando (4 min)".into());
            }
            crate::win::spots::check_movement_epoch()?;
            let Some(p) = position(hwnd).filter(|p| cheb(*p, last) <= 4) else { continue };
            crate::win::spots::check_approach_at(hwnd, t.world, p, false)?;
            if p == last {
                still += 1;
            } else {
                still = 0;
            }
            last = p;
            if cheb(p, target) <= 2 {
                break 'hold;
            }
            {
                let far = { let b = blocks.borrow(); !b.0.is_empty() && cheb(b.1, p) >= tun.forget_blocks_after_tiles };
                if far {
                    set_blocks(Vec::new(), p);
                }
            }
            let remaining = route(p, target).map(|v| v.len()).unwrap_or(usize::MAX);
            match prog.update(ms(), p, remaining) {
                zen_core::navegacao::Verdict::NoProgress { .. } => {
                    h.release();
                    drop(h);
                    let m = format!("sem progresso: {}", prog.describe(ms()));
                    tracing::warn!(hwnd, "{m}");
                    save_error_shot(hwnd, &m);
                    return Err(m);
                }
                zen_core::navegacao::Verdict::Stalled { ms: parado } if local_detours < tun.max_local_detours => {
                    // Etapa 2/3: travado com mob em cima da rota -> esse tile vira obstáculo
                    // temporário e o A* desvia por células livres (sem clicar no mob).
                    let path = route(p, target);
                    let mobs = mob_tiles(hwnd, p);
                    if let (Some(path), Some(mobs)) = (path, mobs) {
                        if let Some(b) = zen_core::combat::blocking_mob(p, &path, &mobs, 4, 4) {
                            local_detours += 1;
                            if mob_seen.insert(b) {
                                tracing::info!(hwnd, posicao = ?p, mob = ?b, parado_ms = parado, destino = ?target, "navegação: mob parado na rota (desvio local {local_detours}/{})", tun.max_local_detours);
                            }
                            let mut v = blocks.borrow().0.clone();
                            v.extend(zen_core::navegacao::mob_blocks(&[b], tun.mob_block_radius, &[target, p]));
                            set_blocks(v, p);
                            status(format!("desviando de mob em {b:?}"));
                            prog.pause(ms());
                            h.release();
                            drop(h);
                            continue 'hold;
                        }
                    }
                }
                _ => {}
            }
            // Perto do spot (até 4 tiles) não: ali os mobs empilham e a limpeza do spot cuida.
            if still >= 2 && clears < 8 && cheb(p, target) > 4 && skill_ok(hwnd, t, p) {
                if let Some(b) = blocker(hwnd, t, p, target, 3, 3).filter(|b| hits.get(b).copied().unwrap_or(0) < 3) {
                    h.release();
                    drop(h);
                    *hits.entry(b).or_insert(0) += 1;
                    if clear_mobs(hwnd, t, 2000, Some(b), status) {
                        clears += 1;
                        since_clear = 0;
                    }
                    prog.pause(ms());
                    continue 'hold;
                }
            }
            if still >= 3 && !storage_open(hwnd) && (npc_dialog_open(hwnd) || inventory_open(hwnd)) {
                h.release();
                drop(h);
                status(format!("janela de NPC/loja abriu no caminho em {p:?}: fechando (ESC) e mudando a direção"));
                tracing::warn!(hwnd, posicao = ?p, npc = npc_dialog_open(hwnd), inventario = inventory_open(hwnd), "navegação: janela de NPC/loja aberta no caminho");
                learn_npc(t.world, tile_at_px(p, cur_px));
                close_npc_dialog(hwnd);
                close_stray_shop(hwnd);
                vault_opens += 1;
                detours += 1;
                angle = [0.9, -0.9, 1.4, -1.4, 0.5, -0.5][(vault_opens as usize) % 6];
                if vault_opens >= 6 {
                    return Err(format!("janelas de NPC abriram {vault_opens} vezes perto de {p:?}"));
                }
                continue 'hold;
            }
            if still >= 3 && storage_open(hwnd) {
                h.release();
                drop(h);
                status(format!("baú abriu no caminho em {p:?}: fechando (ESC) e mudando a direção"));
                learn_npc(t.world, tile_at_px(p, cur_px));
                force_close_storage(hwnd);
                // NUNCA segurar de novo na mesma direção (era o laço do ReDMooN às 21:19).
                vault_opens += 1;
                detours += 1;
                angle = [0.9, -0.9, 1.4, -1.4, 0.5, -0.5][(vault_opens as usize) % 6];
                if vault_opens >= 6 {
                    return Err(format!("o baú abriu {vault_opens} vezes perto de {p:?}"));
                }
                continue 'hold;
            }
            if still >= 3 || remaining > best + 2 {
                detours += 1;
                if detours > 25 {
                    // Segurar não anda (x9901 em (27,217) às 20:03-20:11, 4 falhas seguidas),
                    // mas cliques soltos andavam. Então: 12 cliques pela rota e volta a segurar.
                    // Até 3 rodadas antes de desistir.
                    if click_rounds >= 3 {
                        return Err(format!("desvios demais perto de {p:?} (3 rodadas de cliques)"));
                    }
                    click_rounds += 1;
                    h.release();
                    drop(h);
                    status(format!("segurar não anda em {p:?}: 12 cliques pela rota ({click_rounds}/3)"));
                    let mut cur = p;
                    for k in 0..12 {
                        force_close_storage(hwnd);
                        close_npc_dialog(hwnd);
                        let Some(path) = route(cur, target) else { break };
                        let wp = t.waypoints(cur, &path, 6)[0];
                        let turn = [0.0, 0.4, -0.4][k % 3];
                        let dd = ((wp.0 - cur.0) as f64, (wp.1 - cur.1) as f64);
                        let (sx, sy) = avoid_npc(cur, |a| to_pixels(dd, turn + a, None, 90.0, 230.0));
                        input::run(hwnd, &[Step::Click { x: sx, y: sy }])?;
                        sleep(450);
                        if let Some(n) = near(hwnd, cur) {
                            cur = n;
                        }
                        if cheb(cur, target) <= 2 {
                            break;
                        }
                    }
                    last = cur;
                    detours = 0;
                    continue 'hold;
                }
                // Parado (não só desviando): mob na frente. Com skill configurada, solta o
                // botão, limpa com a skill e segue; a cada 3 travadas, no máximo 6 vezes.
                // Só quando DÁ para usar a skill (tecla configurada e fora da zona segura).
                // Antes (19:00-20:55) entrava aqui também na zona segura, não usava nada e
                // voltava a segurar NA MESMA DIREÇÃO, pulando o desvio de ângulo abaixo: o char
                // ficava empurrando a quina da safe do Kanturu até desistir (dono, 20:54).
                if still >= 3 && clears < 8 && since_clear >= 2 && can_clear(hwnd, t, p) {
                    h.release();
                    drop(h);
                    if clear_mobs(hwnd, t, 2500, None, status) {
                        clears += 1;
                        since_clear = 0;
                    }
                    prog.pause(ms());
                    continue 'hold;
                }
                since_clear += 1;
                angle = [0.5, -0.5, 0.9, -0.9, 0.0][(detours as usize) % 5];
                h.release();
                drop(h);
                continue 'hold;
            }
            best = best.min(remaining);
            if let Some(d) = next_wp(p) {
                let (sx, sy) = hold_px(p, d, 0.0);
                h.steer(sx, sy)?;
                cur_px = (sx, sy);
            }
            status(format!("andando: {p:?} -> {target:?}"));
        }
    }
    sleep(400);
    last = settle(hwnd).unwrap_or(last);
    // ---- cliques finais
    let c0 = std::time::Instant::now();
    let mut final_mobs = None;
    let mut stuck = 0usize;
    for _ in 0..60 {
        crate::win::spots::check_movement_epoch()?;
        let pos = near(hwnd, last).ok_or("leitura da posição implausível")?;
        crate::win::spots::check_approach_at(hwnd, t.world, pos, false)?;
        // Uma leitura de mobs ao entrar nos dois tiles finais, nunca por clique.
        if arrival == WalkArrival::Spot && pos != target && cheb(pos, target) <= 2 && final_mobs.is_none() {
            final_mobs = Some(mob_tiles(hwnd, pos).is_some_and(|m| zen_core::combat::mobs_within(target, &m, 2) > 0));
        }
        if final_walk_arrived(pos, target, final_mobs.unwrap_or(false), c0.elapsed().as_secs(), arrival) {
            return Ok(pos);
        }
        if arrival == WalkArrival::Spot && c0.elapsed().as_secs() >= 10 {
            return Err(format!("ajuste final limitado a 10 s: parou em {pos:?}, destino {target:?}"));
        }
        let path = route(pos, target).ok_or_else(|| format!("sem rota de {pos:?} até {target:?}"))?;
        let wp = t.waypoints(pos, &path, 6)[0];
        let turn = [0.0, 0.5, -0.5, 1.0, -1.0][stuck % 5];
        let dd = ((wp.0 - pos.0) as f64, (wp.1 - pos.1) as f64);
        let (sx, sy) = if arrival == WalkArrival::Trainer { npc_step_px(pos, wp) } else { avoid_npc(pos, |a| to_pixels(dd, turn + a, None, 90.0, 230.0)) };
        input::run(hwnd, &[Step::Click { x: sx, y: sy }])?;
        sleep(350);
        let new = near(hwnd, pos).ok_or("leitura da posição implausível")?;
        if let zen_core::navegacao::Verdict::NoProgress { .. } = prog.update(ms(), new, route(new, target).map(|p| p.len()).unwrap_or(usize::MAX)) {
            let m = format!("sem progresso no ajuste final: {}", prog.describe(ms()));
            tracing::warn!(hwnd, "{m}");
            save_error_shot(hwnd, &m);
            return Err(m);
        }
        if new == pos {
            stuck += 1;
            if force_close_storage(hwnd) {
                status(format!("baú abriu no caminho em {pos:?}: fechado com ESC"));
                learn_npc(t.world, tile_at_px(pos, (sx, sy)));
            } else if close_npc_dialog(hwnd) {
                status(format!("janela de NPC abriu em {pos:?}: fechada com ESC"));
                learn_npc(t.world, tile_at_px(pos, (sx, sy)));
            }
            if stuck == 3 || stuck == 6 {
                clear_mobs(hwnd, t, 2500, None, status);
            }
            if stuck >= 8 {
                return Err(format!("travou em {pos:?} (8 cliques sem andar)"));
            }
        } else {
            stuck = 0;
        }
        last = new;
    }
    Err("cliques demais sem chegar".into())
}

/// Terreno do Kanturu 1 a partir da pasta do jogo (caminho do .exe do processo).
pub fn kanturu_terrain(pid: u32) -> Result<Terrain, String> {
    let exe = crate::win::memory::process_path(pid).ok_or("não achei o executável do jogo")?;
    let dir = std::path::Path::new(&exe).parent().ok_or("caminho do jogo inválido")?;
    let f = dir.join(format!("Data\\World{KANTURU_WORLD}\\EncTerrain{KANTURU_WORLD}.att"));
    let raw = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    let t = Terrain::from_client_file(&raw).map_err(|e| e.to_string())?;
    if t.world != KANTURU_WORLD {
        return Err(format!("arquivo de terreno é do World{}, esperado {KANTURU_WORLD}", t.world));
    }
    Ok(t)
}

/// Fluxo completo. Em caso de falha no meio do caminho (fora da zona segura), religa o helper
/// onde estiver para o char pelo menos farmar ali.
pub fn recover(hwnd: isize, t: &Terrain, spot: (i32, i32), status: &dyn Fn(String)) -> Result<String, String> {
    recover_to(hwnd, t, spot, Dest::Kanturu, status)
}

/// Volta ao spot no Kanturu (`arena` = false) ou na Arena (`t` = terreno do mapa do spot).
pub fn recover_to(hwnd: isize, t: &Terrain, spot: (i32, i32), dest: Dest, status: &dyn Fn(String)) -> Result<String, String> {
    recover_in(hwnd, t, spot, dest, status)
}

/// Mapas validados (v4, Etapa 3): número do World -> destino do M. Os outros mapas só são
/// liberados depois de calibrados (nome no canto + botão do M conferidos).
pub fn dest_of_world(world: u8) -> Option<Dest> {
    Dest::from_world(world)
}

/// Decodifica o mesmo quadro capturado no evento; não faz nova captura em caso de erro.
pub fn decode_spot_frame(frame: Option<crate::win::capture::Image>) -> (Option<&'static str>, Option<(i32, i32)>) {
    let Some(img) = frame else { return (None, None) };
    let bits = binarize_white(&img.bgra, img.w as usize, img.h as usize, COORDS_WHITE);
    let pos = read_coords(&bits).map(|(x,y)| (x as i32,y as i32));
    (zen_core::glyphs::map_name(&bits), pos)
}

/// Painel: usa o recorte visível quando possível; janelas cobertas usam PrintWindow.
pub fn observe_spot_now(hwnd: isize) -> (Option<&'static str>, Option<(i32, i32)>) {
    if unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(windows::Win32::Foundation::HWND(hwnd as *mut _)) }.as_bool() {
        return (None, None);
    }
    let frame = capture::visible_region(hwnd, CORNER).or_else(|| capture::capture(hwnd, Some(CORNER)));
    decode_spot_frame(frame)
}

/// Nome do mapa no canto (só os que o agente conhece) e a posição.
pub fn observe_named(hwnd: isize) -> (Option<&'static str>, Option<(i32, i32)>) {
    let Some(b) = corner(hwnd) else { return (None, None) };
    let pos = read_coords(&b).map(|(x, y)| (x as i32, y as i32));
    if pos.is_none() {
        return (None, None);
    }
    let name = zen_core::glyphs::map_name(&b);
    (name, pos)
}

/// Mapa e coordenada precisam vir do mesmo quadro, sem inferir posição ausente.
pub(in crate::win::nav) fn destination_matches(snap: (Option<&str>, Option<(i32, i32)>), dest: Dest, target: Option<((i32, i32), i32)>) -> bool {
    snap.0 == Some(dest.name()) && snap.1.is_some_and(|p| {
        (0..=255).contains(&p.0) && (0..=255).contains(&p.1)
            && target.map_or(true, |(q, tolerance)| cheb(p,q) <= tolerance)
    })
}

pub(in crate::win::nav) fn ensure_action_map(hwnd: isize, dest: Dest) -> Result<(i32, i32), String> {
    if observe_named(hwnd).0 != Some(dest.name()) { teleport(hwnd, dest)?; }
    confirm_destination(hwnd, dest, None)
}

pub(in crate::win::nav) fn confirm_destination(hwnd: isize, dest: Dest, target: Option<((i32, i32), i32)>) -> Result<(i32, i32), String> {
    let mut last = (None, None);
    for attempt in 1..=3 {
        last = observe_named(hwnd);
        if destination_matches(last, dest, target) {
            tracing::info!(hwnd, mapa = dest.name(), posicao = ?last.1, esperado = ?target, "movimento: destino confirmado");
            return Ok(last.1.unwrap());
        }
        tracing::warn!(hwnd, attempt, mapa_esperado = dest.name(), destino = ?target, mapa_lido = ?last.0, posicao = ?last.1, "movimento: destino não confirmado; relendo antes de avançar");
        if input::manual_mode() { return Err("validação interrompida pelo dono".into()); }
        if attempt < 3 { sleep(250); }
    }
    Err(format!("destino não confirmado: esperado {} {target:?}, lido {:?} {:?}", dest.name(), last.0, last.1))
}

/// Reusa as rotinas de fechamento existentes; não introduz outro controle de mouse.
pub fn prepare_route_retry(hwnd: isize, status: &dyn Fn(String)) -> Result<(),String> {
    if input::manual_mode() {return Err("retentativa pausada pelo dono".into());}
    tracing::warn!(hwnd,mapa_m=map_open(hwnd),inventario=inventory_open(hwnd),npc=npc_dialog_open(hwnd),bau=storage_open(hwnd),"análise antes de repetir movimento");
    status("movimento não confirmado: fechando obstruções antes de uma retentativa".into());
    force_close_storage(hwnd);
    close_npc_dialog(hwnd);
    close_stray_shop(hwnd);
    if map_open(hwnd) { input::run(hwnd,&[Step::Key(VK_M)])?;sleep(300); }
    Ok(())
}

pub(in crate::win::nav) fn in_dest(hwnd: isize, d: Dest) -> (Option<bool>, Option<(i32, i32)>) {
    let (name, pos) = observe_named(hwnd);
    (pos.map(|_| name == Some(d.name())), pos)
}
