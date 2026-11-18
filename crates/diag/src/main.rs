//! zen-diag.exe — diagnóstico. Grava o resultado em arquivo para você me mandar.
//!
//!   zen-diag janelas                 lista janelas do jogo, pid e título
//!   zen-diag ocr [x y w h]           o que o OCR lê no campo do Zen (salva os PNGs)
//!   zen-diag vida-mem [seg] [nome]   acha o endereço do HP (semente = número da tela)
//!   zen-diag tecla <nome> [V|C]     teste de tecla por mensagem de janela (sem driver)
//!   zen-diag zen <pid> <valor>       quantas vezes o valor aparece (u32) na memória
//!   zen-diag chat <pid>              todas as linhas "[PK]" e "[PickItem]" + endereço
//!   zen-diag listachat <pid>         procura a LISTA ORDENADA do chat: arrays de
//!                                    ponteiros que apontam para as linhas achadas
//!
//! A pergunta que o `listachat` responde é a que destrava a contagem de joias:
//! existe na memória uma lista de linhas com ordem? Se existir, contamos pela posição
//! e não pelo texto, e o problema das 5 a 11 cópias some.

#[cfg(not(windows))]
fn main() {
    eprintln!("zen-diag só roda no Windows.");
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use std::fmt::Write as _;
    use zen_agent::win::memory::Proc;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = String::new();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let pid = |i: usize| -> anyhow::Result<u32> {
        args.get(i)
            .ok_or_else(|| anyhow::anyhow!("falta o pid"))?
            .parse()
            .map_err(Into::into)
    };

    match cmd {
        "pick-ver" => {
            // Só olhar a Custom Pick Window (dono, 16/11): Y, prints, ESC, print. Não clica em nada.
            // Uso: zen-diag pick-ver <char> [pasta dos prints]
            use zen_agent::win::{capture, input, nav, windows_list};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta personagem"))?.clone();
            let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
            let w = windows_list::find_by_class("The Classic MU Season 2").into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(name.as_str()))
                .ok_or_else(|| anyhow::anyhow!("personagem não encontrado"))?;
            let (map, pos) = nav::observe_named(w.hwnd);
            writeln!(out, "{name} pid={} mapa={map:?} pos={pos:?}", w.pid)?;
            input::start_manual_guard(180);
            input::set_paused(false);
            let _session = input::Session::begin(w.hwnd).map_err(anyhow::Error::msg)?;
            let shot = |tag: &str| {
                if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("{dir}\\pick_{name}_{tag}.png"), png);
                }
            };
            let wait = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
            shot("0antes");
            input::run(w.hwnd, &[Step::Key(0x59)]).map_err(anyhow::Error::msg)?;
            wait(1200);
            shot("1y");
            wait(1500);
            shot("2y");
            input::run(w.hwnd, &[Step::Key(0x1B)]).map_err(anyhow::Error::msg)?;
            wait(1000);
            shot("3esc");
            writeln!(out, "Y e ESC enviados; prints pick_{name}_*.png em {dir}")?;
        }
        "pick-teste" => {
            // Custom Pick Window: Y, clica no botão "Zen" (233,551 no print de 16/11), fecha com Y
            // (ESC abre o Especial Menu e fecha o Helper). Prints de cada passo.
            // Uso: zen-diag pick-teste <char> [pasta]
            use zen_agent::win::{capture, input, nav, windows_list};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta personagem"))?.clone();
            let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
            let w = windows_list::find_by_class("The Classic MU Season 2").into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(name.as_str()))
                .ok_or_else(|| anyhow::anyhow!("personagem não encontrado"))?;
            let (map, pos) = nav::observe_named(w.hwnd);
            writeln!(out, "{name} pid={} mapa={map:?} pos={pos:?} helper={:?}", w.pid, nav::helper_state(w.hwnd))?;
            input::start_manual_guard(180);
            input::set_paused(false);
            let _session = input::Session::begin(w.hwnd).map_err(anyhow::Error::msg)?;
            let shot = |tag: &str| {
                if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("{dir}\\pickt_{name}_{tag}.png"), png);
                }
            };
            let wait = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
            input::run(w.hwnd, &[Step::Key(0x59)]).map_err(anyhow::Error::msg)?;
            wait(1500);
            shot("1aberta");
            input::run(w.hwnd, &[Step::Click { x: 233, y: 551 }]).map_err(anyhow::Error::msg)?;
            wait(1200);
            shot("2zen");
            input::run(w.hwnd, &[Step::Key(0x59)]).map_err(anyhow::Error::msg)?;
            wait(1200);
            shot("3y");
            writeln!(out, "fim: helper={:?}", nav::helper_state(w.hwnd))?;
        }
        "pick-estado" => {
            // Custom Pick Window: Y, clica no botão "Zen" (233,551 no print de 16/11), fecha com Y
            // (ESC abre o Especial Menu e fecha o Helper). Prints de cada passo.
            // Uso: zen-diag pick-teste <char> [pasta]
            use zen_agent::win::{capture, input, nav, windows_list};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta personagem"))?.clone();
            let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
            let w = windows_list::find_by_class("The Classic MU Season 2").into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.as_deref() == Some(name.as_str()))
                .ok_or_else(|| anyhow::anyhow!("personagem não encontrado"))?;
            let (map, pos) = nav::observe_named(w.hwnd);
            writeln!(out, "{name} pid={} mapa={map:?} pos={pos:?} helper={:?}", w.pid, nav::helper_state(w.hwnd))?;
            input::start_manual_guard(180);
            input::set_paused(false);
            let _session = input::Session::begin(w.hwnd).map_err(anyhow::Error::msg)?;
            let shot = |tag: &str| {
                if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("{dir}\\picke_{name}_{tag}.png"), png);
                }
            };
            let wait = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
            // Estado do botão Zen: aberta, clique, cursor fora, print; clique de novo, print.
            input::run(w.hwnd, &[Step::Key(0x59)]).map_err(anyhow::Error::msg)?;
            wait(1500);
            input::run(w.hwnd, &[Step::Hover { x: 120, y: 605 }]).map_err(anyhow::Error::msg)?;
            wait(400);
            shot("1aberta");
            input::run(w.hwnd, &[Step::Click { x: 233, y: 551 }]).map_err(anyhow::Error::msg)?;
            wait(800);
            input::run(w.hwnd, &[Step::Hover { x: 120, y: 605 }]).map_err(anyhow::Error::msg)?;
            wait(600);
            shot("2clique1");
            input::run(w.hwnd, &[Step::Click { x: 233, y: 551 }]).map_err(anyhow::Error::msg)?;
            wait(800);
            input::run(w.hwnd, &[Step::Hover { x: 120, y: 605 }]).map_err(anyhow::Error::msg)?;
            wait(600);
            shot("3clique2");
            input::run(w.hwnd, &[Step::Key(0x59)]).map_err(anyhow::Error::msg)?;
            wait(1000);
            shot("4fechada");
            writeln!(out, "fim: helper={:?}", nav::helper_state(w.hwnd))?;
        }
        "nav-estado" => {
            use zen_agent::win::{nav,windows_list};
            let name=args.get(1).ok_or_else(||anyhow::anyhow!("falta personagem"))?;
            let w=windows_list::find_by_class("The Classic MU Season 2").into_iter()
                .find(|w|zen_core::title::parse(&w.title).name.as_deref()==Some(name.as_str())).ok_or_else(||anyhow::anyhow!("personagem não encontrado"))?;
            let (map,pos)=nav::observe_named(w.hwnd);
            writeln!(out,"{}",serde_json::json!({"name":name,"pid":w.pid,"map":map,"pos":pos,"helper":nav::helper_state(w.hwnd)}))?;
        }
        "nav-teleporte" => {
            // Preparação explícita do teste de rotas: apenas o Move legado, sem caminhada.
            use zen_agent::win::{input,nav,windows_list,capture};
            let name=args.get(1).ok_or_else(||anyhow::anyhow!("falta personagem"))?;
            let world=args.get(2).ok_or_else(||anyhow::anyhow!("falta World"))?.parse::<u8>()?;
            let dest=nav::Dest::from_world(world).ok_or_else(||anyhow::anyhow!("mapa sem Move validado"))?;
            let w=windows_list::find_by_class("The Classic MU Season 2").into_iter()
                .find(|w|zen_core::title::parse(&w.title).name.as_deref()==Some(name.as_str())).ok_or_else(||anyhow::anyhow!("personagem não encontrado"))?;
            input::start_manual_guard(180);input::set_paused(false);
            let _session=input::Session::begin(w.hwnd).map_err(anyhow::Error::msg)?;
            let at=nav::teleport(w.hwnd,dest).map_err(anyhow::Error::msg)?;
            writeln!(out,"Move confirmado: {name} World{world} {at:?}")?;
            if let Some(png)=capture::capture(w.hwnd,None).and_then(|i|i.to_png()){std::fs::write("nav-teleporte-fim.png",png)?;}
        }
        "spot-leitura" => {
            // Somente leitura: não envia input, não muda foco e não grava spots.
            use zen_agent::win::{capture, nav, windows_list};
            for w in windows_list::find_by_class("The Classic MU Season 2") {
                writeln!(out, "pid={} titulo={}", w.pid, w.title)?;
                for sample in 1..=5 {
                    let t = std::time::Instant::now();
                    let frame = capture::visible_region(w.hwnd, nav::CORNER);
                    let visible = frame.is_some();
                    let copy_us = t.elapsed().as_micros();
                    let snap = if visible {
                        nav::decode_spot_frame(frame)
                    } else {
                        nav::observe_spot_now(w.hwnd)
                    };
                    writeln!(out, "amostra={sample} recorte_visivel={visible} copia_us={copy_us} total_us={} mapa={:?} pos={:?}", t.elapsed().as_micros(), snap.0, snap.1)?;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let t = std::time::Instant::now();
                let old = nav::observe_named(w.hwnd);
                writeln!(out, "captura_completa_us={} mapa={:?} pos={:?}", t.elapsed().as_micros(), old.0, old.1)?;
            }
        }
        "janelas" => {
            for w in zen_agent::win::windows_list::find_by_class("The Classic MU Season 2") {
                writeln!(out, "pid={} hwnd={:#x} titulo={}", w.pid, w.hwnd, w.title)?;
            }
        }
        "ocr" => {
            // Mostra o que o OCR enxerga e salva as imagens, para calibrar a região do Zen.
            // Uso: zen-diag ocr            (região padrão do agent.toml: 786 609 78 14)
            //      zen-diag ocr x y w h    (outra região, em coordenadas da janela inteira)
            // Salva ocr_<pid>_janela.png (janela inteira: meça a região AQUI) e
            //       ocr_<pid>_recorte.png (exatamente o que o OCR recebe).
            use zen_agent::win::{capture, ocr::Ocr};
            let region: [i32; 4] = if args.len() >= 5 {
                [args[1].parse()?, args[2].parse()?, args[3].parse()?, args[4].parse()?]
            } else {
                [786, 609, 78, 14]
            };
            writeln!(out, "região: {region:?} (x, y, largura, altura na janela inteira)")?;
            let ocr = Ocr::new()?;
            let wins = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2");
            if wins.is_empty() {
                writeln!(out, "nenhuma janela com a classe do jogo")?;
            }
            for w in wins {
                writeln!(out, "\npid={} titulo={}", w.pid, w.title)?;
                let Some(full) = capture::capture(w.hwnd, None) else {
                    writeln!(out, "  captura falhou (janela minimizada?)")?;
                    continue;
                };
                if let Some(png) = full.to_png() {
                    std::fs::write(format!("ocr_{}_janela.png", w.pid), png)?;
                }
                writeln!(out, "  janela {}x{} salva em ocr_{}_janela.png", full.w, full.h, w.pid)?;
                let Some(cut) = full.crop(region) else {
                    writeln!(out, "  região fora da janela")?;
                    continue;
                };
                // Leitura que o AGENTE usa: modelo de pixels da fonte do jogo.
                let bits = zen_core::glyphs::binarize_bgra(&cut.bgra, cut.w as usize, cut.h as usize);
                writeln!(out, "  recorte binarizado (# = tinta):")?;
                for y in 0..bits.h {
                    let row: String = (0..bits.w)
                        .map(|x| if bits.get(x as isize, y as isize) { '#' } else { '.' })
                        .collect();
                    writeln!(out, "    {row}")?;
                }
                writeln!(
                    out,
                    "  => Zen lido (modelo de pixels, o que o agente usa): {:?}",
                    zen_core::glyphs::read_number(&bits)
                )?;
                // Só para comparação: OCR do Windows no mesmo recorte tratado.
                let prepared = cut.for_ocr();
                if let Some(png) = prepared.to_png() {
                    std::fs::write(format!("ocr_{}_recorte.png", w.pid), png)?;
                }
                let lines = ocr.lines(&prepared).unwrap_or_default();
                writeln!(out, "  (comparação) OCR do Windows: {lines:?}")?;
            }
        }
        "vida-coleta" => {
            // Só TELA: guarda o recorte do número de HP (esfera vermelha) de cada char sempre
            // que ele muda, para montar os modelos dos dígitos. Não abre a memória do jogo.
            // Uso: zen-diag vida-coleta [segundos] [x y w h]
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use zen_agent::win::capture;
            let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(600);
            let region: [i32; 4] = if args.len() >= 6 {
                [args[2].parse()?, args[3].parse()?, args[4].parse()?, args[5].parse()?]
            } else {
                [165, 752, 100, 36]
            };
            let dir = std::path::Path::new("coleta_vida");
            std::fs::create_dir_all(dir)?;
            let mut last: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
            let mut saved = 0usize;
            let mut full_done: std::collections::HashSet<u32> = std::collections::HashSet::new();
            let t0 = std::time::Instant::now();
            while t0.elapsed().as_secs() < secs {
                for w in zen_agent::win::windows_list::find_by_class("The Classic MU Season 2") {
                    let name = zen_core::title::parse(&w.title)
                        .name
                        .unwrap_or_else(|| format!("pid{}", w.pid));
                    if full_done.insert(w.pid) {
                        if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                            std::fs::write(dir.join(format!("{name}_janela.png")), png)?;
                        }
                    }
                    let Some(img) = capture::capture(w.hwnd, Some(region)) else {
                        continue;
                    };
                    let mut h = DefaultHasher::new();
                    img.bgra.hash(&mut h);
                    let hv = h.finish();
                    if last.get(&w.pid) == Some(&hv) {
                        continue;
                    }
                    last.insert(w.pid, hv);
                    if let Some(png) = img.to_png() {
                        saved += 1;
                        std::fs::write(dir.join(format!("{name}_{saved:04}.png")), png)?;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
            writeln!(
                out,
                "região {region:?}: {saved} recortes diferentes salvos em coleta_vida\\"
            )?;
        }
        "vida-mem" => {
            // Procura onde o HP mora na memória. SÓ LEITURA. Semente = número de HP da tela.
            // Cada vez que o HP da tela muda (e fica igual em 2 leituras seguidas), relê os
            // candidatos e descarta quem não acompanhou. Testa u16 (passo 2) e u32 (passo 4).
            // Uso: zen-diag vida-mem [segundos] [nome do char]
            // Precisa que o char tome dano / recupere vida enquanto roda (farmando já basta).
            use zen_agent::win::capture;
            let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
            let only = args.get(2).cloned();
            let region: [i32; 4] = [165, 752, 100, 36];

            #[derive(Clone, Copy)]
            struct Cand {
                addr: u64,
                wide: bool, // u32; senão u16
                hits: u16,
                miss: u8,
            }
            struct St {
                name: String,
                pid: u32,
                hwnd: isize,
                prev_read: Option<u32>,
                last_used: Option<u32>,
                seeded: bool,
                cands: Vec<Cand>,
                changes: u32,
            }
            const CHUNK: usize = 1 << 20;
            const MAX_SEED: usize = 8_000_000;

            let mut sts: Vec<St> = Vec::new();
            for w in zen_agent::win::windows_list::find_by_class("The Classic MU Season 2") {
                let name = zen_core::title::parse(&w.title).name.unwrap_or_else(|| format!("pid{}", w.pid));
                if only.as_deref().is_some_and(|o| !o.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                sts.push(St { name, pid: w.pid, hwnd: w.hwnd, prev_read: None, last_used: None, seeded: false, cands: vec![], changes: 0 });
            }
            if sts.is_empty() {
                writeln!(out, "nenhuma janela do jogo (ou nenhum char com esse nome)")?;
            }
            let t0 = std::time::Instant::now();
            let mut buf = vec![0u8; CHUNK];
            while t0.elapsed().as_secs() < secs && !sts.is_empty() {
                for st in sts.iter_mut() {
                    let hp = capture::capture(st.hwnd, Some(region)).and_then(|img| {
                        let b = zen_core::hp::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
                        zen_core::hp::read_hp(&b)
                    });
                    let stable = hp.is_some() && hp == st.prev_read;
                    st.prev_read = hp;
                    let Some(hp) = hp.filter(|_| stable) else { continue };
                    if st.last_used == Some(hp) {
                        continue;
                    }
                    st.last_used = Some(hp);
                    let Some(p) = Proc::open(st.pid) else {
                        writeln!(out, "{}: não abriu o processo (rode como admin)", st.name)?;
                        continue;
                    };
                    let t = std::time::Instant::now();
                    if !st.seeded {
                        let (v16, v32) = (hp as u16, hp);
                        'seed: for (base, size) in p.writable_regions() {
                            let mut off = 0usize;
                            while off < size {
                                let n = CHUNK.min(size - off);
                                let at = base + off as u64;
                                if p.read(at, &mut buf[..n]) {
                                    for i in (0..n.saturating_sub(1)).step_by(2) {
                                        if u16::from_le_bytes([buf[i], buf[i + 1]]) == v16 && hp <= 0xFFFF {
                                            st.cands.push(Cand { addr: at + i as u64, wide: false, hits: 1, miss: 0 });
                                        }
                                        if i % 4 == 0 && i + 4 <= n
                                            && u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) == v32
                                        {
                                            st.cands.push(Cand { addr: at + i as u64, wide: true, hits: 1, miss: 0 });
                                        }
                                    }
                                }
                                off += n;
                                if st.cands.len() > MAX_SEED {
                                    break 'seed;
                                }
                            }
                        }
                        st.seeded = true;
                        writeln!(out, "{}: semente HP {hp}: {} candidatos em {:?}", st.name, st.cands.len(), t.elapsed())?;
                    } else {
                        st.changes += 1;
                        // Relê só os blocos onde há candidatos (candidatos ficam em ordem de endereço).
                        let mut i = 0usize;
                        while i < st.cands.len() {
                            let start = st.cands[i].addr;
                            let mut j = i;
                            while j < st.cands.len() && st.cands[j].addr < start + CHUNK as u64 {
                                j += 1;
                            }
                            // Se o bloco passar do fim da região, a leitura falha e cai para
                            // leitura candidato a candidato (mais lenta, mas certa).
                            let ok = p.read(start, &mut buf[..CHUNK]);
                            for c in &mut st.cands[i..j] {
                                let v = if ok {
                                    let o = (c.addr - start) as usize;
                                    if c.wide {
                                        (o + 4 <= CHUNK).then(|| u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]))
                                    } else {
                                        (o + 2 <= CHUNK).then(|| u16::from_le_bytes([buf[o], buf[o + 1]]) as u32)
                                    }
                                } else {
                                    let mut b4 = [0u8; 4];
                                    let n = if c.wide { 4 } else { 2 };
                                    p.read(c.addr, &mut b4[..n]).then(|| u32::from_le_bytes(b4))
                                };
                                if v == Some(hp) {
                                    c.hits += 1;
                                } else {
                                    c.miss = c.miss.saturating_add(1);
                                }
                            }
                            i = j;
                        }
                        // Uma falha tolerada (a tela e a memória podem estar a 1 tique de distância).
                        st.cands.retain(|c| c.miss < 2);
                        let n32 = st.cands.iter().filter(|c| c.wide).count();
                        writeln!(
                            out,
                            "{}: HP {hp} (mudança {}): {} candidatos ({} u32, {} u16) em {:?}",
                            st.name, st.changes, st.cands.len(), n32, st.cands.len() - n32, t.elapsed()
                        )?;
                    }
                    print!("{}", out.lines().last().map(|l| format!("{l}\n")).unwrap_or_default());
                }
                std::thread::sleep(std::time::Duration::from_millis(700));
            }
            writeln!(out, "\n=== resultado ===")?;
            for st in &sts {
                let mut best: Vec<&Cand> = st.cands.iter().filter(|c| c.miss == 0).collect();
                best.sort_by_key(|c| std::cmp::Reverse(c.hits));
                writeln!(
                    out,
                    "{} (pid {}): {} mudanças de HP vistas, {} candidatos sem nenhuma falha",
                    st.name, st.pid, st.changes, best.len()
                )?;
                if st.changes < 5 {
                    writeln!(out, "  POUCAS MUDANÇAS: o char precisa tomar dano/recuperar vida enquanto roda.")?;
                }
                for c in best.iter().take(40) {
                    writeln!(out, "  {:#x} {} acertos={}", c.addr, if c.wide { "u32" } else { "u16" }, c.hits)?;
                }
            }
        }
        "tecla-input" => {
            // Teste de teclado com SendInput (input padrão do Windows, sem driver). Exige foco:
            // traz a janela do char para a frente, confere que ELA está em foco antes de CADA
            // tecla (nunca digita em outra janela), e devolve o foco no final.
            // Uso: zen-diag tecla-input <nome do char> [V|C]
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
                KEYEVENTF_SCANCODE, VIRTUAL_KEY, VK_MENU,
            };
            use windows::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
            };
            use zen_agent::win::capture;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let key = args.get(2).map(|s| s.to_ascii_uppercase()).unwrap_or_else(|| "V".into());
            let (vk, sc): (u16, u16) = match key.as_str() {
                "V" => (0x56, 0x2F),
                "C" => (0x43, 0x2E),
                _ => anyhow::bail!("só V ou C neste teste"),
            };
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let hwnd = HWND(w.hwnd as *mut _);
            let inv_open = || {
                capture::capture(w.hwnd, Some([786, 609, 78, 14])).and_then(|img| {
                    let b = zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
                    zen_core::glyphs::read_number(&b)
                })
            };
            let kb = |vk: u16, sc: u16, flags: KEYBD_EVENT_FLAGS| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: sc, dwFlags: flags, time: 0, dwExtraInfo: 0 },
                },
            };
            let size = std::mem::size_of::<INPUT>() as i32;
            let is_fg = || unsafe { GetForegroundWindow() == hwnd };
            let previous = unsafe { GetForegroundWindow() };
            writeln!(out, "char {name} pid {} tecla {key}", w.pid)?;

            // Trazer para a frente. O Windows só deixa trocar o foco se o processo "acabou de
            // receber input"; um toque de ALT (sozinho não faz nada no jogo) libera a troca.
            unsafe {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                if !SetForegroundWindow(hwnd).as_bool() {
                    let alt = [kb(VK_MENU.0, 0, KEYBD_EVENT_FLAGS(0)), kb(VK_MENU.0, 0, KEYEVENTF_KEYUP)];
                    SendInput(&alt, size);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !is_fg() {
                writeln!(out, "NÃO consegui trazer a janela para a frente: nenhuma tecla enviada.")?;
            } else {
                writeln!(out, "janela em foco: ok")?;
                let methods: [(&str, Box<dyn Fn() -> [INPUT; 2]>); 2] = [
                    ("SendInput tecla virtual", Box::new(move || [kb(vk, 0, KEYBD_EVENT_FLAGS(0)), kb(vk, 0, KEYEVENTF_KEYUP)])),
                    ("SendInput scan code", Box::new(move || [kb(0, sc, KEYEVENTF_SCANCODE), kb(0, sc, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)])),
                ];
                let press = |ins: [INPUT; 2]| -> bool {
                    if !is_fg() {
                        return false;
                    }
                    unsafe {
                        SendInput(&ins[..1], size);
                        std::thread::sleep(std::time::Duration::from_millis(70));
                        SendInput(&ins[1..], size);
                    }
                    true
                };
                let mut worked = None;
                for (i, (label, mk)) in methods.iter().enumerate() {
                    let before = inv_open();
                    if !press(mk()) {
                        writeln!(out, "foco perdido antes do método {}: parei.", i + 1)?;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    let after = inv_open();
                    if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                        let _ = std::fs::write(format!("tecla-input_{name}_{}.png", i + 1), png);
                    }
                    let changed = before.is_some() != after.is_some();
                    writeln!(out, "método {} ({label}): inventário antes={:?} depois={:?} => {}", i + 1, before, after,
                        if changed { "FUNCIONOU" } else { "sem efeito" })?;
                    if changed {
                        worked = Some(i);
                        break;
                    }
                }
                if let Some(i) = worked {
                    let before = inv_open();
                    let ok = press((methods[i].1)());
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    let after = inv_open();
                    writeln!(out, "desfazendo (enviado={ok}): antes={:?} depois={:?} => {}", before, after,
                        if before.is_some() != after.is_some() { "voltou ao estado original" } else { "NÃO voltou" })?;
                } else {
                    writeln!(out, "SendInput sem efeito: o jogo filtra input injetado. Próximo passo: driver (Interception).")?;
                }
            }
            unsafe {
                if !previous.0.is_null() && previous != hwnd {
                    let _ = SetForegroundWindow(previous);
                }
            }
            writeln!(out, "foco devolvido para a janela anterior")?;
        }
        "tecla" => {
            // Teste de teclado SEM driver e SEM foco: mensagens direto na janela do char.
            // Uso: zen-diag tecla <nome do char> [V|C]
            // Mede pelo efeito: com V, o campo do Zen do inventário aparece/some (o leitor do
            // agente detecta). Tenta 3 jeitos; no primeiro que funcionar, manda de novo para
            // deixar a janela como estava. Salva prints antes/depois de cada tentativa.
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_ACTIVATE, WM_CHAR, WM_KEYDOWN, WM_KEYUP};
            use zen_agent::win::capture;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let key = args.get(2).map(|s| s.to_ascii_uppercase()).unwrap_or_else(|| "V".into());
            let (vk, sc): (usize, isize) = match key.as_str() {
                "V" => (0x56, 0x2F),
                "C" => (0x43, 0x2E),
                _ => anyhow::bail!("só V ou C neste teste"),
            };
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let hwnd = HWND(w.hwnd as *mut _);
            let inv_open = || {
                capture::capture(w.hwnd, Some([786, 609, 78, 14])).and_then(|img| {
                    let b = zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
                    zen_core::glyphs::read_number(&b)
                })
            };
            let shot = |tag: &str| {
                if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("tecla_{name}_{tag}.png"), png);
                }
            };
            let down = LPARAM(1 | (sc << 16));
            let up = LPARAM(1 | (sc << 16) | (1 << 30) | (1 << 31));
            let send = |method: u32| unsafe {
                match method {
                    1 => {
                        let _ = PostMessageW(hwnd, WM_KEYDOWN, WPARAM(vk), down);
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        let _ = PostMessageW(hwnd, WM_KEYUP, WPARAM(vk), up);
                    }
                    2 => {
                        let _ = PostMessageW(hwnd, WM_KEYDOWN, WPARAM(vk), down);
                        let _ = PostMessageW(hwnd, WM_CHAR, WPARAM(vk + 0x20), down);
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        let _ = PostMessageW(hwnd, WM_KEYUP, WPARAM(vk), up);
                    }
                    _ => {
                        // Alguns jogos ignoram teclas quando acham que não estão ativos.
                        let _ = PostMessageW(hwnd, WM_ACTIVATE, WPARAM(1), LPARAM(0));
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        let _ = PostMessageW(hwnd, WM_KEYDOWN, WPARAM(vk), down);
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        let _ = PostMessageW(hwnd, WM_KEYUP, WPARAM(vk), up);
                    }
                }
            };
            let label = ["", "PostMessage KEYDOWN/KEYUP", "PostMessage KEYDOWN/CHAR/KEYUP", "PostMessage ACTIVATE + KEYDOWN/KEYUP"];
            writeln!(out, "char {name} pid {} tecla {key}", w.pid)?;
            let mut worked = None;
            for m in 1..=3u32 {
                let before = inv_open();
                shot(&format!("{m}_antes"));
                send(m);
                std::thread::sleep(std::time::Duration::from_millis(900));
                let after = inv_open();
                shot(&format!("{m}_depois"));
                let changed = before.is_some() != after.is_some();
                writeln!(out, "método {m} ({}): inventário antes={:?} depois={:?} => {}", label[m as usize], before, after,
                    if changed { "FUNCIONOU" } else { "sem efeito" })?;
                if changed && key == "V" {
                    worked = Some(m);
                    break;
                }
                if key != "V" {
                    writeln!(out, "  (tecla {key}: confira os prints tecla_{name}_{m}_*.png)")?;
                }
            }
            if let Some(m) = worked {
                let before = inv_open();
                send(m);
                std::thread::sleep(std::time::Duration::from_millis(900));
                let after = inv_open();
                writeln!(out, "desfazendo com o método {m}: antes={:?} depois={:?} => {}", before, after,
                    if before.is_some() != after.is_some() { "voltou ao estado original" } else { "NÃO voltou" })?;
            } else if key == "V" {
                writeln!(out, "nenhum método por mensagem funcionou: o jogo não lê teclado pela janela. Próximo passo: input de verdade (SendInput/Interception), que exige a janela em foco.")?;
            }
        }
        "mobs-scan" => {
            // SÓ LEITURA. Procura na memória gravável pares f32 (x,y) = tile*100 a até 15 tiles do
            // char que MUDAM sozinhos entre leituras (mobs andando), ignorando os que acompanham o
            // char. Grava a lista em mobs-scan.txt para análise. Uso: zen-diag mobs-scan <nome> [rodadas]
            use zen_agent::win::nav;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let rounds: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let p = zen_agent::win::memory::Proc::open(w.pid).ok_or_else(|| anyhow::anyhow!("não abri o processo"))?;
            let hero = nav::position(w.hwnd).ok_or_else(|| anyhow::anyhow!("não li a posição na tela"))?;
            let (hx, hy) = (hero.0 as f32 * 100.0 + 50.0, hero.1 as f32 * 100.0 + 50.0);
            let near = |x: f32, y: f32| x.is_finite() && y.is_finite() && (x - hx).abs() < 1500.0 && (y - hy).abs() < 1500.0 && x > 0.0 && y > 0.0;
            // 1) semente
            let mut cand: Vec<(u64, f32, f32, u32)> = Vec::new(); // addr, x, y, mudanças
            let mut buf = vec![0u8; 1 << 20];
            for (base, size) in p.writable_regions() {
                let mut off = 0usize;
                while off < size {
                    let n = buf.len().min(size - off);
                    if p.read(base + off as u64, &mut buf[..n]) {
                        let mut i = 0;
                        while i + 8 <= n {
                            let x = f32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
                            let y = f32::from_le_bytes([buf[i + 4], buf[i + 5], buf[i + 6], buf[i + 7]]);
                            if near(x, y) && cand.len() < 2_000_000 {
                                cand.push((base + (off + i) as u64, x, y, 0));
                            }
                            i += 4;
                        }
                    }
                    off += n;
                }
            }
            writeln!(out, "{name}: char em {hero:?}; {} pares perto na semente", cand.len())?;
            // 2) rodadas: quem muda sem ser o char
            for r in 0..rounds {
                std::thread::sleep(std::time::Duration::from_millis(1200));
                let h2 = nav::position(w.hwnd).unwrap_or(hero);
                let mut v = [0u8; 8];
                for c in cand.iter_mut() {
                    if p.read(c.0, &mut v) {
                        let x = f32::from_le_bytes([v[0], v[1], v[2], v[3]]);
                        let y = f32::from_le_bytes([v[4], v[5], v[6], v[7]]);
                        if near(x, y) {
                            if (x, y) != (c.1, c.2) {
                                let follows = ((x / 100.0) as i32 - h2.0).abs() <= 1 && ((y / 100.0) as i32 - h2.1).abs() <= 1;
                                if !follows {
                                    c.3 += 1;
                                }
                            }
                            c.1 = x;
                            c.2 = y;
                        } else {
                            c.3 = 0;
                            c.1 = f32::NAN;
                        }
                    }
                }
                cand.retain(|c| c.1.is_finite());
                let moving = cand.iter().filter(|c| c.3 > 0).count();
                writeln!(out, "rodada {}: char {h2:?}; {} pares vivos, {moving} mudaram longe do char", r + 1, cand.len())?;
            }
            let mut moving: Vec<_> = cand.iter().filter(|c| c.3 >= 2).collect();
            moving.sort_by_key(|c| c.0);
            let mut txt = String::new();
            for c in &moving {
                txt.push_str(&format!("{:#x} {} {:.2} {:.2}\n", c.0, c.3, c.1 / 100.0, c.2 / 100.0));
            }
            std::fs::write("mobs-scan.txt", &txt)?;
            writeln!(out, "{} endereços mudaram em >= 2 rodadas (mobs-scan.txt)", moving.len())?;
            for c in moving.iter().take(40) {
                writeln!(out, "  {:#x} mudou {}x  tile ({:.1}, {:.1})", c.0, c.3, c.1 / 100.0, c.2 / 100.0)?;
            }
        }
        "ice" => {
            // Confere o Ice Storm pelo ícone e acha a tecla dele (aperta 1..9 se precisar; NÃO
            // solta a skill). Uso: zen-diag ice <nome do char> [tecla para trocar antes]
            use zen_agent::win::{input, nav};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let _s = input::Session::begin(w.hwnd).map_err(|e| anyhow::anyhow!(e))?;
            nav::set_char_name(w.hwnd, &name);
            if let Some(k) = args.get(2).and_then(|v| v.parse::<u8>().ok()) {
                input::run(w.hwnd, &[Step::Key(0x30 + u16::from(k % 10))]).map_err(|e| anyhow::anyhow!(e))?;
                std::thread::sleep(std::time::Duration::from_millis(300));
                writeln!(out, "tecla {k} apertada antes; Ice Storm selecionado agora: {:?}", nav::selected_is_ice(w.hwnd))?;
            } else {
                writeln!(out, "Ice Storm selecionado agora: {:?}", nav::selected_is_ice(w.hwnd))?;
            }
            let (key, aim) = nav::probe_ice(w.hwnd);
            writeln!(out, "tecla do Ice Storm: {key:?}; selecionado depois: {:?}", nav::selected_is_ice(w.hwnd))?;
            if let Some((p, t, n)) = aim {
                writeln!(out, "char em {p:?}: miraria em {t:?} pegando {n} mob(s) (alcance 6, raio 4)")?;
            }
        }
        "objetos" => {
            // SÓ LEITURA. Mobs/NPCs/jogadores em volta do char, como o agente lê.
            // Uso: zen-diag objetos <nome do char>
            use zen_agent::win::{nav, objects};
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let pos = nav::position(w.hwnd).ok_or_else(|| anyhow::anyhow!("sem posição na tela"))?;
            let t0 = std::time::Instant::now();
            let v = objects::nearby(w.pid, &name, pos).ok_or_else(|| anyhow::anyhow!("não achei a entrada do char na memória"))?;
            writeln!(out, "{name} em {pos:?}: {} objeto(s) vivos em {:?}", v.len(), t0.elapsed())?;
            for o in &v {
                writeln!(out, "  {:?} {} classe {} em ({}, {})", o.kind, o.name, o.class, o.x, o.y)?;
            }
            let t1 = std::time::Instant::now();
            let _ = objects::nearby(w.pid, &name, pos);
            writeln!(out, "segunda leitura (com o endereço guardado): {:?}", t1.elapsed())?;
        }
        "goldens" => {
            // SÓ LEITURA. Vigia os monstros em volta do char por N segundos (padrão 1500) e registra
            // tudo o que tiver "gold" no nome, com classe e posição; no fim, todos os monstros vistos.
            // Uso: zen-diag goldens <nome do char> [segundos]
            use zen_agent::win::{nav, objects};
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let secs: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1500);
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let t0 = std::time::Instant::now();
            let mut vistos: std::collections::BTreeMap<(String, u16), u32> = Default::default();
            let mut ultima = String::new();
            while t0.elapsed().as_secs() < secs {
                let s = t0.elapsed().as_secs();
                match nav::position(w.hwnd) {
                    None => { if ultima != "sempos" { writeln!(out, "{s:>5}s sem posição na tela")?; ultima = "sempos".into(); } }
                    Some(pos) => match objects::nearby(w.pid, &name, pos) {
                        None => { if ultima != "semlista" { writeln!(out, "{s:>5}s {pos:?} lista de objetos não achada")?; ultima = "semlista".into(); } }
                        Some(v) => {
                            for o in v.iter().filter(|o| o.kind == zen_core::objects::Kind::Monster) {
                                *vistos.entry((o.name.clone(), o.class)).or_default() += 1;
                            }
                            let g: Vec<String> = v.iter().filter(|o| o.name.to_ascii_lowercase().contains("gold"))
                                .map(|o| format!("{} classe {} {:?} em ({}, {}) dist {}", o.name, o.class, o.kind, o.x, o.y, (o.x - pos.0).abs().max((o.y - pos.1).abs()))).collect();
                            let mapa = nav::observe_named(w.hwnd).0.unwrap_or("?");
                            let linha = format!("{pos:?} {} objs, goldens: {}", v.len(), if g.is_empty() { "-".into() } else { g.join(" | ") });
                            if linha != ultima {
                                writeln!(out, "{s:>5}s {mapa} {linha}")?;
                                ultima = linha;
                            }
                        }
                    },
                }
                std::fs::write("resultado_goldens.txt", &out)?;
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            writeln!(out, "\n=== monstros vistos (nome, classe, leituras) ===")?;
            for ((n, c), k) in &vistos {
                writeln!(out, "{n} classe {c}: {k}")?;
            }
        }
        "caca-golden" => {
            // Goldens etapa 3/4 (TESTE). Regra do dono (18/11 06:46): andar SEMPRE com o helper
            // desligado; chegar colado no golden e só então ligar (range 6 do helper; o golden é
            // atraído quando o char ataca). O helper pega a caixa sozinho.
            // Uso: zen-diag caca-golden <nome> <segundos> [raio]
            use zen_agent::win::{nav, objects};
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let secs: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1200);
            let raio: i32 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(40);
            // Modo posto (dono, 18/11 09:04): fica parado num tile com o helper ligado e só anda
            // quando um golden aparece a até 12 tiles. Uso: caca-golden <nome> <seg> posto <x> <y>
            let posto: Option<(i32, i32)> = (args.get(3).map(|s| s.as_str()) == Some("posto"))
                .then(|| Some((args.get(4)?.parse().ok()?, args.get(5)?.parse().ok()?)))
                .flatten();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            // Sem isto o andar não lia os mobs da memória (desvio e skill às cegas): foi o caso
            // das 08:00 de 18/11.
            nav::set_char_name(w.hwnd, &name);
            let t = nav::world_terrain_of(w.hwnd, 1).map_err(|e| anyhow::anyhow!(e))?;
            let t0 = std::time::Instant::now();
            let log = |out: &mut String, m: String| {
                let l = format!("{:>5}s {m}\n", t0.elapsed().as_secs());
                out.push_str(&l);
                let _ = std::fs::write("resultado_caca-golden.txt", out.as_bytes());
            };
            let cheb = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
            // Anel: 16 direções em volta da cidade, só pontos andáveis fora da safe e ligados ao
            // anterior por linha reta livre (sem cerca no meio). Pontos intermediários a cada ~10
            // tiles: trecho curto falha rápido e a memória é lida entre um trecho e outro.
            let centro = (135i32, 128i32);
            let mut base: Vec<(i32, i32)> = Vec::new();
            for k in 0..16 {
                let a = k as f64 * std::f64::consts::TAU / 16.0;
                for dr in [0, 4, -4, 8, -8, 12] {
                    let r = (raio + dr) as f64;
                    let p = (centro.0 + (r * a.cos()).round() as i32, centro.1 + (r * a.sin()).round() as i32);
                    if (1..255).contains(&p.0) && (1..255).contains(&p.1) && t.walkable(p.0, p.1) && !t.is_safezone(p.0, p.1) {
                        base.push(p);
                        break;
                    }
                }
            }
            // Só pontos ligados à cidade a pé (sem bolsos murados como o de (173,114)).
            let alcance = {
                let mut src = t.safe_tiles();
                src.push((116, 127));
                t.distances_from(&src)
            };
            base.retain(|p| alcance[p.1 as usize * 256 + p.0 as usize] != u16::MAX);
            let mut anel: Vec<(i32, i32)> = Vec::new();
            for (i, &p) in base.iter().enumerate() {
                let q = base[(i + 1) % base.len()];
                anel.push(p);
                if t.segment_clear(p, q) {
                    let n = (cheb(p, q) / 10).max(1);
                    for k in 1..n {
                        anel.push((p.0 + (q.0 - p.0) * k / n, p.1 + (q.1 - p.1) * k / n));
                    }
                }
            }
            // Patrulha por cobertura: células de 16 tiles até `raio` tiles do centro da cidade, cada
            // uma com um tile andável e alcançável a pé; o alvo é a célula nunca vista (ou a
            // mais antiga) mais perto. O alcance de visão (12) é o que a lista de objetos mostrou
            // em volta do char em Lorencia (objetos a até ~13 tiles).
            let mut cob = zen_core::navegacao::Coverage::new(16, 12);
            let mut cells: Vec<((i32, i32), (i32, i32))> = Vec::new();
            for ky in 0..16 {
                for kx in 0..16 {
                    let ctr = cob.center((kx, ky));
                    if cheb(ctr, centro) > raio + 8 { continue; }
                    let mut achou = None;
                    'busca: for r in 0..7 {
                        for dy in -r..=r {
                            for dx in -r..=r {
                                let q = (ctr.0 + dx, ctr.1 + dy);
                                if (0..256).contains(&q.0) && (0..256).contains(&q.1) && t.walkable(q.0, q.1) && !t.is_safezone(q.0, q.1)
                                    && alcance[q.1 as usize * 256 + q.0 as usize] != u16::MAX {
                                    achou = Some(q);
                                    break 'busca;
                                }
                            }
                        }
                    }
                    if let Some(q) = achou { cells.push(((kx, ky), q)); }
                }
            }
            let mut historico = std::fs::OpenOptions::new().create(true).append(true).open("goldens_historico.csv").ok();
            if let Some(p) = posto {
                anel = vec![p];
                log(&mut out, format!("modo posto em {p:?}"));
            } else {
                log(&mut out, format!("patrulha por cobertura: {} células de 16 tiles até {raio} tiles da cidade", cells.len()));
            }
            let status = |_m: String| {};
            let ler = |pos: (i32, i32)| objects::nearby(w.pid, &name, pos).unwrap_or_default();
            // Começa no ponto do anel mais perto do char.
            let mut vistos: std::collections::BTreeMap<String, u32> = Default::default();
            let mut falhas_seguidas = 0;
            'caca: while t0.elapsed().as_secs() < secs {
                let Some(pos) = nav::position(w.hwnd) else {
                    log(&mut out, "sem posição na tela".into());
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                };
                if nav::observe_named(w.hwnd).0 != Some("Lorencia") {
                    log(&mut out, format!("não está em Lorencia ({:?}); parei", nav::observe_named(w.hwnd).0));
                    break;
                }
                let objs = ler(pos);
                if let Some(b) = objs.iter().find(|o| zen_core::objects::is_golden_boss(o) && cheb((o.x, o.y), pos) <= 12) {
                    log(&mut out, format!("CHEFE {} classe {} em ({}, {}): fugindo para a safe e parando", b.name, b.class, b.x, b.y));
                    let _ = nav::set_helper(w.hwnd, false);
                    let _ = nav::walk_near(w.hwnd, &t, (123, 129), &status);
                    break;
                }
                let mut gs: Vec<_> = objs.iter().filter(|o| zen_core::objects::is_golden(o) && (posto.is_none() || cheb((o.x, o.y), pos) <= 12)).cloned().collect();
                gs.sort_by_key(|o| cheb((o.x, o.y), pos));
                if let Some(g) = gs.first().cloned() {
                    *vistos.entry(format!("{} classe {}", g.name, g.class)).or_default() += 1;
                    if let Some(f) = historico.as_mut() {
                        use std::io::Write;
                        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                        let _ = writeln!(f, "{ts};{};{};{};{};confirmado na memória;{name}", g.name, g.class, g.x, g.y);
                    }
                    log(&mut out, format!("GOLDEN {} classe {} em ({}, {}), char em {pos:?}, dist {}", g.name, g.class, g.x, g.y, cheb((g.x, g.y), pos)));
                    // Chegar colado (até 2 tiles) com o helper DESLIGADO; até 3 tentativas porque ele se mexe.
                    let mut alvo = (g.x, g.y);
                    let mut perto = cheb(alvo, pos) <= 2;
                    for tentativa in 0..3 {
                        if perto { break; }
                        let _ = nav::set_helper(w.hwnd, false);
                        let r = nav::walk_near(w.hwnd, &t, alvo, &status);
                        let p = nav::position(w.hwnd).unwrap_or(pos);
                        let agora = ler(p);
                        let Some(g2) = agora.iter().filter(|o| zen_core::objects::is_golden(o)).min_by_key(|o| cheb((o.x, o.y), p)) else {
                            log(&mut out, format!("  golden sumiu durante a aproximação ({r:?})"));
                            continue 'caca;
                        };
                        alvo = (g2.x, g2.y);
                        perto = cheb(alvo, p) <= 2;
                        log(&mut out, format!("  aproximação {}: char em {p:?}, golden em {alvo:?}, dist {}", tentativa + 1, cheb(alvo, p)));
                    }
                    // Colado: liga o helper e acompanha até o golden sumir (morto ou foi embora).
                    let _ = nav::set_helper(w.hwnd, true);
                    log(&mut out, "  helper LIGADO perto do golden".into());
                    let t_luta = std::time::Instant::now();
                    while t_luta.elapsed().as_secs() < 120 {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        let Some(p) = nav::position(w.hwnd) else { continue };
                        let agora = ler(p);
                        if agora.iter().any(|o| zen_core::objects::is_golden_boss(o) && cheb((o.x, o.y), p) <= 12) {
                            continue 'caca; // o topo do laço foge
                        }
                        match agora.iter().filter(|o| zen_core::objects::is_golden(o)).min_by_key(|o| cheb((o.x, o.y), p)) {
                            None => {
                                log(&mut out, format!("  golden sumiu da tela após {} s (morto ou fugiu); helper fica ligado 6 s para pegar a caixa", t_luta.elapsed().as_secs()));
                                std::thread::sleep(std::time::Duration::from_secs(6));
                                continue 'caca;
                            }
                            Some(g3) if cheb((g3.x, g3.y), p) > 6 => {
                                log(&mut out, format!("  golden se afastou para ({}, {}) (dist {}): reaproximando", g3.x, g3.y, cheb((g3.x, g3.y), p)));
                                continue 'caca;
                            }
                            _ => {}
                        }
                    }
                    log(&mut out, "  2 min lutando sem resolver: volta à patrulha".into());
                    continue;
                }
                if let Some(p) = posto {
                    // Parado no posto com o helper ligado; longe do posto (o helper arrastou ou a
                    // perseguição levou) volta andando com o helper desligado.
                    if cheb(p, pos) > 4 {
                        let _ = nav::set_helper(w.hwnd, false);
                        match nav::walk_near(w.hwnd, &t, p, &status) {
                            Ok(q) => log(&mut out, format!("de volta ao posto {q:?}")),
                            Err(e) => {
                                log(&mut out, format!("volta ao posto falhou: {e}"));
                                std::thread::sleep(std::time::Duration::from_secs(3));
                            }
                        }
                    }
                    let _ = nav::set_helper(w.hwnd, true);
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    continue;
                }
                // Sem golden: próxima célula da cobertura (nunca vista ou mais antiga, perto),
                // helper desligado. Trecho que falha marca a célula por 3 min (não repete o
                // caminho que acabou de falhar).
                let agora = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                cob.mark(pos, agora());
                let dist = t.distances_from(&[pos]);
                let reach = |q: (i32, i32)| { let d = dist[q.1 as usize * 256 + q.0 as usize]; (d != u16::MAX).then_some(d as u32) };
                let Some(alvo) = cob.next_target(agora(), 180, &cells, &reach) else {
                    log(&mut out, "patrulha: nenhuma célula alcançável; esperando".into());
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    continue;
                };
                let _ = nav::set_helper(w.hwnd, false);
                let t_trecho = std::time::Instant::now();
                let rota = t.path(pos, alvo).or_else(|| t.path_escape(pos, alvo)).unwrap_or_default();
                match nav::walk_near(w.hwnd, &t, alvo, &status) {
                    Ok(p) => {
                        falhas_seguidas = 0;
                        for q in rota.iter().step_by(6) { cob.mark(*q, agora()); }
                        cob.mark(p, agora());
                        log(&mut out, format!("patrulha: {pos:?} -> {p:?} em {} s; cobertura {:.0}%", t_trecho.elapsed().as_secs(), 100.0 * cob.covered(agora(), 900, &cells)));
                    }
                    Err(e) => {
                        falhas_seguidas += 1;
                        cob.mark_failed(alvo, agora());
                        if let Some(p) = nav::position(w.hwnd) { cob.mark(p, agora()); }
                        log(&mut out, format!("patrulha: {alvo:?} falhou em {} s: {e}", t_trecho.elapsed().as_secs()));
                        std::thread::sleep(std::time::Duration::from_secs(if falhas_seguidas >= 3 { 5 } else { 1 }));
                    }
                }
            }
            let _ = nav::set_helper(w.hwnd, false);
            log(&mut out, format!("fim. goldens vistos: {vistos:?}"));
        }
        "nome-pos" => {
            // SÓ LEITURA. Para cada cópia do nome do char na memória, procura a posição atual dele
            // (f32 tile*100, u16 ou u8) em até 0x1000 bytes antes/depois. Mostra o deslocamento
            // nome -> posição: é o formato da entrada de personagem (vale para mobs e NPCs).
            // Uso: zen-diag nome-pos <nome do char>
            use zen_agent::win::nav;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let p = zen_agent::win::memory::Proc::open(w.pid).ok_or_else(|| anyhow::anyhow!("não abri o processo"))?;
            let pat: Vec<u8> = name.as_bytes().iter().copied().chain(Some(0)).collect();
            let mut hits = Vec::new();
            let mut buf = vec![0u8; 1 << 20];
            for (base, size) in p.writable_regions() {
                let mut off = 0usize;
                while off < size {
                    let n = buf.len().min(size - off);
                    if p.read(base + off as u64, &mut buf[..n]) {
                        for i in 0..n.saturating_sub(pat.len()) {
                            if buf[i..i + pat.len()] == pat[..] && hits.len() < 400 {
                                hits.push(base + (off + i) as u64);
                            }
                        }
                    }
                    off += n.saturating_sub(pat.len()).max(1);
                }
            }
            let hero = nav::position(w.hwnd).ok_or_else(|| anyhow::anyhow!("sem posição na tela"))?;
            writeln!(out, "{name} em {hero:?}: {} cópias do nome", hits.len())?;
            let mut win = vec![0u8; 0x2000];
            for &a in &hits {
                let start = a.saturating_sub(0x1000);
                if !p.read(start, &mut win) {
                    continue;
                }
                let mut found = Vec::new();
                for i in (0..win.len() - 8).step_by(1) {
                    let rel = i as i64 - 0x1000;
                    if i % 4 == 0 {
                        let x = f32::from_le_bytes([win[i], win[i + 1], win[i + 2], win[i + 3]]);
                        let y = f32::from_le_bytes([win[i + 4], win[i + 5], win[i + 6], win[i + 7]]);
                        if x.is_finite() && y.is_finite() && (x / 100.0) as i32 == hero.0 && (y / 100.0) as i32 == hero.1 {
                            found.push(format!("f32@{rel:+#x}"));
                        }
                    }
                    if win[i] as i32 == hero.0 && win[i + 1] as i32 == hero.1 && win[i + 2] == 0 {
                        found.push(format!("u8@{rel:+#x}"));
                    }
                    if i % 2 == 0 && u16::from_le_bytes([win[i], win[i + 1]]) as i32 == hero.0 && u16::from_le_bytes([win[i + 2], win[i + 3]]) as i32 == hero.1 {
                        found.push(format!("u16@{rel:+#x}"));
                    }
                }
                if !found.is_empty() {
                    writeln!(out, "  nome em {a:#x}: {}", found.into_iter().take(12).collect::<Vec<_>>().join(" "))?;
                }
            }
        }
        "texto" => {
            // SÓ LEITURA. Procura um texto ASCII exato na memória gravável do jogo (nomes de char,
            // mob, NPC) para achar a lista de objetos. Uso: zen-diag texto <nome do char> <texto> [máx]
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let pat = args.get(2).ok_or_else(|| anyhow::anyhow!("falta o texto"))?.as_bytes().to_vec();
            let max: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let p = zen_agent::win::memory::Proc::open(w.pid).ok_or_else(|| anyhow::anyhow!("não abri o processo"))?;
            let mut found = 0usize;
            let mut buf = vec![0u8; 1 << 20];
            'r: for (base, size) in p.writable_regions() {
                let mut off = 0usize;
                while off < size {
                    let n = buf.len().min(size - off);
                    if p.read(base + off as u64, &mut buf[..n]) {
                        for i in 0..n.saturating_sub(pat.len()) {
                            // texto inteiro: termina em 0 (string C)
                            if buf[i..i + pat.len()] == pat[..] && buf.get(i + pat.len()) == Some(&0) {
                                writeln!(out, "{:#x}", base + (off + i) as u64)?;
                                found += 1;
                                if found >= max {
                                    break 'r;
                                }
                            }
                        }
                    }
                    off += n.saturating_sub(pat.len()).max(1);
                }
            }
            writeln!(out, "{found} ocorrência(s) de \"{}\"", String::from_utf8_lossy(&pat))?;
        }
        "mem-dump" => {
            // SÓ LEITURA. Copia um trecho da memória do jogo para arquivo (análise offline da
            // lista de objetos: char, mobs, NPCs). Páginas ilegíveis viram zeros.
            // Uso: zen-diag mem-dump <nome> <endereço hex> <tamanho hex> <arquivo>
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let addr = u64::from_str_radix(args.get(2).ok_or_else(|| anyhow::anyhow!("falta o endereço"))?.trim_start_matches("0x"), 16)?;
            let len = usize::from_str_radix(args.get(3).ok_or_else(|| anyhow::anyhow!("falta o tamanho"))?.trim_start_matches("0x"), 16)?;
            let file = args.get(4).ok_or_else(|| anyhow::anyhow!("falta o arquivo"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let p = zen_agent::win::memory::Proc::open(w.pid).ok_or_else(|| anyhow::anyhow!("não abri o processo"))?;
            let mut out_buf = vec![0u8; len];
            let mut ok_pages = 0;
            for off in (0..len).step_by(4096) {
                let n = 4096.min(len - off);
                if p.read(addr + off as u64, &mut out_buf[off..off + n]) {
                    ok_pages += 1;
                }
            }
            std::fs::write(&file, &out_buf)?;
            writeln!(out, "{name} pid {}: {ok_pages} páginas lidas de {:#x} (+{len:#x}) -> {file}", w.pid, addr)?;
        }
        "pos-coleta" => {
            // Parte 1 (descoberta): onde moram X, Y e o mapa na memória. SÓ LEITURA.
            // Usa a trava da vida (travas.json) como âncora: grava 128 KiB em volta do HP e, no
            // mesmo instante, o recorte do canto da tela com "Mapa (X, Y)". Depois cruzamos.
            // Uso: zen-diag pos-coleta [segundos] [x y w h do recorte]
            use zen_agent::win::{capture, memory::Proc};
            let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
            let region: [i32; 4] = if args.len() >= 6 {
                [args[2].parse()?, args[3].parse()?, args[4].parse()?, args[5].parse()?]
            } else {
                [0, 30, 300, 24]
            };
            let locks: Vec<zen_agent::monitor::SavedLock> =
                serde_json::from_str(&std::fs::read_to_string("travas.json").map_err(|e| anyhow::anyhow!("travas.json: {e}"))?)?;
            let dir = std::path::Path::new("coleta_pos");
            std::fs::create_dir_all(dir)?;
            let mut index = String::new();
            let mut last: std::collections::HashMap<u32, u64> = Default::default();
            let mut count: std::collections::HashMap<u32, usize> = Default::default();
            let wins = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2");
            for l in &locks {
                if let Some(w) = wins.iter().find(|w| w.pid == l.pid) {
                    if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                        std::fs::write(dir.join(format!("{}_janela.png", l.name)), png)?;
                    }
                }
            }
            let t0 = std::time::Instant::now();
            const SPAN: u64 = 0x10000;
            while t0.elapsed().as_secs() < secs {
                for l in &locks {
                    let Some(hp) = l.hp_addr else { continue };
                    let Some(w) = wins.iter().find(|w| w.pid == l.pid) else { continue };
                    let n = count.entry(l.pid).or_default();
                    if *n >= 30 {
                        continue;
                    }
                    let Some(img) = capture::capture(w.hwnd, Some(region)) else { continue };
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    img.bgra.hash(&mut h);
                    let hv = h.finish();
                    if last.get(&l.pid) == Some(&hv) {
                        continue;
                    }
                    let Some(p) = Proc::open(l.pid) else { continue };
                    let base = hp - SPAN;
                    let mut buf = vec![0u8; (2 * SPAN) as usize];
                    // Por páginas: uma página ilegível não derruba o resto (fica com zeros).
                    for off in (0..buf.len()).step_by(0x1000) {
                        let _ = p.read(base + off as u64, &mut buf[off..off + 0x1000]);
                    }
                    last.insert(l.pid, hv);
                    *n += 1;
                    let tag = format!("{}_{:02}", l.name, n);
                    std::fs::write(dir.join(format!("{tag}.bin")), &buf)?;
                    if let Some(png) = img.to_png() {
                        std::fs::write(dir.join(format!("{tag}.png")), png)?;
                    }
                    writeln!(index, "{tag} base={base:#x} hp={hp:#x} t={}", t0.elapsed().as_secs())?;
                }
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
            std::fs::write(dir.join("indice.txt"), &index)?;
            writeln!(out, "{index}amostras: {:?}", count)?;
        }
        "pos-mem" => {
            // Parte 1: acha X e Y na memória. SÓ LEITURA. Semente = coordenadas lidas na tela
            // ("Mapa (X , Y)", mesma fonte do Zen). Testa pares (X,Y) e (Y,X) como u8, u16,
            // i32 e f32 (f32 = posição no mundo, tile = valor/100). Filtra pelas mudanças com
            // a mesma tolerância da vida (leitura antes e depois da tela, 3 falhas seguidas).
            // Uso: zen-diag pos-mem [segundos] [nome do char]
            use zen_agent::win::capture;
            let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
            let only = args.get(2).cloned();
            let region: [i32; 4] = [0, 30, 300, 24];
            const KINDS: [&str; 8] = ["u8 x,y", "u8 y,x", "u16 x,y", "u16 y,x", "i32 x,y", "i32 y,x", "f32 x,y", "f32 y,x"];
            const MAX_SEED: usize = 3_000_000;
            const CHUNK: usize = 1 << 20;
            fn decode(kind: u8, b: &[u8]) -> (f64, f64) {
                let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as f64;
                let i32_ = |i: usize| i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as f64;
                let u16_ = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]) as f64;
                match kind {
                    0 => (b[0] as f64, b[1] as f64),
                    1 => (b[1] as f64, b[0] as f64),
                    2 => (u16_(0), u16_(2)),
                    3 => (u16_(2), u16_(0)),
                    4 => (i32_(0), i32_(4)),
                    5 => (i32_(4), i32_(0)),
                    6 => ((f(0) / 100.0).floor(), (f(4) / 100.0).floor()),
                    _ => ((f(4) / 100.0).floor(), (f(0) / 100.0).floor()),
                }
            }
            #[derive(Clone, Copy)]
            struct C {
                addr: u64,
                kind: u8,
                hits: u32,
                misses: u32,
                consec: u32,
            }
            struct St {
                name: String,
                pid: u32,
                hwnd: isize,
                prev: Option<(u32, u32)>,
                used: Option<(u32, u32)>,
                seeded: bool,
                c: Vec<C>,
                changes: u32,
            }
            let mut sts: Vec<St> = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .filter_map(|w| {
                    let name = zen_core::title::parse(&w.title).name?;
                    if only.as_deref().is_some_and(|o| !o.eq_ignore_ascii_case(&name)) {
                        return None;
                    }
                    Some(St { name, pid: w.pid, hwnd: w.hwnd, prev: None, used: None, seeded: false, c: vec![], changes: 0 })
                })
                .collect();
            let screen = |hwnd: isize| {
                capture::capture(hwnd, Some(region)).and_then(|img| {
                    let b = zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE);
                    zen_core::glyphs::read_coords(&b)
                })
            };
            let read_all = |p: &Proc, c: &[C]| -> Vec<Option<[u8; 8]>> {
                let mut out = vec![None; c.len()];
                let mut buf = vec![0u8; CHUNK + 8];
                let mut i = 0;
                while i < c.len() {
                    let start = c[i].addr;
                    let mut j = i;
                    while j < c.len() && c[j].addr + 8 <= start + CHUNK as u64 {
                        j += 1;
                    }
                    let len = (c[j - 1].addr + 8 - start) as usize;
                    if p.read(start, &mut buf[..len]) {
                        for k in i..j {
                            let o = (c[k].addr - start) as usize;
                            let mut v = [0u8; 8];
                            v.copy_from_slice(&buf[o..o + 8]);
                            out[k] = Some(v);
                        }
                    } else {
                        for k in i..j {
                            let mut v = [0u8; 8];
                            if p.read(c[k].addr, &mut v) {
                                out[k] = Some(v);
                            }
                        }
                    }
                    i = j;
                }
                out
            };
            let t0 = std::time::Instant::now();
            let mut buf = vec![0u8; CHUNK + 8];
            while t0.elapsed().as_secs() < secs && !sts.is_empty() {
                for st in sts.iter_mut() {
                    let Some(p) = Proc::open(st.pid) else { continue };
                    let before = if st.seeded { read_all(&p, &st.c) } else { vec![] };
                    let now = screen(st.hwnd);
                    let stable = now.is_some() && now == st.prev;
                    st.prev = now;
                    let Some((x, y)) = now.filter(|_| stable) else { continue };
                    if st.used == Some((x, y)) {
                        continue;
                    }
                    st.used = Some((x, y));
                    let t = std::time::Instant::now();
                    if !st.seeded {
                        let (fx, fy) = (x as f64, y as f64);
                        'seed: for (base, size) in p.writable_regions() {
                            let mut off = 0usize;
                            while off < size {
                                let n = CHUNK.min(size - off);
                                let at = base + off as u64;
                                if p.read(at, &mut buf[..n]) {
                                    for i in 0..n.saturating_sub(8) {
                                        let w = &buf[i..i + 8];
                                        for kind in 0..8u8 {
                                            if kind >= 2 && i % 2 != 0 {
                                                break;
                                            }
                                            if kind >= 4 && i % 4 != 0 {
                                                break;
                                            }
                                            if decode(kind, w) == (fx, fy) {
                                                st.c.push(C { addr: at + i as u64, kind, hits: 1, misses: 0, consec: 0 });
                                            }
                                        }
                                    }
                                }
                                off += n;
                                if st.c.len() > MAX_SEED {
                                    break 'seed;
                                }
                            }
                        }
                        st.seeded = true;
                        writeln!(out, "{}: semente ({x},{y}): {} candidatos em {:?}", st.name, st.c.len(), t.elapsed())?;
                    } else {
                        st.changes += 1;
                        let after = read_all(&p, &st.c);
                        let (fx, fy) = (x as f64, y as f64);
                        for (k, c) in st.c.iter_mut().enumerate() {
                            let ok = [before.get(k).copied().flatten(), after[k]]
                                .iter()
                                .any(|v| v.is_some_and(|v| decode(c.kind, &v) == (fx, fy)));
                            if ok {
                                c.hits += 1;
                                c.consec = 0;
                            } else {
                                c.misses += 1;
                                c.consec += 1;
                            }
                        }
                        st.c.retain(|c| c.consec < 3 && !(c.hits + c.misses >= 6 && c.misses * 2 > c.hits + c.misses));
                        writeln!(out, "{}: ({x},{y}) mudança {}: {} candidatos em {:?}", st.name, st.changes, st.c.len(), t.elapsed())?;
                        if st.c.is_empty() {
                            st.seeded = false; // recomeça com a próxima leitura
                        }
                    }
                    print!("{}", out.lines().last().map(|l| format!("{l}\n")).unwrap_or_default());
                }
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
            writeln!(out, "\n=== resultado ===")?;
            let hp: std::collections::HashMap<u32, u64> = std::fs::read_to_string("travas.json")
                .ok()
                .and_then(|t| serde_json::from_str::<Vec<zen_agent::monitor::SavedLock>>(&t).ok())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|l| Some((l.pid, l.hp_addr?)))
                .collect();
            for st in &sts {
                let mut best: Vec<&C> = st.c.iter().filter(|c| c.misses * 5 <= c.hits + c.misses).collect();
                best.sort_by_key(|c| std::cmp::Reverse(c.hits));
                writeln!(out, "{} (pid {}): {} mudanças, {} candidatos bons", st.name, st.pid, st.changes, best.len())?;
                for c in best.iter().take(30) {
                    let rel = hp.get(&st.pid).map(|h| format!(" (vida{:+#x})", c.addr as i64 - *h as i64)).unwrap_or_default();
                    writeln!(out, "  {:#x} {} acertos={} falhas={}{rel}", c.addr, KINDS[c.kind as usize], c.hits, c.misses)?;
                }
            }
        }
        "calibra" => {
            // Parte 3: quantos tiles anda um clique a (dx, dy) pixels. Clica em volta do char
            // (a câmera segue o char, então cada clique é independente), lê a posição na
            // memória (0x1ae434, f32 X e Y) antes e ~0,9 s depois, e no fim volta ao ponto
            // inicial. Usa SendInput com foco conferido (mesma regra do agente).
            // Uso: zen-diag calibra <nome do char> [cx cy]   (centro do clique, padrão 515 400)
            use zen_agent::win::{capture, input};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let cx: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(515);
            let cy: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(400);
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let p = Proc::open(w.pid).ok_or_else(|| anyhow::anyhow!("não abriu o processo"))?;
            let pos = || -> Option<(f32, f32)> {
                let mut b = [0u8; 8];
                p.read(0x1ae434, &mut b).then(|| {
                    (f32::from_le_bytes([b[0], b[1], b[2], b[3]]), f32::from_le_bytes([b[4], b[5], b[6], b[7]]))
                })
            };
            let screen = || {
                capture::capture(w.hwnd, Some([0, 30, 300, 24])).and_then(|img| {
                    let b = zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE);
                    zen_core::glyphs::read_coords(&b)
                })
            };
            let start = pos().ok_or_else(|| anyhow::anyhow!("não li a posição"))?;
            let tile = |v: (f32, f32)| ((v.0 / 100.0).floor() as u32, (v.1 / 100.0).floor() as u32);
            writeln!(out, "char {name}: memória {:?} = tile {:?}; tela {:?}", start, tile(start), screen())?;
            if screen().is_some_and(|s| s != tile(start)) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if screen().is_some_and(|s| s != tile(pos().unwrap_or(start))) {
                    anyhow::bail!("posição da memória não bate com a tela: não vou clicar");
                }
            }
            writeln!(out, "dx_px dy_px  antes_x antes_y  depois_x depois_y  (posição no mundo; tile = /100)")?;
            let offs = [(100, 0), (-100, 0), (0, 80), (0, -80), (70, 60), (-70, -60), (70, -60), (-70, 60), (140, 0), (0, 120)];
            for (dx, dy) in offs {
                let Some(a) = pos() else { break };
                if let Err(e) = input::run(w.hwnd, &[Step::Click { x: cx + dx, y: cy + dy }]) {
                    writeln!(out, "clique ({dx},{dy}) não enviado: {e}")?;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(900));
                let Some(b) = pos() else { break };
                writeln!(out, "{dx} {dy}  {:.1} {:.1}  {:.1} {:.1}", a.0, a.1, b.0, b.1)?;
            }
            // Os cliques vêm em pares opostos (+ e -): o char termina perto de onde começou.
            writeln!(out, "fim: memória {:?} = tile {:?}; início era tile {:?}", pos(), pos().map(tile), tile(start))?;
        }
        "calibra2" => {
            // Partes 3 e 4 (medição): helper desligado com Home (conferido pelo ícone pause/play),
            // busca rígida de X/Y na memória com o char andando, cliques em pares opostos,
            // modelo clique->tiles, volta ao tile inicial e Home de novo (helper religado).
            // Uso: zen-diag calibra2 <nome do char> [cx cy]
            use zen_agent::win::{capture, input};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let cx: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(515);
            let cy: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(400);
            const ICON: [i32; 4] = [244, 30, 36, 24];
            const VK_HOME: u16 = 0x24;
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let p = Proc::open(w.pid).ok_or_else(|| anyhow::anyhow!("não abriu o processo"))?;
            let screen = || {
                capture::capture(w.hwnd, Some([0, 30, 300, 24])).and_then(|img| {
                    let b = zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE);
                    zen_core::glyphs::read_coords(&b)
                })
            };
            // Espera a posição na tela ficar parada (2 leituras iguais com 300 ms).
            let settle = || -> Option<(u32, u32)> {
                let mut last = None;
                for _ in 0..20 {
                    let s = screen();
                    if s.is_some() && s == last {
                        return s;
                    }
                    last = s;
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                last
            };
            let icon = || capture::capture(w.hwnd, Some(ICON));
            let icon_diff = |a: &zen_agent::image::Image, b: &zen_agent::image::Image| -> f64 {
                let n = a.bgra.len().min(b.bgra.len()).max(1);
                a.bgra.iter().zip(&b.bgra).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as f64).sum::<f64>() / n as f64
            };
            let save = |tag: &str, img: &Option<zen_agent::image::Image>| {
                if let Some(png) = img.as_ref().and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("calibra2_{name}_{tag}.png"), png);
                }
            };
            let press_home = || input::run(w.hwnd, &[Step::Key(VK_HOME)]);

            // ---- 1. helper: desligar e conferir
            let on_icon = icon().ok_or_else(|| anyhow::anyhow!("não capturei o ícone do helper"))?;
            save("icone_antes", &Some(on_icon.clone()));
            if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                std::fs::write(format!("calibra2_{name}_janela_antes.png"), png)?;
            }
            let start = settle().ok_or_else(|| anyhow::anyhow!("não li as coordenadas na tela"))?;
            writeln!(out, "char {name}: início no tile {start:?}")?;
            press_home().map_err(|e| anyhow::anyhow!("Home não enviado: {e}"))?;
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let off_icon = icon();
            save("icone_depois_home", &off_icon);
            let d = off_icon.as_ref().map(|i| icon_diff(&on_icon, i)).unwrap_or(0.0);
            writeln!(out, "ícone do helper mudou {d:.1} (média por canal) depois do Home")?;
            if d < 8.0 {
                writeln!(out, "o ícone NÃO mudou: não sei se o helper desligou. Parei sem clicar.")?;
                let _ = press_home(); // desfaz, por segurança
                std::thread::sleep(std::time::Duration::from_millis(1200));
                let back = icon().map(|i| icon_diff(&on_icon, &i)).unwrap_or(99.0);
                writeln!(out, "Home de novo: diferença para o ícone original {back:.1}")?;
            } else {
                let off_ref = off_icon.clone().unwrap();
                // Bloco de trabalho: qualquer erro aqui ainda passa pelo religamento no fim.
                let work = (|| -> anyhow::Result<String> {
                    let mut log = String::new();
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let here = settle().unwrap_or(start);
                    // ---- 2. semente rígida de X/Y na memória (u8, u16, i32, f32/100; pares x,y e y,x)
                    const CHUNK: usize = 1 << 20;
                    let mut buf = vec![0u8; CHUNK + 8];
                    let dec = |kind: u8, b: &[u8]| -> (f64, f64) {
                        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as f64;
                        let i3 = |i: usize| i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as f64;
                        let u6 = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]) as f64;
                        match kind {
                            0 => (b[0] as f64, b[1] as f64),
                            1 => (b[1] as f64, b[0] as f64),
                            2 => (u6(0), u6(2)),
                            3 => (u6(2), u6(0)),
                            4 => (i3(0), i3(4)),
                            5 => (i3(4), i3(0)),
                            6 => (f(0) / 100.0, f(4) / 100.0),
                            _ => (f(4) / 100.0, f(0) / 100.0),
                        }
                    };
                    let close = |v: (f64, f64), t: (u32, u32), tol: f64| {
                        v.0.is_finite() && v.1.is_finite() && (v.0 - t.0 as f64).abs() <= tol && (v.1 - t.1 as f64).abs() <= tol
                    };
                    let exact = |kind: u8, v: (f64, f64), t: (u32, u32)| {
                        if kind >= 6 { v.0.floor() == t.0 as f64 && v.1.floor() == t.1 as f64 } else { v == (t.0 as f64, t.1 as f64) }
                    };
                    let mut cands: Vec<(u64, u8)> = Vec::new();
                    for (base, size) in p.writable_regions() {
                        let mut off = 0usize;
                        while off < size {
                            let n = CHUNK.min(size - off);
                            let at = base + off as u64;
                            if p.read(at, &mut buf[..n]) {
                                for i in 0..n.saturating_sub(8) {
                                    for kind in 0..8u8 {
                                        if (kind >= 2 && i % 2 != 0) || (kind >= 4 && i % 4 != 0) {
                                            break;
                                        }
                                        if exact(kind, dec(kind, &buf[i..i + 8]), here) {
                                            cands.push((at + i as u64, kind));
                                        }
                                    }
                                }
                            }
                            off += n;
                        }
                    }
                    writeln!(log, "semente X/Y {here:?}: {} candidatos", cands.len())?;
                    let read8 = |a: u64| {
                        let mut v = [0u8; 8];
                        p.read(a, &mut v).then_some(v)
                    };
                    // ---- 3. cliques em pares opostos
                    let offs = [(110, 0), (-110, 0), (0, 90), (0, -90), (80, 70), (-80, -70), (80, -70), (-80, 70), (150, 0), (-150, 0), (0, 130), (0, -130)];
                    let mut samples: Vec<(f64, f64, f64, f64)> = Vec::new();
                    for (dx, dy) in offs {
                        let a = settle().ok_or_else(|| anyhow::anyhow!("perdi as coordenadas na tela"))?;
                        input::run(w.hwnd, &[Step::Click { x: cx + dx, y: cy + dy }]).map_err(|e| anyhow::anyhow!(e))?;
                        // Durante o movimento: memória tem de ficar a 1 tile da tela; lixo elimina.
                        for _ in 0..6 {
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            let s = screen();
                            if let Some(s) = s {
                                cands.retain(|&(addr, k)| read8(addr).is_some_and(|v| close(dec(k, &v), s, 1.5)));
                            }
                        }
                        let b = settle().ok_or_else(|| anyhow::anyhow!("perdi as coordenadas na tela"))?;
                        cands.retain(|&(addr, k)| read8(addr).is_some_and(|v| exact(k, dec(k, &v), b) || close(dec(k, &v), b, 1.0)));
                        samples.push((dx as f64, dy as f64, b.0 as f64 - a.0 as f64, b.1 as f64 - a.1 as f64));
                        writeln!(log, "clique ({dx},{dy}): {a:?} -> {b:?}  | candidatos X/Y: {}", cands.len())?;
                    }
                    // ---- 4. modelo: tile_delta = A * pixel + b (mínimos quadrados)
                    let solve = |col: usize| -> Option<[f64; 3]> {
                        let mut m = [[0.0f64; 4]; 3];
                        for s in &samples {
                            let f = [s.0, s.1, 1.0];
                            let t = if col == 0 { s.2 } else { s.3 };
                            for r in 0..3 {
                                for c in 0..3 {
                                    m[r][c] += f[r] * f[c];
                                }
                                m[r][3] += f[r] * t;
                            }
                        }
                        for i in 0..3 {
                            let piv = (i..3).max_by(|&a, &b| m[a][i].abs().total_cmp(&m[b][i].abs()))?;
                            m.swap(i, piv);
                            if m[i][i].abs() < 1e-9 {
                                return None;
                            }
                            for r in 0..3 {
                                if r != i {
                                    let k = m[r][i] / m[i][i];
                                    for c in i..4 {
                                        m[r][c] -= k * m[i][c];
                                    }
                                }
                            }
                        }
                        Some([m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]])
                    };
                    let (mx, my) = (solve(0).ok_or_else(|| anyhow::anyhow!("modelo X falhou"))?, solve(1).ok_or_else(|| anyhow::anyhow!("modelo Y falhou"))?);
                    let resid: f64 = samples
                        .iter()
                        .map(|s| ((mx[0] * s.0 + mx[1] * s.1 + mx[2] - s.2).powi(2) + (my[0] * s.0 + my[1] * s.1 + my[2] - s.3).powi(2)).sqrt())
                        .sum::<f64>()
                        / samples.len() as f64;
                    writeln!(log, "MODELO: dX = {:.4}*px {:+.4}*py {:+.2} ; dY = {:.4}*px {:+.4}*py {:+.2} ; erro médio {:.2} tile", mx[0], mx[1], mx[2], my[0], my[1], my[2], resid)?;
                    // ---- volta ao tile inicial usando o modelo
                    let det = mx[0] * my[1] - mx[1] * my[0];
                    for _ in 0..8 {
                        let c = settle().ok_or_else(|| anyhow::anyhow!("perdi as coordenadas na tela"))?;
                        let (tx, ty) = (start.0 as f64 - c.0 as f64, start.1 as f64 - c.1 as f64);
                        if tx == 0.0 && ty == 0.0 {
                            break;
                        }
                        let (ux, uy) = (tx - mx[2], ty - my[2]);
                        let (mut px, mut py) = ((my[1] * ux - mx[1] * uy) / det, (-my[0] * ux + mx[0] * uy) / det);
                        let len = (px * px + py * py).sqrt();
                        if len > 180.0 {
                            px *= 180.0 / len;
                            py *= 180.0 / len;
                        }
                        writeln!(log, "voltando: em {c:?}, faltam ({tx},{ty}) tiles -> clique ({px:.0},{py:.0})")?;
                        input::run(w.hwnd, &[Step::Click { x: cx + px.round() as i32, y: cy + py.round() as i32 }]).map_err(|e| anyhow::anyhow!(e))?;
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    writeln!(log, "posição final: {:?} (início {start:?})", settle())?;
                    writeln!(log, "\nCANDIDATOS X/Y que sobreviveram ao movimento (lixo elimina):")?;
                    for (a, k) in cands.iter().take(40) {
                        writeln!(log, "  {a:#x} tipo {k}")?;
                    }
                    Ok(log)
                })();
                match &work {
                    Ok(l) => out.push_str(l),
                    Err(e) => writeln!(out, "ERRO no meio do teste: {e}")?,
                }
                // ---- 5. helper de volta: Home e conferir que voltou ao ícone original
                let _ = off_ref;
                press_home().map_err(|e| anyhow::anyhow!("Home final não enviado: {e} — RELIGUE O HELPER NA MÃO"))?;
                std::thread::sleep(std::time::Duration::from_millis(1200));
                let fin = icon();
                save("icone_fim", &fin);
                let back = fin.as_ref().map(|i| icon_diff(&on_icon, i)).unwrap_or(99.0);
                writeln!(out, "helper religado? diferença para o ícone original {back:.1} => {}", if back < 8.0 { "SIM" } else { "NÃO — religue na mão" })?;
            }
        }
        "navega" => {
            // Teste das Partes 1-4: helper off (Home + ícone), anda até (ix,iy) pela rota do
            // terreno, volta ao tile de partida, helper on (Home + ícone). Malha fechada: a cada
            // clique lê a posição na tela, corrige e reajusta o modelo pixel->tile.
            // Uso: zen-diag navega <nome> <EncTerrainN.att> <ida_x> <ida_y> [volta_x volta_y]
            use zen_agent::win::{capture, input};
            use zen_core::actions::Step;
            use zen_core::terrain::Terrain;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let att = args.get(2).ok_or_else(|| anyhow::anyhow!("falta o arquivo de terreno"))?.clone();
            let goal: (i32, i32) = (args.get(3).ok_or_else(|| anyhow::anyhow!("falta x"))?.parse()?, args.get(4).ok_or_else(|| anyhow::anyhow!("falta y"))?.parse()?);
            let back_to: Option<(i32, i32)> = match (args.get(5), args.get(6)) {
                (Some(a), Some(b)) => Some((a.parse()?, b.parse()?)),
                _ => None,
            };
            let (cx, cy) = (515, 400);
            let t = Terrain::from_client_file(&std::fs::read(&att)?).map_err(|e| anyhow::anyhow!("{e}"))?;
            writeln!(out, "terreno {att}: World{}", t.world)?;
            const ICON: [i32; 4] = [244, 30, 36, 24];
            const VK_HOME: u16 = 0x24;
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let screen = || {
                capture::capture(w.hwnd, Some([0, 30, 300, 24])).and_then(|img| {
                    let b = zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE);
                    zen_core::glyphs::read_coords(&b).map(|(x, y)| (x as i32, y as i32))
                })
            };
            let settle = || -> Option<(i32, i32)> {
                let mut last = None;
                for _ in 0..25 {
                    let s = screen();
                    if s.is_some() && s == last {
                        return s;
                    }
                    last = s;
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                last
            };
            // Leitura plausível: perto da anterior (um clique anda no máximo ~6 tiles).
            let near = |prev: (i32, i32)| -> Option<(i32, i32)> {
                for _ in 0..4 {
                    let s = settle()?;
                    if (s.0 - prev.0).abs().max((s.1 - prev.1).abs()) <= 10 {
                        return Some(s);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                None
            };
            let icon = || capture::capture(w.hwnd, Some(ICON));
            let diff = |a: &zen_agent::image::Image, b: &zen_agent::image::Image| -> f64 {
                let n = a.bgra.len().min(b.bgra.len()).max(1);
                a.bgra.iter().zip(&b.bgra).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as f64).sum::<f64>() / n as f64
            };
            let on_icon = icon().ok_or_else(|| anyhow::anyhow!("não capturei o ícone do helper"))?;
            let start = settle().ok_or_else(|| anyhow::anyhow!("não li as coordenadas"))?;
            writeln!(out, "início {start:?}, destino {goal:?}")?;
            if !t.walkable(start.0, start.1) || !t.walkable(goal.0, goal.1) {
                anyhow::bail!("início ou destino não é piso neste terreno (mapa errado?)");
            }
            input::run(w.hwnd, &[Step::Key(VK_HOME)]).map_err(|e| anyhow::anyhow!(e))?;
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let d0 = icon().map(|i| diff(&on_icon, &i)).unwrap_or(0.0);
            writeln!(out, "helper: ícone mudou {d0:.1} depois do Home")?;
            if d0 < 8.0 {
                let _ = input::run(w.hwnd, &[Step::Key(VK_HOME)]);
                anyhow::bail!("o ícone do helper não mudou: parei sem andar (Home enviado de novo para desfazer)");
            }
            // Modelo pixel -> tile (medido em 08/11 no spot; erro ~1,5 tile). Reajustado a cada clique.
            let mut m = [[0.0262f64, 0.0120], [0.0141, -0.0177]];
            let mut samples: Vec<(f64, f64, f64, f64)> = Vec::new();
            // Áreas onde clique não anda: janela PICK (esquerda), barra de baixo, minimapa, topo.
            let blocked = |x: i32, y: i32| {
                (80..=390).contains(&x) && (135..=625).contains(&y)
                    || y > 680 || y < 75 || x < 15 || x > 1010
                    || ((x - 910).pow(2) + (y - 555).pow(2)) < 100 * 100
            };
            let mut walk = |target: (i32, i32), log: &mut String| -> anyhow::Result<bool> {
                let t0 = std::time::Instant::now();
                let mut stuck = 0;
                let mut turn = 0.0f64;
                let mut last = settle().ok_or_else(|| anyhow::anyhow!("perdi as coordenadas"))?;
                for step in 0..150 {
                    if t0.elapsed().as_secs() > 300 {
                        writeln!(log, "tempo esgotado (5 min)")?;
                        return Ok(false);
                    }
                    let pos = near(last).ok_or_else(|| anyhow::anyhow!("leitura da posição implausível perto de {last:?}"))?;
                    if pos == target {
                        writeln!(log, "CHEGOU em {target:?} com {step} cliques em {:?}", t0.elapsed())?;
                        return Ok(true);
                    }
                    let path = t.path(pos, target).ok_or_else(|| anyhow::anyhow!("sem rota de {pos:?} para {target:?}"))?;
                    let wps = t.waypoints(pos, &path, 6);
                    let wp = wps[0];
                    let (dx, dy) = ((wp.0 - pos.0) as f64, (wp.1 - pos.1) as f64);
                    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
                    let mut px = (m[1][1] * dx - m[0][1] * dy) / det;
                    let mut py = (-m[1][0] * dx + m[0][0] * dy) / det;
                    if turn != 0.0 {
                        let (c, s) = (turn.cos(), turn.sin());
                        (px, py) = (px * c - py * s, px * s + py * c);
                    }
                    let len = (px * px + py * py).sqrt().max(1.0);
                    let k = (230.0 / len).min(1.0);
                    px *= k;
                    py *= k;
                    let mut tries = 0;
                    while blocked(cx + px as i32, cy + py as i32) && tries < 4 {
                        px *= 0.6;
                        py *= 0.6;
                        tries += 1;
                    }
                    let (sx, sy) = (cx + px.round() as i32, cy + py.round() as i32);
                    input::run(w.hwnd, &[Step::Click { x: sx, y: sy }]).map_err(|e| anyhow::anyhow!(e))?;
                    std::thread::sleep(std::time::Duration::from_millis(350));
                    let new = near(pos).ok_or_else(|| anyhow::anyhow!("leitura da posição implausível perto de {pos:?}"))?;
                    last = new;
                    let (ax, ay) = ((new.0 - pos.0) as f64, (new.1 - pos.1) as f64);
                    writeln!(log, "{pos:?} -> alvo {wp:?} clique ({:.0},{:.0}) -> {new:?}", px, py)?;
                    if ax == 0.0 && ay == 0.0 {
                        stuck += 1;
                        turn = [0.0, 0.5, -0.5, 1.0, -1.0][stuck % 5];
                        if stuck >= 6 {
                            writeln!(log, "TRAVOU: 6 cliques sem sair do lugar em {pos:?}")?;
                            return Ok(false);
                        }
                        continue;
                    }
                    stuck = 0;
                    turn = 0.0;
                    samples.push((px, py, ax, ay));
                    if samples.len() > 20 {
                        samples.remove(0);
                    }
                    if samples.len() >= 4 {
                        // mínimos quadrados sem termo constante: [tx ty] = M [px py]
                        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
                        let (mut bx, mut by) = ([0.0f64; 2], [0.0f64; 2]);
                        for s in &samples {
                            sxx += s.0 * s.0;
                            sxy += s.0 * s.1;
                            syy += s.1 * s.1;
                            bx[0] += s.0 * s.2;
                            bx[1] += s.1 * s.2;
                            by[0] += s.0 * s.3;
                            by[1] += s.1 * s.3;
                        }
                        // prior: puxa para o modelo atual (peso de 2 amostras de 150 px)
                        let w2 = 2.0 * 150.0 * 150.0;
                        let (a11, a12, a22) = (sxx + w2, sxy, syy + w2);
                        let dd = a11 * a22 - a12 * a12;
                        if dd.abs() > 1e-6 {
                            let solve = |b0: f64, b1: f64| ((a22 * b0 - a12 * b1) / dd, (-a12 * b0 + a11 * b1) / dd);
                            let r0 = solve(bx[0] + w2 * m[0][0], bx[1] + w2 * m[0][1]);
                            let r1 = solve(by[0] + w2 * m[1][0], by[1] + w2 * m[1][1]);
                            m = [[r0.0, r0.1], [r1.0, r1.1]];
                        }
                    }
                }
                Ok(false)
            };
            let mut log = String::new();
            let ida = walk(goal, &mut log);
            let home = back_to.unwrap_or(start);
            let volta = match &ida {
                Ok(true) => {
                    writeln!(log, "--- volta para {home:?} ---")?;
                    walk(home, &mut log)
                }
                _ => Ok(false),
            };
            out.push_str(&log);
            writeln!(out, "ida: {:?}  volta: {:?}  modelo final: {m:?}", ida.as_ref().map_err(|e| e.to_string()), volta.as_ref().map_err(|e| e.to_string()))?;
            writeln!(out, "posição final: {:?}", settle())?;
            // Helper de volta, aconteça o que acontecer.
            let _ = input::run(w.hwnd, &[Step::Key(VK_HOME)]);
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let back = icon().map(|i| diff(&on_icon, &i)).unwrap_or(99.0);
            writeln!(out, "helper religado? diferença para o ícone original {back:.1} => {}", if back < 8.0 { "SIM" } else { "NÃO — religue na mão" })?;
            if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                std::fs::write(format!("navega_{name}_fim.png"), png)?;
            }
        }
        "navega2" => {
            // Teste das Partes 1-4: helper off (Home + ícone), anda até (ix,iy) pela rota do
            // terreno, volta ao tile de partida, helper on (Home + ícone). Malha fechada: a cada
            // clique lê a posição na tela, corrige e reajusta o modelo pixel->tile.
            // Modo "segurar e guiar" (dica do dono, 08/11): botão segurado com o cursor na direção
            // da rota; perto do destino (<= 2 tiles) volta aos cliques curtos com correção.
            // Uso: zen-diag navega2 <nome> <EncTerrainN.att> <ida_x> <ida_y> [volta_x volta_y]
            use zen_agent::win::{capture, input};
            use zen_core::actions::Step;
            use zen_core::terrain::Terrain;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let att = args.get(2).ok_or_else(|| anyhow::anyhow!("falta o arquivo de terreno"))?.clone();
            let goal: (i32, i32) = (args.get(3).ok_or_else(|| anyhow::anyhow!("falta x"))?.parse()?, args.get(4).ok_or_else(|| anyhow::anyhow!("falta y"))?.parse()?);
            let no_home = args.iter().any(|a| a == "--sem-home");
            let back_to: Option<(i32, i32)> = match (args.get(5).filter(|a| !a.starts_with("--")), args.get(6).filter(|a| !a.starts_with("--"))) {
                (Some(a), Some(b)) => Some((a.parse()?, b.parse()?)),
                _ => None,
            };
            let (cx, cy) = (515, 400);
            let t = Terrain::from_client_file(&std::fs::read(&att)?).map_err(|e| anyhow::anyhow!("{e}"))?;
            writeln!(out, "terreno {att}: World{}", t.world)?;
            const ICON: [i32; 4] = [244, 30, 36, 24];
            const VK_HOME: u16 = 0x24;
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let screen = || {
                capture::capture(w.hwnd, Some([0, 30, 300, 24])).and_then(|img| {
                    let b = zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE);
                    zen_core::glyphs::read_coords(&b).map(|(x, y)| (x as i32, y as i32))
                })
            };
            let settle = || -> Option<(i32, i32)> {
                let mut last = None;
                for _ in 0..25 {
                    let s = screen();
                    if s.is_some() && s == last {
                        return s;
                    }
                    last = s;
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                last
            };
            // Leitura plausível: perto da anterior (um clique anda no máximo ~6 tiles).
            let near = |prev: (i32, i32)| -> Option<(i32, i32)> {
                for _ in 0..4 {
                    let s = settle()?;
                    if (s.0 - prev.0).abs().max((s.1 - prev.1).abs()) <= 10 {
                        return Some(s);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                None
            };
            let icon = || capture::capture(w.hwnd, Some(ICON));
            let diff = |a: &zen_agent::image::Image, b: &zen_agent::image::Image| -> f64 {
                let n = a.bgra.len().min(b.bgra.len()).max(1);
                a.bgra.iter().zip(&b.bgra).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as f64).sum::<f64>() / n as f64
            };
            let on_icon = icon().ok_or_else(|| anyhow::anyhow!("não capturei o ícone do helper"))?;
            let start = settle().ok_or_else(|| anyhow::anyhow!("não li as coordenadas"))?;
            writeln!(out, "início {start:?}, destino {goal:?}")?;
            let kanturu = capture::capture(w.hwnd, Some([0, 30, 300, 24])).is_some_and(|img| {
                let b = zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE);
                zen_core::glyphs::map_is_kanturu(&b)
            });
            writeln!(out, "mapa no canto é Kanturu: {kanturu}")?;
            if !kanturu && t.world == 38 {
                anyhow::bail!("o char não está no Kanturu 1: não vou andar com o terreno do Kanturu");
            }
            if !t.walkable(start.0, start.1) || !t.walkable(goal.0, goal.1) {
                anyhow::bail!("início ou destino não é piso neste terreno (mapa errado?)");
            }
            if !no_home {
            input::run(w.hwnd, &[Step::Key(VK_HOME)]).map_err(|e| anyhow::anyhow!(e))?;
            std::thread::sleep(std::time::Duration::from_millis(1200));
            }
            let d0 = if no_home { 99.0 } else { icon().map(|i| diff(&on_icon, &i)).unwrap_or(0.0) };
            writeln!(out, "helper: ícone mudou {d0:.1} depois do Home (sem-home={no_home})")?;
            if d0 < 8.0 {
                let _ = input::run(w.hwnd, &[Step::Key(VK_HOME)]);
                anyhow::bail!("o ícone do helper não mudou: parei sem andar (Home enviado de novo para desfazer)");
            }
            // Modelo pixel -> tile (medido em 08/11 no spot; erro ~1,5 tile). Reajustado a cada clique.
            // Modelo aprendido no teste de cliques de 08/11 (~1 tile a cada 80-95 px).
            let mut m = [[0.01056f64, 0.00878], [0.00688, -0.01186]];
            let mut samples: Vec<(f64, f64, f64, f64)> = Vec::new();
            // Áreas onde clique não anda: janela PICK (esquerda), barra de baixo, minimapa, topo.
            let blocked = |x: i32, y: i32| {
                (80..=390).contains(&x) && (135..=625).contains(&y)
                    || y > 680 || y < 75 || x < 15 || x > 1010
                    || ((x - 910).pow(2) + (y - 555).pow(2)) < 100 * 100
            };
            let mut walk = |target: (i32, i32), log: &mut String| -> anyhow::Result<bool> {
                let t0 = std::time::Instant::now();
                let mut stuck = 0;
                let mut turn = 0.0f64;
                let mut last = settle().ok_or_else(|| anyhow::anyhow!("perdi as coordenadas"))?;
                // ---- fase 1: segurar e guiar até 2 tiles do destino
                let cheb = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
                let dir_px = |pos: (i32, i32), angle: f64, radius: f64| -> Option<(i32, i32)> {
                    let path = t.path(pos, target)?;
                    let wp = t.waypoints(pos, &path, 6)[0];
                    let (dx, dy) = ((wp.0 - pos.0) as f64, (wp.1 - pos.1) as f64);
                    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
                    let (mut px, mut py) = ((m[1][1] * dx - m[0][1] * dy) / det, (-m[1][0] * dx + m[0][0] * dy) / det);
                    if angle != 0.0 {
                        let (c, s) = (angle.cos(), angle.sin());
                        (px, py) = (px * c - py * s, px * s + py * c);
                    }
                    let len = (px * px + py * py).sqrt().max(1.0);
                    let (mut px, mut py) = (px / len * radius, py / len * radius);
                    let mut tries = 0;
                    while blocked(cx + px as i32, cy + py as i32) && tries < 4 {
                        px *= 0.7;
                        py *= 0.7;
                        tries += 1;
                    }
                    Some((cx + px.round() as i32, cy + py.round() as i32))
                };
                let (mut holds, mut detours, mut ticks) = (0u32, 0u32, 0u32);
                let mut angle = 0.0f64;
                'hold: while cheb(last, target) > 2 && t0.elapsed().as_secs() < 240 {
                    let Some((hx, hy)) = dir_px(last, angle, 120.0) else { break };
                    let mut h = input::Hold::start(w.hwnd, hx, hy).map_err(|e| anyhow::anyhow!(e))?;
                    holds += 1;
                    let mut still = 0;
                    let mut best = t.path(last, target).map(|p| p.len()).unwrap_or(usize::MAX);
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                        ticks += 1;
                        if t0.elapsed().as_secs() >= 240 {
                            writeln!(log, "segurar: tempo esgotado")?;
                            break 'hold;
                        }
                        // Leitura direta (o char está andando: não dá para esperar ficar parado).
                        let Some(p) = screen().filter(|p| cheb(*p, last) <= 4) else { continue };
                        if p == last { still += 1 } else { still = 0 }
                        last = p;
                        if cheb(p, target) <= 2 {
                            break 'hold;
                        }
                        let remaining = t.path(p, target).map(|v| v.len()).unwrap_or(usize::MAX);
                        if still >= 3 || remaining > best + 2 {
                            // parou (mob embaixo do cursor) ou saiu da rota: solta e tenta outro ângulo
                            writeln!(log, "  desvio em {p:?} (parado={still}, rota {remaining} > melhor {best}+2)")?;
                            detours += 1;
                            angle = [0.5, -0.5, 0.9, -0.9, 0.0][(detours as usize) % 5];
                            h.release();
                            drop(h);
                            if detours > 25 {
                                writeln!(log, "DESISTIU: desvios demais")?;
                                return Ok(false);
                            }
                            continue 'hold;
                        }
                        best = best.min(remaining);
                        if let Some((sx, sy)) = dir_px(p, 0.0, 120.0) {
                            h.steer(sx, sy).map_err(|e| anyhow::anyhow!(e))?;
                        }
                    }
                }
                writeln!(log, "segurar: {holds} vez(es), {detours} desvio(s), {ticks} leituras, chegou a {last:?} em {:?}", t0.elapsed())?;
                std::thread::sleep(std::time::Duration::from_millis(400));
                last = settle().unwrap_or(last);
                // ---- fase 2: cliques curtos com correção (precisão perto do destino).
                // Regra do dono (08/11): tile exato por até 10 s; depois aceita 1 tile.
                let click_t0 = std::time::Instant::now();
                for step in 0..150 {
                    if t0.elapsed().as_secs() > 300 {
                        writeln!(log, "tempo esgotado (5 min)")?;
                        return Ok(false);
                    }
                    let pos = near(last).ok_or_else(|| anyhow::anyhow!("leitura da posição implausível perto de {last:?}"))?;
                    if pos == target {
                        writeln!(log, "CHEGOU em {target:?} com {step} cliques em {:?}", t0.elapsed())?;
                        return Ok(true);
                    }
                    if click_t0.elapsed().as_secs() >= 10 && cheb(pos, target) <= 1 {
                        writeln!(log, "CHEGOU a 1 tile de {target:?} (em {pos:?}) depois de 10 s tentando o exato; {step} cliques, {:?}", t0.elapsed())?;
                        return Ok(true);
                    }
                    let path = t.path(pos, target).ok_or_else(|| anyhow::anyhow!("sem rota de {pos:?} para {target:?}"))?;
                    let wps = t.waypoints(pos, &path, 6);
                    let wp = wps[0];
                    let (dx, dy) = ((wp.0 - pos.0) as f64, (wp.1 - pos.1) as f64);
                    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
                    let mut px = (m[1][1] * dx - m[0][1] * dy) / det;
                    let mut py = (-m[1][0] * dx + m[0][0] * dy) / det;
                    if turn != 0.0 {
                        let (c, s) = (turn.cos(), turn.sin());
                        (px, py) = (px * c - py * s, px * s + py * c);
                    }
                    let len = (px * px + py * py).sqrt().max(1.0);
                    // Clique curto demais cai no mesmo tile (visto: 6 cliques de ~50 px sem andar).
                    let k = (230.0 / len).min(1.0).max(90.0 / len);
                    px *= k;
                    py *= k;
                    let mut tries = 0;
                    while blocked(cx + px as i32, cy + py as i32) && tries < 4 {
                        px *= 0.6;
                        py *= 0.6;
                        tries += 1;
                    }
                    let (sx, sy) = (cx + px.round() as i32, cy + py.round() as i32);
                    input::run(w.hwnd, &[Step::Click { x: sx, y: sy }]).map_err(|e| anyhow::anyhow!(e))?;
                    std::thread::sleep(std::time::Duration::from_millis(350));
                    let new = near(pos).ok_or_else(|| anyhow::anyhow!("leitura da posição implausível perto de {pos:?}"))?;
                    last = new;
                    let (ax, ay) = ((new.0 - pos.0) as f64, (new.1 - pos.1) as f64);
                    writeln!(log, "{pos:?} -> alvo {wp:?} clique ({:.0},{:.0}) -> {new:?}", px, py)?;
                    if ax == 0.0 && ay == 0.0 {
                        stuck += 1;
                        turn = [0.0, 0.5, -0.5, 1.0, -1.0][stuck % 5];
                        if stuck >= 6 {
                            writeln!(log, "TRAVOU: 6 cliques sem sair do lugar em {pos:?}")?;
                            return Ok(false);
                        }
                        continue;
                    }
                    stuck = 0;
                    turn = 0.0;
                    samples.push((px, py, ax, ay));
                    if samples.len() > 20 {
                        samples.remove(0);
                    }
                    if samples.len() >= 4 {
                        // mínimos quadrados sem termo constante: [tx ty] = M [px py]
                        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
                        let (mut bx, mut by) = ([0.0f64; 2], [0.0f64; 2]);
                        for s in &samples {
                            sxx += s.0 * s.0;
                            sxy += s.0 * s.1;
                            syy += s.1 * s.1;
                            bx[0] += s.0 * s.2;
                            bx[1] += s.1 * s.2;
                            by[0] += s.0 * s.3;
                            by[1] += s.1 * s.3;
                        }
                        // prior: puxa para o modelo atual (peso de 2 amostras de 150 px)
                        let w2 = 2.0 * 150.0 * 150.0;
                        let (a11, a12, a22) = (sxx + w2, sxy, syy + w2);
                        let dd = a11 * a22 - a12 * a12;
                        if dd.abs() > 1e-6 {
                            let solve = |b0: f64, b1: f64| ((a22 * b0 - a12 * b1) / dd, (-a12 * b0 + a11 * b1) / dd);
                            let r0 = solve(bx[0] + w2 * m[0][0], bx[1] + w2 * m[0][1]);
                            let r1 = solve(by[0] + w2 * m[1][0], by[1] + w2 * m[1][1]);
                            m = [[r0.0, r0.1], [r1.0, r1.1]];
                        }
                    }
                }
                Ok(false)
            };
            let mut log = String::new();
            let ida = walk(goal, &mut log);
            let home = back_to.unwrap_or(start);
            let volta = match &ida {
                Ok(true) => {
                    writeln!(log, "--- volta para {home:?} ---")?;
                    walk(home, &mut log)
                }
                _ => Ok(false),
            };
            out.push_str(&log);
            writeln!(out, "ida: {:?}  volta: {:?}  modelo final: {m:?}", ida.as_ref().map_err(|e| e.to_string()), volta.as_ref().map_err(|e| e.to_string()))?;
            writeln!(out, "posição final: {:?}", settle())?;
            // Helper de volta, aconteça o que acontecer (no modo --sem-home, quem religa é o comando "helper").
            if !no_home {
                let _ = input::run(w.hwnd, &[Step::Key(VK_HOME)]);
                std::thread::sleep(std::time::Duration::from_millis(1200));
                let back = icon().map(|i| diff(&on_icon, &i)).unwrap_or(99.0);
                writeln!(out, "helper religado? diferença para o ícone original {back:.1} => {}", if back < 8.0 { "SIM" } else { "NÃO — religue na mão" })?;
            }
            if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                std::fs::write(format!("navega2_{name}_fim.png"), png)?;
            }
        }
        "tela-m" => {
            // Abre a janela do M (mapas), tira print e fecha (M de novo). Só olhar: nenhum clique.
            // Uso: zen-diag tela-m <nome do char>
            use zen_agent::win::{capture, input};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let shot = |tag: &str| -> anyhow::Result<()> {
                if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    std::fs::write(format!("telam_{name}_{tag}.png"), png)?;
                }
                Ok(())
            };
            shot("antes")?;
            input::run(w.hwnd, &[Step::Key(0x4D)]).map_err(|e| anyhow::anyhow!(e))?;
            std::thread::sleep(std::time::Duration::from_millis(1200));
            shot("aberto")?;
            input::run(w.hwnd, &[Step::Key(0x4D)]).map_err(|e| anyhow::anyhow!(e))?;
            std::thread::sleep(std::time::Duration::from_millis(1000));
            shot("depois")?;
            writeln!(out, "prints: telam_{name}_antes/aberto/depois.png")?;
        }
        "teleporte" => {
            // Recuperação entre mapas: Home (helper off, conferido) -> M -> clique em "Kanturu"
            // -> confere nome do mapa e salto de posição -> Home (helper on, conferido).
            // Uso: zen-diag teleporte <nome do char> [--ok]   (--ok: clica antes no OK do aviso do helper)
            // Não mexe no helper (isso é o comando "helper").
            use zen_agent::win::{capture, input};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let (bx, by) = (612, 323);
            let click_ok = args.iter().any(|a| a == "--ok");
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let corner = || {
                capture::capture(w.hwnd, Some([0, 30, 300, 24])).map(|img| {
                    zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::glyphs::COORDS_WHITE)
                })
            };
            let coords = || corner().and_then(|b| zen_core::glyphs::read_coords(&b)).map(|(x, y)| (x as i32, y as i32));
            let kanturu = || corner().is_some_and(|b| zen_core::glyphs::map_is_kanturu(&b));
            let shot = |tag: &str| {
                if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("teleporte_{name}_{tag}.png"), png);
                }
            };
            let p0 = coords().ok_or_else(|| anyhow::anyhow!("não li as coordenadas"))?;
            writeln!(out, "antes: {p0:?}, Kanturu={}", kanturu())?;
            let skip = kanturu();
            if skip {
                writeln!(out, "já está no Kanturu: nada a fazer")?;
            }
            if click_ok && !skip {
                input::run(w.hwnd, &[Step::Click { x: 514, y: 316 }]).map_err(|e| anyhow::anyhow!(e))?;
                std::thread::sleep(std::time::Duration::from_millis(800));
            }
            let result = (|| -> anyhow::Result<String> {
                let mut log = String::new();
                if skip {
                    return Ok(log);
                }
                input::run(w.hwnd, &[Step::Key(0x4D)]).map_err(|e| anyhow::anyhow!(e))?;
                std::thread::sleep(std::time::Duration::from_millis(1300));
                shot("m_aberto");
                input::run(w.hwnd, &[Step::Click { x: bx, y: by }]).map_err(|e| anyhow::anyhow!(e))?;
                let t0 = std::time::Instant::now();
                let mut landed = None;
                while t0.elapsed().as_secs() < 15 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if let Some(p) = coords() {
                        if (p.0 - p0.0).abs().max((p.1 - p0.1).abs()) > 10 && kanturu() {
                            std::thread::sleep(std::time::Duration::from_millis(800));
                            landed = coords();
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(800));
                shot("depois");
                match landed {
                    Some(p) => writeln!(log, "TELEPORTOU para {p:?} em {:?} (Kanturu={})", t0.elapsed(), kanturu())?,
                    None => {
                        writeln!(log, "não teleportou em 15 s (posição {:?}); fechando o M", coords())?;
                        let _ = input::run(w.hwnd, &[Step::Key(0x4D)]);
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        shot("m_fechado");
                    }
                }
                Ok(log)
            })();
            match result {
                Ok(l) => out.push_str(&l),
                Err(e) => writeln!(out, "ERRO: {e}")?,
            }
        }
        "bau" => {
            // Baú do Kanturu (Safety Guardian). Uso:
            //   zen-diag bau <nome> ler                   lê as 15 abas, não deposita
            //   zen-diag bau <nome> depositar             deposita tudo menos 40 mi na 1ª aba que cabe
            //   ... [--volta x y]                         depois volta ao spot (x, y) e liga o helper
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let mode = args.get(2).map(String::as_str).unwrap_or("ler");
            let deposit = match mode {
                "ler" => false,
                "depositar" => true,
                m => anyhow::bail!("modo {m}: use ler ou depositar"),
            };
            let back = args.iter().position(|a| a == "--volta").and_then(|i| Some((args.get(i + 1)?.parse::<i32>().ok()?, args.get(i + 2)?.parse::<i32>().ok()?)));
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let t = zen_agent::win::nav::kanturu_terrain(w.pid).map_err(|e| anyhow::anyhow!(e))?;
            let t0 = std::time::Instant::now();
            let say = |m: String| println!("  {:>5.1}s {m}", t0.elapsed().as_secs_f32());
            let shot = |tag: &str| {
                if let Some(png) = zen_agent::win::capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                    let _ = std::fs::write(format!("bau_{name}_{tag}.png"), png);
                }
            };
            let r = zen_agent::win::nav::vault(w.hwnd, &t, zen_core::vault::RESERVE, deposit, !deposit, &say);
            shot("fim");
            match &r {
                Ok(rep) => {
                    writeln!(out, "Zen no char: {} -> {}", zen_core::fmt::int_br(rep.zen_before), zen_core::fmt::int_br(rep.zen_after))?;
                    for (i, (n, z)) in rep.tabs.iter().enumerate() {
                        let label = n.map(|n| format!("aba {n:>2}")).unwrap_or_else(|| "aba ??".into());
                        writeln!(out, "  {:>2}ª vista, {label}: {:>13}", i + 1, zen_core::fmt::int_br(*z))?;
                    }
                    // Títulos binarizados, na ordem vista: viram os modelos de vault_tabs.txt.
                    let mut t = String::new();
                    for (i, b) in rep.titles.iter().enumerate() {
                        t.push_str(&format!("== vista {}\n{b}", i + 1));
                    }
                    std::fs::write(format!("bau_titulos_{name}.txt"), t)?;
                    match rep.deposited {
                        Some((tab, v)) => writeln!(
                            out,
                            "DEPOSITOU {} na {} (conferido na tela)",
                            zen_core::fmt::int_br(v),
                            if tab == 0 { "aba atual (número não lido)".to_string() } else { format!("aba {tab}") }
                        )?,
                        None => writeln!(out, "nada depositado ({mode})")?,
                    }
                }
                Err(e) => writeln!(out, "ERRO: {e}")?,
            }
            if let Some(spot) = back {
                match zen_agent::win::nav::recover(w.hwnd, &t, spot, &say) {
                    Ok(m) => writeln!(out, "volta ao spot: {m}")?,
                    Err(e) => writeln!(out, "volta ao spot FALHOU: {e}")?,
                }
            }
            writeln!(out, "tempo total: {:?}", t0.elapsed())?;
        }
        "passos" => {
            // Teste manual (ponte): sequência fechada de teclas/cliques numa janela do jogo, com
            // prints. Uso: zen-diag passos <nome> k:4D c:612,323 w:1500 s:tag p ...
            //   k:HEX tecla virtual | c:x,y clique (janela inteira) | m:x,y só cursor | n nome do mapa | r:x,y botão direito | R:x,y,ms segura o direito | w:ms espera | s:tag print | p posição
            use zen_agent::win::{capture, input, nav};
            use zen_core::actions::Step;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let _s = input::Session::begin(w.hwnd).map_err(|e| anyhow::anyhow!(e))?;
            for a in &args[2..] {
                let (k, v) = a.split_once(':').unwrap_or((a.as_str(), ""));
                match k {
                    "k" => input::run(w.hwnd, &[Step::Key(u16::from_str_radix(v, 16)?)]).map_err(|e| anyhow::anyhow!(e))?,
                    "c" => {
                        let (x, y) = v.split_once(',').ok_or_else(|| anyhow::anyhow!("clique sem x,y: {a}"))?;
                        input::run(w.hwnd, &[Step::Click { x: x.parse()?, y: y.parse()? }]).map_err(|e| anyhow::anyhow!(e))?
                    }
                    "r" => {
                        let (x, y) = v.split_once(',').ok_or_else(|| anyhow::anyhow!("clique sem x,y: {a}"))?;
                        input::run(w.hwnd, &[Step::RightClick { x: x.parse()?, y: y.parse()? }]).map_err(|e| anyhow::anyhow!(e))?
                    }
                    "R" => {
                        // R:x,y,ms segura o botão direito
                        let p: Vec<&str> = v.split(',').collect();
                        anyhow::ensure!(p.len() == 3, "R precisa de x,y,ms: {a}");
                        input::run(w.hwnd, &[Step::RightHold { x: p[0].parse()?, y: p[1].parse()?, ms: p[2].parse()? }]).map_err(|e| anyhow::anyhow!(e))?
                    }
                    "m" => {
                        // m:x,y só leva o cursor (tooltip)
                        let (x, y) = v.split_once(',').ok_or_else(|| anyhow::anyhow!("m sem x,y: {a}"))?;
                        input::run(w.hwnd, &[Step::Hover { x: x.parse()?, y: y.parse()? }]).map_err(|e| anyhow::anyhow!(e))?
                    }
                    "n" => writeln!(out, "mapa: {:?}", nav::observe_named(w.hwnd))?,
                    "h" => writeln!(out, "helper: {:?} posição: {:?}", nav::helper_state(w.hwnd), nav::position(w.hwnd))?,
                    "w" => std::thread::sleep(std::time::Duration::from_millis(v.parse::<u64>()?.min(30_000))),
                    "s" => {
                        if let Some(png) = capture::capture(w.hwnd, None).and_then(|i| i.to_png()) {
                            std::fs::write(format!("passos_{name}_{v}.png"), png)?;
                        }
                    }
                    "p" => writeln!(out, "posição: {:?} lorencia={} inventário={} baú={}", nav::observe_map(w.hwnd), nav::in_lorencia(w.hwnd), nav::inventory_open(w.hwnd), nav::storage_open(w.hwnd))?,
                    "fecha-inv" => {
                        // V só se o inventário estiver aberto (helper não liga com ele aberto)
                        if nav::inventory_open(w.hwnd) {
                            input::run(w.hwnd, &[Step::Key(0x56)]).map_err(|e| anyhow::anyhow!(e))?;
                            std::thread::sleep(std::time::Duration::from_millis(600));
                        }
                        writeln!(out, "inventário aberto agora: {}", nav::inventory_open(w.hwnd))?
                    }
                    _ => anyhow::bail!("passo desconhecido: {a}"),
                }
                writeln!(out, "ok {a}")?;
            }
        }
        "fortuna" => {
            // Viagem da Fortuna do Imperador (sem voltar ao spot). Uso: zen-diag fortuna <nome> [tudo]
            use zen_agent::win::nav;
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let all = args.iter().any(|a| a == "tudo");
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let lor = nav::world_terrain_of(w.hwnd, nav::LORENCIA_WORLD).map_err(|e| anyhow::anyhow!(e))?;
            let t0 = std::time::Instant::now();
            match nav::fortune(w.hwnd, &lor, all, &|m| println!("  {m}")) {
                Ok(r) => writeln!(out, "OK em {:?}: {} convertidas, {} para o J, Zen {} -> {}", t0.elapsed(), r.converted, r.to_bank, r.zen_before, r.zen_after)?,
                Err(e) => {
                    if let Some(png) = nav::take_error_shot() {
                        std::fs::write(format!("fortuna_{name}_erro.png"), png)?;
                    }
                    writeln!(out, "FALHOU em {:?}: {e}", t0.elapsed())?
                }
            }
        }
        "jbanco" => {
            // Treino do J: Withdraw 1 e Deposit All com prints. Uso: zen-diag jbanco <nome>
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome"))?.clone();
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let (shots, r) = zen_agent::win::nav::bank_train(w.hwnd, &|m| println!("  {m}"));
            for (t, png) in shots {
                std::fs::write(format!("jbanco_{name}_{t}.png"), png)?;
            }
            match r {
                Ok(()) => writeln!(out, "OK")?,
                Err(e) => writeln!(out, "FALHOU: {e}")?,
            }
        }
        "anda" => {
            // Anda até um tile em qualquer mapa (helper deve estar desligado).
            // Uso: zen-diag anda <nome> <world> <x> <y>
            use zen_agent::win::{input, nav};
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let world: u8 = args.get(2).ok_or_else(|| anyhow::anyhow!("falta o world"))?.parse()?;
            let goal: (i32, i32) = (args.get(3).ok_or_else(|| anyhow::anyhow!("falta x"))?.parse()?, args.get(4).ok_or_else(|| anyhow::anyhow!("falta y"))?.parse()?);
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let exe = zen_agent::win::memory::process_path(w.pid).ok_or_else(|| anyhow::anyhow!("sem caminho do jogo"))?;
            let dir = std::path::Path::new(&exe).parent().ok_or_else(|| anyhow::anyhow!("caminho inválido"))?.to_path_buf();
            let raw = std::fs::read(dir.join(format!("Data\\World{world}\\EncTerrain{world}.att")))?;
            let t = zen_core::terrain::Terrain::from_client_file(&raw).map_err(|e| anyhow::anyhow!("{e}"))?;
            let _s = input::Session::begin(w.hwnd).map_err(|e| anyhow::anyhow!(e))?;
            let r = nav::walk(w.hwnd, &t, goal, &|m| println!("  {m}"));
            writeln!(out, "anda: {r:?}")?;
        }
        "helper" => {
            // Estado do helper pelo ícone e Home só quando precisa, conferindo depois.
            // Uso: zen-diag helper <nome> estado|ligar|desligar
            use zen_agent::win::{capture, input};
            use zen_core::actions::{helper_on, Step, HELPER_ICON, HELPER_WHITE};
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("falta o nome do char"))?.clone();
            let want = args.get(2).map(String::as_str).unwrap_or("estado");
            let w = zen_agent::win::windows_list::find_by_class("The Classic MU Season 2")
                .into_iter()
                .find(|w| zen_core::title::parse(&w.title).name.is_some_and(|n| n.eq_ignore_ascii_case(&name)))
                .ok_or_else(|| anyhow::anyhow!("janela do char {name} não encontrada"))?;
            let state = || {
                capture::capture(w.hwnd, Some(HELPER_ICON)).and_then(|img| {
                    helper_on(&zen_core::glyphs::binarize_white(&img.bgra, img.w as usize, img.h as usize, HELPER_WHITE))
                })
            };
            let now = state();
            writeln!(out, "{name}: helper {}", match now { Some(true) => "LIGADO", Some(false) => "DESLIGADO", None => "?" })?;
            let target = match want {
                "ligar" => Some(true),
                "desligar" => Some(false),
                _ => None,
            };
            if let Some(t) = target {
                match now {
                    None => anyhow::bail!("não reconheci o ícone do helper: não aperto Home às cegas"),
                    Some(s) if s == t => writeln!(out, "já está como pedido")?,
                    Some(_) => {
                        input::run(w.hwnd, &[Step::Key(0x24)]).map_err(|e| anyhow::anyhow!(e))?;
                        std::thread::sleep(std::time::Duration::from_millis(1300));
                        let after = state();
                        writeln!(out, "Home enviado; agora: {after:?}")?;
                        if after != Some(t) {
                            anyhow::bail!("o helper não ficou como pedido (ícone {after:?})");
                        }
                    }
                }
            }
        }
        "ocioso" => {
            // Segundos sem uso do PC, como o agente mede (GetLastInputInfo), 5 amostras.
            for _ in 0..5 {
                writeln!(out, "sem uso há {} s", zen_agent::win::input::idle_secs())?;
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
        "entrada" => {
            // Quem está gerando input? Conta eventos de teclado/mouse por 15 s separando FÍSICO
            // (dispositivo real) de INJETADO (algum programa: SendInput, macro, jiggler...).
            // Só observa: os eventos seguem normalmente (CallNextHookEx).
            use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
            use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::UI::WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, PeekMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
                KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_REMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL,
                WM_MOUSEMOVE,
            };
            static MI: AtomicU64 = AtomicU64::new(0);
            static MP: AtomicU64 = AtomicU64::new(0);
            static MMI: AtomicU64 = AtomicU64::new(0);
            static MMP: AtomicU64 = AtomicU64::new(0);
            static KI: AtomicU64 = AtomicU64::new(0);
            static KP: AtomicU64 = AtomicU64::new(0);
            unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
                if code >= 0 {
                    let m = &*(l.0 as *const MSLLHOOKSTRUCT);
                    let inj = m.flags & LLMHF_INJECTED != 0;
                    let mv = w.0 as u32 == WM_MOUSEMOVE;
                    match (inj, mv) {
                        (true, true) => MMI.fetch_add(1, Relaxed),
                        (false, true) => MMP.fetch_add(1, Relaxed),
                        (true, false) => MI.fetch_add(1, Relaxed),
                        (false, false) => MP.fetch_add(1, Relaxed),
                    };
                }
                CallNextHookEx(None, code, w, l)
            }
            unsafe extern "system" fn kb(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
                if code >= 0 {
                    let k = &*(l.0 as *const KBDLLHOOKSTRUCT);
                    if k.flags.0 & LLKHF_INJECTED.0 != 0 {
                        KI.fetch_add(1, Relaxed);
                    } else {
                        KP.fetch_add(1, Relaxed);
                    }
                }
                CallNextHookEx(None, code, w, l)
            }
            unsafe {
                let inst = GetModuleHandleW(None)?;
                let hinst = windows::Win32::Foundation::HINSTANCE::from(inst);
                let hm = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), hinst, 0)?;
                let hk = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb), hinst, 0)?;
                let t0 = std::time::Instant::now();
                let mut msg = MSG::default();
                while t0.elapsed().as_secs() < 15 {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                let _ = UnhookWindowsHookEx(hm);
                let _ = UnhookWindowsHookEx(hk);
            }
            writeln!(out, "15 s de observação:")?;
            writeln!(out, "  mouse movimento: físico {}  injetado {}", MMP.load(Relaxed), MMI.load(Relaxed))?;
            writeln!(out, "  mouse botões/roda: físico {}  injetado {}", MP.load(Relaxed), MI.load(Relaxed))?;
            writeln!(out, "  teclado: físico {}  injetado {}", KP.load(Relaxed), KI.load(Relaxed))?;
            writeln!(out, "  sem uso (GetLastInputInfo) no fim: {} s", zen_agent::win::input::idle_secs())?;
        }
        "zen" => {
            let v: u32 = args.get(2).ok_or_else(|| anyhow::anyhow!("falta o valor"))?.parse()?;
            let t = std::time::Instant::now();
            let found = zen_agent::win::memory::scan_u32(pid(1)?, v);
            writeln!(out, "valor {v}: {} ocorrências em {:?}", found.len(), t.elapsed())?;
            for a in found.iter().take(200) {
                writeln!(out, "  {a:#x}")?;
            }
        }
        // v5 (17/11): procura textos na memória do jogo (sem diferenciar maiúsculas), com o
        // endereço e o trecho em volta. Ex.: zen-diag memtexto <pid> nable "Pick Zen"
        "memtexto" => {
            let p = Proc::open(pid(1)?).ok_or_else(|| anyhow::anyhow!("não abriu o processo (rode como admin)"))?;
            // Cada palavra em texto simples e em UTF-16 (2 bytes por letra), sem diferenciar maiúsculas.
            let needles: Vec<Vec<u8>> = args.iter().skip(2).flat_map(|a| {
                let l = a.to_ascii_lowercase();
                [l.clone().into_bytes(), l.encode_utf16().flat_map(|c| c.to_le_bytes()).collect::<Vec<u8>>()]
            }).collect();
            let t = std::time::Instant::now();
            const CHUNK: usize = 1 << 20;
            let mut buf = vec![0u8; CHUNK + 256];
            let mut hits: Vec<(u64, String)> = Vec::new();
            for (base, size) in p.writable_regions() {
                let mut off = 0usize;
                while off < size {
                    let n = (CHUNK + 256).min(size - off);
                    if p.read(base + off as u64, &mut buf[..n]) {
                        let low: Vec<u8> = buf[..n].iter().map(|b| b.to_ascii_lowercase()).collect();
                        for nd in &needles {
                            let mut i = 0;
                            while let Some(pos) = low[i..].windows(nd.len()).position(|w| w == nd.as_slice()) {
                                let s0 = i + pos;
                                if s0 >= CHUNK.min(n) { break; }
                                let ini = s0.saturating_sub(48);
                                let fim = (s0 + 80).min(n);
                                let utf16 = nd.len() > 1 && nd[1] == 0;
                                let txt: String = buf[ini..fim].iter().filter(|&&b| !(utf16 && b == 0)).map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
                                let txt = if utf16 { format!("[utf16] {txt}") } else { txt };
                                hits.push((base + off as u64 + s0 as u64, txt));
                                i = s0 + nd.len();
                                if hits.len() > 600 { break; }
                            }
                        }
                    }
                    off += CHUNK.min(size - off);
                }
            }
            writeln!(out, "{} ocorrências em {:?}", hits.len(), t.elapsed())?;
            hits.sort_by(|a, b| a.1.cmp(&b.1));
            hits.dedup_by(|a, b| a.1 == b.1);
            for (a, t) in hits.iter().take(300) {
                writeln!(out, "{a:#012x}  {t}")?;
            }
        }
        "avisos" => {
            // Mostra a região onde moram os avisos amarelos do servidor e os textos dela.
            // uso: avisos <pid> [endereço hex, padrão 813e000] [bytes, padrão 0x3000]
            let p = Proc::open(pid(1)?).ok_or_else(|| anyhow::anyhow!("não abriu o processo (rode como admin)"))?;
            let addr = args.get(2).map(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16)).transpose()?.unwrap_or(0x813e000);
            let len = args.get(3).map(|a| usize::from_str_radix(a.trim_start_matches("0x"), 16)).transpose()?.unwrap_or(0x3000);
            match p.region(addr) {
                Some((ab, b, sz, ty, pr)) => {
                    let tipo = match ty { 0x1000000 => "IMAGE", 0x20000 => "PRIVATE", 0x40000 => "MAPPED", _ => "?" };
                    writeln!(out, "região de {addr:#x}: alocação {ab:#x}  base {b:#x}  tamanho {sz:#x}  tipo {tipo}  proteção {pr:#x}  (offset na alocação {:#x})", addr - ab)?;
                    if let Some(img) = p.region(0x400000) {
                        writeln!(out, "região 0x400000 (main.exe?): alocação {:#x} tipo {:#x}", img.0, img.3)?;
                    }
                }
                None => writeln!(out, "região de {addr:#x}: não consultou")?,
            }
            let ini = addr.saturating_sub(len as u64 / 4);
            let mut buf = vec![0u8; len];
            if !p.read(ini, &mut buf) {
                writeln!(out, "não leu {len:#x} bytes em {ini:#x}")?;
            }
            let mut i = 0;
            while i < buf.len() {
                let fim = buf[i..].iter().position(|&b| !(32..127).contains(&b)).map(|e| i + e).unwrap_or(buf.len());
                if fim - i >= 8 {
                    let t: String = buf[i..fim].iter().map(|&b| b as char).collect();
                    writeln!(out, "{:#010x}  {t}", ini + i as u64)?;
                }
                i = fim + 1;
            }
        }
        "avisos-vigia" => {
            // Lê a fila de avisos a cada 2 s e mostra cada mudança, posição por posição.
            // uso: avisos-vigia <pid> <segundos>
            let p = Proc::open(pid(1)?).ok_or_else(|| anyhow::anyhow!("não abriu o processo (rode como admin)"))?;
            let secs: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(600);
            const ANCORA: u64 = 0x813ddd0;
            const PASSO: u64 = 0x108;
            let ler = || -> Vec<String> {
                (-8i64..=8)
                    .map(|j| {
                        let a = (ANCORA as i64 + j * PASSO as i64) as u64;
                        let mut b = vec![0u8; 0x80];
                        if !p.read(a, &mut b) {
                            return "<sem leitura>".into();
                        }
                        let fim = b.iter().rposition(|&x| x != 0).map(|e| e + 1).unwrap_or(0);
                        b[..fim].iter().map(|&x| if (32..127).contains(&x) { (x as char).to_string() } else { format!("\\x{x:02x}") }).collect()
                    })
                    .collect()
            };
            let t0 = std::time::Instant::now();
            let mut antes: Vec<String> = Vec::new();
            while t0.elapsed().as_secs() < secs {
                let agora = ler();
                if agora != antes {
                    writeln!(out, "--- {} s", t0.elapsed().as_secs())?;
                    for (j, t) in agora.iter().enumerate() {
                        let mudou = antes.get(j) != Some(t);
                        if !t.is_empty() || mudou {
                            writeln!(out, "{}{:#x} [{:+}] {t}", if mudou { "*" } else { " " }, (ANCORA as i64 + (j as i64 - 8) * PASSO as i64) as u64, j as i64 - 8)?;
                        }
                    }
                    antes = agora;
                    std::fs::write("resultado_avisos-vigia.txt", &out)?;
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
        "chat" | "listachat" => {
            let p = Proc::open(pid(1)?).ok_or_else(|| anyhow::anyhow!("não abriu o processo (rode como admin)"))?;
            let t = std::time::Instant::now();
            let lines = find_lines(&p);
            writeln!(out, "{} linhas em {:?}", lines.len(), t.elapsed())?;
            for (a, txt) in &lines {
                writeln!(out, "{a:#010x}  {txt}")?;
            }
            if cmd == "listachat" {
                writeln!(out, "\n=== ponteiros para as linhas (4 e 8 bytes) ===")?;
                let targets: std::collections::HashSet<u64> = lines.iter().map(|(a, _)| *a).collect();
                let hits = find_pointers(&p, &targets);
                // Agrupa ponteiros vizinhos: uma lista de chat aparece como vários
                // ponteiros em sequência com passo fixo.
                let mut groups: Vec<Vec<(u64, u64)>> = Vec::new();
                for h in hits {
                    match groups.last_mut() {
                        Some(g) if h.0 - g.last().unwrap().0 <= 0x200 => g.push(h),
                        _ => groups.push(vec![h]),
                    }
                }
                groups.retain(|g| g.len() >= 3);
                writeln!(out, "{} grupos com 3+ ponteiros vizinhos:", groups.len())?;
                for g in &groups {
                    let steps: Vec<u64> = g.windows(2).map(|w| w[1].0 - w[0].0).collect();
                    writeln!(out, "grupo em {:#x}: {} ponteiros, passos {:?}", g[0].0, g.len(), steps)?;
                    for (at, to) in g {
                        let txt = lines
                            .iter()
                            .find(|(a, _)| a == to)
                            .map(|(_, t)| t.as_str())
                            .unwrap_or("?");
                        writeln!(out, "    {at:#x} -> {to:#x}  {txt}")?;
                    }
                }
            }
        }
        _ => {
            eprintln!("uso: zen-diag janelas | ocr [x y w h] | vida-coleta [seg] [x y w h] | vida-mem [seg] [nome] | tecla <nome> [V|C] | tecla-input <nome> [V|C] | pos-coleta [seg] [x y w h] | pos-mem [seg] [nome] | calibra <nome> [cx cy] | calibra2 <nome> [cx cy] | navega <nome> <att> <x> <y> | navega2 <nome> <att> <x> <y> [vx vy] | zen <pid> <valor> | chat <pid> | listachat <pid>");
            std::process::exit(2);
        }
    }

    let file = format!("resultado_{cmd}.txt");
    std::fs::write(&file, &out)?;
    println!("{out}");
    println!("\nGravado em {file}. Mande este arquivo.");
    Ok(())
}

#[cfg(windows)]
fn find_lines(p: &zen_agent::win::memory::Proc) -> Vec<(u64, String)> {
    const NEEDLES: [&[u8]; 2] = [b"[PK] : ", b"[PickItem]"];
    let mut out = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            let n = buf.len().min(size - off);
            if p.read(base + off as u64, &mut buf[..n]) {
                let hay = &buf[..n];
                for needle in NEEDLES {
                    let mut i = 0;
                    while let Some(pos) = hay[i..].windows(needle.len()).position(|w| w == needle) {
                        let s = i + pos;
                        // Volta até o começo da string (byte 0 anterior), no máx. 40 bytes.
                        let mut st = s;
                        while st > 0 && s - st < 40 && hay[st - 1] != 0 {
                            st -= 1;
                        }
                        let end = hay[s..]
                            .iter()
                            .take(160)
                            .position(|&b| b == 0)
                            .map(|e| s + e)
                            .unwrap_or(s);
                        let txt: String = hay[st..end].iter().map(|&b| b as char).collect();
                        out.push((base + off as u64 + st as u64, txt));
                        i = s + needle.len();
                    }
                }
            }
            off += n;
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(windows)]
fn find_pointers(p: &zen_agent::win::memory::Proc, targets: &std::collections::HashSet<u64>) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            let n = buf.len().min(size - off);
            if p.read(base + off as u64, &mut buf[..n]) {
                for i in (0..n.saturating_sub(8)).step_by(4) {
                    let v4 = u64::from(u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()));
                    let v8 = u64::from_le_bytes(buf[i..i + 8].try_into().unwrap());
                    if targets.contains(&v4) {
                        out.push((base + off as u64 + i as u64, v4));
                    } else if i % 8 == 0 && targets.contains(&v8) {
                        out.push((base + off as u64 + i as u64, v8));
                    }
                }
            }
            off += n;
        }
    }
    out.sort();
    out
}
