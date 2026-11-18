//! Orquestração de chegada. Usa nav e objects existentes, sem implementar outro mouse/leitor.
use crate::win::{input, nav, objects};
use crate::{config::AgentConfig, spot_service as group};
use std::cell::RefCell;
use zen_core::spots::{foreign_occupants, CatalogSpot, Spot};

struct Arrival {
    name: String,
    spot: Spot,
    friendly: Vec<String>,
    epoch: u64,
    retain_owned: bool,
    checks: ApproachChecks,
}
#[derive(Default)]
struct ApproachChecks { entered: bool }
impl ApproachChecks {
    fn run(&mut self, distance: i32, final_check: bool, check: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        if distance > 12 || (self.entered && !final_check) { return Ok(()); }
        check()?;
        self.entered = true;
        Ok(())
    }
}

#[cfg(test)]
mod approach_tests {
    use super::*;
    #[test]
    fn fallback_desligado_restringe_destino_sem_desativar_guardas() {
        let first=Spot{world:38,x:10,y:10};
        let alternate=Spot{world:7,x:20,y:20};
        let catalog=vec![CatalogSpot{spot:alternate,priority:0}];
        assert_eq!(arrival_choices(first,first,&catalog,&[7],false,false,false),vec![first]);
        assert_eq!(arrival_choices(first,first,&catalog,&[7],true,false,false),vec![first,alternate]);
        assert_eq!(arrival_choices(alternate,first,&catalog,&[7],false,false,false),vec![alternate]);
        let result=guarded("Teste",first,vec![],0,false,||check_approach_at(0,7,(10,10),true));
        assert!(result.unwrap_err().starts_with("SPOT_MAP_MISMATCH"));
    }
    #[test]
    fn le_ocupacao_so_na_entrada_e_na_finalizacao() {
        let mut gate = ApproachChecks::default();
        let mut reads = 0;
        for distance in (13..=100).rev().chain((0..=12).rev()).chain(std::iter::repeat_n(2, 100)) {
            gate.run(distance, false, || { reads += 1; Ok(()) }).unwrap();
        }
        assert_eq!(reads, 1, "os ciclos e cliques não repetem a leitura");
        gate.run(2, true, || { reads += 1; Ok(()) }).unwrap();
        assert_eq!(reads, 2, "a chegada exige uma leitura nova");
    }
    #[test]
    fn falha_na_leitura_nao_vira_vaga_e_jogador_chegando_depois_bloqueia_helper() {
        let mut gate = ApproachChecks::default();
        assert!(gate.run(10, false, || Err("SPOT_UNVERIFIED".into())).is_err());
        assert!(!gate.entered);
        gate.run(10, false, || Ok(())).unwrap();
        let result = gate.run(2, true, || Err("SPOT_OCCUPIED: jogador chegou depois".into()));
        assert!(occupied_error(&result.unwrap_err()));
    }
}
thread_local! { static ARRIVAL: RefCell<Option<Arrival>> = const {RefCell::new(None)}; }
pub fn occupied_error(error: &str) -> bool {
    error.starts_with("SPOT_OCCUPIED:") || error.starts_with("SPOT_OWNED:")
}

/// Cancelamento leve: nenhuma captura ou leitura de objetos no ciclo de movimento.
pub fn check_movement_epoch() -> Result<(), String> {
    ARRIVAL.with(|slot| {
        let arrival = slot.borrow();
        let Some(a) = arrival.as_ref() else { return Ok(()) };
        if group::epoch(&a.name) != a.epoch {
            return Err("SPOT_CANCELLED: spot alterado pelo dono durante o trajeto".into());
        }
        Ok(())
    })
}

