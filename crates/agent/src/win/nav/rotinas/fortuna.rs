use crate::win::nav::*;

/// Aviso que vai para o celular (CommandResult de falha) quando o baú não abre: nos testes de
/// 15/11 o ClassicSM não abria o baú porque o cliente estava desatualizado e o launcher
/// pedia atualização. Só o dono consegue resolver; o bot não deve ficar insistindo.
pub(in crate::win::nav) const BAU_DICA: &str = "verifique se o jogo precisa de atualização no launcher (o baú não abriu)";

/// Aproxima uma vez e repete no máximo um clique no baú, sem voltar ao tile.
pub(in crate::win::nav) fn open_lor_vault(hwnd: isize, t: &Terrain, status: &dyn Fn(String)) -> Result<(), String> {
    ensure_action_map(hwnd, Dest::Lorencia)?;
    if storage_open(hwnd) {
        return Ok(());
    }
    close_inventory(hwnd);
    // Já ao lado do Safety Guardian (x9901 16/11 06:32, em (145,109) depois do J): andar até o
    // tile do baú fazia o clique cair no NPC, o baú abria "no caminho", era fechado como janela
    // acidental e a Fortuna falhava. Perto (até 3 tiles) clica direto no Guardian.
    // "Perto" é perto do NPC, não do tile do baú: o Lugard (149,114) também fica a 3 tiles do
    // baú, mas de lá o Guardian cai na borda da tela (x9901 16/11 07:15, clique em (15,489)).
    let by_guard = |p: (i32, i32)| (p.0 as f64 - LOR_GUARD.0).abs().max((p.1 as f64 - LOR_GUARD.1).abs()) <= 2.5;
    let near = position(hwnd).is_some_and(|p| cheb(p, LOR_VAULT_TILE) <= 3 && by_guard(p));
    if !near {
        walk_near(hwnd, t, LOR_VAULT_TILE, status)?;
    }
    // O clique no Guardian que não abre o baú leva o char para o lado dele, em (145,109), a 3 tiles
    // do tile do baú (AMUMU 16/11 06:45). De lá o 2º clique abre (visto no x9901): tolerância 3.
    let tol = 3;
    let _ = near;
    for attempt in 0..2 {
        npc_still(hwnd)?;
        let at = match confirm_destination(hwnd, Dest::Lorencia, Some((LOR_VAULT_TILE,tol))) {
            Ok(p) => p,
            // Depois de um clique que não abriu, o char anda até o NPC e sai da conferência do tile:
            // o que o dono precisa saber é que o baú não abriu, não o erro de posição.
            Err(e) if attempt > 0 => return Err(format!("{BAU_DICA}; {e}")),
            Err(e) => return Err(e),
        };
        if cheb(at, LOR_VAULT_TILE) > 2 && !by_guard(at) {
            return Err(format!("baú: clique deslocou o char para {at:?}; parei sem voltar e repetir a rota; {BAU_DICA}"));
        }
        if storage_open(hwnd) {
            return Ok(());
        }
        let npc = npc_px(at, LOR_GUARD);
        status(format!("baú: char em {at:?}, clicando no Safety Guardian em {npc:?} (tentativa {})", attempt + 1));
        input::run(hwnd, &[Step::Click { x: npc.0, y: npc.1 }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 6000 {
            sleep(400);
            if storage_open(hwnd) {
                sleep(500);
                return Ok(());
            }
        }
        save_error_shot(hwnd, &format!("baú de Lorencia não abriu na tentativa {}", attempt + 1));
    }
    Err(format!("o baú de Lorencia não abriu (2 cliques): {BAU_DICA}"))
}

/// Com o baú aberto: volta para a aba 1 (seta esquerda até parar; título em inglês confere).
pub(in crate::win::nav) fn storage_first_tab(hwnd: isize) -> Result<Bits, String> {
    let title = || storage_title(hwnd).ok_or_else(|| "não li o título do baú".to_string());
    let mut prev = title()?;
    let mut misses = 0;
    for _ in 0..30 {
        if zen_core::vault::read_tab(&prev) == Some(1) {
            return Ok(prev);
        }
        input::run(hwnd, &[Step::Click { x: STORAGE_PREV.0, y: STORAGE_PREV.1 }])?;
        sleep(600 + 300 * misses);
        let t = title()?;
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
    match zen_core::vault::read_tab(&prev) {
        Some(1) => Ok(prev),
        None if misses >= 3 => Ok(prev), // título em outro idioma: 3 cliques sem mudar = aba 1
        other => Err(format!("não consegui voltar para a aba 1 do baú (título: {other:?})")),
    }
}


/// Saque conferido: aba perde `amount`, inventário ganha `amount` - taxa.
pub(in crate::win::nav) fn withdraw(hwnd: isize, amount: u64, stored: u64, inv: u64) -> Result<u64, String> {
    use zen_core::vault::digit_keys;
    input::run(hwnd, &[Step::Click { x: STORAGE_WITHDRAW.0, y: STORAGE_WITHDRAW.1 }])?;
    sleep(1000);
    let mut steps = vec![Step::Click { x: DEPOSIT_INPUT.0, y: DEPOSIT_INPUT.1 }, Step::WaitMs(200)];
    for _ in 0..12 {
        steps.push(Step::Key(VK_BACK));
    }
    steps.extend(digit_keys(amount).into_iter().map(Step::Key));
    steps.push(Step::WaitMs(300));
    if let Err(e) = input::run(hwnd, &steps) {
        let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
        return Err(format!("falhou ao digitar: {e}"));
    }
    input::run(hwnd, &[Step::Click { x: DEPOSIT_OK.0, y: DEPOSIT_OK.1 }])?;
    sleep(1500);
    let s = read_zen_stable(hwnd, STORAGE_ZEN);
    let z = read_zen_stable(hwnd, INV_ZEN);
    let want_inv = (inv + amount).saturating_sub(WITHDRAW_FEE);
    match (s, z) {
        // Taxa real observada em 15/11 no RAG9LEL: ~6.200 (não 200) com a aba debitada exata.
        // A aba é a prova do saque; o char pode receber até WITHDRAW_SLACK a menos.
        (Some(s), Some(z)) if s + amount == stored && z <= inv + amount && z + WITHDRAW_SLACK >= want_inv => {
            let fee = (inv + amount).saturating_sub(z);
            if fee > 2 * WITHDRAW_FEE { tracing::warn!(taxa = fee, "saque: taxa maior que a esperada, aceita"); }
            Ok(z)
        }
        (s, z) => {
            let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
            Err(format!("saque de {} NÃO conferiu: aba {} -> {s:?}, char {} -> {z:?}", zen_core::fmt::int_br(amount), zen_core::fmt::int_br(stored), zen_core::fmt::int_br(inv)))
        }
    }
}

/// Abaixo disto a aba 1 é completada de volta para `TAB1_RESERVE` (regra do dono, 09/11 14:58:
/// a aba 1 é o dinheiro dos moves; margem de 600M antes de repor).
pub const TAB1_LOW: u64 = 400_000_000;

/// Depósito conferido na aba aberta (mesma caixa do depósito do Kanturu).
pub(in crate::win::nav) fn deposit_here(hwnd: isize, amount: u64, stored: u64, inv: u64) -> Result<u64, String> {
    use zen_core::vault::{deposit_ok, digit_keys};
    input::run(hwnd, &[Step::Click { x: STORAGE_DEPOSIT.0, y: STORAGE_DEPOSIT.1 }])?;
    sleep(1000);
    let mut steps = vec![Step::Click { x: DEPOSIT_INPUT.0, y: DEPOSIT_INPUT.1 }, Step::WaitMs(200)];
    for _ in 0..12 {
        steps.push(Step::Key(VK_BACK));
    }
    steps.extend(digit_keys(amount).into_iter().map(Step::Key));
    steps.push(Step::WaitMs(300));
    if let Err(e) = input::run(hwnd, &steps) {
        let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
        return Err(format!("falhou ao digitar: {e}"));
    }
    input::run(hwnd, &[Step::Click { x: DEPOSIT_OK.0, y: DEPOSIT_OK.1 }])?;
    sleep(1500);
    match (read_zen_stable(hwnd, STORAGE_ZEN), read_zen_stable(hwnd, INV_ZEN)) {
        (Some(s), Some(z)) if deposit_ok(stored, s, inv, z, amount) => Ok(z),
        (s, z) => {
            let _ = input::run(hwnd, &[Step::Click { x: DEPOSIT_CANCEL.0, y: DEPOSIT_CANCEL.1 }]);
            Err(format!("depósito de {} NÃO conferiu: aba {} -> {s:?}, char {} -> {z:?}", zen_core::fmt::int_br(amount), zen_core::fmt::int_br(stored), zen_core::fmt::int_br(inv)))
        }
    }
}

/// Saca do baú de Lorencia até o inventário ter `target` (até 2B exatos), SÓ da aba 1 (dono,
/// 16/11 08:13: o baú 1 é o repositório único da Fortuna em todas as contas). `tab1_first` ou
/// `emergency`: pode usar a aba 1 inteira; sem eles (converter o baú inteiro) preserva 1B nela.
/// `check_reserve` não repõe mais nada (antes buscava nas outras abas). Devolve o Zen final.
pub fn lor_withdraw(hwnd: isize, t: &Terrain, target: u64, emergency: bool, check_reserve: bool, tab1_first: bool, status: &dyn Fn(String)) -> Result<u64, String> {
    use zen_core::vault::ZEN_CAP;
    // O inventário carrega até 2B exatos (chars parados em 2.000.000.000 confirmam): com 2B o
    // Lugard converte duas seguidas. Correção do dono em 09/11 14:57 (antes parava em 1,999B).
    let target = target.min(ZEN_CAP);
    open_lor_vault(hwnd, t, status)?;
    let fmt = zen_core::fmt::int_br;
    let r = (|| -> Result<u64, String> {
        let mut z = read_zen_stable(hwnd, INV_ZEN).ok_or("não li o Zen do inventário")?;
        if z >= target && !check_reserve {
            return Ok(z);
        }
        // Dono, 16/11 08:13: o baú 1 é o repositório ÚNICO da Fortuna em todas as contas. Não
        // procura Zen nas outras abas nem repõe a aba 1 a partir delas.
        storage_first_tab(hwnd)?;
        let s1 = read_zen_stable(hwnd, STORAGE_ZEN).ok_or("não li o Zen da aba 1")?;
        let _ = check_reserve;
        if z < target {
            let need = (target - z + WITHDRAW_FEE).min(ZEN_CAP - z);
            let take = if emergency {
                // Zen do move: usa a aba 1 toda se preciso.
                need.min(s1)
            } else if tab1_first {
                // Fortuna: empresta da aba 1 só se ela continuar acima de 100M.
                let t = fortune_borrow(need, s1, TAB1_REFILL_TRIGGER);
                if t == 0 {
                    status(format!("baú: aba 1 com {} (gatilho {}): sem saque para Fortuna; o Zen do char vai repor a aba 1", fmt(s1), fmt(TAB1_REFILL_TRIGGER)));
                }
                t
            } else {
                // "Converter o baú inteiro": preserva 1B na aba 1.
                need.min(s1.saturating_sub(TAB1_RESERVE))
            };
            if take > 10 * WITHDRAW_FEE {
                status(format!("baú: sacando {} da aba 1", fmt(take)));
                z = withdraw(hwnd, take, s1, z)?;
            }
            if z + WITHDRAW_SLACK < target {
                status(format!("baú: a aba 1 não tem o suficiente (char com {}); não procuro nas outras abas", fmt(z)));
            }
        }
        Ok(z)
    })();
    if let Err(e) = &r {
        save_error_shot(hwnd, e);
    }
    close_storage(hwnd);
    r
}

/// Converte no Lugard enquanto houver >= 1B no inventário. Confere cada conversão pelo Zen.
pub(in crate::win::nav) fn convert_all(hwnd: isize, t: &Terrain, status: &dyn Fn(String)) -> Result<u32, String> {
    ensure_action_map(hwnd, Dest::Lorencia)?;
    let mut z = inv_zen(hwnd)?;
    if z < FORTUNE_ZEN {
        return Ok(0);
    }
    status("Lugard: andando".into());
    walk_near(hwnd, t, LUGARD_TILE, status)?;
    let (mut n, mut fails) = (0u32, 0u32);
    while z >= FORTUNE_ZEN {
        close_inventory(hwnd);
        npc_still(hwnd)?;
        let at = confirm_destination(hwnd, Dest::Lorencia, Some((LUGARD_TILE,2)))?;
        if cheb(at, LUGARD_TILE) > 2 {
            return Err(format!("Lugard: char deslocado para {at:?}; parei sem repetir a aproximação"));
        }
        let p = npc_px(at, LUGARD);
        status(format!("Lugard: char em {at:?}, clicando em {p:?}"));
        input::run(hwnd, &[Step::Click { x: p.0, y: p.1 }])?;
        sleep(2500);
        let z2 = inv_zen(hwnd)?;
        if z2 < z && z - z2 >= FORTUNE_ZEN - 1_000 {
            n += 1;
            fails = 0;
            status(format!("Lugard: {n} Fortuna(s); Zen {} -> {}", zen_core::fmt::int_br(z), zen_core::fmt::int_br(z2)));
            z = z2;
        } else {
            fails += 1;
            save_error_shot(hwnd, &format!("Lugard não converteu (tentativa {fails})"));
            if fails >= 2 {
                return Err(format!("o Lugard não converteu em 2 tentativas (Zen {})", zen_core::fmt::int_br(z2)));
            }
            // Não anda de volta ao tile depois de um clique falho.
        }
    }
    Ok(n)
}

/// J -> Fortuna do Imperador -> Deposit All. Fecha o Item Bank no fim.
pub(in crate::win::nav) fn bank_deposit_all(hwnd: isize) -> Result<(), String> {
    if !bank_open(hwnd) {
        input::run(hwnd, &[Step::Key(VK_J)])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 3000 && !bank_open(hwnd) {
            sleep(250);
        }
    }
    trace(hwnd, "5-j-aberto");
    let Some(row) = bank_row(hwnd) else {
        save_error_shot(hwnd, "Item Bank (J) não abriu");
        return Err("o Item Bank (J) não abriu ou não achei a linha da Fortuna".into());
    };
    // Clica NO TEXTO da linha achada (medido: texto em (486,343), clique bom em (507,355)).
    // O menu demora mais em PC lento: só clica no "Deposit All" quando ele aparece na tela.
    let mut menu = None;
    for _ in 0..2 {
        input::run(hwnd, &[Step::Click { x: row.0 + 21, y: row.1 + 12 }])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 4000 {
            sleep(250);
            menu = bank_menu(hwnd);
            if menu.is_some() {
                break;
            }
        }
        if menu.is_some() {
            break;
        }
    }
    let Some(dep) = menu else {
        save_error_shot(hwnd, "menu da Fortuna no J não abriu");
        return Err("o menu da Fortuna no Item Bank (J) não abriu".into());
    };
    sleep(300);
    trace(hwnd, "6-j-menu");
    // Texto "Deposit All" em (485,393) -> clique bom em (515,398); "Cancel" 37 px abaixo.
    input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 5 }])?;
    sleep(2000);
    trace(hwnd, "7-depositou");
    input::run(hwnd, &[Step::Click { x: dep.0 + 30, y: dep.1 + 42 }])?;
    sleep(700);
    // Fecha o J (tecla J de novo) para não poluir a tela.
    for _ in 0..2 {
        if !bank_open(hwnd) {
            break;
        }
        input::run(hwnd, &[Step::Key(VK_J)])?;
        sleep(700);
    }
    Ok(())
}

