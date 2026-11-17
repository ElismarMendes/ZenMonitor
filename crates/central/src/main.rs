//! zen-central.exe — roda no PC principal. Recebe os agentes por WebSocket, decide os
//! alertas, guarda o histórico em SQLite e serve o painel em http://<ip>:8787/.

mod config;
mod db;
mod hub;
mod notify;
mod ozt;
mod tokens;
mod update;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, Mutex};
use zen_core::alerts::AlertConfig;
use zen_core::protocol::{version_accepted, AgentMsg, CentralMsg, Command, PanelChar, PROTOCOL_VERSION};

const INDEX_HTML: &str = include_str!("../web/index.html");
/// Painel novo (v4, Etapa 2). Fica em /v4 ao lado do antigo até ser aprovado.
const V4_HTML: &str = include_str!("../web/v4.html");

struct App {
    cfg: config::CentralConfig,
    hub: Mutex<hub::Hub>,
    spot_book: Mutex<zen_core::spots::Directory>,
    pool: sqlx::SqlitePool,
    /// Conexão aberta de cada máquina, para mandar comandos.
    links: Mutex<HashMap<String, mpsc::UnboundedSender<CentralMsg>>>,
    panel_tx: broadcast::Sender<String>,
    next_cmd: Mutex<u64>,
    /// Últimos resultados de comando e prints (memória; não precisam sobreviver a reinício).
    results: Mutex<Vec<Value>>,
    screenshots: Mutex<HashMap<String, Vec<u8>>>,
    ranking: Mutex<Value>,
    http: reqwest::Client,
    ips: Mutex<HashMap<String, String>>,
    /// Tokens dos agentes (fonte da verdade em memória; gravada no central.toml a cada mudança).
    store: Mutex<tokens::TokenStore>,
    /// Interruptor da automação de token (pareamento + renovação).
    auto_pairing: AtomicBool,
    /// Cópia do central.toml usada só para gravar.
    cfg_file: Mutex<config::CentralConfig>,
    /// Últimas mortes (sem a imagem; o print fica no banco e sai por /api/death/<id>).
    deaths: Mutex<Vec<Value>>,
}