/// Reutiliza a posição já lida pela navegação. Lê ocupação na entrada e força nova
/// leitura antes de ligar o Helper. Nunca é gatilho de fuga por KS no spot estabelecido.
pub fn check_approach_at(hwnd: isize, world: u8, pos: (i32, i32), final_check: bool) -> Result<(), String> {
    check_movement_epoch()?;
    ARRIVAL.with(|slot| {
        let mut arrival = slot.borrow_mut();
        let Some(a) = arrival.as_mut() else { return Ok(()) };
        if world != a.spot.world { return Err("SPOT_MAP_MISMATCH: mapa da aproximação incorreto".into()); }
        let distance = (pos.0 - a.spot.x).abs().max((pos.1 - a.spot.y).abs());
        let Arrival { name, spot, friendly, retain_owned, checks, .. } = a;
        checks.run(distance, final_check, || {
            let directory = group::directory(true)?;
            if let Some(owner) = directory.owner(*spot, name, now()) {
                return Err(format!("SPOT_OWNED: {} pertence a {owner}", spot.id()));
            }
            if *retain_owned {
                return Ok(());
            } // Não é uma declaração de spot vazio: a posse foi estabelecida antes.
            // v5: a leitura de jogadores pode falhar num ciclo isolado (x9901, 17/11); tenta de novo
            // antes de desistir. Falha persistente continua sendo "não verificado", nunca "vazio".
            let mut objects = objects::nearby_hwnd(hwnd, name, pos);
            for _ in 0..3 {
                if objects.is_some() { break; }
                std::thread::sleep(std::time::Duration::from_millis(400));
                objects = objects::nearby_hwnd(hwnd, name, pos);
            }
            let foreign = foreign_occupants(objects.as_deref(), *spot, name, |n| {
                directory.friendly(n) || friendly.iter().any(|f| f.eq_ignore_ascii_case(n))
        })
        .map_err(|e| format!("SPOT_UNVERIFIED: {e}"))?;
        if !foreign.is_empty() {
            return Err(format!("SPOT_OCCUPIED: {} por {}", spot.id(), foreign.join(", ")));
        }
        Ok(())
        })
    })
}
fn guarded<T>(
    name: &str,
    spot: Spot,
    friendly: Vec<String>,
    epoch: u64,
    retain_owned: bool,
    f: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<Arrival>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ARRIVAL.with(|s| *s.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(ARRIVAL.with(|s| {
        s.replace(Some(Arrival {
            name: name.into(),
            spot,
            friendly,
            epoch,
            retain_owned,
            checks: ApproachChecks::default(),
        }))
    }));
    f()
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0)
}
/// Checagens de chegada existentes aplicadas às rotas explicitamente aprovadas.
pub fn route_guard<T>(name:&str,spot:Spot,f:impl FnOnce()->T)->Result<T,String> {
    let cfg=AgentConfig::load_file().map_err(|e|e.to_string())?;
    let friendly=cfg.spots.keys().cloned().chain(cfg.friendly_chars.iter().cloned()).collect();
    let retain_owned=group::home(&cfg,name)==Some(spot)||group::active(name).is_some_and(|a|a.spot==spot);
    Ok(guarded(name,spot,friendly,group::epoch(name),retain_owned,f))
}

/// Passos a pé até a entrada de cada mapa: safe zone + pontos de chegada do M (Dungeon não tem
/// safe; os andares da Lost Tower só se ligam por portal). Terreno ilegível = mapa fica de fora
/// (ordem do F2).
fn entry_distances(hwnd: isize, worlds: impl Iterator<Item = u8>) -> std::collections::HashMap<u8, Vec<u16>> {
    let mut out = std::collections::HashMap::new();
    for w in worlds {
        if out.contains_key(&w) { continue; }
        let Ok(t) = nav::world_terrain_of(hwnd, w) else { continue };
        let mut sources = t.safe_tiles();
        if let Some(d) = nav::Dest::from_world(w) {
            sources.extend(d.gates().iter().map(|g| g.arrival));
        }
        out.insert(w, t.distances_from(&sources));
    }
    out
}

pub fn local_catalog(cfg: &AgentConfig) -> Vec<CatalogSpot> {
    let mut result: Vec<_> = cfg
        .catalog
        .iter()
        .enumerate()
        .map(|(i, [x, y])| CatalogSpot {
            spot: Spot {
                world: 38,
                x: *x,
                y: *y,
            },
            priority: i as u32,
        })
        .collect();
    for (map, points) in &cfg.catalog_maps {
        if let Some(world) = zen_core::spots::world_for_name(map) {
            result.extend(points.iter().enumerate().map(|(i, [x, y])| CatalogSpot {
                spot: Spot { world, x: *x, y: *y },
                priority: i as u32,
            }));
        }
    }
    result
}

/// Uma varredura limitada, com reserva exclusiva antes de cada tentativa de movimento.
pub fn recover(hwnd: isize, name: &str, first: Spot, status: &dyn Fn(String)) -> Result<String, String> {
    recover_with(hwnd, name, first, false, status)
}