/// Fecha o Gremory Case (X; se não fechar, K de novo).
pub(in crate::win::nav) fn close_gremory(hwnd: isize) {
    for k in 0..2 {
        if !gremory_open(hwnd) {
            return;
        }
        let _ = if k == 0 { input::run(hwnd, &[Step::Click { x: GREMORY_X.0, y: GREMORY_X.1 }]) } else { input::run(hwnd, &[Step::Key(VK_K)]) };
        sleep(700);
    }
}

/// Quantos dos 64 espaços do inventário estão ocupados (inventário aberto). Espaço vazio é
/// a grade vermelho-escura; ocupado tem >40% de pixels fora dela (medido em 09/11: vazio ~0%,
/// itens 49-98%).
pub(in crate::win::nav) fn inventory_filled(hwnd: isize) -> Option<u32> {
    let img = capture::capture(hwnd, Some([747, 347, 260, 260]))?;
    let w = img.w as usize;
    let mut n = 0;
    for r in 0..8 {
        for c in 0..8 {
            let (x0, y0) = ((c as f64 * 32.1).round() as usize, (r as f64 * 32.1).round() as usize);
            let (mut other, mut total) = (0u32, 0u32);
            for y in y0 + 4..y0 + 28 {
                for x in x0 + 4..x0 + 28 {
                    let i = (y * w + x) * 4;
                    let Some(p) = img.bgra.get(i..i + 4) else { continue };
                    total += 1;
                    if p[1] > 60 || p[0] > 60 || p[2] < 40 {
                        other += 1;
                    }
                }
            }
            if total > 0 && other * 100 / total > 40 {
                n += 1;
            }
        }
    }
    Some(n)
}

