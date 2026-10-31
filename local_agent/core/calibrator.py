import os
import sys
import time
import json
import cv2
import ctypes

try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

PASTA_CORE = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_CORE)
PASTA_RAIZ = os.path.dirname(PASTA_AGENT)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

from core.windows import obter_clientes_mainmu, capturar_tela_janela_fundo, obter_geometria_cliente
from core.ocr import ler_texto_imagem, preprocessar_para_ocr
from core.logger import log_calibration

try:
    import win32gui
    import win32con
except ImportError:
    win32gui = None

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
    cfg = carregar_config_ocr()
    if "resolucoes" in cfg and resolucao in cfg["resolucoes"]:
        return tuple(cfg["resolucoes"][resolucao])
    if "rel_box" in cfg and isinstance(cfg["rel_box"], list) and len(cfg["rel_box"]) == 4:
        return tuple(cfg["rel_box"])
    return None

def focar_janela_jogo(hwnd):
    if not win32gui:
        return
    try:
        if win32gui.IsIconic(hwnd):
            win32gui.ShowWindow(hwnd, win32con.SW_RESTORE)
        ctypes.windll.user32.keybd_event(0x12, 0, 0, 0)
        ctypes.windll.user32.keybd_event(0x12, 0, 2, 0)
        win32gui.SetForegroundWindow(hwnd)
        time.sleep(0.5)
    except Exception:
        pass

def calibrar_interativo(pid_alvo: int = None, forcar_resolucao: str = None) -> tuple:
    clientes = obter_clientes_mainmu()
    if not clientes:
        print("[CALIBRADOR] Nenhuma janela do MU Online encontrada aberta!")
        return None

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
        print("[CALIBRADOR] As janelas do MU estão minimizadas!")
        return None

    hwnd = escolhido_info["hwnd"]
    resolucao = escolhido_info["resolucao"]
    nome_char = escolhido_info["name"]

    geom = obter_geometria_cliente(hwnd)
    if not geom:
        print("[CALIBRADOR] Não foi possível obter a geometria da janela.")
        return None

    _, _, largura, altura = geom

    print("\n========================================================")
    print(f" CALIBRADOR DE ZEN - Janela Base: {nome_char} (PID: {escolhido_pid})")
    print(f" Resolução Detectada: {resolucao}")
    print("========================================================")

    while True:
        focar_janela_jogo(hwnd)
        img_janela = capturar_tela_janela_fundo(hwnd, (0, 0, largura, altura))
        if img_janela is None:
            print("[ERRO] Falha ao capturar imagem da janela do jogo.")
            return None

        titulo_cv = f"Selecione APENAS OS NUMEROS DO ZEN [{nome_char} - {resolucao}] e aperte ENTER"
        cv2.namedWindow(titulo_cv, cv2.WINDOW_NORMAL)
        cv2.resizeWindow(titulo_cv, largura, altura)
        cv2.setWindowProperty(titulo_cv, cv2.WND_PROP_TOPMOST, 1)

        roi = cv2.selectROI(titulo_cv, img_janela, showCrosshair=True, fromCenter=False)
        cv2.destroyWindow(titulo_cv)

        x, y, w, h = [int(v) for v in roi]
        if w <= 0 or h <= 0:
            print("[CALIBRADOR] Seleção cancelada pelo usuário.")
            return None

        rel_box = (x, y, x + w, y + h)
        recorte_amostra = img_janela[y:y+h, x:x+w]
        valor_lido = ler_texto_imagem(recorte_amostra)

        img_ocr_pb = preprocessar_para_ocr(recorte_amostra)
        img_ocr_bgr = cv2.cvtColor(img_ocr_pb, cv2.COLOR_GRAY2BGR)
        amostra_zoom = cv2.resize(recorte_amostra, (img_ocr_bgr.shape[1], img_ocr_bgr.shape[0]), interpolation=cv2.INTER_CUBIC)

        painel_preview = cv2.vconcat([amostra_zoom, img_ocr_bgr])
        txt_zen = f"{valor_lido:,}".replace(",", ".") if valor_lido is not None else "FALHA NA LEITURA"
        
        log_calibration(resolucao, rel_box, valor_lido)

        titulo_prev = f"Como o OCR enxerga: {txt_zen}"
        cv2.imshow(titulo_prev, painel_preview)
        cv2.setWindowProperty(titulo_prev, cv2.WND_PROP_TOPMOST, 1)
        cv2.waitKey(200)

        msg = (
            f"Personagem Base: {nome_char} (PID: {escolhido_pid})\n"
            f"Resolução: {resolucao}\n"
            f"Coordenada Relativa: {rel_box}\n\n"
            f"Valor de Zen lido pelo OCR:\n>>> {txt_zen} <<<\n\n"
            f"A área selecionada está correta?\n"
            f"• Clique em SIM para salvar.\n"
            f"• Clique em NÃO para selecionar novamente."
        )
        resposta = ctypes.windll.user32.MessageBoxW(0, msg, "Validação de Amostra - ZenMonitor", 0x03 | 0x20 | 0x40000)
        cv2.destroyAllWindows()

        if resposta == 6:  # SIM
            cfg = carregar_config_ocr()
            if "resolucoes" not in cfg:
                cfg["resolucoes"] = {}
            cfg["resolucoes"][resolucao] = list(rel_box)
            cfg["rel_box"] = list(rel_box)
            salvar_config_ocr(cfg)
            print(f"[OK] Calibragem salva com sucesso para a resolução {resolucao}: {rel_box}!")
            return rel_box
        elif resposta == 7:  # NÃO
            print("[!] Repetindo seleção da área do Zen...")
            continue
        else:
            print("[CALIBRADOR] Operação cancelada.")
            return None

def garantir_calibragem_inicial() -> bool:
    clientes = obter_clientes_mainmu()
    if not clientes:
        return True

    for pid, info in clientes.items():
        res = info.get("resolucao", "Minimizado")
        if res != "Minimizado":
            box = obter_box_para_resolucao(res)
            if not box:
                print(f"[!] Resolução {res} ainda não calibrada neste computador.")
                novo_box = calibrar_interativo(pid_alvo=pid, forcar_resolucao=res)
                return novo_box is not None
    return True

if __name__ == "__main__":
    calibrar_interativo()