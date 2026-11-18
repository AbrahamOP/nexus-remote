//! Minimal native Windows ActiveX hosting. No browser or managed runtime is involved.
//! ATL implements the OLE container; the RDP client is the copy installed with Windows.
use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::OnceLock;

type Hwnd = isize;
type Hresult = i32;
type AtlCreate = unsafe extern "system" fn(
    *const u16,
    Hwnd,
    *mut c_void,
    *mut *mut c_void,
    *mut *mut c_void,
    *const Guid,
    *mut c_void,
) -> Hresult;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
const IID_NULL: Guid = Guid {
    a: 0,
    b: 0,
    c: 0,
    d: [0; 8],
};
const IID_IDISPATCH: Guid = Guid {
    a: 0x00020400,
    b: 0,
    c: 0,
    d: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

#[link(name = "ole32")]
unsafe extern "system" {
    fn OleInitialize(reserved: *mut c_void) -> Hresult;
    fn OleUninitialize();
}
#[link(name = "oleaut32")]
unsafe extern "system" {
    fn SysAllocStringLen(text: *const u16, length: u32) -> *mut u16;
    fn SysStringLen(text: *const u16) -> u32;
    fn SysFreeString(text: *mut u16);
    fn VariantClear(value: *mut Variant) -> Hresult;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: isize,
        instance: isize,
        parameter: *mut c_void,
    ) -> Hwnd;
    fn DestroyWindow(window: Hwnd) -> i32;
    fn MoveWindow(window: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    fn ShowWindow(window: Hwnd, command: i32) -> i32;
    fn GetFocus() -> Hwnd;
    fn SetFocus(window: Hwnd) -> Hwnd;
    fn GetForegroundWindow() -> Hwnd;
    fn IsChild(parent: Hwnd, window: Hwnd) -> i32;
    fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemDirectoryW(buffer: *mut u16, length: u32) -> u32;
    fn LoadLibraryW(name: *const u16) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *mut c_void;
    fn GetLastError() -> u32;
}

pub(super) struct OleGuard;
impl OleGuard {
    pub(super) fn new() -> Result<Self, String> {
        let result = unsafe { OleInitialize(null_mut()) };
        if result < 0 {
            return Err(format!(
                "Initialisation RDP/OLE impossible (0x{:08X}). Le client nécessite le thread principal Windows en mode STA.",
                result as u32
            ));
        }
        Ok(Self)
    }
}
impl Drop for OleGuard {
    fn drop(&mut self) {
        unsafe { OleUninitialize() }
    }
}

struct AtlApi {
    create: AtlCreate,
}
fn atl_api() -> Result<&'static AtlApi, String> {
    static ATL: OnceLock<Result<AtlApi, String>> = OnceLock::new();
    ATL.get_or_init(|| {
        // Keep the module loaded for the process lifetime: its window procedures remain registered.
        let mut directory = vec![0u16; 32768];
        let count =
            unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
        if count == 0 || count >= directory.len() {
            return Err("Impossible de trouver le dossier système Windows.".into());
        }
        directory.truncate(count);
        directory.extend("\\atl.dll\0".encode_utf16());
        let module = unsafe { LoadLibraryW(directory.as_ptr()) };
        if module == 0 {
            return Err(format!(
                "Le composant Windows atl.dll est indisponible ({}).",
                unsafe { GetLastError() }
            ));
        }
        let create = unsafe { GetProcAddress(module, c"AtlAxCreateControlEx".as_ptr().cast()) };
        let init = unsafe { GetProcAddress(module, c"AtlAxWinInit".as_ptr().cast()) };
        if create.is_null() || init.is_null() {
            return Err("Le composant Windows ATL ne propose pas l’hébergement ActiveX.".into());
        }
        let initialize: unsafe extern "system" fn() -> i32 = unsafe { std::mem::transmute(init) };
        if unsafe { initialize() } == 0 {
            return Err("L’hébergement du Bureau à distance n’a pas pu être initialisé.".into());
        }
        Ok(AtlApi {
            create: unsafe { std::mem::transmute(create) },
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

#[repr(C)]
struct UnknownVtable {
    query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}
#[repr(C)]
struct DispatchVtable {
    unknown: UnknownVtable,
    type_info_count: usize,
    type_info: usize,
    ids: unsafe extern "system" fn(
        *mut c_void,
        *const Guid,
        *mut *mut u16,
        u32,
        u32,
        *mut i32,
    ) -> Hresult,
    invoke: unsafe extern "system" fn(
        *mut c_void,
        i32,
        *const Guid,
        u32,
        u16,
        *mut DispatchParams,
        *mut Variant,
        *mut ExceptionInfo,
        *mut u32,
    ) -> Hresult,
}
#[repr(C)]
struct DispatchParams {
    args: *mut Variant,
    named: *mut i32,
    count: u32,
    named_count: u32,
}
#[repr(C)]
#[derive(Default)]
struct ExceptionInfo {
    code: u16,
    reserved: u16,
    source: *mut u16,
    description: *mut u16,
    help_file: *mut u16,
    help_context: u32,
    reserved_pointer: *mut c_void,
    deferred: usize,
    scode: i32,
}
impl Drop for ExceptionInfo {
    fn drop(&mut self) {
        unsafe {
            SysFreeString(self.source);
            SysFreeString(self.description);
            SysFreeString(self.help_file);
        }
    }
}

/// Windows VARIANT is 24 bytes on x64, including its 16-byte record union.
#[repr(C)]
pub(super) struct Variant {
    vt: u16,
    reserved: [u16; 3],
    data: [u64; 2],
}
impl Variant {
    fn empty() -> Self {
        Self {
            vt: 0,
            reserved: [0; 3],
            data: [0; 2],
        }
    }
    pub(super) fn int(value: i32) -> Self {
        Self {
            vt: 3,
            data: [value as u32 as u64, 0],
            ..Self::empty()
        }
    }
    pub(super) fn uint(value: u32) -> Self {
        Self {
            vt: 19,
            data: [value as u64, 0],
            ..Self::empty()
        }
    }
    pub(super) fn boolean(value: bool) -> Self {
        Self {
            vt: 11,
            data: [if value { 0xffff } else { 0 }, 0],
            ..Self::empty()
        }
    }
    pub(super) fn text(value: &str) -> Result<Self, String> {
        let mut encoded: Vec<u16> = value.encode_utf16().collect();
        let bstr = unsafe { SysAllocStringLen(encoded.as_ptr(), encoded.len() as u32) };
        // Text can be a password; do not leave our temporary UTF-16 copy in an allocator buffer.
        for character in &mut encoded {
            unsafe {
                std::ptr::write_volatile(character, 0);
            }
        }
        if bstr.is_null() && !encoded.is_empty() {
            return Err("Mémoire insuffisante pour les paramètres RDP.".into());
        }
        Ok(Self {
            vt: 8,
            data: [bstr as u64, 0],
            ..Self::empty()
        })
    }
    pub(super) fn as_int(&self) -> Result<i32, String> {
        match self.vt {
            2 => Ok(self.data[0] as i16 as i32),
            3 | 19 | 22 | 23 => Ok(self.data[0] as i32),
            11 => Ok((self.data[0] as i16 != 0) as i32),
            _ => Err(format!("Type de réponse RDP inattendu : {}.", self.vt)),
        }
    }
    fn as_text(&self) -> String {
        if self.vt != 8 {
            return String::new();
        }
        unsafe { bstr_text(self.data[0] as *const u16) }
    }
    pub(super) fn dispatch(&self) -> Result<Dispatch, String> {
        if !matches!(self.vt, 9 | 13) || self.data[0] == 0 {
            return Err("Le composant RDP n’a pas renvoyé l’interface demandée.".into());
        }
        unsafe { Dispatch::from_unknown(self.data[0] as *mut c_void) }
    }
}
impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            if self.vt == 8 && self.data[0] != 0 {
                let text = self.data[0] as *mut u16;
                for index in 0..SysStringLen(text) as usize {
                    std::ptr::write_volatile(text.add(index), 0);
                }
            }
            VariantClear(self);
        }
    }
}
unsafe fn bstr_text(value: *const u16) -> String {
    if value.is_null() {
        return String::new();
    }
    String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(value, SysStringLen(value) as usize)
    })
}

