//! Contexto e aprovações de rotas. O coletor consome somente leituras já existentes.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use zen_core::{
    protocol::CharReport,
    routes::{ObserverContext, RadarSnapshot, RouteDefinition},
    spots::Spot,
};
static LATEST: OnceLock<Mutex<BTreeMap<String, (Instant, RadarSnapshot)>>> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CHARACTER_GENERATIONS: OnceLock<Mutex<BTreeMap<String,u64>>> = OnceLock::new();
fn character_generation(name:&str)->u64 {
    *CHARACTER_GENERATIONS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).get(name).unwrap_or(&0)
}
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn enable(value: bool) {
    ENABLED.store(value, Ordering::SeqCst);
}
pub fn enabled() -> bool { ENABLED.load(Ordering::SeqCst) }
static BOOT: OnceLock<String> = OnceLock::new();
fn boot() -> &'static str {
    BOOT.get_or_init(|| {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    })
}
fn latest() -> &'static Mutex<BTreeMap<String, (Instant, RadarSnapshot)>> {
    LATEST.get_or_init(Default::default)
}
pub fn disconnected() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut m) = latest().lock() {
        m.clear();
    }
}
pub fn invalidate(name: &str) {
    let mut generations=CHARACTER_GENERATIONS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner());
    let generation=generations.entry(name.into()).or_default();
    *generation=generation.saturating_add(1);
    if let Ok(mut m)=latest().lock(){m.remove(name);}
}
pub fn snapshot(name: &str) -> Option<RadarSnapshot> {
    let m = latest().lock().ok()?;
    let (at, s) = m.get(name)?;
    let mut s = s.clone();
    s.context.age_ms = at.elapsed().as_millis() as u64;
    Some(s)
}
/// The normal collector must confirm the new map after a teleport. Never reuse
/// a snapshot from the previous map or a different process while it catches up.
pub fn confirmed_snapshot(name: &str, pid: u32, world: u8) -> Option<RadarSnapshot> {
    snapshot(name).filter(|s| {
        s.usable() && s.context.char_name == name && s.context.pid == pid && s.context.world == world
    })
}
pub fn wire_snapshot(s: &mut RadarSnapshot) -> bool {
    let Some(current) = snapshot(&s.context.char_name) else {
        return false;
    };
    if !s.context.same_observer(&current.context) || s.context.sequence != current.context.sequence {
        return false;
    }
    s.context.age_ms = current.context.age_ms;
    s.context.age_ms <= 10_000
}
#[derive(Default)]
pub struct Feed {
    generation: u64,
    states: BTreeMap<String, (u32, u64, u8, u64, u8, u64)>,
}
impl Feed {
    pub fn observe(&mut self, c: &CharReport, start: u64, sequence: u64, busy: bool) -> Option<RadarSnapshot> {
        let generation = GENERATION.load(Ordering::SeqCst);
        if self.generation != generation {
            self.states.clear();
            self.generation = generation;
        }
        let name = c.title.name.as_ref()?;
        let observed = c.map.as_deref().and_then(zen_core::spots::world_for_name).zip(c.pos);
        let Some((world, (x, y))) = observed else {
            if let Some(state) = self.states.get_mut(name) {
                state.3 += 1;
                state.4 = 0;
            }
            if let Ok(mut m) = latest().lock() {
                m.remove(name);
            }
            return None;
        };
        let generation=character_generation(name);
        let state = self.states.entry(name.clone()).or_insert((c.pid, start, world, 0, 0, generation));
        if (state.0, state.1, state.2, state.5) != (c.pid, start, world, generation) {
            *state = (c.pid, start, world, state.3 + 1, 0, generation);
        }
        state.4 = state.4.saturating_add(1);
        if c.disconnected || c.hp == Some(0) {
            state.3 = state.3.saturating_add(1);
            state.4 = 0;
        }
        let confirmed = state.4 >= 2 && start > 0;
        let snapshot = RadarSnapshot {
            installed_routes: installed_summary(name),
            context: ObserverContext {
                char_name: name.clone(),
                pid: c.pid,
                session: format!("{}:{start}:{}:{}", boot(), c.pid, GENERATION.load(Ordering::SeqCst)),
                epoch: state.3,
                world,
                sequence,
                age_ms: 0,
                map_confirmed: confirmed,
                instance: None,
            },
            position: Spot { world, x, y },
            entities: if confirmed { c.nearby.clone() } else { None },
            truncated: c.nearby.as_ref().is_some_and(|v| v.len() >= 80),
            range: 25,
            busy,
        };
        let generations=CHARACTER_GENERATIONS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner());
        if *generations.get(name).unwrap_or(&0)!=generation {return None;}
        if let Ok(mut m) = latest().lock() {
            m.insert(name.clone(), (Instant::now(), snapshot.clone()));
        }
        Some(snapshot)
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
struct Installed {
    routes: BTreeMap<String, RouteDefinition>,
}
static INSTALLED: OnceLock<Mutex<Installed>> = OnceLock::new();
static ACTIVE: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
fn active() -> &'static Mutex<std::collections::HashSet<String>> {
    ACTIVE.get_or_init(Default::default)
}
pub fn authorize(id: &str) -> Result<(), String> {
    active()
        .lock()
        .map_err(|_| "registro de rotas indisponível")?
        .insert(id.into());
    Ok(())
}
fn installed() -> &'static Mutex<Installed> {
    INSTALLED.get_or_init(|| {
        Mutex::new(
            std::fs::read(crate::config::AgentConfig::dir().join("navigation-routes.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        )
    })
}
fn edit(f: impl FnOnce(&mut Installed)) -> Result<(), String> {
    let mut m = installed().lock().map_err(|_| "catálogo de rotas indisponível")?;
    let mut next = m.clone();
    f(&mut next);
    save(&next)?;
    *m = next;
    Ok(())
}
fn route_key(name: &str, goal: Spot) -> String {
    serde_json::to_string(&(name, goal.world, goal.x, goal.y)).expect("tupla serializável")
}
pub fn install(name: &str, route: &RouteDefinition) -> Result<(), String> {
    let ids = active().lock().map_err(|_| "registro de rotas indisponível")?;
    if !ids.contains(&route.id) {
        return Err("NAV_REVOKED: rota revogada durante a execução".into());
    }
    edit(|m| {
        m.routes.insert(route_key(name, route.goal()), route.clone());
    })
}
pub fn revoke(id: &str) -> Result<(), String> {
    active().lock().map_err(|_| "registro indisponível")?.remove(id);
    edit(|m| m.routes.retain(|_, r| r.id != id))
}
pub fn retain(ids: &[String]) -> Result<(), String> {
    *active().lock().map_err(|_| "registro indisponível")? = ids.iter().cloned().collect();
    edit(|m| m.routes.retain(|_, r| ids.contains(&r.id)))
}
fn save(m: &Installed) -> Result<(), String> {
    let p = crate::config::AgentConfig::dir().join("navigation-routes.json");
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(m).map_err(|e| e.to_string())?)
        .and_then(|_| std::fs::rename(tmp, p))
        .map_err(|e| e.to_string())
}
pub fn installed_summary(name: &str) -> Option<Vec<zen_core::routes::InstalledRoute>> {
    let m = installed().lock().ok()?;
    Some(m.routes.iter().filter_map(|(key, r)| {
        let (owner, _, _, _): (String, u8, i32, i32) = serde_json::from_str(key).ok()?;
        (owner == name && r.validate().is_ok()).then(|| zen_core::routes::InstalledRoute {
            id: r.id.clone(), revision: r.revision, name: r.name.clone(), destination: r.goal(),
        })
    }).collect())
}
pub fn recovery_route(name: &str, goal: Spot) -> Option<RouteDefinition> {
    if !ENABLED.load(Ordering::SeqCst) {
        return None;
    }
    let m = installed().lock().ok()?;
    m.routes
        .get(&route_key(name, goal))
        .filter(|r| r.validate().is_ok() && matches!(r.target, zen_core::routes::RouteTarget::Spot) && r.goal() == goal)
        .cloned()
}

thread_local! {static SCOPE:std::cell::RefCell<Option<(ObserverContext,Instant,String)>>=const{std::cell::RefCell::new(None)};}
pub fn current_name() -> Option<String> {
    SCOPE.with(|v| v.borrow().as_ref().map(|(context, _, _)| context.char_name.clone()))
}
pub fn scoped<T>(context: ObserverContext, route_id: &str, f: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SCOPE.with(|v| *v.borrow_mut() = None);
        }
    }
    SCOPE.with(|v| *v.borrow_mut() = Some((context, Instant::now() + Duration::from_secs(240), route_id.into())));
    let _reset = Reset;
    f()
}
pub fn check() -> Result<(), String> {
    SCOPE.with(|v| {
        let scope = v.borrow();
        let Some((expected, deadline, id)) = scope.as_ref() else {
            return Ok(());
        };
        if Instant::now() > *deadline {
            return Err("NAV_TIMEOUT: tempo global de rota esgotado".into());
        }
        if !crate::net::ONLINE.load(Ordering::SeqCst) {
            return Err("NAV_DISCONNECTED: servidor indisponível".into());
        }
        if !id.is_empty() && !active().lock().map_err(|_| "registro indisponível")?.contains(id) {
            return Err("NAV_REVOKED: rota excluída/desativada".into());
        }
        let current = snapshot(&expected.char_name).ok_or("NAV_STALE: observação ausente")?;
        if !current.usable() || !expected.same_observer(&current.context) {
            return Err("NAV_CONTEXT_CHANGED: mapa, sessão, morte ou leitura desatualizada".into());
        }
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    static FEED_TEST_LOCK:Mutex<()>=Mutex::new(());
    #[test]
    fn installed_keys_isolate_character_map_and_destination() {
        let a = Spot {
            world: 38,
            x: 160,
            y: 197,
        };
        let b = Spot {
            world: 7,
            x: 160,
            y: 197,
        };
        let c = Spot {
            world: 38,
            x: 123,
            y: 233,
        };
        assert_ne!(route_key("x9901", a), route_key("ReDMooN", a));
        assert_ne!(route_key("x9901", a), route_key("x9901", b));
        assert_ne!(route_key("x9901", a), route_key("x9901", c));
    }
    #[test]
    fn feed_reuses_samples_and_invalidates_identity_map_death_and_connection() {
        let _guard=FEED_TEST_LOCK.lock().unwrap();
        let mut c:CharReport=serde_json::from_value(serde_json::json!({"pid":11,"title":{"name":"radar-test"},"zen":null,"zen_source":"unknown","lock_status":"","tcp_connected":true,"disconnected":false,"map":"Lorencia","pos":[122,108],"hp":100,"nearby":[]})).unwrap();
        let mut feed = Feed::default();
        assert!(!feed.observe(&c, 20, 1, false).unwrap().usable());
        let a = feed.observe(&c, 20, 2, false).unwrap();
        assert!(a.usable());
        assert!(confirmed_snapshot("radar-test", 11, 1).is_some());
        assert!(confirmed_snapshot("radar-test", 12, 1).is_none());
        assert!(confirmed_snapshot("radar-test", 11, 38).is_none());
        assert!(!feed.observe(&c, 21, 3, false).unwrap().usable());
        let b = feed.observe(&c, 21, 4, false).unwrap();
        assert!(!a.context.same_observer(&b.context));
        c.map = Some("Kanturu".into());
        assert!(!feed.observe(&c, 21, 5, false).unwrap().usable());
        assert!(confirmed_snapshot("radar-test", 11, 1).is_none());
        assert!(confirmed_snapshot("radar-test", 11, 38).is_none());
        let map = feed.observe(&c, 21, 6, false).unwrap();
        assert!(map.usable());
        assert!(confirmed_snapshot("radar-test", 11, 38).is_some());
        c.hp = Some(0);
        assert!(!feed.observe(&c, 21, 7, false).unwrap().usable());
        c.hp = Some(100);
        assert!(!feed.observe(&c, 21, 8, false).unwrap().usable());
        let revived = feed.observe(&c, 21, 9, false).unwrap();
        assert!(revived.usable());
        assert!(!map.context.same_observer(&revived.context));
        let mut pending = revived.clone();
        assert!(wire_snapshot(&mut pending));
        disconnected();
        assert!(!wire_snapshot(&mut pending));
        assert!(!feed.observe(&c, 21, 10, false).unwrap().usable());
        let reconnected = feed.observe(&c, 21, 11, false).unwrap();
        assert!(!revived.context.same_observer(&reconnected.context));
        scoped(reconnected.context, "", || {
            assert!(check().unwrap_err().starts_with("NAV_DISCONNECTED"))
        });
        assert!(check().is_ok(), "rotinas legadas não recebem o bloqueio opt-in");
    }
    #[test]
    fn invalidating_one_character_preserves_the_other_and_requires_two_new_samples() {
        let _guard=FEED_TEST_LOCK.lock().unwrap();
        let report=|name:&str,pid|serde_json::from_value::<CharReport>(serde_json::json!({"pid":pid,"title":{"name":name},"zen":null,"zen_source":"unknown","lock_status":"","tcp_connected":true,"disconnected":false,"map":"Lorencia","pos":[122,109],"hp":100,"nearby":[]})).unwrap();
        let a=report("isolation-a",31);let b=report("isolation-b",32);let mut feed=Feed::default();
        feed.observe(&a,100,1,false);let before=feed.observe(&a,100,2,false).unwrap();
        feed.observe(&b,200,1,false);let buddy=feed.observe(&b,200,2,false).unwrap();
        invalidate("isolation-a");
        assert!(snapshot("isolation-a").is_none());
        let preserved=snapshot("isolation-b").unwrap();
        assert!(preserved.usable());assert!(preserved.context.same_observer(&buddy.context));
        assert!(!feed.observe(&a,100,3,false).unwrap().usable());
        let resumed=feed.observe(&a,100,4,false).unwrap();
        assert!(resumed.usable());assert!(!before.context.same_observer(&resumed.context));
        disconnected();assert!(snapshot("isolation-a").is_none());assert!(snapshot("isolation-b").is_none());
    }
    #[test]
    fn active_route_aborts_on_death_and_does_not_resume_after_revival() {
        let _guard=FEED_TEST_LOCK.lock().unwrap();
        let mut c:CharReport=serde_json::from_value(serde_json::json!({"pid":55,"title":{"name":"route-death"},"zen":null,"zen_source":"unknown","lock_status":"","tcp_connected":true,"disconnected":false,"map":"Lorencia","pos":[122,109],"hp":100,"nearby":[]})).unwrap();
        let mut feed=Feed::default();feed.observe(&c,500,1,false);
        let before=feed.observe(&c,500,2,false).unwrap();
        let prior=crate::net::ONLINE.swap(true,Ordering::SeqCst);
        scoped(before.context,"",||{
            assert!(check().is_ok());
            c.hp=Some(0);feed.observe(&c,500,3,false);
            assert!(check().unwrap_err().starts_with("NAV_CONTEXT_CHANGED"));
            c.hp=Some(100);feed.observe(&c,500,4,false);feed.observe(&c,500,5,false);
            assert!(check().is_err(),"old route must not resume after death");
            crate::net::ONLINE.store(false,Ordering::SeqCst);
            assert!(check().unwrap_err().starts_with("NAV_DISCONNECTED"));
        });
        crate::net::ONLINE.store(prior,Ordering::SeqCst);
    }
    #[test]
    fn unknown_process_start_does_not_authorize_navigation() {
        let _guard=FEED_TEST_LOCK.lock().unwrap();
        let c:CharReport=serde_json::from_value(serde_json::json!({"pid":33,"title":{"name":"unknown-start"},"zen":null,"zen_source":"unknown","lock_status":"","tcp_connected":true,"disconnected":false,"map":"Lorencia","pos":[122,109],"hp":100,"nearby":[]})).unwrap();
        let mut feed=Feed::default();
        for seq in 1..4 {assert!(!feed.observe(&c,0,seq,false).unwrap().usable());}
        assert!(!feed.observe(&c,300,4,false).unwrap().usable());
        assert!(feed.observe(&c,300,5,false).unwrap().usable());
    }
}
