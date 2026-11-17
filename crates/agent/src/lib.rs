//! Biblioteca do agente. O binário (`main.rs`) e as ferramentas de diagnóstico (`diag`)
//! usam as mesmas funções de Windows daqui.

pub mod spot_service;
pub mod navigation;
pub mod config;
pub mod image;
pub mod monitor;
pub mod net;
pub mod update;
#[cfg(windows)]
pub mod win;

/// Tudo que toca o Windows passa por esta interface. Os testes usam uma versão falsa.
pub trait GameAccess: Send {
    /// Janelas do jogo abertas: (pid, hwnd como inteiro, título).
    fn list_windows(&self) -> Vec<GameWindow>;
    /// O processo tem conexão TCP ESTABELECIDA com endereço externo?
    fn has_external_tcp(&self, pid: u32) -> bool;
    /// Lê 4 bytes. `None` se falhar.
    fn read_u32(&self, pid: u32, addr: u64) -> Option<u32>;
    /// Varredura completa por um valor (só na trava, uma vez).
    fn scan_u32(&self, pid: u32, value: u32) -> Vec<u64>;
    /// Região do Zen configurada. Sem isso o agente não trava (e não varre).
    fn zen_field_ready(&self) -> bool;
    /// Zen lido na tela pelo campo do inventário (leitura por modelo de pixels, sem OCR).
    /// `None` se o inventário estiver fechado ou qualquer pixel não bater.
    fn read_zen_field(&self, hwnd: isize) -> Option<u32>;
    /// Print da janela em PNG.
    fn screenshot_png(&self, hwnd: isize) -> Option<Vec<u8>>;

    /// Região do número de HP configurada (vazia = não lê vida nem detecta morte).
    fn hp_field_ready(&self) -> bool;
    /// HP lido na tela (número em cima da esfera vermelha). `None` se não bater com os modelos.
    fn read_hp_field(&self, hwnd: isize) -> Option<u32>;
    /// Lê 2 bytes de vários endereços de uma vez (em blocos; a ordem da saída = da entrada).
    fn read_u16_many(&self, pid: u32, addrs: &[u64]) -> Vec<Option<u16>>;
    /// Varredura completa por um u16 (passo 2). Para no limite de candidatos.
    fn scan_u16(&self, pid: u32, value: u16) -> Vec<u64>;
    /// Print da janela em PNG + o HP que aparece NESSE print (mesma captura).
    fn death_shot(&self, hwnd: isize) -> Option<(Vec<u8>, Option<u32>)>;
    /// Hora de início do processo (identifica o MESMO processo mesmo se o pid for reaproveitado).
    fn process_start(&self, _pid: u32) -> Option<u64> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct GameWindow {
    pub pid: u32,
    pub hwnd: isize,
    pub title: String,
}
