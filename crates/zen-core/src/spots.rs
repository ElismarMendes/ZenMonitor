//! Políticas puras de spots. Não leem memória e não executam movimento.
use crate::objects::{Kind, Obj};

/// Margem m?xima para confirmar chegada ao spot de farm.
pub const ARRIVAL_RADIUS: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Spot {
    pub world: u8,
    pub x: i32,
    pub y: i32,
}
pub fn world_for_name(name: &str) -> Option<u8> {
    Some(match name {
        "Lorencia" => 1,
        "Dungeon" => 2,
        "Devias" => 3,
        "Noria" => 4,
        "Lost Tower" => 5,
        "Arena" => 7,
        "Atlans" => 8,
        "Tarkan" => 9,
        "Icarus" => 11,
        "Valley of Loren" => 31,
        "Aida" => 34,
        "Crywolf Fortress" => 35,
        "Kanturu" => 38,
        _ => return None,
    })
}
impl Spot {
    pub fn valid(self) -> bool {
        self.world > 0 && (0..=255).contains(&self.x) && (0..=255).contains(&self.y)
    }
    pub fn near(self, other: Self, radius: i32) -> bool {
        self.world == other.world && (self.x - other.x).abs().max((self.y - other.y).abs()) <= radius
    }
    pub fn id(self) -> String {
        format!("{}:{}:{}", self.world, self.x, self.y)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CatalogSpot {
    pub spot: Spot,
    pub priority: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Member {
    pub machine: String,
    pub name: String,
    pub home: Option<Spot>,
    pub active: Option<Spot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lease {
    pub machine: String,
    pub name: String,
    pub spot: Spot,
    pub until: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Directory {
    pub members: Vec<Member>,
    pub catalog: Vec<CatalogSpot>,
    pub leases: Vec<Lease>,
    #[cfg_attr(feature = "serde", serde(default))]
    sources: std::collections::BTreeMap<String, Vec<CatalogSpot>>,
}
impl Directory {
    pub fn friendly(&self, name: &str) -> bool {
        self.members.iter().any(|m| m.name.eq_ignore_ascii_case(name))
    }
    pub fn owner(&self, spot: Spot, except: &str, now: u64) -> Option<&str> {
        self.members
            .iter()
            .find(|m| {
                !m.name.eq_ignore_ascii_case(except)
                    && [m.home, m.active].into_iter().flatten().any(|s| s.near(spot, 2))
            })
            .map(|m| m.name.as_str())
            .or_else(|| {
                self.leases
                    .iter()
                    .find(|l| l.until > now && !l.name.eq_ignore_ascii_case(except) && l.spot.near(spot, 2))
                    .map(|l| l.name.as_str())
            })
    }
    pub fn register(&mut self, mut member: Member) {
        member.home = member.home.filter(|s| s.valid());
        member.active = member.active.filter(|s| s.valid());
        if let Some(old) = self
            .members
            .iter_mut()
            .find(|m| m.machine == member.machine && m.name.eq_ignore_ascii_case(&member.name))
        {
            *old = member;
        } else {
            self.members.push(member);
        }
    }
    pub fn add_catalog(&mut self, entry: CatalogSpot) {
        if !entry.spot.valid() {
            return;
        }
        if let Some(old) = self.catalog.iter_mut().find(|e| e.spot == entry.spot) {
            old.priority = old.priority.min(entry.priority);
        } else {
            self.catalog.push(entry);
        }
        self.catalog
            .sort_by_key(|c| (c.spot.world, c.priority, c.spot.x, c.spot.y));
    }
    pub fn set_catalog(&mut self, machine: &str, entries: Vec<CatalogSpot>) {
        self.sources.insert(machine.into(), entries);
        self.rebuild_catalog();
    }
    fn rebuild_catalog(&mut self) {
        let entries: Vec<_> = self.sources.values().flatten().cloned().collect();
        self.catalog.clear();
        for entry in entries {
            self.add_catalog(entry);
        }
    }
    pub fn remove_machine(&mut self, machine: &str) {
        // Lembra os nomes como próprios, mas uma remoção explícita libera seus spots.
        for m in &mut self.members {
            if m.machine == machine {
                m.home = None;
                m.active = None;
            }
        }
        self.leases.retain(|l| l.machine != machine);
        self.sources.remove(machine);
        self.rebuild_catalog();
    }
    /// Chamado pelo central sob um único mutex: duas máquinas não recebem o mesmo spot.
    pub fn reserve(&mut self, machine: &str, name: &str, spot: Spot, now: u64) -> Result<(), String> {
        self.leases.retain(|l| l.until > now);
        if !spot.valid() {
            return Err("coordenada/mapa inválidos".into());
        }
        if !self
            .members
            .iter()
            .any(|m| m.machine == machine && m.name.eq_ignore_ascii_case(name))
        {
            return Err("personagem não informado por esta máquina".into());
        }
        if let Some(owner) = self.owner(spot, name, now) {
            return Err(format!("spot reservado para {owner}"));
        }
        self.leases
            .retain(|l| !(l.machine == machine && l.name.eq_ignore_ascii_case(name) && l.spot == spot));
        self.leases.push(Lease {
            machine: machine.into(),
            name: name.into(),
            spot,
            until: now.saturating_add(90),
        });
        Ok(())
    }
    pub fn release(&mut self, machine: &str, name: &str, spot: Spot) {
        self.leases
            .retain(|l| !(l.machine == machine && l.name.eq_ignore_ascii_case(name) && l.spot == spot));
    }
}

/// Ausência de leitura não é spot livre. Nossos jogadores nunca são invasores.
pub fn foreign_occupants<'a>(
    objects: Option<&'a [Obj]>,
    spot: Spot,
    me: &str,
    friendly: impl Fn(&str) -> bool,
) -> Result<Vec<&'a str>, &'static str> {
    let objects = objects.ok_or("leitura de jogadores indisponível")?;
    Ok(objects
        .iter()
        .filter(|o| {
            o.kind == Kind::Player
                && !o.name.eq_ignore_ascii_case(me)
                && !friendly(&o.name)
                && (o.x - spot.x).abs().max((o.y - spot.y).abs()) <= 6
        })
        .map(|o| o.name.as_str())
        .collect())
}

/// KS residente e retorno depois da morte preservam a posse; chegada nova inspeciona ocupação.
pub fn retain_owned(return_after_death: bool, already_established: bool, still_at_spot: bool) -> bool {
    return_after_death || (already_established && still_at_spot)
}

/// Ordem local de prioridade é preservada; o destino atual nunca aparece duas vezes.
pub fn candidates(first: Spot, catalog: &[CatalogSpot], secondary: &[u8]) -> Vec<Spot> {
    let mut result = vec![first];
    for world in std::iter::once(first.world).chain(secondary.iter().copied().filter(|w| *w != first.world)) {
        let mut map: Vec<_> = catalog
            .iter()
            .filter(|c| c.spot.world == world && c.spot.valid())
            .collect();
        map.sort_by_key(|c| (c.priority, c.spot.x, c.spot.y));
        for c in map {
            if !result.contains(&c.spot) {
                result.push(c.spot);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(world: u8, x: i32) -> Spot {
        Spot { world, x, y: 10 }
    }
    fn member(machine: &str, name: &str) -> Member {
        Member {
            machine: machine.into(),
            name: name.into(),
            home: None,
            active: None,
        }
    }
    #[test]
    fn pk_volta_ao_spot_e_ks_residente_nao_inicia_troca() {
        assert!(retain_owned(true, true, false));
        assert!(retain_owned(false, true, true));
        assert!(!retain_owned(false, true, false));
        assert!(!retain_owned(false, false, true));
    }

    #[test]
    fn concorrencia_de_reserva_concede_apenas_um_dono() {
        let mut d = Directory::default();
        d.register(member("A", "Um"));
        d.register(member("B", "Dois"));
        let shared = std::sync::Arc::new(std::sync::Mutex::new(d));
        let handles: Vec<_> = [("A", "Um"), ("B", "Dois")]
            .into_iter()
            .map(|(pc, name)| {
                let shared = shared.clone();
                std::thread::spawn(move || shared.lock().unwrap().reserve(pc, name, s(38, 20), 0).is_ok())
            })
            .collect();
        assert_eq!(
            handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count(),
            1
        );
    }
    #[test]
    fn catalogo_removido_na_origem_nao_continua_no_fallback() {
        let mut d = Directory::default();
        d.set_catalog(
            "A",
            vec![CatalogSpot {
                spot: s(38, 10),
                priority: 0,
            }],
        );
        d.set_catalog(
            "B",
            vec![CatalogSpot {
                spot: s(7, 20),
                priority: 0,
            }],
        );
        d.set_catalog("A", vec![]);
        assert_eq!(d.catalog.len(), 1);
        assert_eq!(d.catalog[0].spot.world, 7);
    }
    #[test]
    fn libera_pc_removido_mas_lembra_seus_personagens() {
        let mut d = Directory::default();
        let mut m = member("A", "Um");
        m.home = Some(s(38, 10));
        d.register(m);
        d.remove_machine("A");
        assert!(d.friendly("Um"));
        assert_eq!(d.owner(s(38, 10), "Dois", 100), None);
    }
    #[test]
    fn reserva_exclusiva_entre_pcs_e_expiracao() {
        let mut d = Directory::default();
        d.register(member("A", "Um"));
        d.register(member("B", "Dois"));
        d.reserve("A", "Um", s(38, 10), 0).unwrap();
        assert!(d.reserve("B", "Dois", s(38, 11), 1).is_err());
        d.reserve("B", "Dois", s(38, 11), 91).unwrap();
        assert!(d.reserve("A", "Dois", s(38, 30), 92).is_err());
    }
    #[test]
    fn principal_e_temporario_continuam_proprios_mesmo_offline() {
        let mut d = Directory::default();
        let mut m = member("A", "Um");
        m.home = Some(s(38, 10));
        m.active = Some(s(7, 20));
        d.register(m);
        assert!(d.friendly("uM"));
        assert_eq!(d.owner(s(38, 11), "Dois", 100), Some("Um"));
        assert_eq!(d.owner(s(7, 20), "Dois", 100), Some("Um"));
    }
    #[test]
    fn ocupacao_externa_e_spot_proprio_levam_ao_mapa_secundario() {
        let mut d = Directory::default();
        d.register(member("PC1", "Eu"));
        let mut other = member("PC2", "Nosso");
        other.home = Some(s(38, 20));
        d.register(other);
        let cat = vec![
            CatalogSpot {
                spot: s(38, 20),
                priority: 1,
            },
            CatalogSpot {
                spot: s(7, 30),
                priority: 0,
            },
        ];
        let external = Obj {
            kind: Kind::Player,
            name: "Externo".into(),
            class: 0,
            x: 10,
            y: 10,
        };
        let mut chosen = None;
        for spot in candidates(s(38, 10), &cat, &[7]) {
            if d.owner(spot, "Eu", 0).is_some() {
                continue;
            }
            d.reserve("PC1", "Eu", spot, 0).unwrap();
            let objects = if spot.world == 38 {
                vec![external.clone()]
            } else {
                vec![]
            };
            if !foreign_occupants(Some(&objects), spot, "Eu", |n| d.friendly(n))
                .unwrap()
                .is_empty()
            {
                d.release("PC1", "Eu", spot);
                continue;
            }
            chosen = Some(spot);
            break;
        }
        assert_eq!(chosen, Some(s(7, 30)));
        assert_eq!(d.owner(s(38, 20), "Eu", 0), Some("Nosso"));
    }
    #[test]
    fn prioridade_por_mapa_sem_duplicar() {
        let cat = vec![
            CatalogSpot {
                spot: s(7, 5),
                priority: 0,
            },
            CatalogSpot {
                spot: s(38, 30),
                priority: 2,
            },
            CatalogSpot {
                spot: s(38, 20),
                priority: 1,
            },
        ];
        assert_eq!(
            candidates(s(38, 10), &cat, &[7]),
            vec![s(38, 10), s(38, 20), s(38, 30), s(7, 5)]
        );
    }
    #[test]
    fn nossos_nao_sao_invasores_e_ausencia_nao_e_vazio() {
        let own = Obj {
            kind: Kind::Player,
            name: "Nosso".into(),
            class: 0,
            x: 10,
            y: 10,
        };
        assert!(foreign_occupants(None, s(38, 10), "Eu", |_| false).is_err());
        assert!(foreign_occupants(Some(&[own]), s(38, 10), "Eu", |n| n == "Nosso")
            .unwrap()
            .is_empty());
    }
}
