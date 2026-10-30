import os
import sys

PASTA_SERVICES = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_SERVICES)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

from core.windows import obter_clientes_mainmu, capturar_tela_janela_fundo
from core.ocr import ler_texto_imagem
from core.calibrator import obter_box_para_resolucao, carregar_config_ocr


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
        resolucao = info_janela.get("resolucao", "Minimizado")

        rel_box = obter_box_para_resolucao(resolucao)
        if not rel_box:
            # Tenta pegar qualquer calibragem existente no arquivo
            cfg = carregar_config_ocr()
            if "rel_box" in cfg:
                rel_box = tuple(cfg["rel_box"])

        if not rel_box:
            return {"zen": 0, "status": "Falta Calibrar"}

        img = capturar_tela_janela_fundo(hwnd, rel_box)
        if img is None:
            zen_anterior = self.ultimo_zen_valido.get(int(pid), 0)
            return {"zen": zen_anterior, "status": "Minimizado/Cache"}

        valor = ler_texto_imagem(img)
        if valor is not None:
            self.ultimo_zen_valido[int(pid)] = valor
            return {"zen": valor, "status": "Lido com sucesso"}

        zen_anterior = self.ultimo_zen_valido.get(int(pid), 0)
        status_msg = "Inventario fechado (Cache)" if zen_anterior > 0 else "Aguardando leitura"
        return {"zen": zen_anterior, "status": status_msg}