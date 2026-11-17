use crate::win::nav::*;

/// Pasta World de Lorencia.
pub const LORENCIA_WORLD: u8 = 1;
/// Botão "Lorencia" na janela do M.
pub(in crate::win::nav) const M_LORENCIA: (i32, i32) = (250, 503);
/// Tile de onde se clica no Safety Guardian de Lorencia.
pub const LOR_VAULT_TILE: (i32, i32) = (146, 112);
/// Tile (estimado pelo modelo pixel->tile em 5 cliques reais) do Safety Guardian e do Lugard.
pub(in crate::win::nav) const LOR_GUARD: (f64, f64) = (144.5, 110.5);
pub const LUGARD_TILE: (i32, i32) = (149, 114);
pub(in crate::win::nav) const LUGARD: (f64, f64) = (149.0, 115.9);
pub const FORTUNE_ZEN: u64 = 1_000_000_000;
pub const MOVE_ZEN: u64 = 20_000_000;
/// Volta com 30M (20M do move + Zen para o helper ligar no spot) + folga da taxa do baú.
pub const KEEP_ZEN: u64 = 30_000_000 + 1_000;
/// A aba 1 nunca fica abaixo disto quando o saque é para converter.
pub const TAB1_RESERVE: u64 = 1_000_000_000;
/// Regra do dono (16/11 08:39): a aba 1 é o único baú e tem meta de 1B. Com 100M ou menos
/// ela não empresta Zen para Fortuna; o Zen do próprio char a repõe.
pub const TAB1_REFILL_TRIGGER: u64 = 100_000_000;
/// Margem que o char guarda no inventário ao repor a aba 1 (moves e taxas).
pub const SAFETY_ZEN: u64 = 50_000_000;

/// Quanto sacar da aba 1 para completar uma Fortuna: tudo o que falta ou nada. Não empresta se
/// a aba ficaria em `floor` ou menos (antes ia esvaziando ~40M por ciclo: xWizard 771->691M).
pub(in crate::win::nav) fn fortune_borrow(need: u64, tab1: u64, floor: u64) -> u64 {
    if tab1 <= floor || need > tab1 - floor { 0 } else { need }
}
pub(in crate::win::nav) const WITHDRAW_FEE: u64 = 200;
/// Folga aceita na taxa do saque (a taxa real já passou de 6.000).
pub(in crate::win::nav) const WITHDRAW_SLACK: u64 = 10_000;
/// Botão "Withdraw" (segundo embaixo do baú). A caixa do valor é a mesma do depósito.
pub(in crate::win::nav) const STORAGE_WITHDRAW: (i32, i32) = (535, 672);
pub(in crate::win::nav) const VK_K: u16 = 0x4B;
pub(in crate::win::nav) const VK_J: u16 = 0x4A;
/// Gremory Case: 1ª linha clicável e passo entre linhas (9 visíveis).
pub(in crate::win::nav) const GREMORY_ROW0_Y: f64 = 158.0;
pub(in crate::win::nav) const GREMORY_ROW_STEP: f64 = 36.75;
pub(in crate::win::nav) const GREMORY_X: (i32, i32) = (692, 76);
pub(in crate::win::nav) const USE_IT: (i32, i32) = (379, 361);
pub(in crate::win::nav) const USE_CANCEL: (i32, i32) = (507, 361);

