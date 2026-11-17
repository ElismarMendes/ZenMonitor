use crate::win::nav::*;

/// Modelo pixel -> tile medido em 08/11 (~1 tile a cada 80-95 px).
pub(in crate::win::nav) const MODEL: [[f64; 2]; 2] = [[0.01056, 0.00878], [0.00688, -0.01186]];

pub(in crate::win::nav) fn to_pixels(d: (f64, f64), angle: f64, radius: Option<f64>, min_len: f64, max_len: f64) -> (i32, i32) {
    let m = MODEL;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let (mut px, mut py) = ((m[1][1] * d.0 - m[0][1] * d.1) / det, (-m[1][0] * d.0 + m[0][0] * d.1) / det);
    if angle != 0.0 {
        let (c, s) = (angle.cos(), angle.sin());
        (px, py) = (px * c - py * s, px * s + py * c);
    }
    let len = (px * px + py * py).sqrt().max(1.0);
    let k = match radius {
        Some(r) => r / len,
        None => (max_len / len).min(1.0).max(min_len / len),
    };
    let (mut px, mut py) = (px * k, py * k);
    let mut tries = 0;
    while blocked(CENTER.0 + px as i32, CENTER.1 + py as i32) && tries < 4 {
        px *= 0.7;
        py *= 0.7;
        tries += 1;
    }
    (CENTER.0 + px.round() as i32, CENTER.1 + py.round() as i32)
}

// ---------------------------------------------------------------- limpar mobs com a skill
// Ideia do dono (10/11 18:48), testada no x9901: tecla da skill de área (Ice Storm na tecla 1)
// e o botão direito SEGURADO em cima do char por alguns segundos matam os mobs em volta. É bem
// mais rápido que ligar o helper (que arrasta o char para longe da rota). A tecla é por char
// (no ReDMooN a mesma skill está na 2); sem tecla configurada, fica o helper como antes.

pub(in crate::win::nav) static CLEAR_SKILL: std::sync::Mutex<Option<std::collections::HashMap<isize, u8>>> = std::sync::Mutex::new(None);

/// Tecla (1..9) da skill de limpar deste char; 0 = nenhuma.
pub fn set_clear_skill(hwnd: isize, key: u8) {
    if let Ok(mut m) = CLEAR_SKILL.lock() {
        let m = m.get_or_insert_with(Default::default);
        if key == 0 {
            m.remove(&hwnd);
        } else {
            m.insert(hwnd, key);
        }
    }
}

pub(in crate::win::nav) static CHAR_NAMES: std::sync::Mutex<Option<std::collections::HashMap<isize, String>>> = std::sync::Mutex::new(None);

/// Nome do char de cada janela (para achar a entrada dele na lista de personagens da memória).
pub fn set_char_name(hwnd: isize, name: &str) {
    if let Ok(mut m) = CHAR_NAMES.lock() {
        let m = m.get_or_insert_with(Default::default);
        if m.get(&hwnd).map(String::as_str) != Some(name) {
            m.insert(hwnd, name.to_string());
        }
    }
}

/// Mobs e NPCs em volta, da memória (só leitura). `None` = não deu para ler.
pub(in crate::win::nav) fn objects_near(hwnd: isize, pos: (i32, i32)) -> Option<Vec<zen_core::objects::Obj>> {
    let name = CHAR_NAMES.lock().ok()?.as_ref()?.get(&hwnd)?.clone();
    crate::win::objects::nearby_hwnd(hwnd, &name, pos)
}

/// Algum mob a até `r` tiles? `None` = não sabe (sem leitura da memória).
pub(in crate::win::nav) fn mob_near(hwnd: isize, pos: (i32, i32), r: i32) -> Option<bool> {
    let v = objects_near(hwnd, pos)?;
    Some(v.iter().any(|o| o.kind == zen_core::objects::Kind::Monster && cheb((o.x, o.y), pos) <= r))
}

pub(in crate::win::nav) fn clear_skill(hwnd: isize) -> Option<u8> {
    CLEAR_SKILL.lock().ok()?.as_ref()?.get(&hwnd).copied()
}

/// Dá para limpar com a skill aqui? (tecla configurada e fora da zona segura)
pub(in crate::win::nav) fn can_clear(hwnd: isize, t: &Terrain, p: (i32, i32)) -> bool {
    // Com a lista da memória: só usa a skill se houver mob a até 3 tiles (senão a travada é
    // parede/NPC e o certo é desviar). Sem a lista, como antes. Sem tecla configurada, tenta
    // achar o Ice Storm sozinho (a não ser que já tenha procurado e não achado neste char).
    let has = clear_skill(hwnd).is_some() || !no_ice_recently(hwnd);
    has && !t.is_safezone(p.0, p.1) && mob_near(hwnd, p, 3).unwrap_or(clear_skill(hwnd).is_some())
}

