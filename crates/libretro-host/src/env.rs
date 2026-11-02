//! `retro_environment`: what the host answers to the core. Anything not listed returns
//! `false` ("not supported"), which libretro cores must tolerate.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::panic::catch_unwind;

use emu_api::CoreOption;

use crate::convert::PixelFormat;
use crate::{ctx, Ctx, RetroGameGeometry, RetroSystemAvInfo};

const SET_MESSAGE: c_uint = 6;
const SET_PERFORMANCE_LEVEL: c_uint = 8;
const GET_SYSTEM_DIRECTORY: c_uint = 9;
const SET_PIXEL_FORMAT: c_uint = 10;
const SET_INPUT_DESCRIPTORS: c_uint = 11;
const GET_VARIABLE: c_uint = 15;
const SET_VARIABLES: c_uint = 16;
const GET_VARIABLE_UPDATE: c_uint = 17;
const SET_SUPPORT_NO_GAME: c_uint = 18;
const GET_CAN_DUPE: c_uint = 3;
const GET_INPUT_DEVICE_CAPABILITIES: c_uint = 24;
const GET_CORE_ASSETS_DIRECTORY: c_uint = 30;
const GET_SAVE_DIRECTORY: c_uint = 31;
const SET_SYSTEM_AV_INFO: c_uint = 32;
const SET_CONTROLLER_INFO: c_uint = 35;
const SET_GEOMETRY: c_uint = 37;
const GET_LANGUAGE: c_uint = 39;
const GET_CORE_OPTIONS_VERSION: c_uint = 52;
const GET_MESSAGE_INTERFACE_VERSION: c_uint = 59;

#[repr(C)]
struct RetroVariable {
    key: *const c_char,
    value: *const c_char,
}

pub(crate) unsafe extern "C" fn environment(cmd: c_uint, data: *mut c_void) -> bool {
    catch_unwind(|| unsafe { handle(cmd, data) }).unwrap_or(false)
}

unsafe fn handle(cmd: c_uint, data: *mut c_void) -> bool {
    match cmd {
        GET_CAN_DUPE => unsafe { *data.cast::<bool>() = true },
        SET_MESSAGE | SET_PERFORMANCE_LEVEL | SET_SUPPORT_NO_GAME | SET_CONTROLLER_INFO | SET_INPUT_DESCRIPTORS => {}
        GET_SYSTEM_DIRECTORY | GET_CORE_ASSETS_DIRECTORY => unsafe { *data.cast::<*const c_char>() = ctx().system_dir.as_ptr() },
        GET_SAVE_DIRECTORY => unsafe { *data.cast::<*const c_char>() = ctx().save_dir.as_ptr() },
        SET_PIXEL_FORMAT => match PixelFormat::from_libretro(unsafe { *data.cast::<c_int>() }) {
            Some(f) => ctx().format = f,
            None => return false,
        },
        GET_VARIABLE => return unsafe { get_variable(data.cast()) },
        SET_VARIABLES => unsafe { set_variables(data.cast()) },
        GET_VARIABLE_UPDATE => {
            let mut c = ctx();
            unsafe { *data.cast::<bool>() = c.options_updated };
            c.options_updated = false;
        }
        GET_INPUT_DEVICE_CAPABILITIES => unsafe { *data.cast::<u64>() = 1 << crate::DEVICE_JOYPAD },
        SET_SYSTEM_AV_INFO => apply_av_info(&mut ctx(), unsafe { &*data.cast::<RetroSystemAvInfo>() }),
        SET_GEOMETRY => apply_geometry(&mut ctx(), unsafe { &*data.cast::<RetroGameGeometry>() }),
        // options v0 (SET_VARIABLES) only; messages in the plain format; English
        GET_CORE_OPTIONS_VERSION | GET_MESSAGE_INTERFACE_VERSION | GET_LANGUAGE => unsafe { *data.cast::<c_uint>() = 0 },
        _ => return false,
    }
    true
}

pub(crate) fn apply_geometry(c: &mut Ctx, g: &RetroGameGeometry) {
    c.aspect = if g.aspect_ratio > 0.0 { g.aspect_ratio } else { g.base_width as f32 / g.base_height.max(1) as f32 };
    if c.width == 0 {
        c.width = g.base_width as usize;
        c.height = g.base_height as usize;
    }
}

pub(crate) fn apply_av_info(c: &mut Ctx, av: &RetroSystemAvInfo) {
    apply_geometry(c, &av.geometry);
    if av.timing.fps > 0.0 {
        c.fps = av.timing.fps;
    }
    if av.timing.sample_rate > 0.0 {
        c.core_rate = av.timing.sample_rate;
    }
}

/// The value of a core option: the user's choice, or the first value listed.
unsafe fn get_variable(var: *mut RetroVariable) -> bool {
    let key = unsafe { CStr::from_ptr((*var).key) }.to_string_lossy().into_owned();
    let mut c = ctx();
    if !c.values.contains_key(&key) {
        let Some(default) = c.options.iter().find(|o| o.key == key).and_then(|o| o.values.first()).map(|v| v.0) else {
            unsafe { (*var).value = std::ptr::null() };
            return false;
        };
        c.values.insert(key.clone(), CString::new(default).unwrap_or_default());
    }
    unsafe { (*var).value = c.values[&key].as_ptr() };
    true
}

/// `SET_VARIABLES`: an array ending in a null key; each value is "Description; a|b|c".
unsafe fn set_variables(mut v: *const RetroVariable) {
    let mut options = Vec::new();
    while !unsafe { (*v).key }.is_null() {
        let key = unsafe { CStr::from_ptr((*v).key) }.to_string_lossy().into_owned();
        let desc = unsafe { CStr::from_ptr((*v).value) }.to_string_lossy().into_owned();
        let (label, list) = desc.split_once("; ").unwrap_or((&key, &desc));
        let values: Vec<(&'static str, &'static str)> = list.split('|').map(|x| (crate::leak(x.to_string()), crate::leak(x.to_string()))).collect();
        let label = crate::leak(label.to_string());
        options.push(CoreOption { key: crate::leak(key.clone()), label, values: Box::leak(values.into_boxed_slice()) });
        v = unsafe { v.add(1) };
    }
    let mut c = ctx();
    c.options = Box::leak(options.into_boxed_slice());
    c.values.clear();
}
