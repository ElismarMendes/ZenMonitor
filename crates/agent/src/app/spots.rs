use super::*;

/// Um spot, um dono; char travado não troca de spot.
pub(super) fn can_assign(cfg: &AgentConfig, name: &str, spot: [i32; 2]) -> Result<(), String> {
    if cfg.locked_spots.iter().any(|n| n == name) && cfg.spots.get(name).is_some_and(|s| !same_spot(*s, spot)) {
        return Err(format!("{name} está travado no spot dele: destrave no painel antes"));
    }
    if let Some((other, _)) = cfg.spots.iter().find(|(n, s)| n.as_str() != name && home_dest(cfg, n) == zen_agent::win::nav::Dest::Kanturu && same_spot(**s, spot)) {
        return Err(format!("o spot ({}, {}) já é do {other}: troque ele de spot antes", spot[0], spot[1]));
    }
    Ok(())
}

/// Tiles a até 2 de distância contam como o mesmo spot.
pub(super) fn same_spot(a: [i32; 2], b: [i32; 2]) -> bool {
    (a[0] - b[0]).abs().max((a[1] - b[1]).abs()) <= 2
}

/// Põe o tile no fim do catálogo (se ainda não estiver). Devolve (posição 1..N, é novo).
pub(super) fn catalog_add(cfg: &mut AgentConfig, p: [i32; 2]) -> (usize, bool, [i32; 2]) {
    if let Some(i) = cfg.catalog.iter().position(|c| same_spot(*c, p)) {
        cfg.catalog[i] = p;
        return (i + 1, false, p);
    }
    cfg.catalog.push(p);
    (cfg.catalog.len(), true, p)
}

pub(super) fn effective_spot(cfg: &AgentConfig,name:&str)->Option<[i32;2]> {
    zen_agent::spot_service::effective(cfg,name).map(|s|[s.x,s.y])
}

/// Mapa do spot do char (Kanturu se não houver outro marcado).
pub(super) fn home_dest(cfg: &AgentConfig, name: &str) -> zen_agent::win::nav::Dest {
    use zen_agent::win::nav::Dest;
    if cfg.arena.iter().any(|n| n == name) {
        return Dest::Arena;
    }
    cfg.spot_maps.get(name).and_then(|m| Dest::from_name(m)).unwrap_or(Dest::Kanturu)
}

pub(super) fn spot_dest(cfg: &AgentConfig,name:&str)->zen_agent::win::nav::Dest {
    zen_agent::spot_service::effective(cfg,name).and_then(|s|zen_agent::win::nav::Dest::from_world(s.world))
        .unwrap_or_else(||home_dest(cfg,name))
}

pub(super) static SPOT_EDIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn sync_saved_spots(cfg: &mut AgentConfig, rx: &std::sync::mpsc::Receiver<()>) {
    if rx.try_iter().count() == 0 { return; }
    match AgentConfig::load_file() {
        Ok(saved) => {
            cfg.catalog = saved.catalog;
            cfg.catalog_maps = saved.catalog_maps;
            cfg.spots = saved.spots;
            cfg.arena = saved.arena;
            cfg.spot_maps = saved.spot_maps;
        }
        Err(e) => tracing::error!(erro = %e, "não sincronizei os spots salvos"),
    }
}

pub(super) fn spot_result_ok(message: &str) -> bool {
    !["ignorado", "falha", "falhou", "travado", "já é do", "sem spots", "cidade", "destrave"]
        .iter().any(|s| message.contains(s))
}

pub(super) fn save_spots(cfg: &AgentConfig) -> anyhow::Result<()> {
    let (cat, spots, arena, sm) = (cfg.catalog.clone(), cfg.spots.clone(), cfg.arena.clone(), cfg.spot_maps.clone());
    save_field(move |c| {
        c.catalog = cat;
        c.spots = spots;
        c.arena = arena;
        c.spot_maps = sm;
    })
}

/// F2 / F1 apertados no jogo.
/// Spot fora do Kanturu (Arena, Lost Tower, Dungeon, Devias, ...): um dono por spot no mesmo
/// mapa; respeita a trava do char. Lorencia não (cidade).
pub(super) fn save_spot_other(cfg: &mut AgentConfig, name: &str, map: &str, p: [i32; 2]) -> Result<String, String> {
    if map == "Lorencia" {
        return Err("Lorencia é cidade: spot só nos mapas de farm".into());
    }
    if cfg.locked_spots.iter().any(|n| n == name) && cfg.spots.get(name).is_some_and(|s| !same_spot(*s, p)) {
        return Err(format!("{name} está travado no spot dele: destrave no painel antes"));
    }
    let same_map = |n: &String| if map == "Arena" { cfg.arena.contains(n) } else { cfg.spot_maps.get(n).map(String::as_str) == Some(map) };
    if let Some((other, _)) = cfg.spots.iter().find(|(n, s)| n.as_str() != name && same_map(n) && same_spot(**s, p)) {
        return Err(format!("esse spot da {map} já é do {other}"));
    }
    catalog_here(cfg,map,p)?;
    cfg.spots.insert(name.to_string(), p);
    cfg.arena.retain(|n| n != name);
    cfg.spot_maps.remove(name);
    if map == "Arena" {
        cfg.arena.push(name.to_string());
    } else {
        cfg.spot_maps.insert(name.to_string(), map.to_string());
    }
    let (spots, arena, sm, cm) = (cfg.spots.clone(), cfg.arena.clone(), cfg.spot_maps.clone(),cfg.catalog_maps.clone());
    save_field(move |c| {
        c.catalog_maps = cm;
        c.spots = spots;
        c.arena = arena;
        c.spot_maps = sm;
    })
    .map_err(|e| format!("falha ao salvar: {e}"))?;
    tracing::info!(char = %name, mapa = %map, spot = ?p, "spot salvo");
    Ok(format!("volta para o spot na {map} ({}, {})", p[0], p[1]))
}

