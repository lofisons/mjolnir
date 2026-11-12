//! A small host: it loads the plugin the way a DAW does, through the factory
//! and the COM interfaces, and listens to what comes out.

#![allow(non_snake_case, non_upper_case_globals)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use mjolnir_vst3::core::{self, BEAT, SOUND};
use mjolnir_vst3::GetPluginFactory;
use vst3::{Class, ComPtr, ComWrapper, Interface, Steinberg::Vst::*, Steinberg::*};

struct Events(RefCell<Vec<Event>>);
impl Class for Events {
    type Interfaces = (IEventList,);
}
impl IEventListTrait for Events {
    unsafe fn getEventCount(&self) -> int32 {
        self.0.borrow().len() as int32
    }
    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        match self.0.borrow().get(index as usize) {
            Some(ev) => {
                *e = *ev;
                kResultOk
            }
            None => kInvalidArgument,
        }
    }
    unsafe fn addEvent(&self, _e: *mut Event) -> tresult {
        kResultFalse
    }
}

struct Queue(ParamID, f64);
impl Class for Queue {
    type Interfaces = (IParamValueQueue,);
}
impl IParamValueQueueTrait for Queue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.0
    }
    unsafe fn getPointCount(&self) -> int32 {
        1
    }
    unsafe fn getPoint(&self, _i: int32, offset: *mut int32, value: *mut ParamValue) -> tresult {
        *offset = 0;
        *value = self.1;
        kResultOk
    }
    unsafe fn addPoint(&self, _o: int32, _v: ParamValue, _i: *mut int32) -> tresult {
        kResultFalse
    }
}

struct Changes(Vec<ComPtr<IParamValueQueue>>);
impl Class for Changes {
    type Interfaces = (IParameterChanges,);
}
impl IParameterChangesTrait for Changes {
    unsafe fn getParameterCount(&self) -> int32 {
        self.0.len() as int32
    }
    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        self.0
            .get(index as usize)
            .map_or(std::ptr::null_mut(), |q| q.as_ptr())
    }
    unsafe fn addParameterData(&self, _id: *const ParamID, _i: *mut int32) -> *mut IParamValueQueue {
        std::ptr::null_mut()
    }
}

struct Stream {
    bytes: RefCell<Vec<u8>>,
    at: Cell<usize>,
}
impl Class for Stream {
    type Interfaces = (IBStream,);
}
impl IBStreamTrait for Stream {
    unsafe fn read(&self, buffer: *mut c_void, n: int32, read: *mut int32) -> tresult {
        let bytes = self.bytes.borrow();
        let take = (n.max(0) as usize).min(bytes.len() - self.at.get());
        std::ptr::copy_nonoverlapping(bytes.as_ptr().add(self.at.get()), buffer as *mut u8, take);
        self.at.set(self.at.get() + take);
        if !read.is_null() {
            *read = take as int32;
        }
        kResultOk
    }
    unsafe fn write(&self, buffer: *mut c_void, n: int32, written: *mut int32) -> tresult {
        let src = std::slice::from_raw_parts(buffer as *const u8, n.max(0) as usize);
        self.bytes.borrow_mut().extend_from_slice(src);
        if !written.is_null() {
            *written = n;
        }
        kResultOk
    }
    unsafe fn seek(&self, pos: int64, _mode: int32, result: *mut int64) -> tresult {
        self.at.set(pos.max(0) as usize);
        if !result.is_null() {
            *result = pos;
        }
        kResultOk
    }
    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        *pos = self.at.get() as int64;
        kResultOk
    }
}

struct Host {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    controller: ComPtr<IEditController>,
    quarters: f64,
}

const SR: f64 = 48000.0;
const BLOCK: usize = 256;

fn note(on: bool, pitch: i16, at: i32) -> Event {
    let mut e: Event = unsafe { std::mem::zeroed() };
    e.sampleOffset = at;
    if on {
        e.r#type = Event_::EventTypes_::kNoteOnEvent as u16;
        e.__field0.noteOn = NoteOnEvent {
            channel: 0,
            pitch,
            tuning: 0.0,
            velocity: 0.8,
            length: 0,
            noteId: -1,
        };
    } else {
        e.r#type = Event_::EventTypes_::kNoteOffEvent as u16;
        e.__field0.noteOff = NoteOffEvent {
            channel: 0,
            pitch,
            velocity: 0.0,
            noteId: -1,
            tuning: 0.0,
        };
    }
    e
}

