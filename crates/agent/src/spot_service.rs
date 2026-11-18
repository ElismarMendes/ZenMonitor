//! Coordenação de spots. Comunicação não bloqueia o ciclo de captura de tela.
use crate::config::AgentConfig;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Mutex, OnceLock,
};
use std::time::{Duration, Instant};
use zen_core::protocol::AgentMsg;
use zen_core::spots::{Directory, Spot};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveSpot {
    pub spot: Spot,
    pub home: Spot,
    pub retry_at: u64,
}

impl ActiveSpot {
    pub fn due(&self, home: Spot, now: u64) -> bool {
        self.home == home && self.spot.world != home.world && self.retry_at > 0 && now >= self.retry_at
    }
}

struct State {
    directory: Directory,
    received: Option<Instant>,
    active: BTreeMap<String, ActiveSpot>,
    outgoing: tokio::sync::mpsc::Sender<AgentMsg>,
    pending: BTreeMap<u64, mpsc::Sender<Result<(), String>>>,
    inflight: BTreeMap<String, Spot>,
    heartbeat: Instant,
    epochs: BTreeMap<String, u64>,
    assigned: Vec<String>,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static REQUEST: AtomicU64 = AtomicU64::new(1);

pub fn init(outgoing: tokio::sync::mpsc::Sender<AgentMsg>) -> anyhow::Result<()> {
    let dir = AgentConfig::dir();
    let read = |name: &str| std::fs::read(dir.join(name));
    let directory = match read("spot-group.json") {
        Ok(data) => serde_json::from_slice(&data)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Directory::default(),
        Err(e) => return Err(e.into()),
    };
    let active = match read("spot-fallback.json") {
        Ok(data) => serde_json::from_slice(&data)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e.into()),
    };
    STATE
        .set(Mutex::new(State {
            directory,
            received: None,
            active,
            outgoing,
            pending: BTreeMap::new(),
            inflight: BTreeMap::new(),
            heartbeat: Instant::now(),
            epochs: BTreeMap::new(),
            assigned: Vec::new(),
        }))
        .map_err(|_| anyhow::anyhow!("coordenação já inicializada"))?;
    Ok(())
}
fn persist(name: &str, value: &impl serde::Serialize) -> Result<(), String> {
    let path = AgentConfig::dir().join(name);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub fn receive(directory: Directory) {
    if let Some(state) = STATE.get() {
        if let Ok(mut s) = state.lock() {
            if s.directory != directory {
                if let Err(e) = persist("spot-group.json", &directory) {
                    tracing::error!(erro=%e,"não persisti grupo de spots");
                }
            }
            s.directory = directory;
            s.received = Some(Instant::now());
        }
    }
}
pub fn acknowledge(id: u64, accepted: bool, reason: String) {
    if let Some(state) = STATE.get() {
        if let Ok(mut s) = state.lock() {
            if let Some(tx) = s.pending.remove(&id) {
                let _ = tx.send(if accepted { Ok(()) } else { Err(reason) });
            }
        }
    }
}
pub fn directory(fresh: bool) -> Result<Directory, String> {
    let s = STATE
        .get()
        .ok_or("coordenação não inicializada")?
        .lock()
        .map_err(|_| "coordenação indisponível")?;
    if fresh && !s.received.is_some_and(|t| t.elapsed() < Duration::from_secs(45)) {
        return Err("diretório do grupo indisponível ou desatualizado; aguardando central".into());
    }
    Ok(s.directory.clone())
}
pub fn active(name: &str) -> Option<ActiveSpot> {
    STATE.get()?.lock().ok()?.active.get(name).cloned()
}
pub fn set_active(name: &str, value: ActiveSpot) -> Result<(), String> {
    commit_active(name, value, None)
}
pub fn set_active_for(name: &str, value: ActiveSpot, epoch: u64) -> Result<(), String> {
    commit_active(name, value, Some(epoch))
}
fn commit_active(name: &str, value: ActiveSpot, expected: Option<u64>) -> Result<(), String> {
    let mut s = STATE
        .get()
        .ok_or("coordenação não inicializada")?
        .lock()
        .map_err(|_| "coordenação indisponível")?;
    if expected.is_some_and(|e| e != s.epochs.get(name).copied().unwrap_or(0)) {
        return Err("SPOT_CANCELLED: principal alterado durante o trajeto".into());
    }
    let mut next = s.active.clone();
    next.insert(name.into(), value);
    persist("spot-fallback.json", &next)?;
    s.active = next;
    s.inflight.remove(name);
    Ok(())
}
pub fn clear_active(name: &str) -> Result<(), String> {
    let Some(state) = STATE.get() else { return Ok(()) };
    let mut s = state.lock().map_err(|_| "coordenação indisponível")?;
    let mut next = s.active.clone();
    next.remove(name);
    persist("spot-fallback.json", &next)?;
    s.active = next;
    s.inflight.remove(name);
    *s.epochs.entry(name.into()).or_default() += 1;
    s.assigned.push(name.into());
    Ok(())
}
/// Cancela o trajeto/busca em andamento deste char (sem mexer no spot ativo).
pub fn bump_epoch(name: &str) {
    if let Some(Ok(mut s)) = STATE.get().map(|s| s.lock()) {
        *s.epochs.entry(name.into()).or_default() += 1;
    }
}
pub fn epoch(name: &str) -> u64 {
    STATE
        .get()
        .and_then(|s| s.lock().ok())
        .and_then(|s| s.epochs.get(name).copied())
        .unwrap_or(0)
}
pub fn take_assignments() -> Vec<String> {
    STATE
        .get()
        .and_then(|s| s.lock().ok())
        .map(|mut s| std::mem::take(&mut s.assigned))
        .unwrap_or_default()
}
fn enqueue_request(s: &mut State, name: &str, spot: Spot, release: bool, id: u64,
    tx: mpsc::Sender<Result<(), String>>) -> Result<(), String> {
    // Abandono local é definitivo mesmo sem diretório fresco, fila disponível ou ACK.
    // Um heartbeat já enfileirado precede a liberação; sem ACK a reserva expira em 90 s.
    if release {
        if s.inflight.get(name) == Some(&spot) { s.inflight.remove(name); }
    } else if !s.received.is_some_and(|t| t.elapsed() < Duration::from_secs(45)) {
        return Err("diretório do grupo indisponível ou desatualizado; aguardando central".into());
    }
    s.pending.insert(id, tx);
    if s.outgoing.try_send(AgentMsg::SpotLease {
        request_id: id, char_name: name.into(), spot, release,
    }).is_err() {
        s.pending.remove(&id);
        return Err("fila do central cheia; reserva não confirmada".into());
    }
    Ok(())
}

pub fn request(name: &str, spot: Spot, release: bool) -> Result<(), String> {
    let id = REQUEST.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    {
        let mut s = STATE
            .get()
            .ok_or("coordenação não inicializada")?
            .lock()
            .map_err(|_| "coordenação indisponível")?;
        enqueue_request(&mut s, name, spot, release, id, tx)?;
    }
    let result = rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| Err("central não confirmou reserva em 5 s".into()));
    if let Some(s) = STATE.get() {
        if let Ok(mut s) = s.lock() {
            s.pending.remove(&id);
        }
    }
    if result.is_ok() && !release {
        if let Some(s) = STATE.get() {
            if let Ok(mut s) = s.lock() {
                s.inflight.insert(name.into(), spot);
            }
        }
    }
    result
}

