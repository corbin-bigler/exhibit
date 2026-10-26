//! HEIC/HEIF and AVIF through Windows Imaging Component.
//!
//! Needs the Microsoft Store "HEIF Image Extensions" + "HEVC Video Extensions" (HEIC) or
//! "AV1 Video Extension" (AVIF). Most Windows 11 machines have them; without them WIC
//! reports that no decoder was found and the import fails with that message.

use std::path::Path;

use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, GUID_WICPixelFormat64bppRGBA, IWICColorContext,
    IWICImagingFactory, IWICPixelFormatInfo, WICColorContextProfile, WICConvertBitmapSource,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::{HSTRING, Interface};

use crate::decode::{DecodedImage, fingerprint};

pub fn decode(path: &Path, file_bytes: &[u8]) -> Result<DecodedImage, String> {
    unsafe { decode_unsafe(path, file_bytes) }.map_err(|e| {
        if e.code().0 as u32 == 0x88982F50 {
            "Windows has no decoder for this format. Install HEIF Image Extensions and HEVC Video \
             Extensions (HEIC) or AV1 Video Extension (AVIF) from the Microsoft Store."
                .to_string()
        } else {
            e.message().to_string()
        }
    })
}

unsafe fn decode_unsafe(path: &Path, file_bytes: &[u8]) -> windows::core::Result<DecodedImage> {
    unsafe {
        // Already initialised on this thread is fine; a different apartment is fine for WIC too.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(path.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let (mut width, mut height) = (0u32, 0u32);
        frame.GetSize(&mut width, &mut height)?;

        let icc = read_icc(&factory, &frame)?;

        let source_format = frame.GetPixelFormat()?;
        let info: IWICPixelFormatInfo = factory.CreateComponentInfo(&source_format)?.cast()?;
        let bits = info.GetBitsPerPixel()?;
        let channels = info.GetChannelCount()?.max(1);
        let sixteen = bits / channels > 8;

        let target = if sixteen { GUID_WICPixelFormat64bppRGBA } else { GUID_WICPixelFormat32bppRGBA };
        let converted = WICConvertBitmapSource(&target, &frame)?;
        let bytes_per_pixel = if sixteen { 8 } else { 4 };
        let stride = width * bytes_per_pixel;
        let mut buffer = vec![0u8; (stride * height) as usize];
        converted.CopyPixels(std::ptr::null(), stride, &mut buffer)?;

        let print = fingerprint(file_bytes);
        Ok(if sixteen {
            let samples: Vec<u16> = buffer.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
            DecodedImage::from_interleaved_u16(width, height, 4, true, &samples, icc, print)
        } else {
            DecodedImage::from_interleaved_u8(width, height, 4, true, &buffer, icc, print)
        })
    }
}

unsafe fn read_icc(
    factory: &IWICImagingFactory,
    frame: &windows::Win32::Graphics::Imaging::IWICBitmapFrameDecode,
) -> windows::core::Result<Option<Vec<u8>>> {
    unsafe {
        let mut count = 0u32;
        if frame.GetColorContexts(&mut [], &mut count).is_err() || count == 0 {
            return Ok(None);
        }
        let mut contexts: Vec<Option<IWICColorContext>> =
            (0..count).map(|_| factory.CreateColorContext().ok()).collect();
        frame.GetColorContexts(&mut contexts, &mut count)?;
        for context in contexts.into_iter().flatten() {
            if context.GetType()? != WICColorContextProfile {
                continue;
            }
            let mut size = 0u32;
            context.GetProfileBytes(&mut [], &mut size)?;
            if size == 0 {
                continue;
            }
            let mut profile = vec![0u8; size as usize];
            context.GetProfileBytes(&mut profile, &mut size)?;
            return Ok(Some(profile));
        }
        Ok(None)
    }
}
