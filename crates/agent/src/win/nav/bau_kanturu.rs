use super::*;

/// Tile de onde o baú foi aberto (zona segura do Kanturu, perto de onde o move deixa o char).
pub const VAULT_TILE: (i32, i32) = (33, 217);
/// Tile livre a oeste do baú (caminho de quem chega do teleporte), longe do NPC.
pub(super) const VAULT_EXIT: (i32, i32) = (27, 217);
/// Corpo do Safety Guardian na tela com o char parado em VAULT_TILE.
pub(super) const VAULT_NPC: (i32, i32) = (565, 445);
/// Campo "Zen" do baú (aba atual) e do inventário (mesma fonte vermelha).
pub(super) const STORAGE_ZEN: [i32; 4] = [610, 584, 80, 14];
pub(super) const INV_ZEN: [i32; 4] = [786, 609, 78, 14];
/// Setas "aba anterior" e "próxima aba" no topo do baú. NÃO dão a volta: na 15 a seta
/// direita não faz nada, na 1 a esquerda não faz nada (medido em 08/11).
pub(super) const STORAGE_PREV: (i32, i32) = (511, 49);
pub(super) const STORAGE_NEXT: (i32, i32) = (626, 49);
/// Botão "Deposit" (primeiro botão embaixo do baú).
pub(super) const STORAGE_DEPOSIT: (i32, i32) = (476, 672);
/// Caixa "Enter the amount of Zen you would like to deposit.": campo, OK, CANCEL.
pub(super) const DEPOSIT_INPUT: (i32, i32) = (514, 285);
pub(super) const DEPOSIT_OK: (i32, i32) = (421, 340);
pub(super) const DEPOSIT_CANCEL: (i32, i32) = (605, 340);
pub(super) const VK_BACK: u16 = 0x08;

pub(super) fn read_zen(hwnd: isize, region: [i32; 4]) -> Option<u64> {
    let img = capture::capture(hwnd, Some(region))?;
    zen_core::glyphs::read_number(&zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize))
}

/// Duas leituras iguais com 300 ms de intervalo (o número pode estar mudando).
pub(super) fn read_zen_stable(hwnd: isize, region: [i32; 4]) -> Option<u64> {
    for _ in 0..6 {
        let a = read_zen(hwnd, region);
        sleep(300);
        let b = read_zen(hwnd, region);
        if a.is_some() && a == b {
            return a;
        }
    }
    None
}

/// "Storage N/15" no topo do baú, binarizado (texto branco). `None` = não capturou.
pub fn storage_title(hwnd: isize) -> Option<Bits> {
    let r = zen_core::vault::STORAGE_TITLE;
    let img = capture::capture(hwnd, Some(r))?;
    Some(binarize_white(&img.bgra, img.w as usize, img.h as usize, zen_core::vault::TITLE_WHITE))
}

/// Número da aba na tela (1..15), lido no título.
pub fn storage_tab(hwnd: isize) -> Option<u8> {
    storage_title(hwnd).and_then(|b| zen_core::vault::read_tab(&b))
}

/// Onde o Safety Guardian aparece na tela com o char em `at` (o NPC fica parado; o modelo
/// pixel->tile dá o deslocamento).
pub(super) fn npc_on_screen(at: (i32, i32)) -> (i32, i32) {
    let (dx, dy) = ((VAULT_TILE.0 - at.0) as f64, (VAULT_TILE.1 - at.1) as f64);
    let m = MODEL;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let (px, py) = ((m[1][1] * dx - m[0][1] * dy) / det, (-m[1][0] * dx + m[0][0] * dy) / det);
    (VAULT_NPC.0 + px.round() as i32, VAULT_NPC.1 + py.round() as i32)
}

/// Último print de erro do baú (o agente manda para o central, que guarda para análise).
pub(super) static ERROR_SHOT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// Prints passo a passo (treino do K/J pedido pelo dono): ligado só durante `fortune_bank`.
pub(super) static TRACE: std::sync::Mutex<Option<Vec<(String, Vec<u8>)>>> = std::sync::Mutex::new(None);

