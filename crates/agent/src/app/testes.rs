use super::*;

#[cfg(test)]
mod spot_tests {
    #[test]
    fn raven_bloqueio_longo_so_depois_da_restauracao_confirmada() {
        assert_eq!(super::raven_cooldown(&Ok("Raven restaurado: 0% -> 100%".into())),Some(1800));
        assert_eq!(super::raven_cooldown(&Err("Raven N?O restaurado: sem rota".into())),Some(60));
        assert_eq!(super::raven_cooldown(&Err("n?o iniciou".into())),None);
    }
    use super::*;

    #[test]
    fn f2_cataloga_outro_mapa_sem_trocar_principal_ou_retorno() {
        let mut cfg=AgentConfig::default();
        cfg.spots.insert("Teste".into(),[91,66]);
        let before=(cfg.spots.clone(),cfg.arena.clone(),cfg.spot_maps.clone());
        assert_eq!(catalog_here(&mut cfg,"Dungeon",[20,30]).unwrap(),(1,true));
        assert_eq!(catalog_here(&mut cfg,"Dungeon",[20,30]).unwrap(),(1,false));
        assert_eq!(before,(cfg.spots.clone(),cfg.arena.clone(),cfg.spot_maps.clone()));
        assert_eq!(cfg.catalog_maps["Dungeon"],vec![[20,30]]);
    }

    #[test]
    fn personagem_em_espera_manual_nao_bloqueia_fortuna_do_outro() {
        let mut cfg = AgentConfig::default();
        cfg.recovery_enabled = true;
        let mut rec = Recovery::new();
        assert!(recovery_can_take_priority(&cfg,&rec,"x9901"));
        rec.hold.insert("x9901".into(),"foi até pelo painel".into());
        assert!(!recovery_can_take_priority(&cfg,&rec,"x9901"));
        rec.hold.clear();
        cfg.no_automation.push("x9901".into());
        assert!(!recovery_can_take_priority(&cfg,&rec,"x9901"));
        cfg.no_automation.clear();
        cfg.recovery_enabled = false;
        assert!(!recovery_can_take_priority(&cfg,&rec,"x9901"));
    }

    #[test]
    fn salvar_preserva_tile_exato_perto_de_spot_existente() {
        let mut cfg = AgentConfig::default();
        cfg.catalog = vec![[120, 20]];
        let (id, new, saved) = catalog_add(&mut cfg, [121, 22]);
        assert_eq!((id, new, saved), (1, false, [121, 22]));
        assert_eq!(cfg.catalog, vec![[121, 22]]);
    }

    #[test]
    fn captura_ausente_ou_invalida_falha_sem_reler_ou_alterar_spots() {
        let mut cfg = AgentConfig::default();
        cfg.spots.insert("Teste".into(), [120, 20]);
        for snap in [None, Some((None, None)), Some((None, Some((121, 22))))] {
            let replies = save_spot_snapshot(17, "Teste".into(), snap, &mut cfg);
            assert!(matches!(&replies[0], AgentMsg::CommandResult { id: 17, ok: false, .. }));
            assert_eq!(cfg.spots["Teste"], [120, 20]);
        }
    }

    #[test]
    fn recusas_e_erros_nao_aparecem_como_sucesso() {
        for message in ["atalho ignorado", "falhou ao gravar", "falha ao salvar", "char travado", "esse spot já é do Outro", "Lorencia é cidade: sem spots"] {
            assert!(!spot_result_ok(message), "{message}");
        }
        assert!(spot_result_ok("F2 (Teste): spot 1 salvo em (121, 22)"));
    }
}

#[allow(dead_code)]
fn _assert_access_is_object_safe(_: &dyn GameAccess) {}

#[cfg(test)]
mod guarda_tests {
    use super::*;
    #[test]
    fn panico_na_rotina_libera_o_pc_e_avisa() {
        let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::channel();
        let (b, t) = (busy.clone(), tx.clone());
        let h = std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: b, tx: t, name: "Teste".into() };
            panic!("erro de propósito");
        });
        assert!(h.join().is_err());
        assert!(!busy.load(std::sync::atomic::Ordering::SeqCst), "o PC tem de ficar livre");
        let (name, r) = rx.try_recv().expect("tem de avisar o fim da rotina");
        assert_eq!(name, "Teste");
        assert!(r.unwrap_err().contains("erro interno"));
    }
    #[test]
    fn caminho_normal_nao_muda() {
        let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: "Teste".into() };
        }
        assert!(busy.load(std::sync::atomic::Ordering::SeqCst), "sem pânico a guarda não mexe em nada");
        assert!(rx.try_recv().is_err());
    }
}
