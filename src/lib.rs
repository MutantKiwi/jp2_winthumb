#![crate_type = "dylib"]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(unused_must_use)]
#![allow(non_snake_case)]

use std::{cell::RefCell, io::Read};
use windows::core::{GUID, Interface, implement};

mod registry;
mod winstream;
use winstream::WinStream;

use windows as Windows;
use windows::Win32::{
    Foundation::*,
    Graphics::Imaging::*,
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IStream},
};

mod dll;
mod guid;

/// Decoded JP2 image as a flat RGBA8 buffer.
pub struct DecodedResult {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

#[implement(Windows::Win32::Graphics::Imaging::IWICBitmapDecoder)]
#[derive(Default)]
pub struct JP2WICBitmapDecoder {
    decoded: RefCell<Option<DecodedResult>>,
}

impl JP2WICBitmapDecoder {
    // Freshly generated — unique to this handler.
    pub const CLSID: GUID = GUID::from_u128(0x2a8f3b5e_7c41_4d92_be6a_1f8e5c3d9b47);
    pub const CONTAINER_ID: GUID = GUID::from_u128(0x4d7a9e23_8c51_4f38_ad5b_2e6f3c8a1b94);
}

/// Read the whole IStream into memory. JPEG 2000 decoders need the full
/// codestream before they can produce pixels, and thumbnail requests are
/// small files anyway.
fn read_stream_to_vec(stream: &IStream) -> windows::core::Result<Vec<u8>> {
    let mut reader = WinStream::from(stream);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).map_err(|e| {
        windows::core::Error::new(WINCODEC_ERR_STREAMREAD, format!("{e}"))
    })?;
    Ok(buf)
}

/// Thumbnail requests from Explorer are small (~256 px). Decoding a 10k×10k
/// aerial JP2 at full resolution to answer is 20–30× slower than decoding
/// at a matched pyramid level. 512 gives a generous safety margin above any
/// Explorer thumbnail size while still being tiny.
const TARGET_MAX_DIM: u32 = 512;

/// Decide how many reduce levels to request. `reduce=n` means dimensions
/// divided by 2^n. We want max(w,h) / 2^n <= TARGET_MAX_DIM.
fn pick_reduce(orig_w: u32, orig_h: u32) -> u32 {
    let m = orig_w.max(orig_h);
    let mut level = 0u32;
    let mut cur = m;
    while cur > TARGET_MAX_DIM && level < 8 {
        cur /= 2;
        level += 1;
    }
    level
}