pub(super) fn trace(hwnd: isize, tag: &str) {
    let on = TRACE.lock().map(|g| g.is_some()).unwrap_or(false);
    if !on {
        return;
    }
    if let Some(png) = capture::capture(hwnd, None).and_then(|i| i.to_png()) {
        if let Ok(mut g) = TRACE.lock() {
            if let Some(v) = g.as_mut() {
                if v.len() < 12 {
                    v.push((tag.to_string(), png));
                }
            }
        }
    }
}

/// Prints do último treino (cada um: etiqueta, PNG).
pub fn take_trace() -> Vec<(String, Vec<u8>)> {
    TRACE.lock().ok().and_then(|mut g| g.take()).unwrap_or_default()
}

/// Só o K -> J, no lugar onde o char estiver (sem ir à cidade), com prints de cada passo.
pub fn fortune_bank(hwnd: isize, status: &dyn Fn(String)) -> Result<String, String> {
    let _session = input::Session::begin(hwnd)?;
    if let Ok(mut g) = TRACE.lock() {
        *g = Some(Vec::new());
    }
    // O helper pode continuar ligado: K/J funcionam com ele rodando (dono, 09/11 16:33).
    trace(hwnd, "0-inicio");
    let r = gremory_to_bank(hwnd, status);
    trace(hwnd, "9-fim");
    r.map(|n| format!("{n} Fortuna(s) do K para o inventário e Deposit All no J conferido"))
}

/// Treino só do J (ponte, x9901): abre o J, tira 1 Fortuna ("Withdraw 1") e guarda tudo de
/// novo ("Deposit All"), com prints. Devolve os prints (etiqueta, PNG).
pub fn bank_train(hwnd: isize, status: &dyn Fn(String)) -> (Vec<(String, Vec<u8>)>, Result<(), String>) {
    let _session = match input::Session::begin(hwnd) {
        Ok(s) => s,
        Err(e) => return (Vec::new(), Err(e)),
    };
    if let Ok(mut g) = TRACE.lock() {
        *g = Some(Vec::new());
    }
    let r = (|| -> Result<(), String> {
        close_gremory(hwnd);
        if !bank_open(hwnd) {
            input::run(hwnd, &[Step::Key(VK_J)])?;
            let t0 = std::time::Instant::now();
            while t0.elapsed().as_millis() < 3000 && !bank_open(hwnd) {
                sleep(250);
            }
        }
        trace(hwnd, "1-j-aberto");
        let row = bank_row(hwnd).ok_or("não achei a linha da Fortuna no J")?;
        status(format!("J: linha da Fortuna em {row:?}"));
        input::run(hwnd, &[Step::Click { x: row.0 + 21, y: row.1 + 12 }])?;
        let t0 = std::time::Instant::now();
        let mut dep = None;
        while t0.elapsed().as_millis() < 4000 && dep.is_none() {
            sleep(250);
            dep = bank_menu(hwnd);
        }
        let dep = dep.ok_or("o menu da Fortuna não abriu")?;
        status(format!("J: Deposit All em {dep:?}"));
        trace(hwnd, "2-menu");
        // "Withdraw 1" fica 184 px acima do "Deposit All" (print de 09/11: 214 x 398).
        input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 5 - 184 }])?;
        sleep(2000);
        trace(hwnd, "3-retirou-1");
        if bank_menu(hwnd).is_none() {
            let row = bank_row(hwnd).ok_or("depois do Withdraw 1 não achei a linha da Fortuna")?;
            input::run(hwnd, &[Step::Click { x: row.0 + 21, y: row.1 + 12 }])?;
            sleep(1500);
        }
        let dep = bank_menu(hwnd).ok_or("o menu não voltou para o Deposit All")?;
        input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 5 }])?;
        sleep(2000);
        trace(hwnd, "4-depositou");
        input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 42 }])?;
        sleep(700);
        for _ in 0..2 {
            if !bank_open(hwnd) {
                break;
            }
            input::run(hwnd, &[Step::Key(VK_J)])?;
            sleep(700);
        }
        trace(hwnd, "5-fechou");
        Ok(())
    })();
    (take_trace(), r)
}

pub fn take_error_shot() -> Option<Vec<u8>> {
    ERROR_SHOT.lock().ok().and_then(|mut g| g.take())
}

