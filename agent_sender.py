import time
import socket
import sys
import os
import requests
import pytesseract

tesseract_path = r"C:\Program Files\Tesseract-OCR\tesseract.exe"
if os.path.exists(tesseract_path):
    pytesseract.pytesseract.tesseract_cmd = tesseract_path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "local_agent"))

from services.monitor_service import MonitorService
from core.calibrator import garantir_calibragem_inicial, calibrar_interativo

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5


def iniciar_agente():
    # Se o usuario chamou "python agent_sender.py --calibrar", abre direto o calibrador
    if "--calibrar" in sys.argv:
        calibrar_interativo()
        return

    monitor = MonitorService()
    print("==================================================")
    print(f" Agente Iniciado na Maquina: {MACHINE_ID}")
    print(f" Destino VPS: {VPS_URL}")
    print("==================================================")

    # Verifica automaticamente na primeira execucao se o PC ja tem calibragem para a resolucao do jogo
    garantir_calibragem_inicial()

    while True:
        try:
            # Caso o jogo tenha sido aberto DEPOIS do agente iniciar e ainda nao tenha calibragem
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