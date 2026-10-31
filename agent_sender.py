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
    print("[AUTO-FIX] Instalando dependências ausentes...")
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
from core.calibrator import garantir_calibragem_inicial, calibrar_interativo
from core.windows import TIPO_AMBIENTE
from core.logger import log_init, log_windows, log_sync

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5

def iniciar_agente():
    if "--calibrar" in sys.argv:
        calibrar_interativo()
        return

    monitor = MonitorService()
    print("==================================================")
    log_init(sys.executable, TIPO_AMBIENTE)
    print(f" Destino VPS: {VPS_URL}")
    print("==================================================")

    garantir_calibragem_inicial()

    while True:
        try:
            garantir_calibragem_inicial()

            clientes = monitor.listar_contas_ativas()
            log_windows(len(clientes), clientes)
            clientes_payload = []

            for pid, info in clientes.items():
                resultado_zen = monitor.obter_zen_conta(pid, info)
                clientes_payload.append({
                    "pid": int(pid),
                    "name": str(info.get("name", f"PID:{pid}")),
                    "resolucao": info.get("resolucao", "N/A"),
                    "zen": resultado_zen.get("zen", 0),
                    "status": resultado_zen.get("status", "OK")
                })

            payload = {
                "machine_id": MACHINE_ID,
                "clientes": clientes_payload
            }

            res = requests.post(VPS_URL, json=payload, timeout=5)
            resumo = [f"{c['name']}: {c['zen']:,}".replace(",", ".") for c in clientes_payload]
            resumo_str = " | ".join(resumo) if resumo else "Nenhuma conta"
            
            log_sync(MACHINE_ID, res.status_code, len(clientes_payload), resumo_str)

        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] [ERROR] Erro no loop principal: {e}")

        time.sleep(INTERVALO_SEGUNDOS)

if __name__ == "__main__":
    try:
        iniciar_agente()
    except KeyboardInterrupt:
        print("\nAgente encerrado pelo usuário.")