use super::*;

#[cfg(test)]
mod tab1_tests {
    use super::*;
    #[test]
    fn aba1_empresta_tudo_ou_nada_acima_do_gatilho() {
        let f = TAB1_REFILL_TRIGGER;
        assert_eq!(fortune_borrow(40_000_000, 731_000_000, f), 40_000_000);
        assert_eq!(fortune_borrow(40_000_000, 140_000_001, f), 40_000_000);
        assert_eq!(fortune_borrow(40_000_000, 139_000_000, f), 0); // ficaria abaixo de 100M
        assert_eq!(fortune_borrow(40_000_000, 100_000_000, f), 0); // no gatilho: não empresta
        assert_eq!(fortune_borrow(40_000_000, 0, f), 0);
    }
    #[test]
    fn ciclo_no_gatilho_repoe_a_aba1_e_guarda_50m() {
        // Char com 1,99B, aba 1 com 100M: 1 Fortuna do próprio Zen, nada emprestado, 940M para
        // a aba 1 (fica 1,04B) e 50M no char.
        let (z, tab1) = (1_990_000_000u64, 100_000_000u64);
        assert_eq!(fortune_borrow(FORTUNE_ZEN + KEEP_ZEN - (z - FORTUNE_ZEN) + WITHDRAW_FEE, tab1, TAB1_REFILL_TRIGGER), 0);
        let left = z - FORTUNE_ZEN;
        assert!(left > SAFETY_ZEN + EXCESS_SLACK);
        assert_eq!(tab1 + (left - SAFETY_ZEN), 1_040_000_000);
    }
}

#[cfg(test)]
mod npc_window_tests {
    use super::*;
    fn fixture(bytes: &[u8]) -> capture::Image {
        let mut decoder=png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::EXPAND|png::Transformations::STRIP_16);
        let mut reader=decoder.read_info().unwrap();
        let mut bytes=vec![0;reader.output_buffer_size()];
        let info=reader.next_frame(&mut bytes).unwrap();
        let channels=match info.color_type {png::ColorType::Rgba=>4,png::ColorType::Rgb=>3,_=>panic!("PNG de teste inesperado")};
        let bgra=bytes[..info.buffer_size()].chunks_exact(channels).flat_map(|p|[p[2],p[1],p[0],255]).collect();
        capture::Image{w:info.width,h:info.height,bgra}
    }
    #[test]
    fn trainer_reconhece_menus_reais_e_rejeita_area_sem_texto() {
        let ready=fixture(include_bytes!("../../../tests/fixtures/trainer_menu/ready.png"));
        let yousee=fixture(include_bytes!("../../../tests/fixtures/trainer_menu/yousee-ready.png"));
        let matches=|image:&capture::Image,(origin,txt):((i32,i32),&str)|{
            let template=Bits::from_text(txt);
            let crop=image.crop([origin.0-6,origin.1-6,template.w as i32+12,template.h as i32+12]).unwrap();
            trainer_pattern(&crop,&template)
        };
        assert!(matches(&ready,TRAINER_TITLE));
        assert!(matches(&ready,TRAINER_RESTORE));
        assert!(matches(&yousee,TRAINER_TITLE));
        assert!(matches(&yousee,TRAINER_RESTORE));
        let empty=capture::Image{w:116,h:23,bgra:vec![0;116*23*4]};
        assert!(!trainer_pattern(&empty,&Bits::from_text(RESTORE_TXT)));
    }
    #[test]
    fn csrf_reconhece_zyro_lateral_sem_confundir_com_inventario_do_x9901() {
        let zyro=fixture(include_bytes!("../../../tests/fixtures/npc_side/zyro.png"));
        let chaos=fixture(include_bytes!("../../../tests/fixtures/npc_side/chaos_inventory.png"));
        assert!(side_npc_caption(&zyro.crop([750,35,240,25]).unwrap()));
        assert!(!side_npc_caption(&chaos.crop([750,35,240,25]).unwrap()));
        let (x,y)=gray_template_at(&zyro.crop([735,640,120,80]).unwrap(),SIDE_CLOSE,40,40).unwrap();
        assert!((735+x+20-782).abs()<=2 && (640+y+20-676).abs()<=2);
        // Mesmo botão X existe no inventário: o título precisa fazer parte da decisão.
        assert!(gray_template_at(&chaos.crop([735,640,120,80]).unwrap(),SIDE_CLOSE,40,40).is_some());
        assert!(gray_template_at(&chaos.crop([205,41,134,26]).unwrap(),CHAOS_TITLE,124,16).is_some());
        assert!(gray_template_at(&zyro.crop([205,41,134,26]).unwrap(),CHAOS_TITLE,124,16).is_none());
    }
    #[test]
    fn limite_de_janelas_interrompe_antes_de_enviar_input() {
        let mut count=2;
        let result=recover_route_window(0,Dest::Lorencia,&mut count,&|_|panic!("não deve agir após limite"));
        assert!(result.unwrap_err().contains("limite"));
        assert_eq!(count,2);
    }
}

