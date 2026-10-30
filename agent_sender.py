import time
import socket
import sys
import os
import requests

# Garante que os imports internos (core / services) funcionem sem alterar seus arquivos antigos
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "local_agent"))

from services.monitor_service import MonitorService

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5

def iniciar_agente():
    monitor = MonitorService()
    print("==================================================")
    print(f" Agente Real Iniciado na Maquina: {MACHINE_ID}")
    print(f" Destino VPS: {VPS_URL}")
    print("==================================================")

    while True:
        try:
            clientes = monitor.listar_contas_ativas()
            clientes_payload = []

            for pid, info in clientes.items():
                resultado_zen = monitor.obter_zen_conta(pid)
                clientes_payload.append({
                    "pid": pid,
                    "name": info.get("name", "Desconhecido"),
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
                print(f"[{time.strftime('%H:%M:%S')}] OK! {len(clientes_payload)} conta(s) real(is) enviada(s).")
            else:
                print(f"[{time.strftime('%H:%M:%S')}] Erro HTTP {res.status_code}: {res.text}")
        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] Erro no loop: {e}")

        time.sleep(INTERVALO_SEGUNDOS)

if __name__ == "__main__":
    iniciar_agente()