pub(super) fn save_error_shot(hwnd: isize, why: &str) {
    if let Some(png) = capture::capture(hwnd, None).and_then(|i| i.to_png()) {
        let p = crate::config::AgentConfig::dir().join("logs").join(format!(
            "bau-erro-{}.png",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
        ));
        let _ = std::fs::write(&p, &png);
        tracing::warn!(motivo = why, print = %p.display(), "baú: print salvo");
        if let Ok(mut g) = ERROR_SHOT.lock() {
            *g = Some(png);
        }
    }
}

pub fn storage_open(hwnd: isize) -> bool {
    read_zen(hwnd, STORAGE_ZEN).is_some()
}

/// Fecha baú + inventário (V) e confere. Nunca deixa o baú aberto.
/// Se o V não fechar (item preso no cursor depois de um clique no baú: x9901 em 10/11 19:41),
/// ESC força a saída (dica do dono). ESC só com o baú CONFERIDO aberto: com nada aberto ele
/// abre o menu do jogo.
pub(super) fn close_storage(hwnd: isize) {
    for _ in 0..2 {
        if !storage_open(hwnd) && !inventory_open(hwnd) {
            return;
        }
        let _ = input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)]);
        sleep(800);
    }
    force_close_storage(hwnd);
}

pub(super) const VK_ESCAPE: u16 = 0x1B;

/// Baú aberto fora de hora (o clique de andar caiu no Safety Guardian e pegou um item):
/// ESC até 2 vezes, conferindo antes de cada uma. Devolve se o baú estava aberto.
pub(super) fn force_close_storage(hwnd: isize) -> bool {
    if !storage_open(hwnd) {
        return false;
    }
    for i in 0..2 {
        if !storage_open(hwnd) {
            break;
        }
        save_error_shot(hwnd, &format!("baú aberto no caminho: ESC ({})", i + 1));
        let _ = input::run(hwnd, &[Step::Key(VK_ESCAPE)]);
        sleep(700);
    }
    if inventory_open(hwnd) {
        let _ = input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)]);
        sleep(600);
    }
    true
}

pub struct VaultReport {
    /// (número da aba lido na tela, Zen da aba), na ordem em que foram vistas.
    pub tabs: Vec<(Option<u8>, u64)>,
    /// Título de cada aba binarizado (para montar os modelos dos números).
    pub titles: Vec<String>,
    /// Aba usada (1..15) e valor depositado.
    pub deposited: Option<(usize, u64)>,
    pub zen_before: u64,
    pub zen_after: u64,
}

