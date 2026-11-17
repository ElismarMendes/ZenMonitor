//! Persistência versionada. Editar/salvar nunca envia movimento ao agente.
use crate::Shared;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use sqlx::Row;
use zen_core::routes::RouteDefinition;
pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/routes", get(list).post(save))
        .route("/api/routes/{id}/history", get(history))
        .route("/api/routes/{id}/restore/{revision}", post(restore))
        .route("/api/routes/{id}", axum::routing::delete(remove))
}
pub(super) fn authorized(app: &Shared, headers: &HeaderMap) -> bool {
    crate::config::ct_eq(crate::bearer(headers).as_bytes(), app.cfg.admin_token.as_bytes())
}
pub(super) async fn current(app: &Shared, id: &str) -> Result<RouteDefinition, String> {
    let row = sqlx::query("SELECT definition,deleted FROM route_revisions WHERE id=? ORDER BY revision DESC LIMIT 1")
        .bind(id)
        .fetch_optional(&app.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("rota não encontrada")?;
    if row.get::<i64, _>("deleted") != 0 {
        return Err("rota excluída".into());
    }
    serde_json::from_str(row.get::<&str, _>("definition")).map_err(|e| e.to_string())
}
async fn write(app: &Shared, mut route: RouteDefinition, deleted: bool) -> Result<RouteDefinition, String> {
    route.validate()?;
    if route.id.is_empty() {
        route.id = format!("r-{:032x}", rand::random::<u128>());
        route.revision = 0;
    }
    if !route.id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err("ID inválido".into());
    }
    if let Some(id) = route.return_route.as_ref().filter(|_| !deleted) {
        if id == &route.id {
            return Err("rota de retorno deve ser outra rota".into());
        }
        let back = current(app, id).await?;
        if back.world != route.world {
            return Err("rota de retorno pertence a outro mapa".into());
        }
    }
    let old: Option<i64> = sqlx::query_scalar("SELECT MAX(revision) FROM route_revisions WHERE id=?")
        .bind(&route.id)
        .fetch_one(&app.pool)
        .await
        .map_err(|e| e.to_string())?;
    if old.unwrap_or(0) as u64 != route.revision {
        return Err("CONFLICT: revisão mudou; recarregue antes de salvar".into());
    }
    route.revision += 1;
    sqlx::query("INSERT INTO route_revisions(id,revision,definition,deleted,ts) VALUES(?,?,?,?,?)")
        .bind(&route.id)
        .bind(route.revision as i64)
        .bind(serde_json::to_string(&route).map_err(|e| e.to_string())?)
        .bind(i64::from(deleted))
        .bind(crate::now() as i64)
        .execute(&app.pool)
        .await
        .map_err(|e| e.to_string())?;
    let _ = crate::db::audit(
        &app.pool,
        crate::now(),
        "admin",
        "-",
        &format!("route {} revision {} deleted={deleted}", route.id, route.revision),
    )
    .await;
    Ok(route)
}
fn error(e: String) -> Response {
    (
        if e.starts_with("CONFLICT:") {
            StatusCode::CONFLICT
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        },
        e,
    )
        .into_response()
}
async fn list(State(app): State<Shared>) -> Response {
    match sqlx::query("SELECT definition,deleted FROM route_revisions r WHERE revision=(SELECT MAX(revision) FROM route_revisions WHERE id=r.id) ORDER BY ts DESC LIMIT 500").fetch_all(&app.pool).await {
        Ok(rows)=>Json(json!(rows.into_iter().filter(|r|r.get::<i64,_>("deleted")==0).filter_map(|r|serde_json::from_str::<RouteDefinition>(r.get::<&str,_>("definition")).ok()).collect::<Vec<_>>())).into_response(),
        Err(e)=>error(e.to_string())
    }
}
async fn save(State(app): State<Shared>, headers: HeaderMap, Json(route): Json<RouteDefinition>) -> Response {
    if !authorized(&app, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _lock = app.route_write.lock().await;
    match write(&app, route, false).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => error(e),
    }
}
async fn history(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    match sqlx::query("SELECT definition,deleted,ts FROM route_revisions WHERE id=? ORDER BY revision DESC LIMIT 200").bind(id).fetch_all(&app.pool).await{
        Ok(rows)=>Json(json!(rows.iter().filter_map(|r|serde_json::from_str::<serde_json::Value>(r.get::<&str,_>("definition")).ok().map(|definition|json!({"route":definition,"deleted":r.get::<i64,_>("deleted")!=0,"ts":r.get::<i64,_>("ts")}))).collect::<Vec<_>>())).into_response(),Err(e)=>error(e.to_string())}
}
async fn restore(State(app): State<Shared>, headers: HeaderMap, Path((id, revision)): Path<(String, u64)>) -> Response {
    if !authorized(&app, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _lock = app.route_write.lock().await;
    let row = sqlx::query("SELECT definition FROM route_revisions WHERE id=? AND revision=?")
        .bind(&id)
        .bind(revision as i64)
        .fetch_optional(&app.pool)
        .await;
    let result = async {
        let row = row.map_err(|e| e.to_string())?.ok_or("revisão não encontrada")?;
        let mut route: RouteDefinition =
            serde_json::from_str(row.get::<&str, _>("definition")).map_err(|e| e.to_string())?;
        let latest: Option<i64> = sqlx::query_scalar("SELECT MAX(revision) FROM route_revisions WHERE id=?")
            .bind(&id)
            .fetch_one(&app.pool)
            .await
            .map_err(|e| e.to_string())?;
        route.revision = latest.unwrap_or(0) as u64;
        write(&app, route, false).await
    }
    .await;
    match result {
        Ok(r) => Json(r).into_response(),
        Err(e) => error(e),
    }
}
async fn remove(State(app): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if !authorized(&app, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _lock = app.route_write.lock().await;
    let result = async {
        let route = current(&app, &id).await?;
        write(&app, route, true).await
    }
    .await;
    match result {
        Ok(r) => {
            let capable: Vec<_> = app
                .hub
                .lock()
                .await
                .machines
                .iter()
                .filter(|(_, m)| m.protocol >= 3)
                .map(|(name, _)| name.clone())
                .collect();
            for (machine, link) in app.links.lock().await.iter() {
                if capable.contains(machine) {
                    let _ = link.send(zen_core::protocol::CentralMsg::Command {
                        id: 0,
                        command: zen_core::protocol::Command::RevokeRoute { route_id: id.clone() },
                    });
                }
            }
            Json(r).into_response()
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn revisions_persist_conflict_delete_restore() {
        let mut cfg = crate::config::CentralConfig::default();
        cfg.admin_token = "test-token".into();
        let pool = crate::db::open(":memory:").await.unwrap();
        let app = crate::build_app(cfg, pool).unwrap();
        let r = RouteDefinition {
            id: String::new(),
            revision: 0,
            name: "ida".into(),
            world: 1,
            waypoints: vec![zen_core::spots::Spot {
                world: 1,
                x: 122,
                y: 108,
            }],
            target: zen_core::routes::RouteTarget::Point,
            bindings: vec![],
            return_route: None,
        };
        let a = write(&app, r, false).await.unwrap();
        assert_eq!(current(&app, &a.id).await.unwrap(), a);
        let mut b = a.clone();
        b.name = "volta".into();
        let b = write(&app, b, false).await.unwrap();
        assert_eq!(b.revision, 2);
        assert!(write(&app, a, false).await.unwrap_err().starts_with("CONFLICT:"));
        write(&app, b.clone(), true).await.unwrap();
        assert!(current(&app, &b.id).await.is_err());
        let mut restored = b;
        restored.revision = 3;
        assert_eq!(write(&app, restored, false).await.unwrap().revision, 4);
    }
}