type Shared = Arc<App>;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    config::CentralConfig::load_env_file();
    // Subcomandos (chave-nova, publicar-agente): rodam e saem, sem subir o servidor.
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = update::cli(&args) {
        std::process::exit(code);
    }
    let dir = config::CentralConfig::dir();
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("central")
        .filename_suffix("log")
        .max_log_files(14)
        .build(dir.join("logs"))?;
    let (file, _guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(file)
        .with_ansi(false)
        .init();

    let cfg = config::CentralConfig::load_or_create()?;
    let pool = db::open(&dir.join(&cfg.db_path).to_string_lossy()).await?;
    let app = build_app(cfg, pool)?;
    tracing::info!(
        auto_pairing = app.auto_pairing.load(Ordering::Relaxed),
        maquinas = app.store.lock().await.entries.len(),
        "configuração carregada"
    );

    refresh_deaths(&app).await;
    tokio::spawn(ticker(app.clone()));
    tokio::spawn(slow_jobs(app.clone()));
    let routes = router(app.clone());

    let addr: SocketAddr = app.cfg.bind.parse()?;
    tracing::info!(%addr, "central no ar");
    println!("Painel: http://{addr}/");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, routes.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

fn build_app(cfg: config::CentralConfig, pool: sqlx::SqlitePool) -> anyhow::Result<Shared> {
    let alert_cfg = AlertConfig {
        stopped_after_s: cfg.stopped_after_s,
        ..Default::default()
    };
    let (panel_tx, _) = broadcast::channel(16);
    Ok(Arc::new(App {
        hub: Mutex::new(hub::Hub::new(alert_cfg)),
        spot_book: Mutex::new(load_spot_book()?),
        pool,
        links: Mutex::new(HashMap::new()),
        panel_tx,
        next_cmd: Mutex::new(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64),
        results: Mutex::new(Vec::new()),
        screenshots: Mutex::new(HashMap::new()),
        ranking: Mutex::new(json!({})),
        http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?,
        ips: Mutex::new(HashMap::new()),
        store: Mutex::new(cfg.token_store()),
        auto_pairing: AtomicBool::new(cfg.auto_pairing),
        cfg_file: Mutex::new(cfg.clone()),
        deaths: Mutex::new(Vec::new()),
        cfg,
    }))
}

fn load_spot_book() -> anyhow::Result<zen_core::spots::Directory> {
    let path = config::CentralConfig::dir().join("spot-directory.json");
    if !path.exists() { return Ok(Default::default()); }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn save_spot_book(book: &zen_core::spots::Directory) -> anyhow::Result<()> {
    let path = config::CentralConfig::dir().join("spot-directory.json");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp,serde_json::to_vec(book)?)?;
    std::fs::rename(tmp,path)?;
    Ok(())
}

async fn broadcast_spots(app: &Shared) {
    let directory = app.spot_book.lock().await.clone();
    let capable:Vec<_>=app.hub.lock().await.machines.iter().filter(|(_,m)|m.protocol>=2).map(|(name,_)|name.clone()).collect();
    for (name,link) in app.links.lock().await.iter() {
        if capable.contains(name) {let _ = link.send(CentralMsg::SpotDirectory { directory: directory.clone() });}
    }
}

fn router(app: Shared) -> Router {
    Router::new()
        // Painel antigo descontinuado (dono, 12/11): "/" abre o painel novo.
        .route("/", get(|| async { Html(V4_HTML) }))
        .route("/antigo", get(|| async { Html(INDEX_HTML) }))
        .route("/api/panels", get(api_panels))
        .route("/v4", get(|| async { Html(V4_HTML) }))
        .route("/ws/agent", get(ws_agent))
        .route("/ws/pair", get(ws_pair))
        .route("/ws/panel", get(ws_panel))
        .route("/api/state", get(api_state))
        .route("/api/command", post(api_command))
        .route(
            "/api/auto_pairing",
            get(api_auto_pairing_get).post(api_auto_pairing_set),
        )
        .route("/api/machine/remove", post(api_machine_remove))
        .route("/api/screenshot/{machine}/{char_name}", get(api_screenshot))
        .route("/api/logs/{machine}", get(api_logs))
        .route("/api/death/{id}", get(api_death_png))
        .route("/api/terrain/kanturu", get(api_terrain_kanturu))
        .route("/api/terrain/{world}", get(api_terrain_world))
        .route("/api/maps", get(api_maps))
        .route("/api/minimap/{world}", get(api_minimap))
        .route("/api/mapicon/{name}", get(api_mapicon))
        .route("/api/points", get(api_points_get).post(api_points_add))
        .route("/api/points/{id}", axum::routing::delete(api_points_del))
        .route("/api/update/agent", get(api_update_meta))
        .route("/api/update/agent/bin", get(api_update_bin))
        .with_state(app)
}

/// Grava tokens e o interruptor no central.toml (atômico).
async fn save_tokens(app: &Shared) {
    let store = app.store.lock().await.clone();
    let mut c = app.cfg_file.lock().await;
    c.set_from_store(&store);
    c.auto_pairing = app.auto_pairing.load(Ordering::Relaxed);
    if let Err(e) = c.save() {
        tracing::error!("não consegui gravar o central.toml: {e:#}");
    }
}

fn bearer(headers: &HeaderMap) -> &str {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
}

// ---------------------------------------------------------------- agentes

async fn ws_agent(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(app): State<Shared>,
) -> Response {
    let token = bearer(&headers).to_string();
    let auto = app.auto_pairing.load(Ordering::Relaxed);
    let max_age = app.cfg.token_max_age_days.max(1) * 86_400;
    let (auth, changed) = {
        let mut st = app.store.lock().await;
        let before = st.entries.clone();
        let auth = st.authenticate(&token, auto, now(), max_age, &mut || {
            config::random_token(tokens::TOKEN_LEN)
        });
        (auth, st.entries != before)
    };
    if changed {
        save_tokens(&app).await;
    }
    let Some(auth) = auth else {
        tracing::warn!(%peer, "agente com token inválido");
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if auth.send_token.is_some() {
        tracing::info!(machine = %auth.machine, "token renovado; enviando ao agente");
    }
    app.ips.lock().await.insert(auth.machine.clone(), peer.ip().to_string());
    ws.max_message_size(8 << 20)
        .on_upgrade(move |s| agent_session(s, app, auth.machine, auth.send_token))
}

async fn agent_session(socket: WebSocket, app: Shared, machine: String, new_token: Option<String>) {
    let (mut tx, mut rx) = socket.split();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<CentralMsg>();
    let mut said_hello = false;
    // true quando o painel removeu esta máquina: a sessão fecha e o PC some do painel.
    let mut removed = false;

    loop {
        tokio::select! {
            Some(out) = cmd_rx.recv() => {
                let reject = matches!(out, CentralMsg::Reject { .. });
                let Ok(txt) = serde_json::to_string(&out) else { continue };
                if tx.send(Message::Text(txt.into())).await.is_err() { break; }
                if reject {
                    removed = true;
                    break;
                }
            }
            msg = rx.next() => {
                let Some(Ok(msg)) = msg else { break };
                let Message::Text(txt) = msg else { continue };
                let parsed: AgentMsg = match serde_json::from_str(txt.as_str()) {
                    Ok(m) => m,
                    Err(e) => { tracing::warn!(%machine, "mensagem inválida: {e}"); continue; }
                };
                match parsed {
                    AgentMsg::Hello { protocol, agent_version, channel, .. } => {
                        if !version_accepted(protocol) {
                            let rej = CentralMsg::Reject { reason: format!("protocolo {protocol} não suportado (central usa {PROTOCOL_VERSION})") };
                            let _ = tx.send(Message::Text(serde_json::to_string(&rej).unwrap_or_default().into())).await;
                            break;
                        }
                        said_hello = true;
                        {
                            let mut hub=app.hub.lock().await;
                            hub.on_hello(&machine, &agent_version, &channel);
                            if let Some(m)=hub.machines.get_mut(&machine){m.protocol=protocol;}
                        }
                        app.links.lock().await.insert(machine.clone(), cmd_tx.clone());
                        let w = CentralMsg::Welcome { protocol: PROTOCOL_VERSION, server_time: now() };
                        let _ = tx.send(Message::Text(serde_json::to_string(&w).unwrap_or_default().into())).await;
                        if let Some(t) = &new_token {
                            let m = CentralMsg::NewToken { token: t.clone() };
                            let _ = tx.send(Message::Text(serde_json::to_string(&m).unwrap_or_default().into())).await;
                        }
                        tracing::info!(%machine, %agent_version, "agente conectado");
                    }
                    _ if !said_hello => { tracing::warn!(%machine, "mensagem antes do hello"); break; }
                    AgentMsg::Pair { .. } => { tracing::warn!(%machine, "pareamento pela rota errada"); break; }
                    AgentMsg::SpotLease { request_id, char_name, spot, release } => {
                        let mut book = app.spot_book.lock().await;
                        let before = book.clone();
                        let result = if release { book.release(&machine,&char_name,spot); Ok(()) }
                            else { book.reserve(&machine,&char_name,spot,now()) };
                        let result = result.and_then(|_|save_spot_book(&book).map_err(|e|format!("persistência: {e}")));
                        if result.is_err() { *book = before; }
                        let accepted = result.is_ok();
                        let reason = result.err().unwrap_or_else(||"reserva confirmada".into());
                        drop(book);
                        let _ = cmd_tx.send(CentralMsg::SpotLeaseResult { request_id, accepted, reason });
                        broadcast_spots(&app).await;
                    }
                    AgentMsg::Report { sent_at, chars, events, health, catalog, catalog_other, .. } => {
                        {
                            use zen_core::spots::{Spot,Member,CatalogSpot,world_for_name};
                            let mut book = app.spot_book.lock().await;
                            let before = book.clone();
                            for c in &chars {
                                let Some(name) = c.title.name.clone() else { continue };
                                let active = c.spot.zip(c.spot_map.as_deref().and_then(world_for_name))
                                    .map(|((x,y),world)|Spot{world,x,y});
                                book.register(Member{machine:machine.clone(),name,home:c.home_spot.or(active),active});
                            }
                            let mut entries=Vec::new();
                            for (priority,(x,y)) in catalog.iter().enumerate() {
                                entries.push(CatalogSpot{spot:Spot{world:38,x:*x,y:*y},priority:priority as u32});
                            }
                            let mut priorities = std::collections::HashMap::<u8,u32>::new();
                            for (world,x,y) in &catalog_other {
                                let priority = priorities.entry(*world).or_default();
                                entries.push(CatalogSpot{spot:Spot{world:*world,x:*x,y:*y},priority:*priority});
                                *priority += 1;
                            }
                            book.set_catalog(&machine,entries);
                            if *book != before {
                                if let Err(e) = save_spot_book(&book) { tracing::error!(erro = %e,"não persisti diretório de spots"); }
                            }
                        }
                        let outs = {
                            let mut h = app.hub.lock().await;
                            let o = h.on_report(&machine, now(), sent_at, chars, events, health, catalog);
                            if let Some(m) = h.machines.get_mut(&machine) {
                                m.catalog_other = catalog_other;
                            }
                            o
                        };
                        persist(&app, outs).await;
                        broadcast_spots(&app).await;
                    }
                    AgentMsg::CommandResult { id, ok, message } => {
                        // id 0 = o agente fez sozinho (depósito, morte, atualização...). Falha ou
                        // depósito vão para o celular; o resto só aparece no painel.
                        if id == 0 && (!ok || message.contains("depósito")) {
                            let app2 = app.clone();
                            let text = format!("{machine}: {}{message}", if ok { "" } else { "FALHA — " });
                            tokio::spawn(async move { notify::send(&app2.http, &app2.cfg, &text).await });
                        }
                        push_result(&app, json!({"id": id, "machine": machine, "ok": ok, "message": message, "ts": now()})).await;
                    }
                    AgentMsg::Screenshot { id, char_name, png_base64 } => {
                        use base64::Engine;
                        if let Ok(png) = base64::engine::general_purpose::STANDARD.decode(png_base64) {
                            app.screenshots.lock().await.insert(format!("{machine}/{char_name}"), png);
                            push_result(&app, json!({"id": id, "machine": machine, "ok": true,
                                "message": format!("print de {char_name} pronto"),
                                "screenshot": format!("/api/screenshot/{machine}/{char_name}"), "ts": now()})).await;
                        }
                    }
                    AgentMsg::Death { char_name, at, hp_before, screen_hp, png_base64 } => {
                        use base64::Engine;
                        let png = png_base64.and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok());
                        let char_name = zen_core::sanitize(&char_name, 32);
                        let t = now();
                        let ts = {
                            let mut h = app.hub.lock().await;
                            let ts = h.central_ts(&machine, at, t);
                            h.on_death(&machine, &char_name, ts);
                            ts
                        };
                        tracing::warn!(%machine, char = %char_name, hp_antes = hp_before, tela = ?screen_hp, com_print = png.is_some(), "MORTE");
                        if let Err(e) = db::insert_death(&app.pool, ts, &machine, &char_name, hp_before, screen_hp, png.as_deref()).await {
                            tracing::error!("gravar morte: {e:#}");
                        }
                        refresh_deaths(&app).await;
                    }
                    AgentMsg::Logs { id, text } if id >= AUTO_LOG_ID => {
                        // Pedido automático (a cada 2 min): vira arquivo no PC principal.
                        let d = config::CentralConfig::dir().join("logs").join("agentes");
                        let _ = std::fs::create_dir_all(&d);
                        let _ = std::fs::write(d.join(format!("{machine}.log")), text);
                    }
                    AgentMsg::Logs { id, text } => {
                        push_result(&app, json!({"id": id, "machine": machine, "ok": true, "message": "logs", "logs": text, "ts": now()})).await;
                    }
                }
            }
        }
    }
    // Só mexe no canal de comandos e no estado "conectado" se ESTA sessão for a ativa.
    // Sem isso, uma conexão velha caindo depois de uma reconexão apagava o canal da nova.
    let was_active = {
        let mut links = app.links.lock().await;
        let mine = links.get(&machine).is_some_and(|l| l.same_channel(&cmd_tx));
        if mine {
            links.remove(&machine);
        }
        mine
    };
    if was_active {
    }
    if removed {
        app.hub.lock().await.forget_machine(&machine);
        tracing::warn!(%machine, "sessão encerrada: máquina removida no painel");
    } else if was_active {
        app.hub.lock().await.on_disconnect(&machine);
        tracing::info!(%machine, "agente desconectado");
    }
}

/// Pareamento automático: agente sem token pede um. Só da rede local e só com
/// `auto_pairing` ligado. A conexão fecha logo após a resposta.
async fn ws_pair(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(app): State<Shared>,
) -> Response {
    ws.max_message_size(4096).on_upgrade(move |mut socket| async move {
        let first = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await;
        let Ok(Some(Ok(Message::Text(txt)))) = first else {
            return;
        };
        let reply = match serde_json::from_str::<AgentMsg>(txt.as_str()) {
            Ok(AgentMsg::Pair {
                protocol,
                machine,
                install_id,
            }) if version_accepted(protocol) => {
                let auto = app.auto_pairing.load(Ordering::Relaxed);
                let lan = tokens::is_lan(peer.ip());
                let r = app
                    .store
                    .lock()
                    .await
                    .pair(&machine, &install_id, auto, lan, now(), &mut || {
                        config::random_token(tokens::TOKEN_LEN)
                    });
                match r {
                    tokens::PairResult::Issued(token) | tokens::PairResult::Reissued(token) => {
                        save_tokens(&app).await;
                        tracing::info!(%machine, %peer, "máquina pareada automaticamente");
                        CentralMsg::Paired { machine, token }
                    }
                    tokens::PairResult::Refused(reason) => {
                        tracing::warn!(%machine, %peer, %reason, "pareamento recusado");
                        CentralMsg::Reject { reason }
                    }
                }
            }
            Ok(AgentMsg::Pair { protocol, .. }) => CentralMsg::Reject {
                reason: format!("protocolo {protocol} não suportado (central usa {PROTOCOL_VERSION})"),
            },
            _ => CentralMsg::Reject {
                reason: "esperava mensagem de pareamento".into(),
            },
        };
        let _ = socket
            .send(Message::Text(serde_json::to_string(&reply).unwrap_or_default().into()))
            .await;
        let _ = socket.send(Message::Close(None)).await;
    })
}

async fn persist(app: &Shared, outs: Vec<hub::HubOut>) {
    let mut zen = Vec::new();
    for o in outs {
        match o {
            hub::HubOut::ZenPoint {
                machine,
                char_name,
                ts,
                zen: z,
            } => zen.push((machine, char_name, ts, z)),
            hub::HubOut::Pk(r) => {
                let kind = if r.monster { "Monster" } else { "Player" };
                if let Err(e) = db::insert_pk(&app.pool, r.ts, &r.killer, kind, &r.victim, &r.machine).await {
                    tracing::error!("gravar PK: {e:#}");
                }
            }
        }
    }
    if !zen.is_empty() {
        if let Err(e) = db::insert_zen(&app.pool, &zen).await {
            tracing::error!("gravar Zen: {e:#}");
        }
    }
}

async fn refresh_deaths(app: &Shared) {
    match db::recent_deaths(&app.pool, 30).await {
        Ok(rows) => {
            *app.deaths.lock().await = rows
                .into_iter()
                .map(|d| {
                    json!({"id": d.id, "ts": d.ts, "machine": d.machine, "char": d.char_name,
                           "hp_before": d.hp_before, "screen_hp": d.screen_hp,
                           "png": d.has_png.then(|| format!("/api/death/{}", d.id))})
                })
                .collect()
        }
        Err(e) => tracing::error!("ler mortes: {e:#}"),
    }
}

static RESULT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

async fn push_result(app: &Shared, mut v: Value) {
    v["event_key"] = json!(format!("{}:{}",*BOOT,RESULT_SEQUENCE.fetch_add(1,Ordering::Relaxed)));
    let mut r = app.results.lock().await;
    r.insert(0, v);
    r.truncate(30);
}

// ---------------------------------------------------------------- painel

async fn build_state(app: &Shared) -> Value {
    let t = now();
    let (chars, machines, pks, catalog): (Vec<PanelChar>, Vec<Value>, Vec<Value>, Vec<Value>) = {
        let h = app.hub.lock().await;
        let ips = app.ips.lock().await;
        let machines = h
            .machines
            .iter()
            .map(|(name, m)| {
                json!({
                    "name": name, "version": m.version, "channel": m.channel, "connected": m.connected,
                    "ip": ips.get(name), "last_report_age_s": m.last_report_at.map(|r| t.saturating_sub(r)),
                    "cpu": m.health.cpu_percent, "mem_mb": m.health.mem_mb, "uptime_s": m.health.uptime_s,
                    "last_error": m.health.last_error, "ram_paused": m.health.ram_paused, "automation": m.health.automation, "manual": m.health.manual, "death_return": m.health.death_return,
                    "chars": m.chars.iter().map(|(n, c)| json!({"name": n, "lock": c.last.lock_status, "hp_lock": c.last.hp_status, "auto": c.last.auto_status, "tcp": c.last.tcp_connected})).collect::<Vec<_>>(),
                })
            })
            .collect();
        let pks = h
            .recent_pk
            .iter()
            .map(|p| json!({"ts": p.ts, "killer": p.killer, "monster": p.monster, "victim": p.victim}))
            .collect();
        let catalog: Vec<Value> = h
            .machines
            .iter()
            .flat_map(|(mn, m)| {
                let k = m.catalog.iter().enumerate().map(move |(i, (x, y))| json!({"machine": mn, "n": i + 1, "x": x, "y": y, "world": 38}));
                let mut seen: std::collections::HashMap<u8, usize> = Default::default();
                let o: Vec<Value> = m
                    .catalog_other
                    .iter()
                    .map(|(w, x, y)| {
                        let n = seen.entry(*w).or_insert(0);
                        *n += 1;
                        json!({"machine": mn, "n": *n, "x": x, "y": y, "world": w})
                    })
                    .collect();
                k.chain(o)
            })
            .collect();
        (h.snapshot(t), machines, pks, catalog)
    };
    let spot_directory=app.spot_book.lock().await.clone();
    json!({
        "spot_catalog": spot_directory.catalog.iter().map(|c|json!({"id":c.spot.id(),"world":c.spot.world,"x":c.spot.x,"y":c.spot.y,"priority":c.priority})).collect::<Vec<_>>(),
        "spot_directory": spot_directory,
        "now": t,
        "chars": chars,
        "machines": machines,
        "update": update::current_meta().map(|m| json!({"seq": m.seq, "published_at": m.published_at})),
        "update_test": update::test_meta().map(|m| json!({"seq": m.seq, "published_at": m.published_at})),
        "test_machines": app.cfg.test_machines.clone(),
        "pks": pks,
        "ranking": app.ranking.lock().await.clone(),
        "results": app.results.lock().await.clone(),
        "deaths": app.deaths.lock().await.clone(),
        "catalog": catalog,
        // Joias só entram quando o método exato for provado (fase 6).
        "jewels": Value::Null,
        "auto_pairing": app.auto_pairing.load(Ordering::Relaxed),
    })
}

async fn api_state(State(app): State<Shared>) -> Json<Value> {
    Json(build_state(&app).await)
}

/// Quantas abas do painel estão abertas agora (o zen.ps1 não abre outra se já houver uma).
async fn api_panels(State(app): State<Shared>) -> Json<Value> {
    Json(json!({"open": app.panel_tx.receiver_count(), "boot": *BOOT}))
}

/// Hora em que este central subiu: a aba aberta recarrega sozinha quando muda (painel novo).
static BOOT: std::sync::LazyLock<u64> = std::sync::LazyLock::new(now);

async fn ws_panel(ws: WebSocketUpgrade, State(app): State<Shared>) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let first = build_state(&app).await.to_string();
        if socket.send(Message::Text(first.into())).await.is_err() {
            return;
        }
        let mut rx = app.panel_tx.subscribe();
        loop {
            tokio::select! {
                s = rx.recv() => match s {
                    Ok(s) => if socket.send(Message::Text(s.into())).await.is_err() { break },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                },
                m = socket.recv() => if !matches!(m, Some(Ok(_))) { break },
            }
        }
    })
}

