//! Qual skill está selecionada na caixa da barra de baixo, pelo ícone. O ícone do Ice Storm vem
//! da folha `Data\Interface\newui_skill.OZJ` do próprio cliente (célula 38 = skill 39, 20x28,
//! cinza). Na tela ele aparece 1,6x maior (32x45) em ~(499,746) da janela 1030x797. Conferido em
//! 10/11/2026: x9901 (tecla 1) 0,90 e ReDMooN (tecla 2) 0,87 de correlação.

const ICE_STORM: &[u8; 560] = include_bytes!("ice_storm_20x28.gray");
pub const ICON_W: usize = 32;
pub const ICON_H: usize = 45;
/// Região a capturar (janela inteira): a caixa com folga para pequenos deslocamentos.
pub const REGION: [i32; 4] = [486, 738, 60, 59];
/// Correlação mínima para dizer "é o Ice Storm".
pub const MIN_SCORE: f32 = 0.78;

/// Ícone do Ice Storm no tamanho da tela (bilinear).
fn template() -> Vec<f32> {
    let mut t = vec![0f32; ICON_W * ICON_H];
    for y in 0..ICON_H {
        for x in 0..ICON_W {
            let fx = (x as f32 + 0.5) * 20.0 / ICON_W as f32 - 0.5;
            let fy = (y as f32 + 0.5) * 28.0 / ICON_H as f32 - 0.5;
            let (x0, y0) = (fx.floor().clamp(0.0, 19.0) as usize, fy.floor().clamp(0.0, 27.0) as usize);
            let (x1, y1) = ((x0 + 1).min(19), (y0 + 1).min(27));
            let (ax, ay) = ((fx - x0 as f32).clamp(0.0, 1.0), (fy - y0 as f32).clamp(0.0, 1.0));
            let p = |xx: usize, yy: usize| ICE_STORM[yy * 20 + xx] as f32;
            t[y * ICON_W + x] = p(x0, y0) * (1.0 - ax) * (1.0 - ay) + p(x1, y0) * ax * (1.0 - ay) + p(x0, y1) * (1.0 - ax) * ay + p(x1, y1) * ax * ay;
        }
    }
    t
}

/// Maior correlação do ícone do Ice Storm dentro de uma imagem cinza `w` x `h`.
pub fn ice_storm_score(gray: &[u8], w: usize, h: usize) -> f32 {
    if w < ICON_W || h < ICON_H || gray.len() < w * h {
        return 0.0;
    }
    let t = template();
    let n = (ICON_W * ICON_H) as f32;
    let tm = t.iter().sum::<f32>() / n;
    let tv: f32 = t.iter().map(|v| (v - tm) * (v - tm)).sum();
    let mut best = 0f32;
    for oy in 0..=(h - ICON_H) {
        for ox in 0..=(w - ICON_W) {
            let mut s = 0f32;
            for y in 0..ICON_H {
                for x in 0..ICON_W {
                    s += gray[(oy + y) * w + ox + x] as f32;
                }
            }
            let pm = s / n;
            let (mut cov, mut pv) = (0f32, 0f32);
            for y in 0..ICON_H {
                for x in 0..ICON_W {
                    let p = gray[(oy + y) * w + ox + x] as f32 - pm;
                    cov += p * (t[y * ICON_W + x] - tm);
                    pv += p * p;
                }
            }
            if pv > 1.0 && tv > 1.0 {
                best = best.max(cov / (pv * tv).sqrt());
            }
        }
    }
    best
}

/// BGRA -> cinza.
pub fn gray_of_bgra(bgra: &[u8], w: usize, h: usize) -> Vec<u8> {
    bgra.chunks_exact(4).take(w * h).map(|p| ((p[2] as u32 * 30 + p[1] as u32 * 59 + p[0] as u32 * 11) / 100) as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_o_proprio_icone_e_recusa_ruido() {
        // Ícone ampliado colado numa imagem maior: tem de dar ~1.
        let t = template();
        let (w, h) = (60, 59);
        let mut img = vec![30u8; w * h];
        for y in 0..ICON_H {
            for x in 0..ICON_W {
                img[(y + 8) * w + x + 13] = t[y * ICON_W + x] as u8;
            }
        }
        assert!(ice_storm_score(&img, w, h) > 0.95);
        // Gradiente qualquer: baixo.
        let g: Vec<u8> = (0..w * h).map(|i| ((i % w) * 4) as u8).collect();
        assert!(ice_storm_score(&g, w, h) < MIN_SCORE);
    }
}