/// "Fortuna do Imperador" na fonte da lista do Gremory Case e da caixa "Use It".
pub(in crate::win::nav) const T_FORTUNE: &str = "\
.........................................................................................................
............................................#..............................................#.............
.#####...........#..........................#..........###.................................#.............
.#...............#..........................#...........#..................................#.............
.#......###..#.#.##.#...#.####...##......####..###......#..###.##..####...##..#.#..##...####..###..#.#...
.#####.#...#.##..#..#...#.#...#....#....#...#.#...#.....#..#..#..#.#...#.#..#.##.....#.#...#.#...#.##....
.#.....#...#.#...#..#...#.#...#..###....#...#.#...#.....#..#..#..#.#...#.####.#....###.#...#.#...#.#.....
.#.....#...#.#...#..#...#.#...#.#..#....#...#.#...#.....#..#..#..#.#...#.#....#...#..#.#...#.#...#.#.....
.#......###..#....#..####.#...#..###.....####..###.....###.#..#..#.####...###.#....###..####..###..#.....
...................................................................#.....................................
...................................................................#.....................................";
/// "Fortuna do Imperador" na lista do Item Bank (J), fonte mais grossa.
pub(in crate::win::nav) const T_BANK: &str = "\
..................................................................................................
.####...........#..........................#...........#.................................#........
.#..............#..........................#...........#.................................#........
.#.....###..######.#...#.####..###......####..###......#.###.##..####...##..###.###...####..###..#
.####.#...#.#...#..#...#.#...#....#....#...#.#...#.....#.#..#..#.#...#.#..#.#......#.#...#.#...#.#
.#....#...#.#...#..#...#.#...#..###....#...#.#...#.....#.#..#..#.#...#.####.#....###.#...#.#...#.#
.#....#...#.#...#..#...#.#...#.#..#....#...#.#...#.....#.#..#..#.#...#.#....#...#..#.#...#.#...#.#
.#.....###..#...##..####.#...#..###.....####..###......#.#..#..#.####...###.#....###..####..###..#
.................................................................#................................
.................................................................#................................";
/// Botão "Deposit All" do menu da Fortuna no Item Bank (texto amarelo).
pub(in crate::win::nav) const T_DEPOSIT_ALL: &str = "\
............................................................
...................................................##.##....
..#####..........................##.##.......###...##.##....
..##..##............................##.......###...##.##....
..##...##..###..####...###...###.##.###.....##.##..##.##....
..##...##.##.##.##.##.##.##.##...##.##......##.##..##.##....
..##...##.#####.##.##.##.##..##..##.##.....#######.##.##....
..##..##..##....##.##.##.##...##.##.##.....##...##.##.##....
..#####....####.####...###..###..##..##....##...##.##.##....
................##..........................................
................##..........................................";
/// O menu da Fortuna (Withdraw.../Deposit All) está aberto?
pub(in crate::win::nav) fn bank_menu(hwnd: isize) -> Option<(i32, i32)> {
    text_at(hwnd, [150, 60, 700, 640], T_DEPOSIT_ALL, 2, 14)
}
/// Título "Gremory Case" (igual no cliente em inglês e em português: o "Character Items" não
/// aparece em português, visto no xWizard em 09/11 18:07).
pub(in crate::win::nav) const T_GREMORY: &str = "\
...................................................................
....####.......................................####................
...#..........................................#....................
..#......#.#..##..###.##...###..#.#.#...#....#.......##..###..##...
..#..###.##..#..#.#..#..#.#...#.##...#.#.....#.........#.#...#..#..
..#....#.#...####.#..#..#.#...#.#....#.#.....#.......###..#..####..
...#...#.#...#....#..#..#.#...#.#....#.#......#.....#..#...#.#.....
....####.#....###.#..#..#..###..#.....#........####..###.###..###..
......................................#............................
.....................................#.............................";

pub(in crate::win::nav) fn bin_orange(img: &crate::image::Image) -> Bits {
    let (w, h) = (img.w as usize, img.h as usize);
    let mut b = Bits::new(w, h);
    for (i, p) in img.bgra.chunks_exact(4).take(w * h).enumerate() {
        b.px[i] = p[2] > 150 && p[1] > 90 && p[0] < 90;
    }
    b
}

/// O modelo aparece em `region` (busca em todas as posições)? Menor diferença <= `max`.
pub(in crate::win::nav) fn text_in(hwnd: isize, region: [i32; 4], model: &str, mode: u8, max: usize) -> bool {
    text_at(hwnd, region, model, mode, max).is_some()
}

