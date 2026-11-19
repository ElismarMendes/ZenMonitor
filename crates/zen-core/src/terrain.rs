//! Terreno do mapa (arquivos `Data\WorldN\EncTerrainN.att` do cliente) e cálculo de rota.
//!
//! Formato verificado em 08/11/2026 contra o cliente aberto (MuMain: ZzzLodTerrain.cpp /
//! _crypt.h) e contra os arquivos do The Classic:
//! - Cifra: MapFileDecrypt (chave de 16 bytes, subtração encadeada começando em 0x5E) e
//!   depois BuxConvert (XOR com FC CF AB).
//! - Cabeçalho de 4 bytes: [0, número da pasta World, 255, 255]. Depois 256x256 tiles,
//!   1 byte (arquivo de 65540) ou 2 bytes (131076) por tile; vale o byte baixo.
//! - Índice = y * 256 + x (conferido: os 33 tiles por onde os chars andaram são livres).
//! - Bits: 1 = zona segura, 4 = não anda (parede), 8 = sem chão.
//! - Conferência conhecida: Lorencia (World1) tile [123*256+135] == 5.

pub const SIZE: usize = 256;
pub const SAFEZONE: u8 = 0x01;
/// Entidade/personagem no tile (o servidor marca; o cliente usa para colisão).
pub const CHARACTER: u8 = 0x02;
pub const NOMOVE: u8 = 0x04;
pub const NOGROUND: u8 = 0x08;
pub const WATER: u8 = 0x10;

const KEY: [u8; 16] = [0xD1, 0x73, 0x52, 0xF6, 0xD2, 0x9A, 0xCB, 0x27, 0x3E, 0xAF, 0x59, 0x31, 0x37, 0xB3, 0xE7, 0xA2];
const BUX: [u8; 3] = [0xFC, 0xCF, 0xAB];

#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// Número da pasta World (cabeçalho do arquivo). Kanturu 1 = 38.
    pub world: u8,
    tiles: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerrainError {
    Size(usize),
    Header([u8; 4]),
}

impl std::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerrainError::Size(n) => write!(f, "tamanho inesperado: {n} bytes (esperado 65540 ou 131076)"),
            TerrainError::Header(h) => write!(f, "cabeçalho inesperado: {h:?} (esperado [0, mapa, 255, 255])"),
        }
    }
}

fn decrypt(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut sub: u8 = 0x5E;
    for (n, &c) in raw.iter().enumerate() {
        let v = (KEY[n % 16] ^ c).wrapping_sub(sub);
        sub = c.wrapping_add(0x3D);
        out.push(v ^ BUX[n % 3]);
    }
    out
}

impl Terrain {
    pub fn with_blocked(&self,blocked:&[(i32,i32)])->Self {
        let mut t=self.clone();
        for &(x,y) in blocked {if self.attr(x,y).is_some(){t.tiles[y as usize*SIZE+x as usize]|=NOMOVE;}}
        t
    }
    /// Lê o arquivo criptografado do cliente.
    pub fn from_client_file(raw: &[u8]) -> Result<Self, TerrainError> {
        let per_tile = match raw.len() {
            65540 => 1,
            131076 => 2,
            n => return Err(TerrainError::Size(n)),
        };
        // Terrain.att bruto do servidor: 65540 bytes, cabeçalho [0, mapa, 255, 255] sem cifra.
        let d = if per_tile == 1 && raw[0] == 0 && raw[2] == 255 && raw[3] == 255 { raw.to_vec() } else { decrypt(raw) };
        let hdr = [d[0], d[1], d[2], d[3]];
        if hdr[0] != 0 || hdr[2] != 255 || hdr[3] != 255 {
            return Err(TerrainError::Header(hdr));
        }
        let tiles = d[4..].chunks_exact(per_tile).map(|c| c[0]).collect();
        let t = Self { world: hdr[1], tiles };
        let extra = extra_blocked(t.world);
        Ok(if extra.is_empty() { t } else { t.with_blocked(&extra) })
    }

