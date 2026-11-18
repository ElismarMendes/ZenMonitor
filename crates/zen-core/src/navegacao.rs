//! Navegação (18/11): parâmetros num lugar só, vigia de progresso pela coordenada real e
//! patrulha por cobertura do mapa. Tudo puro (sem tela nem memória): o agente informa posição,
//! tempo e o que leu; daqui saem só decisões. Testado aqui com tempos simulados.

use std::collections::HashMap;

/// Parâmetros da navegação. `[navegacao]` no agent.toml; o que faltar fica no padrão.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct NavTuning {
    /// Parado (mesma coordenada) por isto depois de mandar andar = travado. 2,5 s cobre a
    /// latência normal do servidor (a coordenada muda a cada ~0,3 s andando).
    pub stall_ms: u64,
    /// Sem encurtar a rota restante por isto = trecho sem progresso: desiste do trecho.
    /// (Antes: 90–170 s até "cliques demais sem chegar".)
    pub leg_no_progress_s: u64,
    /// Desvios locais (mob na rota) por trecho antes de recalcular a rota inteira.
    pub max_local_detours: u32,
    /// Recálculos completos por trecho antes de desistir (etapa 6).
    pub max_reroutes: u32,
    /// Raio (tiles) em volta de um mob parado na rota que vira obstáculo temporário.
    pub mob_block_radius: i32,
    /// Depois de andar isto (tiles) os obstáculos temporários são esquecidos (mobs andam).
    pub forget_blocks_after_tiles: i32,
}

impl Default for NavTuning {
    fn default() -> Self {
        Self { stall_ms: 2500, leg_no_progress_s: 15, max_local_detours: 4, max_reroutes: 3, mob_block_radius: 1, forget_blocks_after_tiles: 4 }
    }
}

/// O que a vigia concluiu nesta leitura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Andando ou dentro da tolerância.
    Ok,
    /// Coordenada parada há `ms` (passou de `stall_ms`).
    Stalled { ms: u64 },
    /// A rota restante não encurta há `s` segundos (passou de `leg_no_progress_s`).
    NoProgress { s: u64 },
}

/// Vigia de um trecho. `pause()` marca uma ação legítima (skill, janela de NPC): o tempo dela
/// não conta como travamento.
#[derive(Debug, Clone)]
pub struct Progress {
    pub start: (i32, i32),
    pub target: (i32, i32),
    last_pos: (i32, i32),
    moved_at_ms: u64,
    best_remaining: usize,
    improved_at_ms: u64,
    tuning: NavTuning,
}

impl Progress {
    pub fn new(now_ms: u64, start: (i32, i32), target: (i32, i32), remaining: usize, tuning: NavTuning) -> Self {
        Self { start, target, last_pos: start, moved_at_ms: now_ms, best_remaining: remaining, improved_at_ms: now_ms, tuning }
    }

    pub fn pause(&mut self, now_ms: u64) {
        self.moved_at_ms = now_ms;
        self.improved_at_ms = now_ms;
    }

    pub fn update(&mut self, now_ms: u64, pos: (i32, i32), remaining: usize) -> Verdict {
        if pos != self.last_pos {
            self.last_pos = pos;
            self.moved_at_ms = now_ms;
        }
        if remaining < self.best_remaining {
            self.best_remaining = remaining;
            self.improved_at_ms = now_ms;
        }
        let no_prog = now_ms.saturating_sub(self.improved_at_ms);
        if no_prog >= self.tuning.leg_no_progress_s * 1000 {
            return Verdict::NoProgress { s: no_prog / 1000 };
        }
        let still = now_ms.saturating_sub(self.moved_at_ms);
        if still >= self.tuning.stall_ms {
            return Verdict::Stalled { ms: still };
        }
        Verdict::Ok
    }

    /// Registro para o log: início, destino, posição atual e há quanto tempo sem progresso.
    pub fn describe(&self, now_ms: u64) -> String {
        format!(
            "início {:?}, destino {:?}, posição {:?}, parado há {} ms, sem encurtar a rota há {} s",
            self.start,
            self.target,
            self.last_pos,
            now_ms.saturating_sub(self.moved_at_ms),
            now_ms.saturating_sub(self.improved_at_ms) / 1000
        )
    }
}

