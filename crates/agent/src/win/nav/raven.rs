use super::*;

/// Tile de onde se fala com o Trainer (o x9901 abriu a janela daqui).
pub const TRAINER_STAND: (i32, i32) = (122, 108);
/// Tile do Trainer pelo guia e pela memória; a memória manda quando der para ler.
pub(super) const TRAINER_TILE: (i32, i32) = (122, 110);
pub(super) const TRAINER_RESTORE_BTN: (i32, i32) = (513, 280);
pub(super) const TRAINER_RAVEN_BTN: (i32, i32) = (513, 328);
pub(super) const TRAINER_CLOSE_BTN: (i32, i32) = (513, 421);
/// Textos da janela (branco, limiar 150), com a origem no print de 1030x797.
pub(super) const TRAINER_TITLE: ((i32, i32), &str) = ((495, 140), TITLE_TXT);
pub(super) const TRAINER_RESTORE: ((i32, i32), &str) = ((464, 276), RESTORE_TXT);
pub(super) const TRAINER_SELECT: ((i32, i32), &str) = ((450, 191), SELECT_TXT);
pub(super) const TITLE_TXT: &str = "\
.........................................
.######............##....................
...##....................................
...##...##.#..###..##.####...###..##.#...
...##...####....##.##.##.##.##.##.####...
...##...##....####.##.##.##.#####.##.....
...##...##...##.##.##.##.##.##....##.....
...##...##....####.##.##.##..####.##.....
.........................................";
pub(super) const RESTORE_TXT: &str = "\
........................................................................................................
.....................................#....##..........#.....#................#.......#............#.....
.###............#....................#.#.#...........#......#................#.....#.#.#.#.........#....
.#..#...........#....................#...#..........#.......#................#.......#...#..........#...
.#..#...##..###.##..###..#.#..##.....#.#.###.##.....#....####.#...#.#.#..##..####..#.#.#.##.#...#...#...
.###...#..#.#...#..#...#.##..#..#....#.#.#..#..#....#...#...#.#...#.##.....#.#...#.#.#.#.#...#.#....#...
.#.#...####..#..#..#...#.#...####....#.#.#..####....#...#...#.#...#.#....###.#...#.#.#.#.#...#.#....#...
.#..#..#......#.#..#...#.#...#.......#.#.#..#.......#...#...#.#...#.#...#..#.#...#.#.#.#.#...#.#....#...
.#...#..###.###..#..###..#....###....#.#.#...###....#....####..####.#....###.####..#.#.#..#...#.....#...
.....................................................#........................................#....#....
......................................................#......................................#....#.....";
pub(super) const SELECT_TXT: &str = "\
......................................................................................................................................
.............#....................#................................................................................#....##............
...####......#...........#.....#..#........................#.....#.................................................#.#.#..............
..#..........#...........#.....#..#........................#.....#.................................................#...#..............
..#......##..#..##...###.##....##.####...##.....####...##..##....##..###.....#.#..##...###..###..#...#..##..#.#....#.#.###.##.........
...###..#..#.#.#..#.#....#.....#..#...#.#..#....#...#.#..#.#.....#..#...#....##..#..#.#....#...#..#.#..#..#.##.....#.#.#..#..#........
......#.####.#.####.#....#.....#..#...#.####....#...#.####.#.....#..#...#....#...####.#....#...#..#.#..####.#......#.#.#..####........
......#.#....#.#....#....#.....#..#...#.#.......#...#.#....#.....#..#...#....#...#....#....#...#..#.#..#....#......#.#.#..#...........
..####...###.#..###..###..#.....#.#...#..###....####...###..#.....#..###.....#....###..###..###....#....###.#......#.#.#...###........
................................................#.....................................................................................";

/// O texto `t` está na tela perto de `origin` (até 6 px de deslocamento)?
pub(super) fn trainer_text(hwnd: isize, (origin, txt): ((i32, i32), &str)) -> bool {
    let t = Bits::from_text(txt);
    let s = 6i32;
    let Some(img) = capture::capture(hwnd, Some([origin.0 - s, origin.1 - s, t.w as i32 + 2 * s, t.h as i32 + 2 * s])) else { return false };
    trainer_pattern(&img, &t)
}
pub(super) fn trainer_pattern(img: &capture::Image, t: &Bits) -> bool {
    let b = binarize_white(&img.bgra, img.w as usize, img.h as usize, COORDS_WHITE);
    if b.w < t.w || b.h < t.h { return false; }
    let ones = t.px.iter().filter(|&&p| p).count();
    let tol = (ones / 8).max(6);
    for oy in 0..=(b.h.saturating_sub(t.h)) {
        for ox in 0..=(b.w.saturating_sub(t.w)) {
            let mut d = 0;
            'm: for yy in 0..t.h {
                for xx in 0..t.w {
                    if b.px[(oy + yy) * b.w + ox + xx] != t.px[yy * t.w + xx] {
                        d += 1;
                        if d > tol {
                            break 'm;
                        }
                    }
                }
            }
            if d <= tol {
                return true;
            }
        }
    }
    false
}

