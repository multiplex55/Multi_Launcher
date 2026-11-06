//! Low-level Windows Graphics Capture and D3D11 helpers shared by capture
//! owners. This module does not own a capture item, session, frame pool, or
//! worker lifetime.

pub(crate) fn copy_mapped_bgra_to_rgba(
    source: &[u8],
    row_pitch: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, &'static str> {
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or("captured crop row size overflowed")?;
    if width == 0 || height == 0 || row_pitch < row_bytes {
        return Err("captured crop has an invalid Direct3D row pitch");
    }
    let last_row = usize::try_from(height - 1)
        .map_err(|_| "invalid crop height")?
        .checked_mul(row_pitch)
        .ok_or("captured crop mapped offset overflowed")?;
    let source_bytes = last_row
        .checked_add(row_bytes)
        .ok_or("captured crop mapped size overflowed")?;
    if source_bytes > isize::MAX as usize || source.len() < source_bytes {
        return Err("captured crop mapped data is shorter than its logical rows");
    }
    let output_bytes = row_bytes
        .checked_mul(usize::try_from(height).map_err(|_| "invalid crop height")?)
        .ok_or("captured crop byte size overflowed")?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(output_bytes)
        .map_err(|_| "could not allocate captured crop pixels")?;
    rgba.resize(output_bytes, 0);
    for row in 0..height as usize {
        let row_start = row * row_pitch;
        let bgra_row = &source[row_start..row_start + row_bytes];
        let rgba_row = &mut rgba[row * row_bytes..(row + 1) * row_bytes];
        copy_bgra_row_to_rgba(bgra_row, rgba_row)?;
    }
    Ok(rgba)
}

