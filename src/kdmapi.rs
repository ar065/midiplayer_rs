// Modified KDMAPI Binding for rust that handles missing functions.
// Not tested extensively!

#![allow(dead_code)]
use lazy_static::lazy_static;
use libloading::{Error, Library, Symbol};
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
use std::{
    ffi::{c_char, c_void},
    sync::atomic::{AtomicBool, Ordering},
};

// Macro to create a stub symbol that returns a default value for its type.
macro_rules! create_stub_symbol {
    ($name:ident, (), $ret:expr, $ret_ty:ty) => {
        unsafe extern "C" fn $name() -> $ret_ty {
            $ret
        }
    };
    ($name:ident, ($($arg:ident: $arg_ty:ty),*), $ret:expr, $ret_ty:ty) => {
        unsafe extern "C" fn $name($($arg: $arg_ty),*) -> $ret_ty {
            let _ = ($($arg),*);
            $ret
        }
    };
}

create_stub_symbol!(stub_bool, (), false, bool);
create_stub_symbol!(stub_i32, (), 0, i32);
create_stub_symbol!(stub_u32, (data: u32), 0, u32);
create_stub_symbol!(stub_u32_with_ptr, (data: *mut c_char, len: u32), 0, u32);
create_stub_symbol!(stub_i32_multi_param, (evt: u32, chan: u32, param: u32), 0, i32);
// For void functions, the return type is `()`, which is the unit type.
create_stub_symbol!(stub_void, (), (), ());
create_stub_symbol!(stub_void_multi_param, (setting: u32, mode: u32, value: *mut c_void, cb_value: u32), (), ());
create_stub_symbol!(stub_bool_with_u16_ptr, (path: *const u16), false, bool);
create_stub_symbol!(stub_bool_with_u8_ptr, (path: *const u8), false, bool);
create_stub_symbol!(stub_f32, (), 0.0, f32);
create_stub_symbol!(stub_u64, (), 0, u64);

/// The dynamic bindings for KDMAPI
pub struct KDMAPIBinds {
    is_kdmapi_available: Option<Symbol<'static, unsafe extern "C" fn() -> bool>>,
    initialize_kdmapi_stream: Option<Symbol<'static, unsafe extern "C" fn() -> i32>>,
    terminate_kdmapi_stream: Option<Symbol<'static, unsafe extern "C" fn() -> i32>>,
    reset_kdmapi_stream: Option<Symbol<'static, unsafe extern "C" fn()>>,
    send_direct_data: Option<Symbol<'static, unsafe extern "C" fn(u32) -> u32>>,
    send_direct_data_no_buf: Option<Symbol<'static, unsafe extern "C" fn(u32) -> u32>>,
    send_direct_long_data: Option<Symbol<'static, unsafe extern "C" fn(*mut c_char, u32) -> u32>>,
    send_direct_long_data_no_buf:
        Option<Symbol<'static, unsafe extern "C" fn(*mut c_char, u32) -> u32>>,
    send_custom_event: Option<Symbol<'static, unsafe extern "C" fn(u32, u32, u32) -> i32>>,
    driver_settings: Option<Symbol<'static, unsafe extern "C" fn(u32, u32, *mut c_void, u32)>>,
    #[cfg(target_os = "windows")]
    load_custom_soundfonts_list: Option<Symbol<'static, unsafe extern "C" fn(*const u16) -> bool>>,
    #[cfg(not(target_os = "windows"))]
    load_custom_soundfonts_list: Option<Symbol<'static, unsafe extern "C" fn(*const u8) -> bool>>,
    get_rendering_time: Option<Symbol<'static, unsafe extern "C" fn() -> f32>>,
    get_voice_count: Option<Symbol<'static, unsafe extern "C" fn() -> u64>>,

    is_stream_open: AtomicBool,
}

impl KDMAPIBinds {
    /// Calls `IsKDMAPIAvailable`
    pub fn is_kdmapi_available(&self) -> bool {
        unsafe { self.is_kdmapi_available.as_ref().map_or(false, |f| f()) }
    }

    /// Calls `InitializeKDMAPIStream` and returns a stream struct with access
    /// to the stream functions.
    ///
    /// Automatically calls `TerminateKDMAPIStream` when dropped.
    ///
    /// Errors if multiple streams are opened in parallel.
    pub fn open_stream(&'static self) -> Result<KDMAPIStream, String> {
        if self.is_stream_open.load(Ordering::Relaxed) {
            return Err("KDMAPI stream is already open".into());
        }
        unsafe {
            let result = self.initialize_kdmapi_stream.as_ref().map_or(0, |f| f());
            if result == 0 {
                Err("Failed to initialize KDMAPI stream or function not found".into())
            } else {
                Ok(KDMAPIStream { binds: self })
            }
        }
    }
}

fn load_kdmapi_lib() -> Result<Library, Error> {
    unsafe {
        #[cfg(target_os = "windows")]
        {
            // Try "OmniMIDI\\OmniMIDI"
            let lib = Library::new("OmniMIDI\\OmniMIDI");
            if lib.is_ok() {
                return lib;
            }
            // Try "OmniMIDI"
            return Library::new("OmniMIDI");
        }
        #[cfg(target_os = "linux")]
        return Library::new("libOmniMIDI.so");
        #[cfg(target_os = "macos")]
        return Library::new("libOmniMIDI.dylib");
    }
}

