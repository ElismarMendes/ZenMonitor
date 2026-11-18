//! Gravação das idas a invasões (dono, 18/11 11:39): enquanto o char está fora do spot por um
//! "ir até" (invasão), um quadro da janela a cada `intervalo_s` vai para
//! `gravacoes\<início>-<char>\quadro-<hora unix>.png`, em metade do tamanho, com um
//! `index.html` que passa os quadros como um filme. Pastas com mais de `manter_h` horas são
//! apagadas. Só tira print da janela do jogo; não mexe em nada.

use crate::win::capture;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Ativa {
    dir: PathBuf,
    proximo: Instant,
    quadros: u32,
}

pub struct Gravador {
    raiz: PathBuf,
    ativas: HashMap<String, Ativa>,
    pub intervalo: Duration,
    pub manter_h: u64,
    /// Limite por gravação (uma ida esquecida não enche o disco).
    pub max_quadros: u32,
    limpeza: Instant,
}

fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Gravador {
    pub fn new(raiz: PathBuf, intervalo_s: u64, manter_h: u64) -> Self {
        Self {
            raiz,
            ativas: HashMap::new(),
            intervalo: Duration::from_secs(intervalo_s.max(1)),
            manter_h,
            max_quadros: 1200,
            limpeza: Instant::now() - Duration::from_secs(7200),
        }
    }

    /// `chars`: (nome, janela, motivo) de quem deve estar sendo gravado agora. Quem saiu da
    /// lista tem a gravação fechada (o index.html é refeito).
    pub fn tick(&mut self, chars: &[(String, isize, String)]) {
        if self.limpeza.elapsed() > Duration::from_secs(3600) {
            self.limpeza = Instant::now();
            limpar(&self.raiz, self.manter_h);
        }
        let fim: Vec<String> = self.ativas.keys().filter(|n| !chars.iter().any(|c| &c.0 == *n)).cloned().collect();
        for n in fim {
            if let Some(a) = self.ativas.remove(&n) {
                escrever_index(&a.dir, &n);
                tracing::info!(char = %n, pasta = %a.dir.display(), quadros = a.quadros, "gravação encerrada");
            }
        }
        for (nome, hwnd, motivo) in chars {
            let a = self.ativas.entry(nome.clone()).or_insert_with(|| {
                let motivo: String = motivo.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(30).collect();
                let dir = self.raiz.join(format!("{}-{nome}-{motivo}", agora()));
                let _ = std::fs::create_dir_all(&dir);
                tracing::info!(char = %nome, pasta = %dir.display(), "gravação iniciada");
                Ativa { dir, proximo: Instant::now(), quadros: 0 }
            });
            if Instant::now() < a.proximo || a.quadros >= self.max_quadros {
                continue;
            }
            a.proximo = Instant::now() + self.intervalo;
            if let Some(png) = capture::capture(*hwnd, None).map(|i| i.downscale(2)).and_then(|i| i.to_png()) {
                let _ = std::fs::write(a.dir.join(format!("quadro-{}.png", agora())), png);
                a.quadros += 1;
                if a.quadros % 30 == 1 {
                    escrever_index(&a.dir, nome);
                }
            }
        }
    }

    /// Print avulso (travamento) dentro da gravação ativa do char, se houver.
    pub fn marcar(&self, nome: &str, png: &[u8], motivo: &str) {
        if let Some(a) = self.ativas.get(nome) {
            let _ = std::fs::write(a.dir.join(format!("quadro-{}-trava.png", agora())), png);
            let _ = std::fs::write(a.dir.join(format!("trava-{}.txt", agora())), motivo);
        }
    }
}

fn limpar(raiz: &PathBuf, manter_h: u64) {
    let Ok(rd) = std::fs::read_dir(raiz) else { return };
    let limite = agora().saturating_sub(manter_h * 3600);
    for e in rd.flatten() {
        let nome = e.file_name().to_string_lossy().into_owned();
        let ts: u64 = nome.split('-').next().and_then(|s| s.parse().ok()).unwrap_or(u64::MAX);
        if ts < limite && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Página que passa os quadros como filme (abre no navegador do PC; tudo local).
fn escrever_index(dir: &PathBuf, nome: &str) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut q: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".png")).collect();
    q.sort();
    let lista = q.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(",");
    let html = format!(r#"<!doctype html><meta charset="utf-8"><title>{nome}</title>
<style>body{{background:#111;color:#ddd;font:14px sans-serif;margin:12px}}img{{max-width:100%;border:1px solid #333}}input{{width:100%}}</style>
<h3>{nome} — <span id="h"></span> (<span id="i"></span>)</h3>
<img id="q"><br><input id="s" type="range" min="0" value="0">
<button id="p">▶ / ❚❚</button> velocidade <select id="v"><option>1</option><option selected>4</option><option>10</option></select>x
<script>
const Q=[{lista}];const img=document.getElementById('q'),s=document.getElementById('s');let k=0,play=false;
s.max=Q.length-1;function show(){{img.src=Q[k];s.value=k;const t=parseInt((Q[k].match(/quadro-(\d+)/)||[])[1]||0);
document.getElementById('h').textContent=new Date(t*1000).toLocaleString();document.getElementById('i').textContent=(k+1)+'/'+Q.length+(Q[k].includes('trava')?' TRAVAMENTO':'');}}
s.oninput=()=>{{k=+s.value;show();}};document.getElementById('p').onclick=()=>play=!play;
function tick(){{if(play&&k<Q.length-1){{k++;show();}}setTimeout(tick,2000/(+document.getElementById('v').value));}}tick();
show();
</script>"#);
    let _ = std::fs::write(dir.join("index.html"), html);
}