    /// Bits de cada tile (índice y * 256 + x).
    pub fn tiles(&self) -> &[u8] {
        &self.tiles
    }

    /// Exporta no formato do servidor (`Terrain.att`): 4 bytes de cabeçalho + 65.536 bytes.
    pub fn to_server_file(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + SIZE * SIZE);
        v.extend_from_slice(&[0, self.world, 255, 255]);
        v.extend_from_slice(&self.tiles);
        v
    }

    pub fn attr(&self, x: i32, y: i32) -> Option<u8> {
        if !(0..SIZE as i32).contains(&x) || !(0..SIZE as i32).contains(&y) {
            return None;
        }
        Some(self.tiles[y as usize * SIZE + x as usize])
    }

    /// Piso onde o char pode pisar.
    pub fn walkable(&self, x: i32, y: i32) -> bool {
        self.attr(x, y).is_some_and(|a| a & (NOMOVE | NOGROUND) == 0)
    }

    /// Passos a pé (8 vizinhos) de cada tile até a origem mais próxima; `u16::MAX` = sem
    /// caminho. Origens podem ser não andáveis (safe zone com NOMOVE, ponto de chegada do M).
    pub fn distances_from(&self, sources: &[(i32, i32)]) -> Vec<u16> {
        let mut dist = vec![u16::MAX; SIZE * SIZE];
        let mut q = std::collections::VecDeque::new();
        for &(x, y) in sources {
            if self.attr(x, y).is_some() && dist[y as usize * SIZE + x as usize] != 0 {
                dist[y as usize * SIZE + x as usize] = 0;
                q.push_back((x, y));
            }
        }
        while let Some((x, y)) = q.pop_front() {
            let d = dist[y as usize * SIZE + x as usize];
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if (dx, dy) == (0, 0) || !self.walkable(nx, ny) {
                        continue;
                    }
                    let i = ny as usize * SIZE + nx as usize;
                    if dist[i] == u16::MAX {
                        dist[i] = d + 1;
                        q.push_back((nx, ny));
                    }
                }
            }
        }
        dist
    }

    /// Todos os tiles de safe zone do mapa.
    pub fn safe_tiles(&self) -> Vec<(i32, i32)> {
        (0..SIZE * SIZE)
            .filter(|i| self.tiles[*i] & SAFEZONE != 0)
            .map(|i| ((i % SIZE) as i32, (i / SIZE) as i32))
            .collect()
    }

    pub fn is_safezone(&self, x: i32, y: i32) -> bool {
        self.attr(x, y).is_some_and(|a| a & SAFEZONE != 0)
    }

    /// Rota em tiles de `from` até `to` (A*, 8 direções, sem cortar quina de parede).
    /// Inclui o destino; não inclui a origem. `None` se não houver caminho.
    pub fn path(&self, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        self.path_with(from, to, false)
    }

    /// Rota de fuga: igual a `path`, mas aceita passar na diagonal entre duas quinas de parede.
    /// Medido em 18/11 08:15 (Lorencia): o x9901 entrou em (173,114), um bolso de 27 tiles
    /// separado do resto do mapa por uma parede de 1 tile em x=174; o servidor deixou passar na
    /// diagonal, mas `path` (que não corta quina) não achava saída e a patrulha travou 3 min.
    /// Só é usada quando `path` não acha nada.
    pub fn path_escape(&self, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        self.path_with(from, to, true)
    }

    fn path_with(&self, from: (i32, i32), to: (i32, i32), cut_corners: bool) -> Option<Vec<(i32, i32)>> {
        self.path_opts(from, to, PathOptions { cut_corners, ..Default::default() })
    }

    /// A* (19/11): heap binária com chave f32, vetores planos (g f32, veio-de i32, fechado u8),
    /// 8 direções com custo 1 e √2, heurística octile × 1,001 (desempate a favor da linha reta),
    /// sem cortar quina (a diagonal só passa se os dois vizinhos retos forem piso).
    /// `prefer_safe`: custo × 0,7 dentro da safe e × 1,3 fora. Inclui o destino, não a origem.
    pub fn path_opts(&self, from: (i32, i32), to: (i32, i32), o: PathOptions) -> Option<Vec<(i32, i32)>> {
        if !self.walkable(to.0, to.1) || !self.walkable(from.0, from.1) {
            return None;
        }
        if from == to {
            return Some(vec![to]);
        }
        const N: usize = SIZE * SIZE;
        let idx = |x: i32, y: i32| y as usize * SIZE + x as usize;
        let s2 = std::f32::consts::SQRT_2;
        let h = |x: i32, y: i32| {
            let (dx, dy) = ((x - to.0).abs() as f32, (y - to.1).abs() as f32);
            (dx.max(dy) + (s2 - 1.0) * dx.min(dy)) * 1.001 * if o.prefer_safe { 0.7 } else { 1.0 }
        };
        let mut g = vec![f32::INFINITY; N];
        let mut came = vec![-1i32; N];
        let mut closed = vec![0u8; N];
        let mut heap = MinHeap::default();
        let start = idx(from.0, from.1);
        let goal = idx(to.0, to.1);
        g[start] = 0.0;
        heap.push(h(from.0, from.1), start as u32);
        while let Some(cur) = heap.pop() {
            let cur = cur as usize;
            if closed[cur] != 0 {
                continue;
            }
            closed[cur] = 1;
            if cur == goal {
                let mut out = Vec::new();
                let mut c = cur as i32;
                while c >= 0 && c as usize != start {
                    out.push(((c as usize % SIZE) as i32, (c as usize / SIZE) as i32));
                    c = came[c as usize];
                }
                out.reverse();
                return Some(out);
            }
            let (x, y) = ((cur % SIZE) as i32, (cur / SIZE) as i32);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if !self.walkable(nx, ny) || closed[idx(nx, ny)] != 0 {
                    continue;
                }
                if !o.cut_corners && dx != 0 && dy != 0 && !(self.walkable(x + dx, y) && self.walkable(x, y + dy)) {
                    continue;
                }
                let mut step = if dx != 0 && dy != 0 { s2 } else { 1.0 };
                if o.prefer_safe {
                    step *= if self.is_safezone(nx, ny) { 0.7 } else { 1.3 };
                }
                let ng = g[cur] + step;
                let ni = idx(nx, ny);
                if ng < g[ni] {
                    g[ni] = ng;
                    came[ni] = cur as i32;
                    heap.push(ng + h(nx, ny), ni as u32);
                }
            }
        }
        None
    }

    /// Rota encadeada: início -> wp1 -> wp2 -> ... -> destino, sem repetir os nós de junção.
    pub fn path_via(&self, from: (i32, i32), via: &[(i32, i32)], to: (i32, i32), o: PathOptions) -> Option<Vec<(i32, i32)>> {
        let mut out = Vec::new();
        let mut cur = from;
        for &p in via.iter().chain(std::iter::once(&to)) {
            if p == cur {
                continue;
            }
            out.extend(self.path_opts(cur, p, o)?);
            cur = p;
        }
        Some(out)
    }

    /// Linha de visão por Bresenham sem cortar quina: todos os tiles da linha são piso e cada
    /// passo diagonal tem os dois vizinhos retos livres.
    pub fn los_clear(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        if !self.walkable(a.0, a.1) {
            return false;
        }
        let (mut x, mut y) = a;
        let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
        let (sx, sy) = (if a.0 < b.0 { 1 } else { -1 }, if a.1 < b.1 { 1 } else { -1 });
        let mut err = dx + dy;
        while (x, y) != b {
            let e2 = 2 * err;
            let (mut nx, mut ny) = (x, y);
            if e2 >= dy {
                err += dy;
                nx += sx;
            }
            if e2 <= dx {
                err += dx;
                ny += sy;
            }
            if !self.walkable(nx, ny) || (nx != x && ny != y && !(self.walkable(nx, y) && self.walkable(x, ny))) {
                return false;
            }
            x = nx;
            y = ny;
        }
        true
    }

    /// Suavização por linha de visão: tira os zigue-zagues, mantendo só os pontos de virada
    /// necessários (nenhum trecho atravessa parede nem corta quina).
    pub fn smooth(&self, from: (i32, i32), path: &[(i32, i32)]) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        let mut anchor = from;
        let mut i = 0;
        while i < path.len() {
            let mut j = i;
            while j + 1 < path.len() && self.los_clear(anchor, path[j + 1]) {
                j += 1;
            }
            out.push(path[j]);
            anchor = path[j];
            i = j + 1;
        }
        out
    }

    /// Pontos de passagem para cliques: o ponto mais distante da rota (até `max_step` tiles
    /// em linha reta, sem atravessar parede) a partir de cada parada.
    pub fn waypoints(&self, from: (i32, i32), path: &[(i32, i32)], max_step: i32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        let mut cur = from;
        let mut i = 0;
        while i < path.len() {
            let mut best = i;
            for j in i..path.len() {
                let p = path[j];
                if (p.0 - cur.0).abs().max((p.1 - cur.1).abs()) > max_step {
                    break;
                }
                // 19/11: sem cortar quina (antes `line_clear` deixava o clique atravessar a quina).
                if self.los_clear(cur, p) {
                    best = j;
                }
            }
            out.push(path[best]);
            cur = path[best];
            i = best + 1;
        }
        out
    }

    /// Linha reta entre dois tiles só passa por piso.
    pub fn line_clear(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        let n = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
        (0..=n).all(|k| {
            let x = a.0 as f32 + (b.0 - a.0) as f32 * k as f32 / n as f32;
            let y = a.1 as f32 + (b.1 - a.1) as f32 * k as f32 / n as f32;
            self.walkable(x.round() as i32, y.round() as i32)
        })
    }

    /// Versão para o novo planejador: mesma regra de quina aplicada pelo A*.
    pub fn segment_clear(&self,a:(i32,i32),b:(i32,i32))->bool {
        let n=(b.0-a.0).abs().max((b.1-a.1).abs()).max(1);let mut last=a;
        if !self.walkable(a.0,a.1){return false;}
        for k in 1..=n {let p=(a.0+((b.0-a.0) as f64*k as f64/n as f64).round() as i32,a.1+((b.1-a.1) as f64*k as f64/n as f64).round() as i32);
            if !self.walkable(p.0,p.1)||(p.0!=last.0&&p.1!=last.1&&(!self.walkable(p.0,last.1)||!self.walkable(last.0,p.1))){return false;}last=p;}
        true
    }
}