/// Onde o modelo aparece (canto superior esquerdo, em coordenadas da janela): a posição com
/// menor diferença, se <= `max`.
pub(in crate::win::nav) fn text_at(hwnd: isize, region: [i32; 4], model: &str, mode: u8, max: usize) -> Option<(i32, i32)> {
    let img = capture::capture(hwnd, Some(region))?;
    // 0 = laranja (lista do K), 1 = branco, 2 = amarelo dos botões do Item Bank.
    let b = match mode {
        0 => bin_orange(&img),
        2 => {
            let (w, h) = (img.w as usize, img.h as usize);
            let mut b = Bits::new(w, h);
            for (i, p) in img.bgra.chunks_exact(4).take(w * h).enumerate() {
                b.px[i] = p[2] > 180 && p[1] > 140;
            }
            b
        }
        _ => binarize_white(&img.bgra, img.w as usize, img.h as usize, 170),
    };
    let t = Bits::from_text(model);
    if b.w < t.w || b.h < t.h {
        return None;
    }
    let mut best: Option<(usize, i32, i32)> = None;
    for oy in 0..=(b.h - t.h) {
        for ox in 0..=(b.w - t.w) {
            let lim = best.map_or(max, |b| b.0.min(max));
            let mut d = 0;
            'm: for y in 0..t.h {
                for x in 0..t.w {
                    if b.px[(oy + y) * b.w + ox + x] != t.px[y * t.w + x] {
                        d += 1;
                        if d > lim {
                            break 'm;
                        }
                    }
                }
            }
            if d <= lim && best.map_or(true, |b| d < b.0) {
                best = Some((d, region[0] + ox as i32, region[1] + oy as i32));
            }
        }
    }
    best.map(|(_, x, y)| (x, y))
}

pub(in crate::win::nav) fn gremory_row_y(i: usize) -> i32 {
    (GREMORY_ROW0_Y + GREMORY_ROW_STEP * i as f64).round() as i32
}
pub(in crate::win::nav) fn gremory_open(hwnd: isize) -> bool {
    text_in(hwnd, [405, 74, 92, 26], T_GREMORY, 0, 12)
}
pub(in crate::win::nav) fn gremory_row_is_fortune(hwnd: isize, i: usize) -> bool {
    text_in(hwnd, [192, gremory_row_y(i) - 3, 113, 17], T_FORTUNE, 0, 25)
}
pub(in crate::win::nav) fn use_dialog_is_fortune(hwnd: isize) -> bool {
    text_in(hwnd, [385, 268, 130, 24], T_FORTUNE, 1, 25)
}
/// Linha "Fortuna do Imperador" do Item Bank, procurada na tela toda: a janela do J não abre
/// sempre no mesmo lugar (ELISMAR 09/11 16:01: o clique fixo caiu na linha de baixo).
pub(in crate::win::nav) fn bank_row(hwnd: isize) -> Option<(i32, i32)> {
    text_at(hwnd, [150, 60, 700, 560], T_BANK, 1, 25)
}
pub(in crate::win::nav) fn bank_open(hwnd: isize) -> bool {
    bank_row(hwnd).is_some()
}

/// Está em Lorencia (nome no canto)?
pub fn in_lorencia(hwnd: isize) -> bool {
    corner(hwnd).is_some_and(|b| read_coords(&b).is_some() && zen_core::glyphs::map_is_lorencia(&b))
}

/// Onde o NPC parado no tile `npc` aparece na tela com o char em `at`.
pub(in crate::win::nav) fn npc_px(at: (i32, i32), npc: (f64, f64)) -> (i32, i32) {
    let (dx, dy) = (npc.0 - at.0 as f64, npc.1 - at.1 as f64);
    let m = MODEL;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let (px, py) = ((m[1][1] * dx - m[0][1] * dy) / det, (-m[1][0] * dx + m[0][0] * dy) / det);
    (CENTER.0 + px.round() as i32, CENTER.1 + py.round() as i32)
}

