import time
import platform


def obtaining_time_safe():
    return time.strftime("%H:%M:%S")


def log_init(python_path, env_tipo, is_admin=False):
    os_info = f"{platform.system()} {platform.release()} ({platform.version()})"
    admin_str = "SIM (Leitura RAM Ativa)" if is_admin else "NAO (Execute como Admin para ler RAM!)"
    print(f"[{obtaining_time_safe()}] [INIT] Versao do Python: {platform.python_version()} ({python_path})")
    print(f"[{obtaining_time_safe()}] [INIT] Sistema Operacional: {os_info}")
    print(f"[{obtaining_time_safe()}] [INIT] Ambiente Detectado: {env_tipo}")
    print(f"[{obtaining_time_safe()}] [INIT] Privilegio Administrador: {admin_str}")


def log_windows(total_janelas, clientes_dict):
    print(f"[{obtaining_time_safe()}] [WINDOWS] Janelas ativas detectadas: {total_janelas}")
    for pid, info in clientes_dict.items():
        print(f"[{obtaining_time_safe()}] [WINDOWS] PID: {pid} | Char: {info['name']} | Resolucao: {info['resolucao']}")


def log_calibration(resolucao, rel_box, pct_box, valor_amostra=None):
    pct_fmt = [round(v, 4) for v in pct_box] if pct_box else []
    print(f"[{obtaining_time_safe()}] [CALIBRATION] Resolucao: {resolucao} | Box: {rel_box} | Proporcao Universal: {pct_fmt}")
    if valor_amostra is not None:
        print(f"[{obtaining_time_safe()}] [CALIBRATION] Amostra validada no calibrador: Zen = {valor_amostra:,}".replace(",", "."))


def log_capture(pid, char_name, metodo, status, detalhes=""):
    msg = f"[{obtaining_time_safe()}] [CAPTURE] PID {pid} ({char_name}) | Metodo: {metodo} | Status: {status}"
    if detalhes:
        msg += f" | {detalhes}"
    print(msg)


def log_ocr(pid, char_name, status_leitura, valor_inteiro):
    val_fmt = f"{valor_inteiro:,}".replace(",", ".") if valor_inteiro is not None else "FALHA"
    print(f"[{obtaining_time_safe()}] [OCR] PID {pid} ({char_name}) | Status: {status_leitura} | Zen Lido: {val_fmt}")


def log_memory(pid, char_name, etapa, detalhes, valor_zen=None):
    zen_str = f" | Zen: {valor_zen:,}".replace(",", ".") if valor_zen is not None else ""
    print(f"[{obtaining_time_safe()}] [MEMORY] PID {pid} ({char_name}) | [{etapa}] {detalhes}{zen_str}")


def log_sync(machine_id, status_code, total_contas, resumo_str):
    if status_code == 200:
        print(f"[{obtaining_time_safe()}] [SYNC] POST OK (200) | Maquina: {machine_id} ({total_contas}) | {resumo_str}")
    else:
        print(f"[{obtaining_time_safe()}] [SYNC] ERRO HTTP {status_code} | Maquina: {machine_id}")


def log_error(modulo, erro_msg):
    print(f"[{obtaining_time_safe()}] [ERROR] [{modulo}] {erro_msg}")