/// Opções do A*.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PathOptions {
    /// Aceita a diagonal entre duas quinas (só para rota de fuga).
    pub cut_corners: bool,
    /// Prefere andar pela safe (custo × 0,7 dentro, × 1,3 fora).
    pub prefer_safe: bool,
}

/// Heap binária mínima de (chave f32 >= 0, nó). Chaves não negativas comparam igual aos bits.
#[derive(Default)]
struct MinHeap {
    v: Vec<(u32, u32)>,
}

impl MinHeap {
    fn push(&mut self, key: f32, node: u32) {
        self.v.push((key.max(0.0).to_bits(), node));
        let mut i = self.v.len() - 1;
        while i > 0 {
            let p = (i - 1) / 2;
            if self.v[p].0 <= self.v[i].0 {
                break;
            }
            self.v.swap(p, i);
            i = p;
        }
    }
    fn pop(&mut self) -> Option<u32> {
        if self.v.is_empty() {
            return None;
        }
        let top = self.v.swap_remove(0).1;
        let n = self.v.len();
        let mut i = 0;
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut m = i;
            if l < n && self.v[l].0 < self.v[m].0 {
                m = l;
            }
            if r < n && self.v[r].0 < self.v[m].0 {
                m = r;
            }
            if m == i {
                break;
            }
            self.v.swap(i, m);
            i = m;
        }
        Some(top)
    }
}

