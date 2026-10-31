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

from core.windows import (
    obter_clientes_mainmu,
    capturar_tela_janela_fundo,
    obter_geometria_cliente,
    converter_pct_para_box_cliente,
)
from core.ocr import ler_texto_imagem, preprocessar_para_ocr
from core.logger import log_calibration

try:
    import win32gui
    import win32con
except ImportError:
    win32gui = None

CAMINHO_CONFIG_OCR = os.path.join(PASTA_RAIZ, "ocr_config.json")


def carregar_config_ocr():
    if os.path.exists(CAMINHO_CONFIG_OCR):
        try:
            with open(CAMINHO_CONFIG_OCR, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            pass
    return {}


def salvar_config_ocr(dados):
    try:
        with open(CAMINHO_CONFIG_OCR, "w", encoding="utf-8") as f:
            json.dump(dados, f, indent=4, ensure_ascii=False)
    except Exception:
        pass


def obter_box_inteligente(hwnd, resolucao, nome_char=None):
    cfg = carregar_config_ocr()

    pct_box = None
    if nome_char and "contas" in cfg and nome_char in cfg["contas"]:
        pct_box = cfg["contas"][nome_char].get("pct_box")

    if not pct_box:
        pct_box = cfg.get("_universal_pct_box")

    if pct_box and hwnd:
        rel_box, _ = converter_pct_para_box_cliente(hwnd, pct_box)
        if rel_box:
            return rel_box

    if "resolucoes" in cfg and resolucao in cfg["resolucoes"]:
        box_px = cfg["resolucoes"][resolucao]
        try:
            larg_str, alt_str = resolucao.lower().split("x")
            larg, alt = int(larg_str), int(alt_str)
            pct_migrado = [
                max(0.0, box_px[0] / larg),
                max(0.0, box_px[1] / alt),
                min(1.0, box_px[2] / larg),
                min(1.0, box_px[3] / alt),
            ]
            cfg["_universal_pct_box"] = pct_migrado
            salvar_config_ocr(cfg)
        except Exception:
            pass
        return tuple(box_px)

    if "rel_box" in cfg and isinstance(cfg["rel_box"], list) and len(cfg["rel_box"]) == 4:
        return tuple(cfg["rel_box"])

    return None


def obter_enderecos_ram_salvos(nome_char, pid_atual):
    cfg = carregar_config_ocr()
    dados_conta = cfg.get("contas", {}).get(nome_char, {})
    if dados_conta.get("pid_salvo") == int(pid_atual):
        return [int(e) for e in dados_conta.get("enderecos_ram", [])]
    return []


def salvar_enderecos_ram_conta(nome_char, pid_atual, enderecos):
    cfg = carregar_config_ocr()
    if "contas" not in cfg:
        cfg["contas"] = {}
    if nome_char not in cfg["contas"]:
        cfg["contas"][nome_char] = {}

    cfg["contas"][nome_char]["pid_salvo"] = int(pid_atual)
    cfg["contas"][nome_char]["enderecos_ram"] = [int(e) for e in enderecos]
    salvar_config_ocr(cfg)


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


def calibrar_interativo(pid_alvo=None, forcar_resolucao=None):
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
        print("[CALIBRADOR] Desminimize pelo menos uma janela do MU para calibrar!")
        return None

    hwnd = escolhido_info["hwnd"]
    resolucao = escolhido_info["resolucao"]
    nome_char = escolhido_info["name"]

    geom = obter_geometria_cliente(hwnd)
    if not geom:
        return None

    _, _, largura, altura = geom

    print("\n========================================================")
    print(f" CALIBRADOR UNIVERSAL DE ZEN - Base: {nome_char} (PID: {escolhido_pid})")
    print(f" Resolucao Detectada: {resolucao}")
    print("========================================================")

    while True:
        focar_janela_jogo(hwnd)
        img_janela = capturar_tela_janela_fundo(hwnd, (0, 0, largura, altura), forcar_tela_direta=True)
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
            print("[CALIBRADOR] Selecao cancelada.")
            return None

        rel_box = (x, y, x + w, y + h)
        pct_box = [
            max(0.0, x / largura),
            max(0.0, y / altura),
            min(1.0, (x + w) / largura),
            min(1.0, (y + h) / altura),
        ]

        recorte_amostra = img_janela[y : y + h, x : x + w]
        valor_lido = ler_texto_imagem(recorte_amostra)

        img_ocr_pb = preprocessar_para_ocr(recorte_amostra)
        img_ocr_bgr = cv2.cvtColor(img_ocr_pb, cv2.COLOR_GRAY2BGR)
        amostra_zoom = cv2.resize(recorte_amostra, (img_ocr_bgr.shape[1], img_ocr_bgr.shape[0]), interpolation=cv2.INTER_CUBIC)

        painel_preview = cv2.vconcat([amostra_zoom, img_ocr_bgr])
        txt_zen = f"{valor_lido:,}".replace(",", ".") if valor_lido is not None else "FALHA NA LEITURA"

        log_calibration(resolucao, rel_box, pct_box, valor_lido)

        titulo_prev = f"Preview OCR: {txt_zen}"
        cv2.imshow(titulo_prev, painel_preview)
        cv2.setWindowProperty(titulo_prev, cv2.WND_PROP_TOPMOST, 1)
        cv2.waitKey(200)

        msg = (
            f"Personagem Base: {nome_char} (PID: {escolhido_pid})\n"
            f"Resolucao: {resolucao}\n"
            f"Coordenada Relativa: {rel_box}\n"
            f"Proporcao Universal: {[round(v, 4) for v in pct_box]}\n\n"
            f"Valor de Zen lido pelo OCR:\n{txt_zen}\n\n"
            f"A area selecionada esta correta?\n"
            f"- Clique em SIM para salvar como padrao universal.\n"
            f"- Clique em NAO para selecionar novamente."
        )
        resposta = ctypes.windll.user32.MessageBoxW(0, msg, "Validacao Universal - ZenMonitor", 0x03 | 0x20 | 0x40000)
        cv2.destroyAllWindows()

        if resposta == 6:
            cfg = carregar_config_ocr()
            if "resolucoes" not in cfg:
                cfg["resolucoes"] = {}
            if "contas" not in cfg:
                cfg["contas"] = {}

            cfg["resolucoes"][resolucao] = list(rel_box)
            cfg["rel_box"] = list(rel_box)
            cfg["_universal_pct_box"] = pct_box
            cfg["contas"][nome_char] = {
                "pct_box": pct_box,
                "resolucao_base": resolucao,
                "pid_salvo": int(escolhido_pid),
                "enderecos_ram": [],
            }
            salvar_config_ocr(cfg)
            print("[OK] Calibragem Universal salva! Todas as janelas e resolucoes usarao essa proporcao.")
            return rel_box
        elif resposta == 7:
            continue
        else:
            return None


def garantir_calibragem_inicial():
    cfg = carregar_config_ocr()
    if "_universal_pct_box" in cfg or "rel_box" in cfg or "resolucoes" in cfg:
        return True

    clientes = obter_clientes_mainmu()
    for pid, info in clientes.items():
        res = info.get("resolucao", "Minimizado")
        if res != "Minimizado":
            print("[!] Nenhuma calibracao encontrada. Iniciando calibrador universal...")
            novo_box = calibrar_interativo(pid_alvo=pid, forcar_resolucao=res)
            return novo_box is not None
    return True