#[cfg(test)]
mod spot_frame_tests {
    use super::*;

    #[test]
    fn quadros_reais_preservam_mapa_e_coordenada_sem_nova_captura() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../zen-core/tests/fixtures/coords");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            let stem = p.file_stem().unwrap().to_str().unwrap();
            let mut xy = stem.split('_');
            let x: i32 = xy.next().unwrap().parse().unwrap();
            let y: i32 = xy.next().unwrap().parse().unwrap();
            let bits = Bits::from_text(&std::fs::read_to_string(&p).unwrap());
            let bgra = bits.px.iter().flat_map(|on| if *on { [255,255,255,255] } else { [0,0,0,255] }).collect();
            let frame = capture::Image { w: bits.w as u32, h: bits.h as u32, bgra };
            assert_eq!(decode_spot_frame(Some(frame)), (Some("Kanturu"), Some((x,y))), "{stem}");
            count += 1;
        }
        assert!(count >= 40);
        assert_eq!(decode_spot_frame(None), (None, None));
    }
}

#[cfg(test)]
mod npc_approach_tests {
    use super::*;
    #[test]
    fn redmoon_desvia_do_bloqueio_130_130_com_rota_validada_e_limite() {
        let t=Terrain::from_client_file(include_bytes!("../../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        let at=(130,130);
        let path=npc_route(&t,at,LUGARD_TILE).unwrap();
        let blocked=t.waypoints(at,&path,3)[0];
        let mut detours=NpcDetours::default();
        let first=detours.next(&t,at,blocked,LUGARD_TILE).unwrap();
        assert_ne!(first,blocked);
        assert!(t.line_clear(at,first));
        assert!(t.path(at,first).unwrap().len()<=3);
        assert!(npc_route(&t,first,LUGARD_TILE).is_some());
        assert!(!npc_route(&t,first,LUGARD_TILE).unwrap().contains(&at));
        let second=detours.next(&t,at,blocked,LUGARD_TILE).unwrap();
        assert_ne!(first,second);
        assert!(detours.next(&t,at,blocked,LUGARD_TILE).unwrap_err().contains("limite"));
    }
    #[test]
    fn desvio_nao_inventa_saida_sem_piso_ou_destino_valido() {
        let t=Terrain::from_client_file(include_bytes!("../../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        assert!(NpcDetours::default().next(&t,(-1,-1),(0,0),LUGARD_TILE).is_err());
        assert!(NpcDetours::default().next(&t,(130,130),(132,130),(-1,-1)).is_err());
    }
    #[test]
    fn raven_manual_aceita_vida_alta_e_leitura_transitoria_e_confirmada() {
        for hp in [0,25,50,51,100] { assert!(raven_request_allowed(true,hp)); }
        assert!(!raven_request_allowed(false,100));
        assert!(raven_request_allowed(false,25));
        let mut samples = [None,None,Some(80)].into_iter();
        let mut pauses=0;
        assert_eq!(confirm_raven_read(|| samples.next().flatten(), || pauses+=1),Some(80));
        assert_eq!(pauses,2);
        let mut reads=0;
        assert_eq!(confirm_raven_read(|| {reads+=1;None}, || {}),None);
        assert_eq!(reads,3);
    }
    #[test]
    fn trainer_rota_completa_do_bau_e_saida_nomove() {
        let t=Terrain::from_client_file(include_bytes!("../../../../zen-core/tests/fixtures/terrain/EncTerrain1.att")).unwrap();
        for start in [LOR_VAULT_TILE,(121,108)] {
            let route=walking_route(&t,start,TRAINER_STAND,WalkArrival::Trainer).unwrap();
            assert!(route.iter().all(|p|t.walkable(p.0,p.1)));
            assert_eq!(route.last(),Some(&TRAINER_STAND));
        }
        assert!(final_walk_arrived((121,108),TRAINER_STAND,false,0,WalkArrival::Trainer));
        assert!(!final_walk_arrived((119,113),TRAINER_STAND,false,30,WalkArrival::Trainer));
    }

    #[test]
    fn yousee_chega_ao_trainer_mesmo_no_tile_nomove_registrado_no_log() {
        let raw = include_bytes!("../../../../zen-core/tests/fixtures/terrain/EncTerrain1.att");
        let t = Terrain::from_client_file(raw).unwrap();
        let at = (121, 108);
        assert!(!t.walkable(at.0, at.1));
        assert!(t.walkable(TRAINER_STAND.0, TRAINER_STAND.1));
        assert!(t.path(at, TRAINER_STAND).is_none());
        let progress = NpcProgress { target: TRAINER_STAND, at, previous: None, stalled: 0 };
        assert!(progress.arrived());
        assert!(destination_matches((Some("Lorencia"), Some(at)), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        assert!(!destination_matches((Some("Dungeon"), Some(at)), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        assert!(!destination_matches((Some("Lorencia"), None), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        assert!(!destination_matches((Some("Lorencia"), Some((120,108))), Dest::Lorencia, Some((TRAINER_STAND, 1))));
        let exit = npc_route(&t, at, TRAINER_STAND).unwrap();
        assert_eq!(exit, vec![TRAINER_STAND]);
    }

    fn progress(at: (i32, i32)) -> NpcProgress {
        NpcProgress { target: (149,114), at, previous: None, stalled: 0 }
    }

    #[test]
    fn redmoon_sai_do_spawn_bloqueado_sem_abrir_caminho_em_paredes() {
        let raw = include_bytes!("../../../../zen-core/tests/fixtures/terrain/EncTerrain1.att");
        let t = Terrain::from_client_file(raw).unwrap();
        let from = (125,121);
        assert!(!t.walkable(from.0,from.1));
        assert!(t.path(from,LUGARD_TILE).is_none());
        let route = npc_route(&t,from,LUGARD_TILE).unwrap();
        assert_eq!(cheb(from,route[0]),1);
        assert_eq!((from.0-route[0].0).abs()+(from.1-route[0].1).abs(),1);
        assert!(route.iter().all(|p|t.walkable(p.0,p.1)));
        assert_eq!(route.last(),Some(&LUGARD_TILE));
        assert_eq!(t.waypoints(from,&route,3)[0],route[0]);
        assert!(npc_route(&t,(-1,121),LUGARD_TILE).is_none());
        assert!(npc_route(&t,(123,129),from).is_none());
    }

    #[test]
    fn segura_no_trajeto_e_solta_a_tres_tiles() {
        assert!(npc_hold_far((123,117), LOR_VAULT_TILE));
        assert!(npc_hold_far((142,112), LOR_VAULT_TILE));
        assert!(!npc_hold_far((143,112), LOR_VAULT_TILE));
        assert!(!npc_hold_far((146,112), LOR_VAULT_TILE));
        assert!(npc_hold_far((145,110), LUGARD_TILE));
        assert!(!npc_hold_far((146,111), LUGARD_TILE));
    }

    #[test]
    fn aceita_um_tile_sem_forcar_o_tile_exato() {
        assert!(progress((148,113)).arrived());
        let mut p = progress((146,112));
        p.advance((148,114)).unwrap();
        assert!(p.arrived());
    }

    #[test]
    fn interrompe_retorno_ao_tile_anterior() {
        let mut p = progress((145,112));
        p.advance((146,112)).unwrap();
        assert!(p.advance((145,112)).is_err());
    }

    #[test]
    fn interrompe_duas_tentativas_sem_movimento() {
        let mut p = progress((145,112));
        p.advance((145,112)).unwrap();
        assert!(p.advance((145,112)).is_err());
    }

    #[test]
    fn clique_curto_nao_e_ampliado_e_clique_longo_e_limitado() {
        let near = npc_step_px((146,112), (147,112));
        let distance = ((near.0-CENTER.0) as f64).hypot((near.1-CENTER.1) as f64);
        assert!(distance > 0.0 && distance < 90.0, "{distance}");
        let far = npc_step_px((100,100), (149,114));
        let distance = ((far.0-CENTER.0) as f64).hypot((far.1-CENTER.1) as f64);
        assert!(distance <= 181.0, "{distance}");
    }
}

#[cfg(test)]
mod destination_tests {
    use super::*;
    #[test]
    fn redmoon_chega_imediatamente_a_dois_tiles_com_mobs() {
        let target = (123, 233);
        assert!(final_walk_arrived((121,232), target, true, 0, WalkArrival::Spot));
        assert!(!final_walk_arrived((120,232), target, true, 0, WalkArrival::Spot));
        assert!(!final_walk_arrived((121,232), target, true, 30, WalkArrival::Exact));
    }
    #[test]
    fn sem_mobs_aceita_um_tile_so_apos_dez_segundos() {
        let target = (123,233);
        assert!(!final_walk_arrived((122,233), target, false, 9, WalkArrival::Spot));
        assert!(final_walk_arrived((122,233), target, false, 10, WalkArrival::Spot));
        assert!(!final_walk_arrived((121,233), target, false, 30, WalkArrival::Spot));
        assert!(final_walk_arrived(target, target, false, 0, WalkArrival::Spot));
        assert!(!final_walk_arrived((122,233), target, false, 30, WalkArrival::Exact));
    }
    #[test]
    fn margem_final_do_spot_aceita_dois_e_rejeita_tres_tiles() {
        let target = (123,233);
        assert!(destination_matches((Some("Kanturu"),Some((121,232))),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
        assert!(!destination_matches((Some("Lorencia"),Some(target)),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
        assert!(!destination_matches((Some("Kanturu"),None),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
        assert!(!destination_matches((Some("Kanturu"),Some((120,233))),Dest::Kanturu,Some((target,SPOT_ARRIVAL_TOLERANCE))));
    }
    #[test]
    fn mesma_coordenada_no_mapa_errado_nao_confirma() {
        assert!(!destination_matches((Some("Lorencia"), Some((91,66))), Dest::Kanturu, Some(((91,66),2))));
        assert!(!destination_matches((None, Some((91,66))), Dest::Kanturu, None));
    }
    #[test]
    fn nao_inventa_coordenada_ausente_e_rejeita_destino_distante() {
        assert!(!destination_matches((Some("Kanturu"), None), Dest::Kanturu, None));
        assert!(!destination_matches((Some("Kanturu"), Some((130,130))), Dest::Kanturu, Some(((91,66),2))));
        assert!(!destination_matches((Some("Kanturu"), Some((300,66))), Dest::Kanturu, None));
    }
    #[test]
    fn respeita_tolerancia_estavel_e_regiao_do_spawn() {
        assert!(destination_matches((Some("Kanturu"), Some((92,67))), Dest::Kanturu, Some(((91,66),2))));
        assert!(destination_matches((Some("Lorencia"), Some((123,117))), Dest::Lorencia, Some(((123,129),16))));
        assert!(!destination_matches((Some("Lorencia"), Some((123,100))), Dest::Lorencia, Some(((123,129),16))));
    }
}