/// Posição do mundo (float lido na memória) -> tile. Medido: tile = valor / 100.
pub fn world_to_tile(v: f32) -> i32 {
    (v / 100.0).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> Terrain {
        let mut tiles = vec![NOMOVE; SIZE * SIZE];
        for (y, r) in rows.iter().enumerate() {
            for (x, c) in r.chars().enumerate() {
                tiles[y * SIZE + x] = if c == '.' { 0 } else { NOMOVE };
            }
        }
        Terrain { world: 0, tiles }
    }

    #[test]
    fn rota_contorna_parede_e_nao_corta_quina() {
        let t = grid(&["......", ".####.", ".#..#.", ".#..#.", ".####.", "......"]);
        assert_eq!(t.path((0, 0), (2, 2)), None, "o miolo está fechado");
        let p = t.path((0, 0), (5, 5)).unwrap();
        assert_eq!(*p.last().unwrap(), (5, 5));
        assert!(p.iter().all(|&(x, y)| t.walkable(x, y)));
        // Diagonal entre duas paredes não pode.
        let t = grid(&[".#", "#."]);
        assert_eq!(t.path((0, 0), (1, 1)), None);
    }

    #[test]
    fn fuga_corta_quina_so_quando_preciso() {
        // Bolso fechado por parede de 1 tile, com passagem só na diagonal (Lorencia 173,114).
        let t = grid(&["..#..", "..#..", "...#.", "..#.."]);
        assert_eq!(t.path((0, 0), (4, 0)), None);
        let p = t.path_escape((0, 0), (4, 0)).expect("sai pela diagonal");
        assert_eq!(*p.last().unwrap(), (4, 0));
        assert!(p.iter().all(|&(x, y)| t.walkable(x, y)));
        // Destino murado de verdade continua sem rota.
        let t = grid(&["..#..", "..#..", "..#..", "..#.."]);
        assert_eq!(t.path_escape((0, 0), (4, 0)), None);
    }

    #[test]
    fn waypoints_em_linha_reta_e_limite_de_passo() {
        let t = grid(&["..........", ".........."]);
        let p = t.path((0, 0), (9, 0)).unwrap();
        assert_eq!(t.waypoints((0, 0), &p, 4), vec![(4, 0), (8, 0), (9, 0)]);
    }
    #[test]
    fn segmento_novo_recusa_atalho_pela_quina_e_preserva_legado() {
        let t=grid(&[".#", "#."]);
        assert!(!t.segment_clear((0,0),(1,1)));
        assert!(t.line_clear((0,0),(1,1)),"critério antigo permanece intacto");
        let t=grid(&["....", "...."]);
        assert!(t.segment_clear((0,0),(3,1)));
        assert!(!t.segment_clear((-1,0),(3,1)));
    }
}

