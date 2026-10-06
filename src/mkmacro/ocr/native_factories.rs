//! Scoped native activation factories for the OCR adapter. Callers initialize
//! WinRT before these calls and drop returned temporary interfaces before their
//! runtime guard. Generated static projections cache factories beyond that
//! guard; RoUninitialize can unload their DLLs and close their RPC connections:
//! https://learn.microsoft.com/en-us/windows/win32/api/roapi/nf-roapi-rouninitialize
use windows::{
    Foundation::Collections::IVectorView,
    Globalization::{ILanguageFactory, Language},
    Graphics::Imaging::{
        BitmapAlphaMode, BitmapPixelFormat, ISoftwareBitmapStatics, SoftwareBitmap,
    },
    Media::Ocr::{IOcrEngineStatics, OcrEngine},
    Security::Cryptography::{CryptographicBuffer, ICryptographicBufferStatics},
    Storage::Streams::IBuffer,
    System::UserProfile::{GlobalizationPreferences, IGlobalizationPreferencesStatics},
    Win32::Foundation::E_INVALIDARG,
    core::{HSTRING, Interface, Type, factory},
};

pub(super) fn available_languages() -> windows::core::Result<IVectorView<Language>> {
    let factory = factory::<OcrEngine, IOcrEngineStatics>()?;
    // SAFETY: the live factory's typed ABI initializes the corresponding owned
    // interface pointer. from_abi validates/transfers ownership on success.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().AvailableRecognizerLanguages)(factory.as_raw(), &mut result)
            .and_then(|| Type::from_abi(result))
    }
}

pub(super) fn max_image_dimension() -> windows::core::Result<u32> {
    let factory = factory::<OcrEngine, IOcrEngineStatics>()?;
    // SAFETY: the live factory writes the declared scalar output on success.
    unsafe {
        let mut result = 0;
        (factory.vtable().MaxImageDimension)(factory.as_raw(), &mut result).map(|| result)
    }
}

pub(super) fn language_supported(language: &Language) -> windows::core::Result<bool> {
    let factory = factory::<OcrEngine, IOcrEngineStatics>()?;
    // SAFETY: factory and borrowed language remain alive for this call; the
    // typed ABI writes the declared boolean output on success.
    unsafe {
        let mut result = false;
        (factory.vtable().IsLanguageSupported)(factory.as_raw(), language.as_raw(), &mut result)
            .map(|| result)
    }
}

pub(super) fn language_engine(language: &Language) -> windows::core::Result<OcrEngine> {
    let factory = factory::<OcrEngine, IOcrEngineStatics>()?;
    // SAFETY: both borrowed interfaces are live; the typed ABI returns an owned
    // OcrEngine pointer, validated/transferred by from_abi on success.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().TryCreateFromLanguage)(factory.as_raw(), language.as_raw(), &mut result)
            .and_then(|| Type::from_abi(result))
    }
}

pub(super) fn profile_engine() -> windows::core::Result<OcrEngine> {
    let factory = factory::<OcrEngine, IOcrEngineStatics>()?;
    // SAFETY: the live factory returns an owned OcrEngine pointer;
    // from_abi validates/transfers ownership on success.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().TryCreateFromUserProfileLanguages)(factory.as_raw(), &mut result)
            .and_then(|| Type::from_abi(result))
    }
}

pub(super) fn language(tag: &HSTRING) -> windows::core::Result<Language> {
    let factory = factory::<Language, ILanguageFactory>()?;
    // SAFETY: the HSTRING's ABI handle is borrowed for the call while tag stays
    // alive. The returned owned Language pointer is validated by from_abi.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().CreateLanguage)(
            factory.as_raw(),
            std::mem::transmute_copy(tag),
            &mut result,
        )
        .and_then(|| Type::from_abi(result))
    }
}

pub(crate) enum ProfileLanguagesError {
    Activate(windows::core::Error),
    Read(windows::core::Error),
}

impl ProfileLanguagesError {
    pub(crate) fn operation(&self) -> &'static str {
        match self {
            Self::Activate(_) => "activate profile preferences",
            Self::Read(_) => "read profile languages",
        }
    }
}

impl std::fmt::Display for ProfileLanguagesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Activate(error) | Self::Read(error) => error.fmt(formatter),
        }
    }
}

pub(crate) fn profile_languages() -> Result<IVectorView<HSTRING>, ProfileLanguagesError> {
    let factory = factory::<GlobalizationPreferences, IGlobalizationPreferencesStatics>()
        .map_err(ProfileLanguagesError::Activate)?;
    // SAFETY: the live factory returns an owned IVectorView pointer;
    // from_abi validates/transfers ownership on success.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().Languages)(factory.as_raw(), &mut result)
            .and_then(|| Type::from_abi(result))
    }
    .map_err(ProfileLanguagesError::Read)
}

pub(super) fn pixel_buffer(pixels: &[u8]) -> windows::core::Result<IBuffer> {
    let length = u32::try_from(pixels.len())
        .map_err(|_| windows::core::Error::from_hresult(E_INVALIDARG))?;
    let factory = factory::<CryptographicBuffer, ICryptographicBufferStatics>()?;
    // SAFETY: pixels supplies length readable bytes for this synchronous call;
    // the ABI copies them into an owned IBuffer validated by from_abi.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().CreateFromByteArray)(
            factory.as_raw(),
            length,
            pixels.as_ptr(),
            &mut result,
        )
        .and_then(|| Type::from_abi(result))
    }
}

pub(super) fn bitmap(
    buffer: &IBuffer,
    format: BitmapPixelFormat,
    width: i32,
    height: i32,
    alpha: BitmapAlphaMode,
) -> windows::core::Result<SoftwareBitmap> {
    let factory = factory::<SoftwareBitmap, ISoftwareBitmapStatics>()?;
    // SAFETY: buffer remains alive; the typed ABI copies it into an owned
    // SoftwareBitmap interface validated/transferred by from_abi.
    unsafe {
        let mut result = std::ptr::null_mut();
        (factory.vtable().CreateCopyWithAlphaFromBuffer)(
            factory.as_raw(),
            buffer.as_raw(),
            format,
            width,
            height,
            alpha,
            &mut result,
        )
        .and_then(|| Type::from_abi(result))
    }
}
