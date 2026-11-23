//! Whole-window acrylic that survives loss of focus.
//!
//! DWM-managed acrylic either drops to a flat tint as soon as the window is
//! unfocused (Win10 accent policy) or ignores the tint colour entirely (Win11
//! system backdrop). Drawing the blur into the window's own composition tree
//! instead — a visual brushed with `CreateHostBackdropBrush`, which samples
//! and blurs everything behind the window — keeps the material constant,
//! focused or not, and untinted.

use std::sync::{LazyLock, Mutex};

use windows::Graphics::Effects::{IGraphicsEffect_Impl, IGraphicsEffectSource_Impl};
use windows::Win32::System::WinRT::Graphics::Direct2D::IGraphicsEffectD2D1Interop_Impl;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows::{
    Foundation::PropertyValue,
    Graphics::Effects::{IGraphicsEffect, IGraphicsEffectSource},
    System::DispatcherQueueController,
    UI::Composition::{
        CompositionEffectBrush, CompositionEffectSourceParameter, Compositor, ContainerVisual,
        Desktop::DesktopWindowTarget, SpriteVisual,
    },
    Win32::Foundation::{E_FAIL, E_INVALIDARG, HWND},
    Win32::Graphics::Direct2D::Common::D2D1_BORDER_MODE_HARD,
    Win32::Graphics::Direct2D::{CLSID_D2D1GaussianBlur, D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED},
    Win32::Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
    Win32::System::WinRT::Composition::ICompositorDesktopInterop,
    Win32::System::WinRT::Graphics::Direct2D::{
        GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
        IGraphicsEffectD2D1Interop,
    },
    core::{GUID, HSTRING, Interface, PCWSTR, implement},
};
use windows_numerics::Vector2;

/// Gaussian blur σ (DIPs) for the backdrop; the raw
/// `CompositionHostBackdropBrush` cannot be tuned at all. 30 matches WinUI's
/// desktop acrylic.
const BLUR_SIGMA: f32 = 30.0;

enum Backdrop {
    Composition {
        // Held to keep the visual tree connected to the window.
        _target: DesktopWindowTarget,
        root: ContainerVisual,
        blur: SpriteVisual,
    },
    /// Windows too old for host-backdrop brushes: window-vibrancy's DWM
    /// acrylic, which loses the blur whenever the window is unfocused.
    Dwm,
}

static ACTIVE: LazyLock<Mutex<Option<Backdrop>>> = LazyLock::new(|| Mutex::new(None));

/// The thread that built the composition objects; they are apartment-bound
/// to it, so every later call must come from it too.
static COMPOSITION_THREAD: std::sync::OnceLock<std::thread::ThreadId> =
    std::sync::OnceLock::new();

/// The compositor refuses to activate without a dispatcher queue on the
/// calling thread; tao's main thread never creates one. The controller must
/// outlive every compositor, so it is held for the process lifetime.
static DISPATCHER_QUEUE: LazyLock<Mutex<Option<DispatcherQueueController>>> =
    LazyLock::new(|| Mutex::new(None));

fn ensure_dispatcher_queue() -> windows::core::Result<()> {
    let _ = COMPOSITION_THREAD.set(std::thread::current().id());
    let Ok(mut queue) = DISPATCHER_QUEUE.lock() else {
        return Err(windows::core::Error::from_hresult(E_FAIL));
    };
    if queue.is_some() {
        return Ok(());
    }

    unsafe {
        // The main thread already owns its COM apartment; the queue only
        // needs to exist.
        let options = DispatcherQueueOptions {
            dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        };
        *queue = Some(CreateDispatcherQueueController(options)?);
    }
    Ok(())
}

fn enable(hwnd: HWND) -> windows::core::Result<Backdrop> {
    ensure_dispatcher_queue()?;

    unsafe {
        let compositor = Compositor::new()?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;

        // Not topmost: the backdrop must render behind the webviews, never
        // over them.
        let target = interop.CreateDesktopWindowTarget(hwnd, false)?;
        let root = compositor.CreateContainerVisual()?;

        let blur = compositor.CreateSpriteVisual()?;
        blur.SetBrush(&host_blur_brush(&compositor)?)?;
        root.Children()?.InsertAtTop(&blur)?;
        target.SetRoot(&root)?;

        Ok(Backdrop::Composition {
            _target: target,
            root,
            blur,
        })
    }
}

/// The backdrop brush: a Gaussian blur over the host backdrop, so the blur
/// strength is ours rather than the fixed amount `CreateHostBackdropBrush`
/// bakes in.
fn host_blur_brush(compositor: &Compositor) -> windows::core::Result<CompositionEffectBrush> {
    let source = CompositionEffectSourceParameter::Create(&HSTRING::from("Backdrop"))?;
    let source: IGraphicsEffectSource = source.cast()?;
    let effect: IGraphicsEffect = BlurEffect {
        sigma: BLUR_SIGMA,
        source,
    }
    .into();

    let factory = compositor.CreateEffectFactory(&effect)?;
    let brush = factory.CreateBrush()?;
    let host = compositor.CreateHostBackdropBrush()?;
    brush.SetSourceParameter(&HSTRING::from("Backdrop"), &host)?;

    Ok(brush)
}