#[cfg(test)]
mod distancia_tests {
    use super::*;
    #[test]
    fn distancia_a_pe_contorna_parede_e_marca_sem_caminho() {
        // 5x5 no canto: parede vertical em x=2 com passagem em y=4.
        let mut tiles = vec![NOMOVE; SIZE * SIZE];
        for y in 0..5 { for x in 0..5 { if x != 2 || y == 4 { tiles[y * SIZE + x] = 0; } } }
        tiles[10 * SIZE + 10] = 0; // ilha sem ligação
        let t = Terrain { world: 1, tiles };
        let d = t.distances_from(&[(0, 0)]);
        assert_eq!(d[0], 0);
        assert_eq!(d[4], 8, "(4,0): desce até y=4, passa e sobe");
        assert_eq!(d[10 * SIZE + 10], u16::MAX);
    }
}

/// Obstáculos que o arquivo do cliente marca como piso, mas onde o char trava no jogo. Valem para
/// tudo (rotas do agente, painel, patrulha). Cada entrada: (mapa, x0, y0, x1, y1) inclusivo.
/// - Kanturu, cristais no corredor leste (dono, 18/11 21:44): vários chars presos em (227,148)
///   indo para o spot (225,137); a rota passava colada na parede leste (x 226-227).
const EXTRA_BLOCKED: &[(u8, i32, i32, i32, i32)] = &[(38, 226, 144, 227, 152)];

