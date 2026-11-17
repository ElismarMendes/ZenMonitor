//! Recuperação até o spot (Partes 1-4), testada em 08/11/2026 nos dois chars depois de um DC:
//! Lorencia -> M -> Kanturu (3 s) -> segurar + cliques até o tile exato -> helper ligado.
//!
//! Tudo lido na TELA (coordenadas, nome do mapa, ícone do helper) e conferido a cada passo.
//! Input só pelo `input` (SendInput, foco conferido). Nada de memória do jogo é escrito.

use super::{capture, input};
use zen_core::actions::{helper_on, Step, HELPER_ICON, HELPER_WHITE};
use zen_core::glyphs::{binarize_white, map_is_kanturu, read_coords, Bits, COORDS_WHITE};
use zen_core::terrain::Terrain;

pub const CORNER: [i32; 4] = [0, 30, 300, 24];
const VK_HOME: u16 = 0x24;
const VK_M: u16 = 0x4D;
/// Botão "Kanturu" na janela do M (print de 08/11, cliente 1030x797).
const M_KANTURU: (i32, i32) = (612, 323);
/// Centro dos cliques de movimento (o char fica no meio da tela).
const CENTER: (i32, i32) = (515, 400);
/// Pasta World do Kanturu 1.
pub const KANTURU_WORLD: u8 = 38;

fn sleep(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

fn cheb(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn corner(hwnd: isize) -> Option<Bits> {
    let img = capture::capture(hwnd, Some(CORNER))?;
    Some(binarize_white(&img.bgra, img.w as usize, img.h as usize, COORDS_WHITE))
}

/// (está no Kanturu?, posição). Mapa só é afirmado quando as coordenadas são legíveis.
pub fn observe_map(hwnd: isize) -> (Option<bool>, Option<(i32, i32)>) {
    let Some(b) = corner(hwnd) else { return (None, None) };
    let pos = read_coords(&b).map(|(x, y)| (x as i32, y as i32));
    (pos.map(|_| map_is_kanturu(&b)), pos)
}

pub fn position(hwnd: isize) -> Option<(i32, i32)> {
    corner(hwnd).and_then(|b| read_coords(&b)).map(|(x, y)| (x as i32, y as i32))
}

/// `Some(true)` pause = ligado, `Some(false)` play = desligado, `None` não reconhecido.
pub fn helper_state(hwnd: isize) -> Option<bool> {
    // O ícone nem sempre está no mesmo x: no DESKTOP-KIS9DG7 (09/11) a barra de cima vem 25 px
    // à direita. Procura de -12 a +40 px, o deslocamento mais perto de 0 primeiro.
    let [x, y, w, h] = HELPER_ICON;
    let img = capture::capture(hwnd, Some([x - 12, y, w + 52, h]))?;
    let big = binarize_white(&img.bgra, img.w as usize, img.h as usize, HELPER_WHITE);
    let mut offs: Vec<i32> = (-12..=40).collect();
    offs.sort_by_key(|d| d.abs());
    for d in offs {
        let ox = (d + 12) as usize;
        let mut b = Bits::new(w as usize, h as usize);
        for yy in 0..h as usize {
            for xx in 0..w as usize {
                b.px[yy * w as usize + xx] = big.px[yy * big.w + ox + xx];
            }
        }
        if let Some(on) = helper_on(&b) {
            return Some(on);
        }
    }
    None
}

/// Inventário aberto? (o campo do Zen só aparece com o inventário aberto)
pub fn inventory_open(hwnd: isize) -> bool {
    capture::capture(hwnd, Some([786, 609, 78, 14])).is_some_and(|img| {
        let b = zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
        zen_core::glyphs::read_number(&b).is_some()
    })
}

/// OK dos avisos do jogo no meio da tela ("Helper only runs in field", "close Inventory").
const DIALOG_OK: (i32, i32) = (512, 316);

/// Liga/desliga o helper só se precisar, e confere pelo ícone. Nunca aperta Home às cegas.
/// Para LIGAR: o jogo recusa com o inventário aberto (visto em 08/11: "In order to run The
/// Classic Helper, please close Inventory window"), então fecha o inventário antes; se ainda
/// assim não ligar, fecha o aviso (OK) e tenta mais uma vez.
pub fn set_helper(hwnd: isize, on: bool) -> Result<(), String> {
    let _lease = input::reserve(hwnd)?;
    let initial = helper_state(hwnd);
    tracing::info!(hwnd, solicitado = on, observado = ?initial, "HELPER: solicitação de estado");
    match initial {
        None => Err("não reconheci o ícone do helper".into()),
        Some(s) if s == on => Ok(()),
        Some(_) => {
            for attempt in 0..2 {
                // Confere antes de cada Home: Home alterna, nunca apertar se já está como pedido.
                match helper_state(hwnd) {
                    Some(s) if s == on => return Ok(()),
                    None => return Err("não reconheci o ícone do helper".into()),
                    _ => {}
                }
                if on && inventory_open(hwnd) {
                    input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
                    sleep(700);
                }
                // Janelas da Fortuna (K/J) abertas também podem segurar o helper.
                if on && bank_open(hwnd) {
                    input::run(hwnd, &[Step::Key(VK_J)])?;
                    sleep(600);
                }
                if on && gremory_open(hwnd) {
                    input::run(hwnd, &[Step::Click { x: GREMORY_X.0, y: GREMORY_X.1 }])?;
                    sleep(600);
                }
                // Limpar janelas pode levar tempo. Releia imediatamente antes de alternar:
                // o Helper pode ter ligado/desligado nesse intervalo.
                match helper_state(hwnd) {
                    Some(s) if s == on => return Ok(()),
                    None => return Err("não reconheci o ícone do helper antes do Home".into()),
                    _ => {}
                }
                tracing::info!(hwnd, solicitado = on, tentativa = attempt + 1, "HELPER: alternando com Home");
                input::run(hwnd, &[Step::Key(VK_HOME)])?;
                sleep(1300);
                let observed = helper_state(hwnd);
                tracing::info!(hwnd, solicitado = on, observado = ?observed, "HELPER: resultado após Home");
                match observed {
                    Some(s) if s == on => return Ok(()),
                    _ if attempt == 0 && on => {
                        // provável aviso na tela: OK e tenta de novo
                        input::run(hwnd, &[Step::Click { x: DIALOG_OK.0, y: DIALOG_OK.1 }])?;
                        sleep(800);
                    }
                    other => {
                        let e = format!("o helper não ficou {} (ícone {other:?})", if on { "ligado" } else { "desligado" });
                        save_error_shot(hwnd, &e);
                        return Err(e);
                    }
                }
            }
            Err("o helper não ligou depois de fechar o inventário e o aviso".into())
        }
    }
}

fn settle(hwnd: isize) -> Option<(i32, i32)> {
    let mut last = None;
    for _ in 0..25 {
        let s = position(hwnd);
        if s.is_some() && s == last {
            return s;
        }
        last = s;
        sleep(250);
    }
    last
}

fn near(hwnd: isize, prev: (i32, i32)) -> Option<(i32, i32)> {
    for _ in 0..4 {
        let s = settle(hwnd)?;
        if cheb(s, prev) <= 10 {
            return Some(s);
        }
        sleep(300);
    }
    None
}

/// M -> botão Kanturu. Espera o nome virar Kanturu e a posição saltar.
/// Texto "Kanturu" no mapa (M) aberto: igual no cliente em inglês e em português (prints de
/// 08/11 e 09/11). Fechado, a mesma região difere em 77+ pixels.
const M_LABEL: [i32; 4] = [592, 318, 40, 10];
const M_LABEL_TEXT: &str = "\
........................................
........................................
..#...#..............#..................
..#..#...............#..................
..#.#.....###..###..###.#..#..##.#..#...
..#.#.......#..#..#..#..#..#..#..#..#...
..##.#....###..#..#..#..#..#..#..#..#...
..#...#..#..#..#..#..#..#..#..#..#..#...
..#....#.####..#..#..##..###..#...###...
........................................";

/// O mapa (M) está aberto na tela?
pub fn map_open(hwnd: isize) -> bool {
    map_shift(hwnd).is_some()
}

/// Deslocamento do mapa (M) em relação ao medido no PC1: procura o texto "Kanturu" numa área
/// maior (outro layout de cliente, ex. DESKTOP-KIS9DG7). `None` = mapa fechado.
fn map_shift(hwnd: isize) -> Option<(i32, i32)> {
    let [x, y, w, h] = M_LABEL;
    let (mx, my) = (60, 40);
    let img = capture::capture(hwnd, Some([x - mx, y - my, w + 2 * mx, h + 2 * my]))?;
    let b = binarize_white(&img.bgra, img.w as usize, img.h as usize, COORDS_WHITE);
    let t = Bits::from_text(M_LABEL_TEXT);
    let mut best: Option<(usize, i32, i32)> = None;
    for oy in 0..=(b.h - t.h) {
        for ox in 0..=(b.w - t.w) {
            let mut d = 0;
            for yy in 0..t.h {
                for xx in 0..t.w {
                    if b.px[(oy + yy) * b.w + ox + xx] != t.px[yy * t.w + xx] {
                        d += 1;
                    }
                }
            }
            let (dx, dy) = (ox as i32 - mx, oy as i32 - my);
            if d <= 10 && best.map_or(true, |bb| (d, dx.abs() + dy.abs()) < (bb.0, bb.1.abs() + bb.2.abs())) {
                best = Some((d, dx, dy));
            }
        }
    }
    best.map(|(_, dx, dy)| (dx, dy))
}

/// M -> botão Kanturu. Confere que o mapa abriu antes de clicar; espera o nome virar Kanturu
/// e a posição saltar. Duas tentativas; print de cada falha (09/11 11:59: CSRF "não teleportou").
pub fn teleport_kanturu(hwnd: isize) -> Result<(i32, i32), String> {
    teleport(hwnd, Dest::Kanturu)
}

/// Mapas para onde o agente sabe ir pelo M. Todos conferidos no jogo pelo x9901 em 11/11
/// (botão, nome no canto e tile de chegada; Kanturu, Lorencia e Arena desde 08–09/11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    Kanturu,
    Lorencia,
    Arena,
    LostTower,
    Dungeon,
    Devias,
    Noria,
    Atlans,
    Tarkan,
    Icarus,
    ValleyOfLoren,
    Crywolf,
    Aida,
}

/// Pasta World de cada mapa (World = número do mapa + 1).
pub const LOST_TOWER_WORLD: u8 = 5;
pub const DUNGEON_WORLD: u8 = 2;

/// Uma entrada pelo M: botão na janela do M (cliente 1030x797) e tile onde o char aparece.
#[derive(Debug, Clone, Copy)]
pub struct Gate {
    pub button: (i32, i32),
    pub arrival: (i32, i32),
}

const fn g(button: (i32, i32), arrival: (i32, i32)) -> Gate {
    Gate { button, arrival }
}

/// Chegadas medidas em 11/11 (x9901, um move por botão).
const GATES_LOST_TOWER: [Gate; 7] = [
    g((355, 416), (206, 72)),
    g((323, 440), (243, 237)),
    g((345, 440), (86, 167)),
    g((367, 440), (87, 88)),
    g((389, 440), (128, 53)),
    g((345, 461), (52, 53)),
    g((367, 461), (8, 85)),
];
const GATES_DUNGEON: [Gate; 3] = [g((406, 571), (107, 247)), g((390, 599), (231, 126)), g((412, 599), (3, 83))];

impl Dest {
    pub const ALL: [Dest; 13] = [
        Dest::Kanturu,
        Dest::Lorencia,
        Dest::Arena,
        Dest::LostTower,
        Dest::Dungeon,
        Dest::Devias,
        Dest::Noria,
        Dest::Atlans,
        Dest::Tarkan,
        Dest::Icarus,
        Dest::ValleyOfLoren,
        Dest::Crywolf,
        Dest::Aida,
    ];
    /// Nome como aparece no canto (e no painel).
    pub fn name(self) -> &'static str {
        match self {
            Dest::Kanturu => "Kanturu",
            Dest::Lorencia => "Lorencia",
            Dest::Arena => "Arena",
            Dest::LostTower => "Lost Tower",
            Dest::Dungeon => "Dungeon",
            Dest::Devias => "Devias",
            Dest::Noria => "Noria",
            Dest::Atlans => "Atlans",
            Dest::Tarkan => "Tarkan",
            Dest::Icarus => "Icarus",
            Dest::ValleyOfLoren => "Valley of Loren",
            Dest::Crywolf => "Crywolf Fortress",
            Dest::Aida => "Aida",
        }
    }
    pub fn world(self) -> u8 {
        match self {
            Dest::Kanturu => KANTURU_WORLD,
            Dest::Lorencia => LORENCIA_WORLD,
            Dest::Arena => ARENA_WORLD,
            Dest::LostTower => LOST_TOWER_WORLD,
            Dest::Dungeon => DUNGEON_WORLD,
            Dest::Devias => 3,
            Dest::Noria => 4,
            Dest::Atlans => 8,
            Dest::Tarkan => 9,
            Dest::Icarus => 11,
            Dest::ValleyOfLoren => 31,
            Dest::Aida => 34,
            Dest::Crywolf => 35,
        }
    }
    pub fn from_name(n: &str) -> Option<Dest> {
        Dest::ALL.into_iter().find(|d| d.name() == n)
    }
    pub fn from_world(w: u8) -> Option<Dest> {
        Dest::ALL.into_iter().find(|d| d.world() == w)
    }
    /// Entradas pelo M (andares da Lost Tower e do Dungeon têm botões próprios).
    pub fn gates(self) -> &'static [Gate] {
        const KANTURU: [Gate; 1] = [g(M_KANTURU, (19, 218))];
        const LORENCIA: [Gate; 1] = [g(M_LORENCIA, (123, 129))];
        const ARENA: [Gate; 1] = [g(M_STADIUM, (62, 135))];
        const DEVIAS: [Gate; 1] = [g((283, 339), (215, 45))];
        const NORIA: [Gate; 1] = [g((472, 302), (174, 115))];
        const ATLANS: [Gate; 1] = [g((510, 640), (22, 18))];
        const TARKAN: [Gate; 1] = [g((463, 467), (202, 64))];
        const ICARUS: [Gate; 1] = [g((343, 205), (14, 13))];
        const LOREN: [Gate; 1] = [g((248, 663), (135, 95))];
        const CRYWOLF: [Gate; 1] = [g((808, 380), (228, 44))];
        const AIDA: [Gate; 1] = [g((562, 433), (83, 10))];
        match self {
            Dest::Kanturu => &KANTURU,
            Dest::Lorencia => &LORENCIA,
            Dest::Arena => &ARENA,
            Dest::LostTower => &GATES_LOST_TOWER,
            Dest::Dungeon => &GATES_DUNGEON,
            Dest::Devias => &DEVIAS,
            Dest::Noria => &NORIA,
            Dest::Atlans => &ATLANS,
            Dest::Tarkan => &TARKAN,
            Dest::Icarus => &ICARUS,
            Dest::ValleyOfLoren => &LOREN,
            Dest::Crywolf => &CRYWOLF,
            Dest::Aida => &AIDA,
        }
    }
    /// O agente sabe chegar lá pelo M? (todos, desde 11/11)
    pub fn can_teleport(self) -> bool {
        true
    }
}

/// A entrada do M que deixa o char mais perto de `target`: menor caminho pelo terreno; se
/// nenhuma tiver caminho, a mais perto em linha reta.
pub fn best_gate(dest: Dest, t: Option<&Terrain>, target: (i32, i32)) -> Gate {
    let gs = dest.gates();
    if gs.len() == 1 {
        return gs[0];
    }
    if let Some(t) = t {
        if let Some((_, g)) = gs.iter().filter_map(|g| t.path(g.arrival, target).map(|p| (p.len(), *g))).min_by_key(|(n, _)| *n) {
            return g;
        }
    }
    *gs.iter().min_by_key(|g| cheb(g.arrival, target)).unwrap()
}

/// Pasta World da Arena (Data\World7\EncTerrain7.att).
pub const ARENA_WORLD: u8 = 7;
/// Botão "Stadium" na janela do M (= Arena).
const M_STADIUM: (i32, i32) = (783, 529);

/// Está na Arena (nome no canto)?
pub fn in_arena(hwnd: isize) -> bool {
    corner(hwnd).is_some_and(|b| read_coords(&b).is_some() && zen_core::glyphs::map_is_arena(&b))
}

/// Está no mapa do spot? `Some(true/false)` só com as coordenadas legíveis.
pub fn observe_spot_map(hwnd: isize, arena: bool) -> (Option<bool>, Option<(i32, i32)>) {
    let Some(b) = corner(hwnd) else { return (None, None) };
    let pos = read_coords(&b).map(|(x, y)| (x as i32, y as i32));
    let on = if arena { zen_core::glyphs::map_is_arena(&b) } else { map_is_kanturu(&b) };
    (pos.map(|_| on), pos)
}

/// M -> botão do destino. Mesmo roteiro e conferência (nome do mapa no canto + salto de posição).
pub fn teleport(hwnd: isize, to: Dest) -> Result<(i32, i32), String> {
    teleport_gate(hwnd, to, to.gates()[0])
}