/// Vida do Dark Raven agora (barra do topo). `None` = sem Raven ou não deu para ler.
pub(super) fn confirm_raven_read(mut read: impl FnMut() -> Option<u8>, mut pause: impl FnMut()) -> Option<u8> {
    for attempt in 0..3 {
        if let Some(hp) = read() { return Some(hp); }
        if attempt < 2 { pause(); }
    }
    None
}
pub fn raven_confirmed(hwnd: isize) -> Option<u8> { confirm_raven_read(|| raven_now(hwnd), || sleep(150)) }
pub(super) fn raven_request_allowed(manual: bool, hp: u8) -> bool { manual || hp <= 50 }

pub fn raven_now(hwnd: isize) -> Option<u8> {
    let [x, y, w, h] = zen_core::petbar::RAVEN_SCAN;
    let img = capture::capture(hwnd, Some([x, y, w, h]))?;
    zen_core::petbar::raven_in_scan(&img.bgra, img.w as usize, img.h as usize)
}

pub(super) fn wait_for(hwnd: isize, what: ((i32, i32), &str), ms: u64) -> bool {
    let t0 = std::time::Instant::now();
    loop {
        if trainer_text(hwnd, what) {
            return true;
        }
        if t0.elapsed().as_millis() as u64 >= ms {
            return false;
        }
        sleep(250);
    }
}

/// Resultado da restauração: vida do Raven antes e depois (%).
#[derive(Debug, Clone, Copy)]
pub struct RavenReport {
    pub before: Option<u8>,
    pub after: u8,
}

/// De onde o clique no Trainer funciona: até 3 tiles do NPC (só vale em Lorencia).
pub(super) const TRAINER_CLICK_RANGE: i32 = 3;

/// Garante que o char está em Lorencia e perto do Trainer, lendo a posição de verdade. Perto
/// (até 2 tiles) = não anda. Longe = caminhada completa com desvio. Se a rota falhar (tile de
/// spawn que não deixa traçar rota, p.ex. 121,108), dá um passo para um tile livre vizinho e
/// aceita qualquer posição a até 3 tiles do Trainer.
pub(super) fn approach_trainer(hwnd: isize, lor: &Terrain, status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    let pos = confirm_destination(hwnd, Dest::Lorencia, None)?; // mapa + leitura válidos
    if cheb(pos, TRAINER_TILE) <= 2 {
        return Ok(pos);
    }
    let walked = walk_with_arrival(hwnd, lor, TRAINER_STAND, WalkArrival::Trainer, status);
    let pos = confirm_destination(hwnd, Dest::Lorencia, None)?;
    match walked {
        Ok(_) => Ok(pos),
        Err(e) => {
            if cheb(pos, TRAINER_TILE) <= TRAINER_CLICK_RANGE {
                tracing::info!(hwnd, posicao = ?pos, erro = %e, "Raven: rota falhou mas já estou a até 3 tiles do Trainer");
                return Ok(pos);
            }
            // Passo curto para um vizinho livre do ponto de conversa e nova leitura.
            for cand in [(122, 108), (123, 108), (122, 107), (121, 107), (123, 107), (121, 109), (123, 109)] {
                if !lor.walkable(cand.0, cand.1) || cand == pos {
                    continue;
                }
                let click = npc_step_px(pos, cand);
                status(format!("Raven: saindo de {pos:?} em direção a {cand:?}"));
                input::run(hwnd, &[Step::Click { x: click.0, y: click.1 }])?;
                sleep(900);
                let now = confirm_destination(hwnd, Dest::Lorencia, None)?;
                if cheb(now, TRAINER_TILE) <= TRAINER_CLICK_RANGE {
                    return Ok(now);
                }
                break;
            }
            Err(e)
        }
    }
}

