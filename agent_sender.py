import os
import sys
import subprocess
import time
import socket
import ctypes

PASTA_PROJETO = os.path.dirname(os.path.abspath(__file__))
PYTHON_VENV = os.path.join(PASTA_PROJETO, ".venv", "Scripts", "python.exe")

if os.path.exists(PYTHON_VENV) and os.path.normcase(sys.executable) != os.path.normcase(PYTHON_VENV):
    print("[AUTO-FIX] Alternando automaticamente para o Python da .venv...")
    sys.exit(subprocess.call([PYTHON_VENV] + sys.argv))

try:
    import requests
    import pytesseract
except ImportError:
    print("[AUTO-FIX] Instalando dependencias ausentes...")
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-r", os.path.join(PASTA_PROJETO, "requirements.txt")])
    import requests
    import pytesseract

try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

tesseract_path = r"C:\Program Files\Tesseract-OCR\tesseract.exe"
if os.path.exists(tesseract_path):
    pytesseract.pytesseract.tesseract_cmd = tesseract_path

sys.path.insert(0, os.path.join(PASTA_PROJETO, "local_agent"))

from services.monitor_service import MonitorService
from core.calibrator import garantir_calibragem_inicial, calibrar_interativo, salvar_config_ocr, carregar_config_ocr
from core.windows import TIPO_AMBIENTE
from core.logger import log_init, log_windows, log_sync

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5


def verificar_admin():
    try:
        return ctypes.windll.shell32.IsUserAnAdmin() != 0
    except Exception:
        return False


def elevar_privilegio_se_necessario():
    if not verificar_admin() and "--no-admin" not in sys.argv:
        print("[AUTO-ELEVACAO] Solicitando permissao de Administrador para leitura direta de RAM...")
        parametros = " ".join([f'"{arg}"' for arg in sys.argv])
        res = ctypes.windll.shell32.ShellExecuteW(None, "runas", sys.executable, parametros, PASTA_PROJETO, 1)
        if int(res) > 32:
            sys.exit(0)
        else:
            print("[AVISO] Elevacao recusada. O agente rodara apenas via OCR (sem leitura de RAM).")


def iniciar_agente():
    elevar_privilegio_se_necessario()

    if "--calibrar" in sys.argv:
        calibrar_interativo()
        return

    if "--reset-ram" in sys.argv:
        cfg = carregar_config_ocr()
        for k, v in cfg.get("contas", {}).items():
            if isinstance(v, dict):
                v.pop("enderecos_ram", None)
                v.pop("pid_salvo", None)
        salvar_config_ocr(cfg)
        print("[OK] Enderecos de memoria RAM resetados com sucesso!")
        return

    is_admin = verificar_admin()
    monitor = MonitorService()

    print("==================================================")
    log_init(sys.executable, TIPO_AMBIENTE, is_admin=is_admin)
    print(f" Destino VPS: {VPS_URL}")
    print("==================================================")

    garantir_calibragem_inicial()

    while True:
        try:
            garantir_calibragem_inicial()

            clientes = monitor.listar_contas_ativas()
            log_windows(len(clientes), clientes)

            clientes_payload = monitor.processar_todas_contas_paralelo(clientes)

            payload = {
                "machine_id": MACHINE_ID,
                "clientes": clientes_payload,
            }

            res = requests.post(VPS_URL, json=payload, timeout=5)
            resumo = [f"{c['name']}: {c['zen']:,} [{c['status']}]".replace(",", ".") for c in clientes_payload]
            resumo_str = " | ".join(resumo) if resumo else "Nenhuma conta"

            log_sync(MACHINE_ID, res.status_code, len(clientes_payload), resumo_str)

        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] [ERROR] Erro no loop principal: {e}")

        time.sleep(INTERVALO_SEGUNDOS)


if __name__ == "__main__":
    try:
        iniciar_agente()
    except KeyboardInterrupt:
        print("\nAgente encerrado pelo usuario.")