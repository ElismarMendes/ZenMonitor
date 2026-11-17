//! Área de mundo restante quando os painéis dockados do cliente estão abertos.
//! Só altera o estacionamento do cursor. A projeção/caminhada legada permanece intacta.
use super::capture::Image;
use std::{collections::HashMap, sync::{Mutex, OnceLock, atomic::{AtomicU32, Ordering}}, time::{Duration, Instant}};

const CORNER: &[u8;768] = include_bytes!("panel_corner.gray");
const CHARACTER_CORNER: &[u8;768] = include_bytes!("panel_character_corner.gray");
const PANEL_WIDTH: i32 = 304;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout { pub left: i32, pub right: i32 }
impl Layout {
    pub fn home(self) -> (i32,i32) { ((self.left+self.right)/2,400) }
    pub fn click_changes_layout(self, x: i32, y: i32) -> bool {
        // Painéis, botões de rodapé/HUD. Cliques de caminhada dentro do mundo não invalidam.
        x < self.left || x >= self.right || y >= 640
    }
}

fn panel_at(img: &Image, x: i32) -> bool {
    [CORNER,CHARACTER_CORNER].iter().any(|template| {
        let mut diff=0u32;
        for y in 0..64 { for xx in 0..12 {
            let i=(((28+y)*img.w as usize)+(x as usize+xx))*4;
            let p=&img.bgra[i..i+3];
            let gray=(p[2] as u32*299+p[1] as u32*587+p[0] as u32*114)/1000;
            diff+=gray.abs_diff(template[y*12+xx] as u32);
        }}
        diff<=8*768
    })
}

pub fn detect(img: &Image) -> Option<Layout> {
    if img.w<1027 || img.h<716 || img.bgra.len()!=(img.w as usize*img.h as usize*4) {return None;}
    let mut right=1027;
    // Só conta painéis contíguos à borda: não confunde caixas flutuantes com viewport.
    for x in [723,419,115] {
        if !panel_at(img,x) {break;}
        right=x;
    }
    let mut left=3;
    for x in [3,307,611] {
        if x+PANEL_WIDTH>right || !panel_at(img,x) {break;}
        left=x+PANEL_WIDTH;
    }
    (right>left).then_some(Layout{left,right})
}

struct Sample { pid:u32, epoch:u32, at:Instant, layout:Layout }
static CACHE:OnceLock<Mutex<HashMap<isize,Sample>>>=OnceLock::new();
static EPOCH:AtomicU32=AtomicU32::new(0);
pub fn epoch()->u32 {EPOCH.load(Ordering::SeqCst)}
pub fn invalidate() {EPOCH.fetch_add(1,Ordering::SeqCst);}

/// Reutiliza o quadro completo já obtido por PrintWindow, antes de recortar para OCR.
pub fn observe(hwnd:isize,pid:u32,started:u32,img:&Image) {
    if started!=epoch() {return;}
    let Some(layout)=detect(img) else{return};
    if let Ok(mut cache)=CACHE.get_or_init(Default::default).lock() {
        cache.retain(|_,s|s.at.elapsed()<Duration::from_secs(30));
        if started==epoch() {cache.insert(hwnd,Sample{pid,epoch:started,at:Instant::now(),layout});}
    }
}

pub fn cached(hwnd:isize,pid:u32)->Option<Layout> {
    let cache=CACHE.get_or_init(Default::default).lock().ok()?;
    let sample=cache.get(&hwnd)?;
    (sample.pid==pid && sample.epoch==epoch() && sample.at.elapsed()<Duration::from_secs(3)).then_some(sample.layout)
}

pub fn current(hwnd:isize)->Option<Layout> {
    let pid=super::capture::window_pid(hwnd)?;
    if let Some(layout)=cached(hwnd,pid) {return Some(layout);}
    // Quadro frio/invalidação por abertura de painel: uma leitura, nunca uma por clique de caminhada.
    super::capture::capture(hwnd,Some([0,28,1027,64]))?;
    cached(hwnd,pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(bytes:&[u8])->Image {
        let mut decoder=png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::EXPAND|png::Transformations::STRIP_16);
        let mut reader=decoder.read_info().unwrap();
        let mut buf=vec![0;reader.output_buffer_size()];
        let info=reader.next_frame(&mut buf).unwrap();
        let channels=match info.color_type {png::ColorType::Rgba=>4,png::ColorType::Rgb=>3,_=>panic!("PNG inesperado")};
        Image{w:info.width,h:info.height,bgra:buf[..info.buffer_size()].chunks_exact(channels).flat_map(|p|[p[2],p[1],p[0],255]).collect()}
    }
    #[test]
    fn x9901_sem_paineis_fica_no_centro_legado() {
        let img=image(include_bytes!("../../tests/fixtures/cursor_layout/0-closed.png"));
        assert_eq!(detect(&img),Some(Layout{left:3,right:1027}));
        assert_eq!(detect(&img).unwrap().home(),(515,400));
    }
    #[test]
    fn inventario_e_atributos_usam_o_centro_do_mundo_restante() {
        for bytes in [include_bytes!("../../tests/fixtures/cursor_layout/1-inventory.png").as_slice(),include_bytes!("../../tests/fixtures/cursor_layout/3-character.png").as_slice()] {
            assert_eq!(detect(&image(bytes)).unwrap().home(),(363,400));
        }
    }
    #[test]
    fn expandido_e_atributos_com_inventario_deslocam_dois_paineis() {
        for bytes in [include_bytes!("../../tests/fixtures/cursor_layout/2-expanded.png").as_slice(),include_bytes!("../../tests/fixtures/cursor_layout/4-character-inventory.png").as_slice()] {
            assert_eq!(detect(&image(bytes)).unwrap().home(),(211,400));
        }
    }
    #[test]
    fn reconhece_zyro_e_tres_paineis_do_chaos() {
        assert_eq!(detect(&image(include_bytes!("../../tests/fixtures/npc_side/zyro.png"))).unwrap().home(),(363,400));
        assert_eq!(detect(&image(include_bytes!("../../tests/fixtures/npc_side/chaos_inventory.png"))).unwrap().home(),(59,400));
    }
    #[test]
    fn cliques_do_mundo_preservam_cache_e_footer_invalida() {
        let layout=Layout{left:3,right:723};
        assert!(!layout.click_changes_layout(500,400));
        assert!(layout.click_changes_layout(944,674));
        assert!(layout.click_changes_layout(780,300));
        assert!(detect(&Image{w:20,h:20,bgra:vec![0;1600]}).is_none());
    }
    #[test]
    fn invalida_quadro_anterior_a_tecla_e_nao_reutiliza_pid_diferente() {
        let closed=image(include_bytes!("../../tests/fixtures/cursor_layout/0-closed.png"));
        let inventory=image(include_bytes!("../../tests/fixtures/cursor_layout/1-inventory.png"));
        let before=epoch();
        observe(-100,10,before,&closed);
        assert_eq!(cached(-100,10).unwrap().home(),(515,400));
        invalidate();
        observe(-100,10,before,&closed); // Captura anterior à abertura terminou atrasada.
        assert!(cached(-100,10).is_none());
        observe(-100,20,epoch(),&inventory);
        assert!(cached(-100,10).is_none());
        assert_eq!(cached(-100,20).unwrap().home(),(363,400));
    }
}