impl Host {
    fn open() -> Host {
        unsafe {
            let factory = ComPtr::from_raw(GetPluginFactory()).expect("a factory");
            assert_eq!(factory.countClasses(), 1);
            let factory2 = factory.cast::<IPluginFactory2>().expect("class info 2");
            let mut info: PClassInfo2 = std::mem::zeroed();
            assert_eq!(factory2.getClassInfo2(0, &mut info), kResultOk);
            let sub: Vec<u8> = info.subCategories.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
            assert!(String::from_utf8(sub).unwrap().starts_with("Instrument"));
            assert_eq!(info.classFlags, 0);

            let mut obj: *mut c_void = std::ptr::null_mut();
            let r = factory.createInstance(
                info.cid.as_ptr() as FIDString,
                IComponent::IID.as_ptr() as FIDString,
                &mut obj,
            );
            assert_eq!(r, kResultOk);
            let component = ComPtr::from_raw(obj as *mut IComponent).expect("a component");
            assert_eq!(component.initialize(std::ptr::null_mut()), kResultOk);
            let processor = component.cast::<IAudioProcessor>().expect("a processor");
            let controller = component.cast::<IEditController>().expect("its own controller");

            assert_eq!(component.getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32), 1);
            assert_eq!(component.getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kInput as i32), 0);
            assert_eq!(component.getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kInput as i32), 1);

            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: BLOCK as i32,
                sampleRate: SR,
            };
            assert_eq!(processor.setupProcessing(&mut setup), kResultOk);
            assert_eq!(component.setActive(1), kResultOk);
            assert_eq!(processor.setProcessing(1), kResultOk);
            Host { component, processor, controller, quarters: 0.0 }
        }
    }

    /// Run `blocks` blocks and answer the loudest sample.
    fn run(&mut self, blocks: usize, playing: bool, events: Vec<Event>, changes: Vec<(u32, f64)>) -> f32 {
        let mut peak = 0.0f32;
        let mut events = Some(events);
        let mut changes = Some(changes);
        for _ in 0..blocks {
            let mut left = [0.0f32; BLOCK];
            let mut right = [0.0f32; BLOCK];
            let mut channels = [left.as_mut_ptr(), right.as_mut_ptr()];
            let mut bus: AudioBusBuffers = unsafe { std::mem::zeroed() };
            bus.numChannels = 2;
            bus.__field0.channelBuffers32 = channels.as_mut_ptr();
            let list = ComWrapper::new(Events(RefCell::new(events.take().unwrap_or_default())));
            let list = list.to_com_ptr::<IEventList>().unwrap();
            let queues = changes
                .take()
                .unwrap_or_default()
                .into_iter()
                .map(|(id, v)| ComWrapper::new(Queue(id, v)).to_com_ptr::<IParamValueQueue>().unwrap())
                .collect();
            let params = ComWrapper::new(Changes(queues)).to_com_ptr::<IParameterChanges>().unwrap();
            let mut ctx: ProcessContext = unsafe { std::mem::zeroed() };
            use ProcessContext_::StatesAndFlags_::*;
            ctx.state = (kTempoValid | kProjectTimeMusicValid) as u32 | if playing { kPlaying as u32 } else { 0 };
            ctx.sampleRate = SR;
            ctx.tempo = 90.0;
            ctx.projectTimeMusic = self.quarters;
            let mut data: ProcessData = unsafe { std::mem::zeroed() };
            data.numSamples = BLOCK as i32;
            data.numOutputs = 1;
            data.outputs = &mut bus;
            data.inputEvents = list.as_ptr();
            data.inputParameterChanges = params.as_ptr();
            data.processContext = &mut ctx;
            unsafe {
                assert_eq!(self.processor.process(&mut data), kResultOk);
            }
            if playing {
                self.quarters += BLOCK as f64 / SR * 90.0 / 60.0;
            }
            for s in left.iter().chain(right.iter()) {
                assert!(s.is_finite());
                peak = peak.max(s.abs());
            }
        }
        peak
    }

    /// Give the helper thread time to follow a change.
    fn settle(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(250));
        self.run(8, false, vec![], vec![]);
    }

    fn close(self) {
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            self.component.terminate();
        }
    }
}

fn text(s: &String128) -> String {
    let n = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n].iter().map(|c| *c as u16).collect::<Vec<_>>())
}