// ---------------------------------------------------------------- comandos

#[derive(serde::Deserialize)]
struct CommandReq {
    machine: String,
    command: Command,
}

async fn api_command(State(app): State<Shared>, headers: HeaderMap, Json(req): Json<CommandReq>) -> Response {
    if !config::ct_eq(bearer(&headers).as_bytes(), app.cfg.admin_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "token de admin inválido").into_response();
    }
    let id = {
        let mut n = app.next_cmd.lock().await;
        *n += 1;
        *n
    };
    // Um spot, um dono: recusa dar a um char um spot (até 2 tiles) que já é de outro char,
    // de qualquer PC (o mapa é o mesmo para todos).
    if let Command::AssignSpot { char_name, x, y } = &req.command {
        let owner = app.hub.lock().await.snapshot(now()).into_iter().find(|c| {
            !(c.machine == req.machine && c.name == *char_name) && c.spot_map.as_deref().map_or(true, |m| m == "Kanturu") && c.spot.is_some_and(|s| (s.0 - x).abs().max((s.1 - y).abs()) <= 2)
        });
        if let Some(o) = owner {
            return (StatusCode::CONFLICT, format!("o spot ({x}, {y}) já é do {} ({}). Troque ele de spot antes.", o.name, o.machine)).into_response();
        }
    }
    // Reservar e encaminhar são separados da confirmação física: só o agente confirma chegada.
    let assignment = match &req.command {
        Command::AssignSpot{char_name,x,y}=>Some((char_name,zen_core::spots::Spot{world:38,x:*x,y:*y})),
        Command::GoTo{char_name,world,x,y,make_spot:true}=>Some((char_name,zen_core::spots::Spot{world:*world,x:*x,y:*y})),
        _=>None
    };
    if let Some((name,spot))=assignment {
        let mut book=app.spot_book.lock().await;
        let before=book.clone();
        if let Err(reason)=book.reserve(&req.machine,name,spot,now()) {
            return (StatusCode::CONFLICT,reason).into_response();
        }
        if let Err(e)=save_spot_book(&book) {
            *book=before;return (StatusCode::INTERNAL_SERVER_ERROR,format!("não confirmei reserva: {e}")).into_response();
        }
    }
    let desc = format!("{:?}", req.command);
    let _ = db::audit(&app.pool, now(), "admin", &req.machine, &desc).await;
    let link = app.links.lock().await.get(&req.machine).cloned();
    match link {
        Some(l)
            if l.send(CentralMsg::Command {
                id,
                command: req.command,
            })
            .is_ok() =>
        {
            (StatusCode::ACCEPTED, Json(json!({"id": id}))).into_response()
        }
        _ => (StatusCode::CONFLICT, "máquina não conectada").into_response(),
    }
}

