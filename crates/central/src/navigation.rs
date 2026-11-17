//! Snapshots isolados por observador e execução com prévia de uso único.
use crate::Shared;
use axum::{
    extract::{State,Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{post,get},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::Row;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use zen_core::{
    routes::{ObserverContext, RadarSnapshot, RouteDefinition, RoutePlan},
    spots::Spot,
};
pub struct Observation {
    at: Instant,
    snapshot: RadarSnapshot,
}
struct Preview {
    at: Instant,
    machine: String,
    route: RouteDefinition,
    context: ObserverContext,
    origin: Spot,
    plan: RoutePlan,
}
#[derive(Default)]
pub struct NavigationState {
    snapshots: BTreeMap<(String, String), Observation>,
    previews: BTreeMap<String, Preview>,
}
impl NavigationState {
    pub fn clear(&mut self, machine: &str) {
        self.snapshots.retain(|(m, _), _| m != machine);
        self.previews.retain(|_, p| p.machine != machine);
    }
    pub fn observe(&mut self, machine: &str, snapshot: RadarSnapshot) {
        if snapshot.context.char_name.len() > 32
            || snapshot.context.session.len() > 128
            || !snapshot.position.valid()
            || snapshot.position.world != snapshot.context.world
            || snapshot.context.age_ms > 10_000
            || snapshot.range > 25
            || snapshot.entities.as_ref().is_some_and(|v| {
                v.len() > 80
                    || v.iter()
                        .any(|o| o.x < 0 || o.y < 0 || o.x > 255 || o.y > 255 || o.name.len() > 32)
            })
        {
            return;
        }
        let key = (machine.into(), snapshot.context.char_name.clone());
        if self.snapshots.get(&key).is_some_and(|s| {
            s.snapshot.context.same_observer(&snapshot.context)
                && s.snapshot.context.sequence >= snapshot.context.sequence
        }) {
            return;
        }
        self.snapshots.insert(
            key,
            Observation {
                at: Instant::now(),
                snapshot,
            },
        );
    }
    fn get(&self, machine: &str, name: &str) -> Result<RadarSnapshot, String> {
        let s = self
            .snapshots
            .get(&(machine.into(), name.into()))
            .ok_or("agente sem radar compatível")?;
        let mut snapshot = s.snapshot.clone();
        snapshot.context.age_ms = snapshot
            .context
            .age_ms
            .saturating_add(s.at.elapsed().as_millis() as u64);
        if !snapshot.usable() {
            return Err("observação desconhecida, antiga ou mapa não confirmado".into());
        }
        Ok(snapshot)
    }
    pub fn panel(&self) -> Value {
        json!(self
            .snapshots
            .iter()
            .map(|((machine, _), s)| {
                let mut snapshot = s.snapshot.clone();
                snapshot.context.age_ms += s.at.elapsed().as_millis() as u64;
                json!({"machine":machine,"snapshot":snapshot})
            })
            .collect::<Vec<_>>())
    }
}
pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/navigation/preview", post(preview))
        .route("/api/navigation/execute", post(execute))
        .route("/api/navigation/runs", get(runs))
}
#[derive(Default,serde::Deserialize)]
struct RunQuery {
    machine:Option<String>,char_name:Option<String>,route_id:Option<String>,limit:Option<u32>,
}
async fn runs(State(app):State<Shared>,headers:HeaderMap,Query(q):Query<RunQuery>)->Response {
    if !crate::routes::authorized(&app,&headers){return StatusCode::UNAUTHORIZED.into_response();}
    if q.machine.as_ref().is_some_and(|v|v.len()>32)||q.char_name.as_ref().is_some_and(|v|v.len()>32)||q.route_id.as_ref().is_some_and(|v|v.len()>80){
        return (StatusCode::BAD_REQUEST,"filtro de histórico inválido").into_response();
    }
    let rows=sqlx::query("SELECT u.*,r.definition FROM route_runs u LEFT JOIN route_revisions r ON r.id=u.route_id AND r.revision=u.revision WHERE (? IS NULL OR u.machine=?) AND (? IS NULL OR u.char_name=?) AND (? IS NULL OR u.route_id=?) ORDER BY u.ts DESC,u.command_id DESC LIMIT ?")
        .bind(&q.machine).bind(&q.machine).bind(&q.char_name).bind(&q.char_name).bind(&q.route_id).bind(&q.route_id)
        .bind(q.limit.unwrap_or(30).clamp(1,100) as i64).fetch_all(&app.pool).await;
    match rows {
        Ok(rows)=>Json(json!(rows.iter().map(|r|{
            let name=r.get::<Option<String>,_>("definition").and_then(|s|serde_json::from_str::<RouteDefinition>(&s).ok()).map(|r|r.name);
            json!({"id":r.get::<i64,_>("command_id"),"machine":r.get::<String,_>("machine"),"char_name":r.get::<String,_>("char_name"),"route_id":r.get::<String,_>("route_id"),"route_name":name,"revision":r.get::<i64,_>("revision"),"status":r.get::<String,_>("status"),"started_at":r.get::<i64,_>("ts"),"finished_at":r.get::<Option<i64>,_>("finished_at"),"message":r.get::<Option<String>,_>("message")})
        }).collect::<Vec<_>>())).into_response(),
        Err(e)=>(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()).into_response(),
    }
}
#[derive(serde::Deserialize)]
struct PreviewRequest {
    #[serde(default)]
    machine: String,
    #[serde(default)]
    char_name: String,
    route: RouteDefinition,
    #[serde(default)]
    origin: Option<Spot>,
}
#[derive(serde::Deserialize)]
struct ExecuteRequest {
    token: String,
    confirmed: bool,
    #[serde(default)]
    install_for_recovery: bool,
}
async fn terrain(app: &Shared, world: u8) -> Result<zen_core::terrain::Terrain, String> {
    let path = std::path::Path::new(&app.cfg.game_dir)
        .join("Data")
        .join(format!("World{world}"))
        .join(format!("EncTerrain{world}.att"));
    let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
    let t = zen_core::terrain::Terrain::from_client_file(&bytes).map_err(|e| e.to_string())?;
    if t.world != world {
        return Err("terreno pertence a outro mapa".into());
    }
    Ok(t)
}
fn failure(e: impl ToString) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response()
}
async fn preview(State(app): State<Shared>, headers: HeaderMap, Json(req): Json<PreviewRequest>) -> Response {
    if !crate::routes::authorized(&app, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result: Result<Value, String> = async {
        req.route.validate()?;
        let _permit = app
            .route_planners
            .clone()
            .try_acquire_owned()
            .map_err(|_| "planejamento ocupado; tente novamente")?;
        let snapshot = if req.origin.is_none() {
            Some(app.navigation.lock().await.get(&req.machine, &req.char_name)?)
        } else {
            None
        };
        let origin = req
            .origin
            .or_else(|| snapshot.as_ref().map(|s| s.position))
            .ok_or("origem não informada")?;
        if origin.world != req.route.world {
            return Err("origem e rota precisam pertencer ao mesmo mapa".into());
        }
        let t = terrain(&app, req.route.world).await?;
        let route = req.route.clone();
        let plan = tokio::task::spawn_blocking(move || zen_core::navigation::plan(&t, origin, &route))
            .await
            .map_err(|e| e.to_string())??;
        let Some(snapshot) = snapshot else {
            return Ok(json!({"token":null,"executable":false,"plan":plan,"origin":origin}));
        };
        let token = format!("{:032x}", rand::random::<u128>());
        let mut state = app.navigation.lock().await;
        state.previews.retain(|_, p| p.at.elapsed() < Duration::from_secs(30));
        if state.previews.len() >= 32 {
            return Err("muitas prévias; aguarde expiração".into());
        }
        state.previews.insert(
            token.clone(),
            Preview {
                at: Instant::now(),
                machine: req.machine,
                route: req.route,
                context: snapshot.context,
                origin,
                plan: plan.clone(),
            },
        );
        Ok(json!({"token":token,"executable":true,"plan":plan,"expires_in_s":30,"origin":origin}))
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn execute(State(app): State<Shared>, headers: HeaderMap, Json(req): Json<ExecuteRequest>) -> Response {
    if !crate::routes::authorized(&app, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !req.confirmed {
        return (StatusCode::BAD_REQUEST, "confirmação explícita obrigatória").into_response();
    }
    let result:Result<Value,String>=async {
        let preview=app.navigation.lock().await.previews.remove(&req.token).ok_or("prévia ausente, alterada ou já utilizada")?;
        if !app.cfg.navigation_execution||!app.cfg.test_machines.contains(&preview.machine){return Err("execução experimental permitida somente no canal de teste habilitado".into());}
        if preview.at.elapsed()>Duration::from_secs(30){return Err("prévia expirada".into());}
        let route=crate::routes::current(&app,&preview.route.id).await?;
        if route!=preview.route{return Err("salve e valide a revisão atual antes de executar".into());}
        if req.install_for_recovery && !matches!(route.target,zen_core::routes::RouteTarget::Spot){return Err("instalação automática exige destino Spot".into());}
        if !route.bindings.is_empty()&&!route.bindings.iter().any(|b|b.machine==preview.machine&&b.char_name==preview.context.char_name){return Err("rota pertence a outro personagem".into());}
        let snapshot=app.navigation.lock().await.get(&preview.machine,&preview.context.char_name)?;
        if snapshot.busy||!snapshot.context.same_observer(&preview.context)||!snapshot.position.near(preview.origin,2){return Err("personagem ocupado ou contexto/posição mudou; refaça a prévia".into());}
        let t=terrain(&app,route.world).await?;
        if zen_core::navigation::terrain_hash(&t)!=preview.plan.terrain_hash{return Err("terreno mudou; refaça a prévia".into());}
        let compatible=app.hub.lock().await.machines.get(&preview.machine).is_some_and(|m|m.protocol>=3&&m.connected);
        if !compatible{return Err("agente incompatível ou desconectado".into());}
        let id={let mut n=app.next_cmd.lock().await;*n+=1;*n};
        let link=app.links.lock().await.get(&preview.machine).cloned().ok_or("agente desconectado")?;
        sqlx::query("INSERT INTO route_runs(command_id,machine,route_id,revision,status,ts,char_name) VALUES(?,?,?,?,?,?,?)")
            .bind(id as i64).bind(&preview.machine).bind(&route.id).bind(route.revision as i64).bind("enviada").bind(crate::now() as i64).bind(&preview.context.char_name).execute(&app.pool).await.map_err(|e|e.to_string())?;
        let _=crate::db::audit(&app.pool,crate::now(),"admin",&preview.machine,&format!("route execute {} revision {} command {id}",route.id,route.revision)).await;
        if link.send(zen_core::protocol::CentralMsg::Command{id,command:zen_core::protocol::Command::RunRoute{char_name:preview.context.char_name,route,context:snapshot.context,install_for_recovery:req.install_for_recovery}}).is_err(){
            let _=sqlx::query("UPDATE route_runs SET status='falhou',message='comunicação interrompida antes do envio',finished_at=? WHERE command_id=? AND machine=?").bind(crate::now() as i64).bind(id as i64).bind(&preview.machine).execute(&app.pool).await;
            return Err("comunicação interrompida antes do envio".into());
        }
        Ok(json!({"id":id,"status":"enviada; aguarda confirmação física do agente"}))
    }.await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zen_core::routes::ObserverContext;
    fn sample(name: &str, pid: u32) -> RadarSnapshot {
        RadarSnapshot {
            installed_routes: None,
            context: ObserverContext {
                char_name: name.into(),
                pid,
                session: "s".into(),
                epoch: 1,
                world: 1,
                sequence: 1,
                age_ms: 0,
                map_confirmed: true,
                instance: None,
            },
            position: Spot {
                world: 1,
                x: 122,
                y: 108,
            },
            entities: Some(vec![]),
            truncated: false,
            range: 25,
            busy: false,
        }
    }
    #[test]
    fn observer_isolation_old_sequence_and_expiration() {
        let mut s = NavigationState::default();
        s.observe("PC1", sample("a", 1));
        s.observe("PC2", sample("a", 2));
        assert_eq!(s.get("PC1", "a").unwrap().context.pid, 1);
        assert_eq!(s.get("PC2", "a").unwrap().context.pid, 2);
        let mut old = sample("a", 1);
        old.position.x = 1;
        s.observe("PC1", old);
        assert_eq!(s.get("PC1", "a").unwrap().position.x, 122);
        s.clear("PC1");
        assert!(s.get("PC1", "a").is_err());
        let mut stale = sample("a", 1);
        stale.context.age_ms = 10001;
        s.observe("PC1", stale);
        assert!(s.get("PC1", "a").is_err());
    }
    async fn value(response: Response) -> Value {
        let b = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }
    #[tokio::test]
    async fn run_history_requires_auth_and_isolates_machine_character_and_legacy_rows() {
        let mut cfg=crate::config::CentralConfig::default();cfg.admin_token="history-test".into();
        let app=crate::build_app(cfg,crate::db::open(":memory:").await.unwrap()).unwrap();
        for(id,machine,name) in [(1,"PC1","a"),(2,"PC1","b"),(3,"PC2","a"),(4,"PC1","")] {
            sqlx::query("INSERT INTO route_runs(command_id,machine,route_id,revision,status,ts,char_name) VALUES(?,?,'route',1,'enviada',100,?)")
                .bind(id).bind(machine).bind(name).execute(&app.pool).await.unwrap();
        }
        assert_eq!(runs(State(app.clone()),HeaderMap::new(),Query(RunQuery::default())).await.status(),StatusCode::UNAUTHORIZED);
        let mut headers=HeaderMap::new();headers.insert("authorization","Bearer history-test".parse().unwrap());
        let filtered=value(runs(State(app.clone()),headers.clone(),Query(RunQuery{machine:Some("PC1".into()),char_name:Some("a".into()),..Default::default()})).await).await;
        assert_eq!(filtered.as_array().unwrap().len(),1);assert_eq!(filtered[0]["id"],1);assert_eq!(filtered[0]["status"],"enviada");assert_eq!(filtered[0]["finished_at"],Value::Null);
        let all=value(runs(State(app.clone()),headers.clone(),Query(RunQuery::default())).await).await;
        assert_eq!(all.as_array().unwrap().len(),4);assert_eq!(all[0]["char_name"],"");
        let limited=value(runs(State(app.clone()),headers.clone(),Query(RunQuery{limit:Some(1),..Default::default()})).await).await;
        assert_eq!(limited.as_array().unwrap().len(),1);
        let absent=value(runs(State(app),headers,Query(RunQuery{char_name:Some("' OR 1=1".into()),..Default::default()})).await).await;
        assert!(absent.as_array().unwrap().is_empty());
    }
    #[tokio::test]
    async fn execution_requires_saved_revision_confirmation_fresh_context_and_single_use_nonce() {
        let dir = std::env::temp_dir().join(format!("zen-route-{}", rand::random::<u64>()));
        let data = dir.join("Data/World1");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("EncTerrain1.att"),
            include_bytes!("../../zen-core/tests/fixtures/terrain/EncTerrain1.att"),
        )
        .unwrap();
        let mut cfg = crate::config::CentralConfig::default();
        cfg.game_dir = dir.to_string_lossy().into();
        cfg.admin_token = "route-test-token".into();
        cfg.navigation_execution = true;
        cfg.test_machines = vec!["PC1".into()];
        let app = crate::build_app(cfg, crate::db::open(":memory:").await.unwrap()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer route-test-token".parse().unwrap());
        let route = RouteDefinition {
            id: "route-test".into(),
            revision: 1,
            name: "Trainer".into(),
            world: 1,
            waypoints: vec![Spot {
                world: 1,
                x: 123,
                y: 108,
            }],
            target: zen_core::routes::RouteTarget::Point,
            bindings: vec![zen_core::routes::RouteBinding {
                machine: "PC1".into(),
                char_name: "a".into(),
            }],
            return_route: None,
        };
        sqlx::query("INSERT INTO route_revisions VALUES(?,?,?,0,0)")
            .bind(&route.id)
            .bind(1i64)
            .bind(serde_json::to_string(&route).unwrap())
            .execute(&app.pool)
            .await
            .unwrap();
        app.navigation.lock().await.observe("PC1", sample("a", 1));
        {
            let mut hub = app.hub.lock().await;
            hub.on_hello("PC1", "test", "teste");
            hub.machines.get_mut("PC1").unwrap().protocol = 3;
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.links.lock().await.insert("PC1".into(), tx);
        let req = || PreviewRequest {
            machine: "PC1".into(),
            char_name: "a".into(),
            route: route.clone(),
            origin: None,
        };
        assert_eq!(
            preview(State(app.clone()), HeaderMap::new(), Json(req()))
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let p = value(preview(State(app.clone()), headers.clone(), Json(req())).await).await;
        let token = p["token"].as_str().unwrap().to_string();
        assert_eq!(
            execute(
                State(app.clone()),
                headers.clone(),
                Json(ExecuteRequest {
                    token: token.clone(),
                    confirmed: false,
                    install_for_recovery: false
                })
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(rx.try_recv().is_err(), "prévia e confirmação ausente nunca movimentam");
        let sent = execute(
            State(app.clone()),
            headers.clone(),
            Json(ExecuteRequest {
                token: token.clone(),
                confirmed: true,
                install_for_recovery: false,
            }),
        )
        .await;
        assert_eq!(sent.status(), StatusCode::OK);
        let msg = rx.try_recv().unwrap();
        assert!(matches!(
            msg,
            zen_core::protocol::CentralMsg::Command {
                command: zen_core::protocol::Command::RunRoute { .. },
                ..
            }
        ));
        assert_eq!(
            execute(
                State(app.clone()),
                headers.clone(),
                Json(ExecuteRequest {
                    token,
                    confirmed: true,
                    install_for_recovery: false
                })
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        for case in 0..5 {
            app.navigation.lock().await.observe("PC1", {
                let mut s = sample("a", 1);
                s.context.sequence = case * 2 + 10;
                s
            });
            let p = value(preview(State(app.clone()), headers.clone(), Json(req())).await).await;
            let token = p["token"].as_str().unwrap().to_string();
            match case {
                0 => {
                    app.navigation.lock().await.previews.get_mut(&token).unwrap().at =
                        Instant::now() - Duration::from_secs(31);
                }
                1 => {
                    let mut s = sample("a", 1);
                    s.context.epoch = 2;
                    app.navigation.lock().await.observe("PC1", s);
                }
                2 => {
                    let mut s = sample("a", 1);
                    s.context.sequence = 100;
                    s.busy = true;
                    app.navigation.lock().await.observe("PC1", s);
                }
                3 => {
                    let mut s = sample("a", 1);
                    s.context.sequence = 100;
                    s.position.x += 10;
                    app.navigation.lock().await.observe("PC1", s);
                }
                _ => {
                    app.links.lock().await.remove("PC1");
                }
            }
            assert_eq!(
                execute(
                    State(app.clone()),
                    headers.clone(),
                    Json(ExecuteRequest {
                        token,
                        confirmed: true,
                        install_for_recovery: false
                    })
                )
                .await
                .status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "caso {case}"
            );
            assert!(rx.try_recv().is_err());
            app.navigation.lock().await.clear("PC1");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
