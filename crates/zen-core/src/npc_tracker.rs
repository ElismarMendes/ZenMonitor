//! Confirmação por observações existentes; não inventa endereços/IDs de memória.
use crate::{
    objects::{Kind, Obj},
    routes::{NpcTarget, ObserverContext, RadarSnapshot},
};
#[derive(Debug, Default)]
pub struct NpcTracker {
    last: Option<RadarSnapshot>,
}
impl NpcTracker {
    pub fn observe(&mut self, snapshot: RadarSnapshot) {
        self.last = Some(snapshot);
    }
    pub fn confirmed(&self, next: &RadarSnapshot, target: &NpcTarget) -> Result<Obj, String> {
        let before = self.last.as_ref().ok_or("NPC aguarda duas observações consecutivas")?;
        if !before.usable()
            || !next.usable()
            || !before.context.same_observer(&next.context)
            || next.context.sequence <= before.context.sequence
        {
            return Err("contexto/idade de observação inválido".into());
        }
        let matches = |s: &RadarSnapshot| -> Vec<Obj> {
            s.entities
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter(|o| {
                    o.kind == Kind::Npc
                        && o.class == target.class
                        && o.name.eq_ignore_ascii_case(&target.name)
                        && (o.x - target.location.x).abs().max((o.y - target.location.y).abs()) <= 1
                })
                .cloned()
                .collect()
        };
        let a = matches(before);
        let b = matches(next);
        if a.len() != 1
            || b.len() != 1
            || target.location.world != next.context.world
            || (a[0].x - b[0].x).abs().max((a[0].y - b[0].y).abs()) > 1
        {
            return Err("NPC ausente, ambíguo ou deslocado".into());
        }
        Ok(b[0].clone())
    }
}
pub fn context_matches(a: &ObserverContext, b: &ObserverContext) -> bool {
    a.same_observer(b) && a.usable() && b.usable()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::spots::Spot;
    fn sample(sequence: u64) -> RadarSnapshot {
        RadarSnapshot {
            installed_routes: None,
            context: ObserverContext {
                char_name: "x9901".into(),
                pid: 12,
                session: "process-start".into(),
                epoch: 1,
                world: 1,
                sequence,
                age_ms: 0,
                map_confirmed: true,
                instance: None,
            },
            position: Spot {
                world: 1,
                x: 122,
                y: 108,
            },
            entities: Some(vec![Obj {
                kind: Kind::Npc,
                name: "Trainer".into(),
                class: 226,
                x: 122,
                y: 110,
            }]),
            truncated: false,
            range: 25,
            busy: false,
        }
    }
    fn target() -> NpcTarget {
        NpcTarget {
            class: 226,
            name: "Trainer".into(),
            location: Spot {
                world: 1,
                x: 122,
                y: 110,
            },
            interaction: Spot {
                world: 1,
                x: 122,
                y: 108,
            },
            radius: 2,
        }
    }
    #[test]
    fn trainer_requires_two_fresh_matching_observations() {
        let mut t = NpcTracker::default();
        let next = sample(2);
        assert!(t.confirmed(&next, &target()).is_err());
        t.observe(sample(1));
        assert_eq!(t.confirmed(&next, &target()).unwrap().class, 226);
        for mutate in [0, 1, 2, 3, 4, 5, 6, 7] {
            let mut bad = next.clone();
            match mutate {
                0 => bad.context.pid += 1,
                1 => bad.context.world = 38,
                2 => bad.context.age_ms = 10001,
                3 => bad.entities = None,
                4 => bad.entities.as_mut().unwrap()[0].kind = Kind::Monster,
                5 => bad.entities.as_mut().unwrap()[0].class = 240,
                6 => bad.entities.as_mut().unwrap()[0].x = 130,
                _ => {
                    let e = bad.entities.as_ref().unwrap()[0].clone();
                    bad.entities.as_mut().unwrap().push(e);
                }
            }
            assert!(t.confirmed(&bad, &target()).is_err(), "caso {mutate}");
        }
        let mut bad = next;
        bad.context.sequence = 1;
        assert!(t.confirmed(&bad, &target()).is_err());
    }
}