/// Esvazia o Gremory Case (só as linhas "Fortuna do Imperador") para o inventário e guarda
/// tudo no Item Bank (J). Devolve quantas foram usadas.
pub(in crate::win::nav) fn gremory_to_bank(hwnd: isize, status: &dyn Fn(String)) -> Result<u32, String> {
    close_inventory(hwnd);
    if !gremory_open(hwnd) {
        input::run(hwnd, &[Step::Key(VK_K)])?;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 3000 && !gremory_open(hwnd) {
            sleep(250);
        }
    }
    if !gremory_open(hwnd) {
        save_error_shot(hwnd, "Gremory Case (K) não abriu");
        return Err("o Gremory Case (K) não abriu".into());
    }
    trace(hwnd, "1-k-aberto");
    let (mut used, mut pending) = (0u32, 0u32);
    // O K abre junto com o inventário: conta os espaços ocupados antes, para conferir no fim
    // que as Fortunas foram mesmo para o J (dono, 09/11 14:59).
    let filled0 = if inventory_open(hwnd) { inventory_filled(hwnd) } else { None };
    let r = (|| -> Result<(), String> {
        let mut retried = false;
        for _ in 0..70 {
            let Some(i) = (0..9).find(|&i| gremory_row_is_fortune(hwnd, i)) else { break };
            input::run(hwnd, &[Step::Click { x: 300, y: gremory_row_y(i) - 3 }])?;
            sleep(900);
            if !use_dialog_is_fortune(hwnd) {
                // A caixa às vezes demora (x9901 em 11/11 04:27): espera mais um pouco e, se
                // ainda não for a Fortuna, cancela e tenta a linha de novo UMA vez.
                sleep(800);
            }
            if !use_dialog_is_fortune(hwnd) {
                save_error_shot(hwnd, "caixa Use It sem Fortuna");
                let _ = input::run(hwnd, &[Step::Click { x: USE_CANCEL.0, y: USE_CANCEL.1 }]);
                if !retried {
                    retried = true;
                    status("K: a caixa do Use It não mostrou a Fortuna; tentando de novo".into());
                    sleep(800);
                    continue;
                }
                return Err("a caixa do Use It não mostrou Fortuna do Imperador (2 tentativas): parei".into());
            }
            if used == 0 {
                trace(hwnd, "2-caixa-use-it");
            }
            input::run(hwnd, &[Step::Click { x: USE_IT.0, y: USE_IT.1 }])?;
            sleep(1000);
            used += 1;
            pending += 1;
            status(format!("K: {used} Fortuna(s) para o inventário"));
            if pending >= 20 {
                // K fechado antes do J (dono, 09/11 16:01: tela limpa), depois abre o K de novo.
                close_gremory(hwnd);
                bank_deposit_all(hwnd)?;
                pending = 0;
                input::run(hwnd, &[Step::Key(VK_K)])?;
                let t0 = std::time::Instant::now();
                while t0.elapsed().as_millis() < 3000 && !gremory_open(hwnd) {
                    sleep(250);
                }
            }
        }
        // Sempre (mesmo sem Use It nesta viagem): Fortunas que ficaram no inventário de uma
        // viagem anterior também vão para o J.
        {
            trace(hwnd, "3-k-esvaziado");
            close_gremory(hwnd);
            trace(hwnd, "4-k-fechado");
            bank_deposit_all(hwnd)?;
            // Conferência: o inventário tem de voltar ao que era antes do Use It.
            if let Some(n0) = filled0 {
                for attempt in 0..2 {
                    if !inventory_open(hwnd) {
                        input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
                        sleep(900);
                    }
                    trace(hwnd, "8-inventario-conferencia");
                    let n1 = if inventory_open(hwnd) { inventory_filled(hwnd) } else { None };
                    match n1 {
                        Some(n1) if n1 > n0 => {
                            if attempt == 1 {
                                // Só conta espaços, não reconhece o item: itens de evento/drop que
                                // entraram na viagem davam falso alarme (x9901, 15/11 19:25, 5 pilhas
                                // de abóbora). Fortuna que sobrar vai para o J na próxima viagem
                                // (ver o comentário acima), então não aborta: avisa no log + print.
                                save_error_shot(hwnd, "inventário com espaços a mais depois do Deposit All");
                                tracing::warn!(extras = n1 - n0, "Fortuna: inventário com espaços a mais depois do 2º Deposit All; seguindo (pode ser item de evento)");
                                status(format!("J: {} espaço(s) a mais no inventário depois do 2º Deposit All; seguindo (print salvo)", n1 - n0));
                                break;
                            }
                            status(format!("J: {} espaço(s) a mais no inventário; Deposit All de novo", n1 - n0));
                            bank_deposit_all(hwnd)?;
                        }
                        Some(_) => {
                            status("J: conferido, inventário como antes".into());
                            break;
                        }
                        None => break,
                    }
                }
            }
        }
        Ok(())
    })();
    close_gremory(hwnd);
    for _ in 0..2 {
        if bank_open(hwnd) {
            let _ = input::run(hwnd, &[Step::Key(VK_J)]);
            sleep(700);
        }
    }
    close_inventory(hwnd);
    r.map(|_| used)
}

