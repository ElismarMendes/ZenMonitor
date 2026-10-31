import os
import sys

PASTA_SERVICES = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_SERVICES)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

from core.windows import obter_clientes_mainmu, capturar_tela_janela_fundo
from core.ocr import ler_texto_imagem
from core.calibrator import obter_box_para_resolucao, carregar_config_ocr
from core.logger import log_capture, log_ocr

class MonitorService:
    def __init__(self):
        self.ultimo_zen_valido = {}

    def listar_contas_ativas(self) -> dict:
        return obter_clientes_mainmu()

    def obter_zen_conta(self, pid: int, info_janela: dict = None) -> dict:
        if not info_janela:
            clientes = self.listar_contas_ativas()
            info_janela = clientes.get(int(pid))

        if not info_janela:
            return {"zen": 0, "status": "Janela fechada"}

        hwnd = info_janela.get("hwnd")
        char_name = info_janela.get("name", f"PID:{pid}")
        resolucao = info_janela.get("resolucao", "1024x768")

        rel_box = obter_box_para_resolucao(resolucao)
        if not rel_box:
            cfg = carregar_config_ocr()
            if "rel_box" in cfg:
                rel_box = tuple(cfg["rel_box"])

        if not rel_box:
            return {"zen": 0, "status": "Falta Calibrar"}

        img = capturar_tela_janela_fundo(hwnd, rel_box)
        zen_anterior = self.ultimo_zen_valido.get(int(pid), 0)

        if img is None or img.size == 0:
            log_capture(pid, char_name, "Híbrido", "Minimizado/Vazio")
            status_msg = "Minimizado (Cache)" if zen_anterior > 0 else "Minimizado"
            return {"zen": zen_anterior, "status": status_msg}

        valor = ler_texto_imagem(img)
        
        if valor is not None and valor > 0:
            self.ultimo_zen_valido[int(pid)] = valor
            log_ocr(pid, char_name, "OK", str(valor), valor)
            return {"zen": valor, "status": "Lido com sucesso"}
        
        log_ocr(pid, char_name, "FALHA OCR", "None", 0)
        return {"zen": zen_anterior, "status": "Inventário Fechado (Cache)"}