#[test]
fn every_parameter_has_a_name_and_a_readable_value() {
    let host = Host::open();
    unsafe {
        let n = host.controller.getParameterCount();
        assert_eq!(n as usize, core::PARAMS.len());
        for i in 0..n {
            let mut info: ParameterInfo = std::mem::zeroed();
            assert_eq!(host.controller.getParameterInfo(i, &mut info), kResultOk);
            assert_eq!(info.id, i as u32);
            assert!(!text(&info.title).is_empty());
            let mut s: String128 = [0; 128];
            assert_eq!(host.controller.getParamStringByValue(info.id, info.defaultNormalizedValue, &mut s), kResultOk);
            assert!(!text(&s).is_empty());
            // what it shows can be typed back
            let mut back = -1.0;
            assert_eq!(host.controller.getParamValueByString(info.id, s.as_mut_ptr(), &mut back), kResultOk);
            assert!((back - info.defaultNormalizedValue).abs() < 0.011, "{} {back}", text(&info.title));
        }
        let mut s: String128 = [0; 128];
        host.controller.getParamStringByValue(SOUND as u32, 0.0, &mut s);
        assert_eq!(text(&s), core::sounds()[0].label);
    }
    host.close();
}

#[test]
fn every_sound_answers_a_note() {
    let mut host = Host::open();
    assert!(host.run(40, false, vec![], vec![]) < 1e-4, "silent until played");
    for (i, sound) in core::sounds().iter().enumerate() {
        let v = core::value_of(SOUND, i) as f64;
        host.run(1, false, vec![], vec![(SOUND as u32, v)]);
        host.settle();
        let pitch = if sound.drums { 36 } else { 57 };
        let peak = host.run(60, false, vec![note(true, pitch, 17)], vec![]);
        host.run(1, false, vec![note(false, pitch, 0)], vec![]);
        assert!(peak > 0.01, "{} is silent ({peak})", sound.label);
        assert!(peak < 1.5, "{} is too loud ({peak})", sound.label);
        // let it ring out before the next one
        host.run(200, false, vec![], vec![]);
    }
    host.close();
}

#[test]
fn a_beat_plays_with_the_hosts_song_and_stops_with_it() {
    let mut host = Host::open();
    let beat = core::value_of(BEAT, 6) as f64; // boom bap
    host.run(1, false, vec![], vec![(BEAT as u32, beat)]);
    host.settle();
    assert!(host.run(100, false, vec![], vec![]) < 1e-4, "nothing while the host is stopped");
    let peak = host.run(400, true, vec![], vec![]);
    assert!(peak > 0.05, "the beat plays ({peak})");
    host.run(400, false, vec![], vec![]);
    assert!(host.run(100, false, vec![], vec![]) < 1e-3, "and stops");
    // a beat on a sound that is not a kit stays silent
    let piano = core::sounds().iter().position(|s| !s.drums).unwrap();
    host.run(1, false, vec![], vec![(SOUND as u32, core::value_of(SOUND, piano) as f64)]);
    host.settle();
    assert!(host.run(300, true, vec![], vec![]) < 1e-3);
    host.close();
}

#[test]
fn a_state_comes_back_in_another_instance() {
    let mut a = Host::open();
    let sound = core::value_of(SOUND, 12) as f64;
    a.run(1, false, vec![], vec![(SOUND as u32, sound), (1, 0.37), (BEAT as u32, 1.0)]);
    let stream = ComWrapper::new(Stream { bytes: RefCell::new(Vec::new()), at: Cell::new(0) });
    let ptr = stream.to_com_ptr::<IBStream>().unwrap();
    unsafe {
        assert_eq!(a.component.getState(ptr.as_ptr()), kResultOk);
    }
    assert!(stream.bytes.borrow().starts_with(b"mjolnir 1\n"));
    let b = Host::open();
    unsafe {
        ptr.seek(0, 0, std::ptr::null_mut());
        assert_eq!(b.component.setState(ptr.as_ptr()), kResultOk);
        ptr.seek(0, 0, std::ptr::null_mut());
        assert_eq!(b.controller.setComponentState(ptr.as_ptr()), kResultOk);
        assert!((b.controller.getParamNormalized(SOUND as u32) - sound).abs() < 1e-6);
        assert!((b.controller.getParamNormalized(1) - 0.37).abs() < 1e-6);
        assert!((b.controller.getParamNormalized(BEAT as u32) - 1.0).abs() < 1e-6);
    }
    a.close();
    b.close();
}
