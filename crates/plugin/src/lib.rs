//! Mjolnir as a VST 3 instrument: the sounds, the knobs and the ready-made
//! drum loops, played by a host's notes and following its tempo.
//!
//! One class is both the processor and the controller. It has no window of
//! its own: the host shows the parameters.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

pub mod core;

use std::cell::UnsafeCell;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicU32, Ordering};
use std::{ptr, slice};

use vst3::{uid, Class, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use crate::core::{Audio, Kind, NoteEvent, Params, Transport, PARAMS};

const NAME: &str = "Mjolnir";
const VENDOR: &str = "lofisons";
const URL: &str = "https://github.com/lofisons/mjolnir";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn copy_cstring(src: &str, dst: &mut [c_char]) {
    let room = dst.len().saturating_sub(1);
    let mut n = 0;
    for (b, d) in src.bytes().take(room).zip(dst.iter_mut()) {
        *d = b as c_char;
        n += 1;
    }
    if let Some(end) = dst.get_mut(n) {
        *end = 0;
    }
}

fn copy_wstring(src: &str, dst: &mut [TChar]) {
    let room = dst.len().saturating_sub(1);
    let mut n = 0;
    for (u, d) in src.encode_utf16().take(room).zip(dst.iter_mut()) {
        *d = u as TChar;
        n += 1;
    }
    if let Some(end) = dst.get_mut(n) {
        *end = 0;
    }
}

unsafe fn read_wstring(string: *const TChar) -> String {
    if string.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *string.add(len) != 0 && len < 512 {
        len += 1;
    }
    String::from_utf16_lossy(slice::from_raw_parts(string as *const u16, len))
}

/// The most notes one block carries; a host that sends more loses the rest
/// of that block.
const MAX_EVENTS: usize = 256;

struct Live {
    audio: Option<Audio>,
    events: Vec<NoteEvent>,
    silence: Vec<f32>,
}

pub struct Plugin {
    params: Params,
    sample_rate: AtomicU32,
    /// The audio side. Touched by `setActive`/`setupProcessing` and by
    /// `process`, which a host never calls at the same time.
    live: UnsafeCell<Live>,
}

// SAFETY: `live` is only reached under the VST 3 call rules above; the rest
// is atomics.
unsafe impl Send for Plugin {}
unsafe impl Sync for Plugin {}

impl Class for Plugin {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl Plugin {
    pub const CID: TUID = uid(0x4D6A6F6C, 0x6E697231, 0x6C6F6669, 0x736F6E73);

    pub fn new() -> Plugin {
        Plugin {
            params: Params::default(),
            sample_rate: AtomicU32::new(48000f32.to_bits()),
            live: UnsafeCell::new(Live {
                audio: None,
                events: Vec::with_capacity(MAX_EVENTS),
                silence: Vec::new(),
            }),
        }
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn live(&self) -> &mut Live {
        &mut *self.live.get()
    }

    unsafe fn write_state(&self, stream: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(stream) else {
            return kInvalidArgument;
        };
        let text = self.params.save();
        let mut written = 0;
        stream.write(
            text.as_ptr() as *mut c_void,
            text.len() as int32,
            &mut written,
        )
    }

    unsafe fn read_state(&self, stream: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(stream) else {
            return kInvalidArgument;
        };
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let mut read = 0;
            let r = stream.read(
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as int32,
                &mut read,
            );
            if r != kResultOk || read <= 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read as usize]);
            if bytes.len() > 1 << 16 {
                break;
            }
        }
        match std::str::from_utf8(&bytes) {
            Ok(text) if self.params.open(text) => kResultOk,
            _ => kResultFalse,
        }
    }
}

impl Default for Plugin {
    fn default() -> Self {
        Plugin::new()
    }
}

impl IPluginBaseTrait for Plugin {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        self.live().audio = None;
        kResultOk
    }
}

impl IComponentTrait for Plugin {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        // this class is its own controller
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media: MediaType, dir: BusDirection) -> i32 {
        let input = dir as BusDirections == BusDirections_::kInput;
        match media as MediaTypes {
            MediaTypes_::kAudio => i32::from(!input),
            MediaTypes_::kEvent => i32::from(input),
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        if index != 0 || bus.is_null() {
            return kInvalidArgument;
        }
        let input = dir as BusDirections == BusDirections_::kInput;
        let bus = &mut *bus;
        match media as MediaTypes {
            MediaTypes_::kAudio if !input => {
                bus.mediaType = MediaTypes_::kAudio as MediaType;
                bus.direction = BusDirections_::kOutput as BusDirection;
                bus.channelCount = 2;
                copy_wstring("Output", &mut bus.name);
            }
            MediaTypes_::kEvent if input => {
                bus.mediaType = MediaTypes_::kEvent as MediaType;
                bus.direction = BusDirections_::kInput as BusDirection;
                bus.channelCount = 16;
                copy_wstring("Notes", &mut bus.name);
            }
            _ => return kInvalidArgument,
        }
        bus.busType = BusTypes_::kMain as BusType;
        bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;
        kResultOk
    }

    unsafe fn getRoutingInfo(&self, _in: *mut RoutingInfo, _out: *mut RoutingInfo) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        _media: MediaType,
        _dir: BusDirection,
        _index: i32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, state: TBool) -> tresult {
        let live = self.live();
        if state != 0 {
            let sr = f32::from_bits(self.sample_rate.load(Ordering::Relaxed));
            live.audio = Some(Audio::new(&self.params, sr));
        } else {
            live.audio = None;
        }
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        self.read_state(state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        self.write_state(state)
    }
}