/// Decode a JP2 byte buffer to an 8-bit RGBA flat buffer.
///
/// Two-pass: first peek the header to get real dimensions, then do the real
/// decode at an appropriate reduce level. The peek is a cheap header parse,
/// not a full decode.
fn decode_jp2_to_rgba(bytes: &[u8]) -> windows::core::Result<DecodedResult> {
    // Pass 1 — header peek with max reduction to avoid wasting work. This
    // populates orig_width/orig_height from the codestream without decoding
    // actual pixel data at full resolution.
    let peek_params = jpeg2k::DecodeParameters::new().reduce(8);
    let orig = jpeg2k::Image::from_bytes_with(bytes, peek_params).map_err(|e| {
        windows::core::Error::new(WINCODEC_ERR_BADIMAGE, format!("jpeg2k peek: {e:?}"))
    })?;
    let orig_w = orig.orig_width();
    let orig_h = orig.orig_height();
    drop(orig); // release the peek allocation

    // Pass 2 — real decode at a matched pyramid level.
    let reduce = pick_reduce(orig_w, orig_h);
    let params = jpeg2k::DecodeParameters::new().reduce(reduce);
    let img = jpeg2k::Image::from_bytes_with(bytes, params).map_err(|e| {
        windows::core::Error::new(WINCODEC_ERR_BADIMAGE, format!("jpeg2k decode: {e:?}"))
    })?;
    let dyn_img: image::DynamicImage = (&img).try_into().map_err(|e| {
        windows::core::Error::new(WINCODEC_ERR_BADIMAGE, format!("to_image: {e:?}"))
    })?;
    let rgba = dyn_img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(DecodedResult {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

impl IWICBitmapDecoder_Impl for JP2WICBitmapDecoder_Impl {
    fn QueryCapability(&self, _pistream: Option<&IStream>) -> windows::core::Result<u32> {
        log::trace!("QueryCapability");
        Ok((WICBitmapDecoderCapabilityCanDecodeSomeImages.0
            | WICBitmapDecoderCapabilityCanDecodeAllImages.0) as u32)
    }

    fn Initialize(
        &self,
        pistream: Option<&IStream>,
        _cacheoptions: WICDecodeOptions,
    ) -> windows::core::Result<()> {
        log::trace!("JP2WICBitmapDecoder::Initialize");
        let stream = pistream.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
        let bytes = read_stream_to_vec(stream)?;
        let decoded = decode_jp2_to_rgba(&bytes)?;
        self.decoded.replace(Some(decoded));
        Ok(())
    }

    fn GetContainerFormat(&self) -> windows::core::Result<GUID> {
        log::trace!("JP2WICBitmapDecoder::GetContainerFormat");
        Ok(JP2WICBitmapDecoder::CONTAINER_ID)
    }

    fn GetDecoderInfo(&self) -> windows::core::Result<IWICBitmapDecoderInfo> {
        log::trace!("JP2WICBitmapDecoder::GetDecoderInfo");
        unsafe {
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let component_info = factory.CreateComponentInfo(&JP2WICBitmapDecoder::CLSID)?;
            component_info.cast()
        }
    }

    fn CopyPalette(&self, _pipalette: Option<&IWICPalette>) -> windows::core::Result<()> {
        WINCODEC_ERR_PALETTEUNAVAILABLE.ok()
    }

    fn GetMetadataQueryReader(&self) -> windows::core::Result<IWICMetadataQueryReader> {
        Err(WINCODEC_ERR_UNSUPPORTEDOPERATION.into())
    }

    fn GetPreview(&self) -> windows::core::Result<IWICBitmapSource> {
        Err(WINCODEC_ERR_UNSUPPORTEDOPERATION.into())
    }

    fn GetColorContexts(
        &self,
        _ccount: u32,
        _ppicolorcontexts: *mut Option<IWICColorContext>,
        pcactualcount: *mut u32,
    ) -> windows::core::Result<()> {
        // We assume sRGB and don't carry an embedded ICC profile.
        unsafe {
            if !pcactualcount.is_null() {
                *pcactualcount = 0;
            }
        }
        Ok(())
    }

    fn GetThumbnail(&self) -> windows::core::Result<IWICBitmapSource> {
        Err(WINCODEC_ERR_CODECNOTHUMBNAIL.into())
    }

    fn GetFrameCount(&self) -> windows::core::Result<u32> {
        let decoded = self.decoded.borrow();
        if decoded.is_some() { Ok(1) } else { Err(WINCODEC_ERR_NOTINITIALIZED.into()) }
    }

    fn GetFrame(&self, index: u32) -> windows::core::Result<IWICBitmapFrameDecode> {
        if index != 0 {
            return Err(WINCODEC_ERR_FRAMEMISSING.into());
        }
        let decoded_ref = self.decoded.borrow();
        let decoded = decoded_ref
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(WINCODEC_ERR_NOTINITIALIZED))?;

        let frame = JP2WICBitmapFrameDecode {
            rgba: decoded.rgba.clone(),
            width: decoded.width,
            height: decoded.height,
        };
        Ok(frame.into())
    }
}

#[implement(Windows::Win32::Graphics::Imaging::IWICBitmapFrameDecode)]
pub struct JP2WICBitmapFrameDecode {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

#[allow(non_snake_case)]
#[allow(clippy::missing_safety_doc)]
impl IWICBitmapSource_Impl for JP2WICBitmapFrameDecode_Impl {
    fn GetSize(&self, puiwidth: *mut u32, puiheight: *mut u32) -> windows::core::Result<()> {
        unsafe {
            *puiwidth = self.width;
            *puiheight = self.height;
        }
        Ok(())
    }

    fn GetPixelFormat(&self) -> windows::core::Result<GUID> {
        Ok(GUID_WICPixelFormat32bppRGBA)
    }

    fn GetResolution(&self, pdpix: *mut f64, pdpiy: *mut f64) -> windows::core::Result<()> {
        unsafe {
            *pdpix = 96f64;
            *pdpiy = 96f64;
        }
        Ok(())
    }

    fn CopyPalette(&self, _pipalette: Option<&IWICPalette>) -> windows::core::Result<()> {
        WINCODEC_ERR_PALETTEUNAVAILABLE.ok()
    }

    fn CopyPixels(
        &self,
        prc: *const WICRect,
        cbstride: u32,
        _cbbuffersize: u32,
        pbbuffer: *mut u8,
    ) -> windows::core::Result<()> {
        log::trace!("JP2WICBitmapFrameDecode::CopyPixels");

        let prc_owned = WICRect {
            X: 0,
            Y: 0,
            Width: self.width as i32,
            Height: self.height as i32,
        };
        let prc_ref = unsafe { prc.as_ref() }.unwrap_or(&prc_owned);

        if prc_ref.X < 0
            || prc_ref.Y < 0
            || prc_ref.Width < 0
            || prc_ref.Height < 0
            || (prc_ref.X + prc_ref.Width) as u32 > self.width
            || (prc_ref.Y + prc_ref.Height) as u32 > self.height
        {
            return Err(E_INVALIDARG.into());
        }

        const BPP: usize = 4; // 32bppRGBA = 4 bytes per pixel
        let src_stride = self.width as usize * BPP;
        let dst_stride = cbstride as usize;
        let copy_bytes = prc_ref.Width as usize * BPP;

        for y in 0..prc_ref.Height {
            let src_off =
                (prc_ref.Y + y) as usize * src_stride + prc_ref.X as usize * BPP;
            let dst_off = y as usize * dst_stride;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.rgba.as_ptr().add(src_off),
                    pbbuffer.add(dst_off),
                    copy_bytes,
                );
            }
        }

        Ok(())
    }
}

impl IWICBitmapFrameDecode_Impl for JP2WICBitmapFrameDecode_Impl {
    fn GetMetadataQueryReader(&self) -> windows::core::Result<IWICMetadataQueryReader> {
        Err(WINCODEC_ERR_UNSUPPORTEDOPERATION.into())
    }

    fn GetColorContexts(
        &self,
        _ccount: u32,
        _ppicolorcontexts: *mut Option<IWICColorContext>,
        pcactualcount: *mut u32,
    ) -> windows::core::Result<()> {
        unsafe {
            if !pcactualcount.is_null() {
                *pcactualcount = 0;
            }
        }
        Ok(())
    }

    fn GetThumbnail(&self) -> windows::core::Result<IWICBitmapSource> {
        Err(WINCODEC_ERR_CODECNOTHUMBNAIL.into())
    }
}