/// Leva o char ao baú, abre, lê as abas e (se `deposit`) deposita tudo menos `reserve` na
/// primeira aba onde o valor inteiro cabe. Confere o resultado na tela. Não volta ao spot.
/// `read_all`: lê as 15 abas mesmo depois de achar uma (só para diagnóstico).
pub fn vault(hwnd: isize, t: &Terrain, reserve: u64, deposit: bool, read_all: bool, status: &dyn Fn(String)) -> Result<VaultReport, String> {
    let _session = input::Session::begin(hwnd)?;
    use zen_core::vault::{amount, deposit_ok, digit_keys, fits};
    let (k, pos) = observe_map(hwnd);
    let pos = pos.ok_or("não li as coordenadas na tela")?;
    if k != Some(true) || cheb(pos, VAULT_TILE) > 25 {
        status("baú: M -> Kanturu".into());
        teleport_kanturu(hwnd)?;
    }
    set_helper(hwnd, false)?;
    status("baú: andando até o Safety Guardian".into());
    // O tile do baú às vezes não aceita o clique final (visto em 08/11: travou 1 tile antes).
    // Ao lado serve: o clique no NPC é corrigido pela diferença.
    let at = match walk(hwnd, t, VAULT_TILE, status) {
        Ok(p) => p,
        Err(e) => match position(hwnd) {
            Some(p) if cheb(p, VAULT_TILE) <= 1 => p,
            _ => return Err(e),
        },
    };
    if cheb(at, VAULT_TILE) > 1 {
        return Err(format!("parou em {at:?}, longe de {VAULT_TILE:?}"));
    }
    if inventory_open(hwnd) {
        input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
        sleep(600);
    }
    // ---- abrir
    // Um clique fora do NPC faz o char ANDAR (visto em 09/11 no LOBOLIXO: parou em (33,218), o
    // clique corrigido errou e o segundo clique já era de outra posição). Então: a cada
    // tentativa, volta ao tile do baú, recalcula onde o NPC aparece e clica uma vez.
    status("baú: abrindo".into());
    let mut opened = false;
    let mut at = at;
    for attempt in 0..3 {
        if attempt > 0 {
            if storage_open(hwnd) {
                opened = true;
                break;
            }
            at = match walk(hwnd, t, VAULT_TILE, status) {
                Ok(p) => p,
                Err(_) => position(hwnd).unwrap_or(at),
            };
            if cheb(at, VAULT_TILE) > 1 {
                break;
            }
        }
        at = confirm_destination(hwnd, Dest::Kanturu, Some((VAULT_TILE,1)))?;
        let npc = npc_on_screen(at);
        status(format!("baú: char em {at:?}, clicando no NPC em {npc:?} (tentativa {})", attempt + 1));
        input::run(hwnd, &[Step::Click { x: npc.0, y: npc.1 }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 6000 {
            sleep(400);
            if storage_open(hwnd) {
                opened = true;
                break;
            }
        }
        if opened {
            break;
        }
        save_error_shot(hwnd, &format!("baú não abriu na tentativa {}", attempt + 1));
    }
    if !opened {
        close_storage(hwnd);
        return Err(format!("o baú não abriu (clique no Safety Guardian, 3 tentativas): {BAU_DICA}"));
    }
    sleep(500);
    let result = (|| -> Result<VaultReport, String> {
        let zen_before = read_zen_stable(hwnd, INV_ZEN).ok_or("não li o Zen do inventário")?;
        let want = amount(zen_before, reserve);
        let mut rep = VaultReport { tabs: Vec::new(), titles: Vec::new(), deposited: None, zen_before, zen_after: zen_before };
        let mut chosen = None;
        // O baú abre na última aba vista. Vai até a 1 (seta esquerda até o título parar de
        // mudar), confere "1" no título e anda para a direita contando: o número é exato.
        let title = |what: &str| storage_title(hwnd).ok_or_else(|| format!("não li o título do baú ({what})"));
        let mut prev = title("início")?;
        let mut tab = None;
        // Até 15 abas para trás; um clique pode se perder (visto em 08/11 22:00 no ReDMooN):
        // só conclui "parou" depois de 3 cliques seguidos sem o título mudar.
        let mut misses = 0;
        for _ in 0..30 {
            if zen_core::vault::read_tab(&prev) == Some(1) {
                tab = Some(1u8);
                break;
            }
            input::run(hwnd, &[Step::Click { x: STORAGE_PREV.0, y: STORAGE_PREV.1 }])?;
            sleep(600 + 300 * misses);
            let t = title("voltando")?;
            if zen_core::vault::same_title(&t, &prev) {
                misses += 1;
                if misses >= 3 {
                    break;
                }
                continue;
            }
            misses = 0;
            prev = t;
        }
        if tab.is_none() && zen_core::vault::read_tab(&prev) == Some(1) {
            tab = Some(1);
        }
        // Cliente em português ("Baú N/15", visto em 09/11 no ELISMAR): os modelos do título
        // são do inglês e não reconhecem nenhum número. A seta não passa da aba 1, então 3
        // cliques seguidos sem mudar = aba 1. O número vira contagem (só para o aviso); a
        // escolha da aba continua pelo Zen lido na tela.
        if tab.is_none() && misses >= 3 && zen_core::vault::read_tab(&prev).is_none() {
            status("baú: título em outro idioma; aba 1 pela contagem".into());
            tab = Some(1);
        }
        let Some(mut tab) = tab else {
            let seen = zen_core::vault::read_tab(&prev).map(|n| format!("o título diz aba {n}")).unwrap_or_else(|| "título não reconhecido".into());
            return Err(format!("não consegui voltar para a aba 1 do baú ({seen})"));
        };
        loop {
            let z = read_zen_stable(hwnd, STORAGE_ZEN).ok_or_else(|| format!("não li o Zen da aba {tab}"))?;
            rep.titles.push(prev.to_text());
            rep.tabs.push((Some(tab), z));
            status(format!("baú: aba {tab} tem {}", zen_core::fmt::int_br(z)));
            if chosen.is_none() && deposit && fits(z, want) {
                chosen = Some((tab as usize, z));
                if !read_all {
                    break;
                }
            }
            if tab >= 15 {
                break;
            }
            let mut moved = None;
            for _ in 0..2 {
                input::run(hwnd, &[Step::Click { x: STORAGE_NEXT.0, y: STORAGE_NEXT.1 }])?;
                sleep(600);
                let t = title("avançando")?;
                if !zen_core::vault::same_title(&t, &prev) {
                    moved = Some(t);
                    break;
                }
            }
            let t = moved.ok_or_else(|| format!("a seta não passou da aba {tab}"))?;
            tab += 1;
            if let Some(n) = zen_core::vault::read_tab(&t) {
                if n != tab {
                    return Err(format!("contagem das abas errada: esperava {tab}, o título diz {n}"));
                }
            }
            prev = t;
        }
        if !deposit {
            return Ok(rep);
        }
        if want == 0 {
            return Err(format!("nada a depositar: char tem {} e a reserva é {}", zen_core::fmt::int_br(zen_before), zen_core::fmt::int_br(reserve)));
        }
        let Some((tab, stored)) = chosen else {
            // nenhuma aba cabe
            return Err(format!("nenhuma das 15 abas cabe {} (baú cheio?)", zen_core::fmt::int_br(want)));
        };
        if read_all {
            return Err("read_all com depósito não é permitido (a aba atual não é a escolhida)".into());
        }
        // ---- depositar
        status(format!("baú: depositando {} na aba {tab}", zen_core::fmt::int_br(want)));
        input::run(hwnd, &[Step::Click { x: STORAGE_DEPOSIT.0, y: STORAGE_DEPOSIT.1 }])?;
        sleep(900);
        let mut steps = vec![Step::Click { x: DEPOSIT_INPUT.0, y: DEPOSIT_INPUT.1 }, Step::WaitMs(200)];
        for _ in 0..12 {
            steps.push(Step::Key(VK_BACK));
        }
        steps.extend(digit_keys(want).into_iter().map(Step::Key));
        steps.push(Step::WaitMs(300));
        if let Err(e) = input::run(hwnd, &steps) {
            let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
            return Err(format!("falhou ao digitar: {e}"));
        }
        input::run(hwnd, &[Step::Click { x: DEPOSIT_OK.0, y: DEPOSIT_OK.1 }])?;
        sleep(1500);
        let after_sto = read_zen_stable(hwnd, STORAGE_ZEN);
        let after_inv = read_zen_stable(hwnd, INV_ZEN);
        match (after_sto, after_inv) {
            (Some(s), Some(z)) if deposit_ok(stored, s, zen_before, z, want) => {
                rep.deposited = Some((tab, want));
                rep.zen_after = z;
                Ok(rep)
            }
            (s, z) => {
                // A caixa pode ter ficado aberta: CANCEL não faz mal se já fechou.
                let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
                Err(format!(
                    "depósito NÃO conferiu: aba {tab} {} -> {:?}, char {} -> {:?} (esperado +/- {})",
                    zen_core::fmt::int_br(stored),
                    s,
                    zen_core::fmt::int_br(zen_before),
                    z,
                    zen_core::fmt::int_br(want)
                ))
            }
        }
    })();
    if let Err(e) = &result {
        save_error_shot(hwnd, e);
    }
    close_storage(hwnd);
    result
}

// =====================================================================================
// Fortuna do Imperador (Lorencia). Testado à mão pela ponte no ReDMooN em 09/11/2026 13:55-14:11:
// move Lorencia (2.000 zen), baú (saque, taxa 200 tirada do inventário), Lugard (clicar com
// >= 1B no inventário converte 1B na hora; a Fortuna vai para o Gremory Case, tecla K),
// K -> linha "Fortuna do Imperador" -> "Use It" (vai para o inventário), J -> Item Bank ->
// "Fortuna do Imperador" -> "Deposit All". Volta: move Kanturu custa 20M.
// Regras do dono (09/11 14:15-14:17): volta com 20M; se faltar para o move, busca no baú;
// a aba 1 do baú guarda sempre 1B (reserva para o move/emergência), nunca convertida.
// =====================================================================================