/// Tiles em volta de mobs que ficam bloqueados temporariamente para o A* (nunca no ATT).
pub fn mob_blocks(mobs: &[(i32, i32)], radius: i32, keep_free: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for &(mx, my) in mobs {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let p = (mx + dx, my + dy);
                if !keep_free.contains(&p) && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- patrulha por cobertura

/// Mapa dividido em células de `cell` tiles. Cada célula guarda quando foi vista pela última
/// vez (o char passou a até `sight` tiles do centro dela). Golden só conta como CONFIRMADO
/// quando lido na memória; o histórico de onde já apareceu é só "provável".
#[derive(Debug, Clone)]
pub struct Coverage {
    pub cell: i32,
    pub sight: i32,
    seen: HashMap<(i32, i32), u64>,
    failed: HashMap<(i32, i32), u64>,
}

impl Coverage {
    pub fn new(cell: i32, sight: i32) -> Self {
        Self { cell: cell.max(4), sight, seen: HashMap::new(), failed: HashMap::new() }
    }

    fn key(&self, p: (i32, i32)) -> (i32, i32) {
        (p.0.div_euclid(self.cell), p.1.div_euclid(self.cell))
    }

    pub fn center(&self, k: (i32, i32)) -> (i32, i32) {
        (k.0 * self.cell + self.cell / 2, k.1 * self.cell + self.cell / 2)
    }

    /// O char em `pos` enxerga as células cujo centro está a até `sight` tiles.
    pub fn mark(&mut self, pos: (i32, i32), now: u64) {
        let r = self.sight / self.cell + 1;
        let k = self.key(pos);
        for dy in -r..=r {
            for dx in -r..=r {
                let c = (k.0 + dx, k.1 + dy);
                let ctr = self.center(c);
                if (ctr.0 - pos.0).abs().max((ctr.1 - pos.1).abs()) <= self.sight {
                    self.seen.insert(c, now);
                }
            }
        }
    }

    /// Trecho até esta célula falhou: evita repetir o mesmo caminho por `ttl` segundos.
    pub fn mark_failed(&mut self, target: (i32, i32), now: u64) {
        let k = self.key(target);
        self.failed.insert(k, now);
    }

    pub fn seen_at(&self, p: (i32, i32)) -> Option<u64> {
        self.seen.get(&self.key(p)).copied()
    }

    /// Próximo alvo: entre as células com centro alcançável (`reach` = passos a pé desde a
    /// posição; `None` = não alcança), a nunca vista ou a mais antiga, desempatando pela
    /// distância. Falhas recentes (`fail_ttl` s) ficam de fora. `reach_center` acha um tile
    /// andável dentro da célula (o centro pode ser parede).
    pub fn next_target(
        &self,
        now: u64,
        fail_ttl: u64,
        cells: &[((i32, i32), (i32, i32))],
        reach: &dyn Fn((i32, i32)) -> Option<u32>,
    ) -> Option<(i32, i32)> {
        let mut best: Option<((u64, u32), (i32, i32))> = None;
        for &(k, tile) in cells {
            if self.failed.get(&k).is_some_and(|t| now.saturating_sub(*t) < fail_ttl) {
                continue;
            }
            let Some(d) = reach(tile) else { continue };
            if d == 0 {
                continue;
            }
            // Idade: nunca vista = máxima. Mais velha primeiro; perto antes de longe.
            let age = self.seen.get(&k).map(|t| now.saturating_sub(*t)).unwrap_or(u64::MAX / 2);
            let score = (u64::MAX - age, d);
            if best.map_or(true, |(b, _)| score < b) {
                best = Some((score, tile));
            }
        }
        best.map(|(_, t)| t)
    }

    /// Fração das células dadas que foi vista nos últimos `window` segundos.
    pub fn covered(&self, now: u64, window: u64, cells: &[((i32, i32), (i32, i32))]) -> f64 {
        if cells.is_empty() {
            return 0.0;
        }
        let n = cells.iter().filter(|(k, _)| self.seen.get(k).is_some_and(|t| now.saturating_sub(*t) <= window)).count();
        n as f64 / cells.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> NavTuning {
        NavTuning::default()
    }

    #[test]
    fn andando_nao_e_travado() {
        let mut p = Progress::new(0, (0, 0), (20, 0), 20, t());
        for k in 1..=20u64 {
            assert_eq!(p.update(k * 400, (k as i32, 0), 20 - k as usize), Verdict::Ok);
        }
    }

    #[test]
    fn parado_vira_travado_em_2_5_s() {
        let mut p = Progress::new(0, (0, 0), (20, 0), 20, t());
        assert_eq!(p.update(2000, (0, 0), 20), Verdict::Ok);
        assert_eq!(p.update(2600, (0, 0), 20), Verdict::Stalled { ms: 2600 });
    }

    #[test]
    fn acao_legitima_nao_conta() {
        let mut p = Progress::new(0, (0, 0), (20, 0), 20, t());
        p.pause(2000); // soltou a skill
        assert_eq!(p.update(4000, (0, 0), 20), Verdict::Ok);
    }

    #[test]
    fn oscilando_sem_encurtar_vira_sem_progresso() {
        let mut p = Progress::new(0, (0, 0), (20, 0), 20, t());
        let mut v = Verdict::Ok;
        for k in 0..40u64 {
            // vai e volta entre dois tiles: anda, mas a rota não encurta
            v = p.update(k * 500, ((k % 2) as i32, 1), 20);
        }
        assert_eq!(v, Verdict::NoProgress { s: 19 });
        assert!(p.describe(19_500).contains("destino (20, 0)"));
    }

    #[test]
    fn bloqueio_de_mob_respeita_destino() {
        let b = mob_blocks(&[(5, 5)], 1, &[(5, 6)]);
        assert_eq!(b.len(), 8);
        assert!(!b.contains(&(5, 6)));
    }

    #[test]
    fn cobertura_prefere_nao_visto_e_pula_falha() {
        let mut c = Coverage::new(16, 12);
        let cells: Vec<_> = [(0, 0), (1, 0), (2, 0)].iter().map(|&k| (k, c.center(k))).collect();
        c.mark((8, 8), 100); // vê a célula (0,0)
        let reach = |p: (i32, i32)| Some((p.0 + 1) as u32);
        assert_eq!(c.next_target(100, 60, &cells, &reach), Some((24, 8)));
        c.mark_failed((24, 8), 100);
        assert_eq!(c.next_target(120, 60, &cells, &reach), Some((40, 8)));
        assert_eq!(c.next_target(200, 60, &cells, &reach), Some((24, 8)), "falha antiga volta a valer");
        let sem = |_: (i32, i32)| None;
        assert_eq!(c.next_target(200, 60, &cells, &sem), None);
        assert!((c.covered(100, 60, &cells) - 1.0 / 3.0).abs() < 1e-9);
    }
}