/// Clica num NPC lido da memória e espera a janela dele (Trainer: título próprio; os demais:
/// diálogo padrão de NPC).
pub(in crate::win::nav) fn npc_click(hwnd:isize,at:(i32,i32),name:&str,location:(i32,i32),lift:i32)->Result<(),String>{
    let trainer=name.eq_ignore_ascii_case("Trainer");
    if trainer&&trainer_text(hwnd,TRAINER_TITLE){return Ok(());}
    let (x,y)=npc_px(at,(location.0 as f64,location.1 as f64));
    input::run(hwnd,&[Step::Click{x,y:y-lift}])?;
    let start=std::time::Instant::now();
    while start.elapsed().as_secs()<5 {
        if input::manual_mode(){return Err("interação pausada".into());}
        if trainer {if trainer_text(hwnd,TRAINER_TITLE){return Ok(());}}
        else if npc_dialog_open(hwnd){return Ok(());}
        sleep(150);
    }
    Err("janela do NPC não confirmada após o clique".into())
}

/// Terreno de um World a partir da pasta do jogo (dono da janela).
pub fn world_terrain_of(hwnd: isize, world: u8) -> Result<Terrain, String> {
    let mut pid = 0u32;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(windows::Win32::Foundation::HWND(hwnd as *mut _), Some(&mut pid));
    }
    let exe = crate::win::memory::process_path(pid).ok_or("não achei o executável do jogo")?;
    let dir = std::path::Path::new(&exe).parent().ok_or("caminho do jogo inválido")?;
    let f = dir.join(format!("Data\\World{world}\\EncTerrain{world}.att"));
    let raw = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    Terrain::from_client_file(&raw).map_err(|e| e.to_string())
}

pub(in crate::win::nav) fn close_inventory(hwnd: isize) {
    for _ in 0..2 {
        if !inventory_open(hwnd) {
            return;
        }
        let _ = input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)]);
        sleep(700);
    }
}

/// Zen do inventário pela tela: abre (V, que também fecha diálogos de NPC), lê e fecha.
pub fn inv_zen(hwnd: isize) -> Result<u64, String> {
    for attempt in 0..2 {
        close_npc_dialog(hwnd);
        if npc_dialog_open(hwnd) { return Err("inventário bloqueado por janela de NPC que não fechou".into()); }
        if !inventory_open(hwnd) {
            input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
            let t0 = std::time::Instant::now();
            while t0.elapsed().as_millis() < 2500 && !inventory_open(hwnd) { sleep(250); }
        }
        // V enviado não é confirmação de inventário aberto.
        if inventory_open(hwnd) {
            if let Some(z)=read_zen_stable(hwnd, INV_ZEN) { close_inventory(hwnd); return Ok(z); }
        }
        tracing::warn!(hwnd,tentativa=attempt+1,npc=npc_dialog_open(hwnd),"Zen do inventário não confirmado; relendo após fechar obstruções");
    }
    save_error_shot(hwnd,"inventário/Zen não confirmados após duas tentativas");
    Err("não li o Zen do inventário após duas tentativas e tratamento de janelas".into())
}

/// Passo proporcional até o waypoint, sem forçar 90 pixels perto do NPC.
pub(in crate::win::nav) fn npc_step_px(at: (i32, i32), waypoint: (i32, i32)) -> (i32, i32) {
    let p = npc_px(at, (waypoint.0 as f64, waypoint.1 as f64));
    let (dx, dy) = ((p.0 - CENTER.0) as f64, (p.1 - CENTER.1) as f64);
    let scale = (180.0 / dx.hypot(dy).max(1.0)).min(1.0);
    (CENTER.0 + (dx * scale).round() as i32, CENTER.1 + (dy * scale).round() as i32)
}

/// Não recalcula o clique no NPC enquanto o char ainda estiver andando.
pub(in crate::win::nav) fn npc_still(hwnd: isize) -> Result<(i32, i32), String> {
    let mut last = position(hwnd).ok_or("NPC: não li as coordenadas")?;
    let mut stable = 0;
    for _ in 0..12 {
        if input::manual_mode() { return Err("NPC: aproximação interrompida pelo dono".into()); }
        sleep(200);
        let p = position(hwnd).ok_or("NPC: perdi a leitura da posição")?;
        stable = if p == last { stable + 1 } else { 0 };
        last = p;
        if stable >= 2 { return Ok(p); }
    }
    Err("NPC: char ainda andando; parei sem repetir cliques".into())
}

