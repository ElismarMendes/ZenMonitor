//! Planejamento puro: colisões estáticas, bloqueios temporários e orçamento de progresso.
use crate::{
    routes::{RouteDefinition, RoutePlan},
    spots::Spot,
    terrain::Terrain,
};
use std::collections::BTreeMap;
pub fn safe_path(t: &Terrain, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    if let Some(path) = t.path(from, to) {
        return Some(path);
    }
    if t.walkable(from.0, from.1) || t.attr(from.0, from.1)? & crate::terrain::NOGROUND != 0 {
        return None;
    }
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(dx, dy)| {
            let exit = (from.0 + dx, from.1 + dy);
            let mut path = t.path(exit, to)?;
            path.insert(0, exit);
            Some(path)
        })
        .min_by_key(Vec::len)
}

pub fn terrain_hash(t: &Terrain) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for value in std::iter::once(t.world).chain((0..256).flat_map(|y| (0..256).map(move |x| t.attr(x, y).unwrap_or(0))))
    {
        hash ^= u64::from(value);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
/// A held click can advance beside/past the cached path head between samples.
/// Search the remaining path for a reachable nearby point instead of rejecting
/// it just because the first cached point is now behind the character.
pub fn next_step(t:&Terrain,at:(i32,i32),path:&[(i32,i32)])->Option<(i32,i32)> {
    path.iter().copied()
        .filter(|p|*p!=at && (p.0-at.0).abs().max((p.1-at.1).abs())<=3)
        .filter(|p|t.segment_clear(at,*p)||(!t.walkable(at.0,at.1)&&t.walkable(p.0,p.1)&&(p.0-at.0).abs()+(p.1-at.1).abs()==1))
        .last()
}
pub fn path_to_waypoint(t: &Terrain, at: (i32, i32), route: &RouteDefinition, index: usize) -> Option<Vec<(i32, i32)>> {
    let p = route.waypoints.get(index)?;
    if route.world != t.world
        || p.world != t.world
        || !p.valid()
        || !(0..256).contains(&at.0)
        || !(0..256).contains(&at.1)
    {
        return None;
    }
    let current = Spot {
        world: t.world,
        x: at.0,
        y: at.1,
    };
    let final_npc =
        index + 1 == route.waypoints.len() && matches!(route.target, crate::routes::RouteTarget::Npc { .. });
    if final_npc && route.arrived(index, current) {
        return Some(vec![at]);
    }
    if let Some(path) = safe_path(t, at, (p.x, p.y)) {
        return Some(path);
    }
    if !final_npc {
        return None;
    }
    let mut candidates = Vec::new();
    for dx in -3..=3 {
        for dy in -3..=3 {
            let s = Spot {
                world: t.world,
                x: p.x + dx,
                y: p.y + dy,
            };
            if route.arrived(index, s) && t.walkable(s.x, s.y) {
                candidates.push(s);
            }
        }
    }
    candidates.sort_by_key(|s| {
        (
            (s.x - p.x).abs().max((s.y - p.y).abs()),
            (s.x - at.0).abs().max((s.y - at.1).abs()),
        )
    });
    candidates.into_iter().find_map(|s| safe_path(t, at, (s.x, s.y)))
}
pub fn plan(t: &Terrain, from: Spot, route: &RouteDefinition) -> Result<RoutePlan, String> {
    route.validate()?;
    if from.world != t.world || route.world != t.world || !from.valid() {
        return Err("mapa do terreno/origem/destino divergente".into());
    }
    let mut at = (from.x, from.y);
    let mut segments = Vec::new();
    let mut tiles = 0;
    for (index, p) in route.waypoints.iter().enumerate() {
        let path = path_to_waypoint(t, at, route, index)
            .ok_or_else(|| format!("sem caminho de {at:?} até ({},{})", p.x, p.y))?;
        tiles += path.len();
        if tiles > 8192 {
            return Err("rota excede o limite de 8192 tiles planejados".into());
        }
        at = path.last().copied().unwrap_or((p.x, p.y));
        segments.push(path.into_iter().map(|(x, y)| Spot { world: t.world, x, y }).collect());
    }
    Ok(RoutePlan {
        world: t.world,
        terrain_hash: terrain_hash(t),
        segments,
        tiles,
    })
}
#[derive(Debug, Default)]
pub struct DynamicObstacles {
    blocked: BTreeMap<(i32, i32), u64>,
}
impl DynamicObstacles {
    /// Só recebe bloqueios observados por ausência de progresso; NPC não é parede implícita.
    pub fn record(&mut self, tile: (i32, i32), now_ms: u64) {
        self.blocked.insert(tile, now_ms.saturating_add(15_000));
    }
    pub fn terrain(&mut self, base: &Terrain, now_ms: u64) -> Terrain {
        self.blocked.retain(|_, until| *until > now_ms);
        base.with_blocked(&self.blocked.keys().copied().collect::<Vec<_>>())
    }
}
#[derive(Debug)]
pub struct Progress {
    last: (i32, i32),
    last_progress_ms: u64,
    previous: Option<(i32, i32)>,
    oscillations: u8,
    replans: u8,
}
impl Progress {
    pub fn resume(&mut self, now_ms: u64) {
        self.last_progress_ms = now_ms;
        self.oscillations = 0;
        self.previous = None;
    }
    pub fn new(at: (i32, i32), now_ms: u64) -> Self {
        Self {
            last: at,
            last_progress_ms: now_ms,
            previous: None,
            oscillations: 0,
            replans: 0,
        }
    }
    pub fn observe(&mut self, at: (i32, i32), now_ms: u64) -> bool {
        if at != self.last {
            if Some(at) == self.previous {
                self.oscillations += 1;
            } else {
                self.oscillations = 0;
            }
            self.previous = Some(self.last);
            self.last = at;
            self.last_progress_ms = now_ms;
        }
        now_ms.saturating_sub(self.last_progress_ms) >= 1800 || self.oscillations >= 3
    }
    pub fn replan(&mut self, now_ms: u64) -> Result<(), String> {
        if self.replans >= 2 {
            return Err("limite de dois replanejamentos do segmento".into());
        }
        self.replans += 1;
        self.last_progress_ms = now_ms;
        self.oscillations = 0;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::{RouteDefinition, RouteTarget};
    fn terrain() -> Terrain {
        Terrain::from_client_file(include_bytes!("../tests/fixtures/terrain/EncTerrain1.att")).unwrap()
    }
    #[test]
    fn multi_waypoint_and_blocked_target() {
        let t = terrain();
        let from = Spot {
            world: 1,
            x: 122,
            y: 108,
        };
        let mut r = RouteDefinition {
            id: "r".into(),
            revision: 1,
            name: "Trainer".into(),
            world: 1,
            waypoints: vec![
                from,
                Spot {
                    world: 1,
                    x: 123,
                    y: 108,
                },
            ],
            target: RouteTarget::Point,
            bindings: vec![],
            return_route: None,
        };
        assert_eq!(plan(&t, from, &r).unwrap().segments.len(), 2);
        r.waypoints[1] = Spot {
            world: 1,
            x: 135,
            y: 123,
        };
        assert!(plan(&t, from, &r).is_err());
    }
    #[test]
    fn obstacles_expire_without_changing_original() {
        let t = terrain();
        let mut d = DynamicObstacles::default();
        d.record((122, 108), 0);
        assert!(!d.terrain(&t, 1).walkable(122, 108));
        assert!(t.walkable(122, 108));
        assert!(d.terrain(&t, 15_001).walkable(122, 108));
    }
    #[test]
    fn detects_stall_oscillation_and_limits_retries() {
        let mut p = Progress::new((1, 1), 0);
        assert!(!p.observe((1, 1), 1799));
        assert!(p.observe((1, 1), 1800));
        p.replan(1800).unwrap();
        p.replan(3600).unwrap();
        assert!(p.replan(5400).is_err());
        let mut p = Progress::new((1, 1), 0);
        for (i, at) in [(2, 1), (1, 1), (2, 1)].into_iter().enumerate() {
            p.observe(at, i as u64 * 100);
        }
        assert!(p.observe((1, 1), 400));
    }
    #[test]
    fn closing_a_window_restarts_progress_clock_without_spending_path_retries() {
        let mut p = Progress::new((33, 216), 0);
        assert!(p.observe((33, 216), 1800));
        p.resume(5000);
        assert!(!p.observe((33, 216), 5200));
        p.replan(7000).unwrap();
        p.replan(9000).unwrap();
        assert!(p.replan(11000).is_err());
    }
}
