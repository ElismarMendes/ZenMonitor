//! fake-client.exe — NUNCA é o jogo. Serve para testar o agente sem abrir o MU.
//!
//! Imita: classe "The Classic MU Season 2", título no formato real, Zen como u32 no heap
//! (+ 2 cópias "lixo" que não acompanham), o valor em vermelho no mesmo lugar do campo
//! do inventário do jogo (região padrão do agent.toml) para o OCR, linhas de chat em 6 cópias que trocam de lugar, e uma conexão TCP.
//!
//! Teclas: F = farm liga/desliga | B = baú (+500M) | P = gera PK | D = derruba TCP
//!         C = reconecta TCP     | Z = Zen no limite (1.999.500.000)
//! Uso: fake-client.exe [NomeDoChar] [zen_inicial]
//!
//! Controle sem teclado (para testes automáticos): escreva comandos, um por linha, em
//! `fake-control-<Nome>.txt` ao lado do .exe. O arquivo é lido e apagado a cada 1 s.
//!   farm on | farm off | bau | limite | zen <valor> | pk | dc | reconectar | sair
//!   morrer (vida 0 por 6 s e renasce cheia) | vida <valor>
//! Vida: u16 no heap (+ a vida máxima fixa, que o agente precisa descartar), desenhada em
//! branco no lugar do número da esfera vermelha (hp_region padrão), com a fonte do jogo.
//! A cada 1 s o estado real vai para `fake-status-<Nome>.txt` (zen=, farm=, tcp=), para o
//! teste comparar o que o agente leu com o valor verdadeiro.

#[cfg(not(windows))]
fn main() {
    eprintln!("fake-client só roda no Windows.");
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(windows)]
mod app {
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    struct Game {
        name: String,
        zen: Box<u32>,
        #[allow(dead_code)] // só existem para o agente achar e descartar
        junk: Vec<Box<u32>>,
        farming: bool,
        chat: Vec<Box<[u8; 160]>>,
        tcp: Option<TcpStream>,
        listener: TcpListener,
        tick: u64,
        hp: Box<u16>,
        #[allow(dead_code)] // vida máxima: igual à vida quando cheia, o agente descarta
        hp_max: Box<u16>,
        respawn_in: u32,
        hp_tick: u32,
    }

    const HP_MAX: u16 = 6686;

    static GAME: Mutex<Option<Game>> = Mutex::new(None);

    fn title(g: &Game) -> String {
        format!(
            "TheClassic MU || Name: {} || Level: 400 || Resets: 911 || WC: 0 || WP: 21,519 || GP: 15,840.0",
            g.name
        )
    }

