import time
import requests
import socket
from services.monitor_service import MonitorService

# CONFIGURAÇÕES
# Insira aqui o domínio ou IP da sua VPS onde o vps_main.py está rodando no Coolify
VPS_URL = "http://SUA_VPS_IP_OU_DOMINIO:8000/api/v1/report"

# Identificação única desta máquina (Pode ser o hostname do Windows ou um nome customizado)
MACHINE_ID = socket.gethostname()

monitor = MonitorService()

def coletar_e_enviar():
    print(f"[{MACHINE_ID}] Iniciando agente de envio para a VPS: {VPS_URL}")
    
    while True:
        try:
            # 1. Busca os clientes ativos na máquina local
            clientes = monitor.listar_contas_ativas()
            clientes_payload = []
            
            # 2. Coleta o Zen de cada conta ativa
            for pid, info in clientes.items():
                resultado_zen = monitor.obter_zen_conta(pid)
                clientes_payload.append({
                    "pid": pid,
                    "name": info["name"],
                    "resolucao": info["resolucao"],
                    "zen": resultado_zen.get("zen"),
                    "status": resultado_zen.get("status")
                })
            
            # 3. Monta o payload
            payload = {
                "machine_id": MACHINE_ID,
                "clientes": clientes_payload
            }
            
            # 4. Envia para a VPS via POST
            response = requests.post(VPS_URL, json=payload, timeout=3)
            if response.status_code == 200:
                print(f"[{MACHINE_ID}] Relatório enviado com sucesso. Contas reportadas: {len(clientes_payload)}")
            else:
                print(f"[{MACHINE_ID}] Erro ao enviar para VPS. Status: {response.status_code}")
                
        except requests.exceptions.ConnectionError:
            print(f"[{MACHINE_ID}] Falha de conexão com a VPS. Tentando novamente em instantes...")
        except Exception as e:
            print(f"[{MACHINE_ID}] Erro no agente: {e}")
            
        # Intervalo de envio (a cada 5 segundos)
        time.sleep(5)

if __name__ == "__main__":
    coletar_e_enviar()