/// Atualização publicada (sem token: o arquivo é assinado; o agente confere a assinatura).
/// Esta conexão é de uma máquina do canal de teste? (pelo IP com que o agente está conectado)
async fn is_test_peer(app: &Shared, peer: SocketAddr) -> bool {
    if app.cfg.test_machines.is_empty() {
        return false;
    }
    let ip = peer.ip().to_string();
    let ips = app.ips.lock().await;
    app.cfg.test_machines.iter().any(|m| ips.get(m).is_some_and(|i| *i == ip))
}

async fn api_update_meta(State(app): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    let test = is_test_peer(&app, peer).await;
    match update::meta_for(test) {
        Some(m) => Json(m).into_response(),
        None => (StatusCode::NOT_FOUND, "nada publicado").into_response(),
    }
}

async fn api_update_bin(State(app): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    let test = is_test_peer(&app, peer).await;
    match tokio::fs::read(update::bin_for(test)).await {
        Ok(b) => ([(header::CONTENT_TYPE, "application/octet-stream")], b).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "nada publicado").into_response(),
    }
}

/// Últimas linhas do log de um agente (o central recolhe a cada 2 min). Exige o token de admin.
/// `?todas=1` devolve tudo; sem isso, só avisos/erros (o que interessa no dia a dia).
async fn api_logs(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(machine): Path<String>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    if !config::ct_eq(bearer(&headers).as_bytes(), app.cfg.admin_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "token de admin inválido").into_response();
    }
    if machine.is_empty() || !machine.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return (StatusCode::BAD_REQUEST, "nome de máquina inválido").into_response();
    }
    let p = config::CentralConfig::dir().join("logs").join("agentes").join(format!("{machine}.log"));
    let Ok(raw) = tokio::fs::read(&p).await else {
        return (StatusCode::NOT_FOUND, "ainda sem log desta máquina (chega a cada 2 min)").into_response();
    };
    let txt = String::from_utf8_lossy(&raw);
    let all = q.get("todas").is_some_and(|v| v == "1");
    let lines: Vec<&str> = txt.lines().filter(|l| all || l.contains(" WARN ") || l.contains(" ERROR ")).collect();
    let tail = lines[lines.len().saturating_sub(300)..].join("\n");
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], tail).into_response()
}

