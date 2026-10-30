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
        pass

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
            cfg = carregar_config_ocr()
            if "rel_box" in cfg:
                rel_box = tuple(cfg["rel_box"])

        if not rel_box:
            return {"zen": 0, "status": "Falta Calibrar"}

        # Forca a captura direta exata (igual ao calibrador) para evitar buffer congelado do VMware
        img = capturar_tela_janela_fundo(hwnd, rel_box, forcar_tela_direta=True)
        if img is None or img.size == 0:
            return {"zen": 0, "status": "Minimizado"}

        # Realiza a leitura limpa via OCR em tempo real
        valor = ler_texto_imagem(img)
        
        if valor is not None:
            return {"zen": valor, "status": "Lido com sucesso"}
        
        return {"zen": 0, "status": "Erro OCR / Inventario Fechado"}