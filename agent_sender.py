import time
import socket
import sys
import os
import requests
import pytesseract
import win32gui
import win32process

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

def obter_titulo_janela_por_pid(target_pid):
    """Busca o título real da janela aberta no Windows associada ao PID do jogo"""
    titulo_encontrado = ""
    def enum_windows_callback(hwnd, extra):
        nonlocal titulo_encontrado
        if win32gui.IsWindowVisible(hwnd):
            _, pid = win32process.GetWindowThreadProcessId(hwnd)
            if pid == target_pid:
                txt = win32gui.GetWindowText(hwnd).strip()
                if txt:
                    titulo_encontrado = txt
    try:
        win32gui.EnumWindows(enum_windows_callback, None)
    except Exception:
        pass
    return titulo_encontrado

def limpar_nome_personagem(titulo_janela, pid):
    """Limpa o título da janela para extrair um nome de personagem bonito"""
    if not titulo_janela:
        return f"Char-{pid}"
    
    # Se o título da janela for muito genérico (ex: "MU", "Main"), usa o PID
    if titulo_janela.lower() in ["mu", "main", "muonline"]:
        return f"Char-{pid}"
        
    # Remove sufixos comuns se houver (ex: "MU - ReDMooN" vira "ReDMooN")
    if " - " in titulo_janela:
        partes = titulo_janela.split(" - ")
        return partes[-1].strip() # Pega a última parte geralmente associada ao char
        
    return titulo_janela

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

            if hasattr(monitor, "listar_contas_ativas"):
                clientes = monitor.listar_contas_ativas()
                
                if isinstance(clientes, dict):
                    for pid, info in clientes.items():
                        resultado_zen = {}
                        if hasattr(monitor, "obter_zen_conta"):
                            resultado_zen = monitor.obter_zen_conta(pid)
                        
                        # Tenta pegar o nome via WinAPI se o monitor não trouxe
                        titulo_janela = info.get("name") or info.get("titulo") or obter_titulo_janela_por_pid(int(pid))
                        nome_limpo = limpar_nome_personagem(titulo_janela, pid)
                        
                        clientes_payload.append({
                            "pid": int(pid),
                            "name": str(nome_limpo),
                            "resolucao": info.get("resolucao", "N/A"),
                            "zen": resultado_zen.get("zen", 0),
                            "status": resultado_zen.get("status", "OK")
                        })
                elif isinstance(clientes, list):
                    clientes_payload = clientes

            elif hasattr(monitor, "coletar_contas"):
                clientes_payload = monitor.coletar_contas()

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