async fn api_auto_pairing_get(State(app): State<Shared>) -> Json<Value> {
    Json(json!({ "enabled": app.auto_pairing.load(Ordering::Relaxed) }))
}

#[derive(serde::Deserialize)]
struct AutoPairingReq {
    enabled: bool,
}

/// Liga/desliga a automação de token. Exige o token de admin; fica gravado no central.toml.
async fn api_auto_pairing_set(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<AutoPairingReq>,
) -> Response {
    if !config::ct_eq(bearer(&headers).as_bytes(), app.cfg.admin_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "token de admin inválido").into_response();
    }
    app.auto_pairing.store(req.enabled, Ordering::Relaxed);
    save_tokens(&app).await;
    let _ = db::audit(&app.pool, now(), "admin", "-", &format!("auto_pairing={}", req.enabled)).await;
    tracing::info!(enabled = req.enabled, "automação de token alterada pelo painel");
    Json(json!({ "enabled": req.enabled })).into_response()
}

#[derive(serde::Deserialize)]
struct MachineReq {
    machine: String,
}

/// "Remover PC": apaga o token, bloqueia o nome (o pareamento automático não traz de volta),
/// derruba a conexão e tira do painel. Desfazer: tirar o nome de `blocked_machines` no
/// central.toml (com o central PARADO, senão ele sobrescreve o arquivo).
async fn api_machine_remove(State(app): State<Shared>, headers: HeaderMap, Json(req): Json<MachineReq>) -> Response {
    if !config::ct_eq(bearer(&headers).as_bytes(), app.cfg.admin_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "token de admin inválido").into_response();
    }
    let had_token = app.store.lock().await.remove_and_block(&req.machine);
    save_tokens(&app).await;
    let link = app.links.lock().await.remove(&req.machine);
    if let Some(l) = link {
        let _ = l.send(CentralMsg::Reject {
            reason: "máquina removida no painel".into(),
        });
    }
    app.hub.lock().await.forget_machine(&req.machine);
    {
        let mut book=app.spot_book.lock().await;
        book.remove_machine(&req.machine);
        if let Err(e)=save_spot_book(&book){tracing::error!(erro=%e,"não persisti liberação dos spots da máquina removida");}
    }
    broadcast_spots(&app).await;
    let _ = db::audit(&app.pool, now(), "admin", &req.machine, "remover e bloquear").await;
    tracing::warn!(machine = %req.machine, had_token, "máquina removida e bloqueada pelo painel");
    Json(json!({ "removed": had_token, "blocked": true })).into_response()
}