impl IAudioProcessorTrait for Plugin {
    unsafe fn setBusArrangements(
        &self,
        _inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins == 0 && num_outs == 1 && !outputs.is_null() && *outputs == SpeakerArr::kStereo {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if dir as BusDirections == BusDirections_::kOutput && index == 0 && !arr.is_null() {
            *arr = SpeakerArr::kStereo;
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
        if size as SymbolicSampleSizes == SymbolicSampleSizes_::kSample32 {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        if setup.is_null() {
            return kInvalidArgument;
        }
        let setup = &*setup;
        self.sample_rate
            .store((setup.sampleRate as f32).to_bits(), Ordering::Relaxed);
        let live = self.live();
        let frames = (setup.maxSamplesPerBlock.max(0) as usize).max(1024);
        if live.silence.len() < frames {
            live.silence = vec![0.0; frames];
        }
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        let data = &mut *data;

        // the knobs: the last point of each queue is where the block ends
        if let Some(changes) = ComRef::from_raw(data.inputParameterChanges) {
            for i in 0..changes.getParameterCount() {
                let Some(queue) = ComRef::from_raw(changes.getParameterData(i)) else {
                    continue;
                };
                let id = queue.getParameterId() as usize;
                let points = queue.getPointCount();
                if id >= PARAMS.len() || points <= 0 {
                    continue;
                }
                let mut offset = 0;
                let mut value = 0.0;
                if queue.getPoint(points - 1, &mut offset, &mut value) == kResultOk {
                    self.params.set(id, value as f32);
                }
            }
        }

        let live = self.live();
        let frames = data.numSamples.max(0) as usize;

        // the notes
        live.events.clear();
        if let Some(list) = ComRef::from_raw(data.inputEvents) {
            for i in 0..list.getEventCount() {
                if live.events.len() == MAX_EVENTS {
                    break;
                }
                let mut e: Event = std::mem::zeroed();
                if list.getEvent(i, &mut e) != kResultOk {
                    continue;
                }
                let at = (e.sampleOffset.max(0) as usize).min(frames);
                match e.r#type as Event_::EventTypes {
                    Event_::EventTypes_::kNoteOnEvent => {
                        let n = e.__field0.noteOn;
                        live.events.push(NoteEvent {
                            at,
                            pitch: n.pitch.clamp(0, 127) as u8,
                            // a note on at zero is a note off, as in MIDI
                            velocity: n.velocity.clamp(0.0, 1.0),
                        });
                    }
                    Event_::EventTypes_::kNoteOffEvent => {
                        let n = e.__field0.noteOff;
                        live.events.push(NoteEvent {
                            at,
                            pitch: n.pitch.clamp(0, 127) as u8,
                            velocity: 0.0,
                        });
                    }
                    _ => {}
                }
            }
            live.events.sort_by_key(|e| e.at);
        }

        // the host's song
        let mut transport = Transport::default();
        if !data.processContext.is_null() {
            let c = &*data.processContext;
            use ProcessContext_::StatesAndFlags_::*;
            transport.playing = c.state & kPlaying as u32 != 0;
            if c.state & kTempoValid as u32 != 0 {
                transport.bpm = Some(c.tempo);
            }
            if c.state & kProjectTimeMusicValid as u32 != 0 {
                transport.quarters = Some(c.projectTimeMusic);
            }
        }

        if frames == 0 || data.numOutputs < 1 || data.outputs.is_null() {
            // a block with nothing to fill still carries notes and knobs
            if let Some(audio) = live.audio.as_mut() {
                audio.process(transport, &live.events, &mut [], &mut []);
            }
            return kResultOk;
        }
        let bus = &mut *data.outputs;
        if bus.numChannels < 2 || bus.__field0.channelBuffers32.is_null() {
            return kResultOk;
        }
        let channels = slice::from_raw_parts(bus.__field0.channelBuffers32, 2);
        if channels[0].is_null() || channels[1].is_null() {
            return kResultOk;
        }
        let left = slice::from_raw_parts_mut(channels[0], frames);
        let right = slice::from_raw_parts_mut(channels[1], frames);
        match live.audio.as_mut() {
            Some(audio) => audio.process(transport, &live.events, left, right),
            None => {
                left.fill(0.0);
                right.fill(0.0);
            }
        }
        bus.silenceFlags = 0;
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        kInfiniteTail
    }
}

impl IProcessContextRequirementsTrait for Plugin {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        use IProcessContextRequirements_::Flags_::*;
        (kNeedTempo | kNeedProjectTimeMusic | kNeedTransportState) as u32
    }
}

impl IEditControllerTrait for Plugin {
    unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
        // the component is this same object: it has already read its state
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        PARAMS.len() as i32
    }