    fn fmt_commas(n: u32) -> String {
        let s = n.to_string();
        let mut o = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                o.push(',');
            }
            o.push(c);
        }
        o
    }

    fn write_chat(g: &mut Game, line: &str) {
        // Cópias "andam": nova linha vai para slots diferentes a cada vez.
        let n = g.chat.len();
        for k in 0..6 {
            let slot = &mut g.chat[(g.tick as usize * 3 + k * 7) % n];
            slot.fill(0);
            let b = line.as_bytes();
            slot[..b.len().min(159)].copy_from_slice(&b[..b.len().min(159)]);
        }
    }

    fn control_paths() -> (std::path::PathBuf, std::path::PathBuf) {
        let name = GAME
            .lock()
            .unwrap()
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        (
            dir.join(format!("fake-control-{name}.txt")),
            dir.join(format!("fake-status-{name}.txt")),
        )
    }

    /// Lê e apaga o arquivo de controle, aplica os comandos e grava o estado real.
    unsafe fn control_tick(hwnd: HWND) {
        let (ctl, status) = control_paths();
        let mut quit = false;
        if let Ok(text) = std::fs::read_to_string(&ctl) {
            let _ = std::fs::remove_file(&ctl);
            let mut guard = GAME.lock().unwrap();
            let g = guard.as_mut().unwrap();
            for line in text.lines() {
                let mut it = line.split_whitespace();
                let cmd = it.next().unwrap_or("").to_ascii_lowercase();
                let arg = it.next().unwrap_or("").to_ascii_lowercase();
                match (cmd.as_str(), arg.as_str()) {
                    ("farm", "off") => g.farming = false,
                    ("farm", "on") => g.farming = true,
                    ("bau", _) => *g.zen = (*g.zen).saturating_add(500_000_000).min(2_000_000_000),
                    ("limite", _) => *g.zen = 1_999_500_000,
                    ("zen", v) => {
                        if let Ok(v) = v.parse::<u32>() {
                            *g.zen = v.min(2_000_000_000);
                        }
                    }
                    ("pk", _) => {
                        let l = format!("[PK] : Golden Saty killed {}", g.name);
                        write_chat(g, &l);
                    }
                    ("dc", _) => g.tcp = None,
                    ("reconectar", _) => reconnect(g),
                    ("sair", _) => quit = true,
                    ("morrer", _) => {
                        *g.hp = 0;
                        g.respawn_in = 6;
                    }
                    ("vida", v) => {
                        if let Ok(v) = v.parse::<u16>() {
                            *g.hp = v;
                        }
                    }
                    _ => {}
                }
            }
        }
        {
            let mut guard = GAME.lock().unwrap();
            if let Some(g) = guard.as_mut() {
                // Vida: farmando, toma dano e recupera (muda quase todo segundo). Morto: 0 até renascer.
                g.hp_tick += 1;
                if g.respawn_in > 0 {
                    g.respawn_in -= 1;
                    if g.respawn_in == 0 {
                        *g.hp = HP_MAX;
                    }
                } else if g.farming && *g.hp > 0 {
                    *g.hp = if g.hp_tick % 3 == 0 { HP_MAX } else { 6000 + ((g.hp_tick * 131) % 680) as u16 };
                }
                let _ = std::fs::write(
                    &status,
                    format!(
                        "zen={}\nfarm={}\ntcp={}\nhp={}\n",
                        *g.zen,
                        g.farming as u8,
                        g.tcp.is_some() as u8,
                        *g.hp
                    ),
                );
            }
        }
        let _ = InvalidateRect(hwnd, None, true);
        if quit {
            let _ = DestroyWindow(hwnd);
        }
    }

    fn reconnect(g: &mut Game) {
        let addr = g.listener.local_addr().unwrap();
        g.tcp = TcpStream::connect(addr).ok();
    }

    pub fn run() {
        let mut args = std::env::args().skip(1);
        let name = args.next().unwrap_or_else(|| "FakeChar".into());
        let zen: u32 = args.next().and_then(|z| z.parse().ok()).unwrap_or(423_485_898);
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let mut g = Game {
            name,
            zen: Box::new(zen),
            junk: vec![Box::new(zen), Box::new(zen)],
            farming: true,
            chat: (0..24).map(|_| Box::new([0u8; 160])).collect(),
            tcp: None,
            listener,
            tick: 0,
            hp: Box::new(HP_MAX),
            hp_max: Box::new(HP_MAX),
            respawn_in: 0,
            hp_tick: 0,
        };
        reconnect(&mut g);
        let t = title(&g);
        *GAME.lock().unwrap() = Some(g);

        unsafe {
            let inst = GetModuleHandleW(None).unwrap();
            let class = w!("The Classic MU Season 2");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: inst.into(),
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let title_w: Vec<u16> = t.encode_utf16().chain(Some(0)).collect();
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                PCWSTR(title_w.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1030, // mesmo tamanho da janela do jogo real (as regiões são medidas nela)
                797,
                None,
                None,
                inst,
                None,
            )
            .unwrap();
            SetTimer(hwnd, 1, 5000, None);
            SetTimer(hwnd, 2, 1000, None); // arquivo de controle
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_TIMER if wp.0 == 2 => {
                control_tick(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                let mut guard = GAME.lock().unwrap();
                let g = guard.as_mut().unwrap();
                g.tick += 1;
                if g.farming && *g.zen < 2_000_000_000 {
                    *g.zen = (*g.zen + 250_000 + (g.tick as u32 % 7) * 50_000).min(2_000_000_000);
                }
                let _ = InvalidateRect(hwnd, None, true);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let mut guard = GAME.lock().unwrap();
                let g = guard.as_mut().unwrap();
                match wp.0 as u8 {
                    b'F' => g.farming = !g.farming,
                    b'B' => *g.zen = (*g.zen).saturating_add(500_000_000).min(2_000_000_000),
                    b'Z' => *g.zen = 1_999_500_000,
                    b'P' => {
                        let line = format!("[PK] : Golden Saty killed {}", g.name);
                        write_chat(g, &line);
                    }
                    b'D' => g.tcp = None,
                    b'C' => reconnect(g),
                    _ => {}
                }
                let _ = InvalidateRect(hwnd, None, true);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let guard = GAME.lock().unwrap();
                let g = guard.as_ref().unwrap();
                SetTextColor(hdc, COLORREF(0x00FFFFFF));
                SetBkColor(hdc, COLORREF(0));
                let lines = [
                    format!("Zen {}", fmt_commas(*g.zen)),
                    format!(
                        "farm: {}  tcp: {}  (F B P D C Z)",
                        if g.farming { "ON" } else { "OFF" },
                        if g.tcp.is_some() { "ON" } else { "OFF" }
                    ),
                ];
                for (i, l) in lines.iter().enumerate() {
                    let w: Vec<u16> = l.encode_utf16().collect();
                    let _ = TextOutW(hdc, 40, 40 + i as i32 * 40, &w);
                }
                // Campo do Zen no MESMO lugar do inventário do jogo real: região padrão do
                // agent.toml [786, 609, 78, 14], em coordenadas da janela inteira. Converte
                // para coordenadas da área cliente descontando barra de título e borda.
                let mut wr = RECT::default();
                let _ = GetWindowRect(hwnd, &mut wr);
                let mut origin = POINT { x: 0, y: 0 };
                let _ = ClientToScreen(hwnd, &mut origin);
                let (offx, offy) = (origin.x - wr.left, origin.y - wr.top);
                // Mesma fonte do jogo, pixel a pixel (zen_core::glyphs), em vermelho, na posição
                // em que um número de 9 dígitos aparece no inventário real (coluna 13, linha 5
                // do recorte). O agente lê isto exatamente como lê o jogo.
                let (bx, by) = (786 - offx + 13, 609 - offy + 5);
                for (x, y) in zen_core::glyphs::render(u64::from(*g.zen)) {
                    SetPixel(hdc, bx + x as i32, by + y as i32, COLORREF(0x0000_00E6));
                }
                // Número da vida em branco, na faixa que o agente lê (hp_region [165, 752, 100, 36],
                // faixa começa na linha 12 do recorte).
                let (hx, hy) = (165 - offx + 20, 752 - offy + zen_core::hp::BAND_Y as i32);
                for (x, y) in zen_core::hp::render(u32::from(*g.hp)) {
                    SetPixel(hdc, hx + x as i32, hy + y as i32, COLORREF(0x00FF_FFFF));
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
