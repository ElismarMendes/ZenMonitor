import time
import socket
import sys
import os
import requests
import pytesseract

# Configura o caminho padrao do Tesseract no Windows
tesseract_path = r"C:\Program Files\Tesseract-OCR\tesseract.exe"
if os.path.exists(tesseract_path):
    pytesseract.pytesseract.tesseract_cmd = tesseract_path

# Garante que os modulos locais sejam encontrados
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "local_agent"))

try:
    from services.monitor_service import MonitorService
except ImportError:
    from local_agent.services.monitor_service import MonitorService

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"
MACHINE_ID = socket.gethostname()
INTERVALO_SEGUNDOS = 5

def iniciar_agente():
    try:
        monitor = MonitorService()
    except Exception as e:
        print(f"[ERRO CRITICO] Nao foi possivel inicializar o MonitorService: {e}")
        return

    print("==================================================")
    print(f" Agente Iniciado na Maquina: {MACHINE_ID}")
    print(f" Destino VPS: {VPS_URL}")
    print("==================================================")

    while True:
        try:
            clientes_payload = []

            # 1. Tenta coletar usando o metodo padrao se existir
            if hasattr(monitor, "listar_contas_ativas"):
                clientes = monitor.listar_contas_ativas()
                
                # Se retornar dicionario (ex: {pid: {info}})
                if isinstance(clientes, dict):
                    for pid, info in clientes.items():
                        # Obtém o Zen da conta
                        resultado_zen = {}
                        if hasattr(monitor, "obter_zen_conta"):
                            resultado_zen = monitor.obter_zen_conta(pid)
                        
                        # CORRECAO DO NOME: Se o name vier vazio, usa o PID ou titulo da janela como fallback
                        nome_char = info.get("name") or info.get("titulo") or f"Conta-{pid}"
                        
                        clientes_payload.append({
                            "pid": pid,
                            "name": str(nome_char),
                            "resolucao": info.get("resolucao", "N/A"),
                            "zen": resultado_zen.get("zen", 0),
                            "status": resultado_zen.get("status", "OK")
                        })
                elif isinstance(clientes, list):
                    clientes_payload = clientes

            elif hasattr(monitor, "coletar_contas"):
                clientes_payload = monitor.coletar_contas()

            # Garante que se o nome vier igual ao PID ou vazio, ajustamos para ficar limpo no painel
            for c in clientes_payload:
                if not c.get("name") or c.get("name") == str(c.get("pid")):
                    c["name"] = f"Char-PID {c.get('pid')}"

            payload = {
                "machine_id": MACHINE_ID,
                "clientes": clientes_payload
            }

            res = requests.post(VPS_URL, json=payload, timeout=5)
            if res.status_code == 200:
                print(f"[{time.strftime('%H:%M:%S')}] OK! {len(clientes_payload)} conta(s) enviada(s) para a VPS.")
            else:
                print(f"[{time.strftime('%H:%M:%S')}] Erro HTTP {res.status_code}: {res.text}")
        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] Erro no loop do agente: {e}")

        time.sleep(INTERVALO_SEGUNDOS)

if __name__ == "__main__":
    try:
        iniciar_agente()
    except KeyboardInterrupt:
        print("\nAgente encerrado pelo usuario.")