/// Mapa e posição lidos na tela no instante do pedido (atalho ou painel).
pub(super) type Snap = (Option<&'static str>, Option<(i32, i32)>);

pub(super) fn catalog_here(cfg:&mut AgentConfig,map:&str,p:[i32;2])->Result<(usize,bool),String> {
    if map=="Lorencia" {return Err("Lorencia é cidade: sem spots de farm".into())}
    if map=="Kanturu" {
        let (id,new,_)=catalog_add(cfg,p);return Ok((id,new))
    }
    let list=cfg.catalog_maps.entry(map.into()).or_default();
    if let Some(i)=list.iter().position(|s|same_spot(*s,p)) {list[i]=p;Ok((i+1,false))}
    else {list.push(p);Ok((list.len(),true))}
}

pub(super) fn handle_hotkey(h: zen_agent::win::hotkey::Hotkey, hwnd: isize, snap: Snap, cfg: &mut AgentConfig, access: &WinAccess) -> String {
    let Some(name)=access.list_windows().into_iter().find(|w|w.hwnd==hwnd)
        .and_then(|w|zen_core::title::parse(&w.title).name) else {return "atalho ignorado: janela sem char".into()};
    if h==zen_agent::win::hotkey::Hotkey::AssignSpot {
        return match save_spot_snapshot(0,name.clone(),Some(snap),cfg).into_iter().next() {
            Some(AgentMsg::CommandResult{ok,message,..})=>format!("F1 ({name}): {}{message}",if ok{""}else{"falhou: "}),
            _=>"F1 falhou: resultado ausente".into()
        };
    }
    let (Some(map),Some((x,y)))=snap else {return "F2 ignorado: mapa/coordenada ilegíveis no evento".into()};
    match catalog_here(cfg,map,[x,y]) {
        Ok((id,_))=>{
            let (catalog,catalog_maps)=(cfg.catalog.clone(),cfg.catalog_maps.clone());
            match save_field(move|c|{c.catalog=catalog;c.catalog_maps=catalog_maps;}) {
                Ok(())=>format!("F2 ({name}): catálogo {map}, spot {id} em ({x}, {y}); retorno preservado"),
                Err(e)=>format!("F2 falhou ao gravar: {e}")
            }
        }
        Err(e)=>format!("F2 falhou: {e}")
    }
}

pub(super) fn save_spot_snapshot(id:u64,char_name:String,snap:Option<Snap>,cfg:&mut AgentConfig)->Vec<AgentMsg> {
    let result=(||->Result<String,String>{
        let (Some(map),Some((x,y)))=snap.unwrap_or((None,None)) else {return Err("mapa/coordenada ilegíveis no evento; não reli uma posição posterior".into())};
        let dest=zen_agent::win::nav::Dest::from_name(map).filter(|d|*d!=zen_agent::win::nav::Dest::Lorencia).ok_or("mapa sem spot de farm")?;
        let spot=zen_core::spots::Spot{world:dest.world(),x,y};
        if !spot.valid(){return Err("coordenadas inválidas".into())}
        if cfg.locked_spots.iter().any(|n|n==&char_name) &&
            zen_agent::spot_service::home(cfg,&char_name).is_some_and(|s|s!=spot) {
            return Err("char travado: destrave o spot antes de alterá-lo".into());
        }
        zen_agent::spot_service::request(&char_name,spot,false)?;
        let saved=(||->Result<String,String>{
            if map=="Kanturu" {
                can_assign(cfg,&char_name,[x,y])?;
                let (id,_,_)=catalog_add(cfg,[x,y]);
                cfg.spots.insert(char_name.clone(),[x,y]);cfg.arena.retain(|n|n!=&char_name);cfg.spot_maps.remove(&char_name);
                save_spots(cfg).map_err(|e|e.to_string())?;
                Ok(format!("{char_name}: spot principal {id} em {map} ({x}, {y}) salvo e catalogado"))
            }else{
                save_spot_other(cfg,&char_name,map,[x,y])
            }
        })();
        if saved.is_err(){let _=zen_agent::spot_service::request(&char_name,spot,true);}
        else {zen_agent::spot_service::clear_active(&char_name)?;}
        saved
    })();
    vec![match result {
        Ok(message)=>AgentMsg::CommandResult{id,ok:true,message},
        Err(message)=>AgentMsg::CommandResult{id,ok:false,message},
    }]
}