struct Unknown(*mut c_void);
impl Drop for Unknown {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let table = *(self.0 as *mut *const UnknownVtable);
                ((*table).release)(self.0);
            }
        }
    }
}
pub(super) struct Dispatch(*mut c_void);
impl Dispatch {
    unsafe fn from_unknown(unknown: *mut c_void) -> Result<Self, String> {
        if unknown.is_null() {
            return Err("Le contrôle RDP Windows n’a pas été créé.".into());
        }
        let mut dispatch = null_mut();
        let table = unsafe { *(unknown as *mut *const UnknownVtable) };
        let hr = unsafe { ((*table).query)(unknown, &IID_IDISPATCH, &mut dispatch) };
        if hr < 0 || dispatch.is_null() {
            return Err(format!("Interface RDP indisponible (0x{:08X}).", hr as u32));
        }
        Ok(Self(dispatch))
    }
    fn invoke(&self, name: &str, flags: u16, args: &mut [Variant]) -> Result<Variant, String> {
        let mut encoded = wide(name);
        let mut name_pointer = encoded.as_mut_ptr();
        let mut id = 0;
        let table = unsafe { *(self.0 as *mut *const DispatchVtable) };
        let hr = unsafe { ((*table).ids)(self.0, &IID_NULL, &mut name_pointer, 1, 0, &mut id) };
        if hr < 0 {
            return Err(format!(
                "Le client RDP ne propose pas {} (0x{:08X}).",
                name, hr as u32
            ));
        }
        // IDispatch passes positional parameters from last to first.
        args.reverse();
        let mut property_put = -3;
        let mut params = DispatchParams {
            args: if args.is_empty() {
                null_mut()
            } else {
                args.as_mut_ptr()
            },
            named: if flags == 4 {
                &mut property_put
            } else {
                null_mut()
            },
            count: args.len() as u32,
            named_count: if flags == 4 { 1 } else { 0 },
        };
        let mut output = Variant::empty();
        let mut exception = ExceptionInfo::default();
        let mut argument_error = 0;
        let hr = unsafe {
            ((*table).invoke)(
                self.0,
                id,
                &IID_NULL,
                0,
                flags,
                &mut params,
                &mut output,
                &mut exception,
                &mut argument_error,
            )
        };
        args.reverse();
        if hr < 0 {
            let description = unsafe { bstr_text(exception.description) };
            return Err(format!(
                "RDP / {} : {} (0x{:08X}).",
                name,
                if description.is_empty() {
                    "opération impossible"
                } else {
                    &description
                },
                hr as u32
            ));
        }
        Ok(output)
    }
    pub(super) fn get(&self, name: &str) -> Result<Variant, String> {
        self.invoke(name, 2, &mut [])
    }
    pub(super) fn put(&self, name: &str, value: Variant) -> Result<(), String> {
        self.invoke(name, 4, &mut [value]).map(|_| ())
    }
    pub(super) fn call(&self, name: &str, args: &mut [Variant]) -> Result<Variant, String> {
        self.invoke(name, 1, args)
    }
    pub(super) fn int(&self, name: &str) -> Result<i32, String> {
        self.get(name)?.as_int()
    }
    pub(super) fn child(&self, name: &str) -> Result<Dispatch, String> {
        self.get(name)?.dispatch()
    }
    pub(super) fn description(&self, reason: i32) -> String {
        self.call(
            "GetErrorDescription",
            &mut [Variant::uint(0), Variant::uint(reason as u32)],
        )
        .map(|value| value.as_text())
        .unwrap_or_default()
    }
}
impl Drop for Dispatch {
    fn drop(&mut self) {
        unsafe {
            let table = *(self.0 as *mut *const UnknownVtable);
            ((*table).release)(self.0);
        }
    }
}

