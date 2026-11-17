use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows::Win32::Networking::WinSock::AF_INET;

const MIB_TCP_STATE_ESTAB: u32 = 5;

/// O processo tem conexão TCP ESTABELECIDA com endereço externo?
/// "Externo" = não é 127.x nem 0.0.0.0 (a menos que `allow_loopback` esteja ligado, só para teste).
pub fn has_established(pid: u32, allow_loopback: bool) -> bool {
    let mut size: u32 = 0;
    unsafe {
        // Primeira chamada só para saber o tamanho.
        let _ = GetExtendedTcpTable(None, &mut size, false, AF_INET.0 as u32, TCP_TABLE_OWNER_PID_ALL, 0);
        if size == 0 {
            return false;
        }
        // Folga: a tabela pode crescer entre as duas chamadas.
        let mut buf = vec![0u8; size as usize + 4096];
        size = buf.len() as u32;
        let rc = GetExtendedTcpTable(
            Some(buf.as_mut_ptr() as *mut _),
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
        if rc != 0 {
            return false;
        }
        let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
        let rows = std::slice::from_raw_parts(
            table.table.as_ptr() as *const MIB_TCPROW_OWNER_PID,
            table.dwNumEntries as usize,
        );
        rows.iter().any(|r| {
            if r.dwOwningPid != pid || r.dwState != MIB_TCP_STATE_ESTAB {
                return false;
            }
            // dwRemoteAddr está em ordem de rede: o primeiro octeto é o byte baixo.
            let first_octet = (r.dwRemoteAddr & 0xFF) as u8;
            let loopback = first_octet == 127 || r.dwRemoteAddr == 0;
            allow_loopback || !loopback
        })
    }
}