pub fn heartbeat() {
    if let Some(state) = STATE.get() {
        if let Ok(mut s) = state.lock() {
            if s.heartbeat.elapsed() < Duration::from_secs(20) {
                return;
            }
            s.heartbeat = Instant::now();
            for (name, spot) in &s.inflight {
                let _ = s.outgoing.try_send(AgentMsg::SpotLease {
                    request_id: 0,
                    char_name: name.clone(),
                    spot: *spot,
                    release: false,
                });
            }
        }
    }
}

#[cfg(windows)]
pub fn home(cfg: &AgentConfig, name: &str) -> Option<Spot> {
    let [x, y] = *cfg.spots.get(name)?;
    let map = if cfg.arena.iter().any(|n| n == name) {
        "Arena"
    } else {
        cfg.spot_maps.get(name).map(String::as_str).unwrap_or("Kanturu")
    };
    Some(Spot {
        world: zen_core::spots::world_for_name(map)?,
        x,
        y,
    })
}
#[cfg(windows)]
pub fn effective(cfg: &AgentConfig, name: &str) -> Option<Spot> {
    let h = home(cfg, name)?;
    Some(active(name).filter(|a| a.home == h).map(|a| a.spot).unwrap_or(h))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request_state() -> (State, tokio::sync::mpsc::Receiver<AgentMsg>) {
        let (outgoing, incoming) = tokio::sync::mpsc::channel(1);
        (State { directory: Directory::default(), received: None, active: BTreeMap::new(),
            outgoing, pending: BTreeMap::new(), inflight: BTreeMap::new(), heartbeat: Instant::now(),
            epochs: BTreeMap::new(), assigned: Vec::new() }, incoming)
    }
    #[test]
    fn abandono_para_renovacao_mesmo_com_fila_cheia_ou_sem_diretorio() {
        let spot=Spot{world:38,x:10,y:10};
        let (mut s, mut incoming)=request_state();
        s.inflight.insert("Eu".into(),spot);
        let (tx,_)=mpsc::channel();
        enqueue_request(&mut s,"Eu",spot,true,1,tx).unwrap();
        assert!(s.inflight.is_empty()); // Sem ACK: heartbeat não tem mais o que renovar.
        s.inflight.insert("Eu".into(),spot);
        let (tx,_)=mpsc::channel();
        assert!(enqueue_request(&mut s,"Eu",spot,true,2,tx).is_err());
        assert!(s.inflight.is_empty());
        assert!(!s.pending.contains_key(&2));
        assert!(matches!(incoming.try_recv(),Ok(AgentMsg::SpotLease{release:true,..})));
    }
    #[test]
    fn liberar_reserva_antiga_preserva_destino_novo_e_aquisicao_exige_diretorio() {
        let old=Spot{world:38,x:10,y:10};
        let new=Spot{world:7,x:20,y:20};
        let (mut s, _incoming)=request_state();
        s.inflight.insert("Eu".into(),new);
        let (tx,_)=mpsc::channel();
        enqueue_request(&mut s,"Eu",old,true,1,tx).unwrap();
        assert_eq!(s.inflight.get("Eu"),Some(&new));
        let (tx,_)=mpsc::channel();
        assert!(enqueue_request(&mut s,"Outro",old,false,2,tx).unwrap_err().contains("desatualizado"));
        assert!(!s.inflight.contains_key("Outro"));
    }
    #[test]
    fn temporario_sobrevive_serializacao_e_so_reavalia_no_prazo() {
        let home = Spot {
            world: 38,
            x: 91,
            y: 66,
        };
        let a = ActiveSpot {
            home,
            spot: Spot { world: 7, x: 20, y: 30 },
            retry_at: 600,
        };
        let b: ActiveSpot = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert!(!b.due(home, 599));
        assert!(b.due(home, 600));
        assert!(!b.due(
            Spot {
                world: 38,
                x: 120,
                y: 25
            },
            700
        ));
        assert!(!ActiveSpot { spot: home, ..b }.due(home, 700));
    }
}