pub fn resume(
    hwnd: isize,
    name: &str,
    first: Spot,
    after_death: bool,
    status: &dyn Fn(String),
) -> Result<String, String> {
    let established = group::active(name).is_some_and(|a| a.spot == first);
    let (map, pos) = nav::observe_named(hwnd);
    let resident = map.and_then(zen_core::spots::world_for_name) == Some(first.world)
        && pos.is_some_and(|p| (p.0 - first.x).abs().max((p.1 - first.y).abs()) <= zen_core::spots::ARRIVAL_RADIUS);
    recover_with(
        hwnd,
        name,
        first,
        zen_core::spots::retain_owned(after_death, established, resident),
        status,
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn arrival_choices(first: Spot, home: Spot, catalog: &[CatalogSpot], secondary: &[u8], fallback: bool, keep_owned: bool, locked: bool) -> Vec<Spot> {
    arrival_choices_ranked(first, home, catalog, secondary, fallback, keep_owned, locked, &|_| None)
}

/// Ordem de busca: o destino pedido, depois os spots do mapa do F1 do mais perto ao mais longe
/// da entrada (safe zone ou chegada do M), depois os mapas liberados para o char.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
fn arrival_choices_ranked(first: Spot, home: Spot, catalog: &[CatalogSpot], secondary: &[u8], fallback: bool, keep_owned: bool, locked: bool, dist: &dyn Fn(Spot) -> Option<u32>) -> Vec<Spot> {
    let mut choices = zen_core::spots::candidates_ranked(first, catalog, secondary, dist);
    // O principal sempre é considerado antes dos mapas secundários.
    if first.world != home.world {
        choices = std::iter::once(first)
            .chain(zen_core::spots::candidates_ranked(home, catalog, secondary, dist))
            .collect();
        let mut unique = Vec::new();
        choices.retain(|s| {
            if unique.contains(s) {
                false
            } else {
                unique.push(*s);
                true
            }
        });
    }
    // Desativar fallback limita os destinos; nunca contorna reserva ou ocupação.
    if !fallback || keep_owned || locked {
        choices.truncate(1);
    }
    choices
}

fn recover_with(
    hwnd: isize,
    name: &str,
    first: Spot,
    keep_owned: bool,
    status: &dyn Fn(String),
) -> Result<String, String> {
    let generation = group::epoch(name);
    let cfg = AgentConfig::load_file().map_err(|e| e.to_string())?;
    let home = group::home(&cfg, name).ok_or("personagem sem spot principal")?;
    let directory = group::directory(true)?;
    let mut catalog = local_catalog(&cfg);
    for entry in &directory.catalog {
        if !catalog.iter().any(|c| c.spot == entry.spot) {
            let mut e = entry.clone();
            e.priority = e.priority.saturating_add(10_000);
            catalog.push(e);
        }
    }
    // v5 (18/11 13:27): regras de farm do char. Ordem: spot pedido -> mesmo mapa (mais perto)
    // -> spot alternativo onde já estava -> mapas alternativos (padrão: Arena) na ordem do painel.
    let rules = cfg.farm_rules(name);
    let alt_worlds: Vec<u8> = rules.mapas.iter().filter_map(|m| zen_core::spots::world_for_name(m)).filter(|w| *w != 1).collect();
    let distances = entry_distances(hwnd, std::iter::once(first.world).chain(std::iter::once(home.world)).chain(alt_worlds.iter().copied()));
    let dist = |s: Spot| distances.get(&s.world).and_then(|d| d.get(s.y as usize * 256 + s.x as usize)).filter(|v| **v != u16::MAX).map(|v| *v as u32);
    let current_alt = group::active(name).filter(|a| a.home == home && a.spot.world != home.world).map(|a| a.spot);
    let mut choices = zen_core::spots::search_order(first, home, current_alt, &catalog, &rules, &dist);
    // Desativar fallback limita os destinos; nunca contorna reserva ou ocupação.
    if !cfg.spot_fallback || keep_owned || cfg.locked_spots.iter().any(|n| n == name) {
        choices.truncate(1);
    }
    tracing::info!(char = name, ordem = ?choices.iter().map(|s| format!("{} ({} passos)", s.id(), dist(*s).map_or("?".into(), |d| d.to_string()))).collect::<Vec<_>>(), "spots: ordem de busca");
    let friendly: Vec<_> = cfg
        .spots
        .keys()
        .cloned()
        .chain(cfg.friendly_chars.iter().cloned())
        .collect();
    let mut reasons = Vec::new();
    for choice in choices {
        if group::epoch(name) != generation {
            return Err("SPOT_CANCELLED: principal alterado pelo dono".into());
        }
        if input::manual_mode() {
            return Err("busca de spot pausada pelo dono".into());
        }
        if let Some(owner) = group::directory(true)?.owner(choice, name, now()) {
            reasons.push(format!("{} é de {owner}", choice.id()));
            continue;
        }
        let Some(dest) = nav::Dest::from_world(choice.world).filter(|d| *d != nav::Dest::Lorencia) else {
            continue;
        };
        if let Err(e) = group::request(name, choice, false) {
            reasons.push(format!("{}: {e}", choice.id()));
            continue;
        }
        status(format!("buscando spot {} (principal {})", choice.id(), home.id()));
        let attempt = guarded(name, choice, friendly.clone(), generation, keep_owned, || {
            let terrain = nav::world_terrain_of(hwnd, choice.world)?;
            let mut last = String::new();
            for retry in 0..2 {
                match nav::recover_to(hwnd, &terrain, (choice.x, choice.y), dest, status) {
                    Ok(message) => {
                        return Ok(message);
                    }
                    Err(e) => {
                        tracing::warn!(char=name,spot=%choice.id(),retry,motivo=%e,"chegada ao spot falhou");
                        if occupied_error(&e)
                            || e.contains("SPOT_UNVERIFIED")
                            || e.contains("SPOT_CANCELLED")
                            || input::manual_mode()
                        {
                            return Err(e);
                        }
                        last = e;
                        if retry == 0 {
                            nav::prepare_route_retry(hwnd, status)?;
                        }
                    }
                }
            }
            Err(last)
        });
        match attempt {
            Ok(message) => {
                let retry_at = if choice.world != home.world {
                    group::active(name)
                        .filter(|a| a.home == home && a.retry_at > 0)
                        .map(|a| a.retry_at)
                        .unwrap_or_else(|| now() + (rules.retorno_min * 60).clamp(60, 86400))
                } else {
                    0
                };
                let value = group::ActiveSpot {
                    spot: choice,
                    home,
                    retry_at,
                };
                group::set_active_for(name, value, generation)?;
                return Ok(format!(
                    "{message}; spot assumido {}{}",
                    choice.id(),
                    if choice.world != home.world {
                        " (temporário; reavaliação do mapa principal agendada)"
                    } else {
                        ""
                    }
                ));
            }
            Err(e) => {
                if let Err(release_error) = group::request(name, choice, true) {
                    tracing::warn!(char=name,spot=%choice.id(),erro=%release_error,
                        "liberação remota não confirmada; renovação local interrompida, reserva expira no central");
                }
                if occupied_error(&e) {
                    reasons.push(e);
                    continue;
                }
                // Leitura de jogadores falhou: esse spot fica "desconhecido" (nunca "livre") e a
                // busca segue para o próximo, em vez de parar tudo (dono, 18/11 13:27).
                if e.contains("SPOT_UNVERIFIED") {
                    reasons.push(format!("{}: ocupação desconhecida (leitura falhou)", choice.id()));
                    continue;
                }
                return Err(e);
            }
        }
    }
    // Tudo lotado: não fica parado no spot de outro jogador com o helper desligado (dá na cara).
    // Vai para a safe do mapa principal pelo M e espera lá (o ciclo agenda a próxima busca).
    if let Some(d) = nav::Dest::from_world(home.world).filter(|d| d.can_teleport() && *d != nav::Dest::Lorencia) {
        status(format!("todos os spots lotados: indo esperar na safe da {}", d.name()));
        match nav::teleport(hwnd, d) {
            Ok(p) => tracing::info!(char = name, posicao = ?p, "spots lotados: esperando na safe"),
            Err(e) => tracing::warn!(char = name, erro = %e, "spots lotados: teleporte para a safe falhou"),
        }
    }
    Err(format!(
        "nenhum spot livre confirmado; aguardando nova varredura: {}",
        reasons.join("; ")
    ))
}
