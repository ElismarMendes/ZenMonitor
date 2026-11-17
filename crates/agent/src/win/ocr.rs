//! OCR nativo do Windows (Windows.Media.Ocr). Dispensa o Tesseract.
//! Requer um pacote de idioma com OCR instalado (inglês ou português vêm por padrão).

use super::capture::Image;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

pub struct Ocr {
    engine: OcrEngine,
}

// O OcrEngine é agile (pode ser usado de qualquer thread).
unsafe impl Send for Ocr {}
unsafe impl Sync for Ocr {}

impl Ocr {
    pub fn new() -> anyhow::Result<Self> {
        unsafe {
            // Ignora "já inicializado".
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        Ok(Self { engine })
    }

    pub fn lines(&self, img: &Image) -> anyhow::Result<Vec<String>> {
        let max = OcrEngine::MaxImageDimension()?;
        if img.w > max || img.h > max {
            anyhow::bail!("imagem {}x{} maior que o limite do OCR ({max})", img.w, img.h);
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&img.bgra)?;
        let buffer = writer.DetachBuffer()?;
        let bitmap =
            SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, img.w as i32, img.h as i32)?;
        let result = self.engine.RecognizeAsync(&bitmap)?.get()?;
        let mut out = Vec::new();
        for line in result.Lines()? {
            out.push(line.Text()?.to_string_lossy());
        }
        Ok(out)
    }
}