/// Trainer lido da memória do jogo: exatamente um NPC classe 226 "Trainer" perto do tile
/// conhecido, com duas leituras iguais e o char parado no mesmo tile. Qualquer dúvida = erro.
pub(super) fn trainer_from_memory(hwnd: isize, pos: (i32, i32)) -> Result<(i32, i32), String> {
    let read = |at: (i32, i32)| -> Option<(i32, i32)> {
        let v: Vec<_> = objects_near(hwnd, at)?
            .into_iter()
            .filter(|o| o.kind == zen_core::objects::Kind::Npc && o.class == 226 && o.name == "Trainer" && cheb((o.x, o.y), TRAINER_TILE) <= 2)
            .collect();
        (v.len() == 1).then(|| (v[0].x, v[0].y))
    };
    let first = read(pos).ok_or("Trainer: NPC ausente ou ambíguo na memória (1ª leitura)")?;
    sleep(150);
    let again = confirm_destination(hwnd, Dest::Lorencia, None)?;
    if again != pos {
        return Err(format!("Trainer: o char se moveu de {pos:?} para {again:?} durante a leitura"));
    }
    let second = read(again).ok_or("Trainer: NPC ausente ou ambíguo na memória (2ª leitura)")?;
    if first != second {
        return Err(format!("Trainer: o NPC mudou de {first:?} para {second:?} entre as leituras"));
    }
    if cheb(again, second) > TRAINER_CLICK_RANGE {
        return Err(format!("Trainer: char em {again:?} está a mais de {TRAINER_CLICK_RANGE} tiles do NPC em {second:?}"));
    }
    Ok(second)
}