fn copy_bgra_row_to_rgba(source: &[u8], target: &mut [u8]) -> Result<(), &'static str> {
    if source.is_empty() || source.len() != target.len() || source.len() % 4 != 0 {
        return Err("captured crop row has an invalid byte length");
    }
    for (source, target) in source
        .as_chunks::<4>()
        .0
        .iter()
        .zip(target.as_chunks_mut::<4>().0.iter_mut())
    {
        target[0] = source[2];
        target[1] = source[1];
        target[2] = source[0];
        target[3] = source[3];
    }
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::{copy_bgra_row_to_rgba, copy_mapped_bgra_to_rgba};
    use windows::Graphics::Capture::{
        Direct3D11CaptureFramePool, IDirect3D11CaptureFramePoolStatics2,
        IGraphicsCaptureSessionStatics,
    };
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
        D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
    use windows::core::{Interface, Type, factory};

    pub(crate) fn create_free_threaded_frame_pool(
        device: &IDirect3DDevice,
        pixel_format: DirectXPixelFormat,
        number_of_buffers: i32,
        size: SizeInt32,
    ) -> windows::core::Result<Direct3D11CaptureFramePool> {
        let statics = factory::<Direct3D11CaptureFramePool, IDirect3D11CaptureFramePoolStatics2>()?;
        // SAFETY: the typed factory and borrowed device stay alive for this
        // synchronous ABI call, which writes the declared interface output.
        unsafe {
            let mut result = std::ptr::null_mut();
            (statics.vtable().CreateFreeThreaded)(
                statics.as_raw(),
                device.as_raw(),
                pixel_format,
                number_of_buffers,
                size,
                &mut result,
            )
            .and_then(|| Type::from_abi(result))
        }
    }

    pub(crate) fn graphics_capture_is_supported() -> windows::core::Result<bool> {
        let statics = factory::<
            windows::Graphics::Capture::GraphicsCaptureSession,
            IGraphicsCaptureSessionStatics,
        >()?;
        // SAFETY: the scoped typed factory remains live for this synchronous
        // call, and the ABI writes the declared boolean result.
        unsafe {
            let mut result = false;
            (statics.vtable().IsSupported)(statics.as_raw(), &mut result).map(|| result)
        }
    }

    pub(crate) fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
        let mut device = None;
        let mut context = None;
        // SAFETY: D3D11CreateDevice initializes the optional COM outputs on
        // success; each output is checked before it is returned to its owner.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(|error| format!("could not create a BGRA Direct3D device: {error}"))?;
        Ok((
            device.ok_or_else(|| "Direct3D did not return a device".to_owned())?,
            context.ok_or_else(|| "Direct3D did not return an immediate context".to_owned())?,
        ))
    }

    pub(crate) fn create_winrt_device(device: &ID3D11Device) -> Result<IDirect3DDevice, String> {
        let dxgi: IDXGIDevice = device
            .cast()
            .map_err(|error| format!("could not query the Direct3D DXGI device: {error}"))?;
        let inspectable =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(|error| {
                format!("could not create the Windows Graphics Capture device: {error}")
            })?;
        inspectable
            .cast()
            .map_err(|error| format!("could not query the Windows capture device: {error}"))
    }

    pub(crate) fn create_staging_texture(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    ) -> Result<ID3D11Texture2D, String> {
        if width == 0 || height == 0 {
            return Err("crop staging texture dimensions must be positive".into());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: the descriptor describes one checked, single-sample staging
        // texture and the optional output is validated before ownership moves.
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .map_err(|error| format!("could not create crop staging texture: {error}"))?;
        texture.ok_or_else(|| "Direct3D did not return a crop staging texture".into())
    }

    struct MappedSubresource<'a> {
        context: &'a ID3D11DeviceContext,
        resource: ID3D11Resource,
        data: *const u8,
        row_pitch: usize,
    }

    impl Drop for MappedSubresource<'_> {
        fn drop(&mut self) {
            // SAFETY: this guard is constructed only after a successful Map
            // for this exact resource and context, and drops exactly once.
            unsafe { self.context.Unmap(&self.resource, 0) };
        }
    }

    pub(crate) fn read_staging_rgba(
        context: &ID3D11DeviceContext,
        texture: &ID3D11Texture2D,
        expected_width: u32,
        expected_height: u32,
    ) -> Result<Vec<u8>, String> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: GetDesc writes the full descriptor for the live texture.
        unsafe { texture.GetDesc(&mut desc) };
        if desc.Width == 0
            || desc.Height == 0
            || desc.Width != expected_width
            || desc.Height != expected_height
        {
            return Err("crop staging texture dimensions changed unexpectedly".into());
        }
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
            || desc.SampleDesc.Count != 1
            || desc.Usage != D3D11_USAGE_STAGING
            || desc.CPUAccessFlags & D3D11_CPU_ACCESS_READ.0 as u32 == 0
        {
            return Err("crop staging texture format or CPU access is unsupported".into());
        }
        let row_bytes = usize::try_from(desc.Width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| "captured crop row size overflowed".to_owned())?;
        let height = usize::try_from(desc.Height).map_err(|_| "invalid crop height".to_owned())?;
        let output_bytes = row_bytes
            .checked_mul(height)
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .ok_or_else(|| "captured crop byte size overflowed".to_owned())?;
        let resource: ID3D11Resource = texture
            .cast()
            .map_err(|error| format!("could not query crop staging resource: {error}"))?;
        let mut mapped_subresource = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the resource belongs to this device and is a CPU-readable
        // staging texture. Its mapping is always paired with Unmap below.
        unsafe {
            context.Map(
                &resource,
                0,
                D3D11_MAP_READ,
                0,
                Some(&mut mapped_subresource),
            )
        }
        .map_err(|error| format!("could not map captured crop: {error}"))?;
        let mapped = MappedSubresource {
            context,
            resource,
            data: mapped_subresource.pData.cast(),
            row_pitch: mapped_subresource.RowPitch as usize,
        };
        if mapped.data.is_null() {
            return Err("captured crop has a null Direct3D mapping".into());
        }
        let last_row_offset = (height - 1)
            .checked_mul(mapped.row_pitch)
            .ok_or_else(|| "captured crop mapped offset overflowed".to_owned())?;
        let mapped_end = last_row_offset
            .checked_add(row_bytes)
            .filter(|end| *end <= isize::MAX as usize)
            .ok_or_else(|| "captured crop mapped size overflowed".to_owned())?;
        if mapped.row_pitch < row_bytes || (mapped.data as usize).checked_add(mapped_end).is_none()
        {
            return Err("captured crop has an invalid Direct3D row pitch".into());
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(output_bytes)
            .map_err(|_| "could not allocate captured crop pixels".to_owned())?;
        rgba.resize(output_bytes, 0);
        for row in 0..height {
            let offset = row
                .checked_mul(mapped.row_pitch)
                .ok_or_else(|| "captured crop mapped offset overflowed".to_owned())?;
            let output_start = row
                .checked_mul(row_bytes)
                .ok_or_else(|| "captured crop output offset overflowed".to_owned())?;
            // SAFETY: texture dimensions and pitch were checked above; this
            // slice covers only the logical row bytes, not driver padding.
            let bgra_row =
                unsafe { std::slice::from_raw_parts(mapped.data.add(offset), row_bytes) };
            let rgba_row = &mut rgba[output_start..output_start + row_bytes];
            copy_bgra_row_to_rgba(bgra_row, rgba_row).map_err(str::to_owned)?;
        }
        drop(mapped);
        Ok(rgba)
    }
}

#[cfg(windows)]
pub(crate) use native::{
    create_device, create_free_threaded_frame_pool, create_staging_texture, create_winrt_device,
    graphics_capture_is_supported, read_staging_rgba,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_windows_mapped_bgra_conversion_honors_row_pitch_and_alpha() {
        let source = [
            3, 2, 1, 4, 30, 20, 10, 40, 99, 99, 99, 99, 7, 6, 5, 8, 70, 60, 50, 80, 99, 99, 99, 99,
        ];
        assert_eq!(
            copy_mapped_bgra_to_rgba(&source, 12, 2, 2).unwrap(),
            [1, 2, 3, 4, 10, 20, 30, 40, 5, 6, 7, 8, 50, 60, 70, 80]
        );
        assert_eq!(
            copy_mapped_bgra_to_rgba(&source[..20], 12, 2, 2).unwrap(),
            [1, 2, 3, 4, 10, 20, 30, 40, 5, 6, 7, 8, 50, 60, 70, 80]
        );
        assert!(copy_mapped_bgra_to_rgba(&source, 7, 2, 2).is_err());
        assert!(copy_mapped_bgra_to_rgba(&source[..19], 12, 2, 2).is_err());
    }
}
