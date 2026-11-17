use super::*;

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

pub(super) const fn g(button: (i32, i32), arrival: (i32, i32)) -> Gate {
    Gate { button, arrival }
}

/// Chegadas medidas em 11/11 (x9901, um move por botão).
pub(super) const GATES_LOST_TOWER: [Gate; 7] = [
    g((355, 416), (206, 72)),
    g((323, 440), (243, 237)),
    g((345, 440), (86, 167)),
    g((367, 440), (87, 88)),
    g((389, 440), (128, 53)),
    g((345, 461), (52, 53)),
    g((367, 461), (8, 85)),
];
pub(super) const GATES_DUNGEON: [Gate; 3] = [g((406, 571), (107, 247)), g((390, 599), (231, 126)), g((412, 599), (3, 83))];

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
pub(super) const M_STADIUM: (i32, i32) = (783, 529);

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
pub(super) fn blocked(x: i32, y: i32) -> bool {
    (80..=390).contains(&x) && (135..=625).contains(&y)
        || !(75..=680).contains(&y)
        || !(15..=1010).contains(&x)
        || ((x - 910).pow(2) + (y - 555).pow(2)) < 100 * 100
}