/// Leva o char ao Trainer de Lorencia e restaura a vida do Dark Raven (Zen). Não volta ao
/// spot (quem chama faz isso). Erros com print salvo.
pub fn restore_raven(hwnd: isize, lor: &Terrain, manual: bool, status: &dyn Fn(String)) -> Result<RavenReport, String> {
    if lor.world != LORENCIA_WORLD { return Err("Trainer: terreno n?o ? de Lorencia".into()); }
    let _session = input::Session::begin(hwnd)?;
    let before = raven_confirmed(hwnd).ok_or("não vejo a barra do Dark Raven após 3 leituras de confirmação")?;
    // Confere de novo antes de sair do spot (11/11 21:45: saiu com o Raven em 100% depois de
    // uma leitura baixa errada logo após reiniciar o agente).
    if !raven_request_allowed(manual, before) {
        let b = before;
        return Err(format!("o Raven está em {b}%: não precisa do Trainer agora (leitura anterior errada)"));
    }
    if !in_lorencia(hwnd) {
        status(format!("Raven em {}%: M -> Lorencia (helper continua ligado)", before.to_string()));
        teleport(hwnd, Dest::Lorencia)?;
    }
    ensure_action_map(hwnd, Dest::Lorencia)?;
    // Zen: a restauração cobra Zen. Com menos que a reserva de volta (30M), busca no baú
    // (aba 1) antes, o mesmo do move.
    ensure_move_money(hwnd, lor, false, status)?;
    status("Raven: andando até o Trainer".into());
    close_npc_dialog(hwnd);
    // Interação com NPC: o servidor pode deixar o char em (121,108), tile
    // NOMOVE no arquivo. A aproximação existente aceita a posição de interação
    // validada e também sabe sair de um tile bloqueado, sem repetir a caminhada.
    // Cada tentativa confere o mapa, lê onde o char está e só anda de novo se estiver longe do
    // Trainer: um clique que não abriu a janela pode ter deslocado o char, e isso NÃO encerra
    // a restauração (só depois de esgotar as 3 tentativas).
    let mut open = false;
    let mut last_err = String::new();
    for (k, lift) in [40, 20, 60].into_iter().enumerate() {
        let pos = match approach_trainer(hwnd, lor, status) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(hwnd, tentativa = k + 1, erro = %e, "Raven: aproximação do Trainer falhou; vou ler a posição e tentar de novo");
                status(format!("Raven: não cheguei no Trainer ({e}); tentativa {} de 3", k + 1));
                last_err = e;
                close_npc_dialog(hwnd);
                sleep(500);
                continue;
            }
        };
        close_npc_dialog(hwnd);
        sleep(300);
        // Só o NPC lido da memória: sem tile adivinhado (o fallback fixo clicava no vazio).
        let tile = match trainer_from_memory(hwnd, pos) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(hwnd, tentativa = k + 1, erro = %e, "Raven: Trainer não confirmado pela memória; não clico");
                status(format!("Raven: {e}; tentativa {} de 3", k + 1));
                last_err = e;
                sleep(700);
                continue;
            }
        };
        let (x, y) = npc_px(pos, (tile.0 as f64, tile.1 as f64));
        status(format!("Raven: clicando no Trainer em {tile:?} ({})", k + 1));
        input::run(hwnd, &[Step::Click { x, y: y - lift }])?;
        if wait_for(hwnd, TRAINER_TITLE, 5000) {
            open = true;
            break;
        }
        force_close_storage(hwnd);
    }
    if !open {
        save_error_shot(hwnd, "janela do Trainer não abriu");
        return Err(if last_err.is_empty() {
            "a janela do Trainer não abriu (3 tentativas)".into()
        } else {
            format!("a janela do Trainer não abriu (3 tentativas); última falha ao chegar: {last_err}")
        });
    }
    let close = |hwnd: isize| {
        if trainer_text(hwnd, TRAINER_TITLE) {
            let _ = input::run(hwnd, &[Step::Click { x: TRAINER_CLOSE_BTN.0, y: TRAINER_CLOSE_BTN.1 }]);
            sleep(600);
        }
        if trainer_text(hwnd, TRAINER_TITLE) {
            let _ = input::run(hwnd, &[Step::Key(0x1B)]);
            sleep(500);
        }
    };
    // A janela pode vir apenas com título/saudação (YouSee, 15/11).
    // Nunca clicar onde deveria estar um botão ausente. Uma única reabertura,
    // sem refazer a caminhada, exige NPC confirmado e posição estável novamente.
    if !wait_for(hwnd, TRAINER_RESTORE, 3000) {
        save_error_shot(hwnd, "Trainer: título presente mas menu de atendimento ausente; uma reabertura");
        close(hwnd);
        sleep(700);
        ensure_action_map(hwnd, Dest::Lorencia)?;
        let at = npc_still(hwnd)?;
        let read_trainer=|at|->Option<(i32,i32)>{
            let npcs:Vec<_>=objects_near(hwnd,at)?.into_iter().filter(|o|
                o.kind==zen_core::objects::Kind::Npc && o.class==226 && o.name=="Trainer"
                && cheb((o.x,o.y),TRAINER_TILE)<=2).collect();
            (npcs.len()==1).then(||(npcs[0].x,npcs[0].y))
        };
        let first=read_trainer(at).ok_or("Trainer: NPC ausente ou ambíguo na primeira confirmação")?;
        sleep(150);
        let second_at=confirm_destination(hwnd,Dest::Lorencia,None)?;
        let second=read_trainer(second_at).ok_or("Trainer: NPC ausente ou ambíguo na segunda confirmação")?;
        if second_at!=at || second!=first {return Err("Trainer: personagem/NPC mudou durante a confirmação; menu não reaberto".into());}
        if cheb(second_at,second)>TRAINER_CLICK_RANGE {return Err("Trainer: personagem fora da área de interação; menu não reaberto".into());}
        tracing::warn!(hwnd,posicao=?at,"Raven: reabrindo uma vez o Trainer após menu vazio, sem nova caminhada");
        status("Raven: Trainer abriu sem botões; reabrindo uma única vez".into());
        npc_click(hwnd,second_at,"Trainer",second,40)?;
        if !wait_for(hwnd, TRAINER_RESTORE, 5000) {
            save_error_shot(hwnd, "Trainer: menu continuou vazio após uma reabertura confirmada");
            close(hwnd);
            return Err("a janela do Trainer continuou sem \"Restore life\" após uma reabertura; sem cliques cegos".into());
        }
    }
    input::run(hwnd, &[Step::Click { x: TRAINER_RESTORE_BTN.0, y: TRAINER_RESTORE_BTN.1 }])?;
    let mut listed = wait_for(hwnd, TRAINER_SELECT, 4000);
    if !listed && trainer_text(hwnd, TRAINER_RESTORE) {
        // O clique não pegou (PC lento): mais uma vez.
        input::run(hwnd, &[Step::Click { x: TRAINER_RESTORE_BTN.0, y: TRAINER_RESTORE_BTN.1 }])?;
        listed = wait_for(hwnd, TRAINER_SELECT, 4000);
    }
    if !listed {
        save_error_shot(hwnd, "Trainer sem a lista de pets");
        close(hwnd);
        return Err("o Trainer não mostrou a lista de pets".into());
    }
    status("Raven: Restore life -> Dark Raven".into());
    input::run(hwnd, &[Step::Click { x: TRAINER_RAVEN_BTN.0, y: TRAINER_RAVEN_BTN.1 }])?;
    sleep(2000);
    save_error_shot(hwnd, "Raven: tela depois de restaurar (conferência)");
    close(hwnd);
    sleep(800);
    let after = raven_confirmed(hwnd);
    match after {
        Some(a) if a >= 90 => Ok(RavenReport { before: Some(before), after: a }),
        Some(a) => Err(format!("a vida do Raven ficou em {a}% depois do Trainer (falta Zen? print salvo)")),
        None => Err("não li a barra do Raven depois do Trainer (print salvo)".into()),
    }
}
