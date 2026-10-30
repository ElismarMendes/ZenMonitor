import os
import json
from config import CONFIG_FILE
from core.windows import obter_clientes_mainmu, obter_geometria_cliente, capturar_tela_janela_fundo
from core.ocr import ler_texto_imagem
from core.memory import ler_multiplos_enderecos

class MonitorService:
    def __init__(self):
        self.calibracoes = self.carregar_configuracoes()
        self.estado_memoria = {}

    def carregar_configuracoes(self):
        if os.path.exists(CONFIG_FILE):
            try:
                with open(CONFIG_FILE, "r") as f:
                    return json.load(f)
            except Exception:
                pass
        return {}

    def listar_contas_ativas(self):
        return obter_clientes_mainmu()

    def obter_zen_conta(self, pid: int):
        clientes = self.listar_contas_ativas()
        if pid not in clientes:
            return {"error": "Cliente não encontrado ou fechado."}

        cli = clientes[pid]
        nome = cli["name"]
        hwnd = cli["hwnd"]
        res = cli["resolucao"]

        dados_conta = self.calibracoes.get(nome, {})
        pct_box = dados_conta.get("pct_box") or self.calibracoes.get("_universal_pct_box")

        if not pct_box:
            return {"name": nome, "zen": None, "status": "Não calibrado."}

        geom = obter_geometria_cliente(hwnd)
        if not geom:
            return {"name": nome, "zen": None, "status": "Janela minimizada."}

        client_x, client_y, larg, alt = geom
        px1, py1, px2, py2 = pct_box
        rel_box = [int(px1 * larg), int(py1 * alt), int(px2 * larg), int(py2 * alt)]

        # Tenta BitBlt
        img_np = capturar_tela_janela_fundo(hwnd, rel_box)
        # Fallback ImageGrab
        if img_np is None or img_np.size == 0:
            rx1, ry1, rx2, ry2 = rel_box
            screenshot = ImageGrab.grab(bbox=(client_x + rx1, client_y + ry1, client_x + rx2, client_y + ry2))
            img_np = np.array(screenshot)

        zen_val = ler_texto_imagem(img_np)
        return {
            "name": nome,
            "pid": pid,
            "zen": zen_val,
            "resolucao": res,
            "status": "Lido com sucesso" if zen_val is not None else "Aguardando leitura"
        }