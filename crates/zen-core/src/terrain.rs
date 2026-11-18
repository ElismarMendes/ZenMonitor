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
pub const NOMOVE: u8 = 0x04;
pub const NOGROUND: u8 = 0x08;

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
        let d = decrypt(raw);
        let hdr = [d[0], d[1], d[2], d[3]];
        if hdr[0] != 0 || hdr[2] != 255 || hdr[3] != 255 {
            return Err(TerrainError::Header(hdr));
        }
        let tiles = d[4..].chunks_exact(per_tile).map(|c| c[0]).collect();
        Ok(Self { world: hdr[1], tiles })
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
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        if !self.walkable(to.0, to.1) || !self.walkable(from.0, from.1) {
            return None;
        }
        let idx = |x: i32, y: i32| y as usize * SIZE + x as usize;
        let h = |x: i32, y: i32| {
            let (dx, dy) = ((x - to.0).abs(), (y - to.1).abs());
            (10 * (dx + dy) - 6 * dx.min(dy)) as u32 // octile: reto 10, diagonal 14
        };
        let mut g = vec![u32::MAX; SIZE * SIZE];
        let mut came = vec![u32::MAX; SIZE * SIZE];
        let mut open = BinaryHeap::new();
        g[idx(from.0, from.1)] = 0;
        open.push(Reverse((h(from.0, from.1), 0u32, from.0, from.1)));
        while let Some(Reverse((_, gc, x, y))) = open.pop() {
            if (x, y) == to {
                let mut out = vec![(x, y)];
                let mut cur = idx(x, y);
                while came[cur] != u32::MAX && came[cur] as usize != idx(from.0, from.1) {
                    cur = came[cur] as usize;
                    out.push(((cur % SIZE) as i32, (cur / SIZE) as i32));
                }
                out.reverse();
                return Some(out);
            }
            if gc > g[idx(x, y)] {
                continue;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if !self.walkable(nx, ny) {
                    continue;
                }
                if !cut_corners && dx != 0 && dy != 0 && !(self.walkable(x + dx, y) && self.walkable(x, y + dy)) {
                    continue;
                }
                let ng = gc + if dx != 0 && dy != 0 { 14 } else { 10 };
                if ng < g[idx(nx, ny)] {
                    g[idx(nx, ny)] = ng;
                    came[idx(nx, ny)] = idx(x, y) as u32;
                    open.push(Reverse((ng + h(nx, ny), ng, nx, ny)));
                }
            }
        }
        None
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
                if self.line_clear(cur, p) {
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