pub(in crate::win::nav) struct NpcProgress {
    pub(in crate::win::nav) target: (i32, i32),
    pub(in crate::win::nav) at: (i32, i32),
    pub(in crate::win::nav) previous: Option<(i32, i32)>,
    pub(in crate::win::nav) stalled: u8,
}

impl NpcProgress {
    pub(in crate::win::nav) fn arrived(&self) -> bool { cheb(self.at, self.target) <= 1 }
    pub(in crate::win::nav) fn advance(&mut self, next: (i32, i32)) -> Result<(), String> {
        if cheb(next, self.target) <= 1 { self.at = next; return Ok(()); }
        self.stalled = if next == self.at { self.stalled + 1 } else { 0 };
        if self.stalled >= 2 || self.previous == Some(next) {
            return Err(format!("NPC: sem progresso ou vai e vem em {next:?}; parei sem insistir"));
        }
        self.previous = Some(self.at);
        self.at = next;
        Ok(())
    }
}

/// O servidor pode colocar o char num tile NOMOVE do cliente ao teleportar.
/// Autoriza somente a saída cardinal para um vizinho livre com rota válida.
pub(in crate::win::nav) fn npc_route(t: &Terrain, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    if let Some(route) = t.path(from, to) { return Some(route); }
    let attr = t.attr(from.0, from.1)?;
    if attr & zen_core::terrain::NOGROUND != 0 || t.walkable(from.0, from.1) || !t.walkable(to.0,to.1) {
        return None;
    }
    [(1,0),(-1,0),(0,1),(0,-1)].into_iter()
        .filter_map(|(dx,dy)| {
            let exit = (from.0+dx, from.1+dy);
            let mut route = t.path(exit,to)?;
            if route.first() != Some(&exit) { route.insert(0,exit); }
            Some(route)
        }).min_by_key(Vec::len)
}

/// Segura durante o trajeto e solta a três tiles; só o ajuste curto usa cliques.
pub(in crate::win::nav) fn npc_hold_far(at: (i32, i32), target: (i32, i32)) -> bool {
    cheb(at, target) > 3
}

pub(in crate::win::nav) fn recover_route_window(hwnd: isize, dest: Dest, count: &mut usize, status: &dyn Fn(String)) -> Result<(i32,i32),String> {
    if *count>=2 { return Err("NPC: janelas repetidas no trajeto; limite de duas recuperações".into()); }
    *count+=1;
    status(format!("NPC: fechando janela acidental no trajeto ({}/2)",*count));
    force_close_storage(hwnd);
    close_npc_dialog(hwnd);
    if storage_open(hwnd) || npc_dialog_open(hwnd) { return Err("NPC: janela não fechou; movimento não retomado".into()); }
    confirm_destination(hwnd,dest,None)?;
    npc_still(hwnd)
}

