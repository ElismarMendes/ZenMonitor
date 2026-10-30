import os
import sys
import json
import cv2
import ctypes

PASTA_CORE = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_CORE)
PASTA_RAIZ = os.path.dirname(PASTA_AGENT)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

from core.windows import obter_clientes_mainmu, capturar_tela_janela_fundo, obter_geometria_cliente
from core.ocr import ler_texto_imagem

CAMINHO_CONFIG_OCR = os.path.join(PASTA_RAIZ, "ocr_config.json")


def carregar_config_ocr() -> dict:
    if os.path.exists(CAMINHO_CONFIG_OCR):
        try:
            with open(CAMINHO_CONFIG_OCR, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            pass
    return {}


def salvar_config_ocr(dados: dict):
    with open(CAMINHO_CONFIG_OCR, "w", encoding="utf-8") as f:
        json.dump(dados, f, indent=4, ensure_ascii=False)


def obter_box_para_resolucao(resolucao: str):
    """Retorna a tupla (x1, y1, x2, y2) para a resolucao ou fallback padrao."""
    cfg = carregar_config_ocr()
    # Suporte ao formato novo por resolucao: {"resolucoes": {"1024x768": [x1, y1, x2, y2]}}
    if "resolucoes" in cfg and resolucao in cfg["resolucoes"]:
        return tuple(cfg["resolucoes"][resolucao])
    # Fallback se existir formato antigo {"rel_box": [x1, y1, x2, y2]}
    if "rel_box" in cfg and isinstance(cfg["rel_box"], list) and len(cfg["rel_box"]) == 4:
        return tuple(cfg["rel_box"])
    return None


def calibrar_interativo(pid_alvo: int = None, forcar_resolucao: str = None) -> tuple:
    """
    Captura uma janela do MU, permite selecionar a area do Zen com o mouse,
    exibe uma amostra da leitura do OCR e salva vinculado a resolucao.
    """
    clientes = obter_clientes_mainmu()
    if not clientes:
        print("[CALIBRADOR] Nenhuma janela do MU Online encontrada aberta!")
        return None

    # Escolhe a janela pelo PID informado, pela resolucao solicitada ou pega a primeira visivel
    escolhido_pid = None
    escolhido_info = None

    for pid, info in clientes.items():
        if info["resolucao"] == "Minimizado":
            continue
        if pid_alvo and pid == pid_alvo:
            escolhido_pid, escolhido_info = pid, info
            break
        if forcar_resolucao and info["resolucao"] == forcar_resolucao:
            escolhido_pid, escolhido_info = pid, info
            break
        if not escolhido_pid:
            escolhido_pid, escolhido_info = pid, info

    if not escolhido_info:
        print("[CALIBRADOR] As janelas do MU estao minimizadas! Restaure ao menos uma janela com o inventario (I) aberto.")
        return None

    hwnd = escolhido_info["hwnd"]
    resolucao = escolhido_info["resolucao"]
    nome_char = escolhido_info["name"]

    geom = obter_geometria_cliente(hwnd)
    if not geom:
        print("[CALIBRADOR] Nao foi possivel obter a geometria da janela.")
        return None

    _, _, largura, altura = geom

    print("\n========================================================")
    print(f" CALIBRADOR DE ZEN - Janela Base: {nome_char} (PID: {escolhido_pid})")
    print(f" Resolucao Detectada: {resolucao}")
    print(" -> Certifique-se de que o Inventario (V/I) esta ABERTO no jogo!")
    print("========================================================")

    while True:
        # Captura a janela inteira em segundo plano
        img_janela = capturar_tela_janela_fundo(hwnd, (0, 0, largura, altura))
        if img_janela is None:
            print("[ERRO] Falha ao capturar imagem da janela do jogo.")
            return None

        titulo_cv = f"Selecione o ZEN [{nome_char} - {resolucao}] e pressione ENTER (ESC cancela)"
        cv2.namedWindow(titulo_cv, cv2.WINDOW_NORMAL)
        cv2.setWindowProperty(titulo_cv, cv2.WND_PROP_TOPMOST, 1)

        roi = cv2.selectROI(titulo_cv, img_janela, showCrosshair=True, fromCenter=False)
        cv2.destroyWindow(titulo_cv)

        x, y, w, h = [int(v) for v in roi]
        if w <= 0 or h <= 0:
            print("[CALIBRADOR] Selecao cancelada pelo usuario.")
            return None

        rel_box = (x, y, x + w, y + h)

        # Recorta a amostra exata e testa o OCR na hora
        recorte_amostra = capturar_tela_janela_fundo(hwnd, rel_box)
        valor_lido = ler_texto_imagem(recorte_amostra)

        # Prepara visualizacao ampliada da amostra + filtro P&B para o usuario validar
        amostra_zoom = cv2.resize(recorte_amostra, (0, 0), fx=3, fy=3, interpolation=cv2.INTER_CUBIC)
        gray = cv2.cvtColor(amostra_zoom, cv2.COLOR_BGR2GRAY)
        _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)
        thresh_bgr = cv2.cvtColor(thresh, cv2.COLOR_GRAY2BGR)

        painel_preview = cv2.vconcat([amostra_zoom, thresh_bgr])

        txt_zen = f"{valor_lido:,}".replace(",", ".") if valor_lido is not None else "FALHA NA LEITURA (None)"
        print(f"\n[AMOSTRA OCR] Coordenadas: {rel_box} | Zen Lido: {txt_zen}")

        # Exibe a amostra na tela e pergunta via MessageBox nativa do Windows se ficou bom
        titulo_prev = f"Amostra OCR: {txt_zen}"
        cv2.imshow(titulo_prev, painel_preview)
        cv2.setWindowProperty(titulo_prev, cv2.WND_PROP_TOPMOST, 1)
        cv2.waitKey(200)

        msg = (
            f"Personagem Base: {nome_char} (PID: {escolhido_pid})\n"
            f"Resolução: {resolucao}\n"
            f"Coordenada Relativa: {rel_box}\n\n"
            f"Valor de Zen lido pelo OCR:\n>>> {txt_zen} <<<\n\n"
            f"A leitura está correta?\n"
            f"• Clique em SIM para salvar e usar em todas as janelas {resolucao}.\n"
            f"• Clique em NÃO para selecionar a área novamente."
        )
        # MB_YESNOCANCEL (0x03) | MB_ICONQUESTION (0x20) | MB_TOPMOST (0x40000)
        resposta = ctypes.windll.user32.MessageBoxW(0, msg, "Validação de Amostra - ZenMonitor", 0x03 | 0x20 | 0x40000)
        cv2.destroyAllWindows()

        if resposta == 6:  # IDYES (Sim)
            cfg = carregar_config_ocr()
            if "resolucoes" not in cfg:
                cfg["resolucoes"] = {}
            cfg["resolucoes"][resolucao] = list(rel_box)
            # Mantem também na chave 'rel_box' raiz para total retrocompatibilidade com scripts antigos
            cfg["rel_box"] = list(rel_box)
            salvar_config_ocr(cfg)
            print(f"[OK] Calibragem salva com sucesso para a resolucao {resolucao}: {rel_box}!")
            return rel_box
        elif resposta == 7:  # IDNO (Não - repetir seleção)
            print("[!] Repetindo selecao da area do Zen...")
            continue
        else:  # IDCANCEL
            print("[CALIBRADOR] Operacao cancelada.")
            return None


def garantir_calibragem_inicial() -> bool:
    """
    Verifica no inicio do bot se existe calibragem para as janelas abertas.
    Se nao existir, abre o calibrador automaticamente na primeira vez.
    """
    clientes = obter_clientes_mainmu()
    if not clientes:
        return True

    for pid, info in clientes.items():
        res = info.get("resolucao", "Minimizado")
        if res != "Minimizado":
            box = obter_box_para_resolucao(res)
            if not box:
                print(f"[!] Resolucao {res} ainda nao calibrada neste computador.")
                print("[!] Iniciando calibrador interativo automaticamente...")
                novo_box = calibrar_interativo(pid_alvo=pid, forcar_resolucao=res)
                return novo_box is not None
    return True


if __name__ == "__main__":
    calibrar_interativo()