pub struct FortuneReport {
    pub converted: u32,
    pub to_bank: u32,
    pub zen_before: u64,
    pub zen_after: u64,
}

/// Viagem completa até ficar pronto para voltar (não volta ao spot: isso é `recover`).
/// `all` = converte tudo o que houver no baú (menos a reserva de 1B da aba 1).
pub fn fortune(hwnd: isize, lor: &Terrain, all: bool, status: &dyn Fn(String)) -> Result<FortuneReport, String> {
    let logged_status = |message: String| {
        tracing::info!(hwnd, estado = %message, "Fortuna: etapa");
        status(message);
    };
    let status: &dyn Fn(String) = &logged_status;

    let _session = input::Session::begin(hwnd)?;
    // O M funciona com o helper LIGADO (dono, 11/11 08:46): desligar antes deixava o char parado
    // no spot sem helper quando o teleporte falhava (foco perdido) e ele morria. Agora o helper
    // só é desligado já em Lorencia (zona segura).
    if !in_lorencia(hwnd) {
        status("Fortuna: M -> Lorencia (helper continua ligado)".into());
        teleport(hwnd, Dest::Lorencia)?;
    }
    ensure_action_map(hwnd, Dest::Lorencia)?;
    let zen_before = inv_zen(hwnd)?;
    let mut rep = FortuneReport { converted: 0, to_bank: 0, zen_before, zen_after: zen_before };
    let res = (|| -> Result<(), String> {
        rep.converted += convert_all(hwnd, lor, status)?;
        loop {
            // Normal: completa 1B + 20M (converte mais uma e volta com 20M). Tudo: enche até 2B.
            let target = if all { zen_core::vault::ZEN_CAP } else { FORTUNE_ZEN + KEEP_ZEN };
            let z = lor_withdraw(hwnd, lor, target, false, !all, !all, status)?;
            if z < FORTUNE_ZEN {
                break;
            }
            let n = convert_all(hwnd, lor, status)?;
            rep.converted += n;
            if !all || n == 0 {
                break;
            }
        }
        Ok(())
    })();
    // Antes de sair de Lorencia (dono, 09/11 16:16: o CSRF voltou ao spot com 1,98B porque a
    // conversão falhou e ele desistiu): sobrou >= 1B no inventário -> Lugard de novo.
    let mut res = res;
    for round in 0..2 {
        if res.is_err() { break; } // Não reinicia a rota após falha de aproximação/interação.
        match inv_zen(hwnd) {
            Ok(z) if z >= FORTUNE_ZEN => {
                status(format!("ainda há {} no inventário: Lugard de novo ({})", zen_core::fmt::int_br(z), round + 1));
                match convert_all(hwnd, lor, status) {
                    Ok(n) => rep.converted += n,
                    Err(e) => res = Err(e),
                }
            }
            _ => break,
        }
    }
    // O K é esvaziado mesmo se a conversão parou no meio (prazo e limite de 50).
    let bank = gremory_to_bank(hwnd, status);
    if let Ok(n) = &bank {
        rep.to_bank = *n;
    }
    // Mesmo em falha, esvazia K/J acima; não volta a procurar NPCs depois de desistir.
    res?;
    bank?;
    let mut money = ensure_move_money(hwnd, lor, all, status);
    // Nunca volta ao spot carregando Zen do baú: o que passar de 20M (+ folga) volta para o baú.
    if let Ok(z) = money {
        // Repõe a aba 1 com o que sobrar acima da margem de 50M (regra do dono, 16/11 08:39).
        if z > SAFETY_ZEN + EXCESS_SLACK {
            status(format!("sobrou {} no inventário: guardando na aba 1 o que passa de 50M", zen_core::fmt::int_br(z)));
            money = lor_deposit_excess(hwnd, lor, z - SAFETY_ZEN, status);
        }
    }
    rep.zen_after = money?;
    Ok(rep)
}

