//! Executor opt-in: planejamento novo sobre o mesmo Hold/SendInput e leitores existentes.
use super::{input, nav, objects, viewport};
use std::time::{Duration, Instant};
use zen_core::{
    navigation::{path_to_waypoint, DynamicObstacles, Progress},
    routes::{NpcTarget, RadarSnapshot, RouteDefinition, RouteTarget},
    spots::Spot,
    terrain::Terrain,
};
fn position(hwnd: isize, world: u8) -> Result<(i32, i32), String> {
    let (map, pos) = nav::observe_named(hwnd);
    if map.and_then(zen_core::spots::world_for_name) != Some(world) {
        return Err("NAV_MAP: mapa real diferente do mapa da rota".into());
    }
    pos.ok_or("NAV_POSITION: coordenadas não reconhecidas".into())
}
pub fn walk_points(hwnd: isize, t: &Terrain, route: &RouteDefinition, status: &dyn Fn(String)) -> Result<Spot, String> {
    route.validate()?;
    if route.world != t.world {
        return Err("NAV_TERRAIN: terreno divergente".into());
    }
    let started = Instant::now();
    let mut dynamic = DynamicObstacles::default();
    let mut window_retries = 0;
    let mut observed_sequence = None;
    let mut legacy_fallbacks = 0;
    let mut arrival=None;
    for (index, point) in route.waypoints.iter().enumerate() {
        let mut held: Option<input::Hold> = None;
        let mut at = position(hwnd, t.world)?;
        let mut progress = Progress::new(at, 0);
        let segment = Instant::now();
        let mut cached_path: Vec<(i32, i32)> = Vec::new();
        let mut path_dirty = true;
        let mut view = dynamic.terrain(t, started.elapsed().as_millis() as u64);
        loop {
            crate::navigation::check()?;
            if let Some(scene) = crate::navigation::snapshot(&crate::navigation::current_name().unwrap_or_default())
                .filter(|s| s.usable() && s.context.world == t.world)
            {
                if observed_sequence != Some(scene.context.sequence) {
                    nav::route_scene(
                        t.world,
                        scene
                            .entities
                            .unwrap_or_default()
                            .iter()
                            .filter(|e| e.kind == zen_core::objects::Kind::Npc)
                            .map(|e| (e.x, e.y))
                            .collect(),
                    );
                    observed_sequence = Some(scene.context.sequence);
                }
            } else {
                nav::route_scene(t.world, vec![]);
            }
            super::spots::check_approach_at(hwnd, t.world, at, false)?;
            if input::manual_mode() {
                return Err("NAV_PAUSED: navegação interrompida pelo dono".into());
            }
            if segment.elapsed() > Duration::from_secs(120) || started.elapsed() > Duration::from_secs(240) {
                return Err("NAV_TIMEOUT: limite do segmento/rota".into());
            }
            if route.arrived(
                index,
                Spot {
                    world: t.world,
                    x: at.0,
                    y: at.1,
                },
            ) {
                break;
            }
            let ms = started.elapsed().as_millis() as u64;
            if path_dirty || cached_path.is_empty() {
                view = dynamic.terrain(t, ms);
                cached_path = path_to_waypoint(&view, at, route, index)
                    .ok_or_else(|| format!("NAV_NO_PATH: sem rota de {at:?} até ({},{})", point.x, point.y))?;
                path_dirty = false;
            }
            if let Some(i) = cached_path.iter().position(|p| *p == at) {
                cached_path.drain(..=i);
            }
            let next = zen_core::navigation::next_step(&view,at,&cached_path);
            let Some(next) = next else {
                // Stop before re-evaluating the cached path. A held button must
                // not keep moving while a tight loop spends the replan budget.
                drop(held.take());
                let actual=position(hwnd,t.world)?;
                tracing::warn!(rota=%route.id,ponto=index,origem=?at,atual=?actual,cabeca=?cached_path.first(),"NAV_PATH_HEAD: nenhum próximo tile alcançável no cache");
                if actual!=at {at=actual;path_dirty=true;continue;}
                if let Err(reason)=progress.replan(segment.elapsed().as_millis() as u64) {
                    if legacy_fallbacks>=1{return Err(format!("NAV_NO_STEP: {reason}; fallback legado já utilizado"));}
                    legacy_fallbacks+=1;
                    nav::prepare_route_retry(hwnd,status)?;
                    tracing::warn!(rota=%route.id,ponto=index,motivo=%reason,"NAV_LEGACY_FALLBACK: cache sem passo alcançável; um trecho legado");
                    nav::route_legacy_leg(hwnd,t,(point.x,point.y),index+1==route.waypoints.len()&&matches!(route.target,RouteTarget::Spot),status)?;
                    at=position(hwnd,t.world)?;
                    if route.arrived(index,Spot{world:t.world,x:at.0,y:at.1}){break;}
                    return Err("NAV_ARRIVAL: fallback sem passo não confirmou o destino".into());
                }
                path_dirty = true;
                continue;
            };
            let (x, y) = nav::route_step_px(at, next);
            match held.as_mut() {
                Some(h) => h.steer(x, y)?,
                None => held = Some(input::Hold::start(hwnd, x, y)?),
            }
            status(format!(
                "rota {} · ponto {}/{} · {at:?} → ({},{})",
                route.name,
                index + 1,
                route.waypoints.len(),
                point.x,
                point.y
            ));
            std::thread::sleep(Duration::from_millis(200));
            let current = position(hwnd, t.world)?;
            if (current.0 - at.0).abs().max((current.1 - at.1).abs()) > 6 {
                return Err("NAV_POSITION: salto de coordenadas implausível".into());
            }
            if progress.observe(current, segment.elapsed().as_millis() as u64) {
                if let Some(h) = held.as_mut() {
                    h.release();
                }
                held = None;
                if nav::storage_open(hwnd) || nav::inventory_open(hwnd) || nav::npc_dialog_open(hwnd) {
                    if window_retries >= 2 {
                        return Err("NAV_UI: limite de duas recuperações de janelas no trajeto".into());
                    }
                    window_retries += 1;
                    tracing::warn!(rota=%route.id,origem=?current,tentativa=window_retries,"NAV_UI: janela acidental detectada; fechando antes de continuar");
                    // Só após uma janela realmente aberta: evita temporariamente cruzar o
                    // pequeno espaço clicável do NPC próximo, sem mudar o terreno base.
                    if let Some(scene) = crate::navigation::current_name()
                        .and_then(|name| crate::navigation::snapshot(&name))
                        .filter(|s| s.usable() && s.context.world == t.world)
                    {
                        if let Some(npc) = scene
                            .entities
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|e| e.kind == zen_core::objects::Kind::Npc)
                            .filter(|e| (e.x - current.0).abs().max((e.y - current.1).abs()) <= 4)
                            .min_by_key(|e| (e.x - current.0).abs().max((e.y - current.1).abs()))
                        {
                            for dx in -1..=1 {
                                for dy in -1..=1 {
                                    let tile = (npc.x + dx, npc.y + dy);
                                    if tile != (point.x, point.y) {
                                        dynamic.record(tile, started.elapsed().as_millis() as u64);
                                    }
                                }
                            }
                            tracing::warn!(npc=%npc.name,tile=?((npc.x,npc.y)),"NAV_UI: contorno temporário após interação acidental");
                        }
                    }
                    nav::prepare_route_retry(hwnd, status)?;
                    if nav::inventory_open(hwnd) {
                        input::run(hwnd, &[zen_core::actions::Step::Key(0x56)])?;
                    }
                    if viewport::current(hwnd).map(|v| v.home()) != Some((515, 400)) {
                        return Err("NAV_UI: área de jogo não restaurada".into());
                    }
                    at = position(hwnd, t.world)?;
                    progress.resume(segment.elapsed().as_millis() as u64);
                    path_dirty = true;
                    continue;
                }
                if let Err(reason) = progress.replan(segment.elapsed().as_millis() as u64) {
                    if legacy_fallbacks >= 1 {
                        return Err(format!("NAV_STALL: {reason}; fallback legado já utilizado"));
                    }
                    legacy_fallbacks += 1;
                    tracing::warn!(rota=%route.id,ponto=index,motivo=%reason,"NAV_LEGACY_FALLBACK: um trecho com o motor estável; destino continua obrigatório");
                    status("obstáculo persistente: um trecho com a navegação estável".into());
                    nav::prepare_route_retry(hwnd, status)?;
                    position(hwnd, t.world)?;
                    nav::route_legacy_leg(
                        hwnd,
                        t,
                        (point.x, point.y),
                        index + 1 == route.waypoints.len() && matches!(route.target, RouteTarget::Spot),
                        status,
                    )?;
                    at = position(hwnd, t.world)?;
                    if route.arrived(
                        index,
                        Spot {
                            world: t.world,
                            x: at.0,
                            y: at.1,
                        },
                    ) {
                        break;
                    }
                    return Err("NAV_ARRIVAL: trecho legado não confirmou o waypoint".into());
                }
                // O destino de interação nunca vira parede por ser um NPC.
                let blocked_step = cached_path.first().copied().unwrap_or(next);
                if blocked_step != (point.x, point.y) {
                    dynamic.record(blocked_step, started.elapsed().as_millis() as u64);
                    let dx = (blocked_step.0 - current.0).signum();
                    let dy = (blocked_step.1 - current.1).signum();
                    if dx != 0 && dy != 0 {
                        for p in [(current.0 + dx, current.1), (current.0, current.1 + dy)] {
                            if p != (point.x, point.y) {
                                dynamic.record(p, started.elapsed().as_millis() as u64);
                            }
                        }
                    }
                }
                path_dirty = true;
                tracing::warn!(rota=%route.id,ponto=index,bloqueio=?next,"NAV_STALL: ausência de progresso; replanejamento limitado");
                status(format!("rota: bloqueio em {next:?}; recalculando segmento"));
            }
            at = current;
        }
        drop(held);
        let final_pos = position(hwnd, t.world)?;
        if !route.arrived(
            index,
            Spot {
                world: t.world,
                x: final_pos.0,
                y: final_pos.1,
            },
        ) {
            return Err("NAV_ARRIVAL: destino não confirmado".into());
        }
        arrival=Some(Spot{world:t.world,x:final_pos.0,y:final_pos.1});
    }
    arrival.ok_or_else(||"NAV_ARRIVAL: nenhum waypoint confirmado".into())
}
fn npc_snapshot(hwnd: isize, name: &str, world: u8, sequence: u64) -> Result<RadarSnapshot, String> {
    let (x, y) = position(hwnd, world)?;
    let mut s = crate::navigation::snapshot(name).ok_or("NAV_CONTEXT: observação indisponível")?;
    s.context.sequence = sequence;
    s.context.age_ms = 0;
    s.context.world = world;
    s.context.map_confirmed = true;
    s.position = Spot { world, x, y };
    s.entities = objects::nearby_hwnd(hwnd, name, (x, y));
    Ok(s)
}
pub fn interact(
    hwnd: isize,
    name: &str,
    npc: &NpcTarget,
    t: &Terrain,
    route: &RouteDefinition,
    status: &dyn Fn(String),
) -> Result<(), String> {
    let mut corrected = false;
    let mut last = String::new();
    // As mesmas alturas de clique já utilizadas pelo Trainer legado. Cada tentativa
    // exige parada real, mapa correto e duas novas confirmações do NPC.
    for (attempt, lift) in [40, 20, 60].into_iter().enumerate() {
        crate::navigation::check()?;
        nav::route_settle(hwnd)?;
        let actual = position(hwnd, t.world)?;
        if !route.arrived(
            route.waypoints.len() - 1,
            Spot {
                world: t.world,
                x: actual.0,
                y: actual.1,
            },
        ) {
            if corrected {
                return Err("NAV_NPC_MOVED: posição de interação mudou novamente; sem repetir a rota".into());
            }
            corrected = true;
            let mut leg = route.clone();
            leg.waypoints = vec![npc.interaction];
            status("NPC: clique deslocou o personagem; corrigindo apenas a aproximação final uma vez".into());
            walk_points(hwnd, t, &leg, status)?;
            nav::route_settle(hwnd)?;
            let at = position(hwnd, t.world)?;
            if !route.arrived(
                route.waypoints.len() - 1,
                Spot {
                    world: t.world,
                    x: at.0,
                    y: at.1,
                },
            ) {
                return Err("NAV_NPC_RANGE: posição final não estabilizou".into());
            }
        }
        let first = npc_snapshot(hwnd, name, npc.location.world, 1)?;
        let mut tracker = zen_core::npc_tracker::NpcTracker::default();
        tracker.observe(first);
        std::thread::sleep(Duration::from_millis(150));
        let second = npc_snapshot(hwnd, name, npc.location.world, 2)?;
        let found = tracker.confirmed(&second, npc)?;
        let mut target = npc.clone();
        target.location.x = found.x;
        target.location.y = found.y;
        if !second.position.near(target.location, target.radius as i32) {
            return Err("NAV_NPC_RANGE: fora da área de interação".into());
        }
        tracing::info!(npc=%npc.name,classe=npc.class,origem=?second.position,tentativa=attempt+1,lift,"NAV_NPC: observação dupla e posição estável confirmadas");
        match nav::route_npc_click(hwnd, (second.position.x, second.position.y), &target, lift) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
        let current = position(hwnd, t.world)?;
        if !corrected
            && (current.0 - npc.interaction.x)
                .abs()
                .max((current.1 - npc.interaction.y).abs())
                > 1
        {
            corrected = true;
            status("NPC: janela não abriu deste lado; uma aproximação até a posição de interação cadastrada".into());
            nav::route_npc_adjust(hwnd, t, (npc.interaction.x, npc.interaction.y), status)?;
            nav::route_settle(hwnd)?;
            position(hwnd, t.world)?;
        }
    }
    Err(format!(
        "NAV_NPC: janela não confirmada em três tentativas controladas: {last}"
    ))
}
fn execute_inner(hwnd: isize, name: &str, route: &RouteDefinition, status: &dyn Fn(String)) -> Result<String, String> {
    let _session = input::Session::begin(hwnd)?;
    let from = position(hwnd, route.world)?;
    let terrain = nav::world_terrain_of(hwnd, route.world)?;
    zen_core::navigation::plan(
        &terrain,
        Spot {
            world: route.world,
            x: from.0,
            y: from.1,
        },
        route,
    )?;
    nav::prepare_route_retry(hwnd, status)?;
    if nav::inventory_open(hwnd) {
        input::run(hwnd, &[zen_core::actions::Step::Key(0x56)])?;
    }
    if viewport::current(hwnd).map(|v| v.home()) != Some((515, 400)) {
        return Err("NAV_UI: feche os painéis que alteram a área de jogo".into());
    }
    let current = position(hwnd, route.world)?;
    let stationary = route.waypoints.len() == 1
        && route.arrived(
            0,
            Spot {
                world: route.world,
                x: current.0,
                y: current.1,
            },
        );
    if !terrain.is_safezone(current.0, current.1) && !stationary {
        nav::set_helper(hwnd, false)?;
    }
    let arrival=walk_points(hwnd, &terrain, route, status)?;
    super::spots::check_approach_at(hwnd, route.world, (arrival.x,arrival.y), true)?;
    tracing::info!(rota=%route.id,world=arrival.world,x=arrival.x,y=arrival.y,"NAV_ARRIVAL: coordenada real comprovada antes de ativar o Helper/interagir");
    match &route.target {
        RouteTarget::Npc { npc } => interact(hwnd, name, npc, &terrain, route, status)?,
        _ => {
            let at = position(hwnd, route.world)?;
            if !terrain.is_safezone(at.0, at.1) {
                nav::set_helper(hwnd, true)?;
            }
        }
    }
    let actual = position(hwnd, route.world)?;
    if matches!(route.target,RouteTarget::Npc{..})&&!route.arrived(
        route.waypoints.len() - 1,
        Spot {
            world: route.world,
            x: actual.0,
            y: actual.1,
        },
    ) {
        return Err("NAV_FINAL: posição mudou após a ação; chegada não confirmada".into());
    }
    crate::navigation::check()?;
    Ok(format!(
        "rota {} revisão {} concluída; chegada confirmada em ({}, {})",
        route.name, route.revision,arrival.x,arrival.y
    ))
}
pub fn execute(hwnd: isize, name: &str, route: &RouteDefinition, status: &dyn Fn(String)) -> Result<String, String> {
    let was_on = nav::helper_state(hwnd) == Some(true);
    let result = if matches!(route.target, RouteTarget::Spot) {
        super::spots::route_guard(name, route.goal(), || execute_inner(hwnd, name, route, status))?
    } else {
        execute_inner(hwnd, name, route, status)
    };
    if result.is_err() && was_on && !input::manual_mode() && crate::navigation::check().is_ok() {
        // Falha de percurso não deve deixar um personagem antes protegido sem defesa.
        if let Ok(_session) = input::Session::begin(hwnd) {
            let _ = nav::set_helper(hwnd, true);
        }
    }
    result
}
