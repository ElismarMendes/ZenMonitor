//! Políticas puras de spots. Não leem memória e não executam movimento.
use crate::objects::{Kind, Obj};

/// Margem m?xima para confirmar chegada ao spot de farm.
pub const ARRIVAL_RADIUS: i32 = 2;
/// Jogador de fora a até tantos tiles = spot ocupado. Era 6; o helper tem range 6, então dois chars
/// a até 12 tiles disputam os mesmos mobs (KS do x9901 no 4RTURZ1N, 18/11 08:25).
pub const OCCUPIED_RADIUS: i32 = 12;

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
    /// Spots do catálogo desativados no painel (ficam no catálogo, mas a busca pula).
    #[cfg_attr(feature = "serde", serde(default))]
    pub disabled: Vec<Spot>,
    /// Última ocupação vista de cada spot por QUALQUER char nosso a até `OCCUPIED_RADIUS`
    /// (a central junta a visão de todos os PCs). Só vale como confirmação se for recente.
    #[cfg_attr(feature = "serde", serde(default))]
    pub observed: Vec<Observed>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Observed {
    pub spot: Spot,
    pub at: u64,
    /// Jogadores de fora vistos a até 12 tiles (vazio = livre naquele momento).
    pub foreign: Vec<String>,
    /// Quem viu.
    pub by: String,
}

/// Um char nosso que está enxergando: mapa, posição e jogadores em volta (nome, posição).
pub struct Viewer<'a> {
    pub world: u8,
    pub pos: (i32, i32),
    pub name: &'a str,
    pub players: Vec<(&'a str, (i32, i32))>,
}

/// Ocupação dos spots do catálogo que algum char nosso está vendo agora. Um spot só é
/// "visto" se o char está a até `OCCUPIED_RADIUS` dele (enxerga o raio inteiro).
pub fn observe_catalog(catalog: &[CatalogSpot], viewers: &[Viewer], ours: &dyn Fn(&str) -> bool, now: u64) -> Vec<Observed> {
    let mut out = Vec::new();
    for c in catalog {
        let sp = c.spot;
        let Some(v) = viewers.iter().find(|v| v.world == sp.world && (v.pos.0 - sp.x).abs().max((v.pos.1 - sp.y).abs()) <= OCCUPIED_RADIUS) else { continue };
        let mut foreign: Vec<String> = viewers
            .iter()
            .filter(|v| v.world == sp.world)
            .flat_map(|v| v.players.iter())
            .filter(|(n, p)| !ours(n) && (p.0 - sp.x).abs().max((p.1 - sp.y).abs()) <= OCCUPIED_RADIUS)
            .map(|(n, _)| n.to_string())
            .collect();
        foreign.sort();
        foreign.dedup();
        out.push(Observed { spot: sp, at: now, foreign, by: v.name.to_string() });
    }
    out
}