pub(super) struct NativeRdp {
    window: Hwnd,
    parent: Hwnd,
    client: Option<Dispatch>,
    container: Option<Unknown>,
    visible: Cell<bool>,
    rectangle: Cell<(i32, i32, i32, i32)>,
}
impl NativeRdp {
    pub(super) fn new(parent: Hwnd) -> Result<Self, String> {
        let atl = atl_api()?;
        let class = wide("STATIC");
        let title = wide("");
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                0x40000000 | 0x02000000 | 0x04000000,
                0,
                0,
                1024,
                768,
                parent,
                0,
                0,
                null_mut(),
            )
        };
        if window == 0 {
            return Err(format!(
                "Création de la fenêtre RDP impossible ({}).",
                unsafe { GetLastError() }
            ));
        }
        let control = wide("{8B918B82-7985-4C24-89DF-C33AD2BBFBCD}");
        let mut container = null_mut();
        let mut unknown = null_mut();
        let hr = unsafe {
            (atl.create)(
                control.as_ptr(),
                window,
                null_mut(),
                &mut container,
                &mut unknown,
                &IID_NULL,
                null_mut(),
            )
        };
        let container = Unknown(container);
        let unknown = Unknown(unknown);
        if hr < 0 {
            unsafe {
                DestroyWindow(window);
            }
            return Err(format!(
                "Le contrôle Bureau à distance Windows n’est pas disponible (0x{:08X}).",
                hr as u32
            ));
        }
        let client = match unsafe { Dispatch::from_unknown(unknown.0) } {
            Ok(client) => client,
            Err(error) => {
                unsafe {
                    DestroyWindow(window);
                }
                return Err(error);
            }
        };
        // Prevent native parent painting from erasing the ActiveX child.
        unsafe {
            let style = GetWindowLongPtrW(parent, -16);
            SetWindowLongPtrW(parent, -16, style | 0x02000000);
        }
        Ok(Self {
            window,
            parent,
            client: Some(client),
            container: Some(container),
            visible: Cell::new(false),
            rectangle: Cell::new((0, 0, 1024, 768)),
        })
    }
    pub(super) fn client(&self) -> &Dispatch {
        self.client.as_ref().expect("RDP control is alive")
    }
    pub(super) fn position(&self, x: i32, y: i32, width: i32, height: i32) {
        let rectangle = (x, y, width.max(1), height.max(1));
        if self.rectangle.replace(rectangle) == rectangle {
            return;
        }
        unsafe {
            MoveWindow(self.window, x, y, width.max(1), height.max(1), 1);
        }
    }
    pub(super) fn visible(&self, value: bool) {
        let foreground = if value {
            0
        } else {
            unsafe { GetForegroundWindow() }
        };
        self.set_visible_with_foreground(value, foreground);
    }
    fn set_visible_with_foreground(&self, value: bool, foreground: Hwnd) {
        if !value {
            // The ActiveX client has nested child windows. ShowWindow on their
            // ancestor does not necessarily release their keyboard focus.
            // Restore only our active parent's focus, never another control's
            // or another application's focus. Recheck even when already hidden:
            // the previous hide may have happened while Nexus was inactive.
            restore_focus_before_hide(self.window, self.parent, foreground);
        }
        if self.visible.replace(value) == value {
            return;
        }
        unsafe {
            ShowWindow(self.window, if value { 5 } else { 0 });
        }
    }
}