/// M -> botão `gate` do mapa `to` (andar da Lost Tower/Dungeon), mesma conferência.
pub fn teleport_gate(hwnd: isize, to: Dest, gate: Gate) -> Result<(i32, i32), String> {
    let btn = gate.button;
    let dest = to.name();
    // Caixa de NPC aberta não deixa o M abrir.
    close_npc_dialog(hwnd);
    position(hwnd).ok_or("não li as coordenadas")?;
    let mut last = String::new();
    for attempt in 1..=2 {
        tracing::info!(hwnd, attempt, mapa = dest, chegada = ?gate.arrival, "move: iniciando tentativa");
        if !map_open(hwnd) {
            input::run(hwnd, &[Step::Key(VK_M)])?;
            let t0 = std::time::Instant::now();
            while t0.elapsed().as_millis() < 4000 && !map_open(hwnd) {
                sleep(250);
            }
            if !map_open(hwnd) {
                last = "o mapa (M) não abriu".into();
                save_error_shot(hwnd, &last);
                sleep(800);
                continue;
            }
        }
        sleep(300);
        let (sx, sy) = map_shift(hwnd).unwrap_or((0, 0));
        input::run(hwnd, &[Step::Click { x: btn.0 + sx, y: btn.1 + sy }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < 15 {
            sleep(500);
            let snap = observe_named(hwnd);
            // Spawns variam na região da entrada; salto de posição sozinho não comprova chegada.
            if destination_matches(snap, to, Some((gate.arrival, 16))) {
                sleep(800);
                match confirm_destination(hwnd, to, Some((gate.arrival, 16))) {
                    Ok(p) => return Ok(p),
                    Err(e) => { last = e; break; }
                }
            }
            last = format!("mapa/chegada incorretos: esperado {dest} perto de {:?}, lido {:?} {:?}", gate.arrival, snap.0, snap.1);
        }
        last = format!("move para {dest} não confirmado em 15 s: {last} (confira Zen e entrada)");
        tracing::warn!(hwnd, attempt, motivo = %last, "move: tentativa falhou; não avançou para caminhada");
        save_error_shot(hwnd, &last);
        if map_open(hwnd) {
            let _ = input::run(hwnd, &[Step::Key(VK_M)]); // fecha o M
        }
        sleep(1000);
    }
    if map_open(hwnd) {
        let _ = input::run(hwnd, &[Step::Key(VK_M)]);
    }
    Err(format!("{last} (2 tentativas)"))
}

/// Áreas onde clique não anda: janela PICK, barra de baixo, minimapa, topo.
fn blocked(x: i32, y: i32) -> bool {
    (80..=390).contains(&x) && (135..=625).contains(&y)
        || !(75..=680).contains(&y)
        || !(15..=1010).contains(&x)
        || ((x - 910).pow(2) + (y - 555).pow(2)) < 100 * 100
}

/// Modelo pixel -> tile medido em 08/11 (~1 tile a cada 80-95 px).
const MODEL: [[f64; 2]; 2] = [[0.01056, 0.00878], [0.00688, -0.01186]];

fn to_pixels(d: (f64, f64), angle: f64, radius: Option<f64>, min_len: f64, max_len: f64) -> (i32, i32) {
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

static CLEAR_SKILL: std::sync::Mutex<Option<std::collections::HashMap<isize, u8>>> = std::sync::Mutex::new(None);

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

static CHAR_NAMES: std::sync::Mutex<Option<std::collections::HashMap<isize, String>>> = std::sync::Mutex::new(None);

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
fn objects_near(hwnd: isize, pos: (i32, i32)) -> Option<Vec<zen_core::objects::Obj>> {
    let name = CHAR_NAMES.lock().ok()?.as_ref()?.get(&hwnd)?.clone();
    super::objects::nearby_hwnd(hwnd, &name, pos)
}

/// Algum mob a até `r` tiles? `None` = não sabe (sem leitura da memória).
fn mob_near(hwnd: isize, pos: (i32, i32), r: i32) -> Option<bool> {
    let v = objects_near(hwnd, pos)?;
    Some(v.iter().any(|o| o.kind == zen_core::objects::Kind::Monster && cheb((o.x, o.y), pos) <= r))
}

fn clear_skill(hwnd: isize) -> Option<u8> {
    CLEAR_SKILL.lock().ok()?.as_ref()?.get(&hwnd).copied()
}

/// Dá para limpar com a skill aqui? (tecla configurada e fora da zona segura)
fn can_clear(hwnd: isize, t: &Terrain, p: (i32, i32)) -> bool {
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

const ICE_RANGE: i32 = 6;
const ICE_RADIUS: i32 = 4;

static LEARNED_KEYS: std::sync::Mutex<Vec<(isize, u8)>> = std::sync::Mutex::new(Vec::new());
static NO_ICE: std::sync::Mutex<Option<std::collections::HashMap<isize, std::time::Instant>>> = std::sync::Mutex::new(None);

/// Teclas descobertas desde a última chamada (janela, tecla; 0 = este char não tem Ice Storm).
pub fn take_learned_keys() -> Vec<(isize, u8)> {
    LEARNED_KEYS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

fn no_ice_recently(hwnd: isize) -> bool {
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
fn ensure_ice(hwnd: isize, status: &dyn Fn(String)) -> Option<u8> {
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
fn mob_tiles(hwnd: isize, pos: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    Some(objects_near(hwnd, pos)?.into_iter().filter(|o| o.kind == zen_core::objects::Kind::Monster).map(|o| (o.x, o.y)).collect())
}

/// Melhor ponto para o Ice Storm (`combat::aim`): no mob que bloqueia (`focus`), ou no ponto
/// com mais mobs no raio dentro do alcance. Sem lista de mobs: em cima do char.
fn ice_target(hwnd: isize, pos: (i32, i32), focus: Option<(i32, i32)>) -> ((i32, i32), usize) {
    let mobs = mob_tiles(hwnd, pos).unwrap_or_default();
    zen_core::combat::aim(pos, &mobs, focus, ICE_RANGE, ICE_RADIUS)
}

/// Mob parado na rota daqui até `target` (até 3 tiles à frente). `None` = livre ou sem leitura.
fn blocker(hwnd: isize, t: &Terrain, p: (i32, i32), target: (i32, i32), look: usize, reach: i32) -> Option<(i32, i32)> {
    let path = t.path(p, target)?;
    let mobs = mob_tiles(hwnd, p)?;
    zen_core::combat::blocking_mob(p, &path, &mobs, look, reach)
}

/// Tem como soltar a skill aqui (tecla conhecida ou ainda dá para procurar; fora da safe)?
fn skill_ok(hwnd: isize, t: &Terrain, p: (i32, i32)) -> bool {
    (clear_skill(hwnd).is_some() || !no_ice_recently(hwnd)) && !t.is_safezone(p.0, p.1)
}

/// Segura a skill de área em cima do char por `ms` (só fora da zona segura). Devolve se usou.
fn clear_mobs(hwnd: isize, t: &Terrain, ms: u32, focus: Option<(i32, i32)>, status: &dyn Fn(String)) -> bool {
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
const HOLD_RADIUS: f64 = 120.0;

/// Ponto do cursor para andar segurando, desviando do Safety Guardian do baú do Kanturu: o
/// botão segurado em cima dele abre o baú (x9901 em 10/11 21:03: abriu 3 vezes saindo da
/// quina (27,217)). Perto do baú, se o ponto cair a menos de 60 px do NPC, gira a direção.
fn hold_px(pos: (i32, i32), d: (f64, f64), angle: f64) -> (i32, i32) {
    avoid_npc(pos, |a| to_pixels(d, angle + a, Some(HOLD_RADIUS), 0.0, 0.0))
}

/// O ponto `q` da tela cai no Safety Guardian (corpo + baú)? 60 px de raio não bastou: o
/// ReDMooN (21:19) abriu o baú 12 vezes em 25 s parado em (33,217). A área clicável é alta:
/// elipse de 80 x 115 px centrada 45 px acima do pé do NPC.
fn on_npc(pos: (i32, i32), q: (i32, i32)) -> bool {
    let inside = |n: (i32, i32)| {
        let (dx, dy) = ((q.0 - n.0) as f64 / 80.0, (q.1 - (n.1 - 45)) as f64 / 115.0);
        dx * dx + dy * dy < 1.0
    };
    let world = CUR_WORLD.load(std::sync::atomic::Ordering::SeqCst);
    if world == KANTURU_WORLD && cheb(pos, VAULT_TILE) <= 10 && inside(npc_on_screen(pos)) {
        return true;
    }
    if let Ok(g) = LIVE_NPCS.lock() {
        if g.iter().any(|t| cheb(pos, *t) <= 10 && inside(npc_px(pos, (t.0 as f64 + 0.5, t.1 as f64 + 0.5)))) {
            return true;
        }
    }
    learned_npcs(world).into_iter().any(|t| cheb(pos, t) <= 10 && inside(npc_px(pos, (t.0 as f64, t.1 as f64))))
}

// ---------------------------------------------------------------- janela de NPC aberta (11/11)
// Toda janela de NPC (Trainer, Chaos Goblin, ...) tem a mesma moldura de cima em x 333..690,
// y 122..133: igual pixel a pixel entre as janelas e bem diferente com tudo fechado (medido nos
// prints de 11/11). O YouSee abriu o Chaos Goblin andando até o Trainer e ficou preso.
const NPC_TOP: &[u8; 358 * 12] = include_bytes!("npc_dialog_top.gray");

/// "OK" das caixas de aviso de NPC (ex.: Shadow Phantom Soldier em Devias, 11/11 22:37: "You can
/// now stand alone without my support." travou o x9901: nem andava nem abria o M). Texto branco
/// (menor canal >= 190) achado em (499,307) no print; a caixa pode mudar de altura.
const MSG_OK: &str = "\
................................
................................
.......###......................
.........##.......###.....##....
...##.....####....###.....#.....
..##.......###....###.....#.....
..##........##....###...........
..##........##....###...........
..##........##....###..##.......
..##........##....###.###.......
..##........##....########......
..##........##....####..###.....
..##........##....###...###.....
..##........##....###....###....
..##.......###....###.....###...
..###......##.....###.....###...
...###............###......###..
.....#####........##........##..
................................
................................";

/// Caixa de aviso com botão OK aberta? Devolve o centro do OK.
fn msgbox_ok(hwnd: isize) -> Option<(i32, i32)> {
    let t = Bits::from_text(MSG_OK);
    let (x0, y0, w, h) = (485i32, 230i32, 62i32, 210i32);
    let img = capture::capture(hwnd, Some([x0, y0, w, h]))?;
    let b = binarize_white(&img.bgra, img.w as usize, img.h as usize, 190);
    for oy in 0..=(b.h.saturating_sub(t.h)) {
        for ox in 0..=(b.w.saturating_sub(t.w)) {
            let mut d = 0;
            'm: for yy in 0..t.h {
                for xx in 0..t.w {
                    if b.px[(oy + yy) * b.w + ox + xx] != t.px[yy * t.w + xx] {
                        d += 1;
                        if d > 14 {
                            break 'm;
                        }
                    }
                }
            }
            if d <= 14 {
                return Some((x0 + ox as i32 + t.w as i32 / 2, y0 + oy as i32 + t.h as i32 / 2));
            }
        }
    }
    None
}

/// Alguma janela de NPC (ou caixa de aviso com OK) aberta no meio da tela?
pub fn npc_dialog_open(hwnd: isize) -> bool {
    if msgbox_ok(hwnd).is_some() {
        return true;
    }
    npc_frame_open(hwnd) || side_npc_close(hwnd).is_some() || chaos_shop_open(hwnd)
}

const SIDE_CLOSE: &[u8;1600] = include_bytes!("npc_side_close.gray");
const CHAOS_TITLE: &[u8;1984] = include_bytes!("npc_chaos_title.gray");

fn gray_template_at(img: &capture::Image, template: &[u8], w: usize, h: usize) -> Option<(i32,i32)> {
    if (img.w as usize)<w || (img.h as usize)<h { return None; }
    let mut best=None;
    for y in 0..=img.h as usize-h { for x in 0..=img.w as usize-w {
        let mut error=0u32;
        for ty in 0..h { for tx in 0..w {
            let i=((y+ty)*img.w as usize+x+tx)*4;
            let g=(img.bgra[i+2] as u32*299+img.bgra[i+1] as u32*587+img.bgra[i] as u32*114)/1000;
            error+=g.abs_diff(template[ty*w+tx] as u32);
        }}
        if error/(w*h) as u32<=12 && best.map_or(true,|(e,_,_)|error<e) { best=Some((error,x as i32,y as i32)); }
    }}
    best.map(|(_,x,y)|(x,y))
}
fn side_npc_caption(img: &capture::Image) -> bool {
    img.bgra.chunks_exact(4).filter(|p|p[1]>130 && (p[2] as u32)*10<(p[1] as u32)*7 && p[0]>80).count()>=80
}
fn side_npc_close(hwnd: isize) -> Option<(i32,i32)> {
    // Título de NPC em ciano + botão X. Inventário tem o mesmo X, mas não esse título.
    let header=capture::capture(hwnd,Some([750,35,240,25]))?;
    if !side_npc_caption(&header) { return None; }
    let close=capture::capture(hwnd,Some([735,640,120,80]))?;
    let (x,y)=gray_template_at(&close,SIDE_CLOSE,40,40)?;
    Some((735+x+20,640+y+20))
}
fn chaos_shop_open(hwnd: isize) -> bool {
    capture::capture(hwnd,Some([205,41,134,26])).is_some_and(|img|gray_template_at(&img,CHAOS_TITLE,124,16).is_some())
}

fn npc_frame_open(hwnd: isize) -> bool {
    let (w, h) = (358usize, 12usize);
    let s = 3i32;
    let Some(img) = capture::capture(hwnd, Some([333 - s, 122 - s, w as i32 + 2 * s, h as i32 + 2 * s])) else { return false };
    let iw = img.w as usize;
    let gray = |x: usize, y: usize| {
        let i = (y * iw + x) * 4;
        (u32::from(img.bgra[i + 2]) * 299 + u32::from(img.bgra[i + 1]) * 587 + u32::from(img.bgra[i]) * 114) / 1000
    };
    for oy in 0..=(2 * s) as usize {
        for ox in 0..=(2 * s) as usize {
            let mut d = 0u32;
            for y in 0..h {
                for x in 0..w {
                    d += gray(ox + x, oy + y).abs_diff(u32::from(NPC_TOP[y * w + x]));
                }
            }
            if d / (w * h) as u32 <= 15 {
                return true;
            }
        }
    }
    false
}

/// Fecha a janela de NPC (ESC) se houver uma. Devolve se fechou.
fn close_npc_dialog(hwnd: isize) -> bool {
    let mut handled=false;
    for _ in 0..3 {
        let step=if let Some((x,y))=msgbox_ok(hwnd).or_else(||side_npc_close(hwnd)) {
            Step::Click{x,y}
        } else if npc_frame_open(hwnd) || chaos_shop_open(hwnd) { Step::Key(0x1B) }
        else { return handled; };
        if input::run(hwnd,&[step]).is_err(){return false;}
        handled=true;
        sleep(400);
    }
    handled && !npc_dialog_open(hwnd)
}

/// NPCs vivos perto do char, lidos da memória no começo de cada trecho segurado.
static LIVE_NPCS: std::sync::Mutex<Vec<(i32, i32)>> = std::sync::Mutex::new(Vec::new());

fn refresh_live_npcs(hwnd: isize, pos: (i32, i32)) {
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
static CUR_WORLD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static LEARNED: std::sync::Mutex<Option<std::collections::BTreeMap<u8, Vec<(i32, i32)>>>> = std::sync::Mutex::new(None);

fn learned_path() -> std::path::PathBuf {
    crate::config::AgentConfig::dir().join("npcs_aprendidos.json")
}

fn with_learned<R>(f: impl FnOnce(&mut std::collections::BTreeMap<u8, Vec<(i32, i32)>>) -> R) -> Option<R> {
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
fn tile_at_px(pos: (i32, i32), q: (i32, i32)) -> (i32, i32) {
    let (px, py) = ((q.0 - CENTER.0) as f64, (q.1 - CENTER.1) as f64);
    let m = MODEL;
    (pos.0 + (m[0][0] * px + m[0][1] * py).round() as i32, pos.1 + (m[1][0] * px + m[1][1] * py).round() as i32)
}

fn learn_npc(world: u8, tile: (i32, i32)) {
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
fn avoid_npc(pos: (i32, i32), at: impl Fn(f64) -> (i32, i32)) -> (i32, i32) {
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
enum WalkArrival { Exact, Spot, Trainer }
use zen_core::spots::ARRIVAL_RADIUS as SPOT_ARRIVAL_TOLERANCE;

fn walk_with_arrival(hwnd: isize, t: &Terrain, target: (i32, i32), arrival: WalkArrival, status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    let dest = Dest::from_world(t.world).ok_or("mapa do terreno não reconhecido")?;
    ensure_action_map(hwnd, dest)?;
    walk_legacy(hwnd, t, target, arrival, status)?;
    confirm_destination(hwnd, dest, Some((target, match arrival { WalkArrival::Spot => SPOT_ARRIVAL_TOLERANCE, WalkArrival::Trainer => 1, WalkArrival::Exact => 0 })))
}

fn final_walk_arrived(pos: (i32, i32), target: (i32, i32), mobs: bool, elapsed_s: u64, arrival: WalkArrival) -> bool {
    pos == target || (arrival == WalkArrival::Trainer && cheb(pos,target) <= 1) || (arrival == WalkArrival::Spot &&
        ((cheb(pos, target) <= 2 && mobs) || (elapsed_s >= 10 && cheb(pos, target) <= 1)))
}

fn walking_route(t: &Terrain, from: (i32,i32), to: (i32,i32), arrival: WalkArrival) -> Option<Vec<(i32,i32)>> {
    if arrival == WalkArrival::Trainer { npc_route(t, from, to) } else { t.path(from, to) }
}

fn walk_legacy(hwnd: isize, t: &Terrain, target: (i32, i32), arrival: WalkArrival, status: &dyn Fn(String)) -> Result<(i32, i32), String> {
    let route = |from, to| walking_route(t, from, to, arrival);
    let t0 = std::time::Instant::now();
    CUR_WORLD.store(t.world, std::sync::atomic::Ordering::SeqCst);
    let mut last = settle(hwnd).ok_or("não li as coordenadas")?;
    let next_wp = |pos: (i32, i32)| -> Option<(f64, f64)> {
        let path = route(pos, target)?;
        let wp = t.waypoints(pos, &path, 6)[0];
        Some(((wp.0 - pos.0) as f64, (wp.1 - pos.1) as f64))
    };
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
        super::spots::check_approach_at(hwnd, t.world, last, false)?;
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
            super::spots::check_movement_epoch()?;
            let Some(p) = position(hwnd).filter(|p| cheb(*p, last) <= 4) else { continue };
            super::spots::check_approach_at(hwnd, t.world, p, false)?;
            if p == last {
                still += 1;
            } else {
                still = 0;
            }
            last = p;
            if cheb(p, target) <= 2 {
                break 'hold;
            }
            let remaining = route(p, target).map(|v| v.len()).unwrap_or(usize::MAX);
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
                    continue 'hold;
                }
            }
            if still >= 3 && !storage_open(hwnd) && npc_dialog_open(hwnd) {
                h.release();
                drop(h);
                status(format!("janela de NPC abriu no caminho em {p:?}: fechando (ESC) e mudando a direção"));
                learn_npc(t.world, tile_at_px(p, cur_px));
                close_npc_dialog(hwnd);
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
        super::spots::check_movement_epoch()?;
        let pos = near(hwnd, last).ok_or("leitura da posição implausível")?;
        super::spots::check_approach_at(hwnd, t.world, pos, false)?;
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
    let exe = super::memory::process_path(pid).ok_or("não achei o executável do jogo")?;
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
pub fn decode_spot_frame(frame: Option<super::capture::Image>) -> (Option<&'static str>, Option<(i32, i32)>) {
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
fn destination_matches(snap: (Option<&str>, Option<(i32, i32)>), dest: Dest, target: Option<((i32, i32), i32)>) -> bool {
    snap.0 == Some(dest.name()) && snap.1.is_some_and(|p| {
        (0..=255).contains(&p.0) && (0..=255).contains(&p.1)
            && target.map_or(true, |(q, tolerance)| cheb(p,q) <= tolerance)
    })
}

fn ensure_action_map(hwnd: isize, dest: Dest) -> Result<(i32, i32), String> {
    if observe_named(hwnd).0 != Some(dest.name()) { teleport(hwnd, dest)?; }
    confirm_destination(hwnd, dest, None)
}

fn confirm_destination(hwnd: isize, dest: Dest, target: Option<((i32, i32), i32)>) -> Result<(i32, i32), String> {
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
    if map_open(hwnd) { input::run(hwnd,&[Step::Key(VK_M)])?;sleep(300); }
    Ok(())
}

fn in_dest(hwnd: isize, d: Dest) -> (Option<bool>, Option<(i32, i32)>) {
    let (name, pos) = observe_named(hwnd);
    (pos.map(|_| name == Some(d.name())), pos)
}

/// Vai até `spot` no mapa `dest` (M se precisar, andando pelo terreno `t`) e liga o helper se
/// não for zona segura. Base da volta ao spot e do "ir até" do painel.
pub fn recover_in(hwnd: isize, t: &Terrain, spot: (i32, i32), dest: Dest, status: &dyn Fn(String)) -> Result<String, String> {
    if t.world!=dest.world() {return Err("terreno não corresponde ao mapa de destino".into());}
    let _session = input::Session::begin(hwnd)?;
    let t0 = std::time::Instant::now();
    let mut log = Vec::new();
    // Baú esquecido aberto (ou item preso no cursor) segura o char parado: fecha antes de tudo.
    if force_close_storage(hwnd) {
        log.push("baú estava aberto: fechado com ESC".to_string());
    }
    if close_npc_dialog(hwnd) {
        log.push("janela de NPC estava aberta: fechada com ESC".to_string());
    }
    let (k, pos) = in_dest(hwnd, dest);
    let pos = pos.ok_or("não li as coordenadas na tela")?;
    if k == Some(false) {
        // Em Lorencia sem o Zen do move: busca no baú antes (regra do dono, 09/11 14:17).
        if dest != Dest::Lorencia && in_lorencia(hwnd) {
            let lor = world_terrain_of(hwnd, LORENCIA_WORLD)?;
            ensure_move_money(hwnd, &lor, false, status)?;
        }
        let gate = best_gate(dest, Some(t), spot);
        status(format!("teleportando: M -> {} (chegada {:?})", dest.name(), gate.arrival));
        let p = teleport_gate(hwnd, dest, gate)?;
        log.push(format!("teleportou para {p:?}"));
    } else if dest.gates().len() > 1 && t.path(pos, spot).is_none() {
        // Mesmo mapa, outro andar sem caminho a pé (Lost Tower/Dungeon): vai pelo M até a entrada certa.
        let gate = best_gate(dest, Some(t), spot);
        status(format!("sem caminho a pé de {pos:?}: M -> {} (chegada {:?})", dest.name(), gate.arrival));
        let p = teleport_gate(hwnd, dest, gate)?;
        log.push(format!("outro andar: teleportou para {p:?}"));
    } else {
        log.push(format!("já no mapa do spot em {pos:?}"));
    }
    let walk_start = ensure_action_map(hwnd, dest)?;
    if !t.is_safezone(walk_start.0, walk_start.1) {
        status("desligando o helper para andar dentro do mapa".into());
        set_helper(hwnd, false)?;
    }
    // Saindo do baú: janela do baú/inventário aberta ou o Safety Guardian no caminho seguravam
    // o char em (33,216) (visto em 09/11 03:49: 25 tentativas). Fecha janelas e sai primeiro
    // para o oeste, longe do NPC, e só então segue para o spot.
    if let Some(p) = position(hwnd).filter(|_| dest == Dest::Kanturu) {
        if cheb(p, VAULT_TILE) <= 4 {
            close_storage(hwnd);
            status("saindo de perto do baú".into());
            let _ = walk(hwnd, t, VAULT_EXIT, status);
        }
    }
    status(format!("andando até o spot {spot:?}"));
    // Somente revisões explicitamente instaladas no agente; a main continua no caminho legado.
    if let Some(name)=CHAR_NAMES.lock().ok().and_then(|m|m.as_ref().and_then(|m|m.get(&hwnd).cloned())) {
        if let Some(route)=crate::navigation::recovery_route(&name,zen_core::spots::Spot{world:t.world,x:spot.0,y:spot.1}) {
            let context=wait_route_map(hwnd,&name,t.world,status)?;
            ensure_action_map(hwnd, dest)?;
            crate::navigation::scoped(context,&route.id,||super::navigation::walk_points(hwnd,t,&route,status))?;
        }
    }
    let mut walked = walk_with_arrival(hwnd, t, spot, WalkArrival::Spot, status);
    if let Err(e) = &walked { if e.starts_with("SPOT_") { return Err(e.clone()); } }
    // Caminhada longa que parou no meio (limite de 4 min, janela que perdeu o foco): AMUMU em
    // 15/11 parou em (112,44) e (68,63) e só chegou na 3ª recuperação. Continua de onde parou,
    // no mesmo mapa, até 2 vezes, em vez de encerrar a volta.
    for _ in 0..2 {
        let Err(e) = &walked else { break };
        let Some(p) = position(hwnd) else { break };
        if cheb(p, spot) <= 8 { break; }
        let e = e.clone();
        confirm_destination(hwnd, dest, None)?;
        tracing::warn!(hwnd, posicao = ?p, erro = %e, "volta ao spot: caminhada parou longe; continuando de onde parou");
        status(format!("caminhada parou em {p:?} ({e}); continuando até o spot"));
        log.push(format!("caminhada parou em {p:?}: {e}"));
        walked = walk_with_arrival(hwnd, t, spot, WalkArrival::Spot, status);
        if let Err(e) = &walked { if e.starts_with("SPOT_") { return Err(e.clone()); } }
    }
    // Perto do spot o clique às vezes cai num mob: o char fica dando ataque normal num mob
    // que não morre e pode morrer (dono, 09/11 15:43). Então, travado perto: liga o helper
    // alguns segundos para limpar, desliga e ajusta a posição de novo (até 2 vezes).
    for round in 0..2 {
        let Err(e) = &walked else { break };
        confirm_destination(hwnd,dest,None)?;
        let Some(p) = position(hwnd) else { break };
        // Já na margem: não limpa nem repete a caminhada; confirma e liga o Helper.
        if cheb(p, spot) <= SPOT_ARRIVAL_TOLERANCE { break; }
        if cheb(p, spot) > 8 || t.is_safezone(p.0, p.1) {
            break;
        }
        if !clear_mobs(hwnd, t, 4000, None, status) {
            status(format!("travado em {p:?} perto do spot ({e}): helper ligado 8 s para limpar os mobs ({})", round + 1));
            if set_helper(hwnd, true).is_err() {
                break;
            }
            sleep(8000);
            let _ = set_helper(hwnd, false);
        }
        walked = walk_with_arrival(hwnd, t, spot, WalkArrival::Spot, status);
        if let Err(e) = &walked { if e.starts_with("SPOT_") { return Err(e.clone()); } }
    }
    // O erro real da caminhada vai junto (antes só aparecia "destino não confirmado").
    let arrival = confirm_destination(hwnd, dest, Some((spot, SPOT_ARRIVAL_TOLERANCE)))
        .map_err(|c| match &walked { Err(w) => format!("{c} (caminhada: {w})"), Ok(_) => c })?;
    // Ocupação relida uma vez no destino confirmado, antes de habilitar o Helper.
    super::spots::check_approach_at(hwnd, dest.world(), arrival, true)?;
    let safe = t.is_safezone(arrival.0, arrival.1);
    if !safe {
        status("ligando o helper no spot".into());
        // O Helper assume a limpeza: sem novas caminhadas ou rodadas de skill na margem.
        set_helper(hwnd, true)?;
    }
    if let Err(e) = walked {
        log.push(format!("chegada recuperada dentro da margem: {e}"));
    }
    log.push(format!("chegou em {arrival:?}, a {} tile(s) do spot, em {:?}{}", cheb(arrival, spot), t0.elapsed(), if safe { "; zona segura: helper não ligado" } else { "; helper ligado" }));
    Ok(log.join("; "))
}

// ---------------------------------------------------------------- baú (Safety Guardian)
// Posições medidas na gravação de 08/11 (depósito feito à mão), em coordenadas da janela
// inteira 1030x797, como o resto deste arquivo.

/// Tile de onde o baú foi aberto (zona segura do Kanturu, perto de onde o move deixa o char).
pub const VAULT_TILE: (i32, i32) = (33, 217);
/// Tile livre a oeste do baú (caminho de quem chega do teleporte), longe do NPC.
const VAULT_EXIT: (i32, i32) = (27, 217);
/// Corpo do Safety Guardian na tela com o char parado em VAULT_TILE.
const VAULT_NPC: (i32, i32) = (565, 445);
/// Campo "Zen" do baú (aba atual) e do inventário (mesma fonte vermelha).
const STORAGE_ZEN: [i32; 4] = [610, 584, 80, 14];
const INV_ZEN: [i32; 4] = [786, 609, 78, 14];
/// Setas "aba anterior" e "próxima aba" no topo do baú. NÃO dão a volta: na 15 a seta
/// direita não faz nada, na 1 a esquerda não faz nada (medido em 08/11).
const STORAGE_PREV: (i32, i32) = (511, 49);
const STORAGE_NEXT: (i32, i32) = (626, 49);
/// Botão "Deposit" (primeiro botão embaixo do baú).
const STORAGE_DEPOSIT: (i32, i32) = (476, 672);
/// Caixa "Enter the amount of Zen you would like to deposit.": campo, OK, CANCEL.
const DEPOSIT_INPUT: (i32, i32) = (514, 285);
const DEPOSIT_OK: (i32, i32) = (421, 340);
const DEPOSIT_CANCEL: (i32, i32) = (605, 340);
const VK_BACK: u16 = 0x08;

fn read_zen(hwnd: isize, region: [i32; 4]) -> Option<u64> {
    let img = capture::capture(hwnd, Some(region))?;
    zen_core::glyphs::read_number(&zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize))
}

/// Duas leituras iguais com 300 ms de intervalo (o número pode estar mudando).
fn read_zen_stable(hwnd: isize, region: [i32; 4]) -> Option<u64> {
    for _ in 0..6 {
        let a = read_zen(hwnd, region);
        sleep(300);
        let b = read_zen(hwnd, region);
        if a.is_some() && a == b {
            return a;
        }
    }
    None
}

/// "Storage N/15" no topo do baú, binarizado (texto branco). `None` = não capturou.
pub fn storage_title(hwnd: isize) -> Option<Bits> {
    let r = zen_core::vault::STORAGE_TITLE;
    let img = capture::capture(hwnd, Some(r))?;
    Some(binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::vault::TITLE_WHITE))
}

/// Número da aba na tela (1..15), lido no título.
pub fn storage_tab(hwnd: isize) -> Option<u8> {
    storage_title(hwnd).and_then(|b| zen_core::vault::read_tab(&b))
}

/// Onde o Safety Guardian aparece na tela com o char em `at` (o NPC fica parado; o modelo
/// pixel->tile dá o deslocamento).
fn npc_on_screen(at: (i32, i32)) -> (i32, i32) {
    let (dx, dy) = ((VAULT_TILE.0 - at.0) as f64, (VAULT_TILE.1 - at.1) as f64);
    let m = MODEL;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let (px, py) = ((m[1][1] * dx - m[0][1] * dy) / det, (-m[1][0] * dx + m[0][0] * dy) / det);
    (VAULT_NPC.0 + px.round() as i32, VAULT_NPC.1 + py.round() as i32)
}

/// Último print de erro do baú (o agente manda para o central, que guarda para análise).
static ERROR_SHOT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// Prints passo a passo (treino do K/J pedido pelo dono): ligado só durante `fortune_bank`.
static TRACE: std::sync::Mutex<Option<Vec<(String, Vec<u8>)>>> = std::sync::Mutex::new(None);

fn trace(hwnd: isize, tag: &str) {
    let on = TRACE.lock().map(|g| g.is_some()).unwrap_or(false);
    if !on {
        return;
    }
    if let Some(png) = capture::capture(hwnd, None).and_then(|i| i.to_png()) {
        if let Ok(mut g) = TRACE.lock() {
            if let Some(v) = g.as_mut() {
                if v.len() < 12 {
                    v.push((tag.to_string(), png));
                }
            }
        }
    }
}

/// Prints do último treino (cada um: etiqueta, PNG).
pub fn take_trace() -> Vec<(String, Vec<u8>)> {
    TRACE.lock().ok().and_then(|mut g| g.take()).unwrap_or_default()
}

/// Só o K -> J, no lugar onde o char estiver (sem ir à cidade), com prints de cada passo.
pub fn fortune_bank(hwnd: isize, status: &dyn Fn(String)) -> Result<String, String> {
    let _session = input::Session::begin(hwnd)?;
    if let Ok(mut g) = TRACE.lock() {
        *g = Some(Vec::new());
    }
    // O helper pode continuar ligado: K/J funcionam com ele rodando (dono, 09/11 16:33).
    trace(hwnd, "0-inicio");
    let r = gremory_to_bank(hwnd, status);
    trace(hwnd, "9-fim");
    r.map(|n| format!("{n} Fortuna(s) do K para o inventário e Deposit All no J conferido"))
}

/// Treino só do J (ponte, x9901): abre o J, tira 1 Fortuna ("Withdraw 1") e guarda tudo de
/// novo ("Deposit All"), com prints. Devolve os prints (etiqueta, PNG).
pub fn bank_train(hwnd: isize, status: &dyn Fn(String)) -> (Vec<(String, Vec<u8>)>, Result<(), String>) {
    let _session = match input::Session::begin(hwnd) {
        Ok(s) => s,
        Err(e) => return (Vec::new(), Err(e)),
    };
    if let Ok(mut g) = TRACE.lock() {
        *g = Some(Vec::new());
    }
    let r = (|| -> Result<(), String> {
        close_gremory(hwnd);
        if !bank_open(hwnd) {
            input::run(hwnd, &[Step::Key(VK_J)])?;
            let t0 = std::time::Instant::now();
            while t0.elapsed().as_millis() < 3000 && !bank_open(hwnd) {
                sleep(250);
            }
        }
        trace(hwnd, "1-j-aberto");
        let row = bank_row(hwnd).ok_or("não achei a linha da Fortuna no J")?;
        status(format!("J: linha da Fortuna em {row:?}"));
        input::run(hwnd, &[Step::Click { x: row.0 + 21, y: row.1 + 12 }])?;
        let t0 = std::time::Instant::now();
        let mut dep = None;
        while t0.elapsed().as_millis() < 4000 && dep.is_none() {
            sleep(250);
            dep = bank_menu(hwnd);
        }
        let dep = dep.ok_or("o menu da Fortuna não abriu")?;
        status(format!("J: Deposit All em {dep:?}"));
        trace(hwnd, "2-menu");
        // "Withdraw 1" fica 184 px acima do "Deposit All" (print de 09/11: 214 x 398).
        input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 5 - 184 }])?;
        sleep(2000);
        trace(hwnd, "3-retirou-1");
        if bank_menu(hwnd).is_none() {
            let row = bank_row(hwnd).ok_or("depois do Withdraw 1 não achei a linha da Fortuna")?;
            input::run(hwnd, &[Step::Click { x: row.0 + 21, y: row.1 + 12 }])?;
            sleep(1500);
        }
        let dep = bank_menu(hwnd).ok_or("o menu não voltou para o Deposit All")?;
        input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 5 }])?;
        sleep(2000);
        trace(hwnd, "4-depositou");
        input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 42 }])?;
        sleep(700);
        for _ in 0..2 {
            if !bank_open(hwnd) {
                break;
            }
            input::run(hwnd, &[Step::Key(VK_J)])?;
            sleep(700);
        }
        trace(hwnd, "5-fechou");
        Ok(())
    })();
    (take_trace(), r)
}

pub fn take_error_shot() -> Option<Vec<u8>> {
    ERROR_SHOT.lock().ok().and_then(|mut g| g.take())
}

fn save_error_shot(hwnd: isize, why: &str) {
    if let Some(png) = capture::capture(hwnd, None).and_then(|i| i.to_png()) {
        let p = crate::config::AgentConfig::dir().join("logs").join(format!(
            "bau-erro-{}.png",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
        ));
        let _ = std::fs::write(&p, &png);
        tracing::warn!(motivo = why, print = %p.display(), "baú: print salvo");
        if let Ok(mut g) = ERROR_SHOT.lock() {
            *g = Some(png);
        }
    }
}

pub fn storage_open(hwnd: isize) -> bool {
    read_zen(hwnd, STORAGE_ZEN).is_some()
}

/// Fecha baú + inventário (V) e confere. Nunca deixa o baú aberto.
/// Se o V não fechar (item preso no cursor depois de um clique no baú: x9901 em 10/11 19:41),
/// ESC força a saída (dica do dono). ESC só com o baú CONFERIDO aberto: com nada aberto ele
/// abre o menu do jogo.
fn close_storage(hwnd: isize) {
    for _ in 0..2 {
        if !storage_open(hwnd) && !inventory_open(hwnd) {
            return;
        }
        let _ = input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)]);
        sleep(800);
    }
    force_close_storage(hwnd);
}

