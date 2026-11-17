//! Imagens do cliente do jogo (.OZT = TGA com 4 bytes na frente) para o painel: o desenho do
//! mapa (`Data\WorldN\mini_map.OZT`) e os ícones do minimapa (`Data\Interface`). Só leitura.

/// Imagem RGBA de cima para baixo.
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// Lê um .OZT (TGA sem compressão ou RLE, 24 ou 32 bits).
pub fn decode(raw: &[u8]) -> Result<Rgba, String> {
    let m = raw.get(4..).ok_or("arquivo curto")?;
    if m.len() < 18 {
        return Err("cabeçalho curto".into());
    }
    let (idl, typ) = (m[0] as usize, m[2]);
    let (w, h) = (u16::from_le_bytes([m[12], m[13]]) as usize, u16::from_le_bytes([m[14], m[15]]) as usize);
    let (bpp, desc) = (m[16], m[17]);
    if !(bpp == 24 || bpp == 32) || w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(format!("formato não suportado: {w}x{h} {bpp} bits tipo {typ}"));
    }
    let bp = bpp as usize / 8;
    let data = m.get(18 + idl..).ok_or("sem dados")?;
    let need = w * h * bp;
    let mut src = Vec::with_capacity(need);
    match typ {
        2 => src.extend_from_slice(data.get(..need).ok_or("dados curtos")?),
        10 => {
            let mut i = 0;
            while src.len() < need {
                let c = *data.get(i).ok_or("RLE curto")?;
                i += 1;
                let n = (c & 0x7f) as usize + 1;
                if c & 0x80 != 0 {
                    let p = data.get(i..i + bp).ok_or("RLE curto")?;
                    for _ in 0..n {
                        src.extend_from_slice(p);
                    }
                    i += bp;
                } else {
                    src.extend_from_slice(data.get(i..i + n * bp).ok_or("RLE curto")?);
                    i += n * bp;
                }
            }
            src.truncate(need);
        }
        t => return Err(format!("TGA tipo {t} não suportado")),
    }
    let mut px = vec![0u8; w * h * 4];
    let top_down = desc & 0x20 != 0;
    for y in 0..h {
        let sy = if top_down { y } else { h - 1 - y };
        for x in 0..w {
            let s = (sy * w + x) * bp;
            let d = (y * w + x) * 4;
            px[d] = src[s + 2];
            px[d + 1] = src[s + 1];
            px[d + 2] = src[s];
            px[d + 3] = if bp == 4 { src[s + 3] } else { 255 };
        }
    }
    Ok(Rgba { w, h, px })
}

pub fn png(img: &Rgba) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, img.w as u32, img.h as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&img.px).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Desenho do mapa na orientação do painel (x para a direita, y para baixo), `size` x `size`.
/// No arquivo do jogo o mapa vem transposto: a linha da imagem é o x do tile e a coluna é o
/// y (conferido no Kanturu em 10/11: 84% de acerto contra o terreno, 64% na melhor alternativa).
pub fn minimap_for_panel(img: &Rgba, size: usize) -> Rgba {
    let mut px = vec![0u8; size * size * 4];
    for oy in 0..size {
        for ox in 0..size {
            let sr = (ox * img.h / size).min(img.h - 1); // linha = x do tile
            let sc = (oy * img.w / size).min(img.w - 1); // coluna = y do tile
            let s = (sr * img.w + sc) * 4;
            let d = (oy * size + ox) * 4;
            px[d..d + 4].copy_from_slice(&img.px[s..s + 4]);
            px[d + 3] = 255;
        }
    }
    Rgba { w: size, h: size, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tga(w: u16, h: u16, desc: u8, body: &[u8]) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.push(32);
        v.push(desc);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn le_bgra_de_baixo_para_cima() {
        // 1x2: linha de baixo azul, linha de cima vermelha (BGRA)
        let raw = tga(1, 2, 0, &[255, 0, 0, 255, 0, 0, 255, 255]);
        let img = decode(&raw).unwrap();
        assert_eq!(&img.px[0..4], &[255, 0, 0, 255]); // topo = vermelho
        assert_eq!(&img.px[4..8], &[0, 0, 255, 255]);
        assert!(!png(&img).unwrap().is_empty());
    }

    #[test]
    fn minimapa_transposto() {
        // 2x2 de cima para baixo: (linha 0, col 1) = vermelho -> no painel vai para (x=0, y=1)
        let mut body = vec![0u8; 16];
        body[4..8].copy_from_slice(&[0, 0, 255, 255]);
        let img = decode(&tga(2, 2, 0x20, &body)).unwrap();
        let p = minimap_for_panel(&img, 2);
        assert_eq!(&p.px[8..12], &[255, 0, 0, 255]); // (x=0, y=1)
    }
}