fn restore_focus_before_hide(window: Hwnd, parent: Hwnd, foreground: Hwnd) -> bool {
    if parent == 0 || foreground != parent {
        return false;
    }
    let focused = unsafe { GetFocus() };
    if focused == 0 || (focused != window && unsafe { IsChild(window, focused) } == 0) {
        return false;
    }
    unsafe {
        SetFocus(parent);
        GetFocus() == parent
    }
}
impl Drop for NativeRdp {
    fn drop(&mut self) {
        self.visible(false);
        if let Some(client) = &self.client {
            if client.int("Connected").unwrap_or(0) != 0 {
                let _ = client.call("Disconnect", &mut []);
            }
            if let Ok(settings) = client.child("AdvancedSettings9") {
                if let Ok(empty) = Variant::text("") {
                    let _ = settings.put("ClearTextPassword", empty);
                }
            }
        }
        self.client.take();
        unsafe {
            DestroyWindow(self.window);
        }
        self.container.take();
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiding_rdp_ancestor_restores_its_nested_focus_without_stealing_other_focus() {
        // Hidden synthetic windows reproduce the Win32 hierarchy used by ATL
        // and the RDP client. No ActiveX connection or visible window is needed.
        struct Fixture {
            parent: Hwnd,
            previous_focus: Hwnd,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                unsafe {
                    SetFocus(self.previous_focus);
                    DestroyWindow(self.parent);
                }
            }
        }
        let create = |class_name: &str, style: u32, parent: Hwnd| {
            let class = wide(class_name);
            let title = wide("Nexus hidden focus test");
            let window = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    style,
                    -30000,
                    -30000,
                    32,
                    32,
                    parent,
                    0,
                    0,
                    null_mut(),
                )
            };
            assert_ne!(window, 0);
            window
        };
        let fixture = Fixture {
            parent: create("STATIC", 0x00CF0000, 0), // No WS_VISIBLE.
            previous_focus: unsafe { GetFocus() },
        };
        let container = create("STATIC", 0x50000000, fixture.parent);
        let descendant = create("EDIT", 0x50010000, container);
        let sibling = create("EDIT", 0x50010000, fixture.parent);

        unsafe { SetFocus(descendant) };
        assert_eq!(unsafe { GetFocus() }, descendant);
        unsafe { ShowWindow(container, 0) };
        assert_eq!(
            unsafe { GetFocus() },
            descendant,
            "Baseline: hiding an ancestor leaves focus in its hidden descendant"
        );

        unsafe { ShowWindow(container, 5) };
        // Supply this captured ambient value rather than activating the hidden
        // fixture. Production supplies GetForegroundWindow() at the same point.
        assert!(restore_focus_before_hide(
            container,
            fixture.parent,
            fixture.parent
        ));
        unsafe { ShowWindow(container, 0) };
        assert_eq!(unsafe { GetFocus() }, fixture.parent);

        unsafe { SetFocus(sibling) };
        assert!(!restore_focus_before_hide(
            container,
            fixture.parent,
            fixture.parent
        ));
        assert_eq!(
            unsafe { GetFocus() },
            sibling,
            "Another control keeps its focus"
        );

        unsafe { SetFocus(descendant) };
        assert!(!restore_focus_before_hide(container, fixture.parent, 0));
        assert_eq!(
            unsafe { GetFocus() },
            descendant,
            "An inactive Nexus parent must not take focus from another application"
        );
        assert!(!restore_focus_before_hide(container, 0, 0));

        // A background hide intentionally keeps the foreground application's
        // focus untouched. Once Nexus is active again, the repeated hide must
        // recover our nested focus even though the visibility cache is false.
        unsafe { ShowWindow(container, 5) };
        unsafe { SetFocus(descendant) };
        let cached = NativeRdp {
            window: container,
            parent: fixture.parent,
            client: None,
            container: None,
            visible: Cell::new(true),
            rectangle: Cell::new((0, 0, 32, 32)),
        };
        cached.set_visible_with_foreground(false, 0);
        assert!(!cached.visible.get());
        assert_eq!(unsafe { GetFocus() }, descendant);
        cached.set_visible_with_foreground(false, fixture.parent);
        assert!(!cached.visible.get());
        assert_eq!(unsafe { GetFocus() }, fixture.parent);
        unsafe { SetFocus(sibling) };
        cached.set_visible_with_foreground(false, fixture.parent);
        assert_eq!(unsafe { GetFocus() }, sibling);
    }

    #[test]
    fn native_rdp_host_nla_without_network() {
        let _ole = OleGuard::new().unwrap();
        let class = wide("STATIC");
        let title = wide("Nexus RDP diagnostic — hidden");
        let parent = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                0,
                0,
                0,
                1024,
                768,
                0,
                0,
                0,
                null_mut(),
            )
        };
        assert_ne!(parent, 0);
        {
            let host = NativeRdp::new(parent).unwrap();
            let client = host.client();
            client
                .put("Server", Variant::text("127.0.0.1").unwrap())
                .unwrap();
            client
                .put("UserName", Variant::text("diagnostic").unwrap())
                .unwrap();
            client.put("Domain", Variant::text("").unwrap()).unwrap();
            client.put("DesktopWidth", Variant::int(1024)).unwrap();
            client.put("DesktopHeight", Variant::int(768)).unwrap();
            client.put("ColorDepth", Variant::int(32)).unwrap();
            client.put("FullScreen", Variant::boolean(false)).unwrap();
            let settings = client.child("AdvancedSettings9").unwrap();
            settings
                .put("EnableCredSspSupport", Variant::boolean(true))
                .unwrap();
            settings
                .put("AuthenticationLevel", Variant::uint(1))
                .unwrap();
            settings
                .put("NegotiateSecurityLayer", Variant::boolean(true))
                .unwrap();
            settings.put("SmartSizing", Variant::boolean(true)).unwrap();
            settings.put("RDPPort", Variant::int(3389)).unwrap();
            settings
                .put("RedirectClipboard", Variant::boolean(false))
                .unwrap();
            for name in [
                "RedirectDrives",
                "RedirectPrinters",
                "RedirectPorts",
                "RedirectSmartCards",
                "RedirectDevices",
                "RedirectPOSDevices",
                "EnableAutoReconnect",
            ] {
                settings.put(name, Variant::boolean(false)).unwrap();
            }
            for name in ["DisplayConnectionBar", "GrabFocusOnConnect"] {
                settings.put(name, Variant::boolean(true)).unwrap();
            }
            settings
                .put("overallConnectionTimeout", Variant::int(30))
                .unwrap();
            settings
                .put("singleConnectionTimeout", Variant::int(15))
                .unwrap();
            settings
                .put("ClearTextPassword", Variant::text("").unwrap())
                .unwrap();
            assert_eq!(settings.int("EnableCredSspSupport").unwrap(), 1);
            assert_eq!(settings.int("AuthenticationLevel").unwrap(), 1);
            assert_eq!(client.int("Connected").unwrap(), 0);
            host.position(0, 0, 800, 600);
        }
        unsafe {
            DestroyWindow(parent);
        }
    }
    #[test]
    fn variant_matches_windows_x64_abi() {
        assert_eq!(std::mem::size_of::<Variant>(), 24);
        assert_eq!(std::mem::align_of::<Variant>(), 8);
        assert_eq!(std::mem::size_of::<ExceptionInfo>(), 64);
    }
}