// ---------------------------------------------------------------- Ice Storm certo (10/11 23:56)
// O bot olha a caixa da skill selecionada (barra de baixo) e compara com o ícone do Ice Storm
// tirado do próprio cliente. Se a tecla configurada não mostrar o Ice Storm, aperta 1..9 até
// achar e grava a tecla do char. Alcance e área (Skill_eng.bmd do cliente + servidor):
// lança a até ICE_RANGE tiles do char e acerta tudo a até ICE_RADIUS tiles do ponto.

pub(in crate::win::nav) const ICE_RANGE: i32 = 6;
pub(in crate::win::nav) const ICE_RADIUS: i32 = 4;

pub(in crate::win::nav) static LEARNED_KEYS: std::sync::Mutex<Vec<(isize, u8)>> = std::sync::Mutex::new(Vec::new());
pub(in crate::win::nav) static NO_ICE: std::sync::Mutex<Option<std::collections::HashMap<isize, std::time::Instant>>> = std::sync::Mutex::new(None);

/// Teclas descobertas desde a última chamada (janela, tecla; 0 = este char não tem Ice Storm).
pub fn take_learned_keys() -> Vec<(isize, u8)> {
    LEARNED_KEYS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

pub(in crate::win::nav) fn no_ice_recently(hwnd: isize) -> bool {
    NO_ICE.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&hwnd).map(|t| t.elapsed().as_secs() < 1800))).unwrap_or(false)
}

/// A skill selecionada agora é o Ice Storm? `None` = não deu para ver a tela.
pub fn selected_is_ice(hwnd: isize) -> Option<bool> {
    use zen_core::skillicon::{gray_of_bgra, ice_storm_score, MIN_SCORE, REGION};
    let img = capture::capture(hwnd, Some(REGION))?;
    let g = gray_of_bgra(&img.bgra, img.w as usize, img.h as usize);
    Some(ice_storm_score(&g, img.w as usize, img.h as usize) >= MIN_SCORE)
}

/// Deixa o Ice Storm selecionado e devolve a tecla dele (aprende se precisar).
pub(in crate::win::nav) fn ensure_ice(hwnd: isize, status: &dyn Fn(String)) -> Option<u8> {
    let press = |k: u8| -> Option<bool> {
        input::run(hwnd, &[Step::Key(0x30 + u16::from(k % 10))]).ok()?;
        sleep(250);
        selected_is_ice(hwnd)
    };
    // Tecla conhecida: se apertar e o ícone for Ice Storm, usa (o caso comum: ela já foi
    // aprendida em duas fases ou configurada no painel).
    if let Some(k) = clear_skill(hwnd) {
        if press(k) == Some(true) {
            return Some(k);
        }
    }
    if no_ice_recently(hwnd) {
        return None;
    }
    // Duas fases (10/11 00:20: no ReDMooN a tecla 1 é VAZIA; apertar tecla vazia não troca a
    // skill e o ícone continuava Ice Storm, dando a tecla errada):
    // 1) aperta até sair do Ice Storm (alguma tecla com outra skill);
    // 2) a tecla que VOLTA para o Ice Storm é a dele (tecla vazia não muda nada).
    status("procurando a tecla do Ice Storm (1 a 9) pelo ícone da skill".into());
    let mut off_ice = selected_is_ice(hwnd) == Some(false);
    if !off_ice {
        for k in 1..=9u8 {
            if press(k) == Some(false) {
                off_ice = true;
                break;
            }
        }
    }
    let found = if off_ice {
        (1..=9u8).find(|&k| press(k) == Some(true))
    } else {
        // Nenhuma tecla tira do Ice Storm: só ele (e teclas vazias). Fica a configurada, se houver.
        clear_skill(hwnd)
    };
    if let Some(k) = found {
        set_clear_skill(hwnd, k);
        if let Ok(mut v) = LEARNED_KEYS.lock() {
            v.push((hwnd, k));
        }
        tracing::warn!(tecla = k, "Ice Storm achado pelo ícone");
        return Some(k);
    }
    if let Ok(mut g) = NO_ICE.lock() {
        g.get_or_insert_with(Default::default).insert(hwnd, std::time::Instant::now());
    }
    tracing::warn!("Ice Storm não está em nenhuma tecla de 1 a 9 deste char");
    None
}

/// Para o diag: acha/confere a tecla do Ice Storm e diz onde miraria (sem soltar a skill).
pub fn probe_ice(hwnd: isize) -> (Option<u8>, Option<((i32, i32), (i32, i32), usize)>) {
    let key = ensure_ice(hwnd, &|m| println!("  {m}"));
    let aim = position(hwnd).map(|p| {
        let (t, n) = ice_target(hwnd, p, None);
        (p, t, n)
    });
    (key, aim)
}