const VK_ESCAPE: u16 = 0x1B;

/// Baú aberto fora de hora (o clique de andar caiu no Safety Guardian e pegou um item):
/// ESC até 2 vezes, conferindo antes de cada uma. Devolve se o baú estava aberto.
fn force_close_storage(hwnd: isize) -> bool {
    if !storage_open(hwnd) {
        return false;
    }
    for i in 0..2 {
        if !storage_open(hwnd) {
            break;
        }
        save_error_shot(hwnd, &format!("baú aberto no caminho: ESC ({})", i + 1));
        let _ = input::run(hwnd, &[Step::Key(VK_ESCAPE)]);
        sleep(700);
    }
    if inventory_open(hwnd) {
        let _ = input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)]);
        sleep(600);
    }
    true
}

pub struct VaultReport {
    /// (número da aba lido na tela, Zen da aba), na ordem em que foram vistas.
    pub tabs: Vec<(Option<u8>, u64)>,
    /// Título de cada aba binarizado (para montar os modelos dos números).
    pub titles: Vec<String>,
    /// Aba usada (1..15) e valor depositado.
    pub deposited: Option<(usize, u64)>,
    pub zen_before: u64,
    pub zen_after: u64,
}

/// Leva o char ao baú, abre, lê as abas e (se `deposit`) deposita tudo menos `reserve` na
/// primeira aba onde o valor inteiro cabe. Confere o resultado na tela. Não volta ao spot.
/// `read_all`: lê as 15 abas mesmo depois de achar uma (só para diagnóstico).
pub fn vault(hwnd: isize, t: &Terrain, reserve: u64, deposit: bool, read_all: bool, status: &dyn Fn(String)) -> Result<VaultReport, String> {
    let _session = input::Session::begin(hwnd)?;
    use zen_core::vault::{amount, deposit_ok, digit_keys, fits};
    let (k, pos) = observe_map(hwnd);
    let pos = pos.ok_or("não li as coordenadas na tela")?;
    if k != Some(true) || cheb(pos, VAULT_TILE) > 25 {
        status("baú: M -> Kanturu".into());
        teleport_kanturu(hwnd)?;
    }
    set_helper(hwnd, false)?;
    status("baú: andando até o Safety Guardian".into());
    // O tile do baú às vezes não aceita o clique final (visto em 08/11: travou 1 tile antes).
    // Ao lado serve: o clique no NPC é corrigido pela diferença.
    let at = match walk(hwnd, t, VAULT_TILE, status) {
        Ok(p) => p,
        Err(e) => match position(hwnd) {
            Some(p) if cheb(p, VAULT_TILE) <= 1 => p,
            _ => return Err(e),
        },
    };
    if cheb(at, VAULT_TILE) > 1 {
        return Err(format!("parou em {at:?}, longe de {VAULT_TILE:?}"));
    }
    if inventory_open(hwnd) {
        input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
        sleep(600);
    }
    // ---- abrir
    // Um clique fora do NPC faz o char ANDAR (visto em 09/11 no LOBOLIXO: parou em (33,218), o
    // clique corrigido errou e o segundo clique já era de outra posição). Então: a cada
    // tentativa, volta ao tile do baú, recalcula onde o NPC aparece e clica uma vez.
    status("baú: abrindo".into());
    let mut opened = false;
    let mut at = at;
    for attempt in 0..3 {
        if attempt > 0 {
            if storage_open(hwnd) {
                opened = true;
                break;
            }
            at = match walk(hwnd, t, VAULT_TILE, status) {
                Ok(p) => p,
                Err(_) => position(hwnd).unwrap_or(at),
            };
            if cheb(at, VAULT_TILE) > 1 {
                break;
            }
        }
        at = confirm_destination(hwnd, Dest::Kanturu, Some((VAULT_TILE,1)))?;
        let npc = npc_on_screen(at);
        status(format!("baú: char em {at:?}, clicando no NPC em {npc:?} (tentativa {})", attempt + 1));
        input::run(hwnd, &[Step::Click { x: npc.0, y: npc.1 }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 6000 {
            sleep(400);
            if storage_open(hwnd) {
                opened = true;
                break;
            }
        }
        if opened {
            break;
        }
        save_error_shot(hwnd, &format!("baú não abriu na tentativa {}", attempt + 1));
    }
    if !opened {
        close_storage(hwnd);
        return Err(format!("o baú não abriu (clique no Safety Guardian, 3 tentativas): {BAU_DICA}"));
    }
    sleep(500);
    let result = (|| -> Result<VaultReport, String> {
        let zen_before = read_zen_stable(hwnd, INV_ZEN).ok_or("não li o Zen do inventário")?;
        let want = amount(zen_before, reserve);
        let mut rep = VaultReport { tabs: Vec::new(), titles: Vec::new(), deposited: None, zen_before, zen_after: zen_before };
        let mut chosen = None;
        // O baú abre na última aba vista. Vai até a 1 (seta esquerda até o título parar de
        // mudar), confere "1" no título e anda para a direita contando: o número é exato.
        let title = |what: &str| storage_title(hwnd).ok_or_else(|| format!("não li o título do baú ({what})"));
        let mut prev = title("início")?;
        let mut tab = None;
        // Até 15 abas para trás; um clique pode se perder (visto em 08/11 22:00 no ReDMooN):
        // só conclui "parou" depois de 3 cliques seguidos sem o título mudar.
        let mut misses = 0;
        for _ in 0..30 {
            if zen_core::vault::read_tab(&prev) == Some(1) {
                tab = Some(1u8);
                break;
            }
            input::run(hwnd, &[Step::Click { x: STORAGE_PREV.0, y: STORAGE_PREV.1 }])?;
            sleep(600 + 300 * misses);
            let t = title("voltando")?;
            if zen_core::vault::same_title(&t, &prev) {
                misses += 1;
                if misses >= 3 {
                    break;
                }
                continue;
            }
            misses = 0;
            prev = t;
        }
        if tab.is_none() && zen_core::vault::read_tab(&prev) == Some(1) {
            tab = Some(1);
        }
        // Cliente em português ("Baú N/15", visto em 09/11 no ELISMAR): os modelos do título
        // são do inglês e não reconhecem nenhum número. A seta não passa da aba 1, então 3
        // cliques seguidos sem mudar = aba 1. O número vira contagem (só para o aviso); a
        // escolha da aba continua pelo Zen lido na tela.
        if tab.is_none() && misses >= 3 && zen_core::vault::read_tab(&prev).is_none() {
            status("baú: título em outro idioma; aba 1 pela contagem".into());
            tab = Some(1);
        }
        let Some(mut tab) = tab else {
            let seen = zen_core::vault::read_tab(&prev).map(|n| format!("o título diz aba {n}")).unwrap_or_else(|| "título não reconhecido".into());
            return Err(format!("não consegui voltar para a aba 1 do baú ({seen})"));
        };
        loop {
            let z = read_zen_stable(hwnd, STORAGE_ZEN).ok_or_else(|| format!("não li o Zen da aba {tab}"))?;
            rep.titles.push(prev.to_text());
            rep.tabs.push((Some(tab), z));
            status(format!("baú: aba {tab} tem {}", zen_core::fmt::int_br(z)));
            if chosen.is_none() && deposit && fits(z, want) {
                chosen = Some((tab as usize, z));
                if !read_all {
                    break;
                }
            }
            if tab >= 15 {
                break;
            }
            let mut moved = None;
            for _ in 0..2 {
                input::run(hwnd, &[Step::Click { x: STORAGE_NEXT.0, y: STORAGE_NEXT.1 }])?;
                sleep(600);
                let t = title("avançando")?;
                if !zen_core::vault::same_title(&t, &prev) {
                    moved = Some(t);
                    break;
                }
            }
            let t = moved.ok_or_else(|| format!("a seta não passou da aba {tab}"))?;
            tab += 1;
            if let Some(n) = zen_core::vault::read_tab(&t) {
                if n != tab {
                    return Err(format!("contagem das abas errada: esperava {tab}, o título diz {n}"));
                }
            }
            prev = t;
        }
        if !deposit {
            return Ok(rep);
        }
        if want == 0 {
            return Err(format!("nada a depositar: char tem {} e a reserva é {}", zen_core::fmt::int_br(zen_before), zen_core::fmt::int_br(reserve)));
        }
        let Some((tab, stored)) = chosen else {
            // nenhuma aba cabe
            return Err(format!("nenhuma das 15 abas cabe {} (baú cheio?)", zen_core::fmt::int_br(want)));
        };
        if read_all {
            return Err("read_all com depósito não é permitido (a aba atual não é a escolhida)".into());
        }
        // ---- depositar
        status(format!("baú: depositando {} na aba {tab}", zen_core::fmt::int_br(want)));
        input::run(hwnd, &[Step::Click { x: STORAGE_DEPOSIT.0, y: STORAGE_DEPOSIT.1 }])?;
        sleep(900);
        let mut steps = vec![Step::Click { x: DEPOSIT_INPUT.0, y: DEPOSIT_INPUT.1 }, Step::WaitMs(200)];
        for _ in 0..12 {
            steps.push(Step::Key(VK_BACK));
        }
        steps.extend(digit_keys(want).into_iter().map(Step::Key));
        steps.push(Step::WaitMs(300));
        if let Err(e) = input::run(hwnd, &steps) {
            let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
            return Err(format!("falhou ao digitar: {e}"));
        }
        input::run(hwnd, &[Step::Click { x: DEPOSIT_OK.0, y: DEPOSIT_OK.1 }])?;
        sleep(1500);
        let after_sto = read_zen_stable(hwnd, STORAGE_ZEN);
        let after_inv = read_zen_stable(hwnd, INV_ZEN);
        match (after_sto, after_inv) {
            (Some(s), Some(z)) if deposit_ok(stored, s, zen_before, z, want) => {
                rep.deposited = Some((tab, want));
                rep.zen_after = z;
                Ok(rep)
            }
            (s, z) => {
                // A caixa pode ter ficado aberta: CANCEL não faz mal se já fechou.
                let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
                Err(format!(
                    "depósito NÃO conferiu: aba {tab} {} -> {:?}, char {} -> {:?} (esperado +/- {})",
                    zen_core::fmt::int_br(stored),
                    s,
                    zen_core::fmt::int_br(zen_before),
                    z,
                    zen_core::fmt::int_br(want)
                ))
            }
        }
    })();
    if let Err(e) = &result {
        save_error_shot(hwnd, e);
    }
    close_storage(hwnd);
    result
}

// =====================================================================================
// Fortuna do Imperador (Lorencia). Testado à mão pela ponte no ReDMooN em 09/11/2026 13:55-14:11:
// move Lorencia (2.000 zen), baú (saque, taxa 200 tirada do inventário), Lugard (clicar com
// >= 1B no inventário converte 1B na hora; a Fortuna vai para o Gremory Case, tecla K),
// K -> linha "Fortuna do Imperador" -> "Use It" (vai para o inventário), J -> Item Bank ->
// "Fortuna do Imperador" -> "Deposit All". Volta: move Kanturu custa 20M.
// Regras do dono (09/11 14:15-14:17): volta com 20M; se faltar para o move, busca no baú;
// a aba 1 do baú guarda sempre 1B (reserva para o move/emergência), nunca convertida.
// =====================================================================================

/// Pasta World de Lorencia.
pub const LORENCIA_WORLD: u8 = 1;
/// Botão "Lorencia" na janela do M.
const M_LORENCIA: (i32, i32) = (250, 503);
/// Tile de onde se clica no Safety Guardian de Lorencia.
pub const LOR_VAULT_TILE: (i32, i32) = (146, 112);
/// Tile (estimado pelo modelo pixel->tile em 5 cliques reais) do Safety Guardian e do Lugard.
const LOR_GUARD: (f64, f64) = (144.5, 110.5);
pub const LUGARD_TILE: (i32, i32) = (149, 114);
const LUGARD: (f64, f64) = (149.0, 115.9);
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
fn fortune_borrow(need: u64, tab1: u64, floor: u64) -> u64 {
    if tab1 <= floor || need > tab1 - floor { 0 } else { need }
}
const WITHDRAW_FEE: u64 = 200;
/// Folga aceita na taxa do saque (a taxa real já passou de 6.000).
const WITHDRAW_SLACK: u64 = 10_000;
/// Botão "Withdraw" (segundo embaixo do baú). A caixa do valor é a mesma do depósito.
const STORAGE_WITHDRAW: (i32, i32) = (535, 672);
const VK_K: u16 = 0x4B;
const VK_J: u16 = 0x4A;
/// Gremory Case: 1ª linha clicável e passo entre linhas (9 visíveis).
const GREMORY_ROW0_Y: f64 = 158.0;
const GREMORY_ROW_STEP: f64 = 36.75;
const GREMORY_X: (i32, i32) = (692, 76);
const USE_IT: (i32, i32) = (379, 361);
const USE_CANCEL: (i32, i32) = (507, 361);

/// "Fortuna do Imperador" na fonte da lista do Gremory Case e da caixa "Use It".
const T_FORTUNE: &str = "\
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
const T_BANK: &str = "\
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
const T_DEPOSIT_ALL: &str = "\
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
fn bank_menu(hwnd: isize) -> Option<(i32, i32)> {
    text_at(hwnd, [150, 60, 700, 640], T_DEPOSIT_ALL, 2, 14)
}
/// Título "Gremory Case" (igual no cliente em inglês e em português: o "Character Items" não
/// aparece em português, visto no xWizard em 09/11 18:07).
const T_GREMORY: &str = "\
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

fn bin_orange(img: &crate::image::Image) -> Bits {
    let (w, h) = (img.w as usize, img.h as usize);
    let mut b = Bits::new(w, h);
    for (i, p) in img.bgra.chunks_exact(4).take(w * h).enumerate() {
        b.px[i] = p[2] > 150 && p[1] > 90 && p[0] < 90;
    }
    b
}

/// O modelo aparece em `region` (busca em todas as posições)? Menor diferença <= `max`.
fn text_in(hwnd: isize, region: [i32; 4], model: &str, mode: u8, max: usize) -> bool {
    text_at(hwnd, region, model, mode, max).is_some()
}

/// Onde o modelo aparece (canto superior esquerdo, em coordenadas da janela): a posição com
/// menor diferença, se <= `max`.
fn text_at(hwnd: isize, region: [i32; 4], model: &str, mode: u8, max: usize) -> Option<(i32, i32)> {
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

fn gremory_row_y(i: usize) -> i32 {
    (GREMORY_ROW0_Y + GREMORY_ROW_STEP * i as f64).round() as i32
}
fn gremory_open(hwnd: isize) -> bool {
    text_in(hwnd, [405, 74, 92, 26], T_GREMORY, 0, 12)
}
fn gremory_row_is_fortune(hwnd: isize, i: usize) -> bool {
    text_in(hwnd, [192, gremory_row_y(i) - 3, 113, 17], T_FORTUNE, 0, 25)
}
fn use_dialog_is_fortune(hwnd: isize) -> bool {
    text_in(hwnd, [385, 268, 130, 24], T_FORTUNE, 1, 25)
}
/// Linha "Fortuna do Imperador" do Item Bank, procurada na tela toda: a janela do J não abre
/// sempre no mesmo lugar (ELISMAR 09/11 16:01: o clique fixo caiu na linha de baixo).
fn bank_row(hwnd: isize) -> Option<(i32, i32)> {
    text_at(hwnd, [150, 60, 700, 560], T_BANK, 1, 25)
}
fn bank_open(hwnd: isize) -> bool {
    bank_row(hwnd).is_some()
}

/// Está em Lorencia (nome no canto)?
pub fn in_lorencia(hwnd: isize) -> bool {
    corner(hwnd).is_some_and(|b| read_coords(&b).is_some() && zen_core::glyphs::map_is_lorencia(&b))
}

/// Onde o NPC parado no tile `npc` aparece na tela com o char em `at`.
fn npc_px(at: (i32, i32), npc: (f64, f64)) -> (i32, i32) {
    let (dx, dy) = (npc.0 - at.0 as f64, npc.1 - at.1 as f64);
    let m = MODEL;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let (px, py) = ((m[1][1] * dx - m[0][1] * dy) / det, (-m[1][0] * dx + m[0][0] * dy) / det);
    (CENTER.0 + px.round() as i32, CENTER.1 + py.round() as i32)
}

pub(super) fn route_scene(world:u8,npcs:Vec<(i32,i32)>) {
    CUR_WORLD.store(world,std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut live)=LIVE_NPCS.lock(){*live=npcs;}
}
pub(super) fn route_legacy_leg(hwnd:isize,t:&Terrain,target:(i32,i32),spot:bool,status:&dyn Fn(String))->Result<(i32,i32),String>{
    walk_with_arrival(hwnd,t,target,if spot{WalkArrival::Spot}else{WalkArrival::Exact},status)
}
pub(super) fn route_npc_adjust(hwnd:isize,t:&Terrain,target:(i32,i32),status:&dyn Fn(String))->Result<(i32,i32),String>{
    walk_with_arrival(hwnd,t,target,WalkArrival::Trainer,status)
}
pub(super) fn route_step_px(at:(i32,i32),to:(i32,i32))->(i32,i32){
    let direction=((to.0-at.0) as f64,(to.1-at.1) as f64);
    avoid_npc(at,|angle|to_pixels(direction,angle,None,0.0,HOLD_RADIUS))
}
#[cfg(test)]mod navigation_cursor_tests {
    use super::*;
    #[test]fn rota_nova_reutiliza_protecao_de_clique_do_guardian() {
        let previous=CUR_WORLD.swap(KANTURU_WORLD,std::sync::atomic::Ordering::SeqCst);
        let at=(31,216);let raw=npc_step_px(at,(34,217));
        assert!(on_npc(at,raw),"a mira direta cruza a área clicável do Guardian");
        let corrected=route_step_px(at,(34,217));assert!(!on_npc(at,corrected));
        CUR_WORLD.store(previous,std::sync::atomic::Ordering::SeqCst);
    }
}
pub(super) fn route_settle(hwnd:isize)->Result<(i32,i32),String>{npc_still(hwnd)}
fn wait_route_map(hwnd:isize,name:&str,world:u8,status:&dyn Fn(String))->Result<zen_core::routes::ObserverContext,String>{
    let mut pid=0;
    unsafe {windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(windows::Win32::Foundation::HWND(hwnd as *mut _),Some(&mut pid));}
    let waiting=std::time::Instant::now();
    let mut announced=false;
    loop {
        if input::manual_mode(){return Err("automação pausada enquanto aguardava confirmação do mapa".into());}
        if !crate::net::ONLINE.load(std::sync::atomic::Ordering::SeqCst){return Err("NAV_DISCONNECTED: servidor indisponível".into());}
        if let Some(s)=crate::navigation::confirmed_snapshot(name,pid,world){return Ok(s.context);}
        if waiting.elapsed()>=std::time::Duration::from_secs(20){
            tracing::warn!(char=%name,world,pid,"rota: mapa não confirmado após aguardar o coletor");
            return Err("NAV_STALE: mapa não confirmado pelo radar em 20 segundos".into());
        }
        if !announced {status("aguardando confirmação do mapa após o Move, sem repetir cliques".into());announced=true;}
        // Only inspect cached samples; no additional memory reads or captures.
        sleep(200);
    }
}
pub(super) fn route_npc_click(hwnd:isize,at:(i32,i32),npc:&zen_core::routes::NpcTarget,lift:i32)->Result<(),String>{
    if npc.name.eq_ignore_ascii_case("Trainer")&&trainer_text(hwnd,TRAINER_TITLE){return Ok(());}
    let (x,y)=npc_px(at,(npc.location.x as f64,npc.location.y as f64));
    input::run(hwnd,&[Step::Click{x,y:y-lift}])?;
    let start=std::time::Instant::now();
    while start.elapsed().as_secs()<5 {
        if input::manual_mode(){return Err("interação pausada".into());}
        if npc.name.eq_ignore_ascii_case("Trainer") {if trainer_text(hwnd,TRAINER_TITLE){return Ok(());}}
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
    let exe = super::memory::process_path(pid).ok_or("não achei o executável do jogo")?;
    let dir = std::path::Path::new(&exe).parent().ok_or("caminho do jogo inválido")?;
    let f = dir.join(format!("Data\\World{world}\\EncTerrain{world}.att"));
    let raw = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    Terrain::from_client_file(&raw).map_err(|e| e.to_string())
}

fn close_inventory(hwnd: isize) {
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
fn npc_step_px(at: (i32, i32), waypoint: (i32, i32)) -> (i32, i32) {
    let p = npc_px(at, (waypoint.0 as f64, waypoint.1 as f64));
    let (dx, dy) = ((p.0 - CENTER.0) as f64, (p.1 - CENTER.1) as f64);
    let scale = (180.0 / dx.hypot(dy).max(1.0)).min(1.0);
    (CENTER.0 + (dx * scale).round() as i32, CENTER.1 + (dy * scale).round() as i32)
}

/// Não recalcula o clique no NPC enquanto o char ainda estiver andando.
fn npc_still(hwnd: isize) -> Result<(i32, i32), String> {
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

struct NpcProgress {
    target: (i32, i32),
    at: (i32, i32),
    previous: Option<(i32, i32)>,
    stalled: u8,
}

impl NpcProgress {
    fn arrived(&self) -> bool { cheb(self.at, self.target) <= 1 }
    fn advance(&mut self, next: (i32, i32)) -> Result<(), String> {
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
fn npc_route(t: &Terrain, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
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
fn npc_hold_far(at: (i32, i32), target: (i32, i32)) -> bool {
    cheb(at, target) > 3
}

fn recover_route_window(hwnd: isize, dest: Dest, count: &mut usize, status: &dyn Fn(String)) -> Result<(i32,i32),String> {
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
struct NpcDetours { tried: Vec<(i32, i32)> }
impl NpcDetours {
    fn next(&mut self, t: &Terrain, at: (i32,i32), blocked: (i32,i32), target: (i32,i32)) -> Result<(i32,i32),String> {
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

fn walk_near(hwnd: isize, t: &Terrain, tile: (i32, i32), status: &dyn Fn(String)) -> Result<(i32, i32), String> {
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

/// Aviso que vai para o celular (CommandResult de falha) quando o baú não abre: nos testes de
/// 15/11 o ClassicSM não abria o baú porque o cliente estava desatualizado e o launcher
/// pedia atualização. Só o dono consegue resolver; o bot não deve ficar insistindo.
const BAU_DICA: &str = "verifique se o jogo precisa de atualização no launcher (o baú não abriu)";

/// Aproxima uma vez e repete no máximo um clique no baú, sem voltar ao tile.
fn open_lor_vault(hwnd: isize, t: &Terrain, status: &dyn Fn(String)) -> Result<(), String> {
    ensure_action_map(hwnd, Dest::Lorencia)?;
    if storage_open(hwnd) {
        return Ok(());
    }
    close_inventory(hwnd);
    // Já ao lado do Safety Guardian (x9901 16/11 06:32, em (145,109) depois do J): andar até o
    // tile do baú fazia o clique cair no NPC, o baú abria "no caminho", era fechado como janela
    // acidental e a Fortuna falhava. Perto (até 3 tiles) clica direto no Guardian.
    // "Perto" é perto do NPC, não do tile do baú: o Lugard (149,114) também fica a 3 tiles do
    // baú, mas de lá o Guardian cai na borda da tela (x9901 16/11 07:15, clique em (15,489)).
    let by_guard = |p: (i32, i32)| (p.0 as f64 - LOR_GUARD.0).abs().max((p.1 as f64 - LOR_GUARD.1).abs()) <= 2.5;
    let near = position(hwnd).is_some_and(|p| cheb(p, LOR_VAULT_TILE) <= 3 && by_guard(p));
    if !near {
        walk_near(hwnd, t, LOR_VAULT_TILE, status)?;
    }
    // O clique no Guardian que não abre o baú leva o char para o lado dele, em (145,109), a 3 tiles
    // do tile do baú (AMUMU 16/11 06:45). De lá o 2º clique abre (visto no x9901): tolerância 3.
    let tol = 3;
    let _ = near;
    for attempt in 0..2 {
        npc_still(hwnd)?;
        let at = match confirm_destination(hwnd, Dest::Lorencia, Some((LOR_VAULT_TILE,tol))) {
            Ok(p) => p,
            // Depois de um clique que não abriu, o char anda até o NPC e sai da conferência do tile:
            // o que o dono precisa saber é que o baú não abriu, não o erro de posição.
            Err(e) if attempt > 0 => return Err(format!("{BAU_DICA}; {e}")),
            Err(e) => return Err(e),
        };
        if cheb(at, LOR_VAULT_TILE) > 2 && !by_guard(at) {
            return Err(format!("baú: clique deslocou o char para {at:?}; parei sem voltar e repetir a rota; {BAU_DICA}"));
        }
        if storage_open(hwnd) {
            return Ok(());
        }
        let npc = npc_px(at, LOR_GUARD);
        status(format!("baú: char em {at:?}, clicando no Safety Guardian em {npc:?} (tentativa {})", attempt + 1));
        input::run(hwnd, &[Step::Click { x: npc.0, y: npc.1 }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 6000 {
            sleep(400);
            if storage_open(hwnd) {
                sleep(500);
                return Ok(());
            }
        }
        save_error_shot(hwnd, &format!("baú de Lorencia não abriu na tentativa {}", attempt + 1));
    }
    Err(format!("o baú de Lorencia não abriu (2 cliques): {BAU_DICA}"))
}

/// Com o baú aberto: volta para a aba 1 (seta esquerda até parar; título em inglês confere).
fn storage_first_tab(hwnd: isize) -> Result<Bits, String> {
    let title = || storage_title(hwnd).ok_or_else(|| "não li o título do baú".to_string());
    let mut prev = title()?;
    let mut misses = 0;
    for _ in 0..30 {
        if zen_core::vault::read_tab(&prev) == Some(1) {
            return Ok(prev);
        }
        input::run(hwnd, &[Step::Click { x: STORAGE_PREV.0, y: STORAGE_PREV.1 }])?;
        sleep(600 + 300 * misses);
        let t = title()?;
        if zen_core::vault::same_title(&t, &prev) {
            misses += 1;
            if misses >= 3 {
                break;
            }
            continue;
        }
        misses = 0;
        prev = t;
    }
    match zen_core::vault::read_tab(&prev) {
        Some(1) => Ok(prev),
        None if misses >= 3 => Ok(prev), // título em outro idioma: 3 cliques sem mudar = aba 1
        other => Err(format!("não consegui voltar para a aba 1 do baú (título: {other:?})")),
    }
}


/// Saque conferido: aba perde `amount`, inventário ganha `amount` - taxa.
fn withdraw(hwnd: isize, amount: u64, stored: u64, inv: u64) -> Result<u64, String> {
    use zen_core::vault::digit_keys;
    input::run(hwnd, &[Step::Click { x: STORAGE_WITHDRAW.0, y: STORAGE_WITHDRAW.1 }])?;
    sleep(1000);
    let mut steps = vec![Step::Click { x: DEPOSIT_INPUT.0, y: DEPOSIT_INPUT.1 }, Step::WaitMs(200)];
    for _ in 0..12 {
        steps.push(Step::Key(VK_BACK));
    }
    steps.extend(digit_keys(amount).into_iter().map(Step::Key));
    steps.push(Step::WaitMs(300));
    if let Err(e) = input::run(hwnd, &steps) {
        let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
        return Err(format!("falhou ao digitar: {e}"));
    }
    input::run(hwnd, &[Step::Click { x: DEPOSIT_OK.0, y: DEPOSIT_OK.1 }])?;
    sleep(1500);
    let s = read_zen_stable(hwnd, STORAGE_ZEN);
    let z = read_zen_stable(hwnd, INV_ZEN);
    let want_inv = (inv + amount).saturating_sub(WITHDRAW_FEE);
    match (s, z) {
        // Taxa real observada em 15/11 no RAG9LEL: ~6.200 (não 200) com a aba debitada exata.
        // A aba é a prova do saque; o char pode receber até WITHDRAW_SLACK a menos.
        (Some(s), Some(z)) if s + amount == stored && z <= inv + amount && z + WITHDRAW_SLACK >= want_inv => {
            let fee = (inv + amount).saturating_sub(z);
            if fee > 2 * WITHDRAW_FEE { tracing::warn!(taxa = fee, "saque: taxa maior que a esperada, aceita"); }
            Ok(z)
        }
        (s, z) => {
            let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
            Err(format!("saque de {} NÃO conferiu: aba {} -> {s:?}, char {} -> {z:?}", zen_core::fmt::int_br(amount), zen_core::fmt::int_br(stored), zen_core::fmt::int_br(inv)))
        }
    }
}

/// Abaixo disto a aba 1 é completada de volta para `TAB1_RESERVE` (regra do dono, 09/11 14:58:
/// a aba 1 é o dinheiro dos moves; margem de 600M antes de repor).
pub const TAB1_LOW: u64 = 400_000_000;

/// Depósito conferido na aba aberta (mesma caixa do depósito do Kanturu).
fn deposit_here(hwnd: isize, amount: u64, stored: u64, inv: u64) -> Result<u64, String> {
    use zen_core::vault::{deposit_ok, digit_keys};
    input::run(hwnd, &[Step::Click { x: STORAGE_DEPOSIT.0, y: STORAGE_DEPOSIT.1 }])?;
    sleep(1000);
    let mut steps = vec![Step::Click { x: DEPOSIT_INPUT.0, y: DEPOSIT_INPUT.1 }, Step::WaitMs(200)];
    for _ in 0..12 {
        steps.push(Step::Key(VK_BACK));
    }
    steps.extend(digit_keys(amount).into_iter().map(Step::Key));
    steps.push(Step::WaitMs(300));
    if let Err(e) = input::run(hwnd, &steps) {
        let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
        return Err(format!("falhou ao digitar: {e}"));
    }
    input::run(hwnd, &[Step::Click { x: DEPOSIT_OK.0, y: DEPOSIT_OK.1 }])?;
    sleep(1500);
    match (read_zen_stable(hwnd, STORAGE_ZEN), read_zen_stable(hwnd, INV_ZEN)) {
        (Some(s), Some(z)) if deposit_ok(stored, s, inv, z, amount) => Ok(z),
        (s, z) => {
            let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
            Err(format!("depósito de {} NÃO conferiu: aba {} -> {s:?}, char {} -> {z:?}", zen_core::fmt::int_br(amount), zen_core::fmt::int_br(stored), zen_core::fmt::int_br(inv)))
        }
    }
}

/// Saca do baú de Lorencia até o inventário ter `target` (até 2B exatos), SÓ da aba 1 (dono,
/// 16/11 08:13: o baú 1 é o repositório único da Fortuna em todas as contas). `tab1_first` ou
/// `emergency`: pode usar a aba 1 inteira; sem eles (converter o baú inteiro) preserva 1B nela.
/// `check_reserve` não repõe mais nada (antes buscava nas outras abas). Devolve o Zen final.
pub fn lor_withdraw(hwnd: isize, t: &Terrain, target: u64, emergency: bool, check_reserve: bool, tab1_first: bool, status: &dyn Fn(String)) -> Result<u64, String> {
    use zen_core::vault::ZEN_CAP;
    // O inventário carrega até 2B exatos (chars parados em 2.000.000.000 confirmam): com 2B o
    // Lugard converte duas seguidas. Correção do dono em 09/11 14:57 (antes parava em 1,999B).
    let target = target.min(ZEN_CAP);
    open_lor_vault(hwnd, t, status)?;
    let fmt = zen_core::fmt::int_br;
    let r = (|| -> Result<u64, String> {
        let mut z = read_zen_stable(hwnd, INV_ZEN).ok_or("não li o Zen do inventário")?;
        if z >= target && !check_reserve {
            return Ok(z);
        }
        // Dono, 16/11 08:13: o baú 1 é o repositório ÚNICO da Fortuna em todas as contas. Não
        // procura Zen nas outras abas nem repõe a aba 1 a partir delas.
        storage_first_tab(hwnd)?;
        let s1 = read_zen_stable(hwnd, STORAGE_ZEN).ok_or("não li o Zen da aba 1")?;
        let _ = check_reserve;
        if z < target {
            let need = (target - z + WITHDRAW_FEE).min(ZEN_CAP - z);
            let take = if emergency {
                // Zen do move: usa a aba 1 toda se preciso.
                need.min(s1)
            } else if tab1_first {
                // Fortuna: empresta da aba 1 só se ela continuar acima de 100M.
                let t = fortune_borrow(need, s1, TAB1_REFILL_TRIGGER);
                if t == 0 {
                    status(format!("baú: aba 1 com {} (gatilho {}): sem saque para Fortuna; o Zen do char vai repor a aba 1", fmt(s1), fmt(TAB1_REFILL_TRIGGER)));
                }
                t
            } else {
                // "Converter o baú inteiro": preserva 1B na aba 1.
                need.min(s1.saturating_sub(TAB1_RESERVE))
            };
            if take > 10 * WITHDRAW_FEE {
                status(format!("baú: sacando {} da aba 1", fmt(take)));
                z = withdraw(hwnd, take, s1, z)?;
            }
            if z + WITHDRAW_SLACK < target {
                status(format!("baú: a aba 1 não tem o suficiente (char com {}); não procuro nas outras abas", fmt(z)));
            }
        }
        Ok(z)
    })();
    if let Err(e) = &r {
        save_error_shot(hwnd, e);
    }
    close_storage(hwnd);
    r
}

/// Converte no Lugard enquanto houver >= 1B no inventário. Confere cada conversão pelo Zen.
fn convert_all(hwnd: isize, t: &Terrain, status: &dyn Fn(String)) -> Result<u32, String> {
    ensure_action_map(hwnd, Dest::Lorencia)?;
    let mut z = inv_zen(hwnd)?;
    if z < FORTUNE_ZEN {
        return Ok(0);
    }
    status("Lugard: andando".into());
    walk_near(hwnd, t, LUGARD_TILE, status)?;
    let (mut n, mut fails) = (0u32, 0u32);
    while z >= FORTUNE_ZEN {
        close_inventory(hwnd);
        npc_still(hwnd)?;
        let at = confirm_destination(hwnd, Dest::Lorencia, Some((LUGARD_TILE,2)))?;
        if cheb(at, LUGARD_TILE) > 2 {
            return Err(format!("Lugard: char deslocado para {at:?}; parei sem repetir a aproximação"));
        }
        let p = npc_px(at, LUGARD);
        status(format!("Lugard: char em {at:?}, clicando em {p:?}"));
        input::run(hwnd, &[Step::Click { x: p.0, y: p.1 }])?;
        sleep(2500);
        let z2 = inv_zen(hwnd)?;
        if z2 < z && z - z2 >= FORTUNE_ZEN - 1_000 {
            n += 1;
            fails = 0;
            status(format!("Lugard: {n} Fortuna(s); Zen {} -> {}", zen_core::fmt::int_br(z), zen_core::fmt::int_br(z2)));
            z = z2;
        } else {
            fails += 1;
            save_error_shot(hwnd, &format!("Lugard não converteu (tentativa {fails})"));
            if fails >= 2 {
                return Err(format!("o Lugard não converteu em 2 tentativas (Zen {})", zen_core::fmt::int_br(z2)));
            }
            // Não anda de volta ao tile depois de um clique falho.
        }
    }
    Ok(n)
}

/// J -> Fortuna do Imperador -> Deposit All. Fecha o Item Bank no fim.
fn bank_deposit_all(hwnd: isize) -> Result<(), String> {
    if !bank_open(hwnd) {
        input::run(hwnd, &[Step::Key(VK_J)])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 3000 && !bank_open(hwnd) {
            sleep(250);
        }
    }
    trace(hwnd, "5-j-aberto");
    let Some(row) = bank_row(hwnd) else {
        save_error_shot(hwnd, "Item Bank (J) não abriu");
        return Err("o Item Bank (J) não abriu ou não achei a linha da Fortuna".into());
    };
    // Clica NO TEXTO da linha achada (medido: texto em (486,343), clique bom em (507,355)).
    // O menu demora mais em PC lento: só clica no "Deposit All" quando ele aparece na tela.
    let mut menu = None;
    for _ in 0..2 {
        input::run(hwnd, &[Step::Click { x: row.0 + 21, y: row.1 + 12 }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 4000 {
            sleep(250);
            menu = bank_menu(hwnd);
            if menu.is_some() {
                break;
            }
        }
        if menu.is_some() {
            break;
        }
    }
    let Some(dep) = menu else {
        save_error_shot(hwnd, "menu da Fortuna no J não abriu");
        return Err("o menu da Fortuna no Item Bank (J) não abriu".into());
    };
    sleep(300);
    trace(hwnd, "6-j-menu");
    // Texto "Deposit All" em (485,393) -> clique bom em (515,398); "Cancel" 37 px abaixo.
    input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 5 }])?;
    sleep(2000);
    trace(hwnd, "7-depositou");
    input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 42 }])?;
    sleep(700);
    // Fecha o J (tecla J de novo) para não poluir a tela.
    for _ in 0..2 {
        if !bank_open(hwnd) {
            break;
        }
        input::run(hwnd, &[Step::Key(VK_J)])?;
        sleep(700);
    }
    Ok(())
}

/// Fecha o Gremory Case (X; se não fechar, K de novo).
fn close_gremory(hwnd: isize) {
    for k in 0..2 {
        if !gremory_open(hwnd) {
            return;
        }
        let _ = if k == 0 { input::run(hwnd, &[Step::Click { x: GREMORY_X.0, y: GREMORY_X.1 }]) } else { input::run(hwnd, &[Step::Key(VK_K)]) };
        sleep(700);
    }
}

/// Quantos dos 64 espaços do inventário estão ocupados (inventário aberto). Espaço vazio é
/// a grade vermelho-escura; ocupado tem >40% de pixels fora dela (medido em 09/11: vazio ~0%,
/// itens 49-98%).
fn inventory_filled(hwnd: isize) -> Option<u32> {
    let img = capture::capture(hwnd, Some([747, 347, 260, 260]))?;
    let w = img.w as usize;
    let mut n = 0;
    for r in 0..8 {
        for c in 0..8 {
            let (x0, y0) = ((c as f64 * 32.1).round() as usize, (r as f64 * 32.1).round() as usize);
            let (mut other, mut total) = (0u32, 0u32);
            for y in y0 + 4..y0 + 28 {
                for x in x0 + 4..x0 + 28 {
                    let i = (y * w + x) * 4;
                    let Some(p) = img.bgra.get(i..i + 4) else { continue };
                    total += 1;
                    if p[1] > 60 || p[0] > 60 || p[2] < 40 {
                        other += 1;
                    }
                }
            }
            if total > 0 && other * 100 / total > 40 {
                n += 1;
            }
        }
    }
    Some(n)
}

/// Esvazia o Gremory Case (só as linhas "Fortuna do Imperador") para o inventário e guarda
/// tudo no Item Bank (J). Devolve quantas foram usadas.
fn gremory_to_bank(hwnd: isize, status: &dyn Fn(String)) -> Result<u32, String> {
    close_inventory(hwnd);
    if !gremory_open(hwnd) {
        input::run(hwnd, &[Step::Key(VK_K)])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 3000 && !gremory_open(hwnd) {
            sleep(250);
        }
    }
    if !gremory_open(hwnd) {
        save_error_shot(hwnd, "Gremory Case (K) não abriu");
        return Err("o Gremory Case (K) não abriu".into());
    }
    trace(hwnd, "1-k-aberto");
    let (mut used, mut pending) = (0u32, 0u32);
    // O K abre junto com o inventário: conta os espaços ocupados antes, para conferir no fim
    // que as Fortunas foram mesmo para o J (dono, 09/11 14:59).
    let filled0 = if inventory_open(hwnd) { inventory_filled(hwnd) } else { None };
    let r = (|| -> Result<(), String> {
        let mut retried = false;
        for _ in 0..70 {
            let Some(i) = (0..9).find(|&i| gremory_row_is_fortune(hwnd, i)) else { break };
            input::run(hwnd, &[Step::Click { x: 300, y: gremory_row_y(i) - 3 }])?;
            sleep(900);
            if !use_dialog_is_fortune(hwnd) {
                // A caixa às vezes demora (x9901 em 11/11 04:27): espera mais um pouco e, se
                // ainda não for a Fortuna, cancela e tenta a linha de novo UMA vez.
                sleep(800);
            }
            if !use_dialog_is_fortune(hwnd) {
                save_error_shot(hwnd, "caixa Use It sem Fortuna");
                let _ = input::run(hwnd, &[Step::Click { x: USE_CANCEL.0, y: USE_CANCEL.1 }]);
                if !retried {
                    retried = true;
                    status("K: a caixa do Use It não mostrou a Fortuna; tentando de novo".into());
                    sleep(800);
                    continue;
                }
                return Err("a caixa do Use It não mostrou Fortuna do Imperador (2 tentativas): parei".into());
            }
            if used == 0 {
                trace(hwnd, "2-caixa-use-it");
            }
            input::run(hwnd, &[Step::Click { x: USE_IT.0, y: USE_IT.1 }])?;
            sleep(1000);
            used += 1;
            pending += 1;
            status(format!("K: {used} Fortuna(s) para o inventário"));
            if pending >= 20 {
                // K fechado antes do J (dono, 09/11 16:01: tela limpa), depois abre o K de novo.
                close_gremory(hwnd);
                bank_deposit_all(hwnd)?;
                pending = 0;
                input::run(hwnd, &[Step::Key(VK_K)])?;
                let t0 = std::time::Instant::now();
                while t0.elapsed().as_millis() < 3000 && !gremory_open(hwnd) {
                    sleep(250);
                }
            }
        }
        // Sempre (mesmo sem Use It nesta viagem): Fortunas que ficaram no inventário de uma
        // viagem anterior também vão para o J.
        {
            trace(hwnd, "3-k-esvaziado");
            close_gremory(hwnd);
            trace(hwnd, "4-k-fechado");
            bank_deposit_all(hwnd)?;
            // Conferência: o inventário tem de voltar ao que era antes do Use It.
            if let Some(n0) = filled0 {
                for attempt in 0..2 {
                    if !inventory_open(hwnd) {
                        input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
                        sleep(900);
                    }
                    trace(hwnd, "8-inventario-conferencia");
                    let n1 = if inventory_open(hwnd) { inventory_filled(hwnd) } else { None };
                    match n1 {
                        Some(n1) if n1 > n0 => {
                            if attempt == 1 {
                                // Só conta espaços, não reconhece o item: itens de evento/drop que
                                // entraram na viagem davam falso alarme (x9901, 15/11 19:25, 5 pilhas
                                // de abóbora). Fortuna que sobrar vai para o J na próxima viagem
                                // (ver o comentário acima), então não aborta: avisa no log + print.
                                save_error_shot(hwnd, "inventário com espaços a mais depois do Deposit All");
                                tracing::warn!(extras = n1 - n0, "Fortuna: inventário com espaços a mais depois do 2º Deposit All; seguindo (pode ser item de evento)");
                                status(format!("J: {} espaço(s) a mais no inventário depois do 2º Deposit All; seguindo (print salvo)", n1 - n0));
                                break;
                            }
                            status(format!("J: {} espaço(s) a mais no inventário; Deposit All de novo", n1 - n0));
                            bank_deposit_all(hwnd)?;
                        }
                        Some(_) => {
                            status("J: conferido, inventário como antes".into());
                            break;
                        }
                        None => break,
                    }
                }
            }
        }
        Ok(())
    })();
    close_gremory(hwnd);
    for _ in 0..2 {
        if bank_open(hwnd) {
            let _ = input::run(hwnd, &[Step::Key(VK_J)]);
            sleep(700);
        }
    }
    close_inventory(hwnd);
    r.map(|_| used)
}

pub struct FortuneReport {
    pub converted: u32,
    pub to_bank: u32,
    pub zen_before: u64,
    pub zen_after: u64,
}

/// Viagem completa até ficar pronto para voltar (não volta ao spot: isso é `recover`).
/// `all` = converte tudo o que houver no baú (menos a reserva de 1B da aba 1).
pub fn fortune(hwnd: isize, lor: &Terrain, all: bool, status: &dyn Fn(String)) -> Result<FortuneReport, String> {
    let logged_status = |message: String| {
        tracing::info!(hwnd, estado = %message, "Fortuna: etapa");
        status(message);
    };
    let status: &dyn Fn(String) = &logged_status;

    let _session = input::Session::begin(hwnd)?;
    // O M funciona com o helper LIGADO (dono, 11/11 08:46): desligar antes deixava o char parado
    // no spot sem helper quando o teleporte falhava (foco perdido) e ele morria. Agora o helper
    // só é desligado já em Lorencia (zona segura).
    if !in_lorencia(hwnd) {
        status("Fortuna: M -> Lorencia (helper continua ligado)".into());
        teleport(hwnd, Dest::Lorencia)?;
    }
    ensure_action_map(hwnd, Dest::Lorencia)?;
    let zen_before = inv_zen(hwnd)?;
    let mut rep = FortuneReport { converted: 0, to_bank: 0, zen_before, zen_after: zen_before };
    let res = (|| -> Result<(), String> {
        rep.converted += convert_all(hwnd, lor, status)?;
        loop {
            // Normal: completa 1B + 20M (converte mais uma e volta com 20M). Tudo: enche até 2B.
            let target = if all { zen_core::vault::ZEN_CAP } else { FORTUNE_ZEN + KEEP_ZEN };
            let z = lor_withdraw(hwnd, lor, target, false, !all, !all, status)?;
            if z < FORTUNE_ZEN {
                break;
            }
            let n = convert_all(hwnd, lor, status)?;
            rep.converted += n;
            if !all || n == 0 {
                break;
            }
        }
        Ok(())
    })();
    // Antes de sair de Lorencia (dono, 09/11 16:16: o CSRF voltou ao spot com 1,98B porque a
    // conversão falhou e ele desistiu): sobrou >= 1B no inventário -> Lugard de novo.
    let mut res = res;
    for round in 0..2 {
        if res.is_err() { break; } // Não reinicia a rota após falha de aproximação/interação.
        match inv_zen(hwnd) {
            Ok(z) if z >= FORTUNE_ZEN => {
                status(format!("ainda há {} no inventário: Lugard de novo ({})", zen_core::fmt::int_br(z), round + 1));
                match convert_all(hwnd, lor, status) {
                    Ok(n) => rep.converted += n,
                    Err(e) => res = Err(e),
                }
            }
            _ => break,
        }
    }
    // O K é esvaziado mesmo se a conversão parou no meio (prazo e limite de 50).
    let bank = gremory_to_bank(hwnd, status);
    if let Ok(n) = &bank {
        rep.to_bank = *n;
    }
    // Mesmo em falha, esvazia K/J acima; não volta a procurar NPCs depois de desistir.
    res?;
    bank?;
    let mut money = ensure_move_money(hwnd, lor, all, status);
    // Nunca volta ao spot carregando Zen do baú: o que passar de 20M (+ folga) volta para o baú.
    if let Ok(z) = money {
        // Repõe a aba 1 com o que sobrar acima da margem de 50M (regra do dono, 16/11 08:39).
        if z > SAFETY_ZEN + EXCESS_SLACK {
            status(format!("sobrou {} no inventário: guardando na aba 1 o que passa de 50M", zen_core::fmt::int_br(z)));
            money = lor_deposit_excess(hwnd, lor, z - SAFETY_ZEN, status);
        }
    }
    rep.zen_after = money?;
    Ok(rep)
}

/// Folga antes de devolver Zen ao baú (o char volta com 20M; abaixo disto não vale a viagem).
const EXCESS_SLACK: u64 = 30_000_000;

/// Deposita `amount` no baú de Lorencia: primeiro repõe a aba 1 até 1B, depois na primeira aba
/// (2..15) onde cabe; se não couber inteiro numa só, divide. Devolve o Zen final do inventário.
fn lor_deposit_excess(hwnd: isize, t: &Terrain, amount: u64, status: &dyn Fn(String)) -> Result<u64, String> {
    use zen_core::vault::ZEN_CAP;
    open_lor_vault(hwnd, t, status)?;
    let fmt = zen_core::fmt::int_br;
    let r = (|| -> Result<u64, String> {
        let mut z = read_zen_stable(hwnd, INV_ZEN).ok_or("não li o Zen do inventário")?;
        let mut left = amount.min(z.saturating_sub(MOVE_ZEN));
        // Só a aba 1 (dono, 16/11 08:13), até o limite do baú.
        storage_first_tab(hwnd)?;
        let s = read_zen_stable(hwnd, STORAGE_ZEN).ok_or("não li o Zen da aba 1")?;
        let put = left.min(ZEN_CAP.saturating_sub(s));
        if put > 10 * WITHDRAW_FEE {
            status(format!("baú: devolvendo {} na aba 1", fmt(put)));
            z = deposit_here(hwnd, put, s, z)?;
            left -= put;
        }
        if left > 10 * WITHDRAW_FEE {
            return Err(format!("baú cheio: não consegui devolver {}", fmt(left)));
        }
        Ok(z)
    })();
    if let Err(e) = &r {
        save_error_shot(hwnd, e);
    }
    close_storage(hwnd);
    r
}

/// Garante o Zen do move (20M). Se faltar, busca no baú (pode usar a reserva da aba 1).
/// `check_reserve`: abre o baú mesmo com Zen suficiente, para conferir/repor a aba 1.
pub fn ensure_move_money(hwnd: isize, lor: &Terrain, check_reserve: bool, status: &dyn Fn(String)) -> Result<u64, String> {
    let z = inv_zen(hwnd)?;
    // Volta com 30M (dono, 10/11 00:50): 20M do move + Zen para o helper ligar no spot.
    if z >= KEEP_ZEN - 1_000 && !check_reserve {
        return Ok(z);
    }
    if z < KEEP_ZEN - 1_000 {
        status(format!("só {} no char: buscando o Zen do move no baú", zen_core::fmt::int_br(z)));
    }
    let z = lor_withdraw(hwnd, lor, KEEP_ZEN, true, check_reserve, true, status)?;
    if z < MOVE_ZEN {
        return Err(format!("o baú não tem Zen para o move (char ficou com {})", zen_core::fmt::int_br(z)));
    }
    Ok(z)
}

// ---------------------------------------------------------------- Custom Pick Window (Y), 16/11
// A tecla Y abre a "PICK WINDOW" (servidor: "To activate the new pick menu, press Y"). Visto no
// x9901 em 16/11: janela à esquerda (x 85..385, y 140..620), 6 botões visíveis (Jewel of Soul,
// Bless, Chaos, Creation, Life, Zen) e barra de rolagem que só anda com a roda do mouse. Ao entrar
// em zona segura o pick desliga ("If move to SafeZone it will be necessary to reactivate!").
// - O botão NÃO muda de aparência ligado/desligado (prints idênticos) nem escreve no chat: o bot
//   só clica uma vez por volta ao spot depois de ter saído do mapa do spot (cidade/teleporte),
//   quando o pick com certeza está desligado.
// - ESC NÃO serve para fechar: abre o "Especial Menu" e fecha o Helper (16/11 12:15). Fecha com Y.

/// Texto "If move to SafeZone it will / be necessary to reactivate!" (150x42 em (160,208)).
const PICK_OPEN: &[u8; 150 * 42] = include_bytes!("pick_open.gray");
/// Texto "Zen" do botão (66x16; no print em (200,544), centro do botão em (233,551)).
const PICK_ZEN: &[u8; 66 * 16] = include_bytes!("pick_zen.gray");
/// A janela NÃO fica parada: com o inventário (V) aberto ela vai para o meio da tela (texto em
/// (404,189) em vez de (160,208); x9901/ReDMooN 16/11 16:59-17:51, 6 falhas). Procura o texto
/// numa faixa larga e calcula a lista e a roda a partir dele.
const PICK_SEARCH_AREA: [i32; 4] = [100, 180, 600, 72];
/// Lista de botões relativa ao texto: (-10, +52), 170x335.
const PICK_LIST_FROM_TEXT: (i32, i32, i32, i32) = (-10, 52, 170, 335);
/// Roda do mouse relativa ao texto: meio da lista (fora da janela a roda mexe o zoom da câmera).
const PICK_WHEEL_FROM_TEXT: (i32, i32) = (73, 212);
const VK_Y: u16 = 0x59;

/// Melhor posição do modelo cinza dentro de `img` com erro médio <= `max_err` (0..255).
fn gray_match(img: &capture::Image, template: &[u8], w: usize, h: usize, max_err: u32) -> Option<(i32, i32)> {
    let (iw, ih) = (img.w as usize, img.h as usize);
    if iw < w || ih < h { return None; }
    let gray: Vec<u8> = img.bgra.chunks_exact(4)
        .map(|p| ((p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000) as u8).collect();
    let limit = max_err * (w * h) as u32;
    let mut best: Option<(u32, i32, i32)> = None;
    for y in 0..=ih - h {
        'x: for x in 0..=iw - w {
            let mut err = 0u32;
            for ty in 0..h {
                let row = (y + ty) * iw + x;
                for tx in 0..w {
                    err += (gray[row + tx] as u32).abs_diff(template[ty * w + tx] as u32);
                }
                if err > limit { continue 'x; }
            }
            if best.map_or(true, |(e, _, _)| err < e) { best = Some((err, x as i32, y as i32)); }
        }
    }
    best.map(|(_, x, y)| (x, y))
}
/// Origem do texto da Pick Window (coordenadas da janela do jogo), a partir do recorte de
/// `PICK_SEARCH_AREA`.
fn pick_text_in(img: &capture::Image) -> Option<(i32, i32)> {
    let (x, y) = gray_match(img, PICK_OPEN, 150, 42, 8)?;
    Some((PICK_SEARCH_AREA[0] + x, PICK_SEARCH_AREA[1] + y))
}
fn pick_list_area(text: (i32, i32)) -> [i32; 4] {
    let (dx, dy, w, h) = PICK_LIST_FROM_TEXT;
    [text.0 + dx, text.1 + dy, w, h]
}
/// Centro do botão Zen em coordenadas da janela, a partir do recorte da lista em `area`.
fn pick_zen_in(img: &capture::Image, area: [i32; 4]) -> Option<(i32, i32)> {
    let (x, y) = gray_match(img, PICK_ZEN, 66, 16, 6)?;
    Some((area[0] + x + 33, area[1] + y + 7))
}
/// Onde está a Pick Window (origem do texto), se aberta.
pub fn pick_window_at(hwnd: isize) -> Option<(i32, i32)> {
    pick_text_in(&capture::capture(hwnd, Some(PICK_SEARCH_AREA))?)
}
pub fn pick_window_open(hwnd: isize) -> bool { pick_window_at(hwnd).is_some() }
fn pick_zen_button(hwnd: isize, text: (i32, i32)) -> Option<(i32, i32)> {
    let area = pick_list_area(text);
    pick_zen_in(&capture::capture(hwnd, Some(area))?, area)
}
fn wait_pick(hwnd: isize, open: bool, ms: u128) -> Option<Option<(i32, i32)>> {
    let t0 = std::time::Instant::now();
    loop {
        let at = pick_window_at(hwnd);
        if at.is_some() == open { return Some(at); }
        if t0.elapsed().as_millis() >= ms { return None; }
        sleep(250);
    }
}
/// Fecha a Pick Window com Y (nunca ESC). Confere na tela.
fn close_pick(hwnd: isize) -> bool {
    for _ in 0..2 {
        if !pick_window_open(hwnd) { return true; }
        let _ = input::run(hwnd, &[Step::Key(VK_Y)]);
        if wait_pick(hwnd, false, 2000).is_some() { return true; }
    }
    !pick_window_open(hwnd)
}

/// Liga a coleta de Zen na Pick Window. Só com o char no spot, fora da zona segura e farmando.
/// Abre com Y (2 tentativas, 5 s entre elas), acha o botão "Zen" (rola a lista com a roda se
/// precisar), clica UMA vez, fecha com Y e confere que a tela ficou limpa e o helper ligado.
pub fn enable_zen_pick(hwnd: isize, world: u8, status: &dyn Fn(String)) -> Result<String, String> {
    let t = world_terrain_of(hwnd, world)?;
    let p = position(hwnd).ok_or("Pick: não li a posição")?;
    if t.is_safezone(p.0, p.1) {
        return Err(format!("Pick: char em zona segura em {p:?}; não ativo (o jogo desligaria)"));
    }
    let _session = input::Session::begin(hwnd)?;
    let mut opened = pick_window_at(hwnd);
    for attempt in 0..2 {
        if opened.is_some() { break; }
        if attempt > 0 {
            status("Pick: a janela não abriu; nova tentativa em 5 s".into());
            sleep(5000);
            opened = pick_window_at(hwnd);
            if opened.is_some() { break; }
        }
        status("Pick: Y para abrir a Pick Window".into());
        input::run(hwnd, &[Step::Key(VK_Y)])?;
        opened = wait_pick(hwnd, true, 3000).flatten();
    }
    let Some(text) = opened else {
        save_error_shot(hwnd, "Pick Window não abriu");
        return Err("a Pick Window (Y) não abriu em 2 tentativas".into());
    };
    sleep(400);
    // Relê a posição depois de assentar (a janela pode ter mudado ao abrir).
    let text = pick_window_at(hwnd).unwrap_or(text);
    status(format!("Pick: janela aberta (texto em {text:?})"));
    let wheel = (text.0 + PICK_WHEEL_FROM_TEXT.0, text.1 + PICK_WHEEL_FROM_TEXT.1);
    let mut btn = pick_zen_button(hwnd, text);
    let mut scrolls = 0;
    while btn.is_none() && scrolls < 8 {
        status(format!("Pick: botão Zen fora da lista visível; rolando ({})", scrolls + 1));
        input::run(hwnd, &[Step::Wheel { x: wheel.0, y: wheel.1, delta: -120 }])?;
        sleep(500);
        scrolls += 1;
        btn = pick_zen_button(hwnd, text);
    }
    let Some((bx, by)) = btn else {
        save_error_shot(hwnd, "Pick: botão Zen não achado");
        close_pick(hwnd);
        return Err(format!("Pick: botão Zen não achado ({scrolls} rolagens)"));
    };
    status(format!("Pick: clicando em Zen ({bx}, {by})"));
    input::run(hwnd, &[Step::Click { x: bx, y: by }])?;
    sleep(800);
    if !close_pick(hwnd) {
        save_error_shot(hwnd, "Pick Window não fechou");
        return Err("Pick: Zen clicado, mas a janela não fechou com Y".into());
    }
    // A janela não pode ter mexido no helper (ESC fechava). Confere e religa se preciso.
    if helper_state(hwnd) == Some(false) {
        set_helper(hwnd, true)?;
    }
    Ok(format!("Pick Zen ativado em {p:?}{}", if scrolls > 0 { format!(" após {scrolls} rolagem(ns)") } else { String::new() }))
}

/// Saiu do mapa do spot (cidade, teleporte, morte): o pick do Zen desligou e precisa de Y de novo.
/// Só conta leitura completa (nome do mapa E posição): leitura falhada não é "saiu".
pub fn pick_left_spot_map(map: Option<&str>, pos: Option<(i32, i32)>, spot_map: &str) -> bool {
    matches!((map, pos), (Some(m), Some(_)) if m != spot_map)
}
/// Hora de ativar: precisa (saiu do mapa), farmando no spot há `delay_s` e fora da espera.
pub fn pick_due(needed: bool, farm_since: Option<u64>, helper_on: Option<bool>, now: u64, retry_at: u64, delay_s: u64) -> bool {
    needed && helper_on == Some(true) && now >= retry_at && farm_since.is_some_and(|t| now.saturating_sub(t) >= delay_s)
}

#[cfg(test)]
mod pick_tests {
    use super::*;
    fn img(bytes: &[u8]) -> capture::Image {
        let mut d = png::Decoder::new(bytes);
        d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut r = d.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        let ch = match info.color_type { png::ColorType::Rgba => 4, png::ColorType::Rgb => 3, _ => panic!() };
        let bgra = buf[..info.buffer_size()].chunks_exact(ch).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
        capture::Image { w: info.width, h: info.height, bgra }
    }
    #[test]
    fn acha_a_janela_parada_ou_deslocada_pelo_inventario() {
        assert_eq!(pick_text_in(&img(include_bytes!("../../tests/fixtures/pick/busca_aberta.png"))), Some((160, 208)));
        assert_eq!(pick_text_in(&img(include_bytes!("../../tests/fixtures/pick/busca_deslocada.png"))), Some((404, 189)));
        assert_eq!(pick_text_in(&img(include_bytes!("../../tests/fixtures/pick/busca_fechada.png"))), None);
    }
    #[test]
    fn acha_zen_na_lista_deslocada_e_depois_de_rolar() {
        let a = pick_list_area((160, 208));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../tests/fixtures/pick/lista_aberta.png")), a), Some((233, 551)));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../tests/fixtures/pick/lista_rolada.png")), a), Some((233, 445)));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../tests/fixtures/pick/lista_fechada.png")), a), None);
        let d = pick_list_area((404, 189));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../tests/fixtures/pick/lista_deslocada.png")), d), Some((478, 531)));
    }
    #[test]
    fn ida_a_cidade_e_volta_reativam_uma_vez() {
        // No spot (Kanturu): não marca. Leitura falhada: não marca. Lorencia: marca.
        assert!(!pick_left_spot_map(Some("Kanturu"), Some((160, 197)), "Kanturu"));
        assert!(!pick_left_spot_map(None, Some((160, 197)), "Kanturu"));
        assert!(!pick_left_spot_map(Some("Lorencia"), None, "Kanturu"));
        assert!(pick_left_spot_map(Some("Lorencia"), Some((146, 112)), "Kanturu"));
        // De volta: só depois de 30 s farmando, helper ligado e fora da espera de nova tentativa.
        assert!(!pick_due(true, Some(1000), Some(true), 1010, 0, 30));
        assert!(pick_due(true, Some(1000), Some(true), 1030, 0, 30));
        assert!(!pick_due(true, Some(1000), Some(false), 1030, 0, 30));
        assert!(!pick_due(true, None, Some(true), 1030, 0, 30));
        assert!(!pick_due(true, Some(1000), Some(true), 1030, 1100, 30));
        // Já ativado nesta volta: não clica de novo (o botão alterna e não mostra o estado).
        assert!(!pick_due(false, Some(1000), Some(true), 5000, 0, 30));
    }
}

// ---------------------------------------------------------------- Elpis: Gemstone -> Jewel of Harmony (16/11)
// MaMaKi (PC1) na Refinery Tower, ao lado da Elpis. Visto no jogo em 16/11 20:20:
// - Clique na Elpis -> diálogo (About refinery / Jewel of Harmony / Refine Gemstone / Close).
// - "Refine Gemstone" -> painel Refine + inventário. Botão direito numa Gemstone AVULSA a manda
//   para a caixa ("Refine Gemstone" amarelo; taxa mostrada 15%). Refine -> "OK" -> chat diz
//   "Refine has failed." quando falha. Depois de um refino o painel fica vazio: precisa fechar
//   (X do inventário) e abrir de novo pela Elpis.
// - Separar 1 da pilha: R (repair) com SÓ o inventário aberto e clique na pilha; o repair fica
//   ativo e cada clique separa mais uma. Com o painel do Refine aberto o R não vale e o clique
//   PEGA a pilha inteira (16/11 20:23): por isso só separa com o painel fechado.
// - O bot não anda: o dono leva o char até a Elpis.
const ELPIS_DLG: &[u8; 90 * 16] = include_bytes!("elpis_dlg.gray");
const ELPIS_DLG_AT: (i32, i32) = (468, 465);
const ELPIS_READY: &[u8; 86 * 14] = include_bytes!("elpis_ready.gray");
const ELPIS_READY_AT: (i32, i32) = (138, 103);
const ELPIS_BTN: &[u8; 54 * 44] = include_bytes!("elpis_btn.gray");
const ELPIS_BTN_AT: (i32, i32) = (238, 638);
const ELPIS_CONFIRM: &[u8; 50 * 21] = include_bytes!("elpis_confirm.gray");
const ELPIS_CONFIRM_AT: (i32, i32) = (396, 330);
#[cfg(test)]
const ELPIS_FAILED: &[u8; 78 * 13] = include_bytes!("elpis_failed.gray");
/// X do meio da tela do Refine: aparece com o painel ativo ou vazio (depois de um refino).
const ELPIS_PANEL_X: &[u8; 40 * 38] = include_bytes!("elpis_panel_x.gray");
const ELPIS_PANEL_X_AT: (i32, i32) = (458, 656);
const GEM_SINGLE: &[u8; 32 * 32] = include_bytes!("gem_single.gray");
/// Caixa do Refine vazia (8x4 casas de 32 px a partir de (140,203)).
const ELPIS_BOX_EMPTY: &[u8; 256 * 128] = include_bytes!("elpis_box_empty.gray");
const ELPIS_BOX: [i32; 4] = [140, 203, 256, 128];
const ELPIS_REFINE_OPTION: (i32, i32) = (513, 472);
const ELPIS_DIALOG_CLOSE: (i32, i32) = (513, 542);
const ELPIS_REFINE: (i32, i32) = (265, 659);
const ELPIS_OK: (i32, i32) = (421, 340);
const INV_CLOSE: (i32, i32) = (782, 675);
const INV_GRID: [i32; 4] = [748, 347, 260, 260];
const VK_R: u16 = 0x52;

fn fixed_at(hwnd: isize, t: &[u8], w: usize, h: usize, at: (i32, i32)) -> bool {
    let m = 4;
    capture::capture(hwnd, Some([at.0 - m, at.1 - m, w as i32 + 2 * m, h as i32 + 2 * m]))
        .is_some_and(|img| gray_match(&img, t, w, h, 8).is_some())
}
/// Casamento só pelo desenho do texto/ícone: os pixels claros do modelo (>= 120) têm de bater
/// (erro médio <= 30) e quase nenhum pixel escuro do modelo (< 50) pode estar claro (> 110).
/// Serve para janelas semitransparentes do jogo, em que o cenário atrás muda o fundo.
#[allow(dead_code)]
fn mask_match(img: &capture::Image, t: &[u8], w: usize, h: usize) -> bool {
    let (iw, ih) = (img.w as usize, img.h as usize);
    if iw < w || ih < h { return false; }
    let lit = t.iter().filter(|&&v| v >= 120).count();
    let dark = t.iter().filter(|&&v| v < 50).count();
    if lit < 40 || dark < 40 { return false; }
    let gray: Vec<u8> = img.bgra.chunks_exact(4)
        .map(|p| ((p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000) as u8).collect();
    for y in 0..=ih - h {
        for x in 0..=iw - w {
            let (mut err, mut bright) = (0u32, 0usize);
            for ty in 0..h {
                for tx in 0..w {
                    let (g, v) = (gray[(y + ty) * iw + x + tx], t[ty * w + tx]);
                    if v >= 120 { err += (g as u32).abs_diff(v as u32); }
                    else if v < 50 && g > 110 { bright += 1; }
                }
            }
            if err <= 30 * lit as u32 && bright * 10 <= dark { return true; }
        }
    }
    false
}
fn wait_fixed(hwnd: isize, t: &[u8], w: usize, h: usize, at: (i32, i32), ms: u128) -> bool {
    let t0 = std::time::Instant::now();
    loop {
        if fixed_at(hwnd, t, w, h, at) { return true; }
        if t0.elapsed().as_millis() >= ms { return false; }
        sleep(200);
    }
}
/// Na Refinery Tower a câmera não centraliza o char como nos outros mapas: medido em 16/11 com a
/// Elpis em (78,176) e o char em (75,176) (corpo em ~(535,420), o clique abriu) e em (81,176)
/// (corpo em ~(220,250)): char em ~(377,340) e o modelo dos outros mapas encolhido a 0,82.
fn elpis_click_px(at: (i32, i32), npc: (i32, i32)) -> (i32, i32) {
    let (x, y) = npc_px(at, (npc.0 as f64, npc.1 as f64));
    (377 + ((x - CENTER.0) as f64 * 0.82).round() as i32, 340 + ((y - CENTER.1) as f64 * 0.82).round() as i32)
}
fn elpis_dialog(hwnd: isize) -> bool { fixed_at(hwnd, ELPIS_DLG, 90, 16, ELPIS_DLG_AT) }
/// Casas ocupadas da caixa do Refine (recorte de `ELPIS_BOX`): centro na janela.
fn box_items_in(img: &capture::Image) -> Vec<(i32, i32)> {
    let w = img.w as usize;
    let mut out = Vec::new();
    for r in 0..4usize {
        for c in 0..8usize {
            let (mut e, mut n) = (0u32, 0u32);
            for y in r * 32 + 4..r * 32 + 28 {
                for x in c * 32 + 4..c * 32 + 28 {
                    let i = (y * w + x) * 4;
                    let Some(p) = img.bgra.get(i..i + 3) else { continue };
                    let g = (p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000;
                    e += g.abs_diff(ELPIS_BOX_EMPTY[y * 256 + x] as u32);
                    n += 1;
                }
            }
            if n > 0 && e / n > 12 {
                out.push((ELPIS_BOX[0] + c as i32 * 32 + 16, ELPIS_BOX[1] + r as i32 * 32 + 16));
            }
        }
    }
    out
}
fn box_items(hwnd: isize) -> Option<Vec<(i32, i32)>> { capture::capture(hwnd, Some(ELPIS_BOX)).map(|img| box_items_in(&img)) }

/// Gemstones no inventário (recorte de `INV_GRID`): (centro na janela, avulsa?). Pilha e avulsa
/// têm o mesmo desenho embaixo; a pilha tem o número em cima.
fn gems_in(img: &capture::Image) -> Vec<((i32, i32), bool)> {
    let w = img.w as usize;
    let g = |x: usize, y: usize| -> u32 {
        let i = (y * w + x) * 4;
        img.bgra.get(i..i + 3).map_or(0, |p| (p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000)
    };
    let diff = |cx: usize, cy: usize, ys: std::ops::Range<usize>, xs: std::ops::Range<usize>| -> u32 {
        let (mut e, mut n) = (0u32, 0u32);
        for y in ys.clone() { for x in xs.clone() {
            e += g(cx + x, cy + y).abs_diff(GEM_SINGLE[y * 32 + x] as u32);
            n += 1;
        }}
        e / n.max(1)
    };
    let mut out = Vec::new();
    for r in 0..8 {
        for c in 0..8 {
            let (cx, cy) = ((c as f64 * 32.1).round() as usize, (r as f64 * 32.1).round() as usize);
            if cx + 32 > w || cy + 32 > img.h as usize { continue; }
            if diff(cx, cy, 13..30, 3..29) > 8 { continue; }
            let single = diff(cx, cy, 1..12, 8..30) <= 10;
            out.push(((INV_GRID[0] + cx as i32 + 16, INV_GRID[1] + cy as i32 + 16), single));
        }
    }
    out
}
fn gems(hwnd: isize) -> Vec<((i32, i32), bool)> {
    capture::capture(hwnd, Some(INV_GRID)).map(|img| gems_in(&img)).unwrap_or_default()
}

/// Fecha diálogo da Elpis / painel do Refine / inventário, conferindo na tela.
fn elpis_close_all(hwnd: isize) -> Result<(), String> {
    for _ in 0..3 {
        if elpis_dialog(hwnd) {
            input::run(hwnd, &[Step::Click { x: ELPIS_DIALOG_CLOSE.0, y: ELPIS_DIALOG_CLOSE.1 }])?;
            sleep(700);
            continue;
        }
        if inventory_open(hwnd) || fixed_at(hwnd, ELPIS_PANEL_X, 40, 38, ELPIS_PANEL_X_AT) {
            input::run(hwnd, &[Step::Click { x: INV_CLOSE.0, y: INV_CLOSE.1 }])?;
            sleep(800);
            continue;
        }
        return Ok(());
    }
    Err("Elpis: não consegui fechar as janelas".into())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ElpisReport { pub refined: u32, pub failed: u32, pub ok: u32, pub gems_left: u32 }

/// Refina até `max` Gemstones na Elpis (uma por vez). Para no primeiro passo que não confere na
/// tela, sem cliques às cegas. As Harmonies ficam no inventário.
pub fn elpis_refine(hwnd: isize, char_name: &str, max: u32, status: &dyn Fn(String)) -> Result<ElpisReport, String> {
    let _session = input::Session::begin(hwnd)?;
    let mut rep = ElpisReport::default();
    let mut lift_ok: Option<i32> = None;
    while rep.refined < max {
        // O painel do Refine cobre as coordenadas do canto: fecha antes de ler a posição.
        elpis_close_all(hwnd)?;
        let pos = position(hwnd).ok_or("Elpis: não li a posição")?;
        let elpis = super::objects::nearby_hwnd(hwnd, char_name, pos)
            .and_then(|v| v.into_iter().find(|o| o.kind == zen_core::objects::Kind::Npc && o.name.eq_ignore_ascii_case("Elpis")))
            .map(|o| (o.x, o.y))
            .ok_or("Elpis não encontrada na memória perto do char (leve o char até ela)")?;
        if cheb(pos, elpis) > 5 {
            return Err(format!("char em {pos:?}, longe da Elpis em {elpis:?} (leve o char até ela)"));
        }
        // 1) Inventário só: separa Gemstones se não houver avulsa.
        if !inventory_open(hwnd) {
            input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
            sleep(900);
        }
        if !inventory_open(hwnd) { return Err("Elpis: o inventário não abriu".into()); }
        let mut list = gems(hwnd);
        if !list.iter().any(|g| g.1) {
            let Some(stack) = list.iter().find(|g| !g.1).map(|g| g.0) else {
                rep.gems_left = 0;
                break;
            };
            let before = list.len();
            let free = 64usize.saturating_sub(inventory_filled(hwnd).unwrap_or(64) as usize);
            let n = (max - rep.refined).min(free.saturating_sub(2) as u32).min(30);
            if n == 0 { return Err("Elpis: inventário sem espaço para separar Gemstones".into()); }
            status(format!("Elpis: separando {n} Gemstone(s) (R + clique na pilha)"));
            input::run(hwnd, &[Step::Key(VK_R)])?;
            sleep(400);
            for k in 0..n {
                input::run(hwnd, &[Step::Click { x: stack.0, y: stack.1 }])?;
                sleep(if k == 0 { 700 } else { 350 });
                if k == 0 {
                    let now = gems(hwnd);
                    if !now.iter().any(|g| g.0 == stack) {
                        // A pilha foi para o cursor (repair não ativou): devolve ao mesmo lugar.
                        input::run(hwnd, &[Step::Click { x: stack.0, y: stack.1 }])?;
                        sleep(700);
                        save_error_shot(hwnd, "Elpis: pilha pegou em vez de separar");
                        return Err("Elpis: o R não ativou o repair; a pilha foi devolvida ao lugar".into());
                    }
                    if now.len() <= before {
                        save_error_shot(hwnd, "Elpis: não separou");
                        return Err("Elpis: o clique na pilha não separou nenhuma Gemstone".into());
                    }
                }
            }
            list = gems(hwnd);
        }
        let Some(single) = list.iter().find(|g| g.1).map(|g| g.0) else {
            return Err("Elpis: não achei Gemstone avulsa depois de separar".into());
        };
        rep.gems_left = list.len() as u32;
        // 2) Abre a Elpis (clique no corpo; tenta 3 alturas e lembra a que funcionou). Se o
        // char andar até ela, o diálogo abre depois (21:30: abriu com 3 s): espera parar e
        // relê antes de clicar de novo, nunca clica com o diálogo já aberto.
        let mut opened = false;
        let mut at = pos;
        let mut moves = 0;
        let mut panel = false;
        for lift in lift_ok.into_iter().chain([0, 25, -25]) {
            if fixed_at(hwnd, ELPIS_BTN, 54, 44, ELPIS_BTN_AT) { opened = true; panel = true; break; }
            if elpis_dialog(hwnd) { opened = true; break; }
            let (x, y) = elpis_click_px(at, elpis);
            input::run(hwnd, &[Step::Click { x, y: y - lift }])?;
            if wait_fixed(hwnd, ELPIS_DLG, 90, 16, ELPIS_DLG_AT, 3000) { opened = true; lift_ok = Some(lift); break; }
            let now = npc_still(hwnd).unwrap_or(at);
            if wait_fixed(hwnd, ELPIS_DLG, 90, 16, ELPIS_DLG_AT, 1500) { opened = true; lift_ok = Some(lift); break; }
            if now != at {
                moves += 1;
                if moves > 2 || cheb(now, elpis) > 5 {
                    save_error_shot(hwnd, "Elpis: clique moveu o char");
                    return Err(format!("Elpis: o char andou para {now:?} sem abrir o diálogo; parei"));
                }
                at = now;
            }
        }
        if !opened {
            save_error_shot(hwnd, "Elpis: diálogo não abriu");
            return Err("Elpis: o diálogo não abriu (3 cliques)".into());
        }
        if !panel { input::run(hwnd, &[Step::Click { x: ELPIS_REFINE_OPTION.0, y: ELPIS_REFINE_OPTION.1 }])?; }
        if !wait_fixed(hwnd, ELPIS_BTN, 54, 44, ELPIS_BTN_AT, 3000) {
            save_error_shot(hwnd, "Elpis: painel do Refine não abriu");
            return Err("Elpis: o painel do Refine não abriu".into());
        }
        // 3) Botão direito na avulsa -> caixa; confere "Refine Gemstone" amarelo.
        input::run(hwnd, &[Step::RightClick { x: single.0, y: single.1 }])?;
        if !wait_fixed(hwnd, ELPIS_READY, 86, 14, ELPIS_READY_AT, 2500) {
            save_error_shot(hwnd, "Elpis: Gemstone não entrou na caixa");
            return Err("Elpis: a Gemstone não entrou na caixa do Refine".into());
        }
        // 4) Refine -> OK.
        input::run(hwnd, &[Step::Click { x: ELPIS_REFINE.0, y: ELPIS_REFINE.1 }])?;
        if !wait_fixed(hwnd, ELPIS_CONFIRM, 50, 21, ELPIS_CONFIRM_AT, 2500) {
            save_error_shot(hwnd, "Elpis: sem confirmação");
            return Err("Elpis: a confirmação (OK) não apareceu".into());
        }
        input::run(hwnd, &[Step::Click { x: ELPIS_OK.0, y: ELPIS_OK.1 }])?;
        // O painel perde o botão quando o refino termina.
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 3000 && fixed_at(hwnd, ELPIS_BTN, 54, 44, ELPIS_BTN_AT) { sleep(150); }
        sleep(500);
        rep.refined += 1;
        // Deu certo: a Harmony fica na caixa (dono, 16/11 20:38). Botão direito tira para o
        // inventário; confere que a caixa esvaziou antes de seguir.
        // Quem decide é a caixa: vazia = falhou (a linha "Refine has failed." do chat sobe quando
        // chega outra mensagem e não serve de prova; 16/11 21:18).
        let left = box_items(hwnd).unwrap_or_default();
        if left.is_empty() {
            rep.failed += 1;
        } else {
            rep.ok += 1;
            if rep.ok <= 2 { save_error_shot(hwnd, "Elpis: Harmony na caixa"); }
            for at in &left {
                input::run(hwnd, &[Step::RightClick { x: at.0, y: at.1 }])?;
                sleep(600);
            }
            let still = box_items(hwnd).unwrap_or_default();
            if !still.is_empty() {
                save_error_shot(hwnd, "Elpis: Harmony não saiu da caixa");
                return Err(format!("Elpis: {} item(ns) ficaram na caixa do Refine (tire à mão); parei para não perder a Harmony", still.len()));
            }
            status(format!("Elpis: Harmony! ({} até agora) tirada da caixa para o inventário", rep.ok));
        }
        rep.gems_left = rep.gems_left.saturating_sub(1);
        status(format!("Elpis: {} refino(s), {} Harmony(s), {} falha(s)", rep.refined, rep.ok, rep.failed));
    }
    elpis_close_all(hwnd)?;
    Ok(rep)
}

#[cfg(test)]
mod elpis_tests {
    use super::*;
    fn img(bytes: &[u8]) -> capture::Image {
        let mut d = png::Decoder::new(bytes);
        d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut r = d.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        let ch = match info.color_type { png::ColorType::Rgba => 4, png::ColorType::Rgb => 3, _ => panic!() };
        let bgra = buf[..info.buffer_size()].chunks_exact(ch).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
        capture::Image { w: info.width, h: info.height, bgra }
    }
    #[test]
    fn acha_pilhas_e_avulsas() {
        let a = gems_in(&img(include_bytes!("../../tests/fixtures/elpis/inventario.png")));
        assert_eq!(a, vec![((764, 363), false), ((796, 363), false), ((828, 363), true)]);
        let b = gems_in(&img(include_bytes!("../../tests/fixtures/elpis/inventario2.png")));
        assert_eq!(b, vec![((764, 363), false), ((796, 363), false), ((860, 363), true)]);
    }
    #[test]
    fn clique_na_elpis_bate_com_o_medido_no_jogo() {
        let a = elpis_click_px((75, 176), (78, 176));
        assert!((a.0 - 535).abs() <= 15 && (a.1 - 425).abs() <= 20, "{a:?}");
        let b = elpis_click_px((81, 176), (78, 176));
        assert!((b.0 - 220).abs() <= 15 && (b.1 - 250).abs() <= 20, "{b:?}");
    }
    #[test]
    fn acha_item_na_caixa_do_refine() {
        assert_eq!(box_items_in(&img(include_bytes!("../../tests/fixtures/elpis/caixa_com.png"))), vec![(156, 219)]);
        assert!(box_items_in(&img(include_bytes!("../../tests/fixtures/elpis/caixa_vazia.png"))).is_empty());
    }
    #[test]
    fn reconhece_telas_da_elpis() {
        let m = |b: &[u8], t: &[u8], w, h| gray_match(&img(b), t, w, h, 8).is_some();
        assert!(m(include_bytes!("../../tests/fixtures/elpis/dlg.png"), ELPIS_DLG, 90, 16));
        // Diálogo semitransparente com cenário claro atrás (21:50): só o desenho do texto bate.
        assert!(mask_match(&img(include_bytes!("../../tests/fixtures/elpis/dlg_cenario.png")), ELPIS_DLG, 90, 16));
        assert!(!mask_match(&img(include_bytes!("../../tests/fixtures/elpis/dlg_sem.png")), ELPIS_DLG, 90, 16));
        assert!(m(include_bytes!("../../tests/fixtures/elpis/ready.png"), ELPIS_READY, 86, 14));
        assert!(!m(include_bytes!("../../tests/fixtures/elpis/notready.png"), ELPIS_READY, 86, 14));
        assert!(m(include_bytes!("../../tests/fixtures/elpis/failed.png"), ELPIS_FAILED, 78, 13));
        assert!(!m(include_bytes!("../../tests/fixtures/elpis/notfailed.png"), ELPIS_FAILED, 78, 13));
    }
}

// ---------------------------------------------------------------- Dark Raven (Trainer de Lorencia)
// Conferido no jogo em 11/11 com o x9901: o NPC "Trainer" (classe 226) está em (122,110) na
// lista de personagens da memória (igual ao guia oficial). Clicando nele abre a janela
// "Trainer" com "Restore life (durability)" e "Resurrect spirit"; o primeiro abre "Select the
// pet to recover life" com os botões "Dark horse" e "Dark Raven". A vida do Raven é a barra do
// topo da tela (zen_core::petbar), lida antes e depois para confirmar.

/// Tile de onde se fala com o Trainer (o x9901 abriu a janela daqui).
pub const TRAINER_STAND: (i32, i32) = (122, 108);
/// Tile do Trainer pelo guia e pela memória; a memória manda quando der para ler.
const TRAINER_TILE: (i32, i32) = (122, 110);
const TRAINER_RESTORE_BTN: (i32, i32) = (513, 280);
const TRAINER_RAVEN_BTN: (i32, i32) = (513, 328);
const TRAINER_CLOSE_BTN: (i32, i32) = (513, 421);
/// Textos da janela (branco, limiar 150), com a origem no print de 1030x797.
const TRAINER_TITLE: ((i32, i32), &str) = ((495, 140), TITLE_TXT);
const TRAINER_RESTORE: ((i32, i32), &str) = ((464, 276), RESTORE_TXT);
const TRAINER_SELECT: ((i32, i32), &str) = ((450, 191), SELECT_TXT);
const TITLE_TXT: &str = "\
.........................................
.######............##....................
...##....................................
...##...##.#..###..##.####...###..##.#...
...##...####....##.##.##.##.##.##.####...
...##...##....####.##.##.##.#####.##.....
...##...##...##.##.##.##.##.##....##.....
...##...##....####.##.##.##..####.##.....
.........................................";
const RESTORE_TXT: &str = "\
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
const SELECT_TXT: &str = "\
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
fn trainer_text(hwnd: isize, (origin, txt): ((i32, i32), &str)) -> bool {
    let t = Bits::from_text(txt);
    let s = 6i32;
    let Some(img) = capture::capture(hwnd, Some([origin.0 - s, origin.1 - s, t.w as i32 + 2 * s, t.h as i32 + 2 * s])) else { return false };
    trainer_pattern(&img, &t)
}
fn trainer_pattern(img: &capture::Image, t: &Bits) -> bool {
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
fn confirm_raven_read(mut read: impl FnMut() -> Option<u8>, mut pause: impl FnMut()) -> Option<u8> {
    for attempt in 0..3 {
        if let Some(hp) = read() { return Some(hp); }
        if attempt < 2 { pause(); }
    }
    None
}
pub fn raven_confirmed(hwnd: isize) -> Option<u8> { confirm_raven_read(|| raven_now(hwnd), || sleep(150)) }
fn raven_request_allowed(manual: bool, hp: u8) -> bool { manual || hp <= 50 }

pub fn raven_now(hwnd: isize) -> Option<u8> {
    let [x, y, w, h] = zen_core::petbar::RAVEN_SCAN;
    let img = capture::capture(hwnd, Some([x, y, w, h]))?;
    zen_core::petbar::raven_in_scan(&img.bgra, img.w as usize, img.h as usize)
}

fn wait_for(hwnd: isize, what: ((i32, i32), &str), ms: u64) -> bool {
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
const TRAINER_CLICK_RANGE: i32 = 3;
fn calibrated_trainer_route(name:&str)->zen_core::routes::RouteDefinition {
    use zen_core::{routes::{RouteDefinition,RouteTarget,NpcTarget},spots::Spot};
    let interaction=Spot{world:LORENCIA_WORLD,x:122,y:109};
    RouteDefinition{id:"builtin-trainer".into(),revision:1,name:format!("Raven: aproximação calibrada de {name}"),world:LORENCIA_WORLD,
        waypoints:vec![interaction],target:RouteTarget::Npc{npc:NpcTarget{class:226,name:"Trainer".into(),
            location:Spot{world:LORENCIA_WORLD,x:122,y:110},interaction,radius:1}},bindings:vec![],return_route:None}
}

/// Garante que o char está em Lorencia e perto do Trainer, lendo a posição de verdade. Perto
/// (até 2 tiles) = não anda. Longe = caminhada completa com desvio. Se a rota falhar (tile de
/// spawn que não deixa traçar rota, p.ex. 121,108), dá um passo para um tile livre vizinho e
/// aceita qualquer posição a até 3 tiles do Trainer.
fn approach_trainer(hwnd: isize, lor: &Terrain, status: &dyn Fn(String)) -> Result<(i32, i32), String> {
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
fn trainer_from_memory(hwnd: isize, pos: (i32, i32)) -> Result<(i32, i32), String> {
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
    if crate::navigation::enabled() {
        let name=CHAR_NAMES.lock().ok().and_then(|m|m.as_ref().and_then(|m|m.get(&hwnd).cloned()))
            .ok_or("Raven: personagem não identificado para navegação experimental")?;
        let route=calibrated_trainer_route(&name);
        let context=wait_route_map(hwnd,&name,LORENCIA_WORLD,status)?;
        // Internal path of the explicitly requested Raven action: it does not
        // install a route or accept an arbitrary command from the network.
        crate::navigation::scoped(context,"",||->Result<(),String>{
            ensure_action_map(hwnd,Dest::Lorencia)?;
            status("Raven: aproximação calibrada com desvio e interação confirmada".into());
            super::navigation::walk_points(hwnd,lor,&route,status)?;
            let zen_core::routes::RouteTarget::Npc{npc}=&route.target else {unreachable!()};
            super::navigation::interact(hwnd,&name,npc,lor,&route,status)
        })?;
        open=true;
    }
    for (k, lift) in [40, 20, 60].into_iter().enumerate() {
        if open {break;}
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
        let mut target = zen_core::routes::NpcTarget {
            class: 226, name: "Trainer".into(),
            location: zen_core::spots::Spot {world:LORENCIA_WORLD,x:TRAINER_TILE.0,y:TRAINER_TILE.1},
            interaction: zen_core::spots::Spot {world:LORENCIA_WORLD,x:at.0,y:at.1}, radius:3,
        };
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
        target.location.x=second.0;
        target.location.y=second.1;
        tracing::warn!(hwnd,posicao=?at,"Raven: reabrindo uma vez o Trainer após menu vazio, sem nova caminhada");
        status("Raven: Trainer abriu sem botões; reabrindo uma única vez".into());
        route_npc_click(hwnd,second_at,&target,40)?;
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

#[cfg(test)]
mod tab1_tests {
    use super::*;
    #[test]
    fn aba1_empresta_tudo_ou_nada_acima_do_gatilho() {
        let f = TAB1_REFILL_TRIGGER;
        assert_eq!(fortune_borrow(40_000_000, 731_000_000, f), 40_000_000);
        assert_eq!(fortune_borrow(40_000_000, 140_000_001, f), 40_000_000);
        assert_eq!(fortune_borrow(40_000_000, 139_000_000, f), 0); // ficaria abaixo de 100M
        assert_eq!(fortune_borrow(40_000_000, 100_000_000, f), 0); // no gatilho: não empresta
        assert_eq!(fortune_borrow(40_000_000, 0, f), 0);
    }
    #[test]
    fn ciclo_no_gatilho_repoe_a_aba1_e_guarda_50m() {
        // Char com 1,99B, aba 1 com 100M: 1 Fortuna do próprio Zen, nada emprestado, 940M para
        // a aba 1 (fica 1,04B) e 50M no char.
        let (z, tab1) = (1_990_000_000u64, 100_000_000u64);
        assert_eq!(fortune_borrow(FORTUNE_ZEN + KEEP_ZEN - (z - FORTUNE_ZEN) + WITHDRAW_FEE, tab1, TAB1_REFILL_TRIGGER), 0);
        let left = z - FORTUNE_ZEN;
        assert!(left > SAFETY_ZEN + EXCESS_SLACK);
        assert_eq!(tab1 + (left - SAFETY_ZEN), 1_040_000_000);
    }
}

#[cfg(test)]
mod npc_window_tests {
    use super::*;
    fn fixture(bytes: &[u8]) -> capture::Image {
        let mut decoder=png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::EXPAND|png::Transformations::STRIP_16);
        let mut reader=decoder.read_info().unwrap();
        let mut bytes=vec![0;reader.output_buffer_size()];
        let info=reader.next_frame(&mut bytes).unwrap();
        let channels=match info.color_type {png::ColorType::Rgba=>4,png::ColorType::Rgb=>3,_=>panic!("PNG de teste inesperado")};
        let bgra=bytes[..info.buffer_size()].chunks_exact(channels).flat_map(|p|[p[2],p[1],p[0],255]).collect();
        capture::Image{w:info.width,h:info.height,bgra}
    }
    #[test]
    fn trainer_reconhece_menus_reais_e_rejeita_area_sem_texto() {
        let ready=fixture(include_bytes!("../../tests/fixtures/trainer_menu/ready.png"));
        let yousee=fixture(include_bytes!("../../tests/fixtures/trainer_menu/yousee-ready.png"));
        let matches=|image:&capture::Image,(origin,txt):((i32,i32),&str)|{
            let template=Bits::from_text(txt);
            let crop=image.crop([origin.0-6,origin.1-6,template.w as i32+12,template.h as i32+12]).unwrap();
            trainer_pattern(&crop,&template)
        };
        assert!(matches(&ready,TRAINER_TITLE));
        assert!(matches(&ready,TRAINER_RESTORE));
        assert!(matches(&yousee,TRAINER_TITLE));
        assert!(matches(&yousee,TRAINER_RESTORE));
        let empty=capture::Image{w:116,h:23,bgra:vec![0;116*23*4]};
        assert!(!trainer_pattern(&empty,&Bits::from_text(RESTORE_TXT)));
    }
    #[test]
    fn trainer_calibrado_alcanca_interacao_por_dentro_da_cerca() {
        let terrain=Terrain::from_client_file(include_bytes!("../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        let route=calibrated_trainer_route("YouSee");
        route.validate().unwrap();
        for(x,y) in [(116,122),(117,129),(119,113),(122,107),(118,137)] {
            let origin=zen_core::spots::Spot{world:1,x,y};
            let plan=zen_core::navigation::plan(&terrain,origin,&route).unwrap();
            let arrival=*plan.segments.last().unwrap().last().unwrap();
            assert!(route.arrived(0,arrival));
            assert!(arrival.near(zen_core::spots::Spot{world:1,x:122,y:110},1));
            let path:Vec<_>=plan.segments[0].iter().map(|p|(p.x,p.y)).collect();
            for at in path.iter().copied().take(path.len().saturating_sub(1)) {
                if !route.arrived(0,zen_core::spots::Spot{world:1,x:at.0,y:at.1}) {
                    assert!(zen_core::navigation::next_step(&terrain,at,&path).is_some(),"cache não deve perder o próximo passo em {at:?}");
                }
            }
        }
    }
    #[test]
    fn csrf_reconhece_zyro_lateral_sem_confundir_com_inventario_do_x9901() {
        let zyro=fixture(include_bytes!("../../tests/fixtures/npc_side/zyro.png"));
        let chaos=fixture(include_bytes!("../../tests/fixtures/npc_side/chaos_inventory.png"));
        assert!(side_npc_caption(&zyro.crop([750,35,240,25]).unwrap()));
        assert!(!side_npc_caption(&chaos.crop([750,35,240,25]).unwrap()));
        let (x,y)=gray_template_at(&zyro.crop([735,640,120,80]).unwrap(),SIDE_CLOSE,40,40).unwrap();
        assert!((735+x+20-782).abs()<=2 && (640+y+20-676).abs()<=2);
        // Mesmo botão X existe no inventário: o título precisa fazer parte da decisão.
        assert!(gray_template_at(&chaos.crop([735,640,120,80]).unwrap(),SIDE_CLOSE,40,40).is_some());
        assert!(gray_template_at(&chaos.crop([205,41,134,26]).unwrap(),CHAOS_TITLE,124,16).is_some());
        assert!(gray_template_at(&zyro.crop([205,41,134,26]).unwrap(),CHAOS_TITLE,124,16).is_none());
    }
    #[test]
    fn limite_de_janelas_interrompe_antes_de_enviar_input() {
        let mut count=2;
        let result=recover_route_window(0,Dest::Lorencia,&mut count,&|_|panic!("não deve agir após limite"));
        assert!(result.unwrap_err().contains("limite"));
        assert_eq!(count,2);
    }
}

#[cfg(test)]
mod spot_frame_tests {
    use super::*;

    #[test]
    fn quadros_reais_preservam_mapa_e_coordenada_sem_nova_captura() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../zen-core/tests/fixtures/coords");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            let stem = p.file_stem().unwrap().to_str().unwrap();
            let mut xy = stem.split('_');
            let x: i32 = xy.next().unwrap().parse().unwrap();
            let y: i32 = xy.next().unwrap().parse().unwrap();
            let bits = Bits::from_text(&std::fs::read_to_string(&p).unwrap());
            let bgra = bits.px.iter().flat_map(|on| if *on { [255,255,255,255] } else { [0,0,0,255] }).collect();
            let frame = capture::Image { w: bits.w as u32, h: bits.h as u32, bgra };
            assert_eq!(decode_spot_frame(Some(frame)), (Some("Kanturu"), Some((x,y))), "{stem}");
            count += 1;
        }
        assert!(count >= 40);
        assert_eq!(decode_spot_frame(None), (None, None));
    }
}

#[cfg(test)]
mod npc_approach_tests {
    use super::*;
    #[test]
    fn redmoon_desvia_do_bloqueio_130_130_com_rota_validada_e_limite() {
        let t=Terrain::from_client_file(include_bytes!("../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        let at=(130,130);
        let path=npc_route(&t,at,LUGARD_TILE).unwrap();
        let blocked=t.waypoints(at,&path,3)[0];
        let mut detours=NpcDetours::default();
        let first=detours.next(&t,at,blocked,LUGARD_TILE).unwrap();
        assert_ne!(first,blocked);
        assert!(t.line_clear(at,first));
        assert!(t.path(at,first).unwrap().len()<=3);
        assert!(npc_route(&t,first,LUGARD_TILE).is_some());
        assert!(!npc_route(&t,first,LUGARD_TILE).unwrap().contains(&at));
        let second=detours.next(&t,at,blocked,LUGARD_TILE).unwrap();
        assert_ne!(first,second);
        assert!(detours.next(&t,at,blocked,LUGARD_TILE).unwrap_err().contains("limite"));
    }
    #[test]
    fn desvio_nao_inventa_saida_sem_piso_ou_destino_valido() {
        let t=Terrain::from_client_file(include_bytes!("../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        assert!(NpcDetours::default().next(&t,(-1,-1),(0,0),LUGARD_TILE).is_err());
        assert!(NpcDetours::default().next(&t,(130,130),(132,130),(-1,-1)).is_err());
    }
    #[test]
    fn raven_manual_aceita_vida_alta_e_leitura_transitoria_e_confirmada() {
        for hp in [0,25,50,51,100] { assert!(raven_request_allowed(true,hp)); }
        assert!(!raven_request_allowed(false,100));
        assert!(raven_request_allowed(false,25));
        let mut samples = [None,None,Some(80)].into_iter();
        let mut pauses=0;
        assert_eq!(confirm_raven_read(|| samples.next().flatten(), || pauses+=1),Some(80));
        assert_eq!(pauses,2);
        let mut reads=0;
        assert_eq!(confirm_raven_read(|| {reads+=1;None}, || {}),None);
        assert_eq!(reads,3);
    }
    #[test]
    fn trainer_rota_completa_do_bau_e_saida_nomove() {
        let t=Terrain::from_client_file(include_bytes!("../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        for start in [LOR_VAULT_TILE,(121,108)] {
            let route=walking_route(&t,start,TRAINER_STAND,WalkArrival::Trainer).unwrap();
            assert!(route.iter().all(|p|t.walkable(p.0,p.1)));
            assert_eq!(route.last(),Some(&TRAINER_STAND));
        }
        assert!(final_walk_arrived((121,108),TRAINER_STAND,false,0,WalkArrival::Trainer));
        assert!(!final_walk_arrived((119,113),TRAINER_STAND,false,30,WalkArrival::Trainer));
    }

    #[test]
    fn yousee_chega_ao_trainer_mesmo_no_tile_nomove_registrado_no_log() {
        let raw = include_bytes!("../../../zen-core/tests/fixtures/terrain/EncTerrain1.att");
        let t = Terrain::from_client_file(raw).unwrap();
        let at = (121, 108);
        assert!(!t.walkable(at.0, at.1));
        assert!(t.walkable(TRAINER_STAND.0, TRAINER_STAND.1));
        assert!(t.path(at, TRAINER_STAND).is_none());
        let progress = NpcProgress { target: TRAINER_STAND, at, previous: None, stalled: 0 };
        assert!(progress.arrived());
        assert!(destination_matches((Some("Lorencia"), Some(at)), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        assert!(!destination_matches((Some("Dungeon"), Some(at)), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        assert!(!destination_matches((Some("Lorencia"), None), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        assert!(!destination_matches((Some("Lorencia"), Some((120,108))), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        let exit = npc_route(&t, at, TRAINER_STAND).unwrap();
        assert_eq!(exit, vec![TRAINER_STAND]);
    }

    fn progress(at: (i32, i32)) -> NpcProgress {
        NpcProgress { target: (149,114), at, previous: None, stalled: 0 }
    }

    #[test]
    fn redmoon_sai_do_spawn_bloqueado_sem_abrir_caminho_em_paredes() {
        let raw = include_bytes!("../../../zen-core/tests/fixtures/terrain/EncTerrain1.att");
        let t = Terrain::from_client_file(raw).unwrap();
        let from = (125,121);
        assert!(!t.walkable(from.0,from.1));
        assert!(t.path(from,LUGARD_TILE).is_none());
        let route = npc_route(&t,from,LUGARD_TILE).unwrap();
        assert_eq!(cheb(from,route[0]),1);
        assert_eq!((from.0-route[0].0).abs()+(from.1-route[0].1).abs(),1);
        assert!(route.iter().all(|p|t.walkable(p.0,p.1)));
        assert_eq!(route.last(),Some(&LUGARD_TILE));
        assert_eq!(t.waypoints(from,&route,3)[0],route[0]);
        assert!(npc_route(&t,(-1,121),LUGARD_TILE).is_none());
        assert!(npc_route(&t,(123,129),from).is_none());
    }

    #[test]
    fn segura_no_trajeto_e_solta_a_tres_tiles() {
        assert!(npc_hold_far((123,117), LOR_VAULT_TILE));
        assert!(npc_hold_far((142,112), LOR_VAULT_TILE));
        assert!(!npc_hold_far((143,112), LOR_VAULT_TILE));
        assert!(!npc_hold_far((146,112), LOR_VAULT_TILE));
        assert!(npc_hold_far((145,110), LUGARD_TILE));
        assert!(!npc_hold_far((146,111), LUGARD_TILE));
    }

    #[test]
    fn aceita_um_tile_sem_forcar_o_tile_exato() {
        assert!(progress((148,113)).arrived());
        let mut p = progress((146,112));
        p.advance((148,114)).unwrap();
        assert!(p.arrived());
    }

    #[test]
    fn interrompe_retorno_ao_tile_anterior() {
        let mut p = progress((145,112));
        p.advance((146,112)).unwrap();
        assert!(p.advance((145,112)).is_err());
    }

    #[test]
    fn interrompe_duas_tentativas_sem_movimento() {
        let mut p = progress((145,112));
        p.advance((145,112)).unwrap();
        assert!(p.advance((145,112)).is_err());
    }

    #[test]
    fn clique_curto_nao_e_ampliado_e_clique_longo_e_limitado() {
        let near = npc_step_px((146,112), (147,112));
        let distance = ((near.0-CENTER.0) as f64).hypot((near.1-CENTER.1) as f64);
        assert!(distance > 0.0 && distance < 90.0, "{distance}");
        let far = npc_step_px((100,100), (149,114));
        let distance = ((far.0-CENTER.0) as f64).hypot((far.1-CENTER.1) as f64);
        assert!(distance <= 181.0, "{distance}");
    }
}

#[cfg(test)]
mod destination_tests {
    use super::*;
    #[test]
    fn redmoon_chega_imediatamente_a_dois_tiles_com_mobs() {
        let target = (123, 233);
        assert!(final_walk_arrived((121,232), target, true, 0, WalkArrival::Spot));
        assert!(!final_walk_arrived((120,232), target, true, 0, WalkArrival::Spot));
        assert!(!final_walk_arrived((121,232), target, true, 30, WalkArrival::Exact));
    }
    #[test]
    fn sem_mobs_aceita_um_tile_so_apos_dez_segundos() {
        let target = (123,233);
        assert!(!final_walk_arrived((122,233), target, false, 9, WalkArrival::Spot));
        assert!(final_walk_arrived((122,233), target, false, 10, WalkArrival::Spot));
        assert!(!final_walk_arrived((121,233), target, false, 30, WalkArrival::Spot));
        assert!(final_walk_arrived(target, target, false, 0, WalkArrival::Spot));
        assert!(!final_walk_arrived((122,233), target, false, 30, WalkArrival::Exact));
    }
    #[test]
    fn margem_final_do_spot_aceita_dois_e_rejeita_tres_tiles() {
        let target = (123,233);
        assert!(destination_matches((Some("Kanturu"),Some((121,232))),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
        assert!(!destination_matches((Some("Lorencia"),Some(target)),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
        assert!(!destination_matches((Some("Kanturu"),None),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
        assert!(!destination_matches((Some("Kanturu"),Some((120,233))),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
    }
    #[test]
    fn mesma_coordenada_no_mapa_errado_nao_confirma() {
        assert!(!destination_matches((Some("Lorencia"), Some((91,66))), Dest::Kanturu, Some(((91,66),2))));
        assert!(!destination_matches((None, Some((91,66))), Dest::Kanturu, None));
    }
    #[test]
    fn nao_inventa_coordenada_ausente_e_rejeita_destino_distante() {
        assert!(!destination_matches((Some("Kanturu"), None), Dest::Kanturu, None));
        assert!(!destination_matches((Some("Kanturu"), Some((130,130))), Dest::Kanturu, Some(((91,66),2))));
        assert!(!destination_matches((Some("Kanturu"), Some((300,66))), Dest::Kanturu, None));
    }
    #[test]
    fn respeita_tolerancia_estavel_e_regiao_do_spawn() {
        assert!(destination_matches((Some("Kanturu"), Some((92,67))), Dest::Kanturu, Some(((91,66),2))));
        assert!(destination_matches((Some("Lorencia"), Some((123,117))), Dest::Lorencia, Some(((123,129),16))));
        assert!(!destination_matches((Some("Lorencia"), Some((123,100))), Dest::Lorencia, Some(((123,129),16))));
    }
}
