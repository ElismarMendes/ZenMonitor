//! Imagem BGRA e o tratamento para OCR. Sem Windows: testável em qualquer máquina.

/// Imagem BGRA, linhas de cima para baixo.
#[derive(Clone)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

impl Image {
    pub fn crop(&self, r: [i32; 4]) -> Option<Image> {
        let [x, y, w, h] = r;
        if x < 0 || y < 0 || w <= 0 || h <= 0 || (x + w) as u32 > self.w || (y + h) as u32 > self.h {
            return None;
        }
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let s = ((row as u32 * self.w + x as u32) * 4) as usize;
            out.extend_from_slice(&self.bgra[s..s + (w as usize) * 4]);
        }
        Some(Image {
            w: w as u32,
            h: h as u32,
            bgra: out,
        })
    }

    /// Ampliação por vizinho mais próximo (bordas nítidas ajudam o OCR em fonte pequena).
    pub fn upscale(&self, n: u32) -> Image {
        let n = n.max(1);
        let (w2, h2) = (self.w * n, self.h * n);
        let mut out = vec![0u8; (w2 * h2 * 4) as usize];
        for y in 0..h2 {
            for x in 0..w2 {
                let s = (((y / n) * self.w + x / n) * 4) as usize;
                let d = ((y * w2 + x) * 4) as usize;
                out[d..d + 4].copy_from_slice(&self.bgra[s..s + 4]);
            }
        }
        Image {
            w: w2,
            h: h2,
            bgra: out,
        }
    }

    /// Redução por `n` (média de cada bloco n×n): gravação das invasões ocupa 1/n² do espaço.
    pub fn downscale(&self, n: u32) -> Image {
        let n = n.max(1);
        let (w2, h2) = (self.w / n, self.h / n);
        let mut out = vec![0u8; (w2 * h2 * 4) as usize];
        for y in 0..h2 {
            for x in 0..w2 {
                let mut acc = [0u32; 4];
                for dy in 0..n {
                    for dx in 0..n {
                        let s = ((((y * n + dy) * self.w) + x * n + dx) * 4) as usize;
                        for c in 0..4 {
                            acc[c] += self.bgra[s + c] as u32;
                        }
                    }
                }
                let d = ((y * w2 + x) * 4) as usize;
                for c in 0..4 {
                    out[d + c] = (acc[c] / (n * n)) as u8;
                }
            }
        }
        Image { w: w2, h: h2, bgra: out }
    }

    pub fn upscale2(&self) -> Image {
        self.upscale(2)
    }

    /// Prepara um recorte para o OCR: texto colorido claro (o Zen é VERMELHO) sobre fundo
    /// escuro vira texto preto sobre branco, com contraste esticado, e ganha margem branca.
    /// Usa o maior canal (max de R,G,B) para não perder texto vermelho, que some em cinza comum.
    pub fn for_ocr(&self) -> Image {
        let lum: Vec<u8> = self.bgra.chunks_exact(4).map(|p| p[0].max(p[1]).max(p[2])).collect();
        let (lo, hi) = lum.iter().fold((255u8, 0u8), |(a, b), &v| (a.min(v), b.max(v)));
        let span = (hi.saturating_sub(lo)).max(1) as u32;
        let mut out = Vec::with_capacity(self.bgra.len());
        for &v in &lum {
            let stretched = ((v.saturating_sub(lo)) as u32 * 255 / span) as u8;
            // Limiar simples: texto vira preto puro, fundo branco puro.
            let g = if stretched > 110 { 0 } else { 255 };
            out.extend_from_slice(&[g, g, g, 255]);
        }
        let img = Image {
            w: self.w,
            h: self.h,
            bgra: out,
        }
        .upscale(3);
        img.pad(12)
    }

    fn pad(&self, m: u32) -> Image {
        let (w2, h2) = (self.w + 2 * m, self.h + 2 * m);
        let mut out = vec![255u8; (w2 * h2 * 4) as usize];
        for y in 0..self.h {
            let s = (y * self.w * 4) as usize;
            let d = (((y + m) * w2 + m) * 4) as usize;
            out[d..d + (self.w * 4) as usize].copy_from_slice(&self.bgra[s..s + (self.w * 4) as usize]);
        }
        Image {
            w: w2,
            h: h2,
            bgra: out,
        }
    }

    pub fn to_png(&self) -> Option<Vec<u8>> {
        let mut rgba = self.bgra.clone();
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.w, self.h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().ok()?;
            wr.write_image_data(&rgba).ok()?;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, bgra: [u8; 4]) -> Image {
        Image {
            w,
            h,
            bgra: bgra.repeat((w * h) as usize),
        }
    }

    #[test]
    fn recorte_e_limites() {
        let img = solid(10, 10, [1, 2, 3, 255]);
        assert_eq!(img.crop([2, 2, 3, 4]).map(|i| (i.w, i.h)), Some((3, 4)));
        assert!(img.crop([8, 8, 3, 3]).is_none());
        assert!(img.crop([-1, 0, 3, 3]).is_none());
    }

    #[test]
    fn texto_vermelho_vira_preto_no_branco() {
        // fundo escuro com 1 pixel vermelho
        let mut img = solid(4, 1, [20, 15, 30, 255]);
        img.bgra[4..8].copy_from_slice(&[40, 40, 230, 255]); // BGRA: vermelho
        let p = img.for_ocr();
        assert_eq!((p.w, p.h), (4 * 3 + 24, 3 + 24));
        let px = |x: u32, y: u32| p.bgra[((y * p.w + x) * 4) as usize];
        assert_eq!(px(0, 0), 255); // margem branca
        assert_eq!(px(12 + 3, 12), 0); // o pixel vermelho (ampliado 3x) virou preto
        assert_eq!(px(12, 12), 255); // fundo virou branco
    }

    #[test]
    fn imagem_lisa_nao_quebra() {
        let p = solid(3, 3, [0, 0, 0, 255]).for_ocr();
        assert!(p.bgra.chunks_exact(4).all(|c| c[0] == 255));
    }
}