async fn api_screenshot(State(app): State<Shared>, Path((machine, char_name)): Path<(String, String)>) -> Response {
    match app.screenshots.lock().await.get(&format!("{machine}/{char_name}")) {
        Some(png) => ([(header::CONTENT_TYPE, "image/png")], png.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Terreno do Kanturu 1 para o mini-mapa do painel: 256x256 códigos em base64
/// (0 = piso, 1 = parede/sem chão, 2 = zona segura). Arquivo do cliente embutido no binário.
async fn api_terrain_kanturu() -> Response {
    use base64::Engine;
    static RAW: &[u8] = include_bytes!("../../zen-core/tests/fixtures/terrain/EncTerrain38.att");
    let Ok(t) = zen_core::terrain::Terrain::from_client_file(RAW) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut codes = Vec::with_capacity(256 * 256);
    for y in 0..256 {
        for x in 0..256 {
            codes.push(if !t.walkable(x, y) { 1u8 } else if t.is_safezone(x, y) { 2 } else { 0 });
        }
    }
    (
        [(header::CACHE_CONTROL, "public, max-age=86400")],
        Json(json!({"w": 256, "h": 256, "codes": base64::engine::general_purpose::STANDARD.encode(codes)})),
    )
        .into_response()
}

// ---------------------------------------------------------------- mapas (v4, Etapa 3)

/// Mapas do M. `validado`: nome no canto, botão do M e World conferidos visitando o mapa
/// (Kanturu, Lorencia e Arena em 08–09/11; os demais em 11/11, um move por botão com o x9901).
const MAPS: &[(u8, &str, bool)] = &[
    (38, "Kanturu", true),
    (1, "Lorencia", true),
    (7, "Arena (Stadium)", true),
    (2, "Dungeon", true),
    (3, "Devias", true),
    (4, "Noria", true),
    (5, "Lost Tower", true),
    (8, "Atlans", true),
    (9, "Tarkan", true),
    (11, "Icarus", true),
    (31, "Valley of Loren", true),
    (34, "Aida", true),
    (35, "Crywolf", true),
];

async fn api_maps(State(app): State<Shared>) -> Json<Value> {
    let dir = std::path::Path::new(&app.cfg.game_dir).join("Data");
    Json(json!(MAPS
        .iter()
        .map(|(w, n, v)| {
            let has = dir.join(format!("World{w}")).join(format!("EncTerrain{w}.att")).exists() || *w == 38;
            json!({"world": w, "name": n, "validated": v, "terrain": has})
        })
        .collect::<Vec<_>>()))
}

async fn api_terrain_world(State(app): State<Shared>, Path(world): Path<u8>) -> Response {
    use base64::Engine;
    let p = std::path::Path::new(&app.cfg.game_dir).join("Data").join(format!("World{world}")).join(format!("EncTerrain{world}.att"));
    let raw = match tokio::fs::read(&p).await {
        Ok(r) => r,
        Err(_) if world == 38 => include_bytes!("../../zen-core/tests/fixtures/terrain/EncTerrain38.att").to_vec(),
        Err(e) => return (StatusCode::NOT_FOUND, format!("{}: {e}", p.display())).into_response(),
    };
    let t = match zen_core::terrain::Terrain::from_client_file(&raw) {
        Ok(t) => t,
        Err(e) => return (StatusCode::UNPROCESSABLE_ENTITY, format!("terreno do World{world} em formato desconhecido: {e}")).into_response(),
    };
    let mut codes = Vec::with_capacity(256 * 256);
    for y in 0..256 {
        for x in 0..256 {
            codes.push(if !t.walkable(x, y) { 1u8 } else if t.is_safezone(x, y) { 2 } else { 0 });
        }
    }
    (
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        Json(json!({"w": 256, "h": 256, "world": world, "codes": base64::engine::general_purpose::STANDARD.encode(codes)})),
    )
        .into_response()
}

/// Desenho do mapa tirado do cliente do jogo (`Data\WorldN\mini_map.OZT`), já na orientação
/// do painel, 768 x 768 (3 px por tile). Só leitura do arquivo do jogo.
async fn api_minimap(State(app): State<Shared>, Path(world): Path<u8>) -> Response {
    let p = std::path::Path::new(&app.cfg.game_dir).join("Data").join(format!("World{world}")).join("mini_map.OZT");
    let raw = match tokio::fs::read(&p).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::NOT_FOUND, format!("{}: {e}", p.display())).into_response(),
    };
    let r = tokio::task::spawn_blocking(move || ozt::decode(&raw).and_then(|img| ozt::png(&ozt::minimap_for_panel(&img, 768)))).await;
    match r {
        Ok(Ok(png)) => ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")], png).into_response(),
        Ok(Err(e)) => (StatusCode::UNPROCESSABLE_ENTITY, format!("mini_map.OZT do World{world}: {e}")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Ícones do minimapa do jogo (`Data\Interface`): lista fechada.
async fn api_mapicon(State(app): State<Shared>, Path(name): Path<String>) -> Response {
    const OK: [&str; 5] = ["mini_map_ui_cha", "mini_map_ui_npc", "mini_map_ui_portal", "mini_map_ui_party", "Hero"];
    let name = name.trim_end_matches(".png");
    if !OK.contains(&name) {
        return (StatusCode::NOT_FOUND, "ícone desconhecido").into_response();
    }
    let p = std::path::Path::new(&app.cfg.game_dir).join("Data").join("Interface").join(format!("{name}.OZT"));
    match tokio::fs::read(&p).await.map_err(|e| e.to_string()).and_then(|raw| ozt::decode(&raw)).and_then(|i| ozt::png(&i)) {
        Ok(png) => ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")], png).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, format!("{}: {e}", p.display())).into_response(),
    }
}

/// Pontos salvos (favoritos, farm, destinos): pontos.json ao lado do central.
fn points_path() -> std::path::PathBuf {
    config::CentralConfig::dir().join("pontos.json")
}
fn points_load() -> Vec<Value> {
    std::fs::read_to_string(points_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn points_save(v: &[Value]) -> anyhow::Result<()> {
    let tmp = points_path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(v)?)?;
    std::fs::rename(&tmp, points_path())?;
    Ok(())
}
async fn api_points_get() -> Json<Value> {
    Json(json!(points_load()))
}
#[derive(serde::Deserialize)]
struct PointReq {
    name: String,
    world: u8,
    x: i32,
    y: i32,
    #[serde(default)]
    kind: String,
}
async fn api_points_add(State(app): State<Shared>, headers: HeaderMap, Json(p): Json<PointReq>) -> Response {
    if !config::ct_eq(bearer(&headers).as_bytes(), app.cfg.admin_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "token de admin inválido").into_response();
    }
    if !(0..256).contains(&p.x) || !(0..256).contains(&p.y) || p.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "ponto inválido").into_response();
    }
    let mut v = points_load();
    let id = v.iter().filter_map(|x| x["id"].as_u64()).max().unwrap_or(0) + 1;
    let kind = if ["farm", "favorito", "destino"].contains(&p.kind.as_str()) { p.kind } else { "favorito".into() };
    v.push(json!({"id": id, "name": zen_core::sanitize(&p.name, 40), "world": p.world, "x": p.x, "y": p.y, "kind": kind, "at": now()}));
    match points_save(&v) {
        Ok(()) => Json(json!({"id": id})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
async fn api_points_del(State(app): State<Shared>, headers: HeaderMap, Path(id): Path<u64>) -> Response {
    if !config::ct_eq(bearer(&headers).as_bytes(), app.cfg.admin_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "token de admin inválido").into_response();
    }
    let mut v = points_load();
    let n = v.len();
    v.retain(|x| x["id"].as_u64() != Some(id));
    if v.len() == n {
        return StatusCode::NOT_FOUND.into_response();
    }
    match points_save(&v) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn api_death_png(State(app): State<Shared>, Path(id): Path<i64>) -> Response {
    match db::death_png(&app.pool, id).await {
        Ok(Some(png)) => (
            [(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "private, max-age=86400")],
            png,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ---------------------------------------------------------------- tarefas

/// A cada 1 s: estado para os painéis e notificações para o celular.
async fn ticker(app: Shared) {
    let mut iv = tokio::time::interval(Duration::from_secs(1));
    loop {
        iv.tick().await;
        let state = build_state(&app).await.to_string();
        let _ = app.panel_tx.send(state);
        let ev = app.hub.lock().await.alert_events(now(), app.cfg.notify_cooldown_s);
        for m in &ev.started {
            tracing::info!(alerta_comecou = %m);
        }
        for m in &ev.ended {
            tracing::info!(alerta_terminou = %m);
        }
        for m in ev.phone {
            let app2 = app.clone();
            tokio::spawn(async move { notify::send(&app2.http, &app2.cfg, &m).await });
        }
    }
}

/// Ids de comando acima disto = pedido automático de logs (não aparece em Resultados).
const AUTO_LOG_ID: u64 = 1 << 60;

/// A cada 1 min: ranking, heartbeat, limpeza; a cada 2 min: logs de todos os agentes em
/// logs\agentes\<máquina>.log; 1x por dia: backup (7 cópias).
async fn slow_jobs(app: Shared) {
    let mut iv = tokio::time::interval(Duration::from_secs(60));
    let mut last_day = 0u64;
    let mut tick = 0u64;
    loop {
        iv.tick().await;
        tick += 1;
        if tick % 2 == 1 {
            for (i, l) in app.links.lock().await.values().enumerate() {
                let _ = l.send(CentralMsg::Command { id: AUTO_LOG_ID + tick * 1000 + i as u64, command: Command::SendLogs });
            }
        }
        let t = now();
        let mut rank = serde_json::Map::new();
        for (k, since) in [("24h", t - 86400), ("7d", t - 7 * 86400), ("total", 0)] {
            if let Ok(r) = db::pk_ranking(&app.pool, since).await {
                rank.insert(k.into(), json!(r));
            }
        }
        *app.ranking.lock().await = Value::Object(rank);
        notify::heartbeat(&app.http, &app.cfg.healthcheck_url).await;

        let day = t / 86400;
        if day != last_day {
            last_day = day;
            let _ = db::prune(&app.pool, t).await;
            let dir = config::CentralConfig::dir().join("backup");
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join(format!("central-{}.db", day % 7));
            match db::backup(&app.pool, &file.to_string_lossy()).await {
                Ok(()) => tracing::info!(arquivo = %file.display(), "backup diário"),
                Err(e) => tracing::error!("backup falhou: {e:#}"),
            }
        }
    }
}

// ---------------------------------------------------------------- teste de ponta a ponta
//
// Sobe o central de verdade numa porta livre, pareia um agente falso, conecta com o token,
// manda um relatório e confere o painel. Roda com `cargo test -p zen-central`.

#[cfg(test)]
mod e2e {
    use super::*;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::Message as TMsg;
    use zen_core::protocol::{CharReport, Health, ZenSource};
    use zen_core::title::TitleInfo;

    #[test]
    fn comandos_de_automacao_do_painel_sao_entendidos() {
        // Exatamente o JSON que o painel manda.
        let r: CommandReq = serde_json::from_str(
            r#"{"machine":"PC1","command":{"cmd":"run_action","char_name":"ReDMooN","action":"toggle_inventory"}}"#,
        )
        .unwrap();
        assert!(matches!(r.command, Command::RunAction { action: zen_core::actions::ActionKind::ToggleInventory, .. }));
        let r: CommandReq =
            serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"set_config","automation_enabled":false}}"#).unwrap();
        assert!(matches!(r.command, Command::SetConfig { automation_enabled: Some(false), chat_enabled: None, .. }));
        let r: CommandReq = serde_json::from_str(
            r#"{"machine":"PC1","command":{"cmd":"run_action","char_name":"x9901","action":"return_to_spot"}}"#,
        )
        .unwrap();
        assert!(matches!(r.command, Command::RunAction { action: zen_core::actions::ActionKind::ReturnToSpot, .. }));
        let r: CommandReq =
            serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"save_spot","char_name":"x9901"}}"#).unwrap();
        assert!(matches!(r.command, Command::SaveSpot { .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"assign_spot","char_name":"x9901","x":123,"y":233}}"#).unwrap();
        assert!(matches!(r.command, Command::AssignSpot { x: 123, y: 233, .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"delete_spot","x":30,"y":210}}"#).unwrap();
        assert!(matches!(r.command, Command::DeleteSpot { x: 30, y: 210, .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"set_return_on_death","char_name":"x9901","enabled":false}}"#).unwrap();
        assert!(matches!(r.command, Command::SetReturnOnDeath { enabled: false, .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"set_auto_deposit","char_name":"x9901","enabled":true}}"#).unwrap();
        assert!(matches!(r.command, Command::SetAutoDeposit { enabled: true, .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"set_spot_lock","char_name":"x9901","locked":true}}"#).unwrap();
        assert!(matches!(r.command, Command::SetSpotLock { locked: true, .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"set_char_automation","char_name":"x9901","enabled":false}}"#).unwrap();
        assert!(matches!(r.command, Command::SetCharAutomation { enabled: false, .. }));
        let r: CommandReq = serde_json::from_str(r#"{"machine":"PC1","command":{"cmd":"set_death_return","max_returns":3,"window_min":10,"delay_min_s":60,"delay_max_s":600}}"#).unwrap();
        assert!(matches!(r.command, Command::SetDeathReturn { max_returns: 3, window_min: 10, .. }));
        // Fora da lista fechada: recusado.
        assert!(serde_json::from_str::<CommandReq>(
            r#"{"machine":"PC1","command":{"cmd":"run_action","char_name":"x","action":"digitar_qualquer_coisa"}}"#
        )
        .is_err());
    }

    async fn start() -> (Shared, SocketAddr, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("zen-e2e-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("ZEN_CENTRAL_DIR", &dir);
        std::env::remove_var("ZEN_AUTO_PAIRING");
        let cfg = config::CentralConfig::load_or_create().unwrap();
        assert!(cfg.auto_pairing, "automação de token deve vir ligada por padrão");
        let pool = db::open(&dir.join("teste.db").to_string_lossy()).await.unwrap();
        let app = build_app(cfg, pool).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = router(app.clone());
        tokio::spawn(async move {
            axum::serve(listener, routes.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        });
        (app, addr, dir)
    }

    async fn pair(addr: SocketAddr, machine: &str, install_id: &str) -> CentralMsg {
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/pair"))
            .await
            .unwrap();
        let msg = AgentMsg::Pair {
            protocol: PROTOCOL_VERSION,
            machine: machine.into(),
            install_id: install_id.into(),
        };
        ws.send(TMsg::Text(serde_json::to_string(&msg).unwrap())).await.unwrap();
        loop {
            match ws.next().await.unwrap().unwrap() {
                TMsg::Text(t) => return serde_json::from_str(&t).unwrap(),
                _ => continue,
            }
        }
    }

    fn report(name: &str, zen: u64) -> AgentMsg {
        AgentMsg::Report {
            seq: 1,
            sent_at: now(),
            chars: vec![CharReport {
                primary_retry_at: None,
                home_spot: None,
                pid: 1,
                title: TitleInfo {
                    name: Some(name.into()),
                    level: Some(400),
                    ..Default::default()
                },
                zen: Some(zen),
                zen_source: ZenSource::Ram,
                lock_status: "travado 0x1".into(),
                tcp_connected: true,
                disconnected: false,
                hp: Some(6429),
                hp_status: "vida travada 0x1".into(),
                auto_status: String::new(),
                pos: Some((123, 233)),
                map: Some("Kanturu".into()),
                spot_map: Some("Kanturu".into()),
                spot: Some((123, 233)),
                return_on_death: Some(true),
                auto_deposit: Some(true),
                spot_locked: Some(false),
                automation: Some(true),
                skill_key: Some(1),
                pet_hp: None,
                nearby: None,
            }],
            events: vec![],
            health: Health::default(),
            catalog: vec![(30, 210), (123, 233)],
            catalog_other: vec![],
        }
    }

    #[tokio::test]
    async fn pareia_conecta_relata_e_respeita_o_interruptor() {
        let (app, addr, dir) = start().await;

        // 1. Pareamento automático.
        let CentralMsg::Paired { token, machine } = pair(addr, "PCTESTE", "install-id-e2e-0123456789").await else {
            panic!("pareamento deveria funcionar com auto_pairing ligado")
        };
        assert_eq!(machine, "PCTESTE");
        let toml = std::fs::read_to_string(dir.join("central.toml")).unwrap();
        assert!(
            toml.contains("PCTESTE") && toml.contains(&token),
            "token deve ser gravado no central.toml"
        );

        // 2. Token errado é recusado.
        let mut bad = format!("ws://{addr}/ws/agent").into_client_request().unwrap();
        bad.headers_mut()
            .insert("authorization", "Bearer token-errado-0123456789abcdef".parse().unwrap());
        assert!(tokio_tungstenite::connect_async(bad).await.is_err());

        // 3. Token certo: hello, welcome, relatório, painel.
        let mut req = format!("ws://{addr}/ws/agent").into_client_request().unwrap();
        req.headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        let hello = AgentMsg::Hello {
            protocol: PROTOCOL_VERSION,
            agent_version: "teste".into(),
            machine: "PCTESTE".into(),
            channel: "stable".into(),
        };
        ws.send(TMsg::Text(serde_json::to_string(&hello).unwrap()))
            .await
            .unwrap();
        let welcome = ws.next().await.unwrap().unwrap();
        assert!(welcome.to_text().unwrap().contains("welcome"));
        let mut first_report=report("Teste",423_485_898);
        if let AgentMsg::Report{chars,..}=&mut first_report {
            let mut second=chars[0].clone();second.title.name=Some("ZOutro".into());
            second.spot=Some((200,200));chars.push(second);
        }
        ws.send(TMsg::Text(
            serde_json::to_string(&first_report).unwrap(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let state = build_state(&app).await;
        assert_eq!(state["chars"][0]["name"], "Teste");
        assert_eq!(state["chars"][0]["zen"], 423_485_898u64);
        assert_eq!(state["auto_pairing"], true);
        assert_eq!(state["chars"][0]["hp"], 6429);
        assert_eq!(state["catalog"][1]["n"], 2);
        assert_eq!(state["catalog"][1]["x"], 123);
        assert_eq!(state["chars"][0]["spot"][0], 123);
        let terr: Value =
            serde_json::from_str(&reqwest::get(format!("http://{addr}/api/terrain/kanturu")).await.unwrap().text().await.unwrap()).unwrap();
        assert_eq!(terr["w"], 256);

        // Reservas passam pelo WebSocket autenticado e são persistidas antes do ACK.
        use zen_core::spots::{Spot,Directory};
        let target=Spot{world:38,x:220,y:220};
        ws.send(TMsg::Text(serde_json::to_string(&AgentMsg::SpotLease {
            request_id:71,char_name:"Teste".into(),spot:target,release:false
        }).unwrap())).await.unwrap();
        let ack=tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                if let Some(Ok(TMsg::Text(text)))=ws.next().await {
                    if let Ok(CentralMsg::SpotLeaseResult{request_id:71,accepted,..})=serde_json::from_str(&text){break accepted}
                }
            }
        }).await.unwrap();
        assert!(ack);
        let saved:Directory=serde_json::from_slice(&std::fs::read(dir.join("spot-directory.json")).unwrap()).unwrap();
        assert!(saved.friendly("Teste"));assert!(saved.leases.iter().any(|l|l.spot==target));
        // O mesmo socket não pode reservar usando um personagem de outro PC.
        ws.send(TMsg::Text(serde_json::to_string(&AgentMsg::SpotLease {
            request_id:72,char_name:"Intruso".into(),spot:Spot{world:7,x:10,y:10},release:false
        }).unwrap())).await.unwrap();
        let ack=tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                if let Some(Ok(TMsg::Text(text)))=ws.next().await {
                    if let Ok(CentralMsg::SpotLeaseResult{request_id:72,accepted,..})=serde_json::from_str(&text){break accepted}
                }
            }
        }).await.unwrap();
        assert!(!ack);

        ws.send(TMsg::Text(serde_json::to_string(&AgentMsg::SpotLease {
            request_id:73,char_name:"Teste".into(),spot:Spot{world:38,x:200,y:200},release:false
        }).unwrap())).await.unwrap();
        let ack=tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                if let Some(Ok(TMsg::Text(text)))=ws.next().await {
                    if let Ok(CentralMsg::SpotLeaseResult{request_id:73,accepted,..})=serde_json::from_str(&text){break accepted}
                }
            }
        }).await.unwrap();
        assert!(!ack,"spot do outro personagem próprio deve permanecer reservado");

        // Atalhos repetidos, inclusive com mensagem igual, precisam de identidades distintas.
        for _ in 0..2 {push_result(&app,json!({"id":0,"machine":"PCTESTE","ok":true,"message":"F2 (Teste): catálogo salvo","ts":now()})).await;}
        {let r=app.results.lock().await;assert_ne!(r[0]["event_key"],r[1]["event_key"]);}

        // 3b. Morte com print: alerta MORTO, lista de mortes e a imagem servida.
        let death = AgentMsg::Death {
            char_name: "Teste".into(),
            at: now(),
            hp_before: 312,
            screen_hp: Some(0),
            png_base64: Some("iVBORw0KGgo=".into()), // só a assinatura do PNG
        };
        ws.send(TMsg::Text(serde_json::to_string(&death).unwrap())).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let state = build_state(&app).await;
        assert!(state["chars"][0]["alerts"].as_array().unwrap().iter().any(|a| a == "Dead"), "{state}");
        let d = &state["deaths"][0];
        assert_eq!(d["char"], "Teste");
        assert_eq!(d["hp_before"], 312);
        let url = d["png"].as_str().unwrap().to_string();
        let img = reqwest::get(format!("http://{addr}{url}")).await.unwrap();
        assert_eq!(img.status(), 200);
        assert_eq!(img.bytes().await.unwrap().as_ref(), b"\x89PNG\r\n\x1a\n");

        // 4. Outro PC tentando o mesmo nome: recusado.
        assert!(matches!(
            pair(addr, "PCTESTE", "outro-install-id-9999999").await,
            CentralMsg::Reject { .. }
        ));

        // 5. Interruptor desligado: pareamento recusado, token existente continua valendo.
        app.auto_pairing.store(false, Ordering::Relaxed);
        assert!(matches!(
            pair(addr, "PCNOVO", "install-id-novo-0123456789").await,
            CentralMsg::Reject { .. }
        ));
        // (esta segunda conexão abre e cai sem hello: NÃO pode derrubar o canal da primeira)
        let mut req = format!("ws://{addr}/ws/agent").into_client_request().unwrap();
        req.headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        assert!(tokio_tungstenite::connect_async(req).await.is_ok());
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            app.links.lock().await.contains_key("PCTESTE"),
            "conexão velha caindo apagou o canal da ativa"
        );

        // 6. Remover PC: token apagado, nome bloqueado, conexão aberta derrubada, some do painel.
        let r = reqwest::Client::new()
            .post(format!("http://{addr}/api/machine/remove"))
            .bearer_auth(&app.cfg.admin_token)
            .header("content-type", "application/json")
            .body(r#"{"machine":"PCTESTE"}"#)
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success(), "remover PC: {}", r.status());
        let mut got_reject = false;
        while let Ok(Some(Ok(m))) = tokio::time::timeout(Duration::from_secs(2), ws.next()).await {
            if let TMsg::Text(t) = m {
                got_reject |= t.contains("reject");
            }
        }
        assert!(got_reject, "a conexão aberta deve receber reject");
        let mut req = format!("ws://{addr}/ws/agent").into_client_request().unwrap();
        req.headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        assert!(
            tokio_tungstenite::connect_async(req).await.is_err(),
            "token removido deve ser recusado"
        );
        app.auto_pairing.store(true, Ordering::Relaxed);
        assert!(
            matches!(
                pair(addr, "PCTESTE", "install-id-e2e-0123456789").await,
                CentralMsg::Reject { .. }
            ),
            "nome bloqueado não pode voltar pelo pareamento"
        );
        let toml = std::fs::read_to_string(dir.join("central.toml")).unwrap();
        assert!(toml.contains("blocked_machines") && toml.contains("PCTESTE"));
        assert_eq!(build_state(&app).await["chars"].as_array().map(|a| a.len()), Some(0));

        // 7. Sem token de admin, remover é recusado.
        let r = reqwest::Client::new()
            .post(format!("http://{addr}/api/machine/remove"))
            .header("content-type", "application/json")
            .body(r#"{"machine":"X"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 401);

        let _ = std::fs::remove_dir_all(dir);
    }
}