/// Folga antes de devolver Zen ao baú (o char volta com 20M; abaixo disto não vale a viagem).
pub(in crate::win::nav) const EXCESS_SLACK: u64 = 30_000_000;

/// Deposita `amount` no baú de Lorencia: primeiro repõe a aba 1 até 1B, depois na primeira aba
/// (2..15) onde cabe; se não couber inteiro numa só, divide. Devolve o Zen final do inventário.
pub(in crate::win::nav) fn lor_deposit_excess(hwnd: isize, t: &Terrain, amount: u64, status: &dyn Fn(String)) -> Result<u64, String> {
    use zen_core::vault::ZEN_CAP;
    open_lor_vault(hwnd, t, status)?;
    let fmt = zen_core::fmt::int_br;
    let r = (|| -> Result<u64, String> {
        let mut z = read_zen_stable(hwnd, INV_ZEN).ok_or("não li o Zen do inventário")?;
        let mut left = amount.min(z.saturating_sub(MOVE_ZEN));
        // Só a aba 1 (dono, 16/11 08:13), até o limite do baú.
        storage_first_tab(hwnd)?;
        let s = read_zen_stable(hwnd, STORAGE_ZEN).ok_or("não li o Zen da aba 1")?;
        let put = left.min(ZEN_CAP.saturating_sub(s));
        if put > 10 * WITHDRAW_FEE {
            status(format!("baú: devolvendo {} na aba 1", fmt(put)));
            z = deposit_here(hwnd, put, s, z)?;
            left -= put;
        }
        if left > 10 * WITHDRAW_FEE {
            return Err(format!("baú cheio: não consegui devolver {}", fmt(left)));
        }
        Ok(z)
    })();
    if let Err(e) = &r {
        save_error_shot(hwnd, e);
    }
    close_storage(hwnd);
    r
}

