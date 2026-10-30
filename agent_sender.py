import time
import socket
import requests
from local_agent.services.monitor_service import MonitorService

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5

def iniciar_agente():
    monitor = MonitorService()
    print(f"==================================================")
    print(f" Agente Iniciado na Maquina: {MACHINE_ID}")
    print(f" Destino VPS: {VPS_URL}")
    print(f"==================================================")

    while True:
        try:
            contas = monitor.coletar_contas()
            payload = {
                "machine_id": MACHINE_ID,
                "clientes": contas
            }
            res = requests.post(VPS_URL, json=payload, timeout=5)
            if res.status_code == 200:
                print(f"[{time.strftime('%H:%M:%S')}] OK! {len(contas)} conta(s) enviada(s) para a VPS.")
            else:
                print(f"[{time.strftime('%H:%M:%S')}] Erro HTTP {res.status_code}: {res.text}")
        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] Falha de conexao com a VPS: {e}")

        time.sleep(INTERVALO_SEGUNDOS)

if __name__ == "__main__":
    iniciar_agente()