/// Win2D-style description of a D2D Gaussian blur. The composition engine
/// instantiates the D2D effect from the CLSID and reads properties and
/// sources back out through the interop interface.
#[implement(IGraphicsEffectSource, IGraphicsEffect, IGraphicsEffectD2D1Interop)]
struct BlurEffect {
    sigma: f32,
    source: IGraphicsEffectSource,
}

impl IGraphicsEffectSource_Impl for BlurEffect_Impl {}

impl IGraphicsEffect_Impl for BlurEffect_Impl {
    fn Name(&self) -> windows::core::Result<HSTRING> {
        Ok(HSTRING::from("Blur"))
    }

    fn SetName(&self, _name: &HSTRING) -> windows::core::Result<()> {
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for BlurEffect_Impl {
    fn GetEffectId(&self) -> windows::core::Result<GUID> {
        Ok(CLSID_D2D1GaussianBlur)
    }

    fn GetNamedPropertyMapping(
        &self,
        name: &PCWSTR,
        index: *mut u32,
        mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> windows::core::Result<()> {
        unsafe {
            match name.to_string().as_deref() {
                Ok("BlurAmount") => {
                    *index = 0;
                    *mapping = GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT;
                    Ok(())
                }
                _ => Err(windows::core::Error::from_hresult(E_INVALIDARG)),
            }
        }
    }

    fn GetPropertyCount(&self) -> windows::core::Result<u32> {
        Ok(3)
    }

    fn GetProperty(
        &self,
        index: u32,
    ) -> windows::core::Result<windows::Foundation::IPropertyValue> {
        match index {
            0 => PropertyValue::CreateSingle(self.sigma)?.cast(),
            1 => PropertyValue::CreateUInt32(D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED.0 as u32)?
                .cast(),
            2 => PropertyValue::CreateUInt32(D2D1_BORDER_MODE_HARD.0 as u32)?.cast(),
            _ => Err(windows::core::Error::from_hresult(E_INVALIDARG)),
        }
    }

    fn GetSource(&self, index: u32) -> windows::core::Result<IGraphicsEffectSource> {
        match index {
            0 => Ok(self.source.clone()),
            _ => Err(windows::core::Error::from_hresult(E_INVALIDARG)),
        }
    }

    fn GetSourceCount(&self) -> windows::core::Result<u32> {
        Ok(1)
    }
}

impl Backdrop {
    fn resize(&self, size: Vector2) {
        let Self::Composition { root, blur, .. } = self else {
            return;
        };
        let _ = root.SetSize(size);
        let _ = blur.SetSize(size);
    }

    fn detach(mut self) {
        // Clear the visuals before the target is released so the backdrop
        // disappears immediately rather than at compositor collection.
        if let Self::Composition { root, .. } = &mut self {
            let _ = root.Children().and_then(|children| children.RemoveAll());
        }
    }
}

/// Applies or removes the whole-window backdrop. The tint only matters for
/// the pre-HostBackdrop DWM fallback; the composition material is untinted.
pub(crate) fn apply(window: &tauri::Window, enabled: bool, tint: Option<(u8, u8, u8, u8)>) {
    if let Some(thread) = COMPOSITION_THREAD.get() {
        debug_assert_eq!(*thread, std::thread::current().id());
    }

    let Ok(mut active) = ACTIVE.lock() else {
        return;
    };

    if enabled {
        if active.is_none() {
            *active = build(window, tint);
        }
        if let Some(backdrop) = active.as_ref()
            && let Ok(size) = window.inner_size()
        {
            backdrop.resize(physical(size));
        }
    } else if let Some(backdrop) = active.take() {
        if let Ok(hwnd) = window.hwnd() {
            set_host_backdrop(hwnd, false);
        }
        if matches!(backdrop, Backdrop::Dwm) {
            let _ = window_vibrancy::clear_acrylic(window);
        }
        backdrop.detach();
    }
}

fn build(window: &tauri::Window, tint: Option<(u8, u8, u8, u8)>) -> Option<Backdrop> {
    let Ok(hwnd) = window.hwnd() else {
        return None;
    };
    set_host_backdrop(hwnd, true);

    match enable(hwnd) {
        Ok(backdrop) => Some(backdrop),
        Err(e) => {
            eprintln!("[mesa] host backdrop unavailable ({e}), falling back to DWM acrylic");
            let _ = window_vibrancy::apply_acrylic(window, tint);
            Some(Backdrop::Dwm)
        }
    }
}

/// Matches the backdrop to the window on resize.
pub(crate) fn sync_size(window: &tauri::Window) {
    let Ok(active) = ACTIVE.lock() else { return };
    let Some(backdrop) = active.as_ref() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    backdrop.resize(physical(size));
}

fn set_host_backdrop(hwnd: HWND, enabled: bool) {
    // HostBackdropBrush only samples the screen once this attribute is set.
    let value = windows::core::BOOL::from(enabled);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH,
            &value as *const _ as _,
            std::mem::size_of::<windows::core::BOOL>() as u32,
        );
    }
}

fn physical(size: tauri::PhysicalSize<u32>) -> Vector2 {
    Vector2 {
        X: size.width as f32,
        Y: size.height as f32,
    }
}
