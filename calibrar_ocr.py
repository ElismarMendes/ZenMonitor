import os
import sys
import json
import tkinter as tk
from tkinter import ttk, messagebox
from PIL import ImageGrab, ImageTk
import win32gui

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "local_agent"))
try:
    from config import CONFIG_FILE
except ImportError:
    CONFIG_FILE = "ocr_config.json"

class CalibradorOCR:
    def __init__(self, root):
        self.root = root
        self.root.title("ZenMonitor - Calibrador de OCR Aprimorado")
        self.root.geometry("520x300")
        self.root.resizable(False, False)

        self.janelas = self.listar_janelas_mu()

        frame = ttk.Frame(root, padding=15)
        frame.pack(fill="both", expand=True)

        ttk.Label(frame, text="1. Abra o inventário (tecla V) com o Zen visível:", font=("Segoe UI", 9, "bold")).pack(anchor="w", pady=(0, 5))
        self.combo = ttk.Combobox(frame, values=[f"{hwnd} - {titulo}" for hwnd, titulo in self.janelas], width=58, state="readonly")
        self.combo.pack(fill="x", pady=(0, 10))
        if self.janelas:
            self.combo.current(0)

        btn_frame = ttk.Frame(frame)
        btn_frame.pack(fill="x", pady=5)
        ttk.Button(btn_frame, text="🔄 Atualizar Lista", command=self.atualizar_lista).pack(side="left", padx=(0, 10))
        ttk.Button(btn_frame, text="🎯 Iniciar Calibragem Visual", command=self.iniciar_selecao).pack(side="left")

        self.lbl_status = ttk.Label(frame, text="Status: Aguardando seleção...", foreground="blue", font=("Consolas", 9))
        self.lbl_status.pack(anchor="w", pady=(15, 0))
        self.mostrar_config_atual()

    def listar_janelas_mu(self):
        res = []
        def callback(hwnd, _):
            if win32gui.IsWindowVisible(hwnd):
                titulo = win32gui.GetWindowText(hwnd)
                if titulo:
                    res.append((hwnd, titulo))
        win32gui.EnumWindows(callback, None)
        return res

    def atualizar_lista(self):
        self.janelas = self.listar_janelas_mu()
        self.combo["values"] = [f"{hwnd} - {titulo}" for hwnd, titulo in self.janelas]
        if self.janelas:
            self.combo.current(0)

    def mostrar_config_atual(self):
        if os.path.exists(CONFIG_FILE):
            try:
                with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                    dados = json.load(f)
                info_res = dados.get("zen_roi", {})
                self.lbl_status.config(text=f"Calibrado! X={info_res.get('x')}, Y={info_res.get('y')}, W={info_res.get('w')}, H={info_res.get('h')}")
                return
            except Exception:
                pass
        self.lbl_status.config(text="Ainda não calibrado nesta máquina.")

    def iniciar_selecao(self):
        selecao = self.combo.get()
        if not selecao:
            messagebox.showwarning("Aviso", "Selecione uma janela primeiro!")
            return

        hwnd = int(selecao.split(" - ")[0])
        try:
            if win32gui.IsIconic(hwnd):
                win32gui.ShowWindow(hwnd, 9)
            win32gui.SetForegroundWindow(hwnd)
        except Exception:
            pass

        self.root.withdraw()
        self.root.after(500, lambda: self.abrir_tela_recorte(hwnd))

    def abrir_tela_recorte(self, hwnd):
        try:
            rect = win32gui.GetWindowRect(hwnd)
            x1, y1, x2, y2 = rect
            
            # Captura a tela inteira para evitar desalinhamento de DPI/Bordas
            img = ImageGrab.grab(all_screens=True)
            
            top = tk.Toplevel(self.root)
            top.attributes("-fullscreen", True)
            top.attributes("-topmost", True)

            canvas = tk.Canvas(top, cursor="cross", bg="black")
            canvas.pack(fill="both", expand=True)

            tk_img = ImageTk.PhotoImage(img)
            canvas.img = tk_img
            canvas.create_image(0, 0, anchor="nw", image=tk_img)

            # Guia visual da janela do jogo detectada
            canvas.create_rectangle(x1, y1, x2, y2, outline="yellow", width=1, dash=(4, 4))
            canvas.create_text(x1, y1 - 15, text=f"Janela Alvo (Ref: {x1}, {y1}) - Arraste sobre o Zen", fill="yellow", anchor="nw", font=("Arial", 10, "bold"))

            coords = {"start_x": 0, "start_y": 0, "rect": None}

            def on_press(event):
                coords["start_x"] = event.x
                coords["start_y"] = event.y
                if coords["rect"]:
                    canvas.delete(coords["rect"])
                coords["rect"] = canvas.create_rectangle(event.x, event.y, event.x, event.y, outline="#00ff00", width=2)

            def on_drag(event):
                canvas.coords(coords["rect"], coords["start_x"], coords["start_y"], event.x, event.y)

            def on_release(event):
                rx1 = min(coords["start_x"], event.x)
                ry1 = min(coords["start_y"], event.y)
                rx2 = max(coords["start_x"], event.x)
                ry2 = max(coords["start_y"], event.y)
                w = rx2 - rx1
                h = ry2 - ry1

                if w < 5 or h < 5:
                    messagebox.showwarning("Aviso", "Seleção muito pequena. Tente novamente.")
                    top.destroy()
                    self.root.deiconify()
                    return

                # Salva as coordenadas absolutas na tela e relativas à janela do jogo
                rel_x = rx1 - x1
                rel_y = ry1 - y1

                dados = {}
                if os.path.exists(CONFIG_FILE):
                    try:
                        with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                            dados = json.load(f)
                    except Exception:
                        dados = {}

                novo_bloco = {
                    "x": rel_x, "y": rel_y, "w": w, "h": h,
                    "abs_x1": rx1, "abs_y1": ry1, "abs_x2": rx2, "abs_y2": ry2
                }
                dados["zen_roi"] = novo_bloco

                for destino in [CONFIG_FILE, os.path.join("local_agent", os.path.basename(CONFIG_FILE))]:
                    try:
                        with open(destino, "w", encoding="utf-8") as f:
                            json.dump(dados, f, indent=4)
                    except Exception:
                        pass

                top.destroy()
                self.root.deiconify()
                self.mostrar_config_atual()
                messagebox.showinfo("Sucesso", f"Calibragem salva com sucesso!\nRelativo à janela: X={rel_x}, Y={rel_y}, Largura={w}, Altura={h}")

            canvas.bind("<ButtonPress-1>", on_press)
            canvas.bind("<B1-Motion>", on_drag)
            canvas.bind("<ButtonRelease-1>", on_release)
            top.bind("<Escape>", lambda e: (top.destroy(), self.root.deiconify()))

        except Exception as e:
            messagebox.showerror("Erro", f"Falha ao capturar tela: {e}")
            self.root.deiconify()

if __name__ == "__main__":
    root = tk.Tk()
    app = CalibradorOCR(root)
    root.mainloop()