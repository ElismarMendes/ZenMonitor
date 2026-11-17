//! Contratos de rotas. Dados declarativos; nenhuma tecla, shell ou caminho de arquivo.
use crate::{objects::Obj, spots::Spot};
pub const MAX_WAYPOINTS: usize = 64;
pub const MAX_SAMPLE_AGE_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObserverContext {
    pub char_name: String,
    pub pid: u32,
    pub session: String,
    pub epoch: u64,
    pub world: u8,
    pub sequence: u64,
    pub age_ms: u64,
    pub map_confirmed: bool,
    pub instance: Option<String>,
}
impl ObserverContext {
    pub fn same_observer(&self, other: &Self) -> bool {
        self.char_name == other.char_name
            && self.pid == other.pid
            && self.session == other.session
            && self.epoch == other.epoch
            && self.world == other.world
            && self.instance == other.instance
    }
    pub fn usable(&self) -> bool {
        self.pid > 0
            && !self.char_name.is_empty()
            && self.char_name.len() <= 32
            && !self.session.is_empty()
            && self.session.len() <= 128
            && self.world > 0
            && self.map_confirmed
            && self.age_ms <= MAX_SAMPLE_AGE_MS
    }
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RadarSnapshot {
    #[cfg_attr(feature = "serde", serde(default))]
    pub installed_routes: Option<Vec<InstalledRoute>>,
    pub context: ObserverContext,
    pub position: Spot,
    pub entities: Option<Vec<Obj>>,
    pub truncated: bool,
    pub range: u16,
    pub busy: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InstalledRoute {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub destination: Spot,
}
impl RadarSnapshot {
    pub fn usable(&self) -> bool {
        self.context.usable() && self.position.valid() && self.context.world == self.position.world
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NpcTarget {
    pub class: u16,
    pub name: String,
    pub location: Spot,
    pub interaction: Spot,
    pub radius: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum RouteTarget {
    Point,
    Spot,
    Npc { npc: NpcTarget },
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RouteBinding {
    pub machine: String,
    pub char_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RouteDefinition {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub world: u8,
    pub waypoints: Vec<Spot>,
    pub target: RouteTarget,
    pub bindings: Vec<RouteBinding>,
    pub return_route: Option<String>,
}
impl RouteDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 120
            || self.id.len() > 80
            || self.world == 0
            || self.revision >= i64::MAX as u64
            || self
                .return_route
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 80)
        {
            return Err("identificação da rota inválida".into());
        }
        if self.waypoints.is_empty() || self.waypoints.len() > MAX_WAYPOINTS {
            return Err("a rota deve ter de 1 a 64 waypoints".into());
        }
        if self.bindings.len() > 32
            || self.bindings.iter().any(|b| {
                b.machine.is_empty() || b.machine.len() > 64 || b.char_name.is_empty() || b.char_name.len() > 32
            })
        {
            return Err("associação inválida".into());
        }
        if self.waypoints.iter().any(|p| !p.valid() || p.world != self.world) {
            return Err("waypoint inválido ou em outro mapa".into());
        }
        if let RouteTarget::Npc { npc } = &self.target {
            if npc.class == 0
                || npc.name.is_empty()
                || npc.name.len() > 32
                || npc.radius == 0
                || npc.radius > 3
                || !npc.location.valid()
                || !npc.interaction.valid()
                || npc.location.world != self.world
                || npc.interaction.world != self.world
                || !npc.location.near(npc.interaction, npc.radius as i32)
                || self.waypoints.last() != Some(&npc.interaction)
            {
                return Err("NPC/posição de interação inválidos".into());
            }
        }
        Ok(())
    }
    pub fn goal(&self) -> Spot {
        *self.waypoints.last().expect("rota validada")
    }
    pub fn arrived(&self, index: usize, position: Spot) -> bool {
        if !position.valid() || position.world != self.world {
            return false;
        }
        let Some(goal) = self.waypoints.get(index) else {
            return false;
        };
        if index + 1 == self.waypoints.len() {
            match &self.target {
                RouteTarget::Spot => return position.near(*goal, 2),
                RouteTarget::Npc { npc } => {
                    return position.near(npc.interaction, npc.radius as i32)
                        && position.near(npc.location, npc.radius as i32)
                }
                _ => {}
            }
        }
        position.near(*goal, 1)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoutePlan {
    pub world: u8,
    pub terrain_hash: String,
    pub segments: Vec<Vec<Spot>>,
    pub tiles: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spot_stays_at_two_tiles_and_npc_uses_its_interaction_area() {
        let mut r = route();
        r.target = RouteTarget::Spot;
        assert!(r.arrived(
            0,
            Spot {
                world: 1,
                x: 124,
                y: 110
            }
        ));
        assert!(!r.arrived(
            0,
            Spot {
                world: 1,
                x: 125,
                y: 108
            }
        ));
        r.target = RouteTarget::Npc {
            npc: NpcTarget {
                class: 226,
                name: "Trainer".into(),
                location: Spot {
                    world: 1,
                    x: 122,
                    y: 110,
                },
                interaction: r.goal(),
                radius: 3,
            },
        };
        assert!(r.arrived(
            0,
            Spot {
                world: 1,
                x: 122,
                y: 111
            }
        ));
        assert!(!r.arrived(
            0,
            Spot {
                world: 1,
                x: 119,
                y: 113
            }
        ));
        assert!(!r.arrived(
            0,
            Spot {
                world: 38,
                x: 122,
                y: 111
            }
        ));
    }
    pub fn route() -> RouteDefinition {
        RouteDefinition {
            id: "a".into(),
            revision: 1,
            name: "rota".into(),
            world: 1,
            waypoints: vec![Spot {
                world: 1,
                x: 122,
                y: 108,
            }],
            target: RouteTarget::Point,
            bindings: vec![],
            return_route: None,
        }
    }
    #[test]
    fn rejects_wrong_world_and_invalid_npc() {
        let mut r = route();
        r.waypoints[0].world = 38;
        assert!(r.validate().is_err());
        r = route();
        r.target = RouteTarget::Npc {
            npc: NpcTarget {
                class: 0,
                name: "Trainer".into(),
                location: Spot {
                    world: 1,
                    x: 122,
                    y: 110,
                },
                interaction: r.goal(),
                radius: 2,
            },
        };
        assert!(r.validate().is_err());
    }
}