impl Directory {
    pub fn is_disabled(&self, s: Spot) -> bool {
        self.disabled.iter().any(|d| d.near(s, 0))
    }
    /// Observação recente (até `max_age` s) do spot, se houver.
    pub fn observation(&self, s: Spot, now: u64, max_age: u64) -> Option<&Observed> {
        self.observed.iter().find(|o| o.spot.near(s, 2) && now.saturating_sub(o.at) <= max_age)
    }
    /// Junta observações novas às antigas (a mais nova de cada spot fica).
    pub fn merge_observed(&mut self, fresh: Vec<Observed>) {
        for o in fresh {
            self.observed.retain(|x| !x.spot.near(o.spot, 0));
            self.observed.push(o);
        }
    }
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
                && (o.x - spot.x).abs().max((o.y - spot.y).abs()) <= OCCUPIED_RADIUS
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

/// v5 (17/11, pedido do dono): candidatos em ordem de distância a pé da entrada do mapa (safe
/// zone ou chegada do M), só nos mapas permitidos ao char. O destino pedido (`first`) vem
/// sempre primeiro. Spots a até 2 tiles de um já listado contam como o mesmo spot. Sem
/// distância conhecida (terreno ilegível, sem caminho) o spot vai para o fim, na ordem do F2.
pub fn candidates_ranked(first: Spot, catalog: &[CatalogSpot], worlds: &[u8], dist: &dyn Fn(Spot) -> Option<u32>) -> Vec<Spot> {
    let mut result = vec![first];
    let mut order: Vec<u8> = vec![first.world];
    for w in worlds {
        if !order.contains(w) {
            order.push(*w);
        }
    }
    for world in order {
        let mut map: Vec<_> = catalog.iter().filter(|c| c.spot.world == world && c.spot.valid()).collect();
        map.sort_by_key(|c| (dist(c.spot).unwrap_or(u32::MAX), c.priority, c.spot.x, c.spot.y));
        for c in map {
            if !result.iter().any(|r| r.near(c.spot, 2)) {
                result.push(c.spot);
            }
        }
    }
    result
}

/// Regras de farm de um char (dono, 18/11 13:27). Mapa ORIGINAL = mapa do F1 (só muda por ação
/// explícita). Mapas alternativos em ordem de prioridade (padrão: Arena primeiro).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FarmRules {
    pub mapas: Vec<String>,
    /// Minutos num mapa alternativo antes de tentar o original de novo.
    pub retorno_min: u64,
    /// Procura spot em mapa alternativo quando o original está lotado.
    pub alternativos: bool,
    /// Na hora da reavaliação, ir até o mapa original conferir pessoalmente. Desligado = só
    /// reavalia quando houver vaga confirmada por outra fonte (hoje não há: fica onde está).
    pub sondagem: bool,
}

impl Default for FarmRules {
    fn default() -> Self {
        Self { mapas: vec!["Arena".into()], retorno_min: 60, alternativos: true, sondagem: true }
    }
}

/// Ordem final da busca: spot pedido -> spots do mesmo mapa (mais perto primeiro) -> o spot
/// alternativo onde já estava (não larga um spot produtivo à toa) -> mapas alternativos na
/// ordem dada. Mapas desconhecidos e o próprio mapa original saem da lista dos alternativos.
pub fn search_order(
    first: Spot,
    home: Spot,
    current_alt: Option<Spot>,
    catalog: &[CatalogSpot],
    rules: &FarmRules,
    dist: &dyn Fn(Spot) -> Option<u32>,
) -> Vec<Spot> {
    let alt_worlds: Vec<u8> = if rules.alternativos {
        rules.mapas.iter().filter_map(|m| world_for_name(m)).filter(|w| *w != home.world && *w != 1).collect()
    } else {
        Vec::new()
    };
    let mut out = candidates_ranked(first, catalog, &[home.world], dist);
    if first.world != home.world {
        for s in candidates_ranked(home, catalog, &[], dist) {
            if !out.iter().any(|r| r.near(s, 2)) {
                out.push(s);
            }
        }
    }
    if let Some(a) = current_alt.filter(|a| a.world != home.world) {
        if !out.iter().any(|r| r.near(a, 2)) {
            out.push(a);
        }
    }
    for w in alt_worlds {
        let mut map: Vec<_> = catalog.iter().filter(|c| c.spot.world == w && c.spot.valid()).collect();
        map.sort_by_key(|c| (dist(c.spot).unwrap_or(u32::MAX), c.priority, c.spot.x, c.spot.y));
        for c in map {
            if !out.iter().any(|r| r.near(c.spot, 2)) {
                out.push(c.spot);
            }
        }
    }
    out
}

#[cfg(test)]
mod observed_tests {
    use super::*;
    #[test]
    fn so_ve_quem_esta_perto_e_ignora_os_nossos() {
        let cat = vec![CatalogSpot { spot: Spot { world: 38, x: 100, y: 100 }, priority: 0 }, CatalogSpot { spot: Spot { world: 38, x: 10, y: 10 }, priority: 1 }];
        let v = vec![Viewer { world: 38, pos: (105, 100), name: "Eu", players: vec![("Fulano", (110, 100)), ("Nosso", (100, 101)), ("Longe", (130, 100))] }];
        let o = observe_catalog(&cat, &v, &|n| n == "Nosso", 50);
        assert_eq!(o.len(), 1, "o spot (10,10) ninguém está vendo");
        assert_eq!(o[0].foreign, vec!["Fulano".to_string()]);
        let mut d = Directory::default();
        d.merge_observed(o);
        assert!(d.observation(Spot { world: 38, x: 100, y: 100 }, 100, 60).is_some());
        assert!(d.observation(Spot { world: 38, x: 100, y: 100 }, 200, 60).is_none(), "antiga não vale");
        d.disabled.push(Spot { world: 38, x: 10, y: 10 });
        assert!(d.is_disabled(Spot { world: 38, x: 10, y: 10 }));
    }
}

