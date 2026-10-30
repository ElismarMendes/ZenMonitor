import os
import re

CONFIG_FILE = "config_zen.json"
TESSERACT_CMD = r'C:\Program Files\Tesseract-OCR\tesseract.exe'
PADRAO_TITULO = re.compile(r"Name:\s*([^\s\|]+)")