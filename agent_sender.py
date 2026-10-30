import os
import sys
import subprocess
import time
import socket
import ctypes

# 1. AUTO-CORRECAO DE AMBIENTE (.venv) E DEPENDENCIAS
PASTA_PROJETO = os.path.dirname(os.path.abspath(__file__))
PYTHON_VENV = os.path.join(PASTA_PROJETO, ".venv", "Scripts", "python.exe")

# Se a .venv existe mas o script foi aberto pelo Python global do Windows, relanca dentro da .venv!
if os.path.exists(PYTHON_VENV) and os.path.normcase(sys.executable) != os.path.normcase(PYTHON_VENV):
    print("[AUTO-FIX] Alternando automaticamente para o Python da .venv...")
    sys.exit(subprocess.call([PYTHON_VENV] + sys.argv))

# Se faltar 'requests' ou 'pytesseract', instala automaticamente no Python atual
try:
    import requests
    import pytesseract
except ImportError:
    print("[AUTO-FIX] Instalando dependencias ausentes (requests, pytesseract, opencv, pywin32)...")
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-r", os.path.join(PASTA_PROJETO, "requirements.txt")])
    import requests
    import pytesseract

# 2. Ativa DPI real do Windows para nao deslocar recortes de OCR
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

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5


def iniciar_agente():
    if "--calibrar" in sys.argv:
        calibrar_interativo()
        return

    monitor = MonitorService()
    print("==================================================")
    print(f" Agente Iniciado na Maquina: {MACHINE_ID}")
    print(f" Python em uso: {sys.executable}")
    print(f" Destino VPS: {VPS_URL}")
    print("==================================================")

    garantir_calibragem_inicial()

    while True:
        try:
            garantir_calibragem_inicial()

            clientes = monitor.listar_contas_ativas()
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
            if res.status_code == 200:
                resumo = [f"{c['name']}: {c['zen']:,}".replace(",", ".") for c in clientes_payload]
                print(f"[{time.strftime('%H:%M:%S')}] OK ({len(clientes_payload)}) -> {' | '.join(resumo)}")
            else:
                print(f"[{time.strftime('%H:%M:%S')}] Erro HTTP {res.status_code}: {res.text}")
        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] Erro no loop: {e}")

        time.sleep(INTERVALO_SEGUNDOS)


if __name__ == "__main__":
    try:
        iniciar_agente()
    except KeyboardInterrupt:
        print("\nAgente encerrado pelo usuario.")