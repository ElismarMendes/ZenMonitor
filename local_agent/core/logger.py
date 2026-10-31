import time
import platform

def log_init(python_path, env_tipo):
    os_info = f"{platform.system()} {platform.release()} ({platform.version()})"
    print(f"[{obtaining_time_safe()}] [INIT] Versão do Python: {platform.python_version()} ({python_path})")
    print(f"[{obtaining_time_safe()}] [INIT] Sistema Operacional: {os_info}")
    print(f"[{obtaining_time_safe()}] [INIT] Ambiente Detectado: ==> {env_tipo} <==")

def log_windows(total_janelas, clientes_dict):
    print(f"[{obtaining_time_safe()}] [WINDOWS] Janelas ativas detectadas: {total_janelas}")
    for pid, info in clientes_dict.items():
        print(f"[{obtaining_time_safe()}] [WINDOWS] -> PID: {pid} | Char: {info['name']} | Título: '{info['titulo']}' | Resolução: {info['resolucao']}")

def log_calibration(resolucao, rel_box, valor_amostra=None):
    print(f"[{obtaining_time_safe()}] [CALIBRATION] Resolução alvo: {resolucao}")
    print(f"[{obtaining_time_safe()}] [CALIBRATION] Coordenadas ROI mapeadas: {rel_box}")
    if valor_amostra is not None:
        print(f"[{obtaining_time_safe()}] [CALIBRATION] Amostra validada no calibrador: Zen = {valor_amostra:,}".replace(",", "."))

def log_capture(pid, char_name, metodo, status, detalhes=""):
    msg = f"[{obtaining_time_safe()}] [CAPTURE] PID {pid} ({char_name}) -> Método: {metodo} | Status: {status}"
    if detalhes:
        msg += f" | {detalhes}"
    print(msg)

def log_ocr(pid, char_name, texto_bruto, texto_limpo, valor_inteiro):
    val_fmt = f"{valor_inteiro:,}".replace(",", ".") if valor_inteiro is not None else "FALHA"
    print(f"[{obtaining_time_safe()}] [OCR] PID {pid} ({char_name}) -> Bruto: '{texto_bruto}' | Limpo: '{texto_limpo}' | Inteiro: {val_fmt}")

def log_sync(machine_id, status_code, total_contas, resumo_str):
    if status_code == 200:
        print(f"[{obtaining_time_safe()}] [SYNC] POST OK (200) para VPS | Máquina: {machine_id} | Contas: {total_contas} | Resumo: {resumo_str}")
    else:
        print(f"[{obtaining_time_safe()}] [SYNC] ERRO HTTP {status_code} | Máquina: {machine_id}")

def log_error(modulo, erro_msg):
    print(f"[{obtaining_time_safe()}] [ERROR] [{modulo}] {erro_msg}")

def obtaining_time_safe():
    return time.strftime("%H:%M:%S")