#[derive(Default)]
pub(in crate::win::nav) struct NpcDetours { pub(super) tried: Vec<(i32, i32)> }
impl NpcDetours {
    pub(in crate::win::nav) fn next(&mut self, t: &Terrain, at: (i32,i32), blocked: (i32,i32), target: (i32,i32)) -> Result<(i32,i32),String> {
        if self.tried.len() >= 2 { return Err("NPC: limite de dois desvios locais; parei sem repetir a rota".into()); }
        // O tile para onde não deu para ir fica bloqueado no resto da rota (jogador parado ou
        // objeto que não está no arquivo do terreno; Lorencia (130,130), 15/11).
        // Nunca bloqueia o próprio destino (LOBOLIXO 16/11 00:38: depois de fechar uma janela
        // acidental o "bloqueado" era o Lugard e não sobrava rota nenhuma).
        let tb=if blocked!=target {t.with_blocked(&[blocked])} else {t.clone()};
        let t=&tb;
        let forward=((blocked.0-at.0) as f64,(blocked.1-at.1) as f64);
        let next=[2,3].into_iter().flat_map(|r|[(r,0),(-r,0),(0,r),(0,-r)])
            .filter_map(|(dx,dy)| {
                let via=(at.0+dx,at.1+dy);
                if via==blocked || self.tried.contains(&via) || !t.line_clear(at,via) { return None; }
                // Não repetir a direção que acabou de falhar. Saída lateral ou para trás.
                let cosine=(forward.0*dx as f64+forward.1*dy as f64)/(forward.0.hypot(forward.1).max(1.0)*(dx as f64).hypot(dy as f64));
                if cosine > 0.8 { return None; }
                let local=t.path(at,via)?;
                if local.len()>3 { return None; }
                let rest=npc_route(t,via,target)?;
                if rest.contains(&at) { return None; } // Não voltar pelo ponto que acabou de prender.
                Some((local.len()+rest.len(),via))
            }).min_by_key(|(len,via)|(*len,*via)).map(|(_,via)|via)
            .ok_or("NPC: sem desvio local seguro ao redor do obstáculo")?;
        self.tried.push(next);
        Ok(next)
    }
}

