use crate::win::nav::*;

/// Vai até `spot` no mapa `dest` (M se precisar, andando pelo terreno `t`) e liga o helper se
/// não for zona segura. Base da volta ao spot e do "ir até" do painel.
pub fn recover_in(hwnd: isize, t: &Terrain, spot: (i32, i32), dest: Dest, status: &dyn Fn(String)) -> Result<String, String> {
    if t.world!=dest.world() {return Err("terreno não corresponde ao mapa de destino".into());}
    let _session = input::Session::begin(hwnd)?;
    let t0 = std::time::Instant::now();
    let mut log = Vec::new();
    // Baú esquecido aberto (ou item preso no cursor) segura o char parado: fecha antes de tudo.
    if force_close_storage(hwnd) {
        log.push("baú estava aberto: fechado com ESC".to_string());
    }
    if close_npc_dialog(hwnd) {
        log.push("janela de NPC estava aberta: fechada com ESC".to_string());
    }
    let (k, pos) = in_dest(hwnd, dest);
    let pos = pos.ok_or("não li as coordenadas na tela")?;
    if k == Some(false) {
        // Em Lorencia sem o Zen do move: busca no baú antes (regra do dono, 09/11 14:17).
        if dest != Dest::Lorencia && in_lorencia(hwnd) {
            let lor = world_terrain_of(hwnd, LORENCIA_WORLD)?;
            ensure_move_money(hwnd, &lor, false, status)?;
        }
        let gate = best_gate(dest, Some(t), spot);
        status(format!("teleportando: M -> {} (chegada {:?})", dest.name(), gate.arrival));
        let p = teleport_gate(hwnd, dest, gate)?;
        log.push(format!("teleportou para {p:?}"));
    } else if dest.gates().len() > 1 && t.path(pos, spot).is_none() {
        // Mesmo mapa, outro andar sem caminho a pé (Lost Tower/Dungeon): vai pelo M até a entrada certa.
        let gate = best_gate(dest, Some(t), spot);
        status(format!("sem caminho a pé de {pos:?}: M -> {} (chegada {:?})", dest.name(), gate.arrival));
        let p = teleport_gate(hwnd, dest, gate)?;
        log.push(format!("outro andar: teleportou para {p:?}"));
    } else {
        log.push(format!("já no mapa do spot em {pos:?}"));
    }
    let walk_start = ensure_action_map(hwnd, dest)?;
    if !t.is_safezone(walk_start.0, walk_start.1) {
        status("desligando o helper para andar dentro do mapa".into());
        set_helper(hwnd, false)?;
    }
    // Saindo do baú: janela do baú/inventário aberta ou o Safety Guardian no caminho seguravam
    // o char em (33,216) (visto em 09/11 03:49: 25 tentativas). Fecha janelas e sai primeiro
    // para o oeste, longe do NPC, e só então segue para o spot.
    if let Some(p) = position(hwnd).filter(|_| dest == Dest::Kanturu) {
        if cheb(p, VAULT_TILE) <= 4 {
            close_storage(hwnd);
            status("saindo de perto do baú".into());
            let _ = walk(hwnd, t, VAULT_EXIT, status);
        }
    }
    status(format!("andando até o spot {spot:?}"));
    let mut walked = walk_with_arrival(hwnd, t, spot, WalkArrival::Spot, status);
    if let Err(e) = &walked { if e.starts_with("SPOT_") { return Err(e.clone()); } }
    // Caminhada longa que parou no meio (limite de 4 min, janela que perdeu o foco): AMUMU em
    // 15/11 parou em (112,44) e (68,63) e só chegou na 3ª recuperação. Continua de onde parou,
    // no mesmo mapa, até 2 vezes, em vez de encerrar a volta.
    for _ in 0..2 {
        let Err(e) = &walked else { break };
        let Some(p) = position(hwnd) else { break };
        if cheb(p, spot) <= 8 { break; }
        let e = e.clone();
        confirm_destination(hwnd, dest, None)?;
        tracing::warn!(hwnd, posicao = ?p, erro = %e, "volta ao spot: caminhada parou longe; continuando de onde parou");
        status(format!("caminhada parou em {p:?} ({e}); continuando até o spot"));
        log.push(format!("caminhada parou em {p:?}: {e}"));
        walked = walk_with_arrival(hwnd, t, spot, WalkArrival::Spot, status);
        if let Err(e) = &walked { if e.starts_with("SPOT_") { return Err(e.clone()); } }
    }
    // Perto do spot o clique às vezes cai num mob: o char fica dando ataque normal num mob
    // que não morre e pode morrer (dono, 09/11 15:43). Então, travado perto: liga o helper
    // alguns segundos para limpar, desliga e ajusta a posição de novo (até 2 vezes).
    for round in 0..2 {
        let Err(e) = &walked else { break };
        confirm_destination(hwnd,dest,None)?;
        let Some(p) = position(hwnd) else { break };
        // Já na margem: não limpa nem repete a caminhada; confirma e liga o Helper.
        if cheb(p, spot) <= SPOT_ARRIVAL_TOLERANCE { break; }
        if cheb(p, spot) > 8 || t.is_safezone(p.0, p.1) {
            break;
        }
        if !clear_mobs(hwnd, t, 4000, None, status) {
            status(format!("travado em {p:?} perto do spot ({e}): helper ligado 8 s para limpar os mobs ({})", round + 1));
            if set_helper(hwnd, true).is_err() {
                break;
            }
            sleep(8000);
            let _ = set_helper(hwnd, false);
        }
        walked = walk_with_arrival(hwnd, t, spot, WalkArrival::Spot, status);
        if let Err(e) = &walked { if e.starts_with("SPOT_") { return Err(e.clone()); } }
    }
    // O erro real da caminhada vai junto (antes só aparecia "destino não confirmado").
    let arrival = confirm_destination(hwnd, dest, Some((spot, SPOT_ARRIVAL_TOLERANCE)))
        .map_err(|c| match &walked { Err(w) => format!("{c} (caminhada: {w})"), Ok(_) => c })?;
    // Ocupação relida uma vez no destino confirmado, antes de habilitar o Helper.
    crate::win::spots::check_approach_at(hwnd, dest.world(), arrival, true)?;
    let safe = t.is_safezone(arrival.0, arrival.1);
    if !safe {
        status("ligando o helper no spot".into());
        // O Helper assume a limpeza: sem novas caminhadas ou rodadas de skill na margem.
        set_helper(hwnd, true)?;
    }
    if let Err(e) = walked {
        log.push(format!("chegada recuperada dentro da margem: {e}"));
    }
    log.push(format!("chegou em {arrival:?}, a {} tile(s) do spot, em {:?}{}", cheb(arrival, spot), t0.elapsed(), if safe { "; zona segura: helper não ligado" } else { "; helper ligado" }));
    Ok(log.join("; "))
}

// ---------------------------------------------------------------- baú (Safety Guardian)
// Posições medidas na gravação de 08/11 (depósito feito à mão), em coordenadas da janela
// inteira 1030x797, como o resto deste arquivo.