pub fn extra_blocked(world: u8) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for &(w, x0, y0, x1, y1) in EXTRA_BLOCKED {
        if w == world {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    v.push((x, y));
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod astar_tests {
    use super::*;

    fn grid(rows: &[&str]) -> Terrain {
        let mut tiles = vec![NOMOVE; SIZE * SIZE];
        for (y, r) in rows.iter().enumerate() {
            for (x, c) in r.chars().enumerate() {
                tiles[y * SIZE + x] = match c { '.' => 0, 's' => SAFEZONE, _ => NOMOVE };
            }
        }
        Terrain { world: 9, tiles }
    }

    #[test]
    fn linha_reta_quando_possivel_e_custo_octile() {
        let t = grid(&["..........", "..........", ".........."]);
        let p = t.path((0, 1), (9, 1)).unwrap();
        assert_eq!(p.len(), 9);
        assert!(p.iter().all(|&(_, y)| y == 1), "desempate 1,001 favorece a reta: {p:?}");
    }

    #[test]
    fn diagonal_so_com_os_dois_vizinhos_retos_livres() {
        let t = grid(&["..#", "...", "..."]);
        let p = t.path((1, 1), (2, 0));
        assert_eq!(p, None, "(2,0) é parede");
        let t = grid(&[".#.", "...", "..."]);
        let p = t.path((0, 1), (2, 0)).unwrap();
        // (0,1)->(1,0) cortaria a quina de (1,0)? (1,0) é parede; não pode ir na diagonal para (1,0)
        assert!(p.windows(2).all(|w| { let (a, b) = (w[0], w[1]); a.0 == b.0 || a.1 == b.1 || (t.walkable(b.0, a.1) && t.walkable(a.0, b.1)) }));
    }

    #[test]
    fn varios_waypoints_sem_repetir_juncao() {
        let t = grid(&["..........", "..........", ".........."]);
        let p = t.path_via((0, 0), &[(5, 0), (5, 2)], (9, 2), PathOptions::default()).unwrap();
        assert_eq!(p.iter().filter(|&&q| q == (5, 0)).count(), 1);
        assert_eq!(p.iter().filter(|&&q| q == (5, 2)).count(), 1);
        assert_eq!(*p.last().unwrap(), (9, 2));
    }

    #[test]
    fn prefere_safe_quando_pedido() {
        // Duas faixas: em cima normal, embaixo safe; a de baixo é 1 passo mais longa.
        let t = grid(&["..........", ".########.", "ssssssssss"]);
        let normal = t.path((0, 0), (9, 0)).unwrap();
        assert!(normal.iter().all(|&(_, y)| y == 0));
        let safe = t.path_opts((0, 1), (9, 1), PathOptions { prefer_safe: true, ..Default::default() }).unwrap();
        assert!(safe.iter().filter(|&&(_, y)| y == 2).count() > safe.iter().filter(|&&(_, y)| y == 0).count(), "{safe:?}");
    }

    #[test]
    fn suavizacao_tira_zigue_zague_sem_atravessar_quina() {
        let t = grid(&["..........", "....#.....", ".........."]);
        let p = t.path((0, 1), (9, 1)).unwrap();
        let s = t.smooth((0, 1), &p);
        assert!(s.len() < p.len());
        let mut a = (0, 1);
        for &b in &s {
            assert!(t.los_clear(a, b), "{a:?} -> {b:?}");
            a = b;
        }
        assert_eq!(*s.last().unwrap(), (9, 1));
        let q = grid(&[".#", "#."]);
        assert!(!q.los_clear((0, 0), (1, 1)));
    }

    #[test]
    fn exporta_e_le_formato_do_servidor() {
        let t = grid(&["s.#", "..."]);
        let raw = t.to_server_file();
        assert_eq!(raw.len(), 65540);
        assert_eq!(&raw[..4], &[0, 9, 255, 255]);
        let back = Terrain::from_client_file(&raw).unwrap();
        assert_eq!(back, t);
    }
}