    unsafe fn getParameterInfo(&self, index: i32, info: *mut ParameterInfo) -> tresult {
        let Some(p) = usize::try_from(index).ok().and_then(|i| PARAMS.get(i)) else {
            return kInvalidArgument;
        };
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        use ParameterInfo_::ParameterFlags_::*;
        info.id = index as u32;
        copy_wstring(p.title, &mut info.title);
        copy_wstring(p.title, &mut info.shortTitle);
        copy_wstring(if p.kind == Kind::Amount { "%" } else { "" }, &mut info.units);
        let n = core::choices(index as usize);
        info.stepCount = if n > 1 { n as i32 - 1 } else { 0 };
        info.defaultNormalizedValue = p.default as f64;
        info.unitId = 0;
        info.flags = if p.kind == Kind::Amount {
            kCanAutomate as i32
        } else {
            (kCanAutomate | kIsList) as i32
        };
        kResultOk
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value: f64,
        string: *mut String128,
    ) -> tresult {
        if id as usize >= PARAMS.len() || string.is_null() {
            return kInvalidArgument;
        }
        copy_wstring(&core::display(id as usize, value as f32), &mut *string);
        kResultOk
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value: *mut f64,
    ) -> tresult {
        let id = id as usize;
        if id >= PARAMS.len() || value.is_null() {
            return kInvalidArgument;
        }
        let text = read_wstring(string);
        let text = text.trim();
        let n = core::choices(id);
        if n > 1 {
            for c in 0..n {
                let v = core::value_of(id, c);
                if core::display(id, v).eq_ignore_ascii_case(text) {
                    *value = v as f64;
                    return kResultOk;
                }
            }
            return kResultFalse;
        }
        match text.trim_end_matches('%').trim().parse::<f64>() {
            Ok(v) => {
                *value = (v / 100.0).clamp(0.0, 1.0);
                kResultOk
            }
            Err(_) => kResultFalse,
        }
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value: f64) -> f64 {
        let id = id as usize;
        if id >= PARAMS.len() {
            return 0.0;
        }
        match core::choices(id) {
            0 | 1 => value * 100.0,
            _ => core::choice(id, value as f32) as f64,
        }
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain: f64) -> f64 {
        let id = id as usize;
        if id >= PARAMS.len() {
            return 0.0;
        }
        match core::choices(id) {
            0 | 1 => (plain / 100.0).clamp(0.0, 1.0),
            _ => core::value_of(id, plain.max(0.0).round() as usize) as f64,
        }
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        if (id as usize) < PARAMS.len() {
            self.params.get(id as usize) as f64
        } else {
            0.0
        }
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        if (id as usize) < PARAMS.len() {
            self.params.set(id as usize, value as f32);
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        // no window of its own: the host shows the parameters
        ptr::null_mut()
    }
}

struct Factory;

impl Class for Factory {
    type Interfaces = (IPluginFactory, IPluginFactory2);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        copy_cstring(VENDOR, &mut info.vendor);
        copy_cstring(URL, &mut info.url);
        copy_cstring("", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as int32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        1
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if index != 0 || info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        info.cid = Plugin::CID;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_cstring("Audio Module Class", &mut info.category);
        copy_cstring(NAME, &mut info.name);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        if *(cid as *const TUID) != Plugin::CID {
            *obj = ptr::null_mut();
            return kInvalidArgument;
        }
        let Some(instance) = ComWrapper::new(Plugin::new()).to_com_ptr::<FUnknown>() else {
            return kResultFalse;
        };
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
    }
}

impl IPluginFactory2Trait for Factory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        if index != 0 || info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        info.cid = Plugin::CID;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_cstring("Audio Module Class", &mut info.category);
        copy_cstring(NAME, &mut info.name);
        // not distributable: processor and controller are one object
        info.classFlags = 0;
        copy_cstring("Instrument|Synth|Sampler|Drum", &mut info.subCategories);
        copy_cstring(VENDOR, &mut info.vendor);
        copy_cstring(VERSION, &mut info.version);
        copy_cstring("VST 3.7", &mut info.sdkVersion);
        kResultOk
    }
}

#[cfg(target_os = "windows")]
#[no_mangle]
extern "system" fn InitDll() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[no_mangle]
extern "system" fn ExitDll() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[no_mangle]
extern "system" fn bundleEntry(_bundle: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[no_mangle]
extern "system" fn bundleExit() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[no_mangle]
extern "system" fn ModuleEntry(_library: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "linux")]
#[no_mangle]
extern "system" fn ModuleExit() -> bool {
    true
}

#[no_mangle]
pub extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory)
        .to_com_ptr::<IPluginFactory>()
        .map_or(ptr::null_mut(), |p| p.into_raw())
}