/// Tiles dos mobs em volta (memória, só leitura). `None` = sem leitura.
pub(in crate::win::nav) fn mob_tiles(hwnd: isize, pos: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    Some(objects_near(hwnd, pos)?.into_iter().filter(|o| o.kind == zen_core::objects::Kind::Monster).map(|o| (o.x, o.y)).collect())
}

/// Melhor ponto para o Ice Storm (`combat::aim`): no mob que bloqueia (`focus`), ou no ponto
/// com mais mobs no raio dentro do alcance. Sem lista de mobs: em cima do char.
pub(in crate::win::nav) fn ice_target(hwnd: isize, pos: (i32, i32), focus: Option<(i32, i32)>) -> ((i32, i32), usize) {
    let mobs = mob_tiles(hwnd, pos).unwrap_or_default();
    zen_core::combat::aim(pos, &mobs, focus, ICE_RANGE, ICE_RADIUS)
}

/// Mob parado na rota daqui até `target` (até 3 tiles à frente). `None` = livre ou sem leitura.
pub(in crate::win::nav) fn blocker(hwnd: isize, t: &Terrain, p: (i32, i32), target: (i32, i32), look: usize, reach: i32) -> Option<(i32, i32)> {
    let path = t.path(p, target)?;
    let mobs = mob_tiles(hwnd, p)?;
    zen_core::combat::blocking_mob(p, &path, &mobs, look, reach)
}

/// Tem como soltar a skill aqui (tecla conhecida ou ainda dá para procurar; fora da safe)?
pub(in crate::win::nav) fn skill_ok(hwnd: isize, t: &Terrain, p: (i32, i32)) -> bool {
    (clear_skill(hwnd).is_some() || !no_ice_recently(hwnd)) && !t.is_safezone(p.0, p.1)
}

/// Segura a skill de área em cima do char por `ms` (só fora da zona segura). Devolve se usou.
pub(in crate::win::nav) fn clear_mobs(hwnd: isize, t: &Terrain, ms: u32, focus: Option<(i32, i32)>, status: &dyn Fn(String)) -> bool {
    let Some(p) = position(hwnd) else { return false };
    if t.is_safezone(p.0, p.1) {
        return false;
    }
    let Some(key) = ensure_ice(hwnd, status) else {
        // Sem Ice Storm na barra (AMUMU 16/11 06:43 e 08:09: morreu andando até o spot com o
        // helper desligado, logo depois deste aviso). Liga o helper alguns segundos para se
        // defender e desliga para seguir a caminhada.
        status(format!("mobs em {p:?} e sem Ice Storm: helper ligado {:.1} s para se defender", ms.max(3000) as f64 / 1000.0));
        tracing::warn!(hwnd, posicao = ?p, "caminhada: sem Ice Storm; helper ligado para se defender");
        if set_helper(hwnd, true).is_err() { return false; }
        sleep(ms.max(3000) as u64);
        let _ = set_helper(hwnd, false);
        return true;
    };
    // Mira: o ponto com mais mobs no raio do Ice Storm (dentro do alcance); senão, o char.
    let (tile, n) = ice_target(hwnd, p, focus);
    let mut px = if tile == p { CENTER } else { npc_px(p, (tile.0 as f64 + 0.5, tile.1 as f64 + 0.5)) };
    if blocked(px.0, px.1) {
        px = CENTER;
    }
    let why = if focus.is_some() { "mob no caminho" } else { "limpando" };
    status(format!("{why} em {p:?}: Ice Storm (tecla {key}) em {tile:?} pegando {n} mob(s), {:.1} s", ms as f64 / 1000.0));
    let r = input::run(hwnd, &[Step::RightHold { x: px.0, y: px.1, ms }]);
    sleep(300);
    r.is_ok()
}

/// Distância do cursor ao centro enquanto anda segurando o botão. 170 px (10/11 19:30) foi
/// um erro: para a esquerda o ponto cai na faixa `blocked` (x 80..390), é encolhido até ~40 px
/// e o char não sai do lugar (x9901 parado em (27,217) às 20:01). Fica 120 px, o valor testado.
pub(in crate::win::nav) const HOLD_RADIUS: f64 = 120.0;

/// Ponto do cursor para andar segurando, desviando do Safety Guardian do baú do Kanturu: o
/// botão segurado em cima dele abre o baú (x9901 em 10/11 21:03: abriu 3 vezes saindo da
/// quina (27,217)). Perto do baú, se o ponto cair a menos de 60 px do NPC, gira a direção.
pub(in crate::win::nav) fn hold_px(pos: (i32, i32), d: (f64, f64), angle: f64) -> (i32, i32) {
    avoid_npc(pos, |a| to_pixels(d, angle + a, Some(HOLD_RADIUS), 0.0, 0.0))
}