#[allow(mismatched_lifetime_syntaxes)]
fn load_kdmapi_binds(lib: &'static Result<Library, Error>) -> Result<KDMAPIBinds, &Error> {
    unsafe {
        match lib {
            Ok(lib) => Ok(KDMAPIBinds {
                is_kdmapi_available: lib.get(b"IsKDMAPIAvailable").ok(),
                initialize_kdmapi_stream: lib.get(b"InitializeKDMAPIStream").ok(),
                terminate_kdmapi_stream: lib.get(b"TerminateKDMAPIStream").ok(),
                reset_kdmapi_stream: lib.get(b"ResetKDMAPIStream").ok(),
                send_direct_data: lib.get(b"SendDirectData").ok(),
                send_direct_data_no_buf: lib.get(b"SendDirectDataNoBuf").ok(),
                send_direct_long_data: lib.get(b"SendDirectLongData").ok(),
                send_direct_long_data_no_buf: lib.get(b"SendDirectLongDataNoBuf").ok(),
                send_custom_event: lib.get(b"SendCustomEvent").ok(),
                driver_settings: lib.get(b"DriverSettings").ok(),
                load_custom_soundfonts_list: lib.get(b"LoadCustomSoundFontsList").ok(),
                get_rendering_time: lib.get(b"GetRenderingTime").ok(),
                get_voice_count: lib.get(b"GetVoiceCount").ok(),
                is_stream_open: AtomicBool::new(false),
            }),
            Err(err) => Err(err),
        }
    }
}

/// Struct that provides access to KDMAPI's stream functions
///
/// Automatically calls `TerminateKDMAPIStream` when dropped.
pub struct KDMAPIStream {
    binds: &'static KDMAPIBinds,
}

impl KDMAPIStream {
    /// Calls `ResetKDMAPIStream`
    pub fn reset(&self) {
        unsafe {
            let _ = self.binds.reset_kdmapi_stream.as_ref().map_or((), |f| f());
        }
    }

    /// Calls `SendDirectData`
    pub fn send_direct_data(&self, data: u32) -> u32 {
        unsafe { self.binds.send_direct_data.as_ref().map_or(0, |f| f(data)) }
    }

    /// Calls `SendDirectDataNoBuf`
    pub fn send_direct_data_no_buf(&self, data: u32) -> u32 {
        unsafe {
            self.binds
                .send_direct_data_no_buf
                .as_ref()
                .map_or(0, |f| f(data))
        }
    }

    /// Calls `SendDirectLongData`
    pub fn send_direct_long_data(&self, data: &[u8]) -> u32 {
        unsafe {
            self.binds
                .send_direct_long_data
                .as_ref()
                .map_or(0, |f| f(data.as_ptr() as *mut c_char, data.len() as u32))
        }
    }

    /// Calls `SendDirectLongDataNoBuf`
    pub fn send_direct_long_data_no_buf(&self, data: &[u8]) -> u32 {
        unsafe {
            self.binds
                .send_direct_long_data_no_buf
                .as_ref()
                .map_or(0, |f| f(data.as_ptr() as *mut c_char, data.len() as u32))
        }
    }

    /// Calls `SendCustomEvent`
    pub fn send_custom_event(&self, evt: u32, chan: u32, param: u32) -> i32 {
        unsafe {
            self.binds
                .send_custom_event
                .as_ref()
                .map_or(0, |f| f(evt, chan, param))
        }
    }

    /// Calls `DriverSettings`
    pub fn driver_settings(&self, setting: u32, mode: u32, value: *mut c_void, cb_value: u32) {
        unsafe {
            let _ = self
                .binds
                .driver_settings
                .as_ref()
                .map_or((), |f| f(setting, mode, value, cb_value));
        }
    }

    /// Calls `LoadCustomSoundFontsList`
    pub fn load_custom_soundfonts_list(&self, path: &str) -> bool {
        #[cfg(target_os = "windows")]
        let path: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(Some(0).into_iter())
            .collect();
        #[cfg(not(target_os = "windows"))]
        let path: Vec<u8> = path.as_bytes().iter().copied().chain(Some(0)).collect();

        unsafe {
            self.binds
                .load_custom_soundfonts_list
                .as_ref()
                .map_or(false, |f| f(path.as_ptr()))
        }
    }

    /// Calls `GetRenderingTime`
    pub fn get_rendering_time(&self) -> f32 {
        unsafe { self.binds.get_rendering_time.as_ref().map_or(0.0, |f| f()) }
    }

    /// Calls `GetVoiceCount`
    pub fn get_voice_count(&self) -> u64 {
        unsafe { self.binds.get_voice_count.as_ref().map_or(0, |f| f()) }
    }
}

impl Drop for KDMAPIStream {
    fn drop(&mut self) {
        unsafe {
            let _ = self.binds.terminate_kdmapi_stream.as_ref().map_or((), |f| {
                f();
            });
        }
        self.binds.is_stream_open.store(false, Ordering::Relaxed);
    }
}

lazy_static! {
    static ref KDMAPI_LIB: Result<Library, Error> = load_kdmapi_lib();

    /// The dynamic library for KDMAPI. Is loaded when this field is accessed.
    pub static ref KDMAPI: Result<KDMAPIBinds, &'static Error> = load_kdmapi_binds(&KDMAPI_LIB);
}