pub(in crate::win::nav) fn walk_near(hwnd: isize, t: &Terrain, tile: (i32, i32), status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    let started = std::time::Instant::now();
    let dest = Dest::from_world(t.world).ok_or("NPC: terreno de mapa desconhecido")?;
    ensure_action_map(hwnd, dest)?;
    let mut at = npc_still(hwnd)?;
    let mut recovered_windows=0;
    if npc_hold_far(at, tile) {
        let mut progress = NpcProgress { target: tile, at, previous: None, stalled: 0 };
        let mut held: Option<input::Hold> = None;
        let mut moved = std::time::Instant::now();
        let mut detours = NpcDetours::default();
        let mut via = None;
        while npc_hold_far(at, tile) {
            if input::manual_mode() { return Err("NPC: trajeto interrompido pelo dono".into()); }
            if started.elapsed().as_secs() >= 40 { return Err("NPC: limite de tempo no trajeto segurado".into()); }
            if storage_open(hwnd) || npc_dialog_open(hwnd) {
                if let Some(h)=held.as_mut(){h.release();}
                drop(held.take());
                at=recover_route_window(hwnd,dest,&mut recovered_windows,status)?;
                if npc_hold_far(at,tile) { via=Some(detours.next(t,at,tile,tile)?); }
                else { via=None; }
                progress=NpcProgress{target:tile,at,previous:None,stalled:0};
                moved=std::time::Instant::now();
                continue;
            }
            if via.is_some_and(|p|cheb(at,p)<=1) {
                via=None;
                progress=NpcProgress{target:tile,at,previous:None,stalled:0};
            }
            let leg=via.unwrap_or(tile);
            let path = npc_route(t, at, leg).ok_or_else(|| format!("NPC: sem rota de {at:?} até {leg:?}"))?;
            let waypoint = t.waypoints(at, &path, 3).into_iter().next().ok_or("NPC: rota vazia")?;
            if !t.walkable(at.0,at.1) {
                tracing::warn!(hwnd, origem = ?at, atributo = ?t.attr(at.0,at.1), saida = ?waypoint, "NPC: saída curta de tile de spawn bloqueado no arquivo");
            }
            let cursor = npc_step_px(at, waypoint);
            match held.as_mut() {
                Some(h) => h.steer(cursor.0, cursor.1)?,
                None => {
                    status(format!("NPC: trajeto com botão segurado, {at:?} -> {tile:?}"));
                    tracing::info!(hwnd, origem = ?at, destino = ?tile, "NPC: iniciou trajeto segurado");
                    held = Some(input::Hold::start(hwnd, cursor.0, cursor.1)?);
                }
            }
            sleep(200);
            let next = position(hwnd).ok_or("NPC: perdi a posição no trajeto segurado")?;
            if cheb(next, at) > 6 { return Err("NPC: leitura implausível; botão solto".into()); }
            if next != at {
                if let Err(e) = progress.advance(next) {
                    // Vai e vem no trajeto segurado (x9901, 15/11: (130,130) <-> (129,131)): solta
                    // o botão e tenta um desvio local com o tile da frente bloqueado, em vez de
                    // abortar a Fortuna. Os desvios continuam limitados a 2.
                    if let Some(h)=held.as_mut(){h.release();}
                    drop(held.take());
                    confirm_destination(hwnd,dest,None)?;
                    let settled=npc_still(hwnd)?;
                    let avoid=detours.next(t,settled,waypoint,tile).map_err(|d|format!("{e}; {d}"))?;
                    tracing::warn!(hwnd,origem=?settled,direcao_bloqueada=?waypoint,desvio=?avoid,tentativa=detours.tried.len(),"NPC: vai e vem no trajeto; desvio local");
                    status(format!("NPC: vai e vem em {settled:?}; desvio por {avoid:?} ({}/2)",detours.tried.len()));
                    via=Some(avoid);
                    at=settled;
                    progress=NpcProgress{target:tile,at,previous:None,stalled:0};
                    moved=std::time::Instant::now();
                    continue;
                }
                at = next;
                moved = std::time::Instant::now();
                status(format!("NPC: botão segurado, {at:?} -> {tile:?}"));
            } else if moved.elapsed().as_millis() >= 1800 {
                if let Some(h)=held.as_mut(){h.release();}
                drop(held.take());
                confirm_destination(hwnd,dest,None)?;
                let settled=npc_still(hwnd)?;
                if settled!=at {
                    at=settled;
                } else {
                    let avoid=detours.next(t,at,waypoint,tile)?;
                    tracing::warn!(hwnd,origem=?at,direcao_bloqueada=?waypoint,desvio=?avoid,tentativa=detours.tried.len(),"NPC: obstáculo no trajeto; botão solto e desvio local validado");
                    status(format!("NPC: obstáculo em {at:?}; desvio local por {avoid:?} ({}/2)",detours.tried.len()));
                    via=Some(avoid);
                }
                progress=NpcProgress{target:tile,at,previous:None,stalled:0};
                moved=std::time::Instant::now();
            }
        }
        if let Some(h) = held.as_mut() { h.release(); }
        drop(held);
        tracing::info!(hwnd, posicao = ?at, destino = ?tile, "NPC: soltou botão antes da aproximação final");
        at = npc_still(hwnd)?;
    }
    let mut progress = NpcProgress { target: tile, at, previous: None, stalled: 0 };
    for step in 0..6 {
        let at = progress.at;
        if progress.arrived() { return confirm_destination(hwnd, dest, Some((tile, 1))); }
        if started.elapsed().as_secs() >= 40 { break; }
        if storage_open(hwnd) || npc_dialog_open(hwnd) {
            let at=recover_route_window(hwnd,dest,&mut recovered_windows,status)?;
            progress=NpcProgress{target:tile,at,previous:None,stalled:0};
            continue;
        }
        let path = npc_route(t, at, tile).ok_or_else(|| format!("NPC: sem rota de {at:?} até {tile:?}"))?;
        let waypoint = t.waypoints(at, &path, 3).into_iter().next().ok_or("NPC: rota vazia")?;
        let click = npc_step_px(at, waypoint);
        status(format!("NPC: ajuste final {step}, {at:?} -> {tile:?}"));
        tracing::info!(hwnd, step, origem = ?at, destino = ?tile, clique = ?click, "NPC: ajuste final após soltar botão");
        input::run(hwnd, &[Step::Click { x: click.0, y: click.1 }])?;
        progress.advance(npc_still(hwnd)?)?;
        if progress.arrived() { return confirm_destination(hwnd, dest, Some((tile, 1))); }
    }
    Err(format!("NPC: limite de aproximação em {:?}, destino {tile:?}", progress.at))
}