#[cfg(test)]
mod farm_tests {
    use super::*;
    fn c(w: u8, x: i32, p: u32) -> CatalogSpot { CatalogSpot { spot: Spot { world: w, x, y: 50 }, priority: p } }
    #[test]
    fn ordem_original_depois_arena_depois_outros() {
        let home = Spot { world: 38, x: 10, y: 50 };
        let cat = vec![c(38, 10, 0), c(38, 100, 1), c(2, 30, 0), c(7, 40, 0), c(7, 60, 1)];
        let rules = FarmRules { mapas: vec!["Arena".into(), "Dungeon".into()], ..Default::default() };
        let d = |s: Spot| Some(s.x as u32);
        let o = search_order(home, home, None, &cat, &rules, &d);
        let worlds: Vec<u8> = o.iter().map(|s| s.world).collect();
        assert_eq!(worlds, vec![38, 38, 7, 7, 2]);
    }
    #[test]
    fn reavaliacao_volta_ao_alternativo_antes_de_outros_mapas() {
        let home = Spot { world: 38, x: 10, y: 50 };
        let alt = Spot { world: 7, x: 60, y: 50 };
        let cat = vec![c(38, 10, 0), c(7, 40, 0), c(7, 60, 1)];
        let o = search_order(home, home, Some(alt), &cat, &FarmRules::default(), &|s: Spot| Some(s.x as u32));
        assert_eq!(o[1], alt, "original lotado: volta ao spot alternativo onde estava");
    }
    #[test]
    fn sem_alternativos_fica_no_mapa_original() {
        let home = Spot { world: 38, x: 10, y: 50 };
        let cat = vec![c(38, 10, 0), c(7, 40, 0)];
        let rules = FarmRules { alternativos: false, ..Default::default() };
        let o = search_order(home, home, None, &cat, &rules, &|_| Some(1));
        assert!(o.iter().all(|s| s.world == 38));
        let lor = FarmRules { mapas: vec!["Lorencia".into(), "Inexistente".into()], ..Default::default() };
        assert!(search_order(home, home, None, &cat, &lor, &|_| Some(1)).iter().all(|s| s.world == 38));
    }
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
    fn ordem_pela_distancia_da_safe_so_nos_mapas_permitidos() {
        let c = |w: u8, x: i32, y: i32, p: u32| CatalogSpot { spot: Spot { world: w, x, y }, priority: p };
        let catalog = vec![c(38, 90, 66, 0), c(38, 40, 232, 5), c(38, 41, 233, 6), c(38, 123, 233, 1), c(2, 9, 59, 0), c(7, 48, 57, 0)];
        let dist = |s: Spot| Some(match (s.x, s.y) { (40, 232) => 13, (41, 233) => 14, (123, 233) => 80, (90, 66) => 433, _ => 50 });
        let home = Spot { world: 38, x: 46, y: 71 };
        // Só o mapa do F1: o mais perto da safe primeiro; 41,233 (a 1 tile de 40,232) não repete.
        assert_eq!(candidates_ranked(home, &catalog, &[], &dist),
            vec![home, Spot { world: 38, x: 40, y: 232 }, Spot { world: 38, x: 123, y: 233 }, Spot { world: 38, x: 90, y: 66 }]);
        // Dungeon liberado: entra depois de todo o Kanturu; Arena (7) não liberada nunca aparece.
        let r = candidates_ranked(home, &catalog, &[2], &dist);
        assert_eq!(r.last(), Some(&Spot { world: 2, x: 9, y: 59 }));
        assert!(!r.iter().any(|s| s.world == 7));
        // Sem distância: ordem do F2 no fim.
        let r = candidates_ranked(home, &catalog, &[], &|_| None);
        assert_eq!(r[1], Spot { world: 38, x: 90, y: 66 });
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
