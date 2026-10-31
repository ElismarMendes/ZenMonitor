# -*- coding: utf-8 -*-
import os
import sys
import subprocess
import time
import socket
import ctypes
import uuid
import json

# Garante o diretorio de trabalho correto logo de inicio
PASTA_PROJETO = os.path.dirname(os.path.abspath(__file__))
if PASTA_PROJETO:
    os.chdir(PASTA_PROJETO)

PYTHON_VENV = os.path.join(PASTA_PROJETO, ".venv", "Scripts", "python.exe")

# Se a venv existir e nao estivermos nela, reinicia usando o python da venv
if os.path.exists(PYTHON_VENV) and os.path.normcase(sys.executable) != os.path.normcase(PYTHON_VENV):
    print("[AUTO-FIX] Alternando automaticamente para o Python da .venv...")
    sys.exit(subprocess.call([PYTHON_VENV] + sys.argv))

try:
    import requests
    import pytesseract
except ImportError:
    print("[AUTO-FIX] Instalando dependencias ausentes...")
    try:
        subprocess.check_call([sys.executable, "-m", "pip", "install", "-r", os.path.join(PASTA_PROJETO, "requirements.txt")])
        import requests
        import pytesseract
    except Exception as e:
        print(f"[ERRO CRITICO] Falha ao instalar dependencias: {e}")
        input("Pressione ENTER para sair...")
        sys.exit(1)

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

try:
    from services.monitor_service import MonitorService
    from core.calibrator import garantir_calibragem_inicial, calibrar_interativo, salvar_config_ocr, carregar_config_ocr
    from core.windows import TIPO_AMBIENTE, obter_clientes_mainmu
    from core.logger import log_init, log_windows, log_sync
    from core.memory import buscar_valor_4bytes
except Exception as e:
    print(f"[ERRO CRITICO] Falha ao importar modulos internos: {e}")
    input("Pressione ENTER para sair...")
    sys.exit(1)

VPS_URL = "https://zen.brautosystem.com.br/api/v1/report"

# ==============================================================================
# GERENCIAMENTO INTELIGENTE E ÚNICO DE MACHINE_ID (EVITA CONFLITOS NA VPS)
# ==============================================================================
CONFIG_INSTANCIA_FILE = "machine_uuid.json"

def obter_machine_id():
    # Permite definir um ID personalizado direto na linha de comando, ex: python agent_sender.py --id MeuPC
    for i, arg in enumerate(sys.argv):
        if arg == "--id" and i + 1 < len(sys.argv):
            return sys.argv[i + 1]

    hostname = socket.gethostname()
    
    # Se ja existir um ID unico salvo nesta pasta, reaproveita ele
    if os.path.exists(CONFIG_INSTANCIA_FILE):
        try:
            with open(CONFIG_INSTANCIA_FILE, "r", encoding="utf-8") as f:
                data = json.load(f)
                if "machine_id" in data:
                    return data["machine_id"]
        except Exception:
            pass

    # Caso contrario, gera um identificador exclusivo combinando hostname + sufixo aleatorio
    unique_id = f"{hostname}-{uuid.uuid4().hex[:6]}"
    try:
        with open(CONFIG_INSTANCIA_FILE, "w", encoding="utf-8") as f:
            json.dump({"machine_id": unique_id}, f)
    except Exception:
        pass
    return unique_id

MACHINE_ID = obter_machine_id()
INTERVALO_SEGUNDOS = 5


def verificar_admin():
    try:
        return ctypes.windll.shell32.IsUserAnAdmin() != 0
    except Exception:
        return False


def elevar_privilegio_se_necessario():
    if not verificar_admin() and "--no-admin" not in sys.argv:
        print("[AUTO-ELEVACAO] Solicitando permissao de Administrador para leitura direta de RAM...")
        
        script_path = os.path.abspath(__file__)
        python_exec = PYTHON_VENV if os.path.exists(PYTHON_VENV) else sys.executable
        
        parametros = f'"{script_path}" ' + " ".join([f'"{arg}"' for arg in sys.argv[1:]])
        
        res = ctypes.windll.shell32.ShellExecuteW(None, "runas", python_exec, parametros, PASTA_PROJETO, 1)
        if int(res) > 32:
            sys.exit(0)
        else:
            print("[AVISO] Elevacao recusada. O agente rodara apenas via OCR (sem leitura de RAM).")


def executar_teste_ram():
    print("\n[DIAGNOSTICO] Testando acesso direto a RAM dos processos do MU...")
    is_admin = verificar_admin()
    print(f"[DIAGNOSTICO] Rodando como Administrador: {is_admin}")
    print(f"[DIAGNOSTICO] Machine ID Ativo: {MACHINE_ID}")
    
    clientes = obter_clientes_mainmu()
    if not clientes:
        print("[DIAGNOSTICO] Nenhuma janela do MU Online encontrada para testar!")
        return

    for pid, info in clientes.items():
        print(f"[DIAGNOSTICO] Testando PID {pid} ({info['name']})...")
        cands = buscar_valor_4bytes(pid, 1000)
        print(f"[DIAGNOSTICO] PID {pid} -> Resultado: {'SUCESSO (RAM Acessivel)' if len(cands) > 0 else 'FALHA (Sem acesso a memoria ou processo protegido)'}")
    print("\n[DIAGNOSTICO] Teste concluido.")


def iniciar_agente():
    elevar_privilegio_se_necessario()

    if "--calibrar" in sys.argv:
        calibrar_interativo()
        return

    if "--test-ram" in sys.argv:
        executar_teste_ram()
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
    print(f" ID da Maquina (Instancia): {MACHINE_ID}")
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
    except Exception as e:
        print(f"[ERRO FATAL] {e}")
        import traceback
        traceback.print_exc()
        input("\nPressione ENTER para fechar a janela...")
    except KeyboardInterrupt:
        print("\nAgente encerrado pelo usuario.")