/// Garante o Zen do move (20M). Se faltar, busca no baú (pode usar a reserva da aba 1).
/// `check_reserve`: abre o baú mesmo com Zen suficiente, para conferir/repor a aba 1.
pub fn ensure_move_money(hwnd: isize, lor: &Terrain, check_reserve: bool, status: &dyn Fn(String)) -> Result<u64, String> {
    let z = inv_zen(hwnd)?;
    // Volta com 30M (dono, 10/11 00:50): 20M do move + Zen para o helper ligar no spot.
    if z >= KEEP_ZEN - 1_000 && !check_reserve {
        return Ok(z);
    }
    if z < KEEP_ZEN - 1_000 {
        status(format!("só {} no char: buscando o Zen do move no baú", zen_core::fmt::int_br(z)));
    }
    let z = lor_withdraw(hwnd, lor, KEEP_ZEN, true, check_reserve, true, status)?;
    if z < MOVE_ZEN {
        return Err(format!("o baú não tem Zen para o move (char ficou com {})", zen_core::fmt::int_br(z)));
    }
    Ok(z)
}

// ---------------------------------------------------------------- Custom Pick Window (Y), 16/11
// A tecla Y abre a "PICK WINDOW" (servidor: "To activate the new pick menu, press Y"). Visto no
// x9901 em 16/11: janela à esquerda (x 85..385, y 140..620), 6 botões visíveis (Jewel of Soul,
// Bless, Chaos, Creation, Life, Zen) e barra de rolagem que só anda com a roda do mouse. Ao entrar
// em zona segura o pick desliga ("If move to SafeZone it will be necessary to reactivate!").
// - O botão NÃO muda de aparência ligado/desligado (prints idênticos) nem escreve no chat: o bot
//   só clica uma vez por volta ao spot depois de ter saído do mapa do spot (cidade/teleporte),
//   quando o pick com certeza está desligado.
// - ESC NÃO serve para fechar: abre o "Especial Menu" e fecha o Helper (16/11 12:15). Fecha com Y.
